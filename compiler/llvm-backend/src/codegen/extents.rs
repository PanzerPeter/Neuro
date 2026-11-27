// Run-time extents of tensors with dynamic (`?`) axes.
//
// A static axis is part of the type, so every size and stride of its tensor is a constant. A
// dynamic axis is known only when the program runs. Its handle carries the extents and strides
// in the `shape` and `strides` arrays DLPack reads, which for a dynamic type are a per-value
// block (`dlpack.rs`). The helpers here read those DLPack fields and never the type's shape, so
// one body serves every size. A static handle widened to a dynamic type reads the same way, its
// arrays being the shared constants.
//
// Every extent a construction, an external result or a shape cast needs is a `HirExtent`, which
// `eval_extent` evaluates. The external-body wrapper and the shape cast share that one evaluator.

use inkwell::IntPredicate;
use inkwell::values::{BasicValueEnum, IntValue, PointerValue};
use neuro_hir::{HirExpr, HirExtent};

use crate::codegen::alias::MemoryClass;
use crate::codegen::context::CodegenContext;
use crate::codegen::dlpack::DLPACK_DATA_ALIGN;
use crate::errors::{CodegenError, CodegenResult};
use crate::types::{Type, is_dynamic};

/// What a size that does not fit in the byte count reports.
const SIZE_OVERFLOW: &str = "tensor size overflows";

/// The element count and the byte length of a tensor's element run, unpadded, as `i64`s.
#[derive(Clone, Copy)]
pub(crate) struct ElementRun<'ctx> {
    pub(crate) count: IntValue<'ctx>,
    pub(crate) bytes: IntValue<'ctx>,
}

/// One operand an extent expression may read, at the position it takes among a function's
/// parameters (or, for a shape cast, the receiver at position 0).
pub(crate) enum ExtentOperand<'ctx> {
    /// A tensor: its handle and its declared shape. A `&Tensor` is loaded to its handle first.
    Tensor {
        handle: PointerValue<'ctx>,
        shape: Vec<Option<usize>>,
    },
    /// An integer, with whether its type is signed, which decides how it widens to `u64`.
    Integer { value: IntValue<'ctx>, signed: bool },
    /// Any other operand. No extent reads one, so an extent that does is a compiler bug.
    Other,
}

impl<'ctx> CodegenContext<'ctx> {
    /// One `i64` per axis of `shape`: the constant of a static axis, and for each dynamic one
    /// the next of `dims`, its run-time extent. `dims` carries exactly one value per dynamic
    /// axis, in axis order.
    pub(crate) fn axis_extents(
        &self,
        shape: &[Option<usize>],
        dims: &[IntValue<'ctx>],
    ) -> CodegenResult<Vec<IntValue<'ctx>>> {
        let open = shape.iter().filter(|extent| extent.is_none()).count();
        if open != dims.len() {
            return Err(CodegenError::InternalError(format!(
                "a tensor with {open} dynamic axis(es) was given {} run-time extent(s)",
                dims.len()
            )));
        }
        let i64_type = self.context.i64_type();
        let mut open_dims = dims.iter().copied();
        let mut extents = Vec::with_capacity(shape.len());
        for extent in shape {
            extents.push(match extent {
                Some(extent) => i64_type.const_int(*extent as u64, false),
                // The count was checked above, so every dynamic axis finds its value.
                None => open_dims.next().ok_or_else(|| {
                    CodegenError::InternalError("a dynamic axis has no run-time extent".into())
                })?,
            });
        }
        Ok(extents)
    }

