// `.sum()`, `.mean()`, `.max()` and `.min()` as `linalg`, for GPU bodies.
//
// A reduction is a `linalg.generic` whose index space is the result's axes followed by the
// reduced ones: the result axes are `parallel` and become GPU threads, the reduced ones are
// `reduction` and become a sequential loop inside each thread. So every result element is
// folded by one thread in the source's own order, which is the order the LLVM backend folds
// in, and the two give the same bits. A whole-tensor reduction writes a one-element tensor
// (its one parallel axis has extent 1), since a GPU body returns buffers only.
//
// A run longer than `REDUCE_LANES` folds in the language's lane order first: one more
// all-parallel `linalg.generic` over the result's axes and a lane axis, one GPU thread per
// lane, each running an `scf.for` over its run positions `lane, lane + REDUCE_LANES, ...`
// into a partials tensor. Adjacent lanes read adjacent elements, so a warp's loads coalesce.
// The fold above then reduces the lane axis, exactly as the LLVM backend's lane order does,
// so even a whole-tensor sum keeps the host's bits while running on thousands of threads.
// The lane generic reads the source with `tensor.extract` at a position it computes, so it
// has no input for the elementwise fusion to fold into the next generic.
//
// The host takes the same lane order nested the other way (`build_host_lanes`): an `scf.for`
// over the run's rows of lanes outside one all-parallel generic per row over every lane, the
// partials in the source's own layout, so its inner loop folds adjacent elements into
// independent accumulators and vectorizes.
//
// The destination is seeded before the fold, because a reduction reads it at every point:
// with `-0.0` for a sum (the one float that adds as nothing, signed zeros included), and
// with the run's first element for `.max()` / `.min()`, which the fold then meets again
// harmlessly. That is the LLVM backend's own start, so a NaN or an all-equal run picks the
// same element on both.
//
// An integer sum starts at 0 and folds left to right with the host's overflow check, never in
// lanes: the host folds an integer run in order, and where an overflow is caught depends on
// the order of the additions.
//
// A dynamic run may be longer than `REDUCE_LANES`, and the shape it folds in is decided at run
// time, so a float run of dynamic length always takes the lanes (the device's shape, on the host
// too), with `min(length, REDUCE_LANES)` of them. Every lane starts inside the run, so the lanes
// of a short run are its elements in order, which is the sequential fold a short static run
// takes, and the bits match. A mean divides by the run-time count; an integer mean divides as
// integer `/` does.

use crate::{
    errors::MlirError,
    guards::{At, Element, Lowering, Side},
    lower::map_type,
    tensor_arithmetic::{
        Extent, Generic, OperandAxes, build_expression, cast_elements, dynamic_sizes, empty_tensor,
        fill_block, generic_op, half_precision, indexing_maps, iterator_types, iterators, multiply,
        parse_attribute, result_extents, shape_type, tensor_parts, with_element,
    },
    tensor_layout::linalg_index,
    tensor_sort::precedes,
};

use ast_types::BinaryOp;
use melior::{
    Context,
    dialect::arith::{self, CmpiPredicate},
    ir::{
        Block, BlockLike, Identifier, Location, Region, RegionLike, Type, Value, ValueLike,
        attribute::{FloatAttribute, IntegerAttribute},
        operation::OperationBuilder,
        r#type::{IntegerType, RankedTensorType},
    },
};
use neuro_hir::{HirExpr, HirExprKind, HirReduceOp, HirType, REDUCE_LANES};

/// How many region arguments a fold body takes: the source element, then the accumulator.
const FOLD_BODY_ARGUMENTS: usize = 2;

/// The seed of a sum: adding it to any float gives that float back, `-0.0` included.
const SUM_SEED: f64 = -0.0;

/// The most rows of lanes a host last-axis run may span and still fold lane by lane
/// ([`build_lanes`]) rather than row by row ([`build_host_lanes`]). Measured: up to 16
/// rows the lane loop keeps each partial in a register and runs 10 to 40% faster; at 64
/// it is four times slower.
const HOST_LANE_ROW_LOOP_MAX_ROWS: usize = 16;

