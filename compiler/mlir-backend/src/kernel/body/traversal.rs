//! The bodies a function outlined to follow its operands launches where `linalg` cannot say
//! them: a compound assignment, which must write its target's own buffer, and a traversal
//! (`.map`, `.zip`, `.reduce`), which calls a closure per element. Both are per-thread code,
//! so they are emitted like a `@kernel` body, but with the host's integer checks: a failed one
//! lowers the launcher's status word to its number and stops the thread, and the caller
//! panics as the host would.
//!
//! `.map` and `.zip` run one thread per element. `.reduce` folds in one thread, in the
//! receiver's row-major order, because its function is arbitrary and only that order is the
//! host's.

use ast_types::BinaryOp;
use neuro_hir::{
    HirClosure, HirExpr, HirExprKind, HirFunction, HirItem, HirPlace, HirProgram, HirStmt,
    HirTensorApply, HirType,
};

use super::{Binding, BodyEmitter, Failure, Lowered, Refused, is_float, memref_type, scalar_type};
use crate::guards::{Guard, Overflow};
use crate::kernel::GuardStyle;

/// Threads per block of an element-wise launch: one thread per element, the grid's one axis.
const THREADS_PER_BLOCK: u32 = 256;

/// One launch: the grid, the region the threads run, the `memref` type of the result the
/// launcher takes after its parameters, if it has one, and the checks the region reports
/// through the status word, which the launcher then takes before the result.
pub(in crate::kernel) struct Launch {
    pub(in crate::kernel) blocks: [u64; 3],
    pub(in crate::kernel) threads: [u32; 3],
    pub(in crate::kernel) region: String,
    pub(in crate::kernel) result: Option<String>,
    pub(in crate::kernel) guards: Vec<Guard>,
}

/// The launch an outlined `function` computes its body with, `None` for a body of another
/// shape, which is `linalg`'s or the host's.
///
/// # Errors
///
/// The construct the body stopped at, which leaves the function to the host.
pub(in crate::kernel) fn outlined_launch(
    program: &HirProgram,
    function: &HirFunction,
    guard: GuardStyle,
    overflow: Overflow,
    math: bool,
) -> Lowered<Option<Launch>> {
    let emitter = |threads| {
        let mut emitter = BodyEmitter::new(function, (guard, overflow), threads, Vec::new(), math);
        emitter.failure = Failure::Report;
        emitter
    };
    match function.body.as_slice() {
        [
            HirStmt::TensorCompoundAssign {
                place: HirPlace::Deref { pointer, .. },
                op,
                value,
                ty,
                span,
            },
        ] => emitter(element_threads())
            .compound(pointer, *op, value, ty, *span)
            .map(Some),
        [HirStmt::Expr(expr)] => match &expr.kind {
            HirExprKind::TensorApply {
                kind: HirTensorApply::Map | HirTensorApply::Zip,
                receiver,
                operand,
                callee,
            } => {
                let closure = closure(program, callee)?;
                emitter(element_threads())
                    .elementwise(closure, receiver, operand.as_deref(), &expr.ty)
                    .map(Some)
            }
            HirExprKind::TensorLiteral { elements } => match elements.as_slice() {
                [
                    HirExpr {
                        kind:
                            HirExprKind::TensorApply {
                                kind: HirTensorApply::Reduce,
                                receiver,
                                operand: Some(seed),
                                callee,
                            },
                        ..
                    },
                ] => {
                    let closure = closure(program, callee)?;
                    emitter([1, 1, 1])
                        .fold(closure, receiver, seed, &expr.ty)
                        .map(Some)
                }
                _ => Ok(None),
            },
            _ => Ok(None),
        },
        _ => Ok(None),
    }
}

fn element_threads() -> [u32; 3] {
    [THREADS_PER_BLOCK, 1, 1]
}