    /// The element count and byte length of a `tensor_ty` buffer whose dynamic axes have the
    /// run-time extents `dims`. A static type's run is a constant and checks nothing. A dynamic
    /// one is multiplied out under an overflow trap located at `offset`, the source position the
    /// panic names, and its byte length must leave room for the padding `alloc_host_buffer` adds.
    pub(crate) fn element_run(
        &mut self,
        tensor_ty: &Type,
        dims: &[IntValue<'ctx>],
        offset: usize,
    ) -> CodegenResult<ElementRun<'ctx>> {
        let Type::Tensor { element, shape } = tensor_ty else {
            return Err(CodegenError::InternalError(
                "a tensor's element run is only sized for a tensor type".to_string(),
            ));
        };
        let i64_type = self.context.i64_type();
        let extents = self.axis_extents(shape, dims)?;
        if !is_dynamic(shape) {
            let count: u64 = shape.iter().flatten().map(|d| *d as u64).product();
            return Ok(ElementRun {
                count: i64_type.const_int(count, false),
                bytes: i64_type.const_int(self.type_mapper.tensor_buffer_bytes(tensor_ty)?, false),
            });
        }

        let elem_bytes = i64_type.const_int(self.type_mapper.tensor_element_bytes(element)?, false);
        let mut count = i64_type.const_int(1, false);
        for extent in &extents {
            count = self.checked_mul(count, *extent, offset)?;
        }
        let bytes = self.checked_mul(count, elem_bytes, offset)?;
        let limit = i64_type.const_int(u64::MAX - (DLPACK_DATA_ALIGN - 1), false);
        let fits =
            self.builder
                .build_int_compare(IntPredicate::ULE, bytes, limit, "tensor.size.fits")?;
        self.codegen_guard_or_panic(fits, SIZE_OVERFLOW, offset)?;
        Ok(ElementRun { count, bytes })
    }