/// Lower `reduce`, a `TensorReduce`, into a tensor of type `result`: the reduction's own
/// type for an axis, or a one-element tensor for a whole-tensor reduction.
pub(crate) fn build_reduce<'c, 'a>(
    context: &'c Context,
    location: Location<'c>,
    block: &'a Block<'c>,
    reduce: &HirExpr,
    result: &HirType,
    scope: &[(String, Value<'c, 'a>)],
    lowering: &Lowering<'c, 'a>,
) -> Result<Option<Value<'c, 'a>>, MlirError> {
    let HirExprKind::TensorReduce { receiver, op, axis } = &reduce.kind else {
        return Ok(None);
    };
    let (Some((element, source_shape)), Some((_, extents))) =
        (tensor_parts(&receiver.ty), tensor_parts(result))
    else {
        return Ok(None);
    };
    // A half-precision run folds in `f32` and is rounded once at the end, so a long sum
    // does not stall where one more element no longer changes a 16-bit total.
    let half = half_precision(element);
    let wide = HirType::F32;
    let narrow = element;
    let (element, folded_type) = match half {
        true => (&wide, with_element(result, &wide)),
        false => (element, result.clone()),
    };
    let Some(layout) = ReduceLayout::new(source_shape, extents, *axis) else {
        return Ok(None);
    };
    let Some(kind) = Element::of(element) else {
        return Ok(None);
    };
    let Some(source) = build_expression(context, location, block, receiver, scope, lowering)?
    else {
        return Ok(None);
    };
    let source = match half {
        true => cast_elements(
            context,
            location,
            block,
            source,
            (narrow, &with_element(&receiver.ty, &wide)),
        )?,
        false => source,
    };
    // The run-time extent of every source axis, and of the run each result element folds.
    let dims = source_extents(context, location, block, source, source_shape)?;
    let length = run_length(context, location, block, &dims, *axis)?;

    let tensor_type = map_type(context, &folded_type)?;
    let element_type = map_type(context, element)?;
    let fold = Fold {
        lowering,
        op: *op,
        kind,
        offset: reduce.span.start,
    };
    // A float run past `REDUCE_LANES` folds in lanes. A dynamic run may be that long, so it
    // always does, with as many lanes as its length needs: a short run's lanes are its elements,
    // in the order the sequential fold takes them, so the bits are the same.
    let laned = kind == Element::Float
        && match length {
            Extent::Static(length) => length > REDUCE_LANES,
            Extent::Dynamic(_) => true,
        };
    let (source, layout) = match layout.lanes {
        Some(ref run) if laned => {
            let static_source = source_shape.iter().all(Option::is_some);
            let lanes = Lanes {
                context,
                location,
                source,
                run,
                dims: &dims,
                length,
                fold: &fold,
                element_type,
            };
            // A last-axis run of a few rows folds lane by lane on the host too: the same
            // order, so the same bits, with each partial kept in a register.
            let few_rows = match length {
                Extent::Static(length) => {
                    length.div_ceil(REDUCE_LANES) <= HOST_LANE_ROW_LOOP_MAX_ROWS
                        && run.axis.is_some_and(|axis| axis + 1 == run.extents.len())
                }
                Extent::Dynamic(_) => false,
            };
            let host_length = match (lowering.side, length) {
                (Side::Host, Extent::Static(length)) if static_source && !few_rows => Some(length),
                _ => None,
            };
            let (partials, (shape, axis)) = match host_length {
                Some(length) => (
                    build_host_lanes(block, lanes, length)?,
                    (run.host_partials(), run.axis),
                ),
                None => {
                    let count = match length {
                        Extent::Static(_) => Some(REDUCE_LANES),
                        Extent::Dynamic(_) => None,
                    };
                    (
                        build_lanes(block, &lanes)?,
                        (run.partials(count), Some(run.result_rank)),
                    )
                }
            };
            let Some(partial_layout) = ReduceLayout::new(&shape, extents, axis) else {
                return Ok(None);
            };
            (partials, partial_layout)
        }
        _ => (source, layout),
    };
    let parallel = layout.result_rank;
    let result_axes: OperandAxes = Some((0..parallel).map(Some).collect());
    // The result's dynamic axes are the source's kept ones, in order. A whole-tensor result is
    // one static element.
    let empty = block
        .append_operation(empty_tensor(
            location,
            tensor_type,
            &dynamic_sizes(&kept_extents(&dims, *axis)),
        )?)
        .result(0)?
        .into();

    let (seed_input, seed_axes) = match op {
        HirReduceOp::Sum | HirReduceOp::Mean => {
            let seed = match kind {
                Element::Float => FloatAttribute::new(context, element_type, SUM_SEED).into(),
                _ => IntegerAttribute::new(element_type, 0).into(),
            };
            let seed = block
                .append_operation(arith::constant(context, seed, location))
                .result(0)?
                .into();
            (seed, None)
        }
        HirReduceOp::Max | HirReduceOp::Min => (source, Some(layout.first.clone())),
    };
    let seeded = apply(
        context,
        location,
        block,
        Generic {
            inputs: &[seed_input],
            destination: empty,
            indexing_maps: indexing_maps(context, parallel, &[&seed_axes, &result_axes])?,
            iterators: iterator_types(context, parallel, 0)?,
        },
        tensor_type,
        fill_block(location, element_type)?,
    )?;

    // The host's loops nest in index-space order, so there the fold keeps the source's own
    // axis order: reducing an outer axis then reads the source row by row, its inner loop over
    // independent accumulators, rather than down a column per result element. Each element
    // still folds its run in run order, so the bits do not change. A GPU keeps the result's
    // axes first, its threads.
    let (walk, written, reduced): (OperandAxes, OperandAxes, Vec<bool>) =
        match (lowering.side, layout.axis) {
            (Side::Host, Some(axis)) => (
                Some((0..layout.space).map(Some).collect()),
                Some((0..layout.space).filter(|&d| d != axis).map(Some).collect()),
                (0..layout.space).map(|d| d == axis).collect(),
            ),
            _ => (
                Some(layout.walk.clone()),
                result_axes.clone(),
                (0..layout.space).map(|d| d >= parallel).collect(),
            ),
        };
    let folded = apply(
        context,
        location,
        block,
        Generic {
            inputs: &[source],
            destination: seeded,
            indexing_maps: indexing_maps(context, layout.space, &[&walk, &written])?,
            iterators: iterators(context, &reduced)?,
        },
        tensor_type,
        fold_block(context, location, element_type, &fold)?,
    )?;
    let value = match *op {
        HirReduceOp::Mean => {
            let count = count_of(context, location, block, element_type, kind, length)?;
            apply(
                context,
                location,
                block,
                Generic {
                    inputs: &[count],
                    destination: folded,
                    indexing_maps: indexing_maps(context, parallel, &[&None, &result_axes])?,
                    iterators: iterator_types(context, parallel, 0)?,
                },
                tensor_type,
                mean_block(location, element_type, kind, count.r#type())?,
            )?
        }
        _ => folded,
    };
    Ok(Some(match half {
        true => cast_elements(context, location, block, value, (&wide, result))?,
        false => value,
    }))
}

/// The run-time extent of each axis of `source`, of shape `shape`: a static one as it is, a
/// dynamic one read off `source` itself.
fn source_extents<'c, 'a>(
    context: &'c Context,
    location: Location<'c>,
    block: &'a Block<'c>,
    source: Value<'c, 'a>,
    shape: &[Option<usize>],
) -> Result<Vec<Extent<'c, 'a>>, MlirError> {
    let own: OperandAxes = Some((0..shape.len()).map(Some).collect());
    result_extents(context, location, block, shape, &[(source, &own)])?
        .ok_or(MlirError::ModuleVerificationFailed)
}

/// The extents a reduction's result keeps: the source's, but the reduced axis. A whole-tensor
/// reduction keeps none, its result being one static element.
fn kept_extents<'c, 'a>(dims: &[Extent<'c, 'a>], axis: Option<usize>) -> Vec<Extent<'c, 'a>> {
    match axis {
        None => Vec::new(),
        Some(axis) => dims
            .iter()
            .enumerate()
            .filter(|(index, _)| *index != axis)
            .map(|(_, extent)| *extent)
            .collect(),
    }
}

/// How many elements one run folds: the reduced axis's extent, or every element's for a
/// whole-tensor reduction. Multiplied out at run time only when an extent is dynamic.
fn run_length<'c, 'a>(
    context: &'c Context,
    location: Location<'c>,
    block: &'a Block<'c>,
    dims: &[Extent<'c, 'a>],
    axis: Option<usize>,
) -> Result<Extent<'c, 'a>, MlirError> {
    let reduced = match axis {
        Some(axis) => &dims[axis..=axis],
        None => dims,
    };
    reduced
        .iter()
        .try_fold(Extent::Static(1), |product, extent| {
            multiply(context, location, block, (product, *extent))
        })
}

/// `length` as the float a mean divides by, of `element`.
fn count_of<'c, 'a>(
    context: &'c Context,
    location: Location<'c>,
    block: &'a Block<'c>,
    element: Type<'c>,
    kind: Element,
    length: Extent<'c, '_>,
) -> Result<Value<'c, 'a>, MlirError> {
    // An integer count is `i64` whatever the element: a run longer than the element's range
    // has a mean inside it, but its count does not fit there.
    let wide: Type<'c> = IntegerType::new(context, 64).into();
    let integer = kind != Element::Float;
    match length {
        Extent::Static(length) => append(
            block,
            arith::constant(
                context,
                match integer {
                    true => IntegerAttribute::new(wide, length as i64).into(),
                    false => FloatAttribute::new(context, element, length as f64).into(),
                },
                location,
            ),
        ),
        Extent::Dynamic(length) => {
            let length = append(block, arith::index_cast(length, wide, location))?;
            if integer {
                return Ok(length);
            }
            append(
                block,
                OperationBuilder::new("arith.sitofp", location)
                    .add_operands(&[length])
                    .add_results(&[element])
                    .build()?,
            )
        }
    }
}

/// What one fold step computes: the reduction, its element, and where a sum's overflow
/// check points.
struct Fold<'l, 'c, 'a> {
    lowering: &'l Lowering<'c, 'a>,
    op: HirReduceOp,
    kind: Element,
    offset: usize,
}

