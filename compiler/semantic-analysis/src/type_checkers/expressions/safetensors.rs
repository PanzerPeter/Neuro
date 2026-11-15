// `std::safetensors`'s two compiler-known methods.
//
// A tensor's element type and shape cannot yet be a library function's type parameter, so
// `file.tensor(name)` and `out.add(name, &tensor)` are typed here, and the library declares
// neither. A read takes its tensor type from what its context expects, as `Tensor::zeros()`
// does, and reaches that expectation through a `?`, a `??` or a `val ... else` annotation.

use ast_types::Expr;
use shared_types::Span;

use super::super::TypeChecker;
use crate::errors::TypeError;
use crate::types::Type;

pub(crate) const FILE_TYPE: &str = "std::safetensors::File";
pub(crate) const WRITER_TYPE: &str = "std::safetensors::Writer";
const READ_METHOD: &str = "tensor";
const WRITE_METHOD: &str = "add";
const RESULT_ENUM: &str = "Result";
const IO_ERROR: &str = "IoError";

/// Whether `expr` is a `.tensor(...)` call. A `?` or `??` hands such a call the payload it
/// expects, which is how `val w: Tensor<..> = file.tensor("w")?` is typed; any other operand
/// is checked with no expectation, as before.
pub(crate) fn reads_tensor_by_context(expr: &Expr) -> bool {
    let Expr::Call { func, .. } = expr else {
        return false;
    };
    matches!(&**func, Expr::FieldAccess { field, .. } if field.name == READ_METHOD)
}

fn text() -> Type {
    Type::Reference {
        inner: Box::new(Type::String),
        mutable: false,
    }
}

impl TypeChecker {
    /// The type of a compiler-known method on a `std::safetensors` struct, or `None` for any
    /// other method, which the ordinary method path then resolves or reports.
    #[expect(clippy::too_many_arguments)]
    pub(super) fn check_safetensors_method(
        &mut self,
        struct_name: &str,
        object: &Expr,
        obj_ty: &Type,
        method: &str,
        args: &[Expr],
        expected: Option<&Type>,
        span: Span,
    ) -> Option<Type> {
        match (struct_name, method) {
            (FILE_TYPE, READ_METHOD) => Some(self.check_tensor_read(object, args, expected, span)),
            (WRITER_TYPE, WRITE_METHOD) => {
                self.check_tensor_write(object, obj_ty, args, span);
                Some(Type::Void)
            }
            _ => None,
        }
    }

    /// `file.tensor(name)`: a `Result<Tensor<T, S>, IoError>`, with `Tensor<T, S>` read off
    /// the expectation, either the tensor itself (under `?` or `??`) or the whole `Result`.
    fn check_tensor_read(
        &mut self,
        object: &Expr,
        args: &[Expr],
        expected: Option<&Type>,
        span: Span,
    ) -> Type {
        self.check_receiver_call_args(Some((object, false)), args, &[text()], span);
        let tensor = match expected {
            Some(ty @ Type::Tensor { .. }) => Some(ty.clone()),
            Some(Type::Enum(name)) => self
                .instance_parts(name)
                .filter(|(base, _)| base == RESULT_ENUM)
                .and_then(|(_, args)| args.into_iter().next())
                .filter(|ty| matches!(ty, Type::Tensor { .. })),
            _ => None,
        };
        let Some(tensor) = tensor else {
            self.record_error(TypeError::TensorReadNeedsType { span });
            return Type::Unknown;
        };
        let Type::Tensor { shape, .. } = &tensor else {
            return Type::Unknown;
        };
        // A `?` extent binds from the stored shape, which is a later item's work.
        if self.reject_dynamic_extent(&shape.clone(), "a safetensors read", &tensor, span) {
            return Type::Unknown;
        }
        let error = Type::Struct(IO_ERROR.to_string());
        self.instantiate_generic_enum(RESULT_ENUM, &[tensor, error], span)
    }

    /// `out.add(name, &tensor)`: the writer is updated in place, and the tensor, of any
    /// element type and static shape, is only read.
    fn check_tensor_write(&mut self, object: &Expr, obj_ty: &Type, args: &[Expr], span: Span) {
        self.check_mut_self_receiver(object, obj_ty, span);
        let [name, tensor] = args else {
            self.record_error(TypeError::ArgumentCountMismatch {
                expected: 2,
                found: args.len(),
                span,
            });
            for arg in args {
                self.check_expr(arg, None);
            }
            return;
        };
        self.check_receiver_call_args(
            Some((object, true)),
            std::slice::from_ref(name),
            &[text()],
            span,
        );
        let found = self.check_expr(tensor, None).unwrap_or(Type::Unknown);
        match &found {
            Type::Reference { inner, .. } if matches!(**inner, Type::Tensor { .. }) => {
                if let Type::Tensor { shape, .. } = &**inner {
                    self.reject_dynamic_extent(&shape.clone(), "a safetensors write", inner, span);
                }
            }
            Type::Unknown => {}
            _ => self.record_error(TypeError::TensorWriteNeedsBorrow {
                found,
                span: tensor.span(),
            }),
        }
    }
}
