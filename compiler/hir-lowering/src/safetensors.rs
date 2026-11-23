//! `std::safetensors`'s two compiler-known methods, mirroring the checker.
//!
//! A read desugars to the library's `locate`, which checks the stored dtype and shape against
//! the annotated ones (both passed as text, a `?` axis matching any extent), and a copy of the
//! bytes it hands back, sized by the stored extents handed back with them:
//!
//! ```text
//! match std::safetensors::locate(&file, name, "F32", "[?, 3]") {
//!     Ok(__tensor_read_N) => Ok(__tensor_from_bytes(__tensor_read_N)),
//!     __tensor_read_err_N => Err(__tensor_read_err_N)
//! }
//! ```
//!
//! A write copies the tensor's bytes and extents out first: `append(&mut out, name, "F32",
//! __tensor_bytes(&t))`. The two intrinsics are the backend's; only this desugar emits them,
//! so no checker table names them.

use ast_types::Expr;
use neuro_hir::{HirExpr, HirExprKind, HirMatchArm, HirMatchBinding, HirMatchTest, HirType};
use shared_types::{Literal, Span};

use crate::{Lowerer, LoweringError, MonoArg};

const FILE_TYPE: &str = "std::safetensors::File";
const WRITER_TYPE: &str = "std::safetensors::Writer";
const READ_METHOD: &str = "tensor";
const WRITE_METHOD: &str = "add";
const LOCATE_FN: &str = "std::safetensors::locate";
const APPEND_FN: &str = "std::safetensors::append";
const FROM_BYTES: &str = "__tensor_from_bytes";
const TO_BYTES: &str = "__tensor_bytes";
const RESULT_ENUM: &str = "Result";
const SUCCESS: &str = "Ok";
const FAILURE: &str = "Err";
const IO_ERROR: &str = "IoError";

/// Whether `expr` is a `.tensor(...)` call, which a `?` or `??` hands the payload it expects.
pub(crate) fn reads_tensor_by_context(expr: &Expr) -> bool {
    let Expr::Call { func, .. } = expr else {
        return false;
    };
    matches!(&**func, Expr::FieldAccess { field, .. } if field.name == READ_METHOD)
}

/// Whether `method` on a receiver of type `receiver` is one of the two.
pub(crate) fn is_safetensors_method(receiver: &HirType, method: &str) -> bool {
    matches!(receiver.referent(), HirType::Struct(name)
        if (name == FILE_TYPE && method == READ_METHOD) || (name == WRITER_TYPE && method == WRITE_METHOD))
}

fn text() -> HirType {
    HirType::Reference {
        inner: Box::new(HirType::String),
        mutable: false,
    }
}

fn text_literal(value: String, span: Span) -> HirExpr {
    HirExpr::new(HirExprKind::Literal(Literal::String(value)), text(), span)
}

/// The safetensors dtype a tensor element is stored as.
fn dtype(element: &HirType) -> Result<&'static str, LoweringError> {
    let name = match element {
        HirType::Bool => "BOOL",
        HirType::U8 => "U8",
        HirType::I8 => "I8",
        HirType::U16 => "U16",
        HirType::I16 => "I16",
        HirType::F16 => "F16",
        HirType::BF16 => "BF16",
        HirType::U32 => "U32",
        HirType::I32 => "I32",
        HirType::F32 => "F32",
        HirType::U64 => "U64",
        HirType::I64 => "I64",
        HirType::F64 => "F64",
        other => {
            return Err(LoweringError::Malformed {
                detail: format!("a safetensors tensor of `{other}` elements"),
            });
        }
    };
    Ok(name)
}

/// The dtype and the shape a read expects, as the library compares them: `"F32"` and
/// `"[?, 3]"`, a `?` for each axis the stored shape decides.
fn layout_text(tensor: &HirType) -> Result<(&'static str, String), LoweringError> {
    let HirType::Tensor { element, shape, .. } = tensor else {
        return Err(LoweringError::Malformed {
            detail: format!("a safetensors call over `{tensor}`, which is not a tensor"),
        });
    };
    let extents: Vec<String> = shape
        .iter()
        .map(|extent| extent.map_or_else(|| "?".to_string(), |e| e.to_string()))
        .collect();
    Ok((dtype(element)?, format!("[{}]", extents.join(", "))))
}

