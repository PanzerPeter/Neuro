// The host's sort: one `scf.for` over the runs, each sorted in place in `memref` buffers by
// a sequential bottom-up merge, or by LSD radix (`radix.rs`) when its elements are integers
// and it is at least `RADIX_BUCKETS` long. A run length known only at run time makes that
// choice an `scf.if`.
//
// A merge pass of width `w` walks each pair of sorted blocks of `w` positions in step, so a
// pass is O(n) and the sort O(n log n) comparisons. Each pass reads and writes the keys beside
// the positions, in two buffers each that the passes swap, so no pass reads the source out of
// order. A right key goes first only if it strictly precedes the left one, which keeps equal
// elements in their order. Every loop works on buffers and the order is handed over as a
// tensor only at the end: one-shot bufferization's analysis of tensors carried through loops
// grows exponentially with the passes.

use super::{
    RADIX_BUCKETS, Runs, buffer_shape, order_type, precedes,
    radix::{Radix, radix_counts, radix_run},
    read, repeat, scf_yield, to_tensor,
};
use crate::{
    errors::MlirError,
    tensor_arithmetic::{Extent, dynamic_sizes},
    tensor_reduce::{append, index_constant},
};

use melior::{
    Context,
    dialect::{
        arith::{self, CmpiPredicate},
        memref, scf,
    },
    ir::{
        Block, BlockLike, Location, Region, RegionLike, Type, Value,
        attribute::IntegerAttribute,
        operation::OperationBuilder,
        r#type::{IntegerType, MemRefType},
    },
};

/// The stable order of every run on the host, or `None` for runs of one element. `bits` is
/// the key width of an integer element, which may take the radix sort.
pub(super) fn host_order<'c, 'a>(
    context: &'c Context,
    location: Location<'c>,
    block: &'a Block<'c>,
    runs: &Runs<'c, 'a, '_>,
    bits: Option<u32>,
) -> Result<Option<Value<'c, 'a>>, MlirError> {
    let radix_taken = match (runs.length(), bits) {
        (Extent::Static(length), _) if length <= 1 => return Ok(None),
        (_, None) => Some(false),
        (Extent::Static(length), Some(_)) => Some(length >= RADIX_BUCKETS),
        (Extent::Dynamic(_), Some(_)) => None,
    };
    let index = Type::index(context);
    let sizes = dynamic_sizes(&runs.dims);
    let shape = buffer_shape(runs.shape);
    let positions_type = MemRefType::new(index, &shape, None, None);
    let allocate = |ty: MemRefType<'c>| {
        append(
            block,
            memref::alloc(context, ty, &sizes, &[], None, location),
        )
    };
    let out = allocate(positions_type)?;
    let scratch = allocate(positions_type)?;
    let extents = runs
        .dims
        .iter()
        .map(|dim| dim.index(context, location, block))
        .collect::<Result<Vec<_>, _>>()?;
    let length = extents[runs.axis];
    let merge = match radix_taken {
        Some(true) => None,
        _ => Some(Merge::prepare(
            context, location, block, runs, length, out, scratch,
        )?),
    };
    let counts = match (radix_taken, bits) {
        (Some(false), _) | (_, None) => None,
        (_, Some(bits)) => Some((radix_counts(context, location, block)?, bits)),
    };
    let mut run_count = index_constant(context, location, block, 1)?;
    for (axis, extent) in extents.iter().enumerate() {
        if axis != runs.axis {
            run_count = append(block, arith::muli(run_count, *extent, location))?;
        }
    }

    let body = Block::new(&[(index, location)]);
    {
        let run: Value = body.argument(0)?.into();
        let at = run_coordinates(context, location, &body, runs.axis, &extents, run)?;
        let sort_by_radix = |into: &Block<'c>| -> Result<(), MlirError> {
            let Some((counts, bits)) = counts else {
                return Ok(());
            };
            let radix = Radix {
                context,
                location,
                runs,
                length,
                at: &at,
                counts,
                bits,
            };
            radix_run(&radix, into, out, scratch)
        };
        let sort_by_merge = |into: &Block<'c>| match &merge {
            Some(merge) => merge.run(into, runs, &at),
            None => Ok(()),
        };
        match radix_taken {
            Some(true) => sort_by_radix(&body)?,
            Some(false) => sort_by_merge(&body)?,
            None => {
                let threshold = index_constant(context, location, &body, RADIX_BUCKETS)?;
                let long = append(
                    &body,
                    arith::cmpi(context, CmpiPredicate::Uge, length, threshold, location),
                )?;
                let by_radix = Block::new(&[]);
                sort_by_radix(&by_radix)?;
                scf_yield(&by_radix, location, &[])?;
                let by_merge = Block::new(&[]);
                sort_by_merge(&by_merge)?;
                scf_yield(&by_merge, location, &[])?;
                let (then, otherwise) = (Region::new(), Region::new());
                then.append_block(by_radix);
                otherwise.append_block(by_merge);
                body.append_operation(scf::r#if(long, &[], then, otherwise, location));
            }
        }
        scf_yield(&body, location, &[])?;
    }
    let zero = index_constant(context, location, block, 0)?;
    let one = index_constant(context, location, block, 1)?;
    repeat(location, block, (zero, run_count, one), &[], body)?;
    let order_type = order_type(context, runs.shape);
    Ok(Some(to_tensor(context, location, block, out, order_type)?))
}

