// The runtime checks a linked body carries for integer arithmetic, and how it reports one
// that fails.
//
// The LLVM backend guards integer elements: an overflow panics on the debug tier and a zero
// divisor on every tier, each naming the operator's position. A body lowered here cannot
// panic that way. On the host it has no access to the backend's panic path or its location
// rendering, and on a GPU nothing can stop the program at all. So a guarded body takes one
// more parameter, a `memref<1xi64>` status word its caller fills with all ones, and a failing
// check lowers it to a key with an atomic unsigned min. The key's low `SITE_BITS` hold the
// check's number, 1 for the first; the caller reads the word once the body has run and
// panics with that check's message at that check's position, the host's own diagnostic. The
// checks travel with the symbol, so only the caller renders anything.
//
// The rest of the key orders failures as the host meets them, so the min keeps the one the
// host would have stopped at, whichever GPU thread got there first: the operation's number
// on top (operations run one after another on the host, operands first, which is the order
// checks are numbered in), then the row-major position of the element. Only a division
// carries two checks one element can fail in different ways, so only a division passes its
// element's position; any other operation's checks of one element all report the same
// diagnostic.
//
// A body keeps computing after a failed check, which is harmless because its caller will
// abort, but it must stay defined: a zero divisor is replaced by 1, and so is the `-1` of
// `MIN / -1`, before the division runs.

use std::cell::{Cell, RefCell};

use ast_types::BinaryOp;
use melior::{
    Context,
    dialect::{arith, llvm, scf},
    ir::{
        Block, BlockLike, Identifier, Location, Region, RegionLike, Type, Value, ValueLike,
        attribute::{DenseI64ArrayAttribute, IntegerAttribute},
        operation::OperationBuilder,
        r#type::{IntegerType, MemRefType},
    },
};
use neuro_hir::{HirTarget, HirType};

use crate::{errors::MlirError, tensor_reduce::append};

/// The status word's element width and extent: one `i64`.
pub(crate) const STATUS_BITS: u32 = 64;
const STATUS_EXTENT: i64 = 1;

/// The key's low bits, which hold a failed check's number. A body with more checks than they
/// can number is left to the LLVM backend.
pub(crate) const SITE_BITS: u32 = 12;
pub(crate) const MAX_SITES: usize = (1 << SITE_BITS) - 1;

/// Where the key's operation number starts, above 40 bits of element position.
const UNIT_SHIFT: u32 = 52;

/// The width of the flag an overflow intrinsic returns beside its value.
const FLAG_BITS: u32 = 1;

/// `memref.atomic_rmw`'s `kind` for an unsigned min: MLIR's `arith::AtomicRMWKind::minu`.
const ATOMIC_MIN_UNSIGNED: i64 = 11;

/// Whether integer `+`, `-` and `*` panic when they overflow, the debug tier, or wrap, the
/// release tier. A zero divisor panics on both.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Overflow {
    /// Debug builds: an overflowing element, and `MIN / -1`, is a panic.
    Checked,
    /// Release builds: two's-complement wraparound.
    Wrapping,
}

/// What a failed check in a linked body means, which picks the caller's panic message.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GuardKind {
    Overflow,
    DivisionByZero,
    RemainderByZero,
}

/// One check in a linked body. A body that fails it lowers its status word to a key whose
/// low [`SITE_BITS`] are the check's position in the symbol's guard list, counted from 1.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Guard {
    pub kind: GuardKind,
    /// The byte offset of the operation in the source, where the host's diagnostic points.
    pub offset: usize,
}

/// How one element of an arithmetic operator computes: in floating point, or as an
/// integer of this many bits whose signedness picks the division and the overflow test.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Element {
    Float,
    Signed(u32),
    Unsigned(u32),
}

impl Element {
    /// `None` for an element the language gives no tensor arithmetic: `f16`, `bf16`,
    /// `bool` and everything that is not a number.
    pub(crate) fn of(ty: &HirType) -> Option<Self> {
        Some(match ty {
            HirType::F32 | HirType::F64 => Element::Float,
            HirType::I8 => Element::Signed(8),
            HirType::I16 => Element::Signed(16),
            HirType::I32 => Element::Signed(32),
            HirType::I64 => Element::Signed(64),
            HirType::U8 => Element::Unsigned(8),
            HirType::U16 => Element::Unsigned(16),
            HirType::U32 => Element::Unsigned(32),
            HirType::U64 => Element::Unsigned(64),
            _ => return None,
        })
    }
}

/// Where one element's arithmetic sits: the operation's byte offset in the source, and,
/// for a division, the element's row-major position (an `index`).
#[derive(Clone, Copy)]
pub(crate) struct At<'c, 'v> {
    pub(crate) offset: usize,
    pub(crate) position: Option<Value<'c, 'v>>,
}