fn borrow(value: HirExpr, mutable: bool) -> HirExpr {
    if matches!(value.ty, HirType::Reference { .. }) {
        return value;
    }
    let ty = HirType::Reference {
        inner: Box::new(value.ty.clone()),
        mutable,
    };
    let span = value.span;
    HirExpr::new(
        HirExprKind::Reference {
            operand: Box::new(value),
            mutable,
        },
        ty,
        span,
    )
}

fn call(name: &str, params: Vec<HirType>, ret: HirType, args: Vec<HirExpr>, span: Span) -> HirExpr {
    let callee = HirExpr::new(
        HirExprKind::Variable(name.to_string()),
        HirType::Function {
            params,
            ret: Box::new(ret.clone()),
        },
        span,
    );
    HirExpr::new(
        HirExprKind::Call {
            callee: Box::new(callee),
            args,
        },
        ret,
        span,
    )
}

impl Lowerer {
    /// Lower `file.tensor(name)` or `out.add(name, &tensor)`; `receiver` is already lowered.
    pub(crate) fn lower_safetensors_method(
        &mut self,
        receiver: HirExpr,
        method: &str,
        args: &[Expr],
        expected: Option<&HirType>,
        span: Span,
    ) -> Result<HirExpr, LoweringError> {
        if method == READ_METHOD {
            self.lower_tensor_read(receiver, args, expected, span)
        } else {
            self.lower_tensor_write(receiver, args, span)
        }
    }

    fn lower_tensor_read(
        &mut self,
        file: HirExpr,
        args: &[Expr],
        expected: Option<&HirType>,
        span: Span,
    ) -> Result<HirExpr, LoweringError> {
        let tensor = match expected {
            Some(ty @ HirType::Tensor { .. }) => ty.clone(),
            Some(HirType::Enum(instance)) => self
                .enum_variant(instance, SUCCESS)?
                .1
                .first()
                .map(|(_, ty)| ty.clone())
                .ok_or_else(|| LoweringError::Malformed {
                    detail: format!("`{instance}::Ok` carries no payload"),
                })?,
            _ => {
                return Err(LoweringError::Malformed {
                    detail: "`file.tensor(...)` with no tensor type to read".to_string(),
                });
            }
        };
        let (dtype, shape) = layout_text(&tensor)?;
        let [name] = args else {
            return Err(LoweringError::Malformed {
                detail: "`file.tensor` takes one name".to_string(),
            });
        };
        let name = self.lower_expr(name, Some(&text()))?;

        let (params, located) = self.functions.get(LOCATE_FN).cloned().ok_or_else(|| {
            LoweringError::UnresolvedBinding {
                name: LOCATE_FN.to_string(),
            }
        })?;
        let scrutinee = call(
            LOCATE_FN,
            params,
            located.clone(),
            vec![
                borrow(file, false),
                name,
                text_literal(dtype.to_string(), span),
                text_literal(shape, span),
            ],
            span,
        );
        let HirType::Enum(located_instance) = &located else {
            return Err(LoweringError::Malformed {
                detail: format!("`{LOCATE_FN}` returns `{located}`"),
            });
        };
        let (found, bytes) = self.enum_variant(located_instance, SUCCESS)?;
        let bytes_ty =
            bytes
                .first()
                .map(|(_, ty)| ty.clone())
                .ok_or_else(|| LoweringError::Malformed {
                    detail: format!("`{located_instance}::Ok` carries no payload"),
                })?;

        let error_ty = HirType::Struct(IO_ERROR.to_string());
        let result = self.instantiate_generic_enum(
            RESULT_ENUM,
            &[
                MonoArg::Type(tensor.clone()),
                MonoArg::Type(error_ty.clone()),
            ],
        )?;
        let (ok_tag, _) = self.enum_variant(&result, SUCCESS)?;
        let (err_tag, _) = self.enum_variant(&result, FAILURE)?;

        let index = self.try_counter;
        self.try_counter += 1;
        let bytes_name = format!("__tensor_read_{index}");
        let error_name = format!("__tensor_read_err_{index}");
        let bytes_value = HirExpr::new(
            HirExprKind::Variable(bytes_name.clone()),
            bytes_ty.clone(),
            span,
        );
        let copied = call(
            FROM_BYTES,
            vec![bytes_ty.clone()],
            tensor,
            vec![bytes_value],
            span,
        );
        let success = HirMatchArm {
            tests: vec![HirMatchTest::Tag { tag: found }],
            bindings: vec![HirMatchBinding::of_payload_slot(bytes_name, bytes_ty, 0)],
            guard: None,
            body: self.build_enum_construct(&result, SUCCESS, ok_tag, vec![copied], span),
        };
        let error_value = HirExpr::new(
            HirExprKind::Variable(error_name.clone()),
            error_ty.clone(),
            span,
        );
        let failure = HirMatchArm {
            tests: vec![HirMatchTest::Wildcard],
            bindings: vec![HirMatchBinding::of_payload_slot(error_name, error_ty, 0)],
            guard: None,
            body: self.build_enum_construct(&result, FAILURE, err_tag, vec![error_value], span),
        };
        Ok(HirExpr::new(
            HirExprKind::Match {
                scrutinee: Box::new(scrutinee),
                arms: vec![success, failure],
            },
            HirType::Enum(result),
            span,
        ))
    }

