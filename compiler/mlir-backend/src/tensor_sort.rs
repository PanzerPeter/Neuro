// `.sort()`, `.argsort()` and `.topk()` as `linalg` and `scf`, on the host and on a GPU.
//
// Each sort builds the stable order of every run (the positions of its elements, smallest
// first under the comparator) and then gathers what each result wants through it: the
// elements, the positions, or `.topk`'s leading `k` of both, so `.topk` never reads past
// its `k`. Every stable sort under one comparator gives the same order, so which algorithm
// ran is never observable.
//
// The merge sort runs everywhere. It is bottom-up: the pass of width `w` (1, 2, 4, ...)
// merges each pair of sorted blocks of `w` run positions into one of `2w`. A pass is one
// all-parallel `linalg.generic` over the source's axes, so on a GPU every position of every
// run is a thread, and each finds its own element by a merge-path binary search for how many
// of the left block's elements come before it. A right element goes first only if it strictly
// precedes, which keeps equal elements in their order. The search is an `scf.for` of a
// count fixed by the pass, so the threads of a warp do not diverge.
//
// ponytail: O(n log^2 n) comparisons, a binary search per position per pass. A host merge
// that walks both blocks in step is O(n log n), if a host float sort ever measures slow.
//
// An integer run on the host at least `RADIX_BUCKETS` long sorts by LSD radix instead
// (`radix.rs`).
//
// A run whose length is dynamic has no static pass count, so its merge passes are a loop: the
// `ceil(log2(n))` passes run two at a time over two buffers, which the loop names statically
// (no buffer is carried through the loop, since bufferization would copy the swapped ones on
// every iteration). The width of pass `k` is `2^k` and its search takes `k + 1` steps. An odd
// count copies the answer back into the first buffer. Radix needs static extents and so is not
// taken for a dynamic run, which is no change to the order: every stable sort gives the same one.

use crate::{
    errors::MlirError,
    guards::{Element, Lowering, Side},
    lower::{dynamic_extent, map_type},
    tensor_arithmetic::{
        Extent, Generic, OperandAxes, build_expression, dynamic_sizes, empty_tensor, indexing_maps,
        iterator_types, result_extents, shape_type, tensor_parts,
    },
    tensor_layout::linalg_index,
    tensor_reduce::{append, apply, index_constant, yield_value},
};

use melior::{
    Context,
    dialect::{
        arith::{self, CmpfPredicate, CmpiPredicate},
        memref, scf,
    },
    ir::{
        Attribute, Block, BlockLike, Identifier, Location, Region, RegionLike, Type, TypeLike,
        Value, ValueLike,
        attribute::{DenseI32ArrayAttribute, IntegerAttribute},
        operation::OperationBuilder,
        r#type::{IntegerType, MemRefType},
    },
};
use neuro_hir::{HirExpr, HirExprKind, HirSortKind, HirType};

mod radix;

/// The digits a radix pass counts. A host integer run at least this long sorts by radix,
/// so a pass never spends more on its buckets than on its elements.
const RADIX_BUCKETS: usize = 256;

/// What one output of a sort takes from the position its order holds.
#[derive(Clone, Copy)]
enum Picked {
    /// The element there: `.sort()`, and `.topk`'s values.
    Element,
    /// The position itself: `.argsort()`, and `.topk`'s indices.
    Position,
}

/// The runs a sort orders: the source, along `axis`.
struct Runs<'c, 'a, 'e> {
    source: Value<'c, 'a>,
    element: Type<'c>,
    kind: Element,
    descending: bool,
    /// The source's extents, `None` where one is dynamic.
    shape: &'e [Option<usize>],
    /// Each axis's run-time extent: the static one, or a dynamic one read off the source.
    dims: Vec<Extent<'c, 'a>>,
    axis: usize,
}

impl<'c, 'a> Runs<'c, 'a, '_> {
    /// The number of elements in one run.
    fn length(&self) -> Extent<'c, 'a> {
        self.dims[self.axis]
    }

    fn rank(&self) -> usize {
        self.dims.len()
    }
}