/// Where one reduction's index space puts each axis.
struct ReduceLayout {
    /// The index space's rank: the result's axes, then the reduced ones.
    space: usize,
    result_rank: usize,
    /// The index-space dimension each source axis walks during the fold.
    walk: Vec<Option<usize>>,
    /// Each source axis read over the result's index space when taking a run's first
    /// element: the result axis it matches, or element 0 of a reduced axis.
    first: Vec<Option<usize>>,
    /// The reduced source axis, `None` for a whole-tensor reduction.
    axis: Option<usize>,
    /// Where a run's elements sit, for folding a long run in lanes.
    lanes: Option<Run>,
}

/// The source shape and reduced axis, which place each run position in the source.
struct Run {
    /// The source's extents, `None` where one is dynamic.
    extents: Vec<Option<usize>>,
    /// The reduced axis; `None` for a whole-tensor reduction, whose run is the source in
    /// row-major order.
    axis: Option<usize>,
    result_rank: usize,
}

impl Run {
    /// The partials tensor's extents: the result's, then one per lane, `lanes` of them (`None`
    /// when that count is only known at run time).
    fn partials(&self, lanes: Option<usize>) -> Vec<Option<usize>> {
        let mut extents: Vec<Option<usize>> = match self.axis {
            None => vec![Some(1)],
            Some(axis) => (0..self.extents.len())
                .filter(|&i| i != axis)
                .map(|i| self.extents[i])
                .collect(),
        };
        extents.push(lanes);
        extents
    }

    /// The host's partials tensor: the source's shape with a lane axis in the reduced axis's
    /// place, or the lanes alone for a whole-tensor reduction. Reducing it along that axis is
    /// then the source's own reduction, its loops walking the lanes in order.
    fn host_partials(&self) -> Vec<Option<usize>> {
        let Some(axis) = self.axis else {
            return vec![Some(REDUCE_LANES)];
        };
        let mut extents = self.extents.clone();
        extents[axis] = Some(REDUCE_LANES);
        extents
    }
}

impl ReduceLayout {
    /// `None` for a source that does not reduce to the result shape it implies.
    fn new(
        source: &[Option<usize>],
        result: &[Option<usize>],
        axis: Option<usize>,
    ) -> Option<Self> {
        let rank = source.len();
        let Some(axis) = axis else {
            // One parallel axis of extent 1, then every source axis reduced.
            return (result == [Some(1)]).then(|| Self {
                space: rank + 1,
                result_rank: 1,
                walk: (1..=rank).map(Some).collect(),
                first: vec![None; rank],
                axis: None,
                lanes: Some(Run {
                    extents: source.to_vec(),
                    axis: None,
                    result_rank: 1,
                }),
            });
        };
        if axis >= rank {
            return None;
        }
        let kept: Vec<Option<usize>> = (0..rank)
            .filter(|&i| i != axis)
            .map(|i| source[i])
            .collect();
        if kept != result {
            return None;
        }
        let result_axis = |i: usize| (i != axis).then(|| if i < axis { i } else { i - 1 });
        Some(Self {
            space: rank,
            result_rank: rank - 1,
            walk: (0..rank)
                .map(|i| Some(result_axis(i).unwrap_or(rank - 1)))
                .collect(),
            first: (0..rank).map(result_axis).collect(),
            axis: Some(axis),
            lanes: Some(Run {
                extents: source.to_vec(),
                axis: Some(axis),
                result_rank: rank - 1,
            }),
        })
    }
}

/// One long run's lanes: the source they read, the run they cover, and the fold they run.
struct Lanes<'r, 'c, 'v> {
    context: &'c Context,
    location: Location<'c>,
    source: Value<'c, 'v>,
    run: &'r Run,
    /// The run-time extent of each source axis.
    dims: &'r [Extent<'c, 'v>],
    /// How many elements one run folds.
    length: Extent<'c, 'v>,
    fold: &'r Fold<'r, 'c, 'v>,
    element_type: Type<'c>,
}

