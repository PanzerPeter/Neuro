// The host's sort for an integer run at least `RADIX_BUCKETS` long: LSD radix, one byte a
// pass, in `scf.for` loops over buffers, inside `host.rs`'s loop over runs. A pass counts each digit, turns the counts into
// first slots, then places every position at its digit's next slot in run order, which is
// what keeps it stable. Flipping the sign bit turns a signed order into an unsigned one, and
// flipping every bit reverses it for `descending`, equal keys still in run order.
//
// A GPU body cannot take it: its loops are no `linalg`, so a launcher would run them on the
// host against device memory, and a radix pass written in `linalg` would need a scan and a
// scatter.

use super::{RADIX_BUCKETS, Runs, read, repeat, scf_yield};
use crate::{
    errors::MlirError,
    guards::Element,
    tensor_reduce::{append, index_constant},
};

use melior::{
    Context,
    dialect::{arith, memref},
    ir::{
        Block, BlockLike, Location, Type, Value,
        attribute::IntegerAttribute,
        r#type::{IntegerType, MemRefType},
    },
};

/// The bits of one radix digit, the ones [`RADIX_BUCKETS`] counts.
const DIGIT_BITS: u32 = 8;

/// The counts buffer every radix pass tallies its digits in, allocated once in `block`.
pub(super) fn radix_counts<'c, 'a>(
    context: &'c Context,
    location: Location<'c>,
    block: &'a Block<'c>,
) -> Result<Value<'c, 'a>, MlirError> {
    let counts_type = MemRefType::new(Type::index(context), &[RADIX_BUCKETS as i64], None, None);
    append(
        block,
        memref::alloc(context, counts_type, &[], &[], None, location),
    )
}

/// Sort the run at `at` by LSD radix, one [`DIGIT_BITS`] digit of the `bits`-wide key a
/// pass, its passes writing `out` and `scratch` in turn so the last one writes `out`.
pub(super) fn radix_run<'c>(
    radix: &Radix<'c, '_, '_>,
    body: &Block<'c>,
    out: Value<'c, '_>,
    scratch: Value<'c, '_>,
) -> Result<(), MlirError> {
    let passes = (radix.bits / DIGIT_BITS) as usize;
    let orders = match passes % 2 {
        1 => [out, scratch],
        _ => [scratch, out],
    };
    let mut previous = None;
    for (pass, &order) in orders.iter().cycle().take(passes).enumerate() {
        radix.clear(body)?;
        radix.tally(body, previous, pass)?;
        radix.firsts(body)?;
        radix.place(body, order, previous, pass)?;
        previous = Some(order);
    }
    Ok(())
}

/// The radix sort's loops over one run, whose coordinates are `at`, counting digits in
/// `counts`.
pub(super) struct Radix<'c, 'v, 'r> {
    pub(super) context: &'c Context,
    pub(super) location: Location<'c>,
    pub(super) runs: &'r Runs<'c, 'v, 'r>,
    /// The length of one run, which every pass counts its positions by.
    pub(super) length: Value<'c, 'v>,
    pub(super) at: &'r [Value<'c, 'v>],
    pub(super) counts: Value<'c, 'v>,
    pub(super) bits: u32,
}

impl<'c> Radix<'c, '_, '_> {
    /// Every digit's count set to 0.
    fn clear(&self, block: &Block<'c>) -> Result<(), MlirError> {
        let index = Type::index(self.context);
        let step = Block::new(&[(index, self.location)]);
        {
            let digit: Value = step.argument(0)?.into();
            let zero = index_constant(self.context, self.location, &step, 0)?;
            step.append_operation(memref::store(zero, self.counts, &[digit], self.location));
            scf_yield(&step, self.location, &[])?;
        }
        radix_loop(self.context, self.location, block, None, &[], step)?;
        Ok(())
    }

    /// One more count for the digit `pass` reads from every element of the run.
    fn tally(
        &self,
        block: &Block<'c>,
        previous: Option<Value<'c, '_>>,
        pass: usize,
    ) -> Result<(), MlirError> {
        let index = Type::index(self.context);
        let location = self.location;
        let step = Block::new(&[(index, location)]);
        {
            let slot: Value = step.argument(0)?.into();
            let held = self.held(&step, previous, slot)?;
            let digit = self.digit(&step, held, pass)?;
            let count = append(&step, memref::load(self.counts, &[digit], location))?;
            let one = index_constant(self.context, location, &step, 1)?;
            let count = append(&step, arith::addi(count, one, location))?;
            step.append_operation(memref::store(count, self.counts, &[digit], location));
            scf_yield(&step, location, &[])?;
        }
        radix_loop(self.context, location, block, Some(self.length), &[], step)?;
        Ok(())
    }