/// Lower `sort`, a `TensorSort`, into one tensor per result: the sorted elements, the
/// order, or `.topk`'s values and then its indices.
pub(crate) fn build_sort<'c, 'a>(
    context: &'c Context,
    location: Location<'c>,
    block: &'a Block<'c>,
    sort: &HirExpr,
    scope: &[(String, Value<'c, 'a>)],
    lowering: &Lowering<'c, 'a>,
) -> Result<Option<Vec<Value<'c, 'a>>>, MlirError> {
    let HirExprKind::TensorSort {
        receiver,
        kind,
        axis,
        descending,
    } = &sort.kind
    else {
        return Ok(None);
    };
    let Some((element, shape)) = tensor_parts(&receiver.ty) else {
        return Ok(None);
    };
    let Some(compared) = Element::computed(element) else {
        return Ok(None);
    };
    if *axis >= shape.len() {
        return Ok(None);
    }
    let outputs: Vec<(Picked, &HirType)> = match (kind, &sort.ty) {
        (HirSortKind::Values, ty) => vec![(Picked::Element, ty)],
        (HirSortKind::Indices, ty) => vec![(Picked::Position, ty)],
        (HirSortKind::TopK(_), HirType::Tuple(parts)) => match parts.as_slice() {
            [values, indices] => vec![(Picked::Element, values), (Picked::Position, indices)],
            _ => return Ok(None),
        },
        (HirSortKind::TopK(_), _) => return Ok(None),
    };
    let Some(source) = build_expression(context, location, block, receiver, scope, lowering)?
    else {
        return Ok(None);
    };

    let dims = source_extents(context, location, block, source, shape)?;
    let runs = Runs {
        source,
        element: map_type(context, element)?,
        kind: compared,
        descending: *descending,
        shape,
        dims,
        axis: *axis,
    };
    // The radix sort's loops need every extent static. The merge sort takes any run.
    let static_extents = shape.iter().copied().collect::<Option<Vec<usize>>>();
    let order = match (compared, static_extents) {
        (Element::Signed(bits) | Element::Unsigned(bits), Some(extents))
            if lowering.side == Side::Host && extents[*axis] >= RADIX_BUCKETS =>
        {
            Some(radix::radix_order(
                context, location, block, &runs, &extents, bits,
            )?)
        }
        _ => merge_order(context, location, block, &runs)?,
    };
    let mut results = Vec::with_capacity(outputs.len());
    for (picked, ty) in outputs {
        let Some(result) = gather(context, location, block, &runs, order, ty, picked)? else {
            return Ok(None);
        };
        results.push(result);
    }
    Ok(Some(results))
}

/// A tensor of run positions shaped like the source.
fn order_type<'c>(context: &'c Context, shape: &[Option<usize>]) -> Type<'c> {
    shape_type(Type::index(context), shape)
}

/// The run-time extents of `source`, of shape `shape`: a static one as it is, a dynamic one
/// read off `source` itself.
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

/// One output of the sort, of type `result`: at each of its positions, what `picked` takes
/// from the run position `order` holds there, or from that position itself when `order` is
/// `None` (runs of one element).
fn gather<'c, 'a>(
    context: &'c Context,
    location: Location<'c>,
    block: &'a Block<'c>,
    runs: &Runs<'c, 'a, '_>,
    order: Option<Value<'c, 'a>>,
    result: &HirType,
    picked: Picked,
) -> Result<Option<Value<'c, 'a>>, MlirError> {
    let Some((element, shape)) = tensor_parts(result) else {
        return Ok(None);
    };
    let rank = shape.len();
    let tensor_type = map_type(context, result)?;
    let element_type = map_type(context, element)?;
    // A sort keeps every axis of its source, so a dynamic axis of the result is the source's.
    let extents: Vec<Extent> = shape
        .iter()
        .zip(&runs.dims)
        .map(|(extent, dim)| extent.map_or(*dim, Extent::Static))
        .collect();
    let empty = append(
        block,
        empty_tensor(location, tensor_type, &dynamic_sizes(&extents))?,
    )?;

    let body = Block::new(&[(element_type, location)]);
    let at = position(context, location, &body, rank)?;
    let held = match order {
        Some(order) => read(
            location,
            &body,
            order,
            (&at, runs.axis, at[runs.axis]),
            Type::index(context),
        )?,
        None => at[runs.axis],
    };
    let value = match picked {
        Picked::Element => read(
            location,
            &body,
            runs.source,
            (&at, runs.axis, held),
            runs.element,
        )?,
        Picked::Position => append(&body, arith::index_cast(held, element_type, location))?,
    };
    yield_value(&body, location, value)?;
    Ok(Some(all_parallel(
        context,
        location,
        block,
        empty,
        tensor_type,
        rank,
        body,
    )?))
}