impl<'c> Lanes<'_, 'c, '_> {
    /// The type of a partials tensor of extents `shape`.
    fn partials_type(&self, shape: &[Option<usize>]) -> Type<'c> {
        shape_type(self.element_type, shape)
    }

    /// Run position `row * REDUCE_LANES + lane` folded into `carried`. A `ragged` row may
    /// end before the lanes do: past the run's end the lane keeps `carried`.
    fn fold_row<'b>(
        &self,
        block: &'b Block<'c>,
        kept: &[Value<'c, '_>],
        lane: Value<'c, '_>,
        row: Value<'c, '_>,
        carried: Value<'c, '_>,
        ragged: bool,
    ) -> Result<Value<'c, 'b>, MlirError> {
        let (context, location) = (self.context, self.location);
        let width = index_constant(context, location, block, REDUCE_LANES)?;
        let scaled = append(block, arith::muli(row, width, location))?;
        let position = append(block, arith::addi(scaled, lane, location))?;
        let read = |at| {
            read_run(
                context,
                location,
                block,
                self.source,
                self.run,
                self.dims,
                kept,
                at,
                self.element_type,
            )
        };
        if !ragged {
            return fold_step(
                context,
                location,
                block,
                self.fold,
                read(position)?,
                carried,
            );
        }
        let end = self.length.index(context, location, block)?;
        let inside = append(
            block,
            arith::cmpi(context, CmpiPredicate::Ult, position, end, location),
        )?;
        // A position past the run's end reads element 0, which a non-empty run always has. Its
        // answer is discarded below, so only that the read is in bounds matters.
        let zero = index_constant(context, location, block, 0)?;
        let at = append(block, arith::select(inside, position, zero, location))?;
        let folded = fold_step(context, location, block, self.fold, read(at)?, carried)?;
        append(block, arith::select(inside, folded, carried, location))
    }
}

/// Fold a long run's lanes into a partials tensor of the result's axes and a lane axis,
/// one GPU thread per lane. Lane `l` starts at run position `l` and folds `l + k * REDUCE_LANES`
/// for `k` from 1 while that is inside the run, the order the LLVM backend's lanes take. A
/// dynamic run has `min(length, REDUCE_LANES)` lanes, so every lane starts inside the run and no
/// partial is a placeholder: the partials are no bigger than the run needs.
fn build_lanes<'c, 'a>(
    block: &'a Block<'c>,
    lanes: &Lanes<'_, 'c, '_>,
) -> Result<Value<'c, 'a>, MlirError> {
    let (context, location, element_type) = (lanes.context, lanes.location, lanes.element_type);
    let run = lanes.run;
    let index = Type::index(context);
    let lane_count = match lanes.length {
        Extent::Static(_) => Extent::Static(REDUCE_LANES),
        Extent::Dynamic(length) => {
            let width = index_constant(context, location, block, REDUCE_LANES)?;
            Extent::Dynamic(append(block, arith::minui(length, width, location))?)
        }
    };
    let static_lanes = match lane_count {
        Extent::Static(count) => Some(count),
        Extent::Dynamic(_) => None,
    };
    let partials = run.partials(static_lanes);
    let partials_type = lanes.partials_type(&partials);
    let rank = run.result_rank + 1;
    let mut sizes = dynamic_sizes(&kept_extents(lanes.dims, run.axis));
    if let Extent::Dynamic(count) = lane_count {
        sizes.push(count);
    }
    let empty = append(block, empty_tensor(location, partials_type, &sizes)?)?;

    let body = Block::new(&[(element_type, location)]);
    // The result position, one `linalg.index` per surviving source axis.
    let kept: Vec<Value> = (0..run.result_rank)
        .map(|dimension| append(&body, linalg_index(context, location, dimension)?))
        .collect::<Result<_, _>>()?;
    let lane = append(&body, linalg_index(context, location, run.result_rank)?)?;
    let seed = read_run(
        context,
        location,
        &body,
        lanes.source,
        run,
        lanes.dims,
        &kept,
        lane,
        element_type,
    )?;
    let first = index_constant(context, location, &body, 1)?;
    let rows = match lanes.length {
        Extent::Static(length) => {
            index_constant(context, location, &body, length.div_ceil(REDUCE_LANES))?
        }
        Extent::Dynamic(_) => {
            let length = lanes.length.index(context, location, &body)?;
            let slack = index_constant(context, location, &body, REDUCE_LANES - 1)?;
            let biased = append(&body, arith::addi(length, slack, location))?;
            let width = index_constant(context, location, &body, REDUCE_LANES)?;
            append(&body, arith::divui(biased, width, location))?
        }
    };

    let step = Block::new(&[(index, location), (element_type, location)]);
    let row: Value = step.argument(0)?.into();
    let carried: Value = step.argument(1)?.into();
    let kept_value = lanes.fold_row(&step, &kept, lane, row, carried, true)?;
    step.append_operation(
        OperationBuilder::new("scf.yield", location)
            .add_operands(&[kept_value])
            .build()?,
    );
    let region = Region::new();
    region.append_block(step);
    let lane_value = append(
        &body,
        OperationBuilder::new("scf.for", location)
            .add_operands(&[first, rows, first, seed])
            .add_results(&[element_type])
            .add_regions([region])
            .build()?,
    )?;
    yield_value(&body, location, lane_value)?;

    let own = Some((0..rank).map(Some).collect());
    apply(
        context,
        location,
        block,
        Generic {
            inputs: &[],
            destination: empty,
            indexing_maps: indexing_maps(context, rank, &[&own])?,
            iterators: iterator_types(context, rank, 0)?,
        },
        partials_type,
        body,
    )
}