/// The lifted closure a traversal calls. The outlining pass keeps only closure literals in
/// a body, so the target is always known here.
fn closure<'p>(program: &'p HirProgram, callee: &HirExpr) -> Lowered<&'p HirClosure> {
    let HirExprKind::Closure { name, .. } = &callee.kind else {
        return Err(Refused::new(
            callee.span,
            "a function that is not a closure literal",
        ));
    };
    program
        .items
        .iter()
        .find_map(|item| match item {
            HirItem::Closure(closure) if closure.name == *name => Some(closure),
            _ => None,
        })
        .ok_or_else(|| Refused::new(callee.span, "a closure with no body"))
}

fn blocks_for(count: usize, threads: [u32; 3]) -> [u64; 3] {
    [(count as u64).div_ceil(u64::from(threads[0])), 1, 1]
}

impl BodyEmitter<'_> {
    /// `target OP= value`, one thread per element of the target, each reading and writing
    /// its own element. The value broadcasts up to the target as on the host: aligned at the
    /// trailing axis, an extent of 1 read at index 0.
    fn compound(
        mut self,
        pointer: &HirExpr,
        op: BinaryOp,
        value: &HirExpr,
        ty: &HirType,
        span: shared_types::Span,
    ) -> Lowered<Launch> {
        let (target, target_ty, extents) = self.tensor(pointer)?;
        let HirType::Tensor { element, .. } = ty else {
            return Err(Refused::new(span, "a compound assignment to a non-tensor"));
        };
        let Some(mlir) = scalar_type(element).filter(|_| **element != HirType::Bool) else {
            return Err(Refused::new(
                span,
                "a compound assignment to a non-numeric tensor",
            ));
        };
        let float = match op {
            BinaryOp::Add => "addf",
            BinaryOp::Subtract => "subf",
            BinaryOp::Multiply => "mulf",
            BinaryOp::Divide => "divf",
            _ => return Err(Refused::new(span, "this compound operator")),
        };
        let count: usize = extents.iter().product();
        let exit = self.block();
        self.stop = Some(exit.clone());
        let indices = self.thread_element(count, &extents, &exit);
        let joined = indices.join(", ");
        let current = self.assign(&format!("memref.load {target}[{joined}] : {target_ty}"));
        let operand = match value.ty.referent() {
            HirType::Tensor { .. } => {
                let (memref, ty, own) = self.tensor(value)?;
                let offset = extents
                    .len()
                    .checked_sub(own.len())
                    .ok_or_else(|| Refused::new(span, "a value of higher rank than its target"))?;
                let mut read = Vec::with_capacity(own.len());
                for (axis, extent) in own.iter().enumerate() {
                    read.push(match *extent {
                        e if e == extents[offset + axis] => indices[offset + axis].clone(),
                        1 => self.assign("arith.constant 0 : index"),
                        _ => return Err(Refused::new(span, "a value that does not broadcast")),
                    });
                }
                let read = read.join(", ");
                self.assign(&format!("memref.load {memref}[{read}] : {ty}"))
            }
            _ => self.expr(value, &exit)?,
        };
        // The host checks a tensor's integer element exactly as the scalar operator, at the
        // statement.
        let updated = match is_float(element) {
            true => self.assign(&format!("arith.{float} {current}, {operand} : {mlir}")),
            false => self.checked_int(op, (&current, &operand), element, span.start),
        };
        self.line(&format!(
            "memref.store {updated}, {target}[{joined}] : {target_ty}"
        ));
        let threads = self.threads;
        let (region, guards) = self.finish(&exit);
        Ok(Launch {
            blocks: blocks_for(count, threads),
            threads,
            region,
            result: None,
            guards,
        })
    }

    /// `.map(f)` or `.zip(other, f)`: one thread per element, each calling `f` on its
    /// element (and `other`'s at the same position) and writing the answer to the result.
    fn elementwise(
        mut self,
        closure: &HirClosure,
        receiver: &HirExpr,
        other: Option<&HirExpr>,
        result: &HirType,
    ) -> Lowered<Launch> {
        let out_ty = memref_type(result)
            .ok_or_else(|| Refused::new(receiver.span, "a result of this type"))?;
        let out = format!("%arg{}", self.function.params.len());
        let (source, source_ty, extents) = self.tensor(receiver)?;
        let paired = other.map(|other| self.tensor(other)).transpose()?;
        if paired.as_ref().is_some_and(|(_, _, own)| *own != extents) {
            return Err(Refused::new(receiver.span, "a `.zip` over two shapes"));
        }
        let count: usize = extents.iter().product();
        let exit = self.block();
        self.stop = Some(exit.clone());
        let indices = self.thread_element(count, &extents, &exit).join(", ");
        let mut args = vec![self.assign(&format!("memref.load {source}[{indices}] : {source_ty}"))];
        if let Some((memref, ty, _)) = paired {
            args.push(self.assign(&format!("memref.load {memref}[{indices}] : {ty}")));
        }
        let value = self.call_closure(closure, &args, receiver.span)?;
        self.line(&format!(
            "memref.store {value}, {out}[{indices}] : {out_ty}"
        ));
        let threads = self.threads;
        let (region, guards) = self.finish(&exit);
        Ok(Launch {
            blocks: blocks_for(count, threads),
            threads,
            region,
            result: Some(out_ty),
            guards,
        })
    }

    /// `.reduce(init, f)`: one thread carrying the accumulator through every element in
    /// row-major order, then writing it to the one-element result.
    fn fold(
        mut self,
        closure: &HirClosure,
        receiver: &HirExpr,
        seed: &HirExpr,
        result: &HirType,
    ) -> Lowered<Launch> {
        let out_ty = memref_type(result)
            .ok_or_else(|| Refused::new(receiver.span, "a result of this type"))?;
        let out = format!("%arg{}", self.function.params.len());
        let (source, source_ty, extents) = self.tensor(receiver)?;
        let Some(carried) = scalar_type(&seed.ty) else {
            return Err(Refused::new(seed.span, "an accumulator of this type"));
        };
        let count: usize = extents.iter().product();
        let exit = self.block();
        self.stop = Some(exit.clone());
        let start = self.expr(seed, &exit)?;
        let accumulator = self.slot(carried);
        self.line(&format!(
            "memref.store {start}, {accumulator}[] : memref<{carried}>"
        ));
        let counter = self.slot("i64");
        let zero = self.assign("arith.constant 0 : i64");
        self.line(&format!("memref.store {zero}, {counter}[] : memref<i64>"));

        let head = self.block();
        let body = self.block();
        let after = self.block();
        self.branch(&head);
        self.start(&head);
        let at = self.assign(&format!("memref.load {counter}[] : memref<i64>"));
        let bound = self.assign(&format!("arith.constant {count} : i64"));
        let more = self.assign(&format!("arith.cmpi ult, {at}, {bound} : i64"));
        self.cond_branch(&more, &body, &after);

        self.start(&body);
        let indices = self.delinearize(&at, &extents);
        let element = self.assign(&format!("memref.load {source}[{indices}] : {source_ty}"));
        let so_far = self.assign(&format!("memref.load {accumulator}[] : memref<{carried}>"));
        let next = self.call_closure(closure, &[so_far, element], receiver.span)?;
        self.line(&format!(
            "memref.store {next}, {accumulator}[] : memref<{carried}>"
        ));
        let at = self.assign(&format!("memref.load {counter}[] : memref<i64>"));
        let one = self.assign("arith.constant 1 : i64");
        let stepped = self.assign(&format!("arith.addi {at}, {one} : i64"));
        self.line(&format!(
            "memref.store {stepped}, {counter}[] : memref<i64>"
        ));
        self.branch(&head);

        self.start(&after);
        let total = self.assign(&format!("memref.load {accumulator}[] : memref<{carried}>"));
        let first = self.assign("arith.constant 0 : index");
        self.line(&format!("memref.store {total}, {out}[{first}] : {out_ty}"));
        let threads = self.threads;
        let (region, guards) = self.finish(&exit);
        Ok(Launch {
            blocks: [1, 1, 1],
            threads,
            region,
            result: Some(out_ty),
            guards,
        })
    }

    /// Skip a thread past the last of `count` elements, and otherwise give it the indices
    /// of the element its position names, row-major over `extents`.
    fn thread_element(&mut self, count: usize, extents: &[usize], exit: &str) -> Vec<String> {
        let position = self.thread_position(0);
        let bound = self.assign(&format!("arith.constant {count} : index"));
        let inside = self.assign(&format!("arith.cmpi ult, {position}, {bound} : index"));
        let run = self.block();
        self.cond_branch(&inside, &run, exit);
        self.start(&run);
        let flat = self.assign(&format!("arith.index_cast {position} : index to i64"));
        self.element = Some(flat.clone());
        self.delinearize_each(&flat, extents)
    }

    /// Run `closure`'s body with its parameters bound to `args`, returning its value. A
    /// `return` in it leaves the closure, so it ends at the block after the call.
    fn call_closure(
        &mut self,
        closure: &HirClosure,
        args: &[String],
        span: shared_types::Span,
    ) -> Lowered<String> {
        let Some(ty) = scalar_type(&closure.return_type) else {
            return Err(Refused::new(span, "a function returning this type"));
        };
        if closure.params.len() != args.len() {
            return Err(Refused::new(span, "a function of another arity"));
        }
        let slot = self.slot(ty);
        let done = self.block();
        self.scopes.push(
            closure
                .params
                .iter()
                .zip(args)
                .map(|(param, value)| {
                    let binding = Binding::Param {
                        value: value.clone(),
                        ty: param.ty.clone(),
                    };
                    (param.name.clone(), binding)
                })
                .collect(),
        );
        let saved = self.returns.replace((slot.clone(), ty));
        let walked = self.arm(&closure.body, Some((&slot, ty)), &done);
        self.returns = saved;
        self.scopes.pop();
        walked?;
        self.branch(&done);
        self.start(&done);
        Ok(self.assign(&format!("memref.load {slot}[] : memref<{ty}>")))
    }

    /// The region's text, ending in the block that terminates the launch, and its checks.
    fn finish(mut self, exit: &str) -> (String, Vec<Guard>) {
        self.branch(exit);
        self.start(exit);
        self.line("gpu.terminator");
        (format!("{}{}", self.slots, self.text), self.sites)
    }
}