/// The stable order of every run by merge sort, or `None` for runs of one element.
fn merge_order<'c, 'a>(
    context: &'c Context,
    location: Location<'c>,
    block: &'a Block<'c>,
    runs: &Runs<'c, 'a, '_>,
) -> Result<Option<Value<'c, 'a>>, MlirError> {
    let order_type = order_type(context, runs.shape);
    let sizes = dynamic_sizes(&runs.dims);
    let index = Type::index(context);
    match runs.length() {
        Extent::Static(length) => {
            let mut order = None;
            let mut width = 1;
            while width < length {
                let empty = append(block, empty_tensor(location, order_type, &sizes)?)?;
                let width_value = index_constant(context, location, block, width)?;
                let steps = index_constant(
                    context,
                    location,
                    block,
                    (usize::BITS - width.leading_zeros()) as usize,
                )?;
                let body = merge_pass(context, location, runs, order, width_value, steps)?;
                order = Some(all_parallel(
                    context,
                    location,
                    block,
                    empty,
                    order_type,
                    runs.rank(),
                    body,
                )?);
                width *= 2;
            }
            Ok(order)
        }
        Extent::Dynamic(length) => {
            // A run of `n` takes `ceil(log2(n))` doublings, which is the bit length of `n - 1`
            // (none for a run of one, and a run of zero has none to sort). Counted at run time,
            // so the passes are loops over two buffers, `first` and `second`. Each loop iteration
            // runs two passes with the buffers named statically (`first` to `second`, then back),
            // the second guarded by `scf.if`, and a final copy brings the answer back to `first`
            // when the count is odd. No buffer is carried through a loop: bufferization would copy
            // the swapped ones on each iteration.
            let one = index_constant(context, location, block, 1)?;
            let shorter = append(block, arith::subi(length, one, location))?;
            let word = IntegerType::new(context, 64).into();
            let shorter = append(block, arith::index_cast(shorter, word, location))?;
            let leading = append(
                block,
                OperationBuilder::new("math.ctlz", location)
                    .add_operands(&[shorter])
                    .add_results(&[word])
                    .build()?,
            )?;
            let sixty_four = append(
                block,
                arith::constant(context, IntegerAttribute::new(word, 64).into(), location),
            )?;
            let bits = append(block, arith::subi(sixty_four, leading, location))?;
            let passes = append(block, arith::index_cast(bits, index, location))?;
            let zero = index_constant(context, location, block, 0)?;
            let two = index_constant(context, location, block, 2)?;

            let sizes = dynamic_sizes(&runs.dims);
            let buffer_type = MemRefType::new(index, &buffer_shape(runs.shape), None, None);
            let first = append(
                block,
                memref::alloc(context, buffer_type, &sizes, &[], None, location),
            )?;
            let second = append(
                block,
                memref::alloc(context, buffer_type, &sizes, &[], None, location),
            )?;
            identity_into(context, location, block, runs, first)?;

            // Pass `k` doubles blocks of width `2^k` and is the bit length `k + 1` steps of the search.
            // The loop index `p` runs passes `2p` and `2p + 1`, so pass `2p` is always there.
            let pairs = {
                let plus_one = append(block, arith::addi(passes, one, location))?;
                append(block, arith::shrui(plus_one, one, location))?
            };
            let body = Block::new(&[(index, location)]);
            let p: Value = body.argument(0)?.into();
            let one_in_body = index_constant(context, location, &body, 1)?;
            let even = append(&body, arith::muli(p, two, location))?;
            let even_width = append(&body, arith::shli(one_in_body, even, location))?;
            let even_steps = append(&body, arith::addi(even, one_in_body, location))?;
            let merged = merge_pass(context, location, runs, Some(first), even_width, even_steps)?;
            buffer_generic(context, location, &body, second, runs.rank(), merged)?;

            let odd = append(&body, arith::addi(even, one_in_body, location))?;
            let more = append(
                &body,
                arith::cmpi(context, CmpiPredicate::Ult, odd, passes, location),
            )?;
            let odd_width = append(&body, arith::shli(one_in_body, odd, location))?;
            let odd_steps = append(&body, arith::addi(odd, one_in_body, location))?;
            let guarded = Block::new(&[]);
            let merged = merge_pass(context, location, runs, Some(second), odd_width, odd_steps)?;
            buffer_generic(context, location, &guarded, first, runs.rank(), merged)?;
            scf_yield(&guarded, location, &[])?;
            let region = Region::new();
            region.append_block(guarded);
            body.append_operation(scf::r#if(more, &[], region, Region::new(), location));
            scf_yield(&body, location, &[])?;
            for_loop(location, block, (zero, pairs, one), body)?;

            // An odd count leaves the answer in `second`, which is copied back to `first`.
            let remainder = append(block, arith::andi(passes, one, location))?;
            let odd_count = append(
                block,
                arith::cmpi(context, CmpiPredicate::Ne, remainder, zero, location),
            )?;
            let copy = Block::new(&[]);
            buffer_generic(
                context,
                location,
                &copy,
                first,
                runs.rank(),
                copy_from(context, location, runs, second)?,
            )?;
            scf_yield(&copy, location, &[])?;
            let region = Region::new();
            region.append_block(copy);
            block.append_operation(scf::r#if(odd_count, &[], region, Region::new(), location));
            Ok(Some(to_tensor(
                context, location, block, first, order_type,
            )?))
        }
    }
}

/// The body that copies `source`, a buffer of run positions, position for position: each point
/// yields the element `source` holds there.
fn copy_from<'c>(
    context: &'c Context,
    location: Location<'c>,
    runs: &Runs<'c, '_, '_>,
    source: Value<'c, '_>,
) -> Result<Block<'c>, MlirError> {
    let body = Block::new(&[(Type::index(context), location)]);
    let at = position(context, location, &body, runs.rank())?;
    let value = append(&body, memref::load(source, &at, location))?;
    yield_value(&body, location, value)?;
    Ok(body)
}

/// A `scf.for` over `bounds` (lower, upper, step) running `body`, which takes the induction
/// variable and carries no values.
fn for_loop<'c>(
    location: Location<'c>,
    block: &Block<'c>,
    (lower, upper, step): (Value<'c, '_>, Value<'c, '_>, Value<'c, '_>),
    body: Block<'c>,
) -> Result<(), MlirError> {
    let region = Region::new();
    region.append_block(body);
    block.append_operation(
        OperationBuilder::new("scf.for", location)
            .add_operands(&[lower, upper, step])
            .add_regions([region])
            .build()?,
    );
    Ok(())
}

/// A buffer of run positions shaped like the source: a dynamic axis is MLIR's dynamic extent.
fn buffer_shape(shape: &[Option<usize>]) -> Vec<i64> {
    shape
        .iter()
        .map(|extent| extent.map_or(dynamic_extent() as i64, |extent| extent as i64))
        .collect()
}