/// What every run's merge shares: its pass count and the buffers its passes swap, the first
/// pair chosen so that the last pass writes `out`.
struct Merge<'c, 'a> {
    context: &'c Context,
    location: Location<'c>,
    length: Value<'c, 'a>,
    passes: Value<'c, 'a>,
    /// The positions and keys the first pass reads, then the pair it writes.
    first: (Value<'c, 'a>, Value<'c, 'a>),
    second: (Value<'c, 'a>, Value<'c, 'a>),
}

impl<'c, 'a> Merge<'c, 'a> {
    fn prepare(
        context: &'c Context,
        location: Location<'c>,
        block: &'a Block<'c>,
        runs: &Runs<'c, 'a, '_>,
        length: Value<'c, 'a>,
        out: Value<'c, 'a>,
        scratch: Value<'c, 'a>,
    ) -> Result<Self, MlirError> {
        let index = Type::index(context);
        let sizes = dynamic_sizes(&runs.dims);
        let keys_type = MemRefType::new(runs.element, &buffer_shape(runs.shape), None, None);
        let keys = [
            append(
                block,
                memref::alloc(context, keys_type, &sizes, &[], None, location),
            )?,
            append(
                block,
                memref::alloc(context, keys_type, &sizes, &[], None, location),
            )?,
        ];
        // A run of `n` takes `ceil(log2(n))` doublings, the bit length of `n - 1`, and a run
        // of zero none: a constant for a static run, counted at run time for a dynamic one.
        let one = index_constant(context, location, block, 1)?;
        let passes = match runs.length() {
            Extent::Static(length) => index_constant(
                context,
                location,
                block,
                (usize::BITS - length.saturating_sub(1).leading_zeros()) as usize,
            )?,
            Extent::Dynamic(_) => {
                let at_least_one = append(block, arith::maxui(length, one, location))?;
                let shorter = append(block, arith::subi(at_least_one, one, location))?;
                let word: Type = IntegerType::new(context, 64).into();
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
                append(block, arith::index_cast(bits, index, location))?
            }
        };
        // An odd count starts in `scratch`, so the passes end in `out`.
        let parity = append(block, arith::andi(passes, one, location))?;
        let zero = index_constant(context, location, block, 0)?;
        let odd = append(
            block,
            arith::cmpi(context, CmpiPredicate::Ne, parity, zero, location),
        )?;
        let pick = |a, b| append(block, arith::select(odd, a, b, location));
        Ok(Self {
            context,
            location,
            length,
            passes,
            first: (pick(scratch, out)?, keys[0]),
            second: (pick(out, scratch)?, keys[1]),
        })
    }

