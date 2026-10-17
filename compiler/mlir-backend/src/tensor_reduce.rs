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

use crate::{
    errors::MlirError,
    guards::{At, Element, Lowering, Side},
    lower::map_type,
    tensor_arithmetic::{
        Generic, OperandAxes, build_expression, cast_elements, empty_tensor, fill_block,
        generic_op, half_precision, indexing_maps, iterator_types, iterators, parse_attribute,
        tensor_parts, with_element,
    },
    tensor_layout::linalg_index,
    tensor_sort::precedes,
};

use ast_types::BinaryOp;
use melior::{
    Context,
    dialect::arith::{self, CmpiPredicate},
    ir::{
        Block, BlockLike, Identifier, Location, Region, RegionLike, Type, Value,
        attribute::{FloatAttribute, IntegerAttribute},
        operation::OperationBuilder,
        r#type::RankedTensorType,
    },
};
use neuro_hir::{HirExpr, HirExprKind, HirReduceOp, HirType, REDUCE_LANES};

/// How many region arguments a fold body takes: the source element, then the accumulator.
const FOLD_BODY_ARGUMENTS: usize = 2;

/// The seed of a sum: adding it to any float gives that float back, `-0.0` included.
const SUM_SEED: f64 = -0.0;

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
    let (Some((element, source)), Some((_, extents))) =
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
    let Some(layout) = ReduceLayout::new(source, extents, *axis) else {
        return Ok(None);
    };
    let Some(kind) = Element::of(element) else {
        return Ok(None);
    };
    if kind != Element::Float && *op == HirReduceOp::Mean {
        return Ok(None);
    }
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

    let tensor_type = map_type(context, &folded_type)?;
    let element_type = map_type(context, element)?;
    let length = layout.length;
    let fold = Fold {
        lowering,
        op: *op,
        kind,
        offset: reduce.span.start,
    };
    let (source, layout) = match layout.lanes {
        Some(ref run) if length > REDUCE_LANES && kind == Element::Float => {
            let lanes = Lanes {
                context,
                location,
                source,
                run,
                fold: &fold,
                element_type,
            };
            let (partials, (shape, axis)) = match lowering.side {
                Side::Host => (
                    build_host_lanes(block, lanes)?,
                    (run.host_partials(), run.axis),
                ),
                Side::Device => (
                    build_lanes(block, &lanes)?,
                    (run.partials(), Some(run.result_rank)),
                ),
            };
            let Some(lanes) = ReduceLayout::new(&shape, extents, axis) else {
                return Ok(None);
            };
            (partials, lanes)
        }
        _ => (source, layout),
    };
    let parallel = layout.result_rank;
    let result_axes: OperandAxes = Some((0..parallel).map(Some).collect());
    let empty = block
        .append_operation(empty_tensor(location, tensor_type, &[])?)
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
            let length = FloatAttribute::new(context, element_type, length as f64).into();
            let length = block
                .append_operation(arith::constant(context, length, location))
                .result(0)?
                .into();
            apply(
                context,
                location,
                block,
                Generic {
                    inputs: &[length],
                    destination: folded,
                    indexing_maps: indexing_maps(context, parallel, &[&None, &result_axes])?,
                    iterators: iterator_types(context, parallel, 0)?,
                },
                tensor_type,
                mean_block(location, element_type)?,
            )?
        }
        _ => folded,
    };
    Ok(Some(match half {
        true => cast_elements(context, location, block, value, (&wide, result))?,
        false => value,
    }))
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
    /// How many elements one run folds, which a mean divides by.
    length: usize,
    /// Where a run's elements sit, for folding a long run in lanes.
    lanes: Option<Run>,
}

/// The source shape and reduced axis, which place each run position in the source.
struct Run {
    extents: Vec<usize>,
    /// The reduced axis; `None` for a whole-tensor reduction, whose run is the source in
    /// row-major order.
    axis: Option<usize>,
    result_rank: usize,
}

impl Run {
    /// The partials tensor's extents: the result's, then one per lane.
    fn partials(&self) -> Vec<Option<usize>> {
        let mut extents: Vec<Option<usize>> = match self.axis {
            None => vec![Some(1)],
            Some(axis) => (0..self.extents.len())
                .filter(|&i| i != axis)
                .map(|i| Some(self.extents[i]))
                .collect(),
        };
        extents.push(Some(REDUCE_LANES));
        extents
    }

    /// The host's partials tensor: the source's shape with a lane axis in the reduced axis's
    /// place, or the lanes alone for a whole-tensor reduction. Reducing it along that axis is
    /// then the source's own reduction, its loops walking the lanes in order.
    fn host_partials(&self) -> Vec<Option<usize>> {
        let Some(axis) = self.axis else {
            return vec![Some(REDUCE_LANES)];
        };
        let mut extents: Vec<Option<usize>> = self.extents.iter().copied().map(Some).collect();
        extents[axis] = Some(REDUCE_LANES);
        extents
    }
}

impl ReduceLayout {
    /// `None` for anything but a static source reduced to the result shape it implies.
    fn new(
        source: &[Option<usize>],
        result: &[Option<usize>],
        axis: Option<usize>,
    ) -> Option<Self> {
        let extents = source.iter().copied().collect::<Option<Vec<usize>>>()?;
        let rank = extents.len();
        let Some(axis) = axis else {
            // One parallel axis of extent 1, then every source axis reduced.
            return (result == [Some(1)]).then(|| Self {
                space: rank + 1,
                result_rank: 1,
                walk: (1..=rank).map(Some).collect(),
                first: vec![None; rank],
                axis: None,
                length: extents.iter().product(),
                lanes: Some(Run {
                    extents: extents.clone(),
                    axis: None,
                    result_rank: 1,
                }),
            });
        };
        let kept: Vec<Option<usize>> = (0..rank)
            .filter(|&i| i != axis)
            .map(|i| Some(extents[i]))
            .collect();
        if axis >= rank || kept != result {
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
            length: extents[axis],
            lanes: Some(Run {
                extents: extents.clone(),
                axis: Some(axis),
                result_rank: rank - 1,
            }),
        })
    }
}