/// The host's lanes, in the same order as [`build_lanes`] but nested the other way: the
/// partials are seeded from the run's first row, then an `scf.for` over the full rows runs
/// one all-parallel `linalg.generic` over every lane at once, and a last generic folds a
/// ragged tail. Each lane still folds its own positions in run order, so the bits are the
/// device's, but the inner loop now walks lanes, independent accumulators over adjacent
/// elements, which vectorizes where one lane's chain down a 16 KB stride did not.
///
/// A whole-tensor source is collapsed to rank 1 first, so a run position is an index rather
/// than a division and a remainder per axis. An axis reduction's generic keeps the source's
/// own axis order (the lane in the reduced axis's place), so an outer-axis reduction reads
/// its rows contiguously, as the host fold below does.
fn build_host_lanes<'c, 'a>(
    block: &'a Block<'c>,
    lanes: Lanes<'_, 'c, 'a>,
    length: usize,
) -> Result<Value<'c, 'a>, MlirError> {
    let (context, location, element_type) = (lanes.context, lanes.location, lanes.element_type);
    let flat;
    let flat_dims;
    let lanes = match lanes.run.axis {
        None if lanes.run.extents.len() > 1 => {
            let rank = lanes.run.extents.len();
            let reassociation = (0..rank).map(|d| d.to_string()).collect::<Vec<_>>();
            let flat_type = RankedTensorType::new(&[length as u64], element_type, None).into();
            let source = append(
                block,
                OperationBuilder::new("tensor.collapse_shape", location)
                    .add_operands(&[lanes.source])
                    .add_attributes(&[(
                        Identifier::new(context, "reassociation"),
                        parse_attribute(context, &format!("[[{}]]", reassociation.join(", ")))?,
                    )])
                    .add_results(&[flat_type])
                    .build()?,
            )?;
            flat = Run {
                extents: vec![Some(length)],
                axis: None,
                result_rank: 1,
            };
            flat_dims = [Extent::Static(length)];
            Lanes {
                source,
                run: &flat,
                dims: &flat_dims,
                length: Extent::Static(length),
                ..lanes
            }
        }
        _ => lanes,
    };
    // The run is longer than the lanes, so it has a second row, ragged or not, which the
    // seed folds in at once rather than writing the partials only to read them back.
    let full = length / REDUCE_LANES;
    let ragged = !length.is_multiple_of(REDUCE_LANES);
    let constant = |value| index_constant(context, location, block, value);
    let mut partials = host_lane_pass(block, &lanes, None, (constant(1)?, full == 1))?;
    if full > 2 {
        let partials_type = lanes.partials_type(&lanes.run.host_partials());
        let step = Block::new(&[(Type::index(context), location), (partials_type, location)]);
        let row: Value = step.argument(0)?.into();
        let carried: Value = step.argument(1)?.into();
        let folded = host_lane_pass(&step, &lanes, Some(carried), (row, false))?;
        step.append_operation(
            OperationBuilder::new("scf.yield", location)
                .add_operands(&[folded])
                .build()?,
        );
        let region = Region::new();
        region.append_block(step);
        partials = append(
            block,
            OperationBuilder::new("scf.for", location)
                .add_operands(&[constant(2)?, constant(full)?, constant(1)?, partials])
                .add_results(&[partials_type])
                .add_regions([region])
                .build()?,
        )?;
    }
    if ragged && full > 1 {
        partials = host_lane_pass(block, &lanes, Some(partials), (constant(full)?, true))?;
    }
    Ok(partials)
}

/// One host generic over the partials folding run row `row` (`ragged` when it may end
/// before the lanes do) into each lane of `from`, or into each lane's first element when
/// there is no `from` yet.
fn host_lane_pass<'c, 'b>(
    at: &'b Block<'c>,
    lanes: &Lanes<'_, 'c, '_>,
    from: Option<Value<'c, '_>>,
    (row, ragged): (Value<'c, '_>, bool),
) -> Result<Value<'c, 'b>, MlirError> {
    let (context, location, element_type) = (lanes.context, lanes.location, lanes.element_type);
    let run = lanes.run;
    let shape = run.host_partials();
    let rank = shape.len();
    let partials_type = lanes.partials_type(&shape);
    // The partials keep the source's axes, so the lane is the reduced axis's dimension and
    // every other dimension is the source axis it names.
    let lane_dimension = run.axis.unwrap_or(0);
    let body = Block::new(&[(element_type, location)]);
    let kept: Vec<Value> = (0..rank)
        .filter(|&dimension| dimension != lane_dimension)
        .map(|dimension| append(&body, linalg_index(context, location, dimension)?))
        .collect::<Result<_, MlirError>>()?;
    let lane = append(&body, linalg_index(context, location, lane_dimension)?)?;
    let (destination, carried) = match from {
        Some(from) => (from, body.argument(0)?.into()),
        None => (
            append(at, empty_tensor(location, partials_type, &[])?)?,
            read_run(
                context,
                location,
                &body,
                lanes.source,
                run,
                lanes.dims,
                &kept,
                lane,
                element_type,
            )?,
        ),
    };
    let value = lanes.fold_row(&body, &kept, lane, row, carried, ragged)?;
    yield_value(&body, location, value)?;
    let own = Some((0..rank).map(Some).collect());
    apply(
        context,
        location,
        at,
        Generic {
            inputs: &[],
            destination,
            indexing_maps: indexing_maps(context, rank, &[&own])?,
            iterators: iterator_types(context, rank, 0)?,
        },
        partials_type,
        body,
    )
}

/// The source element at run position `position` of the run whose result position is
/// `kept`: along the reduced axis for an axis reduction, and row-major through every axis
/// for a whole-tensor one. `dims` are the source's extents, read at run time where dynamic.
#[expect(
    clippy::too_many_arguments,
    reason = "the builder's context, location and block travel beside the run's operands, so a parameter struct would only rename them"
)]
fn read_run<'c, 'a>(
    context: &'c Context,
    location: Location<'c>,
    at: &'a Block<'c>,
    source: Value<'c, '_>,
    run: &Run,
    dims: &[Extent<'c, '_>],
    kept: &[Value<'c, '_>],
    position: Value<'c, '_>,
    element_type: Type<'c>,
) -> Result<Value<'c, 'a>, MlirError> {
    let mut operands: Vec<Value> = vec![source];
    match run.axis {
        // `kept` holds one index per surviving axis, in order, so axis `i` past the reduced
        // one is `kept[i - 1]`.
        Some(axis) => operands.extend((0..dims.len()).map(|i| match i.cmp(&axis) {
            std::cmp::Ordering::Less => kept[i],
            std::cmp::Ordering::Equal => position,
            std::cmp::Ordering::Greater => kept[i - 1],
        })),
        None => {
            // An axis's coordinate is the position divided by the product of the extents after
            // it, modulo its own extent.
            let mut strides = vec![Extent::Static(1); dims.len()];
            let mut stride = Extent::Static(1);
            for (slot, extent) in strides.iter_mut().zip(dims).rev() {
                *slot = stride;
                stride = multiply(context, location, at, (stride, *extent))?;
            }
            for (axis, (stride, extent)) in strides.iter().zip(dims).enumerate() {
                let mut coordinate = position;
                if !matches!(stride, Extent::Static(1)) {
                    let divisor = stride.index(context, location, at)?;
                    coordinate = append(at, arith::divui(coordinate, divisor, location))?;
                }
                if axis > 0 {
                    let modulus = extent.index(context, location, at)?;
                    coordinate = append(at, arith::remui(coordinate, modulus, location))?;
                }
                operands.push(coordinate);
            }
        }
    }
    append(
        at,
        OperationBuilder::new("tensor.extract", location)
            .add_operands(&operands)
            .add_results(&[element_type])
            .build()?,
    )
}

pub(crate) fn index_constant<'c, 'a>(
    context: &'c Context,
    location: Location<'c>,
    at: &'a Block<'c>,
    value: usize,
) -> Result<Value<'c, 'a>, MlirError> {
    let index = Type::index(context);
    append(
        at,
        arith::constant(
            context,
            IntegerAttribute::new(index, value as i64).into(),
            location,
        ),
    )
}