/// What one function body is being lowered for: its target, and the checks its integer
/// arithmetic has recorded so far. The status word is added to the body's entry block the
/// first time a check needs it, so a body with none keeps its signature.
pub(crate) struct Lowering<'c, 'a> {
    pub(crate) target: HirTarget,
    context: &'c Context,
    location: Location<'c>,
    entry: &'a Block<'c>,
    overflow: Overflow,
    status: Cell<Option<Value<'c, 'a>>>,
    sites: RefCell<Vec<Guard>>,
    /// The number of the operation being lowered: its first check's.
    unit: Cell<usize>,
}

impl<'c, 'a> Lowering<'c, 'a> {
    pub(crate) fn new(
        context: &'c Context,
        location: Location<'c>,
        entry: &'a Block<'c>,
        target: HirTarget,
        overflow: Overflow,
    ) -> Self {
        Lowering {
            target,
            context,
            location,
            entry,
            overflow,
            status: Cell::new(None),
            sites: RefCell::new(Vec::new()),
            unit: Cell::new(0),
        }
    }

    /// The checks recorded, in site order.
    pub(crate) fn into_sites(self) -> Vec<Guard> {
        self.sites.into_inner()
    }

    /// `lhs op rhs` for one element, appended to `block`, with the host's checks for an
    /// integer element. `None` for an operator this path does not lower.
    pub(crate) fn arith<'b>(
        &self,
        block: &'b Block<'c>,
        op: BinaryOp,
        element: Element,
        (lhs, rhs): (Value<'c, '_>, Value<'c, '_>),
        at: At<'c, '_>,
    ) -> Result<Option<Value<'c, 'b>>, MlirError> {
        let location = self.location;
        self.unit.set(self.sites.borrow().len() + 1);
        let operation = match (element, op) {
            (Element::Float, BinaryOp::Add) => arith::addf(lhs, rhs, location),
            (Element::Float, BinaryOp::Subtract) => arith::subf(lhs, rhs, location),
            (Element::Float, BinaryOp::Multiply) => arith::mulf(lhs, rhs, location),
            (Element::Float, BinaryOp::Divide) => arith::divf(lhs, rhs, location),
            (Element::Float, _) => return Ok(None),
            (_, BinaryOp::Add | BinaryOp::Subtract | BinaryOp::Multiply) => {
                return self
                    .overflowing(block, op, element, (lhs, rhs), at)
                    .map(Some);
            }
            (_, BinaryOp::Divide) => {
                return self.divide(block, element, (lhs, rhs), at).map(Some);
            }
            _ => return Ok(None),
        };
        append(block, operation).map(Some)
    }

    /// Integer `+`, `-` or `*`: through LLVM's `with.overflow` intrinsic on the debug tier,
    /// as the LLVM backend computes it, and the plain wrapping operation otherwise.
    fn overflowing<'b>(
        &self,
        block: &'b Block<'c>,
        op: BinaryOp,
        element: Element,
        (lhs, rhs): (Value<'c, '_>, Value<'c, '_>),
        at: At<'c, '_>,
    ) -> Result<Value<'c, 'b>, MlirError> {
        let location = self.location;
        if self.overflow == Overflow::Wrapping {
            let operation = match op {
                BinaryOp::Add => arith::addi(lhs, rhs, location),
                BinaryOp::Subtract => arith::subi(lhs, rhs, location),
                _ => arith::muli(lhs, rhs, location),
            };
            return append(block, operation);
        }
        let signed = matches!(element, Element::Signed(_));
        let intrinsic = match (op, signed) {
            (BinaryOp::Add, true) => "llvm.intr.sadd.with.overflow",
            (BinaryOp::Add, false) => "llvm.intr.uadd.with.overflow",
            (BinaryOp::Subtract, true) => "llvm.intr.ssub.with.overflow",
            (BinaryOp::Subtract, false) => "llvm.intr.usub.with.overflow",
            (_, true) => "llvm.intr.smul.with.overflow",
            (_, false) => "llvm.intr.umul.with.overflow",
        };
        let context = self.context;
        let value_type = lhs.r#type();
        let flag_type = IntegerType::new(context, FLAG_BITS).into();
        let pair = llvm::r#type::r#struct(context, &[value_type, flag_type], false);
        let both = append(
            block,
            OperationBuilder::new(intrinsic, location)
                .add_operands(&[lhs, rhs])
                .add_results(&[pair])
                .build()?,
        )?;
        let field = |index: i64, ty: Type<'c>| {
            append(
                block,
                llvm::extract_value(
                    context,
                    both,
                    DenseI64ArrayAttribute::new(context, &[index]),
                    ty,
                    location,
                ),
            )
        };
        let value = field(0, value_type)?;
        let overflowed = field(1, flag_type)?;
        self.fail_if(block, overflowed, GuardKind::Overflow, at)?;
        Ok(value)
    }

    /// Integer `/`. A zero divisor fails on every tier, `MIN / -1` on the debug tier; each
    /// then divides by 1, which also gives the release tier's wrap (`MIN / 1` is `MIN`).
    fn divide<'b>(
        &self,
        block: &'b Block<'c>,
        element: Element,
        (lhs, rhs): (Value<'c, '_>, Value<'c, '_>),
        at: At<'c, '_>,
    ) -> Result<Value<'c, 'b>, MlirError> {
        let location = self.location;
        let ty = lhs.r#type();
        let constant = |value: i64| {
            append(
                block,
                arith::constant(
                    self.context,
                    IntegerAttribute::new(ty, value).into(),
                    location,
                ),
            )
        };
        let compare = |a, b| {
            append(
                block,
                arith::cmpi(self.context, arith::CmpiPredicate::Eq, a, b, location),
            )
        };
        let one = constant(1)?;
        let by_zero = compare(rhs, constant(0)?)?;
        self.fail_if(block, by_zero, GuardKind::DivisionByZero, at)?;
        let divisor = append(block, arith::select(by_zero, one, rhs, location))?;
        let Element::Signed(bits) = element else {
            return append(block, arith::divui(lhs, divisor, location));
        };
        // `MIN` of a `bits`-wide integer, sign-extended into the attribute's 64 bits.
        let min = constant(i64::MIN >> (i64::BITS - bits))?;
        let lhs_min = compare(lhs, min)?;
        let minus_one = compare(divisor, constant(-1)?)?;
        let overflows = append(block, arith::andi(lhs_min, minus_one, location))?;
        if self.overflow == Overflow::Checked {
            self.fail_if(block, overflows, GuardKind::Overflow, at)?;
        }
        let safe = append(block, arith::select(overflows, one, divisor, location))?;
        append(block, arith::divsi(lhs, safe, location))
    }

    /// Record a check at `at` and append, to `block`, the lowering of the status word to its
    /// key when `failed` holds.
    fn fail_if(
        &self,
        block: &Block<'c>,
        failed: Value<'c, '_>,
        kind: GuardKind,
        at: At<'c, '_>,
    ) -> Result<(), MlirError> {
        let location = self.location;
        let context = self.context;
        let number = {
            let mut sites = self.sites.borrow_mut();
            sites.push(Guard {
                kind,
                offset: at.offset,
            });
            sites.len()
        };
        let status = self.status();
        let word = IntegerType::new(context, STATUS_BITS).into();
        let report = Block::new(&[]);
        let constant = |value: i64, ty: Type<'c>| {
            append(
                &report,
                arith::constant(context, IntegerAttribute::new(ty, value).into(), location),
            )
        };
        let fixed = ((self.unit.get() as i64) << UNIT_SHIFT) | number as i64;
        let mut key = constant(fixed, word)?;
        if let Some(position) = at.position {
            let position = append(&report, arith::index_cast(position, word, location))?;
            let shift = constant(i64::from(SITE_BITS), word)?;
            let shifted = append(&report, arith::shli(position, shift, location))?;
            key = append(&report, arith::ori(shifted, key, location))?;
        }
        let first = constant(0, Type::index(context))?;
        report.append_operation(
            OperationBuilder::new("memref.atomic_rmw", location)
                .add_operands(&[key, status, first])
                .add_attributes(&[(
                    Identifier::new(context, "kind"),
                    IntegerAttribute::new(
                        IntegerType::new(context, i64::BITS).into(),
                        ATOMIC_MIN_UNSIGNED,
                    )
                    .into(),
                )])
                .add_results(&[word])
                .build()?,
        );
        report.append_operation(scf::r#yield(&[], location));
        let then = Region::new();
        then.append_block(report);
        block.append_operation(scf::r#if(failed, &[], then, Region::new(), location));
        Ok(())
    }

    /// The status word, appended to the entry block's parameters on first use.
    fn status(&self) -> Value<'c, 'a> {
        if let Some(status) = self.status.get() {
            return status;
        }
        let word = IntegerType::new(self.context, STATUS_BITS).into();
        let ty = MemRefType::new(word, &[STATUS_EXTENT], None, None).into();
        let status = self.entry.add_argument(ty, self.location);
        self.status.set(Some(status));
        status
    }
}
