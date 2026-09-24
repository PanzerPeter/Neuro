// Unit tests for the llvm-backend entry point: they drive `compile` over real
// source through parse -> lower, and assert on the emitted module rather than on
// internals. Integration-level behaviour lives in the neurc suite.

use super::*;
use crate::codegen::context::ALIGNED_ALLOC_FN;
use crate::type_mapping::TypeMapper;

/// Parse and lower `source` to typed HIR for the backend smoke tests. Mirrors the
/// `parse → lower → compile` pipeline `neurc` runs (lowering assumes well-typedness).
fn lower(source: &str) -> neuro_hir::HirProgram {
    let ast = syntax_parsing::parse(source).expect("parsing failed");
    hir_lowering::lower_program(&ast).expect("HIR lowering failed")
}

/// Compile `source` to LLVM IR text, for the tests that assert on module structure
/// rather than on the opaque object code `compile` returns.
fn module_ir(source: &str, optimization: OptimizationLevelSetting) -> String {
    let hir = lower(source);
    let context = LLVMContext::create();
    let codegen_ctx = build_module(&context, &hir, optimization, source, "outlining.nr")
        .expect("module generation failed");
    codegen_ctx.module.print_to_string().to_string()
}

/// `source` lowered, compiled, and run through the optimization pipeline for
/// `optimization`: the IR instruction selection actually receives, rather than the
/// unoptimized IR `module_ir` returns.
fn optimized_ir(source: &str, optimization: OptimizationLevelSetting) -> String {
    let hir = lower(source);
    let context = LLVMContext::create();
    let codegen_ctx = build_module(&context, &hir, optimization, source, "optimized.nr")
        .expect("module generation failed");
    let (machine, triple) =
        host_target_machine(optimization).expect("host target machine unavailable");
    optimize_module(&codegen_ctx, &machine, &triple, optimization)
        .expect("optimization pipeline failed");
    codegen_ctx.module.print_to_string().to_string()
}

/// The body of the function named `name` in `ir`, excluding every other definition.
fn function_body<'a>(ir: &'a str, name: &str) -> &'a str {
    let header = format!("@{}(", name);
    let start = ir
        .find(&header)
        .unwrap_or_else(|| panic!("no definition of @{} in the module", name));
    let rest = &ir[start..];
    match rest.find("\n}\n") {
        Some(end) => &rest[..end],
        None => rest,
    }
}

/// The prelude declarations the `PoolAware` tests need. `module_ir` drives the
/// pipeline without `neurc`, which is what prepends the real prelude.
const POOL_AWARE_PRELUDE: &str = "
    struct PoolHandle { id: u64 }
    trait PoolAware {
        func register_with_pool(&self, arena: &PoolHandle)
        func bulk_release(&mut self)
    }
    struct Handle { id: i32 }
    impl PoolAware for Handle {
        func register_with_pool(&self, arena: &PoolHandle) { }
        func bulk_release(&mut self) { }
    }
";

/// Section 4.9's release order, read off the IR: every `PoolAware` value the block
/// owns is registered where it is built, and the sweep that releases them runs
/// before the mark restore that reclaims the memory they live in.
#[test]
fn a_pool_sweeps_its_registrations_before_reclaiming_the_arena() {
    let source = format!(
        "{POOL_AWARE_PRELUDE}
        func main() -> i32 {{
            pool scratch {{
                val first = Handle {{ id: 1 }}
                val second = Handle {{ id: 2 }}
            }}
            return 0
        }}"
    );
    let ir = module_ir(&source, OptimizationLevelSetting::O0);
    let body = function_body(&ir, "main");

    let registrations = body.matches("call void @__neuro_pool_register").count();
    assert_eq!(
        registrations, 2,
        "one registration per owned value:\n{body}"
    );
    assert_eq!(
        body.matches("call void @Handle__register_with_pool")
            .count(),
        2,
        "the type's own hook runs at each construction:\n{body}"
    );

    let sweep = body
        .find("call void @__neuro_pool_sweep")
        .expect("no sweep emitted");
    let reclaim = body
        .find("call void @__neuro_arena_release")
        .expect("no mark restore emitted");
    assert!(sweep < reclaim, "sweep must precede the restore:\n{body}");
}

/// A pool whose values are plain data owes the arena nothing per object, so none of
/// the registry is emitted and its exit stays the single store it was.
#[test]
fn a_pool_without_pool_aware_values_emits_no_registry() {
    let source = r#"
        struct Point { x: i32 }
        func main() -> i32 {
            pool scratch {
                val p = Point { x: 1 }
            }
            return 0
        }
    "#;
    let ir = module_ir(source, OptimizationLevelSetting::O0);
    assert!(!ir.contains("__neuro_pool_register"), "{ir}");
    assert!(!ir.contains("__neuro_pool_sweep"), "{ir}");
}

/// A tensor value is a filled-in `DLManagedTensorVersioned`, not a bare
/// buffer pointer with a conversion step waiting at an FFI boundary.
#[test]
fn a_tensor_value_is_a_populated_dlpack_handle() {
    let source = r#"
        func main() -> i32 {
            val m: Tensor<f32, [2, 3]> = [[1.0, 2.0, 3.0], [4.0, 5.0, 6.0]]
            return 0
        }
    "#;
    let ir = module_ir(source, OptimizationLevelSetting::O0);

    // Shape and strides are shared per tensor type; strides count elements, not bytes.
    assert!(
        ir.contains("@__neuro_dlpack_shape_f32_2x3 = private constant [2 x i64] [i64 2, i64 3]")
    );
    assert!(
        ir.contains("@__neuro_dlpack_strides_f32_2x3 = private constant [2 x i64] [i64 3, i64 1]")
    );

    let body = function_body(&ir, "main");
    // version { 1, 1 }: the versioned structure, not the deprecated one.
    assert!(body.contains("store { i32, i32 } { i32 1, i32 1 }"));
    // device { kDLCPU, 0 }.
    assert!(body.contains("store { i32, i32 } { i32 1, i32 0 }"));
    // dtype { kDLFloat, 32 bits, 1 lane }.
    assert!(body.contains("store { i8, i8, i16 } { i8 2, i8 32, i16 1 }"));
    assert!(
        body.contains("store i32 2, ptr %dlpack.field"),
        "ndim is the rank"
    );
    assert!(body.contains("store ptr @__neuro_dlpack_shape_f32_2x3"));
    assert!(body.contains("store ptr @__neuro_dlpack_strides_f32_2x3"));
    assert!(body.contains("store ptr @__neuro_dlpack_deleter"));
}

/// `manager_ctx` carries the compiler's own per-tensor control block, which trails the
/// exchange structure inside the SAME allocation: reserving the field costs a store,
/// not a second `malloc` and not a second free.
#[test]
fn a_handle_carries_a_control_block_inside_its_own_allocation() {
    let source = r#"
        func main() -> i32 {
            val m: Tensor<f32, [2, 3]> = [[1.0, 2.0, 3.0], [4.0, 5.0, 6.0]]
            return 0
        }
    "#;
    let ir = module_ir(source, OptimizationLevelSetting::O0);
    let body = function_body(&ir, "main");

    // The control block is reached from the handle itself, so `manager_ctx` is an
    // interior pointer rather than null.
    assert!(
        body.contains("%dlpack.control = getelementptr inbounds"),
        "the control block trails the handle:\n{body}"
    );
    assert!(
        !body.contains("store ptr null, ptr %dlpack.field"),
        "`manager_ctx` is no longer null:\n{body}"
    );
    // Six f32 elements: the unpadded run the buffer holds.
    assert!(
        body.contains("store i64 24, ptr %dlpack.field"),
        "the control block records the element buffer's byte length:\n{body}"
    );
    // One `malloc` for handle plus control block, one over-aligned allocation for the
    // elements. The reservation added neither.
    assert_eq!(body.matches("call ptr @malloc(").count(), 1);
    assert_eq!(
        body.matches(&format!("call ptr @{ALIGNED_ALLOC_FN}("))
            .count(),
        1
    );
}