/// Append one `linalg.generic` with `body` as its single block, yielding its result.
pub(crate) fn apply<'c, 'a>(
    context: &'c Context,
    location: Location<'c>,
    block: &'a Block<'c>,
    generic: Generic<'c, '_>,
    tensor_type: Type<'c>,
    body: Block<'c>,
) -> Result<Value<'c, 'a>, MlirError> {
    let region = Region::new();
    region.append_block(body);
    Ok(block
        .append_operation(generic_op(context, location, generic, tensor_type, region)?)
        .result(0)?
        .into())
}

/// One fold step over `(element, accumulator)`. A sum adds the element to the
/// accumulator. `.max()` / `.min()` keep the element when it is a number that sorts
/// before the accumulator, or when the accumulator is NaN: the LLVM backend's sorting
/// comparator, so a NaN never wins and a NaN seed gives way to the first number.
fn fold_block<'c>(
    context: &'c Context,
    location: Location<'c>,
    element: Type<'c>,
    fold: &Fold<'_, 'c, '_>,
) -> Result<Block<'c>, MlirError> {
    let block = Block::new(&[(element, location); FOLD_BODY_ARGUMENTS]);
    let value: Value = block.argument(0)?.into();
    let carried: Value = block.argument(1)?.into();
    let folded = fold_step(context, location, &block, fold, value, carried)?;
    yield_value(&block, location, folded)?;
    Ok(block)
}

/// `value` folded into `carried`, appended to `block`. An integer sum carries the host's
/// overflow check, at the reduction.
fn fold_step<'c, 'a>(
    context: &'c Context,
    location: Location<'c>,
    block: &'a Block<'c>,
    fold: &Fold<'_, 'c, '_>,
    value: Value<'c, '_>,
    carried: Value<'c, '_>,
) -> Result<Value<'c, 'a>, MlirError> {
    let descending = match fold.op {
        HirReduceOp::Sum | HirReduceOp::Mean => {
            let at = At {
                offset: fold.offset,
                position: None,
            };
            return fold
                .lowering
                .arith(block, BinaryOp::Add, fold.kind, (carried, value), at)?
                .ok_or(MlirError::ModuleVerificationFailed);
        }
        HirReduceOp::Max => true,
        HirReduceOp::Min => false,
    };
    let wins = precedes(
        context,
        location,
        block,
        (value, carried),
        fold.kind,
        descending,
    )?;
    append(block, arith::select(wins, value, carried, location))
}

/// A mean's last step over `(length, total)`: the total divided by the run length. An integer
/// total divides as integer `/` does, truncating toward zero, in 64 bits beside the `i64` count.
/// The run is not empty (that is checked before the fold) and the quotient is no larger than the
/// total, so neither of `/`'s guards can fire and it narrows back without loss.
fn mean_block<'c>(
    location: Location<'c>,
    element: Type<'c>,
    kind: Element,
    count: Type<'c>,
) -> Result<Block<'c>, MlirError> {
    let block = Block::new(&[(count, location), (element, location)]);
    let length: Value = block.argument(0)?.into();
    let total: Value = block.argument(1)?.into();
    let mean = match kind {
        Element::Float | Element::Half => append(&block, arith::divf(total, length, location))?,
        Element::Signed(bits) => {
            let wide = match bits < 64 {
                true => append(&block, arith::extsi(total, count, location))?,
                false => total,
            };
            narrow(
                &block,
                location,
                element,
                bits,
                arith::divsi(wide, length, location),
            )?
        }
        Element::Unsigned(bits) => {
            let wide = match bits < 64 {
                true => append(&block, arith::extui(total, count, location))?,
                false => total,
            };
            narrow(
                &block,
                location,
                element,
                bits,
                arith::divui(wide, length, location),
            )?
        }
    };
    yield_value(&block, location, mean)?;
    Ok(block)
}

/// `quotient`, a 64-bit integer, appended to `block` and truncated to `element`'s `bits`.
fn narrow<'c, 'a>(
    block: &'a Block<'c>,
    location: Location<'c>,
    element: Type<'c>,
    bits: u32,
    quotient: melior::ir::Operation<'c>,
) -> Result<Value<'c, 'a>, MlirError> {
    let quotient = append(block, quotient)?;
    match bits < 64 {
        true => append(block, arith::trunci(quotient, element, location)),
        false => Ok(quotient),
    }
}

pub(crate) fn append<'c, 'a>(
    block: &'a Block<'c>,
    operation: melior::ir::Operation<'c>,
) -> Result<Value<'c, 'a>, MlirError> {
    Ok(block.append_operation(operation).result(0)?.into())
}

pub(crate) fn yield_value<'c>(
    block: &Block<'c>,
    location: Location<'c>,
    value: Value<'c, '_>,
) -> Result<(), MlirError> {
    block.append_operation(
        OperationBuilder::new("linalg.yield", location)
            .add_operands(&[value])
            .build()?,
    );
    Ok(())
}

#[cfg(test)]
mod tests {
    use crate::{
        GpuTarget,
        bridge::tests::{assert_launches_on_gpu, assert_lowered, mlir_of, operand, tensor_of},
        lower_for_gpu, lower_for_link,
    };

    use neuro_hir::{
        HirExpr, HirExprKind, HirFunction, HirItem, HirParam, HirProgram, HirReduceOp, HirStmt,
        HirTarget, HirType,
    };
    use shared_types::Span;

    /// A program with a transfer, so lowering outlines its operations. The harness binds
    /// no named arguments, so an axis is positional.
    fn program(body: &str) -> HirProgram {
        let source = format!(
            "enum Device {{\n    CPU,\n    GPU(i32)\n}}\n\nfunc main() -> i32 {{\n    val m: Tensor<f32, [37, 19]> = Tensor::ones()\n    val g = m.clone().to(Device::GPU(0))\n{body}\n    return 0\n}}\n"
        );
        let ast = syntax_parsing::parse(&source).expect("the program parses");
        hir_lowering::lower_program(&ast).expect("the program lowers to HIR")
    }

    fn nvidia() -> GpuTarget {
        GpuTarget::Nvidia {
            chip: "sm_80".to_string(),
        }
    }