/// The identity order written into `buffer`: every position holds its own run position, which
/// is the order before any pass merges anything.
fn identity_into<'c>(
    context: &'c Context,
    location: Location<'c>,
    block: &Block<'c>,
    runs: &Runs<'c, '_, '_>,
    buffer: Value<'c, '_>,
) -> Result<(), MlirError> {
    let body = Block::new(&[(Type::index(context), location)]);
    let at = position(context, location, &body, runs.rank())?;
    yield_value(&body, location, at[runs.axis])?;
    buffer_generic(context, location, block, buffer, runs.rank(), body)
}

/// One all-parallel `linalg.generic` writing `buffer` in place, its body's yield stored at each
/// position. Unlike [`all_parallel`] it returns nothing: a buffer is written, not produced.
fn buffer_generic<'c>(
    context: &'c Context,
    location: Location<'c>,
    block: &Block<'c>,
    buffer: Value<'c, '_>,
    rank: usize,
    body: Block<'c>,
) -> Result<(), MlirError> {
    let region = Region::new();
    region.append_block(body);
    let own: OperandAxes = Some((0..rank).map(Some).collect());
    block.append_operation(
        OperationBuilder::new("linalg.generic", location)
            .add_operands(&[buffer])
            .add_attributes(&[
                (
                    Identifier::new(context, "indexing_maps"),
                    indexing_maps(context, rank, &[&own])?,
                ),
                (
                    Identifier::new(context, "iterator_types"),
                    iterator_types(context, rank, 0)?,
                ),
                (
                    Identifier::new(context, "operandSegmentSizes"),
                    DenseI32ArrayAttribute::new(context, &[0, 1]).into(),
                ),
            ])
            .add_regions([region])
            .build()?,
    );
    Ok(())
}

/// The tensor a finished order buffer is handed on as.
fn to_tensor<'c, 'a>(
    context: &'c Context,
    location: Location<'c>,
    block: &'a Block<'c>,
    buffer: Value<'c, '_>,
    tensor: Type<'c>,
) -> Result<Value<'c, 'a>, MlirError> {
    append(
        block,
        OperationBuilder::new("bufferization.to_tensor", location)
            .add_operands(&[buffer])
            .add_attributes(&[(
                Identifier::new(context, "restrict"),
                Attribute::unit(context),
            )])
            .add_results(&[tensor])
            .build()?,
    )
}

/// One merge pass's body: the run position that this point holds once the sorted blocks of
/// `width` around it are merged in pairs. `steps` is the bit length of `width`, the count of
/// halvings its binary search takes.
fn merge_pass<'c, 'a>(
    context: &'c Context,
    location: Location<'c>,
    runs: &Runs<'c, 'a, '_>,
    order: Option<Value<'c, '_>>,
    width: Value<'c, '_>,
    steps: Value<'c, '_>,
) -> Result<Block<'c>, MlirError> {
    let index = Type::index(context);
    let body = Block::new(&[(index, location)]);
    {
        let length = runs.length().index(context, location, &body)?;
        let at = position(context, location, &body, runs.rank())?;
        let point = at[runs.axis];
        let twice = append(&body, arith::addi(width, width, location))?;
        let offset = append(&body, arith::remui(point, twice, location))?;
        let start = append(&body, arith::subi(point, offset, location))?;
        let rest = append(&body, arith::subi(length, start, location))?;
        let left = append(&body, arith::minui(width, rest, location))?;
        let past_left = append(&body, arith::subi(rest, left, location))?;
        let pass = Pass {
            context,
            location,
            runs,
            order,
            at: &at,
            start,
            right_start: append(&body, arith::addi(start, width, location))?,
            offset,
            left,
            right: append(&body, arith::minui(width, past_left, location))?,
        };
        let taken = pass.search(&body, steps)?;
        let (from_left, left_held) = pass.left_slot(&body, taken)?;
        let (from_right, right_held) = pass.right_slot(&body, taken)?;
        let right_first = pass.right_first(&body, right_held, left_held)?;
        // The left element is next unless there is none, or the right one strictly precedes.
        let right_wins = append(&body, arith::andi(from_right, right_first, location))?;
        let left_wins = not(context, location, &body, right_wins)?;
        let left_wins = append(&body, arith::andi(from_left, left_wins, location))?;
        let next = append(
            &body,
            arith::select(left_wins, left_held, right_held, location),
        )?;
        yield_value(&body, location, next)?;
    }
    Ok(body)
}

/// One merge pass at one point: the block pair it merges, and the order it merges from.
struct Pass<'c, 'v, 'r> {
    context: &'c Context,
    location: Location<'c>,
    runs: &'r Runs<'c, 'v, 'r>,
    order: Option<Value<'c, 'v>>,
    at: &'r [Value<'c, 'v>],
    start: Value<'c, 'v>,
    right_start: Value<'c, 'v>,
    /// The point's place in the merged pair: how many of the pair's elements precede it.
    offset: Value<'c, 'v>,
    left: Value<'c, 'v>,
    right: Value<'c, 'v>,
}