    /// Each digit's count turned into its first slot: the sum of the counts before it.
    fn firsts(&self, block: &Block<'c>) -> Result<(), MlirError> {
        let index = Type::index(self.context);
        let location = self.location;
        let step = Block::new(&[(index, location), (index, location)]);
        {
            let digit: Value = step.argument(0)?.into();
            let total: Value = step.argument(1)?.into();
            let count = append(&step, memref::load(self.counts, &[digit], location))?;
            step.append_operation(memref::store(total, self.counts, &[digit], location));
            let total = append(&step, arith::addi(total, count, location))?;
            scf_yield(&step, location, &[total])?;
        }
        let zero = index_constant(self.context, location, block, 0)?;
        radix_loop(self.context, location, block, None, &[zero], step)?;
        Ok(())
    }

    /// Every run position written into `order` at its digit's next slot, in run order.
    fn place(
        &self,
        block: &Block<'c>,
        order: Value<'c, '_>,
        previous: Option<Value<'c, '_>>,
        pass: usize,
    ) -> Result<(), MlirError> {
        let index = Type::index(self.context);
        let location = self.location;
        let step = Block::new(&[(index, location)]);
        {
            let slot: Value = step.argument(0)?.into();
            let held = self.held(&step, previous, slot)?;
            let digit = self.digit(&step, held, pass)?;
            let next = append(&step, memref::load(self.counts, &[digit], location))?;
            let mut at = self.at.to_vec();
            at[self.runs.axis] = next;
            step.append_operation(memref::store(held, order, &at, location));
            let one = index_constant(self.context, location, &step, 1)?;
            let next = append(&step, arith::addi(next, one, location))?;
            step.append_operation(memref::store(next, self.counts, &[digit], location));
            scf_yield(&step, location, &[])?;
        }
        radix_loop(self.context, location, block, Some(self.length), &[], step)?;
        Ok(())
    }

    /// The run position `previous` holds at run slot `slot`, or `slot` itself before the
    /// first pass.
    fn held<'b>(
        &self,
        block: &'b Block<'c>,
        previous: Option<Value<'c, '_>>,
        slot: Value<'c, 'b>,
    ) -> Result<Value<'c, 'b>, MlirError> {
        let Some(previous) = previous else {
            return Ok(slot);
        };
        let mut at = self.at.to_vec();
        at[self.runs.axis] = slot;
        append(block, memref::load(previous, &at, self.location))
    }

    /// Digit `pass` of the key of the element at run position `held`, as an `index`. The key
    /// is the element with its sign bit flipped when signed, so the order is unsigned, and
    /// every bit flipped when descending.
    fn digit<'b>(
        &self,
        block: &'b Block<'c>,
        held: Value<'c, '_>,
        pass: usize,
    ) -> Result<Value<'c, 'b>, MlirError> {
        let location = self.location;
        let runs = self.runs;
        let key_type: Type = IntegerType::new(self.context, self.bits).into();
        let constant = |value: i64| {
            append(
                block,
                arith::constant(
                    self.context,
                    IntegerAttribute::new(key_type, value).into(),
                    location,
                ),
            )
        };
        let element = read(
            location,
            block,
            runs.source,
            (self.at, runs.axis, held),
            runs.element,
        )?;
        // The sign bit as a signed `bits`-wide value, so the attribute fits its type.
        let sign = match runs.kind {
            Element::Signed(_) => i64::MIN >> (i64::BITS - self.bits),
            _ => 0,
        };
        let flip = if runs.descending { !sign } else { sign };
        let mut key = element;
        if flip != 0 {
            key = append(block, arith::xori(key, constant(flip)?, location))?;
        }
        if pass > 0 {
            let shift = constant(pass as i64 * i64::from(DIGIT_BITS))?;
            key = append(block, arith::shrui(key, shift, location))?;
        }
        if self.bits > DIGIT_BITS {
            let mask = constant(RADIX_BUCKETS as i64 - 1)?;
            key = append(block, arith::andi(key, mask, location))?;
        }
        append(
            block,
            arith::index_castui(key, Type::index(self.context), location),
        )
    }
}

/// An `scf.for` over `0..count` in `block`, carrying `initial` through `step`: `count` is a
/// run's length, or [`RADIX_BUCKETS`] when `None`.
fn radix_loop<'c, 'a>(
    context: &'c Context,
    location: Location<'c>,
    block: &'a Block<'c>,
    count: Option<Value<'c, '_>>,
    initial: &[Value<'c, '_>],
    step: Block<'c>,
) -> Result<Vec<Value<'c, 'a>>, MlirError> {
    let zero = index_constant(context, location, block, 0)?;
    let one = index_constant(context, location, block, 1)?;
    let count = match count {
        Some(count) => count,
        None => index_constant(context, location, block, RADIX_BUCKETS)?,
    };
    repeat(location, block, (zero, count, one), initial, step)
}