    fn outlined(program: &HirProgram) -> usize {
        program
            .items
            .iter()
            .filter(|item| {
                matches!(item, HirItem::Function(f) if f.target == HirTarget::FollowsOperands)
            })
            .count()
    }

    #[test]
    fn every_reduction_becomes_a_kernel_per_step() {
        for (body, launches) in [
            ("    val r = g.sum(1)", 2),
            ("    val r = g.mean(0)", 3),
            ("    val r = g.max()", 2),
            ("    val r = g.min(1)", 2),
        ] {
            let program = program(body);
            let bodies = lower_for_gpu(&program, &nvidia(), crate::Overflow::Checked)
                .expect("a reduction lowers");
            assert_eq!(bodies.functions.len(), 1, "`{body}`");
            let ir = &bodies.llvm_ir;
            assert_eq!(
                ir.matches("call void @mgpuLaunchKernel").count(),
                launches,
                "`{body}`: a seed, a fold, and a mean's division:\n{ir}"
            );
        }
    }

    #[test]
    fn a_run_longer_than_the_lanes_folds_its_lanes_first() {
        // One more kernel than a short run: the lane partials, then the seed and the fold
        // over the lane axis (and a mean's division).
        for (shape, body, launches) in [
            ("[5000]", "    val r = g.sum()", 3),
            ("[3, 9000]", "    val r = g.mean(1)", 4),
            ("[9000, 3]", "    val r = g.max(0)", 3),
            ("[4096, 2]", "    val r = g.sum(0)", 2),
        ] {
            let source = format!(
                "enum Device {{\n    CPU,\n    GPU(i32)\n}}\n\nfunc main() -> i32 {{\n    val m: Tensor<f32, {shape}> = Tensor::ones()\n    val g = m.clone().to(Device::GPU(0))\n{body}\n    return 0\n}}\n"
            );
            let ast = syntax_parsing::parse(&source).expect("the program parses");
            let program = hir_lowering::lower_program(&ast).expect("the program lowers to HIR");
            let ir = lower_for_gpu(&program, &nvidia(), crate::Overflow::Checked)
                .expect("a reduction lowers")
                .llvm_ir;
            assert_eq!(
                ir.matches("call void @mgpuLaunchKernel").count(),
                launches,
                "`{body}` over {shape}:\n{ir}"
            );
        }
    }

    #[test]
    fn a_max_keeps_the_host_comparator() {
        let ir = lower_for_gpu(
            &program("    val r = g.max()"),
            &nvidia(),
            crate::Overflow::Checked,
        )
        .expect("a reduction lowers")
        .llvm_ir;
        assert!(
            ir.contains("setp.gt.f32") || ir.contains("setp.gtu.f32"),
            "{ir}"
        );
        assert!(
            ir.contains("setp.nan.f32"),
            "a NaN accumulator gives way:\n{ir}"
        );
    }

    #[test]
    fn a_body_the_gpu_path_cannot_take_keeps_its_host_body_without_error() {
        // A rank-1 axis reduction has a rank-0 result, which no GPU launch covers, so it has
        // a host body alone and is no error.
        let program = program("    val v: Tensor<f32, [4]> = Tensor::ones()\n    val s = v.sum(0)");
        assert_eq!(outlined(&program), 1);
        let bodies = lower_for_gpu(&program, &nvidia(), crate::Overflow::Checked)
            .expect("nothing to refuse");
        assert!(bodies.functions.is_empty());
        let host = lower_for_link(&program, crate::Overflow::Checked).expect("the host lowers");
        assert_eq!(host.functions.len(), 1);
    }

    #[test]
    fn an_integer_sum_folds_in_order_with_its_overflow_check() {
        let integer = program(
            "    val i: Tensor<i32, [100, 50]> = Tensor::ones()\n    val gi = i.to(Device::GPU(0))\n    val s = gi.sum()",
        );
        assert_eq!(outlined(&integer), 1);
        let bodies = lower_for_gpu(&integer, &nvidia(), crate::Overflow::Checked)
            .expect("an integer sum lowers");
        assert_eq!(bodies.functions.len(), 1);
        let [(_, checks)] = bodies.guards.as_slice() else {
            panic!("expected one checked symbol, got {:?}", bodies.guards);
        };
        assert_eq!(checks.len(), 1, "one add, checked for overflow");
        // 5000 elements, past `REDUCE_LANES`, fold in one thread rather than in lanes, the
        // host's order: a seed and a fold, where a float sum takes a lane pass between them.
        let launches = |ir: &str| ir.matches("call void @mgpuLaunchKernel").count();
        let float = lower_for_gpu(
            &program(
                "    val f: Tensor<f32, [100, 50]> = Tensor::ones()\n    val gf = f.to(Device::GPU(0))\n    val s = gf.sum()",
            ),
            &nvidia(),
            crate::Overflow::Checked,
        )
        .expect("a float sum lowers");
        assert_eq!(launches(&bodies.llvm_ir), 2, "{}", bodies.llvm_ir);
        assert_eq!(launches(&float.llvm_ir), 3, "{}", float.llvm_ir);
    }