/// The element buffer comes from the over-aligned allocator at DLPack's 64-byte
/// alignment, and only the elements themselves are copied into it: the padding is the
/// allocator's.
#[test]
fn a_tensor_buffer_meets_the_dlpack_alignment() {
    let source = r#"
        func main() -> i32 {
            val m: Tensor<f32, [2, 3]> = [[1.0, 2.0, 3.0], [4.0, 5.0, 6.0]]
            return 0
        }
    "#;
    let ir = module_ir(source, OptimizationLevelSetting::O0);
    let body = function_body(&ir, "main");
    // 24 bytes of elements round up to one whole alignment unit, so both arguments
    // are 64 here whichever way round the platform takes them.
    assert!(body.contains(&format!("call ptr @{ALIGNED_ALLOC_FN}(i64 64, i64 64)")));
    assert!(
        body.contains("i64 24)"),
        "six f32 elements are 24 bytes to copy"
    );
}

/// The two spellings of the over-aligned allocator take the same pair of `size_t`s the
/// other way round, so a buffer larger than one alignment unit pins the order: passing
/// them the wrong way round asks for 64 bytes at 256-byte alignment and overruns.
#[test]
fn the_over_aligned_allocator_is_called_in_its_platforms_argument_order() {
    let source = r#"
        func main() -> i32 {
            val m = Tensor::<f32, [64]>::zeros()
            return 0
        }
    "#;
    let ir = module_ir(source, OptimizationLevelSetting::O0);
    let body = function_body(&ir, "main");
    let expected = if cfg!(target_os = "windows") {
        // `_aligned_malloc(size, alignment)`
        "call ptr @_aligned_malloc(i64 256, i64 64)"
    } else {
        // `aligned_alloc(alignment, size)`
        "call ptr @aligned_alloc(i64 64, i64 256)"
    };
    assert!(body.contains(expected), "expected {expected} in:\n{body}");
}

/// Release runs through the handle's own `deleter` field, so a tensor leaving scope
/// performs exactly the release a foreign owner of the handle would.
#[test]
fn a_tensor_is_released_through_its_own_deleter() {
    let source = r#"
        func main() -> i32 {
            val m = Tensor::<f32, [4, 4]>::identity()
            return 0
        }
    "#;
    let ir = module_ir(source, OptimizationLevelSetting::O0);
    let body = function_body(&ir, "main");
    assert!(body.contains("%dlpack.deleter = load ptr"));
    assert!(body.contains("call void %dlpack.deleter(ptr %tensor.drop.handle)"));

    // Each block goes back to the allocator that produced it: the buffer to the
    // release paired with the over-aligned allocation, the structure to plain `free`.
    // On Windows `free` cannot release an over-aligned block, so crossing them
    // corrupts the heap rather than leaking.
    let deleter = function_body(&ir, "__neuro_dlpack_deleter");
    let data_free = deleter
        .find("call void @__neuro_aligned_release(ptr %dlpack.data)")
        .expect("the deleter frees the element buffer");
    let self_free = deleter
        .rfind("call void @__neuro_release(ptr %0)")
        .expect("the deleter frees the structure");
    // The buffer is freed before the structure that names it.
    assert!(data_free < self_free);

    // Two blocks, not three: the control block rides in the structure's allocation, so
    // the deleter releases it without naming it. It also never READS `manager_ctx` —
    // on a handle built elsewhere that field is a foreign producer's context.
    assert!(!deleter.contains("dlpack.control"));
    assert!(!deleter.contains("manager"));
}

/// A reduction's receiver that no binding owns is released once the fold has read it.
/// Without that release `(&a + &b).sum()` leaks the operator's buffer per evaluation.
#[test]
fn a_reduction_releases_an_unbound_receiver() {
    let source = r#"
        func main() -> i32 {
            val a: Tensor<i32, [4]> = [1, 2, 3, 4]
            val b: Tensor<i32, [4]> = [1, 2, 3, 4]
            return (&a + &b).sum()
        }
    "#;
    let ir = module_ir(source, OptimizationLevelSetting::O0);
    let body = function_body(&ir, "main");
    let fold = body
        .find("tensor.reduce.done")
        .expect("the reduction emits its exit block");
    let release = body[fold..]
        .find("dlpack.deleter")
        .map(|at| at + fold)
        .expect("the temporary is released after the fold");
    assert!(release > fold);
}

/// A receiver that IS a binding keeps its single release at scope exit: releasing it at
/// the reduction as well would free the buffer twice.
#[test]
fn a_reduction_leaves_a_bound_receiver_to_its_own_drop() {
    let source = r#"
        func main() -> i32 {
            val a: Tensor<i32, [4]> = [1, 2, 3, 4]
            return a.sum()
        }
    "#;
    let ir = module_ir(source, OptimizationLevelSetting::O0);
    let body = function_body(&ir, "main");
    assert_eq!(
        body.matches("%dlpack.deleter = load ptr").count(),
        1,
        "a bound receiver is released exactly once, in:\n{body}"
    );
}

/// The arena's shape, read off the IR: a pool block is a mark and a restore, and
/// the allocations between them take the bump path instead of libc.
#[test]
fn a_pool_block_marks_and_restores_the_arena() {
    let source = r#"
        func main() -> i32 {
            pool {
                val joined = "a" + "b"
                println(joined)
            }
            return 0
        }
    "#;
    let ir = module_ir(source, OptimizationLevelSetting::O0);
    let body = function_body(&ir, "main");

    let mark = body
        .find("call i64 @__neuro_arena_mark()")
        .expect("entering a pool reads the arena mark");
    let alloc = body
        .find("call ptr @__neuro_arena_alloc(")
        .expect("an allocation written inside the block comes from the arena");
    let release = body
        .find("call void @__neuro_arena_release(")
        .expect("leaving a pool restores the mark");
    assert!(mark < alloc && alloc < release);
}

/// The same allocation outside a pool stays on libc: the bump path is entered by
/// where the code is written, never by what it allocates.
#[test]
fn an_allocation_outside_a_pool_stays_on_the_heap() {
    let source = r#"
        func main() -> i32 {
            val joined = "a" + "b"
            println(joined)
            return 0
        }
    "#;
    let ir = module_ir(source, OptimizationLevelSetting::O0);
    let body = function_body(&ir, "main");
    assert!(body.contains("call ptr @malloc("));
    assert!(!body.contains("__neuro_arena"));
}

/// The routing rule, read off the IR: a store whose place outlives the block
/// bypasses the arena, because the mark restore would leave that buffer dangling.
/// Both statements are written inside the same block, so nothing but the target
/// separates them.
#[test]
fn a_store_that_outlives_the_pool_bypasses_the_arena() {
    let source = r#"
        func main() -> i32 {
            mut outer: string = "x"
            pool {
                mut inner: string = "y"
                inner = "a" + "b"
                outer = "c" + "d"
                println(inner)
                println(outer)
            }
            return 0
        }
    "#;
    let ir = module_ir(source, OptimizationLevelSetting::O0);
    let body = function_body(&ir, "main");

    let arena = body
        .find("call ptr @__neuro_arena_alloc(")
        .expect("the block's own binding keeps the bump path");
    let heap = body[arena..]
        .find("call ptr @malloc(")
        .expect("the binding that outlives the block is routed to libc");
    assert!(
        heap > 0,
        "the routed store must follow the pooled one, in:\n{body}"
    );
}

/// The routing reaches a place the store is only rooted in: `outer.text` is a field
/// of a binding declared before the block, so the buffer written into it outlives
/// the arena exactly as the whole binding would.
#[test]
fn a_store_into_a_field_of_an_outliving_binding_is_routed_too() {
    let source = r#"
        struct Row {
            text: string
        }

        func main() -> i32 {
            mut outer = Row { text: "x" }
            pool {
                outer.text = "c" + "d"
            }
            println(outer.text)
            return 0
        }
    "#;
    let ir = module_ir(source, OptimizationLevelSetting::O0);
    let body = function_body(&ir, "main");
    assert!(
        body.contains("call ptr @malloc("),
        "a field of an outliving binding is not the arena's, in:\n{body}"
    );
}

/// Nested pools share one arena: the inner block restores to its own mark, which
/// is why nesting needs no second chunk and no second offset.
#[test]
fn nested_pools_take_nested_marks() {
    let source = r#"
        func main() -> i32 {
            pool outer {
                val a = "a" + "b"
                pool inner {
                    val b = "c" + "d"
                    println(b)
                }
                println(a)
            }
            return 0
        }
    "#;
    let ir = module_ir(source, OptimizationLevelSetting::O0);
    let body = function_body(&ir, "main");
    assert_eq!(body.matches("call i64 @__neuro_arena_mark()").count(), 2);
    assert_eq!(body.matches("call void @__neuro_arena_release(").count(), 2);
}