#[cfg(test)]
mod tests {
    use crate::{GpuTarget, MlirError, kernel::kernel_launchers, lower_for_gpu};

    use neuro_hir::HirProgram;

    fn program(body: &str) -> HirProgram {
        let source = format!(
            "enum Device {{\n    CPU,\n    GPU(i32)\n}}\n\nfunc main() -> i32 {{\n    val m: Tensor<f32, [37, 19]> = Tensor::ones()\n    val g = m.clone().to(Device::GPU(0))\n    val scale = 2.0f32\n{body}\n    return 0\n}}\n"
        );
        let ast = syntax_parsing::parse(&source).expect("the program parses");
        hir_lowering::lower_program(&ast).expect("the program lowers to HIR")
    }

    fn nvidia() -> GpuTarget {
        GpuTarget::Nvidia {
            chip: "sm_80".to_string(),
        }
    }

    fn launched(body: &str, math: bool) -> (String, usize) {
        let launchers =
            kernel_launchers(&program(body), &nvidia(), crate::Overflow::Checked, &|| {
                math
            })
            .expect("an outlined body is never refused");
        (launchers.text, launchers.functions.len())
    }

    #[test]
    fn a_map_calls_its_closure_once_per_element() {
        let (text, count) = launched("    val r = g.map(|x: f32| -> f32 { x * scale })", true);
        assert_eq!(count, 1);
        // 703 elements in blocks of 256, the capture after the receiver, the result last.
        assert!(text.contains("%grid0 = arith.constant 3 : index"), "{text}");
        assert!(
            text.contains("(%arg0: memref<37x19xf32>, %arg1: f32, %arg2: memref<37x19xf32>)"),
            "{text}"
        );
        assert!(text.contains("arith.mulf"), "{text}");
        lower_for_gpu(
            &program("    val r = g.map(|x: f32| -> f32 { x * scale })"),
            &nvidia(),
            crate::Overflow::Checked,
        )
        .expect("the launcher lowers for NVIDIA");
    }