    #[test]
    fn a_host_fold_walks_its_source_in_row_major_order() {
        // Reducing an outer axis on the host keeps the source's axis order, so the loops read
        // it row by row and the inner one runs over independent accumulators. A GPU fold keeps
        // the result's axes first, one thread per result element.
        let program = program("    val r = g.sum(0)");
        let fold = |side| {
            let context = crate::context::new_context();
            let (module, _) = crate::lower::build_linkable_module(
                &context,
                &program,
                (crate::Overflow::Checked, side),
                &|function| function.target == HirTarget::FollowsOperands,
            )
            .expect("the body builds");
            module.as_operation().to_string()
        };
        let host = fold(crate::guards::Side::Host);
        assert!(
            host.contains("affine_map<(d0, d1) -> (d1)>")
                && host.contains(r#"["reduction", "parallel"]"#),
            "{host}"
        );
        let device = fold(crate::guards::Side::Device);
        assert!(device.contains(r#"["parallel", "reduction"]"#), "{device}");
    }

    #[test]
    fn a_long_host_run_folds_row_by_row_over_every_lane() {
        // The host's lanes: a generic over every lane per row inside an `scf.for` over the
        // rows, the partials in the source's own layout, and a whole-tensor source flattened
        // first. The device keeps one thread per lane, its rows a loop inside the thread.
        let module = |shape: &str, body: &str, side| {
            let source = format!(
                "enum Device {{\n    CPU,\n    GPU(i32)\n}}\n\nfunc main() -> i32 {{\n    val m: Tensor<f32, {shape}> = Tensor::ones()\n    val g = m.clone().to(Device::GPU(0))\n{body}\n    return 0\n}}\n"
            );
            let ast = syntax_parsing::parse(&source).expect("the program parses");
            let program = hir_lowering::lower_program(&ast).expect("the program lowers to HIR");
            let context = crate::context::new_context();
            let (module, _) = crate::lower::build_linkable_module(
                &context,
                &program,
                (crate::Overflow::Checked, side),
                &|function| function.target == HirTarget::FollowsOperands,
            )
            .expect("the body builds");
            module.as_operation().to_string()
        };
        let host = crate::guards::Side::Host;
        for (shape, body, partials) in [
            ("[3, 70000]", "    val r = g.sum(1)", "tensor<3x4096xf32>"),
            ("[20000, 3]", "    val r = g.max(0)", "tensor<4096x3xf32>"),
            ("[5, 4000]", "    val r = g.mean()", "tensor<4096xf32>"),
        ] {
            let ir = module(shape, body, host);
            assert!(
                ir.contains(&format!(") -> ({partials}) {{")),
                "{shape}: the row loop carries the partials:\n{ir}"
            );
            assert!(
                !ir.contains(") -> (f32) {"),
                "{shape}: no lane loops down its rows alone:\n{ir}"
            );
        }
        assert!(module("[5, 4000]", "    val r = g.sum()", host).contains("tensor.collapse_shape"));
        // A last-axis run of a few rows folds lane by lane on the host as well.
        let short = module("[3, 20000]", "    val r = g.sum(1)", host);
        assert!(short.contains(") -> (f32) {"), "{short}");
        let device = module(
            "[3, 20000]",
            "    val r = g.sum(1)",
            crate::guards::Side::Device,
        );
        assert!(
            device.contains(") -> (f32) {") && !device.contains("tensor.collapse_shape"),
            "{device}"
        );
    }

    #[test]
    fn the_cpu_path_computes_every_reduction() {
        let program = program("    val r = g.sum(1)\n    val t = g.mean()\n    val m = g.max(0)");
        let bodies =
            lower_for_link(&program, crate::Overflow::Checked).expect("the CPU path lowers");
        assert_eq!(bodies.functions.len(), outlined(&program));
    }

    /// `func f(a: T) -> R { return a.OP(axis) }`, outlined the way lowering writes it: a
    /// whole-tensor reduction is boxed in a one-element tensor, an axis reduction is bare.
    fn reduction_over(
        element: HirType,
        shape: Vec<Option<usize>>,
        op: HirReduceOp,
        axis: Option<usize>,
    ) -> HirProgram {
        let span = Span::new(0, 1);
        let source = tensor_of(element.clone(), shape.clone());
        let reduce = |ty: HirType| {
            HirExpr::new(
                HirExprKind::TensorReduce {
                    receiver: Box::new(operand("a", &source)),
                    op,
                    axis,
                },
                ty,
                span,
            )
        };
        let (result, body) = match axis {
            None => {
                let boxed = tensor_of(element.clone(), vec![Some(1)]);
                let literal = HirExpr::new(
                    HirExprKind::TensorLiteral {
                        elements: vec![reduce(element)],
                    },
                    boxed.clone(),
                    span,
                );
                (boxed, literal)
            }
            Some(axis) => {
                let kept: Vec<Option<usize>> = shape
                    .iter()
                    .enumerate()
                    .filter(|(index, _)| *index != axis)
                    .map(|(_, extent)| *extent)
                    .collect();
                let result = tensor_of(element, kept);
                (result.clone(), reduce(result))
            }
        };
        HirProgram {
            items: vec![HirItem::Function(HirFunction {
                result_extents: Vec::new(),
                name: "f".to_string(),
                params: vec![HirParam {
                    name: "a".to_string(),
                    ty: source,
                    span,
                }],
                return_type: result,
                body: vec![HirStmt::Return {
                    value: Some(body),
                    span,
                }],
                target: HirTarget::FollowsOperands,
                span,
            })],
        }
    }

    /// A float sum over a dynamic run may be longer than `REDUCE_LANES`, so it folds in lanes
    /// whatever its run-time length: the lane loop is an `scf.for`, and the answer is the
    /// language's lane order.
    #[test]
    fn a_dynamic_float_sum_folds_in_lanes() {
        let program = reduction_over(HirType::F32, vec![None, Some(3)], HirReduceOp::Sum, None);
        let mlir = mlir_of(&program);
        assert!(mlir.contains("scf.for"), "the lanes are a loop:\n{mlir}");
        assert_lowered(&program);
        assert_launches_on_gpu(&program);
    }

    /// `.mean(axis: 0)` over a dynamic axis divides by that axis's run-time count.
    #[test]
    fn a_dynamic_mean_divides_by_the_run_time_count() {
        let program = reduction_over(
            HirType::F32,
            vec![None, Some(3)],
            HirReduceOp::Mean,
            Some(0),
        );
        let mlir = mlir_of(&program);
        assert!(
            mlir.contains("arith.sitofp") && mlir.contains("tensor.dim"),
            "the divisor is the run-time count:\n{mlir}"
        );
        assert_lowered(&program);
        assert_launches_on_gpu(&program);
    }

    /// An integer sum folds in order and never in lanes, so a dynamic one is one sequential fold.
    #[test]
    fn a_dynamic_integer_sum_folds_in_order() {
        let program = reduction_over(HirType::I32, vec![None], HirReduceOp::Sum, None);
        let mlir = mlir_of(&program);
        assert!(
            !mlir.contains("scf.for"),
            "no lanes for an integer run:\n{mlir}"
        );
        assert!(
            mlir.contains("memref<1xi64>"),
            "the overflow check:\n{mlir}"
        );
        assert_lowered(&program);
        assert_launches_on_gpu(&program);
    }

    /// A max over a dynamic kept axis keeps that axis dynamic in the result.
    #[test]
    fn a_dynamic_kept_axis_stays_dynamic_in_an_axis_reduction() {
        let program = reduction_over(HirType::F32, vec![None, Some(3)], HirReduceOp::Max, Some(1));
        let mlir = mlir_of(&program);
        assert!(mlir.contains("tensor<?xf32>"), "{mlir}");
        assert_lowered(&program);
        assert_launches_on_gpu(&program);
    }
}