    /// Sort the run at `at` into `out`: the identity order and the run's keys written into
    /// the first pair, then every pass.
    fn run(
        &self,
        body: &Block<'c>,
        runs: &Runs<'c, '_, '_>,
        at: &[Value<'c, '_>],
    ) -> Result<(), MlirError> {
        let (context, location) = (self.context, self.location);
        let index = Type::index(context);
        let zero = index_constant(context, location, body, 0)?;
        let one = index_constant(context, location, body, 1)?;
        let slot_at = |slot| {
            let mut indices = at.to_vec();
            indices[runs.axis] = slot;
            indices
        };

        let fill = Block::new(&[(index, location)]);
        {
            let slot: Value = fill.argument(0)?.into();
            let key = read(
                location,
                &fill,
                runs.source,
                (at, runs.axis, slot),
                runs.element,
            )?;
            fill.append_operation(memref::store(slot, self.first.0, &slot_at(slot), location));
            fill.append_operation(memref::store(key, self.first.1, &slot_at(slot), location));
            scf_yield(&fill, location, &[])?;
        }
        repeat(location, body, (zero, self.length, one), &[], fill)?;

        // Pass `2p` reads the first pair into the second, and pass `2p + 1`, when there is
        // one, reads it back: the buffers are named statically, since a buffer carried through
        // a loop would make the deallocation pipeline track it on every iteration.
        let pairs = append(body, arith::addi(self.passes, one, location))?;
        let pairs = append(body, arith::shrui(pairs, one, location))?;
        let pair = Block::new(&[(index, location)]);
        {
            let number: Value = pair.argument(0)?.into();
            let one = index_constant(context, location, &pair, 1)?;
            let even = append(&pair, arith::shli(number, one, location))?;
            self.pass(&pair, runs, at, even, self.first, self.second)?;
            let odd = append(&pair, arith::addi(even, one, location))?;
            let more = append(
                &pair,
                arith::cmpi(context, CmpiPredicate::Ult, odd, self.passes, location),
            )?;
            let guarded = Block::new(&[]);
            self.pass(&guarded, runs, at, odd, self.second, self.first)?;
            scf_yield(&guarded, location, &[])?;
            let region = Region::new();
            region.append_block(guarded);
            pair.append_operation(scf::r#if(more, &[], region, Region::new(), location));
            scf_yield(&pair, location, &[])?;
        }
        repeat(location, body, (zero, pairs, one), &[], pair)?;
        Ok(())
    }

    /// Pass `number` of the run at `at`: each pair of sorted blocks of `2^number` positions in
    /// `from` merged, positions and keys, into one block in `to`.
    fn pass(
        &self,
        block: &Block<'c>,
        runs: &Runs<'c, '_, '_>,
        at: &[Value<'c, '_>],
        number: Value<'c, '_>,
        from: (Value<'c, '_>, Value<'c, '_>),
        to: (Value<'c, '_>, Value<'c, '_>),
    ) -> Result<(), MlirError> {
        let (context, location) = (self.context, self.location);
        let index = Type::index(context);
        let slot_at = |slot| {
            let mut indices = at.to_vec();
            indices[runs.axis] = slot;
            indices
        };
        let one = index_constant(context, location, block, 1)?;
        let width = append(block, arith::shli(one, number, location))?;
        let pair = append(block, arith::shli(width, one, location))?;
        let zero = index_constant(context, location, block, 0)?;

        let blocks = Block::new(&[(index, location)]);
        {
            let low: Value = blocks.argument(0)?.into();
            let middle = append(&blocks, arith::addi(low, width, location))?;
            let middle = append(&blocks, arith::minui(middle, self.length, location))?;
            let high = append(&blocks, arith::addi(low, pair, location))?;
            let high = append(&blocks, arith::minui(high, self.length, location))?;
            let last = append(&blocks, arith::subi(self.length, one, location))?;

            let step = Block::new(&[(index, location), (index, location), (index, location)]);
            {
                let slot: Value = step.argument(0)?.into();
                let left: Value = step.argument(1)?.into();
                let right: Value = step.argument(2)?.into();
                // Both reads stay in the run: an exhausted side is clamped, and not taken.
                let left_at = append(&step, arith::minui(left, last, location))?;
                let right_at = append(&step, arith::minui(right, last, location))?;
                let load =
                    |buffer, slot| append(&step, memref::load(buffer, &slot_at(slot), location));
                let left_key = load(from.1, left_at)?;
                let right_key = load(from.1, right_at)?;
                let left_done = append(
                    &step,
                    arith::cmpi(context, CmpiPredicate::Uge, left, middle, location),
                )?;
                let right_left = append(
                    &step,
                    arith::cmpi(context, CmpiPredicate::Ult, right, high, location),
                )?;
                let right_first = precedes(
                    context,
                    location,
                    &step,
                    (right_key, left_key),
                    runs.kind,
                    runs.descending,
                )?;
                let wins = append(&step, arith::ori(left_done, right_first, location))?;
                let take_right = append(&step, arith::andi(right_left, wins, location))?;
                let taken_at = append(
                    &step,
                    arith::select(take_right, right_at, left_at, location),
                )?;
                let position = load(from.0, taken_at)?;
                let key = append(
                    &step,
                    arith::select(take_right, right_key, left_key, location),
                )?;
                step.append_operation(memref::store(position, to.0, &slot_at(slot), location));
                step.append_operation(memref::store(key, to.1, &slot_at(slot), location));
                let one = index_constant(context, location, &step, 1)?;
                let next_left = append(&step, arith::addi(left, one, location))?;
                let next_right = append(&step, arith::addi(right, one, location))?;
                let left = append(&step, arith::select(take_right, left, next_left, location))?;
                let right = append(
                    &step,
                    arith::select(take_right, next_right, right, location),
                )?;
                scf_yield(&step, location, &[left, right])?;
            }
            repeat(location, &blocks, (low, high, one), &[low, middle], step)?;
            scf_yield(&blocks, location, &[])?;
        }
        repeat(location, block, (zero, self.length, pair), &[], blocks)?;
        Ok(())
    }
}

/// The coordinates of run number `run`, counted row-major over every axis but the sorted
/// one, `axis`, which holds 0.
fn run_coordinates<'c, 'a>(
    context: &'c Context,
    location: Location<'c>,
    block: &'a Block<'c>,
    axis: usize,
    extents: &[Value<'c, '_>],
    run: Value<'c, 'a>,
) -> Result<Vec<Value<'c, 'a>>, MlirError> {
    let mut at = vec![index_constant(context, location, block, 0)?; extents.len()];
    let mut rest = run;
    for (position, extent) in extents.iter().enumerate().rev() {
        if position == axis {
            continue;
        }
        at[position] = append(block, arith::remui(rest, *extent, location))?;
        rest = append(block, arith::divui(rest, *extent, location))?;
    }
    Ok(at)
}