    #[test]
    fn a_reduce_folds_in_one_thread() {
        let (text, count) = launched(
            "    val r = g.reduce(0.0f32, |acc: f32, x: f32| -> f32 { acc + x * scale })",
            true,
        );
        assert_eq!(count, 1);
        assert!(
            text.contains("%grid0 = arith.constant 1 : index")
                && text.contains("%block0 = arith.constant 1 : index"),
            "{text}"
        );
    }

    #[test]
    fn a_compound_assignment_writes_its_target() {
        let (text, count) = launched("    mut w = m.clone()\n    w -= &g", true);
        assert_eq!(count, 1);
        assert!(text.contains("arith.subf"), "{text}");
        assert!(
            text.contains("memref.store") && text.contains("%arg1["),
            "the update lands in the `&mut` target:\n{text}"
        );
    }

    #[test]
    fn integer_arithmetic_in_a_closure_reports_through_the_status_word() {
        let (text, count) = launched(
            "    val r = g.map(|x: f32| -> f32 { ((x as i32 + 1) / (x as i32)) as f32 })",
            true,
        );
        assert_eq!(count, 1);
        assert!(
            text.contains("llvm.intr.sadd.with.overflow")
                && text.contains("%status: memref<1xi64>, %arg1: memref<37x19xf32>"),
            "the status word sits before the result:\n{text}"
        );
        assert_eq!(
            text.matches("memref.atomic_rmw minu").count(),
            3,
            "the add's overflow, the divisor and `MIN / -1` each report a failure:\n{text}"
        );
    }

