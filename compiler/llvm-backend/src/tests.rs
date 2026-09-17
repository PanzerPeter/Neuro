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