impl<'c> Pass<'c, '_, '_> {
    /// How many left elements precede the point: the most `taken` such that the first
    /// `taken` left elements all do, which holds exactly while the right element at
    /// `offset - taken`, if there is one, does not strictly precede left element
    /// `taken - 1`. It holds for 0 and cannot hold past `min(offset, left)`, a range of at
    /// most `width + 1` that `steps`, the bit length of `width`, halvings close.
    fn search<'b>(
        &self,
        body: &'b Block<'c>,
        steps: Value<'c, '_>,
    ) -> Result<Value<'c, 'b>, MlirError> {
        let location = self.location;
        let index = Type::index(self.context);
        let one = index_constant(self.context, location, body, 1)?;
        let reach = append(body, arith::minui(self.offset, self.left, location))?;
        let low = index_constant(self.context, location, body, 0)?;
        let high = append(body, arith::addi(reach, one, location))?;

        let step = Block::new(&[(index, location); 3]);
        let low_value: Value = step.argument(1)?.into();
        let high_value: Value = step.argument(2)?.into();
        {
            let one = index_constant(self.context, location, &step, 1)?;
            let span = append(&step, arith::subi(high_value, low_value, location))?;
            let open = append(
                &step,
                arith::cmpi(self.context, CmpiPredicate::Ugt, span, one, location),
            )?;
            let sum = append(&step, arith::addi(low_value, high_value, location))?;
            let middle = append(&step, arith::shrui(sum, one, location))?;
            let before = append(&step, arith::subi(middle, one, location))?;
            let (_, left_held) = self.left_slot(&step, before)?;
            let (has_right, right_held) = self.right_slot(&step, middle)?;
            let right_first = self.right_first(&step, right_held, left_held)?;
            let blocked = append(&step, arith::andi(has_right, right_first, location))?;
            let shrink = append(&step, arith::andi(open, blocked, location))?;
            let fits = not(self.context, location, &step, blocked)?;
            let grow = append(&step, arith::andi(open, fits, location))?;
            let low_next = append(&step, arith::select(grow, middle, low_value, location))?;
            let high_next = append(&step, arith::select(shrink, middle, high_value, location))?;
            scf_yield(&step, location, &[low_next, high_next])?;
        }
        Ok(repeat(location, body, (low, steps, one), &[low, high], step)?[0])
    }

    /// Whether the left block has element `count`, and the run position it holds there
    /// (at the block's first slot, always in range, when it has none).
    fn left_slot<'b>(
        &self,
        block: &'b Block<'c>,
        count: Value<'c, '_>,
    ) -> Result<(Value<'c, 'b>, Value<'c, 'b>), MlirError> {
        let location = self.location;
        let inside = append(
            block,
            arith::cmpi(self.context, CmpiPredicate::Ult, count, self.left, location),
        )?;
        let slot = append(block, arith::addi(self.start, count, location))?;
        let slot = append(block, arith::select(inside, slot, self.start, location))?;
        Ok((inside, self.held(block, slot)?))
    }

    /// Whether the right block has the element that follows `count` left ones at the
    /// point, and the run position it holds there (the left block's first when it has none).
    fn right_slot<'b>(
        &self,
        block: &'b Block<'c>,
        count: Value<'c, '_>,
    ) -> Result<(Value<'c, 'b>, Value<'c, 'b>), MlirError> {
        let location = self.location;
        // Past `offset` this wraps to a huge index, which is past the block too.
        let taken = append(block, arith::subi(self.offset, count, location))?;
        let inside = append(
            block,
            arith::cmpi(
                self.context,
                CmpiPredicate::Ult,
                taken,
                self.right,
                location,
            ),
        )?;
        let slot = append(block, arith::addi(self.right_start, taken, location))?;
        let slot = append(block, arith::select(inside, slot, self.start, location))?;
        Ok((inside, self.held(block, slot)?))
    }

    /// The run position the pass's input order holds at run slot `slot`.
    fn held<'b>(
        &self,
        block: &'b Block<'c>,
        slot: Value<'c, 'b>,
    ) -> Result<Value<'c, 'b>, MlirError> {
        let Some(order) = self.order else {
            return Ok(slot);
        };
        read(
            self.location,
            block,
            order,
            (self.at, self.runs.axis, slot),
            Type::index(self.context),
        )
    }

    /// Whether the element at run position `right` strictly precedes the one at `left`.
    fn right_first<'b>(
        &self,
        block: &'b Block<'c>,
        right: Value<'c, '_>,
        left: Value<'c, '_>,
    ) -> Result<Value<'c, 'b>, MlirError> {
        let runs = self.runs;
        let element = |position| {
            read(
                self.location,
                block,
                runs.source,
                (self.at, runs.axis, position),
                runs.element,
            )
        };
        let (right, left) = (element(right)?, element(left)?);
        precedes(
            self.context,
            self.location,
            block,
            (right, left),
            runs.kind,
            runs.descending,
        )
    }
}

/// The current point of a `linalg.generic` of `rank` axes, one `linalg.index` per axis.
fn position<'c, 'a>(
    context: &'c Context,
    location: Location<'c>,
    body: &'a Block<'c>,
    rank: usize,
) -> Result<Vec<Value<'c, 'a>>, MlirError> {
    (0..rank)
        .map(|axis| append(body, linalg_index(context, location, axis)?))
        .collect()
}