    /// `lhs * rhs` as an unsigned `i64`, trapping at `offset` when the product does not fit.
    fn checked_mul(
        &mut self,
        lhs: IntValue<'ctx>,
        rhs: IntValue<'ctx>,
        offset: usize,
    ) -> CodegenResult<IntValue<'ctx>> {
        let (product, overflowed) = self.emit_with_overflow("llvm.umul.with.overflow", lhs, rhs)?;
        let fits = self
            .builder
            .build_not(overflowed, "tensor.size.no_overflow")?;
        self.codegen_guard_or_panic(fits, SIZE_OVERFLOW, offset)?;
        Ok(product)
    }

    /// The row-major element stride of each axis of a tensor with `extents`: the product of the
    /// extents below it. Constants fold, so a static tensor's strides are constants too.
    pub(crate) fn row_major_values(
        &self,
        extents: &[IntValue<'ctx>],
    ) -> CodegenResult<Vec<IntValue<'ctx>>> {
        let i64_type = self.context.i64_type();
        let mut strides = vec![i64_type.const_int(1, false); extents.len()];
        for axis in (0..extents.len().saturating_sub(1)).rev() {
            strides[axis] = self.builder.build_int_mul(
                strides[axis + 1],
                extents[axis + 1],
                "extent.stride",
            )?;
        }
        Ok(strides)
    }

    /// `value`, or 1 when it is zero. A constant folds; a run-time zero is replaced, so a value
    /// used as a divisor or a size never reaches an instruction that traps on zero.
    pub(crate) fn at_least_one(&self, value: IntValue<'ctx>) -> CodegenResult<IntValue<'ctx>> {
        let i64_type = self.context.i64_type();
        if let Some(constant) = value.get_zero_extended_constant() {
            return Ok(i64_type.const_int(constant.max(1), false));
        }
        let is_zero = self.builder.build_int_compare(
            IntPredicate::EQ,
            value,
            i64_type.const_zero(),
            "extent.is_zero",
        )?;
        Ok(self
            .builder
            .build_select(
                is_zero,
                i64_type.const_int(1, false),
                value,
                "extent.nonzero",
            )?
            .into_int_value())
    }

    /// Evaluate `extent` against `operands`, the run-time value of each operand the function's
    /// parameters name (a shape cast's receiver is operand 0). Division is floored; every
    /// divisor is nonzero, which lowering has established.
    pub(crate) fn eval_extent(
        &mut self,
        extent: &HirExtent,
        operands: &[ExtentOperand<'ctx>],
    ) -> CodegenResult<IntValue<'ctx>> {
        let i64_type = self.context.i64_type();
        Ok(match extent {
            HirExtent::Axis { operand, axis } => match operands.get(*operand) {
                Some(ExtentOperand::Tensor { handle, shape }) => {
                    self.handle_extent(*handle, shape, *axis)?
                }
                _ => {
                    return Err(CodegenError::InternalError(format!(
                        "an extent reads axis {axis} of operand {operand}, which is no tensor"
                    )));
                }
            },
            HirExtent::Scalar(operand) => match operands.get(*operand) {
                Some(ExtentOperand::Integer { value, signed }) => self
                    .builder
                    .build_int_cast_sign_flag(*value, i64_type, *signed, "extent.scalar")?,
                _ => {
                    return Err(CodegenError::InternalError(format!(
                        "an extent reads operand {operand} as an integer, which it is not"
                    )));
                }
            },
            HirExtent::Constant(value) => i64_type.const_int(*value, false),
            HirExtent::Add(lhs, rhs) => {
                let (lhs, rhs) = (
                    self.eval_extent(lhs, operands)?,
                    self.eval_extent(rhs, operands)?,
                );
                self.builder.build_int_add(lhs, rhs, "extent.add")?
            }
            HirExtent::Sub(lhs, rhs) => {
                let (lhs, rhs) = (
                    self.eval_extent(lhs, operands)?,
                    self.eval_extent(rhs, operands)?,
                );
                self.builder.build_int_sub(lhs, rhs, "extent.sub")?
            }
            HirExtent::Mul(lhs, rhs) => {
                let (lhs, rhs) = (
                    self.eval_extent(lhs, operands)?,
                    self.eval_extent(rhs, operands)?,
                );
                self.builder.build_int_mul(lhs, rhs, "extent.mul")?
            }
            HirExtent::Div(lhs, rhs) => {
                let (lhs, rhs) = (
                    self.eval_extent(lhs, operands)?,
                    self.eval_extent(rhs, operands)?,
                );
                self.builder
                    .build_int_unsigned_div(lhs, rhs, "extent.div")?
            }
        })
    }

    /// Lower `TensorExtent`: the extent of axis `axis` of the tensor `tensor` borrows, as a
    /// `u64`. Borrowing consumes nothing. A static axis is the type's own extent, and a dynamic
    /// one is read from the handle.
    pub(crate) fn codegen_tensor_extent(
        &mut self,
        tensor: &HirExpr,
        axis: usize,
    ) -> CodegenResult<BasicValueEnum<'ctx>> {
        let tensor_ty = Type::from_hir(&tensor.ty);
        let borrowed = self.codegen_expr(tensor)?;
        let Type::Tensor { shape, .. } = tensor_ty.referent() else {
            return Err(CodegenError::InternalError(
                "a tensor extent reads a tensor borrow".to_string(),
            ));
        };
        let extent = match shape.get(axis) {
            Some(Some(extent)) => self.context.i64_type().const_int(*extent as u64, false),
            Some(None) => {
                let BasicValueEnum::PointerValue(borrowed) = borrowed else {
                    return Err(CodegenError::InternalError(
                        "a tensor borrow does not lower to a pointer".to_string(),
                    ));
                };
                let handle = self.tensor_value_handle(borrowed, &tensor_ty)?;
                self.handle_extent(handle, shape, axis)?
            }
            None => {
                return Err(CodegenError::InternalError(format!(
                    "axis {axis} is outside a rank-{} tensor",
                    shape.len()
                )));
            }
        };
        Ok(extent.into())
    }

    /// The handle a tensor value holds: the value itself for an owned tensor, and for a
    /// `&Tensor`, which is the address of a cell holding the handle, the handle that cell holds.
    pub(crate) fn tensor_value_handle(
        &self,
        value: PointerValue<'ctx>,
        tensor_ty: &Type,
    ) -> CodegenResult<PointerValue<'ctx>> {
        if !matches!(tensor_ty, Type::Reference { .. }) {
            return Ok(value);
        }
        let handle = self.builder.build_load(
            self.context.ptr_type(inkwell::AddressSpace::default()),
            value,
            "tensor.recv.handle",
        )?;
        Ok(self
            .tag_load(handle, MemoryClass::TensorHandle)?
            .into_pointer_value())
    }
}