    #[test]
    fn what_the_device_cannot_compute_is_left_to_the_host() {
        for body in [
            "    val r = g.map(|x: f32| -> f32 { ((x as i32) << 1) as f32 })",
            "    val r = g.map(|x: f32| -> f32 { x.exp() })",
        ] {
            let (_, count) = launched(body, false);
            assert_eq!(count, 0, "`{body}`");
        }
        let (_, count) = launched("    val r = g.map(|x: f32| -> f32 { x.exp() })", true);
        assert_eq!(count, 1, "a math function with its library");
    }

    #[test]
    fn a_kernel_without_the_math_library_is_refused_at_the_call() {
        let source = r#"
@kernel(threads: [4])
func k(a: Tensor<f32, [4]>, out: KernelOut<Tensor<f32, [4]>>) {
    val i = thread_id.x
    if i < 4 {
        unsafe { out[i] = a[i].exp() }
    }
}

func main() -> i32 {
    val a = Tensor::<f32, [4]>::ones()
    mut o = Tensor::<f32, [4]>::zeros()
    k(a, &mut o)
    return 0
}
"#;
        let ast = syntax_parsing::parse(source).expect("the kernel parses");
        let program = hir_lowering::lower_program(&ast).expect("the kernel lowers to HIR");
        let Err(refusals) =
            kernel_launchers(&program, &nvidia(), crate::Overflow::Checked, &|| false)
        else {
            panic!("expected the math call refused");
        };
        assert!(
            refusals[0].what.contains("device math library"),
            "{refusals:?}"
        );
        assert!(kernel_launchers(&program, &nvidia(), crate::Overflow::Checked, &|| true).is_ok());
        let _ = MlirError::KernelBodiesNotLowered(refusals);
    }
}