/// One long run's lanes: the source they read and the fold they run.
struct Lanes<'r, 'c, 'v> {
    context: &'c Context,
    location: Location<'c>,
    source: Value<'c, 'v>,
    run: &'r Run,
    fold: &'r Fold<'r, 'c, 'v>,
    element_type: Type<'c>,
}

impl<'c> Lanes<'_, 'c, '_> {
    /// The type of a partials tensor of extents `shape`.
    fn partials_type(&self, shape: &[Option<usize>]) -> Type<'c> {
        // Every extent is static: `ReduceLayout::new` admits only a static source.
        let dimensions: Vec<u64> = shape
            .iter()
            .map(|extent| extent.map_or(0, |extent| extent as u64))
            .collect();
        RankedTensorType::new(&dimensions, self.element_type, None).into()
    }

    /// How many elements one run folds.
    fn length(&self) -> usize {
        match self.run.axis {
            Some(axis) => self.run.extents[axis],
            None => self.run.extents.iter().product(),
        }
    }

    /// Run position `row * REDUCE_LANES + lane` folded into `carried`. A `ragged` row may
    /// end before the lanes do: past the run's end the lane reads its own first element and
    /// keeps `carried`.
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
        let end = index_constant(context, location, block, self.length())?;
        let inside = append(
            block,
            arith::cmpi(context, CmpiPredicate::Ult, position, end, location),
        )?;
        let at = append(block, arith::select(inside, position, lane, location))?;
        let folded = fold_step(context, location, block, self.fold, read(at)?, carried)?;
        append(block, arith::select(inside, folded, carried, location))
    }
}

/// Fold a long run's lanes into a partials tensor of the result's axes and a lane axis,
/// one GPU thread per lane. Lane `l` starts at run position `l` (inside the run, which is
/// longer than the lanes) and folds `l + k * REDUCE_LANES` for `k` from 1 while that is
/// inside the run, the order the LLVM backend's lanes take.
fn build_lanes<'c, 'a>(
    block: &'a Block<'c>,
    lanes: &Lanes<'_, 'c, '_>,
) -> Result<Value<'c, 'a>, MlirError> {
    let (context, location, element_type) = (lanes.context, lanes.location, lanes.element_type);
    let run = lanes.run;
    let index = Type::index(context);
    let partials_type = lanes.partials_type(&run.partials());
    let rank = run.result_rank + 1;
    let empty = append(block, empty_tensor(location, partials_type, &[])?)?;

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
        &kept,
        lane,
        element_type,
    )?;
    let first = index_constant(context, location, &body, 1)?;
    let rows = index_constant(
        context,
        location,
        &body,
        lanes.length().div_ceil(REDUCE_LANES),
    )?;

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
) -> Result<Value<'c, 'a>, MlirError> {
    let (context, location, element_type) = (lanes.context, lanes.location, lanes.element_type);
    let length = lanes.length();
    let flat;
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
                extents: vec![length],
                axis: None,
                result_rank: 1,
            };
            Lanes {
                source,
                run: &flat,
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
/// for a whole-tensor one.
#[expect(clippy::too_many_arguments)]
fn read_run<'c, 'a>(
    context: &'c Context,
    location: Location<'c>,
    at: &'a Block<'c>,
    source: Value<'c, '_>,
    run: &Run,
    kept: &[Value<'c, '_>],
    position: Value<'c, '_>,
    element_type: Type<'c>,
) -> Result<Value<'c, 'a>, MlirError> {
    let constant = |value| index_constant(context, location, at, value);
    let mut operands: Vec<Value> = vec![source];
    match run.axis {
        // `kept` holds one index per surviving axis, in order, so axis `i` past the reduced
        // one is `kept[i - 1]`.
        Some(axis) => operands.extend((0..run.extents.len()).map(|i| match i.cmp(&axis) {
            std::cmp::Ordering::Less => kept[i],
            std::cmp::Ordering::Equal => position,
            std::cmp::Ordering::Greater => kept[i - 1],
        })),
        None => {
            let mut stride: usize = run.extents.iter().product();
            for (i, extent) in run.extents.iter().enumerate() {
                stride /= extent;
                let mut coordinate = position;
                if stride > 1 {
                    let divisor = constant(stride)?;
                    coordinate = append(at, arith::divui(coordinate, divisor, location))?;
                }
                if i > 0 {
                    let modulus = constant(*extent)?;
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

/// A mean's last step over `(length, total)`: the total divided by the run length.
fn mean_block<'c>(location: Location<'c>, element: Type<'c>) -> Result<Block<'c>, MlirError> {
    let block = Block::new(&[(element, location); FOLD_BODY_ARGUMENTS]);
    let mean = append(
        &block,
        arith::divf(
            block.argument(1)?.into(),
            block.argument(0)?.into(),
            location,
        ),
    )?;
    yield_value(&block, location, mean)?;
    Ok(block)
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
    use crate::{GpuTarget, lower_for_gpu, lower_for_link};

    use neuro_hir::{HirItem, HirProgram, HirTarget};

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
            ("[3, 20000]", "    val r = g.sum(1)", "tensor<3x4096xf32>"),
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
}