/// The in-place guarantee, read off the IR: a compound assignment allocates nothing.
///
/// The update runs against the buffer the target's handle already addresses, so the
/// handle and its `data` pointer are the same values after the statement as before —
/// which is what keeps a pointer held by an optimizer or a foreign DLPack consumer
/// valid. The desugaring this node replaces would build a second tensor here.
#[test]
fn a_tensor_compound_assignment_allocates_nothing() {
    let source = r#"
        func update(w: &mut Tensor<f32, [8, 8]>) { }

        func step(g: &Tensor<f32, [8, 8]>) -> i32 {
            mut w = Tensor::<f32, [8, 8]>::zeros()
            w -= g
            w += g
            return 0
        }

        func main() -> i32 {
            return 0
        }
    "#;
    let ir = module_ir(source, OptimizationLevelSetting::O0);
    let body = function_body(&ir, "step");
    // One allocation for `zeros()`, and none for either update.
    assert_eq!(
        body.matches(&format!("call ptr @{ALIGNED_ALLOC_FN}"))
            .count(),
        1,
        "only the construction allocates:\n{body}"
    );
    // Block labels are defined at the start of a line; the branches naming them are
    // indented, so this counts loops rather than mentions.
    assert_eq!(
        body.matches("\ntensor.op.head").count(),
        2,
        "each update is one counted loop over the buffer:\n{body}"
    );
}

/// A tensor's arithmetic is its element's arithmetic, so an integer update carries the
/// same debug-tier overflow guard the scalar operator does, and a float update does
/// not (IEEE-754 has an answer for every pair).
#[test]
fn an_integer_compound_assignment_keeps_the_scalar_overflow_guard() {
    let source = r#"
        func step(g: &Tensor<i32, [4]>) -> i32 {
            mut w = Tensor::<i32, [4]>::zeros()
            w += g
            return 0
        }

        func main() -> i32 {
            return 0
        }
    "#;
    let ir = module_ir(source, OptimizationLevelSetting::O0);
    let body = function_body(&ir, "step");
    assert!(
        body.contains("@llvm.sadd.with.overflow.i32"),
        "an i32 element overflows the way an i32 scalar does:\n{body}"
    );
}

/// Each element type reaches its own DLPack type code and width.
#[test]
fn every_element_type_carries_its_own_dlpack_dtype() {
    let cases = [
        ("i32", "{ i8 0, i8 32, i16 1 }"),
        ("u8", "{ i8 1, i8 8, i16 1 }"),
        ("f64", "{ i8 2, i8 64, i16 1 }"),
        ("bf16", "{ i8 4, i8 16, i16 1 }"),
        ("bool", "{ i8 6, i8 8, i16 1 }"),
    ];
    for (element, dtype) in cases {
        let source = format!(
            r#"
            func main() -> i32 {{
                val t = Tensor::<{}, [2, 2]>::zeros()
                return 0
            }}
        "#,
            element
        );
        let ir = module_ir(&source, OptimizationLevelSetting::O0);
        let body = function_body(&ir, "main");
        assert!(
            body.contains(&format!("store {{ i8, i8, i16 }} {}", dtype)),
            "`{}` should carry dtype {}",
            element,
            dtype
        );
    }
}

/// A rank-0 tensor has no axis to describe, so DLPack's spelling for a scalar, null
/// `shape` and `strides` with `ndim` 0, is what it gets.
#[test]
fn a_rank_zero_tensor_has_null_shape_and_strides() {
    let source = r#"
        func main() -> i32 {
            val s: Tensor<f32, []> = Tensor::scalar(42.0)
            return 0
        }
    "#;
    let ir = module_ir(source, OptimizationLevelSetting::O0);
    assert!(!ir.contains("__neuro_dlpack_shape_f32_"));
    let body = function_body(&ir, "main");
    assert!(body.contains("store i32 0, ptr %dlpack.field"), "ndim is 0");
}

#[test]
fn panic_diagnostics_are_outlined_out_of_the_hot_function() {
    let source = r#"
        func main() -> i32 {
            val arr: [i32; 3] = [1, 2, 3]
            assert(arr.len() == 3)
            return 0
        }
    "#;

    let ir = module_ir(source, OptimizationLevelSetting::O0);
    let main_body = function_body(&ir, "main");

    assert!(
        !main_body.contains("@abort") && !main_body.contains("@write"),
        "the diagnostic machinery must not remain inline in @main:\n{}",
        main_body
    );
    assert!(
        main_body.contains("call void @neuro.cold.panic.0()"),
        "the failure block must call the outlined thunk:\n{}",
        main_body
    );
    assert!(
        ir.contains("define private void @neuro.cold.panic.0()"),
        "the outlined thunk must be a module-private definition:\n{}",
        ir
    );
}

#[test]
fn outlined_thunks_are_cold_noreturn_and_pinned() {
    let source = r#"
        func main() -> i32 {
            panic("stop")
        }
    "#;

    let ir = module_ir(source, OptimizationLevelSetting::O0);

    // `noinline` is the attribute that actually holds the outlining in place; without
    // it the inliner folds a single-call-site function back into its caller.
    for attribute in ["cold", "noreturn", "noinline", "minsize"] {
        assert!(
            ir.contains(attribute),
            "outlined thunks must carry `{}`:\n{}",
            attribute,
            ir
        );
    }
    assert!(
        ir.contains("declare void @abort() #"),
        "abort must carry an attribute group (cold, noreturn):\n{}",
        ir
    );
}

#[test]
fn a_runtime_panic_message_is_passed_to_the_thunk() {
    // The message is a runtime `string`, so only the constant fragments are baked
    // into the thunk; the fat pointer travels as two arguments.
    let source = r#"
        func main() -> i32 {
            panic("stop")
        }
    "#;

    let ir = module_ir(source, OptimizationLevelSetting::O0);

    assert!(
        ir.contains("define private void @neuro.cold.panic.0(ptr %0, i64 %1)"),
        "the message thunk must take the fat pointer's (ptr, len) pair:\n{}",
        ir
    );
}

#[test]
fn identically_worded_failures_share_one_thunk() {
    // Monomorphization copies a generic body once per type argument, so both copies
    // render the same diagnostic text from the same span.
    let source = r#"
        func checked<T>(value: T) -> T {
            assert(true)
            value
        }

        func main() -> i32 {
            val a = checked(1)
            val b = checked(2.5)
            return a
        }
    "#;

    let ir = module_ir(source, OptimizationLevelSetting::O0);
    let thunks = ir.matches("define private void @neuro.cold.panic.").count();

    assert_eq!(thunks, 1, "the two instances must share one thunk:\n{}", ir);
}