#[cfg(test)]
mod tests {
    use inkwell::context::Context;
    use neuro_hir::{
        AxisNames, HirExpr, HirExprKind, HirExtent, HirFunction, HirItem, HirParam, HirProgram,
        HirStmt, HirTarget, HirTensorAxis, HirType,
    };
    use shared_types::{Literal, Span};

    use crate::codegen::context::ALIGNED_ALLOC_FN;
    use crate::tests::source_map;
    use crate::{BodyMemory, ExternalBodies, GpuVendor, OptimizationLevelSetting, build_module};

    fn span() -> Span {
        Span::new(0, 1)
    }

    fn expr(kind: HirExprKind, ty: HirType) -> HirExpr {
        HirExpr::new(kind, ty, span())
    }

    fn var(name: &str, ty: HirType) -> HirExpr {
        expr(HirExprKind::Variable(name.to_string()), ty)
    }

    fn u64_var(name: &str) -> HirExpr {
        var(name, HirType::U64)
    }

    fn tensor(shape: &[Option<usize>]) -> HirType {
        HirType::Tensor {
            element: Box::new(HirType::F32),
            shape: shape.to_vec(),
            names: AxisNames(vec![None; shape.len()]),
        }
    }

    fn borrow(ty: HirType) -> HirType {
        HirType::Reference {
            inner: Box::new(ty),
            mutable: false,
        }
    }

    fn ret(value: HirExpr) -> HirStmt {
        HirStmt::Return {
            value: Some(value),
            span: span(),
        }
    }

    fn function(
        name: &str,
        params: Vec<(&str, HirType)>,
        return_type: HirType,
        body: Vec<HirStmt>,
    ) -> HirFunction {
        HirFunction {
            name: name.to_string(),
            params: params
                .into_iter()
                .map(|(name, ty)| HirParam {
                    name: name.to_string(),
                    ty,
                    span: span(),
                })
                .collect(),
            return_type,
            body,
            target: HirTarget::Host,
            span: span(),
            result_extents: Vec::new(),
        }
    }

    /// The module `functions` lower to, with `external` bodies linked in.
    fn module_ir(functions: Vec<HirFunction>, external: &[ExternalBodies]) -> String {
        let program = HirProgram {
            items: functions.into_iter().map(HirItem::Function).collect(),
        };
        let context = Context::create();
        let codegen = build_module(
            &context,
            &program,
            OptimizationLevelSetting::O0,
            &source_map("extents.nr", ""),
            external,
            GpuVendor::Nvidia,
            None,
        )
        .expect("a hand-built module should lower");
        codegen.module.print_to_string().to_string()
    }