/// `tensor` read at `at` with its sorted `axis` at `along`.
fn read<'c, 'a>(
    location: Location<'c>,
    block: &'a Block<'c>,
    tensor: Value<'c, '_>,
    (at, axis, along): (&[Value<'c, '_>], usize, Value<'c, '_>),
    element: Type<'c>,
) -> Result<Value<'c, 'a>, MlirError> {
    let mut indices = at.to_vec();
    indices[axis] = along;
    extract(location, block, tensor, &indices, element)
}

/// The element of `tensor` at `indices`: a `tensor.extract` of a tensor value, or a
/// `memref.load` of a buffer a dynamic run's passes write.
fn extract<'c, 'a>(
    location: Location<'c>,
    block: &'a Block<'c>,
    tensor: Value<'c, '_>,
    indices: &[Value<'c, '_>],
    element: Type<'c>,
) -> Result<Value<'c, 'a>, MlirError> {
    if tensor.r#type().is_mem_ref() {
        return append(block, memref::load(tensor, indices, location));
    }
    append(
        block,
        OperationBuilder::new("tensor.extract", location)
            .add_operands(&[tensor])
            .add_operands(indices)
            .add_results(&[element])
            .build()?,
    )
}

/// One all-parallel `linalg.generic` over `rank` axes writing `destination`, whose `body`
/// reads everything it needs itself.
fn all_parallel<'c, 'a>(
    context: &'c Context,
    location: Location<'c>,
    block: &'a Block<'c>,
    destination: Value<'c, '_>,
    tensor_type: Type<'c>,
    rank: usize,
    body: Block<'c>,
) -> Result<Value<'c, 'a>, MlirError> {
    let own = Some((0..rank).map(Some).collect());
    apply(
        context,
        location,
        block,
        Generic {
            inputs: &[],
            destination,
            indexing_maps: indexing_maps(context, rank, &[&own])?,
            iterators: iterator_types(context, rank, 0)?,
        },
        tensor_type,
        body,
    )
}

/// An `scf.for` over `bounds` (lower, upper, step) carrying `initial` through `body`, which
/// takes the induction variable then the carried values and ends in its own `scf.yield`.
fn repeat<'c, 'a>(
    location: Location<'c>,
    block: &'a Block<'c>,
    (lower, upper, step): (Value<'c, '_>, Value<'c, '_>, Value<'c, '_>),
    initial: &[Value<'c, '_>],
    body: Block<'c>,
) -> Result<Vec<Value<'c, 'a>>, MlirError> {
    let types: Vec<Type> = initial.iter().map(|value| value.r#type()).collect();
    let region = Region::new();
    region.append_block(body);
    let operation = block.append_operation(
        OperationBuilder::new("scf.for", location)
            .add_operands(&[lower, upper, step])
            .add_operands(initial)
            .add_results(&types)
            .add_regions([region])
            .build()?,
    );
    (0..initial.len())
        .map(|result| Ok(operation.result(result)?.into()))
        .collect()
}

/// `value` negated: an `i1` xor true.
fn not<'c, 'a>(
    context: &'c Context,
    location: Location<'c>,
    block: &'a Block<'c>,
    value: Value<'c, '_>,
) -> Result<Value<'c, 'a>, MlirError> {
    let truth = IntegerType::new(context, 1).into();
    let every = append(
        block,
        arith::constant(context, IntegerAttribute::new(truth, -1).into(), location),
    )?;
    append(block, arith::xori(value, every, location))
}

fn scf_yield<'c>(
    block: &Block<'c>,
    location: Location<'c>,
    values: &[Value<'c, '_>],
) -> Result<(), MlirError> {
    block.append_operation(
        OperationBuilder::new("scf.yield", location)
            .add_operands(values)
            .build()?,
    );
    Ok(())
}

/// Whether `a` strictly precedes `b` under the LLVM backend's sorting comparator: in
/// order by `<` (by `>` when `descending`), with a NaN sorting last in either direction,
/// so it precedes nothing and everything else precedes it. An integer compares by its
/// signedness and has no NaN.
pub(crate) fn precedes<'c, 'a>(
    context: &'c Context,
    location: Location<'c>,
    block: &'a Block<'c>,
    (a, b): (Value<'c, '_>, Value<'c, '_>),
    kind: Element,
    descending: bool,
) -> Result<Value<'c, 'a>, MlirError> {
    let integer = match (kind, descending) {
        (Element::Float | Element::Half, _) => None,
        (Element::Signed(_), true) => Some(CmpiPredicate::Sgt),
        (Element::Signed(_), false) => Some(CmpiPredicate::Slt),
        (Element::Unsigned(_), true) => Some(CmpiPredicate::Ugt),
        (Element::Unsigned(_), false) => Some(CmpiPredicate::Ult),
    };
    if let Some(predicate) = integer {
        return append(block, arith::cmpi(context, predicate, a, b, location));
    }
    let predicate = if descending {
        CmpfPredicate::Ogt
    } else {
        CmpfPredicate::Olt
    };
    let ordered = append(block, arith::cmpf(context, predicate, a, b, location))?;
    let b_nan = append(
        block,
        arith::cmpf(context, CmpfPredicate::Uno, b, b, location),
    )?;
    let a_real = append(
        block,
        arith::cmpf(context, CmpfPredicate::Ord, a, a, location),
    )?;
    let before = append(block, arith::ori(ordered, b_nan, location))?;
    append(block, arith::andi(a_real, before, location))
}