    fn lower_tensor_write(
        &mut self,
        out: HirExpr,
        args: &[Expr],
        span: Span,
    ) -> Result<HirExpr, LoweringError> {
        let [name, tensor] = args else {
            return Err(LoweringError::Malformed {
                detail: "`out.add` takes a name and a tensor".to_string(),
            });
        };
        let name = self.lower_expr(name, Some(&text()))?;
        let tensor = self.lower_expr(tensor, None)?;
        // The library spells the shape from the extents `__tensor_bytes` copies out.
        let (dtype, _) = layout_text(tensor.ty.referent())?;
        let (params, ret) = self.functions.get(APPEND_FN).cloned().ok_or_else(|| {
            LoweringError::UnresolvedBinding {
                name: APPEND_FN.to_string(),
            }
        })?;
        let data_ty = params
            .last()
            .cloned()
            .ok_or_else(|| LoweringError::Malformed {
                detail: format!("`{APPEND_FN}` takes no parameters"),
            })?;
        let data = call(
            TO_BYTES,
            vec![tensor.ty.clone()],
            data_ty,
            vec![tensor],
            span,
        );
        Ok(call(
            APPEND_FN,
            params,
            ret,
            vec![
                borrow(out, true),
                name,
                text_literal(dtype.to_string(), span),
                data,
            ],
            span,
        ))
    }
}

#[cfg(test)]
mod tests {
    use super::layout_text;
    use neuro_hir::{AxisNames, HirType};

    fn tensor(element: HirType, shape: Vec<Option<usize>>) -> HirType {
        HirType::Tensor {
            element: Box::new(element),
            shape,
            names: AxisNames::default(),
        }
    }

    #[test]
    fn the_layout_is_spelled_as_the_library_compares_it() {
        let (dtype, shape) =
            layout_text(&tensor(HirType::BF16, vec![Some(2), Some(3)])).expect("a tensor");
        assert_eq!((dtype, shape.as_str()), ("BF16", "[2, 3]"));
        let (dtype, shape) = layout_text(&tensor(HirType::Bool, vec![])).expect("a scalar");
        assert_eq!((dtype, shape.as_str()), ("BOOL", "[]"));
    }

    #[test]
    fn a_dynamic_extent_is_spelled_as_a_question_mark() {
        let (_, shape) = layout_text(&tensor(HirType::F32, vec![None, Some(4)])).expect("a tensor");
        assert_eq!(shape, "[?, 4]");
    }

    #[test]
    fn a_non_tensor_has_no_layout() {
        assert!(layout_text(&HirType::F32).is_err());
    }
}