    /// The text of the definition of `name` in `ir`, up to its closing brace.
    fn definition<'a>(ir: &'a str, name: &str) -> &'a str {
        let header = format!("@{name}(");
        let start = ir
            .lines()
            .scan(0, |offset, line| {
                let at = *offset;
                *offset += line.len() + 1;
                Some((at, line))
            })
            .find(|(_, line)| line.starts_with("define") && line.contains(&header))
            .map(|(at, line)| at + line.find(&header).unwrap_or(0))
            .unwrap_or_else(|| panic!("no definition of `{name}` in:\n{ir}"));
        let rest = &ir[start..];
        match rest.find("\n}") {
            Some(end) => &rest[..end],
            None => rest,
        }
    }

    /// Where `needle` first occurs in `haystack`, or a panic naming it.
    fn position(haystack: &str, needle: &str) -> usize {
        haystack
            .find(needle)
            .unwrap_or_else(|| panic!("expected `{needle}` in:\n{haystack}"))
    }

    /// `fill(n: u64) -> Tensor<f32, [?, 4]>`, the `zeros(dims: [n])` a run-time size takes.
    fn fill_module() -> String {
        let shape = [None, Some(4)];
        let fill = expr(
            HirExprKind::TensorFill {
                value: Box::new(expr(
                    HirExprKind::Literal(Literal::Float(0.0, None)),
                    HirType::F32,
                )),
                dims: vec![u64_var("n")],
            },
            tensor(&shape),
        );
        module_ir(
            vec![function(
                "fill",
                vec![("n", HirType::U64)],
                tensor(&shape),
                vec![ret(fill)],
            )],
            &[],
        )
    }

    #[test]
    fn a_dynamic_tensor_takes_its_shape_block_from_run_time_extents() {
        let ir = fill_module();
        let body = definition(&ir, "neuro.fn.fill");
        // The size trap panics through the module's outlined thunk, not the body itself.
        assert!(ir.contains("tensor size overflows"), "{ir}");

        // Two `i64`s per axis, extents then strides, in one block of its own.
        position(body, "call ptr @malloc(i64 32)");
        // The static stride of axis 0 is the fixed extent of axis 1, and axis 1's is 1.
        position(body, "store i64 4, ptr %dlpack.shape.slot");
        position(body, "store i64 1, ptr %dlpack.shape.slot");
        // The element run is multiplied out at run time, and the padded length is what the
        // over-aligned allocator is given, in its platform's argument order.
        let allocation = if cfg!(target_os = "windows") {
            "call ptr @_aligned_malloc(i64 %tensor.padded, i64 64)"
        } else {
            "call ptr @aligned_alloc(i64 64, i64 %tensor.padded"
        };
        position(body, allocation);

        // The deleter frees the block, after the buffer and before the structure.
        let deleter = definition(&ir, "__neuro_dlpack_deleter");
        let data = position(
            deleter,
            "call void @__neuro_aligned_release(ptr %dlpack.data)",
        );
        let block = position(
            deleter,
            "call void @__neuro_release(ptr %dlpack.shape.block",
        );
        let structure = position(deleter, "call void @__neuro_release(ptr %0)");
        assert!(
            data < block && block < structure,
            "release order:\n{deleter}"
        );
    }

    #[test]
    fn a_tensor_extent_loads_a_dynamic_axis_and_folds_a_static_one() {
        let borrowed = borrow(tensor(&[None, Some(4)]));
        let dynamic = expr(
            HirExprKind::TensorExtent {
                tensor: Box::new(var("x", borrowed.clone())),
                axis: 0,
            },
            HirType::U64,
        );
        let fixed = expr(
            HirExprKind::TensorExtent {
                tensor: Box::new(var("x", borrowed.clone())),
                axis: 1,
            },
            HirType::U64,
        );
        let ir = module_ir(
            vec![
                function(
                    "dynamic_axis",
                    vec![("x", borrowed.clone())],
                    HirType::U64,
                    vec![ret(dynamic)],
                ),
                function(
                    "static_axis",
                    vec![("x", borrowed)],
                    HirType::U64,
                    vec![ret(fixed)],
                ),
            ],
            &[],
        );

        // The handle is loaded from the borrow's cell, then axis 0's extent from its shape array.
        let dynamic_body = definition(&ir, "neuro.fn.dynamic_axis");
        position(dynamic_body, "dlpack.axis.slot");
        position(dynamic_body, "dlpack.axis");
        // A static axis is the type's own extent, read from no handle.
        let static_body = definition(&ir, "neuro.fn.static_axis");
        assert!(static_body.contains("ret i64 4"), "{static_body}");
        assert!(!static_body.contains("dlpack"), "{static_body}");
    }

    /// A static `zeros()` keeps its constant sizes: the padded run and the byte count fold, so
    /// the allocator and the control block see the same constants as before run-time sizes.
    #[test]
    fn a_static_tensor_keeps_constant_sizes_and_no_size_trap() {
        let shape = [Some(2), Some(3)];
        let fill = expr(
            HirExprKind::TensorFill {
                value: Box::new(expr(
                    HirExprKind::Literal(Literal::Float(0.0, None)),
                    HirType::F32,
                )),
                dims: Vec::new(),
            },
            tensor(&shape),
        );
        let ir = module_ir(
            vec![function(
                "zeros",
                Vec::new(),
                tensor(&shape),
                vec![ret(fill)],
            )],
            &[],
        );
        let body = definition(&ir, "neuro.fn.zeros");
        assert!(
            body.contains(&format!("call ptr @{ALIGNED_ALLOC_FN}(i64 64, i64 64)")),
            "{body}"
        );
        assert!(body.contains("store i64 24, ptr %dlpack.field"), "{body}");
        assert!(
            !body.contains("tensor.size"),
            "a static size has no trap:\n{body}"
        );
        assert!(
            !body.contains("call ptr @malloc(i64 16)") && !body.contains("dlpack.shape"),
            "{body}"
        );
    }

    #[test]
    fn a_fill_with_run_time_dims_loops_over_the_run_time_count() {
        let ir = fill_module();
        let body = definition(&ir, "neuro.fn.fill");
        // The loop's bound is the run-time count, not a constant.
        position(body, "%tensor.fill.more = icmp ult i64 %tensor.fill.idx, %");
    }

    #[test]
    fn an_identity_with_run_time_dims_divides_by_the_run_time_column_count() {
        let shape = [None, None];
        let identity = expr(
            HirExprKind::TensorIdentity {
                dims: vec![u64_var("n"), u64_var("n")],
            },
            tensor(&shape),
        );
        let ir = module_ir(
            vec![function(
                "identity",
                vec![("n", HirType::U64)],
                tensor(&shape),
                vec![ret(identity)],
            )],
            &[],
        );
        let body = definition(&ir, "neuro.fn.identity");
        position(body, "udiv i64");
        position(body, "urem i64");
    }

    #[test]
    fn a_shape_cast_to_a_dynamic_result_allocates_a_block_for_it() {
        // `Tensor<f32, [2, 3]>` re-described as `Tensor<f32, [?]>` with 6 elements.
        let cast = expr(
            HirExprKind::TensorShapeCast {
                receiver: Box::new(var("x", tensor(&[Some(2), Some(3)]))),
                permutation: None,
                dims: vec![HirExtent::Constant(6)],
            },
            tensor(&[None]),
        );
        let ir = module_ir(
            vec![function(
                "widen",
                vec![("x", tensor(&[Some(2), Some(3)]))],
                tensor(&[None]),
                vec![ret(cast)],
            )],
            &[],
        );
        let body = definition(&ir, "neuro.fn.widen");
        // One axis, so two `i64`s.
        position(body, "call ptr @malloc(i64 16)");
        position(body, "store i64 6, ptr %dlpack.shape.slot");
        // The receiver was static, so it owned no block to free.
        assert!(
            !body.contains("__neuro_release(ptr %dlpack.shape.block"),
            "{body}"
        );
    }

    #[test]
    fn a_shape_cast_from_a_dynamic_receiver_frees_its_block_and_points_at_the_constants() {
        let cast = expr(
            HirExprKind::TensorShapeCast {
                receiver: Box::new(var("x", tensor(&[None]))),
                permutation: None,
                dims: Vec::new(),
            },
            tensor(&[Some(2), Some(3)]),
        );
        let ir = module_ir(
            vec![function(
                "narrow",
                vec![("x", tensor(&[None]))],
                tensor(&[Some(2), Some(3)]),
                vec![ret(cast)],
            )],
            &[],
        );
        let body = definition(&ir, "neuro.fn.narrow");
        // The receiver's block is released and its control field cleared.
        position(body, "call void @__neuro_release(ptr %dlpack.shape.block");
        position(body, "store ptr null, ptr %dlpack.field");
        // The static result points at the shared constants again.
        position(body, "store ptr @__neuro_dlpack_shape_f32_2x3");
    }

    #[test]
    fn a_dynamic_shape_cast_reads_its_extents_before_it_frees_the_old_block() {
        // `Tensor<f32, [?]>` re-described as `Tensor<f32, [?]>` with twice the extent.
        let cast = expr(
            HirExprKind::TensorShapeCast {
                receiver: Box::new(var("x", tensor(&[None]))),
                permutation: None,
                dims: vec![HirExtent::Mul(
                    Box::new(HirExtent::Axis {
                        operand: 0,
                        axis: 0,
                    }),
                    Box::new(HirExtent::Constant(2)),
                )],
            },
            tensor(&[None]),
        );
        let ir = module_ir(
            vec![function(
                "double",
                vec![("x", tensor(&[None]))],
                tensor(&[None]),
                vec![ret(cast)],
            )],
            &[],
        );
        let body = definition(&ir, "neuro.fn.double");
        let read = position(body, "dlpack.axis");
        let freed = position(body, "call void @__neuro_release(ptr %dlpack.shape.block");
        assert!(
            read < freed,
            "the new extent is read before the old block goes:\n{body}"
        );
    }

    #[test]
    fn an_element_read_over_a_dynamic_axis_uses_the_handles_strides_and_bounds() {
        let element = expr(
            HirExprKind::TensorIndex {
                object: Box::new(var("x", borrow(tensor(&[None, Some(3)])))),
                axes: vec![
                    HirTensorAxis::Position(u64_var("i")),
                    HirTensorAxis::Position(expr(
                        HirExprKind::Literal(Literal::Integer(1, None)),
                        HirType::U64,
                    )),
                ],
            },
            HirType::F32,
        );
        let ir = module_ir(
            vec![function(
                "at",
                vec![("x", borrow(tensor(&[None, Some(3)]))), ("i", HirType::U64)],
                HirType::F32,
                vec![ret(element)],
            )],
            &[],
        );
        let body = definition(&ir, "neuro.fn.at");
        // The bound on axis 0 is the handle's extent, loaded, not a constant.
        position(body, "icmp ult i64 %");
        position(body, "dlpack.axis");
    }

    #[test]
    fn a_dynamic_tensor_built_in_a_pool_is_bumped_from_its_arena() {
        let shape = [None, Some(4)];
        let fill = expr(
            HirExprKind::TensorFill {
                value: Box::new(expr(
                    HirExprKind::Literal(Literal::Float(0.0, None)),
                    HirType::F32,
                )),
                dims: vec![u64_var("n")],
            },
            tensor(&shape),
        );
        let pool = expr(
            HirExprKind::Pool {
                label: None,
                routed: Vec::new(),
                stmts: vec![HirStmt::VarDecl {
                    name: "t".to_string(),
                    ty: tensor(&shape),
                    init: Some(fill),
                    mutable: false,
                    span: span(),
                }],
            },
            HirType::Void,
        );
        let ir = module_ir(
            vec![function(
                "main",
                vec![("n", HirType::U64)],
                HirType::I32,
                vec![
                    HirStmt::Expr(pool),
                    ret(expr(
                        HirExprKind::Literal(Literal::Integer(0, None)),
                        HirType::I32,
                    )),
                ],
            )],
            &[],
        );
        let body = definition(&ir, "main");
        // The shape block and the element buffer both come from the pool's arena, not libc.
        position(body, "call ptr @__neuro_arena_alloc(i64 32)");
        position(body, "call ptr @__neuro_arena_aligned_alloc(");
    }

    #[test]
    fn a_dynamic_tensor_without_its_run_time_extents_is_refused() {
        let fill = expr(
            HirExprKind::TensorFill {
                value: Box::new(expr(
                    HirExprKind::Literal(Literal::Float(0.0, None)),
                    HirType::F32,
                )),
                dims: Vec::new(),
            },
            tensor(&[None]),
        );
        let program = HirProgram {
            items: vec![HirItem::Function(function(
                "unsized",
                Vec::new(),
                tensor(&[None]),
                vec![ret(fill)],
            ))],
        };
        let context = Context::create();
        let result = build_module(
            &context,
            &program,
            OptimizationLevelSetting::O0,
            &source_map("extents.nr", ""),
            &[],
            GpuVendor::Nvidia,
            None,
        );
        assert!(
            result.is_err(),
            "a `?` tensor with no run-time extent to size it from must not lower"
        );
    }

    /// A stand-in for a body `mlir-backend` computes: one rank-`2` input and one rank-`2`
    /// result, each a memref descriptor (two pointers, an offset, two sizes, two strides).
    const DYNAMIC_BODY: &str = r#"
        define void @ext_dyn(ptr %0, ptr %1, i64 %2, i64 %3, i64 %4, i64 %5, i64 %6, ptr %7, ptr %8, i64 %9, i64 %10, i64 %11, i64 %12, i64 %13) {
          ret void
        }
    "#;

    fn wrapped(name: &str, symbol: &str, ir: &str) -> ExternalBodies {
        ExternalBodies {
            llvm_ir: ir.to_string(),
            functions: vec![(name.to_string(), symbol.to_string())],
            memory: BodyMemory::Host,
            guards: Vec::new(),
        }
    }

    #[test]
    fn a_wrapper_reads_a_dynamic_operand_from_its_handle_and_sizes_its_result_from_result_extents()
    {
        let operand = borrow(tensor(&[None, Some(2)]));
        let mut scale = function(
            "scale_dyn",
            vec![("a", operand)],
            tensor(&[None, Some(2)]),
            Vec::new(),
        );
        scale.result_extents = vec![HirExtent::Axis {
            operand: 0,
            axis: 0,
        }];
        let ir = module_ir(
            vec![scale],
            &[wrapped("scale_dyn", "ext_dyn", DYNAMIC_BODY)],
        );
        let body = definition(&ir, "neuro.fn.scale_dyn");

        // The operand's extents and strides are read from its handle, not from constants.
        let call = position(body, "call void @ext_dyn(");
        assert!(body[..call].contains("dlpack.axis"), "{body}");
        // The result is allocated from the operand's own axis-0 extent.
        position(body, &format!("@{ALIGNED_ALLOC_FN}("));
        assert!(body[..call].contains("tensor.padded"), "{body}");
    }

    #[test]
    fn a_wrapper_evaluates_an_integer_operand_into_a_result_extent() {
        let operand = borrow(tensor(&[None]));
        let mut grow = function(
            "grow_dyn",
            vec![("a", operand), ("k", HirType::U32)],
            tensor(&[None]),
            Vec::new(),
        );
        // The result is one longer than the operand, plus the unsigned integer `k`.
        grow.result_extents = vec![HirExtent::Add(
            Box::new(HirExtent::Axis {
                operand: 0,
                axis: 0,
            }),
            Box::new(HirExtent::Scalar(1)),
        )];
        let body_ir = r#"
            define void @ext_grow(ptr %0, ptr %1, i64 %2, i64 %3, i64 %4, i32 %5, ptr %6, ptr %7, i64 %8, i64 %9, i64 %10) {
              ret void
            }
        "#;
        let ir = module_ir(vec![grow], &[wrapped("grow_dyn", "ext_grow", body_ir)]);
        let body = definition(&ir, "neuro.fn.grow_dyn");
        // `k` widens as unsigned, and the sum is a run-time add.
        position(body, "zext i32");
        position(body, "add i64");
    }
}