#[cfg(test)]
mod tests {
    use crate::{
        GpuTarget,
        bridge::tests::{
            assert_host_only_on_gpu, assert_launches_on_gpu, assert_lowered, mlir_of, operand,
            outlined, tensor_of,
        },
        context::new_context,
        gpu::math_functions,
        guards::Side,
        lower::build_linkable_module,
        lower_for_gpu, lower_for_link,
    };

    use neuro_hir::{HirExpr, HirExprKind, HirProgram, HirSortKind, HirType};
    use shared_types::Span;

    /// A program with a transfer, so lowering outlines its operations. The harness binds
    /// no named arguments, so `k` and the axis are positional.
    fn program(declaration: &str, body: &str) -> HirProgram {
        let source = format!(
            "enum Device {{\n    CPU,\n    GPU(i32)\n}}\n\nfunc main() -> i32 {{\n    {declaration}\n    val g = m.clone().to(Device::GPU(0))\n{body}\n    return 0\n}}\n"
        );
        let ast = syntax_parsing::parse(&source).expect("the program parses");
        hir_lowering::lower_program(&ast).expect("the program lowers to HIR")
    }

    fn floats(body: &str) -> HirProgram {
        program("val m: Tensor<f64, [37, 19]> = Tensor::ones()", body)
    }

    fn nvidia() -> GpuTarget {
        GpuTarget::Nvidia {
            chip: "sm_80".to_string(),
        }
    }

    /// The host module's text before any pass runs.
    fn host_module(program: &HirProgram) -> String {
        let context = new_context();
        let (module, _) = build_linkable_module(
            &context,
            program,
            (crate::Overflow::Checked, Side::Host),
            &|_| true,
        )
        .expect("the host module builds");
        module.as_operation().to_string()
    }

    #[test]
    fn a_device_sort_launches_a_merge_pass_per_doubling_then_a_gather_per_output() {
        // 19 long: widths 1, 2, 4, 8, 16. 37 long: one more, 32.
        for (body, launches) in [
            ("    val s = g.sort()", 5 + 1),
            ("    val s = g.argsort(0)", 6 + 1),
            ("    val (v, i) = g.topk(3)", 5 + 2),
        ] {
            let bodies = lower_for_gpu(&floats(body), &nvidia(), crate::Overflow::Checked)
                .expect("a selection lowers");
            assert_eq!(bodies.functions.len(), 1, "`{body}`");
            let ir = &bodies.llvm_ir;
            assert_eq!(
                ir.matches("call void @mgpuLaunchKernel").count(),
                launches,
                "`{body}`:\n{ir}"
            );
        }
    }

    #[test]
    fn a_device_integer_sort_merges_however_long_its_runs() {
        let program = program(
            "val m: Tensor<i32, [3, 300]> = Tensor::ones()",
            "    val s = g.sort()",
        );
        let bodies = lower_for_gpu(&program, &nvidia(), crate::Overflow::Checked).expect("lowers");
        // 300 long: widths 1 to 256, nine passes.
        assert_eq!(
            bodies
                .llvm_ir
                .matches("call void @mgpuLaunchKernel")
                .count(),
            9 + 1
        );
    }

    #[test]
    fn a_long_host_integer_run_sorts_by_radix_and_a_short_one_by_merging() {
        let long = host_module(&program(
            "val m: Tensor<i16, [3, 300]> = Tensor::ones()",
            "    val s = g.sort()",
        ));
        // Two byte passes over each of the three runs, and no merge pass.
        assert!(long.contains("memref<256xindex>"), "{long}");
        assert_eq!(long.matches("linalg.generic").count(), 1, "{long}");

        let short = host_module(&program(
            "val m: Tensor<i16, [300, 3]> = Tensor::ones()",
            "    val s = g.sort()",
        ));
        assert!(!short.contains("memref<256xindex>"), "{short}");
        assert_eq!(short.matches("linalg.generic").count(), 2 + 1, "{short}");

        let float = host_module(&program(
            "val m: Tensor<f32, [300]> = Tensor::ones()",
            "    val s = g.sort()",
        ));
        assert!(!float.contains("memref<256xindex>"), "{float}");
    }

    #[test]
    fn topk_writes_its_two_results_through_two_out_params() {
        let bodies = lower_for_gpu(
            &floats("    val (v, i) = g.topk(3)"),
            &nvidia(),
            crate::Overflow::Checked,
        )
        .expect("lowers");
        let (_, symbol) = &bodies.functions[0];
        let ir = &bodies.llvm_ir;
        let start = ir
            .find(&format!("define void @{symbol}("))
            .unwrap_or_else(|| panic!("`{symbol}` is defined:\n{ir}"));
        let signature = &ir[start..start + ir[start..].find('{').unwrap_or(0)];
        // Three rank-2 descriptors of seven words each: the receiver, the values, the
        // indices.
        assert_eq!(signature.matches("ptr").count(), 6, "{signature}");
        assert_eq!(signature.matches("i64").count(), 15, "{signature}");
    }