#[test]
fn guard_and_overflow_branches_are_weighted() {
    // At -O0 arithmetic panics on overflow, so this program carries two runtime
    // guards: a bounds check and an overflow check. Both report through the panic
    // machinery, so both have the one guard shape and weight the same edge cold.
    let source = r#"
        func main() -> i32 {
            val arr: [i32; 3] = [1, 2, 3]
            mut i: i32 = 0
            val total = arr[i] + 1
            return total
        }
    "#;

    let ir = module_ir(source, OptimizationLevelSetting::O0);

    assert!(
        ir.contains(r#"!{!"branch_weights", i32 2000, i32 1}"#),
        "a guard's failure edge must be the unlikely one:\n{}",
        ir
    );
    assert!(
        ir.contains("panic: integer overflow at"),
        "an overflow must report a located diagnostic, not a bare trap:\n{}",
        ir
    );
    assert!(
        !ir.contains("@llvm.trap"),
        "nothing may still abort through a silent trap:\n{}",
        ir
    );
}

/// The `string` positions of `name` that a store actually armed.
///
/// A position is PLANNED for every `string` a holder has, and its release is emitted
/// under a flag that starts `false`, so counting releases would count the ones that can
/// never run. What a program owns is what it armed.
fn armed_string_positions(ir: &str, name: &str) -> usize {
    let armed = format!(
        "store i1 true, ptr %{}",
        crate::codegen::drops::STRING_POSITION_FLAG
    );
    function_body(ir, name).matches(&armed).count()
}

/// Calls to `free` in the body of the function named `name`.
/// Releases emitted in `name`. Every release goes through the arena wrapper,
/// which forwards to libc for anything the pool arena does not own.
fn free_calls(ir: &str, name: &str) -> usize {
    function_body(ir, name)
        .matches("call void @__neuro_release(")
        .count()
}

#[test]
fn an_interpolated_temporary_is_freed() {
    // `println` copies the bytes out to fd 1 and keeps none of them, so both buffers
    // the argument cost (the rendered hole and the joined result) are dead on
    // return. A loop around this is what made the leak unbounded.
    let source = r#"
        func main() -> i32 {
            mut i: i32 = 0
            while i < 3 {
                println("line {i}")
                i += 1
            }
            return 0
        }
    "#;

    let ir = module_ir(source, OptimizationLevelSetting::O0);

    assert!(
        free_calls(&ir, "main") >= 2,
        "the rendered hole and the joined string must both be released:\n{}",
        function_body(&ir, "main")
    );
}

#[test]
fn a_borrowed_argument_is_never_freed() {
    // A literal lives in `.rodata`. Handing that pointer to `free` would abort, so
    // the ownership test has to answer `false` for everything it cannot prove.
    let source = r#"
        func main() -> i32 {
            println("line")
            return 0
        }
    "#;

    let ir = module_ir(source, OptimizationLevelSetting::O0);

    assert_eq!(
        free_calls(&ir, "main"),
        0,
        "a `.rodata` literal must not be freed:\n{}",
        function_body(&ir, "main")
    );
}

#[test]
fn a_heap_initialized_string_binding_is_freed_at_scope_exit() {
    // The binding outlives the statement that built it, so its buffer is released by
    // the scope-exit machinery rather than at the point of use.
    let source = r#"
        func main() -> i32 {
            val greeting = "a" + "b"
            println(greeting)
            return 0
        }
    "#;

    let ir = module_ir(source, OptimizationLevelSetting::O0);
    let body = function_body(&ir, "main");

    assert!(
        free_calls(&ir, "main") >= 1,
        "a binding initialized by concatenation owns its buffer:\n{}",
        body
    );
    assert!(
        body.contains("drop.flag"),
        "the release must be flag-guarded, so a moved value is not freed twice:\n{}",
        body
    );
}

#[test]
fn a_reassigned_string_binding_frees_its_prior_buffer() {
    // Two owners pass through one binding, so two buffers are released: the first at
    // the reassignment that displaces it, the second at scope exit.
    let source = r#"
        func main() -> i32 {
            mut s = "a" + "b"
            s = s + "!"
            println(s)
            return 0
        }
    "#;

    let ir = module_ir(source, OptimizationLevelSetting::O0);
    let body = function_body(&ir, "main");

    assert_eq!(
        free_calls(&ir, "main"),
        2,
        "the displaced buffer is released as well as the final one:\n{body}"
    );
    assert_eq!(
        body.matches("store i1 true, ptr %drop.flag,").count(),
        2,
        "the declaration arms the flag and the reassignment re-arms it:\n{body}"
    );
}

#[test]
fn a_string_reassigned_from_a_literal_disowns_its_buffer() {
    // The prior buffer is still released, but the literal that replaces it points at
    // `.rodata`, so the binding must be left un-armed rather than freed again at exit.
    let source = r#"
        func main() -> i32 {
            mut s = "a" + "b"
            s = "plain"
            println(s)
            return 0
        }
    "#;

    let ir = module_ir(source, OptimizationLevelSetting::O0);
    let body = function_body(&ir, "main");

    // The scope-exit release is still emitted; what disowns the binding is the flag
    // the reassignment leaves clear, so the guard at that site never takes its branch.
    assert_eq!(
        body.matches("store i1 true, ptr %drop.flag,").count(),
        1,
        "only the declaration arms the flag; the literal must not re-arm it:\n{body}"
    );
}

#[test]
fn a_reassigned_collection_binding_frees_its_prior_buffer() {
    // A collection owns its buffer by type, so the re-armed flag is unconditional and
    // both the displaced and the final vector are released.
    let source = r#"
        func main() -> i32 {
            mut v: Vec<i32> = Vec::new()
            v.push(1)
            v = Vec::new()
            return v.len() as i32
        }
    "#;

    let ir = module_ir(source, OptimizationLevelSetting::O0);

    assert_eq!(
        free_calls(&ir, "main"),
        2,
        "the displaced vector's buffer is released as well as the final one:\n{}",
        function_body(&ir, "main")
    );
}

#[test]
fn a_borrowed_string_binding_is_not_freed() {
    // Same shape, but the initializer is a literal: nothing was allocated, so nothing
    // may be released.
    let source = r#"
        func main() -> i32 {
            val greeting = "ab"
            println(greeting)
            return 0
        }
    "#;

    let ir = module_ir(source, OptimizationLevelSetting::O0);

    assert_eq!(
        free_calls(&ir, "main"),
        0,
        "a binding holding a literal owns nothing:\n{}",
        function_body(&ir, "main")
    );
}

#[test]
fn a_pass_through_transform_frees_only_what_it_replaced() {
    // `__neuro_pad` returns its input untouched when the text already fills the
    // field, so the result can be the same buffer that was handed in. The release
    // is therefore guarded by a pointer comparison rather than emitted outright.
    let source = r#"
        func main() -> i32 {
            val n = 7
            println("{n:>8}")
            return 0
        }
    "#;

    let ir = module_ir(source, OptimizationLevelSetting::O0);
    let body = function_body(&ir, "main");

    assert!(
        body.contains("interp.same"),
        "a padded hole must compare the two pointers before freeing either:\n{}",
        body
    );
}

#[test]
fn optimization_levels_run_an_ir_pipeline() {
    // Codegen gives every local an `alloca`. Only an IR pass promotes those to SSA,
    // not the `TargetMachine`'s optimization level, which runs none
    // values, so a surviving `alloca` in this loop means no pipeline ran.
    let source = r#"
        func total(n: i32) -> i32 {
            mut sum: i32 = 0
            mut i: i32 = 0
            while i < n {
                sum += i
                i += 1
            }
            return sum
        }

        func main() -> i32 {
            return total(10)
        }
    "#;

    for level in [
        OptimizationLevelSetting::O1,
        OptimizationLevelSetting::O2,
        OptimizationLevelSetting::O3,
    ] {
        let ir = optimized_ir(source, level);
        let body = function_body(&ir, "total");
        assert!(
            !body.contains("alloca"),
            "{:?} left a stack slot unpromoted, so no IR pipeline ran:\n{}",
            level,
            body
        );
    }

    // -O0 deliberately runs no pipeline: its trapping arithmetic and bounds guards
    // must stay exactly where codegen emitted them.
    let unoptimized = optimized_ir(source, OptimizationLevelSetting::O0);
    assert!(
        function_body(&unoptimized, "total").contains("alloca"),
        "-O0 must hand the IR to instruction selection untouched:\n{}",
        unoptimized
    );
}

#[test]
fn the_optimized_module_carries_its_target() {
    // Without a data layout the optimizer cannot reason about the size, alignment,
    // or pointer width of what it transforms.
    let ir = optimized_ir(
        "func main() -> i32 { return 0 }",
        OptimizationLevelSetting::O2,
    );

    assert!(
        ir.contains("target datalayout") && ir.contains("target triple"),
        "the module must name the target it was compiled for:\n{}",
        ir
    );
}

#[test]
fn test_type_mapper_primitives() {
    let context = LLVMContext::create();
    let mapper = TypeMapper::new(&context);

    assert!(mapper.map_type(&Type::I32).is_ok());
    assert!(mapper.map_type(&Type::I64).is_ok());
    assert!(mapper.map_type(&Type::F32).is_ok());
    assert!(mapper.map_type(&Type::F64).is_ok());
    assert!(mapper.map_type(&Type::Bool).is_ok());
    assert!(mapper.map_type(&Type::Void).is_err());
}

#[test]
fn test_type_predicates() {
    assert!(TypeMapper::is_float_type(&Type::F32));
    assert!(TypeMapper::is_float_type(&Type::F64));
    assert!(!TypeMapper::is_float_type(&Type::I32));

    // Test unsigned integer predicate
    assert!(TypeMapper::is_unsigned_int(&Type::U32));
    assert!(!TypeMapper::is_unsigned_int(&Type::I32));
}

#[test]
fn test_compile_simple_function() {
    let source = r#"
        func add(a: i32, b: i32) -> i32 {
            return a + b
        }
    "#;

    let hir = lower(source);
    let result = compile(&hir, OptimizationLevelSetting::O0, source, "test.nr");

    assert!(result.is_ok(), "compilation failed: {:?}", result.err());
    let object_code = result.unwrap();
    assert!(!object_code.is_empty(), "object code should not be empty");
}

#[test]
fn test_compile_milestone_program() {
    let source = r#"
        func add(a: i32, b: i32) -> i32 {
            return a + b
        }

        func main() -> i32 {
            val result = add(5, 3)
            return result
        }
    "#;

    let hir = lower(source);
    let result = compile(&hir, OptimizationLevelSetting::O2, source, "test.nr");

    assert!(result.is_ok(), "compilation failed: {:?}", result.err());
    let object_code = result.unwrap();
    assert!(!object_code.is_empty(), "object code should not be empty");
}

#[test]
fn test_overflow_checks_emit_valid_ir_at_o0() {
    // -O0 routes integer +/-/* through the with-overflow intrinsics and a
    // trap block; module verification must accept the resulting IR.
    let source = r#"
        func main() -> i32 {
            mut x: i32 = 2147483647
            val y: i32 = 1
            val z: i32 = x + y
            return z
        }
    "#;

    let hir = lower(source);
    let result = compile(&hir, OptimizationLevelSetting::O0, source, "test.nr");

    assert!(result.is_ok(), "compilation failed: {:?}", result.err());
    assert!(
        !result.unwrap().is_empty(),
        "object code should not be empty"
    );
}

#[test]
fn test_overflow_wraps_emit_valid_ir_at_o2() {
    // -O2 emits plain wrapping arithmetic (no intrinsic, no trap block).
    let source = r#"
        func main() -> i32 {
            mut x: u8 = 200u8
            val y: u8 = 100u8
            val z: u8 = x + y
            return z as i32
        }
    "#;

    let hir = lower(source);
    let result = compile(&hir, OptimizationLevelSetting::O2, source, "test.nr");

    assert!(result.is_ok(), "compilation failed: {:?}", result.err());
    assert!(
        !result.unwrap().is_empty(),
        "object code should not be empty"
    );
}

#[test]
fn standard_output_is_buffered_rather_than_written_per_call() {
    let source = r#"
        func main() -> i32 {
            println("one")
            return 0
        }
    "#;

    let ir = module_ir(source, OptimizationLevelSetting::O0);
    let main_body = function_body(&ir, "main");

    assert!(
        !main_body.contains("@write("),
        "a print must reach the buffer, not the syscall:\n{}",
        main_body
    );
    assert_eq!(
        main_body.matches("call void @neuro.print.emit(").count(),
        2,
        "println emits its text and its newline into the same buffer:\n{}",
        main_body
    );
    assert!(
        ir.contains("@neuro.print.buffer = private global"),
        "the buffer must be a module-private reservation:\n{}",
        ir
    );
}

#[test]
fn a_module_that_never_prints_reserves_no_output_buffer() {
    // The drain is inserted after every body is generated precisely so that a
    // program with no print keeps its exit paths, and its .bss, untouched.
    let source = r#"
        func main() -> i32 {
            val n: i32 = 41
            return n + 1
        }
    "#;

    let ir = module_ir(source, OptimizationLevelSetting::O0);

    assert!(
        !ir.contains("neuro.print"),
        "the standard-output runtime must not be emitted at all:\n{}",
        ir
    );
}

#[test]
fn every_exit_path_drains_the_output_buffer() {
    // `main` returning and `abort` are the only two ways this language stops
    // running, and `abort` runs no exit hook. Both a panicking `assert` and an
    // overflowing `+` reach the second.
    let source = r#"
        func main() -> i32 {
            mut n: i32 = 2147483647
            println("working")
            assert(n > 0)
            n = n + 1
            return 0
        }
    "#;

    let ir = module_ir(source, OptimizationLevelSetting::O0);

    assert!(
        function_body(&ir, "main").contains("call void @neuro.print.flush()"),
        "main must drain before it returns:\n{}",
        function_body(&ir, "main")
    );
    let drained = ir
        .split("call void @abort()")
        .next()
        .map(|head| head.trim_end().ends_with("call void @neuro.print.flush()"))
        .unwrap_or(false);
    assert!(
        drained,
        "the panic runtime must drain the buffer first:\n{}",
        ir
    );
}

#[test]
fn test_optimization_level_parsing() {
    assert_eq!(
        OptimizationLevelSetting::from_u8(0).unwrap(),
        OptimizationLevelSetting::O0
    );
    assert_eq!(
        OptimizationLevelSetting::from_u8(1).unwrap(),
        OptimizationLevelSetting::O1
    );
    assert_eq!(
        OptimizationLevelSetting::from_u8(2).unwrap(),
        OptimizationLevelSetting::O2
    );
    assert_eq!(
        OptimizationLevelSetting::from_u8(3).unwrap(),
        OptimizationLevelSetting::O3
    );
    assert!(OptimizationLevelSetting::from_u8(4).is_err());
}

/// A statically shaped tensor has its whole shape in its type, so the buffer is a
/// flat row-major array. It lives out of line: the constant lands in `.rodata` and
/// the binding gets an owning copy of it.
#[test]
fn a_tensor_literal_lowers_to_a_flat_row_major_buffer() {
    let source = r#"
        func main() -> i32 {
            val m: Tensor<f32, [2, 3]> = [
                [1.0, 2.0, 3.0],
                [4.0, 5.0, 6.0]
            ]
            return 0
        }
    "#;

    let ir = module_ir(source, OptimizationLevelSetting::O0);
    assert!(
        ir.contains("private constant [6 x float]"),
        "a [2, 3] tensor is a 6-element buffer:\n{ir}"
    );
    assert!(
        ir.contains("float 3.000000e+00") && ir.contains("float 6.000000e+00"),
        "the literal's values must reach the buffer:\n{ir}"
    );
    let body = function_body(&ir, "main");
    assert!(
        body.contains("call ptr @malloc(") && body.contains("@memcpy"),
        "the buffer is allocated and copied into, not held as a value:\n{body}"
    );
}

/// A tensor owns its buffer, so a binding releases it when its scope ends, under the
/// same drop flag a move clears.
#[test]
fn a_tensor_binding_frees_its_buffer_at_scope_exit() {
    let source = r#"
        func main() -> i32 {
            val z = Tensor::<f32, [4, 4]>::zeros()
            return 0
        }
    "#;

    let ir = module_ir(source, OptimizationLevelSetting::O0);
    let body = function_body(&ir, "main");
    assert!(
        body.contains("call void %dlpack.deleter("),
        "the binding releases its tensor through the handle's deleter:\n{body}"
    );
    assert!(
        body.contains("drop.run"),
        "the release is guarded by the binding's drop flag:\n{body}"
    );
}

/// `zeros()` is a constant fill, so nothing per-element survives to run time.
#[test]
fn a_zeros_tensor_lowers_to_a_zero_initializer() {
    let source = r#"
        func main() -> i32 {
            val z = Tensor::<f32, [4, 4]>::zeros()
            return 0
        }
    "#;

    let ir = module_ir(source, OptimizationLevelSetting::O0);
    assert!(
        ir.contains("private constant [16 x float] zeroinitializer"),
        "zeros() is a zero-initialized 16-element buffer:\n{ir}"
    );
}

/// The diagonal is what distinguishes `identity()` from `ones()`, and it is folded
/// at compile time rather than written by a loop.
#[test]
fn an_identity_tensor_carries_ones_only_on_its_diagonal() {
    let source = r#"
        func main() -> i32 {
            val e = Tensor::<i32, [3, 3]>::identity()
            return 0
        }
    "#;

    let ir = module_ir(source, OptimizationLevelSetting::O0);
    assert!(
        ir.contains("[9 x i32] [i32 1, i32 0, i32 0, i32 0, i32 1, i32 0, i32 0, i32 0, i32 1]"),
        "identity() puts ones on the diagonal of a row-major buffer:\n{ir}"
    );
}

/// `.clone()` duplicates the allocation rather than aliasing it, which is what makes
/// the copy independent and both buffer addresses stable.
#[test]
fn a_tensor_clone_duplicates_the_allocation() {
    let source = r#"
        func main() -> i32 {
            val a = Tensor::<f32, [4, 4]>::zeros()
            val b = a.clone()
            return 0
        }
    "#;

    let ir = module_ir(source, OptimizationLevelSetting::O0);
    let body = function_body(&ir, "main");
    assert_eq!(
        body.matches("call ptr @malloc(").count(),
        2,
        "the clone allocates a buffer of its own:\n{body}"
    );
    assert_eq!(
        body.matches("call ptr @memcpy(").count(),
        2,
        "the clone copies the elements rather than aliasing them:\n{body}"
    );
}

/// `random_normal` is the one construction with a runtime cost: a counted loop over
/// the buffer, drawing through the module's own generator.
#[test]
fn random_normal_fills_the_buffer_through_the_module_generator() {
    let source = r#"
        func main() -> i32 {
            val r = Tensor::<f32, [8, 4]>::random_normal(0.0f32, 0.02f32)
            return 0
        }
    "#;

    let ir = module_ir(source, OptimizationLevelSetting::O0);
    assert!(
        ir.contains("@__neuro_rng_state = private global i64"),
        "the generator keeps its state in a private module global:\n{ir}"
    );
    assert!(
        ir.contains("define internal double @__neuro_rng_normal_f64()"),
        "the normal draw is emitted once per module:\n{ir}"
    );
    let body = function_body(&ir, "main");
    assert!(
        body.contains("call double @__neuro_rng_normal_f64()") && body.contains("tensor.rand.head"),
        "the fill is a counted loop over the buffer:\n{body}"
    );
}

/// A chain of concatenations allocates one buffer per `+`, and every buffer but the
/// last is read by the next `+` and then unreachable. Without a release at the operand
/// the chain leaks every intermediate result, which is what `a + b + c` in a loop shows
/// up as. The count is what is asserted: one release for the intermediate, one for the
/// binding that owns the final buffer.
#[test]
fn a_concatenation_chain_releases_its_intermediate_buffers() {
    let source = r#"
        func main() -> i32 {
            val a = "one"
            val s = a + a + a
            return s.len() as i32
        }
    "#;
    let body = module_ir(source, OptimizationLevelSetting::O0);
    let body = function_body(&body, "main");
    assert_eq!(
        body.matches("call void @__neuro_release(").count(),
        2,
        "the intermediate and the bound result are each released once:\n{body}"
    );
}

/// An operand built for a comparison, a `.len()` receiver built for its call, and a
/// concatenation in statement position all hand their buffer to a consumer that copies
/// nothing out of it, so each is released where it is consumed rather than leaking.
#[test]
fn a_string_temporary_is_released_at_the_consumer_that_discards_it() {
    let cases = [
        r#"if a + a == a { return 1 }"#,
        r#"return (a + a).len() as i32"#,
        r#"a + a"#,
    ];
    for case in cases {
        let source = format!(
            r#"
        func main() -> i32 {{
            val a = "one"
            {case}
            return 0
        }}
    "#
        );
        let ir = module_ir(&source, OptimizationLevelSetting::O0);
        let body = function_body(&ir, "main");
        assert!(
            body.contains("call void @__neuro_release("),
            "`{case}` leaves its operand unreleased:\n{body}"
        );
    }
}

/// An owned `string` handed to a parameter the callee only reads is released exactly
/// once, by the place that owns it. The argument loop cleared the caller's drop flag for every by-value
/// argument, while the release after the call reached only an argument that ALLOCATED
/// in place, so a named binding, and a field read through one, arrived at the call owned
/// and left it owned by nobody: one buffer leaked per call.
#[test]
fn regression_a_read_only_argument_releases_the_place_it_came_from() {
    let cases = [
        // A named binding: its own scope is the releaser the move used to disarm.
        r#"
        func size(s: string) -> u64 { s.len() }
        func main() -> i32 {
            val s = "one" + "two"
            return size(s) as i32
        }
        "#,
        // A field read through a binding: the holder releases the position.
        r#"
        struct Doc { title: string }
        func size(s: string) -> u64 { s.len() }
        func main() -> i32 {
            val d = Doc { title: "one" + "two" }
            return size(d.title) as i32
        }
        "#,
        // An argument that allocates in place is released at the call, as before: the
        // flag it never had cannot be the thing that releases it.
        r#"
        func size(s: string) -> u64 { s.len() }
        func main() -> i32 {
            return size("one" + "two") as i32
        }
        "#,
    ];
    for case in cases {
        let ir = module_ir(case, OptimizationLevelSetting::O0);
        let body = function_body(&ir, "main");
        // Every release is emitted under a flag, so counting releases counts the ones
        // that can never run: the leak is a place that is DISARMED without its release
        // having run. One disarm per arming is what a place released exactly once
        // looks like; the extra disarm the move used to emit is the defect.
        assert_eq!(
            body.matches("store i1 true").count(),
            body.matches("store i1 false").count(),
            "an armed `string` place is disarmed only by the drop that releases it, in:\n{body}"
        );
        assert_eq!(
            free_calls(&ir, "main"),
            1,
            "the buffer reaches exactly one release, in:\n{body}"
        );
    }
}

/// A parameter handed on to another read-only parameter, read through `.clone()`, or read
/// under an `as` cast is read only too, so the caller releases the buffer it passed once
/// the call returns. Each shape was counted a retention, and each call leaked the argument.
#[test]
fn a_parameter_passed_on_to_a_read_only_one_is_read_only() {
    let cases = [
        r#"
        func size(s: string) -> u64 { s.len() }
        func relay(s: string) -> u64 { size(s) }
        func main() -> i32 {
            return relay("one" + "two") as i32
        }
        "#,
        r#"
        func size(s: string) -> i64 { s.clone().len() as i64 }
        func main() -> i32 {
            return size("one" + "two") as i32
        }
        "#,
    ];
    for case in cases {
        let ir = module_ir(case, OptimizationLevelSetting::O0);
        assert_eq!(
            free_calls(&ir, "main"),
            1,
            "the argument is released once the call returns, in:\n{ir}"
        );
    }
}

/// `string.clone()` is a deep copy, so the clone is a buffer of its own that its binding
/// releases, beside the original's.
#[test]
fn regression_bug_057_a_string_clone_is_an_owned_copy() {
    let source = r#"
        func main() -> i32 {
            val a = "one" + "two"
            val b = a.clone()
            return b.len() as i32
        }
    "#;
    let ir = module_ir(source, OptimizationLevelSetting::O0);
    let body = function_body(&ir, "main");
    assert!(
        body.contains("str.dup"),
        "a clone copies the bytes into a fresh buffer:\n{body}"
    );
    assert_eq!(
        free_calls(&ir, "main"),
        2,
        "the original and the clone are each released, in:\n{body}"
    );

    // A receiver built only to be cloned is dead once its bytes are copied.
    let temporary = r#"
        func main() -> i32 {
            val a = "one"
            val c = (a + a).clone()
            return c.len() as i32
        }
    "#;
    let ir = module_ir(temporary, OptimizationLevelSetting::O0);
    assert_eq!(
        free_calls(&ir, "main"),
        2,
        "the temporary receiver and the clone are each released, in:\n{}",
        function_body(&ir, "main")
    );
}

/// A `string` binding owns what a move hands it and what a reassignment hands it,
/// whatever it was initialized from. `val u = t` and `s = t` carry the source's runtime
/// flag over before the move clears it, and a `mut` binding initialized from a literal
/// still has a flag for a later `s = a + b` to arm. Each shape used to leave the buffer
/// owned by nobody.
#[test]
fn a_moved_or_reassigned_string_binding_owns_its_buffer() {
    let moves = [
        r#"
        func main() -> i32 {
            val a = "one"
            val t = a + a
            val u = t
            return u.len() as i32
        }
        "#,
        r#"
        func main() -> i32 {
            val a = "one"
            mut s = a + a
            val t = a + a
            s = t
            return s.len() as i32
        }
        "#,
    ];
    for case in moves {
        let ir = module_ir(case, OptimizationLevelSetting::O0);
        let body = function_body(&ir, "main");
        assert!(
            body.contains("str.owns"),
            "the move carries the source's ownership flag, in:\n{body}"
        );
    }
    let reassigned = r#"
        func main() -> i32 {
            val a = "one"
            mut s = "lit"
            s = a + a
            return s.len() as i32
        }
    "#;
    let ir = module_ir(reassigned, OptimizationLevelSetting::O0);
    assert!(
        free_calls(&ir, "main") > 0,
        "a `mut` binding begun from a literal releases what a reassignment gives it:\n{}",
        function_body(&ir, "main")
    );
}

/// A holder moved whole (`val q = p`, `q = p`) hands each `string` position's runtime
/// ownership flag to the holder that takes it. The positions were armed only from the
/// initializer's literal shape, so a moved holder's buffers were owned by nobody.
#[test]
fn a_moved_holder_hands_over_its_string_positions() {
    let cases = [
        r#"
        struct Entry { label: string }
        func main() -> i32 {
            val a = "one"
            val p = Entry { label: a + a }
            val q = p
            return q.label.len() as i32
        }
        "#,
        r#"
        struct Entry { label: string }
        func main() -> i32 {
            val a = "one"
            mut q = Entry { label: "x" }
            val p = Entry { label: a + a }
            q = p
            return q.label.len() as i32
        }
        "#,
    ];
    for case in cases {
        let ir = module_ir(case, OptimizationLevelSetting::O0);
        let body = function_body(&ir, "main");
        assert!(
            body.contains("held.str.owns"),
            "the move carries the position's ownership flag, in:\n{body}"
        );
    }
}

/// A `string` binding moved into a literal position (`Entry { label: t }`, `(t, 1)`,
/// `[t, u]`, a nested literal) hands its runtime ownership flag to the holder's position,
/// and a functional update hands over the flags of the positions it takes from its base.
/// The literal armed only positions whose own expression allocated, so each of these
/// buffers was owned by nobody.
#[test]
fn a_string_moved_into_a_literal_position_is_owned_by_the_holder() {
    let fields = [
        r#"
        struct Entry { label: string }
        func main() -> i32 {
            val a = "one"
            val t = a + a
            val e = Entry { label: t }
            return e.label.len() as i32
        }
        "#,
        r#"
        func main() -> i32 {
            val a = "one"
            val t = a + a
            val pair = (t, 1)
            return pair.0.len() as i32
        }
        "#,
        r#"
        struct Entry { label: string }
        struct Pair { left: Entry }
        func main() -> i32 {
            val a = "one"
            val t = a + a
            mut p = Pair { left: Entry { label: "x" } }
            p = Pair { left: Entry { label: t } }
            return p.left.label.len() as i32
        }
        "#,
    ];
    for case in fields {
        let ir = module_ir(case, OptimizationLevelSetting::O0);
        let body = function_body(&ir, "main");
        assert!(
            body.contains("%str.owns"),
            "the move carries the source binding's ownership flag, in:\n{body}"
        );
    }
    let update = r#"
        struct Entry { id: i32, label: string }
        func main() -> i32 {
            val a = "one"
            val base = Entry { id: 1, label: a + a }
            val next = Entry { id: 2, ..base }
            return next.label.len() as i32
        }
    "#;
    let ir = module_ir(update, OptimizationLevelSetting::O0);
    let body = function_body(&ir, "main");
    assert!(
        body.contains("held.str.owns"),
        "the update carries the base position's ownership flag, in:\n{body}"
    );
}

/// `String::to_string` copies the builder's bytes into a buffer of their own on every
/// call, so a binding initialized from it owns that buffer and releases it at scope
/// exit, exactly as one initialized from `+` does. A user `to_string` on a struct is
/// not the builder's and must not be mistaken for it.
#[test]
fn the_builder_copy_out_is_an_owned_string_and_a_user_method_is_not() {
    let owned = r#"
        func main() -> i32 {
            mut b = String::new()
            b.push_str("text")
            val s = b.to_string()
            return s.len() as i32
        }
    "#;
    let ir = module_ir(owned, OptimizationLevelSetting::O0);
    let body = function_body(&ir, "main");
    assert_eq!(
        body.matches("call void @__neuro_release(").count(),
        2,
        "the copy and the builder's own buffer are each released:\n{body}"
    );

    let shadowed = r#"
        struct Tag { id: i32 }

        impl Tag {
            func to_string(&self) -> string { "tag" }
        }

        func main() -> i32 {
            val t = Tag { id: 1 }
            val s = t.to_string()
            return s.len() as i32
        }
    "#;
    let ir = module_ir(shadowed, OptimizationLevelSetting::O0);
    let body = function_body(&ir, "main");
    assert!(
        !body.contains("call void @__neuro_release("),
        "a user `to_string` returns a literal and owns nothing:\n{body}"
    );
}

/// A holder owns what it holds, and a `string` position is a position like any
/// other once the store into it proved it allocated. The type cannot prove it, so the
/// literal in the same field must arm nothing.
#[test]
fn a_struct_field_releases_the_buffer_stored_into_it() {
    let owned = r#"
        struct Label { text: string, id: i32 }

        func main() -> i32 {
            val a = "left"
            val b = "right"
            val l = Label { text: a + b, id: 1 }
            return l.text.len() as i32
        }
    "#;
    let ir = module_ir(owned, OptimizationLevelSetting::O0);
    let body = function_body(&ir, "main");
    assert_eq!(
        free_calls(&ir, "main"),
        1,
        "the field's buffer is released once, when the holder is destroyed:\n{body}"
    );
    assert_eq!(
        armed_string_positions(&ir, "main"),
        1,
        "the field's position is armed by the store that allocated into it:\n{body}"
    );

    // A disarmed position still emits its flag-guarded release, so the literal case is
    // read off the flag rather than off the call: the position is planned and never
    // armed, and the release under it can therefore never run.
    let borrowed = r#"
        struct Label { text: string, id: i32 }

        func main() -> i32 {
            val l = Label { text: "static", id: 1 }
            return l.text.len() as i32
        }
    "#;
    assert_eq!(
        armed_string_positions(&module_ir(borrowed, OptimizationLevelSetting::O0), "main"),
        0,
        "a `.rodata` literal in the same field is owned by nobody and freed by nobody"
    );
}

/// The positions an aggregate exposes are uniform, so an array element and a
/// tuple element take the same treatment as a field, at any depth.
#[test]
fn an_element_and_a_nested_field_release_their_buffers() {
    let source = r#"
        struct Inner { text: string }
        struct Outer { inner: Inner }

        func main() -> i32 {
            val a = "x"
            val pair = (a + a, 7)
            val cells = [a + a, a + a]
            val nested = Outer { inner: Inner { text: a + a } }
            return (pair.0.len() + cells[0].len() + nested.inner.text.len()) as i32
        }
    "#;
    let ir = module_ir(source, OptimizationLevelSetting::O0);
    assert_eq!(
        free_calls(&ir, "main"),
        4,
        "one release per stored buffer: a tuple element, two array elements, and a \
         field one level down:\n{}",
        function_body(&ir, "main")
    );
}

/// A field assignment is a drop site: the displaced buffer goes, and the
/// incoming one is owned only if it was allocated.
#[test]
fn a_field_assignment_releases_what_it_displaces() {
    let source = r#"
        struct Label { text: string }

        func main() -> i32 {
            val a = "x"
            mut l = Label { text: a + a }
            l.text = a + a + a
            return l.text.len() as i32
        }
    "#;
    let ir = module_ir(source, OptimizationLevelSetting::O0);
    let body = function_body(&ir, "main");
    // The displaced buffer, the intermediate the second concatenation built, and the
    // final value the holder carries to its scope exit.
    assert_eq!(
        free_calls(&ir, "main"),
        3,
        "the displaced buffer and the replacement are both released:\n{body}"
    );

    let to_literal = r#"
        struct Label { text: string }

        func main() -> i32 {
            val a = "x"
            mut l = Label { text: a + a }
            l.text = "static"
            return l.text.len() as i32
        }
    "#;
    let ir = module_ir(to_literal, OptimizationLevelSetting::O0);
    assert_eq!(
        armed_string_positions(&ir, "main"),
        1,
        "only the first store arms the field; the literal replacing it owns nothing:\n{}",
        function_body(&ir, "main")
    );
}

/// The caller of a function that allocates on every return path owns what it gets
/// back, which is the one ownership question no expression at the call site can answer.
#[test]
fn a_call_that_allocates_on_every_path_hands_the_buffer_to_its_caller() {
    let source = r#"
        func joined(a: string, b: string) -> string {
            return a + b
        }

        func main() -> i32 {
            val s = joined("l", "r")
            return s.len() as i32
        }
    "#;
    let ir = module_ir(source, OptimizationLevelSetting::O0);
    assert_eq!(
        free_calls(&ir, "main"),
        1,
        "the caller releases the buffer at its own scope exit:\n{}",
        function_body(&ir, "main")
    );
    assert_eq!(
        free_calls(&ir, "joined"),
        0,
        "the callee released nothing: it handed the buffer over"
    );

    let one_borrowed_path = r#"
        func maybe(a: string, flag: bool) -> string {
            if flag {
                return "static"
            }
            return a + a
        }

        func main() -> i32 {
            val s = maybe("l", true)
            return s.len() as i32
        }
    "#;
    assert_eq!(
        free_calls(
            &module_ir(one_borrowed_path, OptimizationLevelSetting::O0),
            "main"
        ),
        0,
        "one path returning `.rodata` disqualifies the function: freeing it would abort"
    );
}

/// A tail `if`, `match` or block exits through each branch's tail, so a function whose
/// every branch allocates is a producer like one whose every `return` does. One branch
/// yielding a literal disqualifies it exactly as one literal `return` would.
#[test]
fn a_branching_tail_allocates_when_every_branch_does() {
    let branches = [
        r#"
        func picked(a: string, flag: bool) -> string {
            if flag { a + "x" } else if a.len() > 3 { a + a } else { "{a}!" }
        }
        func main() -> i32 {
            val s = picked("l", true)
            return s.len() as i32
        }
        "#,
        r#"
        func picked(a: string, n: i32) -> string {
            match n {
                0 => a + "0",
                _ => {
                    val k = n * 2
                    a + "{k}"
                }
            }
        }
        func main() -> i32 {
            val s = picked("l", 1)
            return s.len() as i32
        }
        "#,
    ];
    for case in branches {
        let ir = module_ir(case, OptimizationLevelSetting::O0);
        assert_eq!(
            free_calls(&ir, "main"),
            1,
            "the caller releases what every branch allocated:\n{}",
            function_body(&ir, "main")
        );
    }
    let one_literal = r#"
        func picked(a: string, flag: bool) -> string {
            if flag { "static" } else { a + a }
        }
        func main() -> i32 {
            val s = picked("l", true)
            return s.len() as i32
        }
    "#;
    assert_eq!(
        free_calls(
            &module_ir(one_literal, OptimizationLevelSetting::O0),
            "main"
        ),
        0,
        "a literal branch disqualifies the function: freeing it would abort"
    );
}

/// A buffer handed to a parameter the callee only reads is dead when the call returns,
/// so the caller releases it there.
#[test]
fn an_owned_argument_to_a_reading_parameter_is_released_after_the_call() {
    let reads = r#"
        func show(s: string) {
            println(s)
        }

        func main() -> i32 {
            val a = "x"
            show(a + a)
            return 0
        }
    "#;
    let ir = module_ir(reads, OptimizationLevelSetting::O0);
    let body = function_body(&ir, "main");
    let call = body.find("call void @show").expect("the call is emitted");
    let release = body[call..]
        .find("call void @__neuro_release(")
        .expect("the argument is released after the call");
    assert_eq!(
        free_calls(&ir, "main"),
        1,
        "exactly one release, and it is the argument's:\n{body}"
    );
    assert!(release > 0, "the release follows the call:\n{body}");

    let retains = r#"
        struct Label { text: string }

        func keep(s: string) -> Label {
            return Label { text: s }
        }

        func main() -> i32 {
            val a = "x"
            val l = keep(a + a)
            return l.text.len() as i32
        }
    "#;
    let ir = module_ir(retains, OptimizationLevelSetting::O0);
    let body = function_body(&ir, "main");
    let entry = &body[..body.find("\ndrop.run:").unwrap_or(body.len())];
    assert!(
        !entry.contains("call void @__neuro_release("),
        "a callee that stores the parameter keeps the buffer alive past the call, so \
         the caller must not release it there:\n{body}"
    );
    assert_eq!(
        armed_string_positions(&ir, "main"),
        0,
        "nor at the holder it was stored into: the buffer reached the field through a \
         call, which proves nothing about who owns it:\n{body}"
    );
}

/// A collection of `string` elements copies the bytes into slots of its own and walks
/// those slots when it is destroyed, so no element it holds outlives it unreleased.
#[test]
fn a_string_collection_releases_the_elements_it_owns() {
    let source = r#"
        func main() -> i32 {
            mut v: Vec<string> = Vec::new()
            v.push("one")
            return v.len() as i32
        }
    "#;
    let ir = module_ir(source, OptimizationLevelSetting::O0);
    assert!(
        ir.contains("define private void @__neuro_vec_drop_elems_string("),
        "the instantiation's element-release helper is emitted:\n{ir}"
    );
    let body = function_body(&ir, "main");
    assert!(
        body.contains("call void @__neuro_vec_drop_elems_string("),
        "and called before the buffer is freed:\n{body}"
    );
}

/// A collection whose elements are `Copy` owns nothing beyond its buffer, so it emits
/// no element walk at all.
#[test]
fn a_copy_element_collection_emits_no_element_release() {
    let source = r#"
        func main() -> i32 {
            mut v: Vec<i32> = Vec::new()
            v.push(1)
            return v.len() as i32
        }
    "#;
    let ir = module_ir(source, OptimizationLevelSetting::O0);
    assert!(
        !ir.contains("drop_elems"),
        "a `Vec<i32>` costs exactly what it did before:\n{ir}"
    );
}

/// An element read hands back a copy, which makes the reader its owner: the binding is
/// registered for release the same way a concatenation's result is, and the collection
/// keeps the slot it still holds.
#[test]
fn an_element_read_is_owned_by_the_reader() {
    let source = r#"
        func main() -> i32 {
            mut v: Vec<string> = Vec::new()
            v.push("one")
            val s = v[0]
            return s.len() as i32
        }
    "#;
    let ir = module_ir(source, OptimizationLevelSetting::O0);
    let body = function_body(&ir, "main");
    assert!(
        body.contains("str.dup"),
        "the read copies the bytes out of the slot:\n{body}"
    );
    assert_eq!(
        body.matches("call void @__neuro_release(").count(),
        2,
        "the copy and the collection's buffer are each released once, and the element \
         walk releases the slot through its own helper:\n{body}"
    );
}

/// An insertion of a binding copies rather than taking the binding's buffer, which is
/// what leaves the source usable afterwards and keeps a `.rodata` literal out of the
/// release path.
#[test]
fn an_insertion_copies_instead_of_taking_the_operand() {
    let source = r#"
        func main() -> i32 {
            val a = "one"
            mut v: Vec<string> = Vec::new()
            v.push(a)
            return a.len() as i32
        }
    "#;
    let ir = module_ir(source, OptimizationLevelSetting::O0);
    let body = function_body(&ir, "main");
    assert!(
        body.contains("str.dup"),
        "the slot takes a copy of the operand's bytes:\n{body}"
    );
}

/// An operand the expression itself allocated is adopted instead of copied: there is no
/// second owner to keep it alive, so a copy would allocate twice and free once.
#[test]
fn an_insertion_adopts_a_buffer_built_for_it() {
    let source = r#"
        func main() -> i32 {
            val a = "one"
            mut v: Vec<string> = Vec::new()
            v.push(a + a)
            return v.len() as i32
        }
    "#;
    let ir = module_ir(source, OptimizationLevelSetting::O0);
    let body = function_body(&ir, "main");
    assert!(
        !body.contains("str.dup"),
        "a concatenation built for the push is stored as it is:\n{body}"
    );
}