    #[test]
    fn the_cpu_path_computes_every_selection() {
        let program = floats("    val s = g.sort()\n    val (v, i) = g.topk(3)");
        let bodies =
            lower_for_link(&program, crate::Overflow::Checked).expect("the CPU path lowers");
        assert_eq!(bodies.functions.len(), 2, "{:?}", bodies.functions);
    }

    #[test]
    fn a_long_host_integer_sort_links() {
        let program = program(
            "val m: Tensor<u64, [2, 3, 400]> = Tensor::ones()",
            "    val s = g.argsort(2)\n    val (v, i) = g.topk(5)\n    val d = g.sort(1)",
        );
        let bodies =
            lower_for_link(&program, crate::Overflow::Checked).expect("the CPU path lowers");
        assert_eq!(bodies.functions.len(), 3, "{:?}", bodies.functions);
    }

    /// `func f(a: T) -> R { return a.sort(...) }` over a tensor of `element` and `shape`.
    fn selection(
        element: HirType,
        shape: Vec<Option<usize>>,
        kind: HirSortKind,
        axis: usize,
    ) -> HirProgram {
        let span = Span::new(0, 1);
        let source = tensor_of(element.clone(), shape.clone());
        let result = match kind {
            HirSortKind::Indices => tensor_of(HirType::I32, shape),
            _ => tensor_of(element, shape),
        };
        let sort = HirExpr::new(
            HirExprKind::TensorSort {
                receiver: Box::new(operand("a", &source)),
                kind,
                axis,
                descending: false,
            },
            result.clone(),
            span,
        );
        outlined(&[("a", source)], sort, result)
    }

    /// A sort over a dynamic run counts its merge passes at run time: `ceil(log2(n))` of them,
    /// as the bit length of `n - 1`, in an `scf.for` whose width is `2^pass`.
    #[test]
    fn a_dynamic_sort_counts_its_passes_at_run_time() {
        let program = selection(HirType::F32, vec![None], HirSortKind::Values, 0);
        let mlir = mlir_of(&program);
        assert!(
            mlir.contains("math.ctlz") && mlir.contains("scf.for"),
            "{mlir}"
        );
        // No buffer or tensor is carried through a loop: bufferization would copy the swapped
        // buffers on every iteration, and on a device those copies run on the host. The search's
        // indices may still be carried.
        assert!(
            !mlir.contains("-> (memref") && !mlir.contains("-> (tensor"),
            "{mlir}"
        );
        assert_lowered(&program);
        assert_host_only_on_gpu(&program);
    }

    /// A run of static length keeps the static passes even when another axis is dynamic.
    #[test]
    fn a_dynamic_axis_beside_a_static_run_keeps_static_passes() {
        let program = selection(HirType::F32, vec![None, Some(4)], HirSortKind::Indices, 1);
        let mlir = mlir_of(&program);
        assert!(!mlir.contains("math.ctlz"), "{mlir}");
        assert!(mlir.contains("tensor<?x4xi32>"), "{mlir}");
        assert_lowered(&program);
        assert_host_only_on_gpu(&program);
    }

    /// `.topk` over a dynamic axis gathers its static `k`, which the result takes.
    #[test]
    fn a_dynamic_topk_gathers_its_k_positions() {
        let span = Span::new(0, 1);
        let source = tensor_of(HirType::F32, vec![None]);
        let values = tensor_of(HirType::F32, vec![Some(2)]);
        let indices = tensor_of(HirType::I32, vec![Some(2)]);
        let pair = HirType::Tuple(vec![values, indices]);
        let top = HirExpr::new(
            HirExprKind::TensorSort {
                receiver: Box::new(operand("a", &source)),
                kind: HirSortKind::TopK(2),
                axis: 0,
                descending: true,
            },
            pair.clone(),
            span,
        );
        let program = outlined(&[("a", source)], top, pair);
        let mlir = mlir_of(&program);
        assert!(mlir.contains("tensor<2xi32>"), "{mlir}");
        assert_lowered(&program);
        assert_launches_on_gpu(&program);
    }

    /// A dynamic run counts its passes with `math.ctlz`, an LLVM intrinsic on every device,
    /// so the body needs no device math library and stays on a GPU without the CUDA toolkit.
    #[test]
    fn regression_a_dynamic_sort_needs_no_device_math_library() {
        let source = tensor_of(HirType::F32, vec![None]);
        let sorted = HirExpr::new(
            HirExprKind::TensorSort {
                receiver: Box::new(operand("a", &source)),
                kind: HirSortKind::Values,
                axis: 0,
                descending: false,
            },
            source.clone(),
            Span::new(0, 1),
        );
        let program = outlined(&[("a", source.clone())], sorted, source);
        let context = new_context();
        let (module, (functions, _)) = build_linkable_module(
            &context,
            &program,
            (crate::Overflow::Checked, Side::Device),
            &|_| true,
        )
        .expect("the body lowers");
        assert!(module.as_operation().to_string().contains("math.ctlz"));
        assert!(math_functions(&module, &functions).is_empty());
    }
}
