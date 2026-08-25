// Codegen for interpolated string literals.
//
// Every part is rendered to a `{ ptr, len }` fat pointer, then the parts are
// concatenated into one freshly allocated buffer. Rendering is chosen from the
// hole's resolved type and its format spec, both fixed at compile time, so the
// only runtime work is the formatting itself.

use inkwell::values::{BasicValueEnum, IntValue, PointerValue};
use neuro_hir::HirInterpPart;
use shared_types::{FormatAlign, FormatKind, FormatSpec};

use super::format_layout::{ALIGN_CENTER, ALIGN_LEFT, ALIGN_RIGHT};
use crate::codegen::context::CodegenContext;
use crate::errors::{CodegenError, CodegenResult};
use crate::types::Type;

/// `%.16g` round-trips an `f64` while still printing `0.1` as `0.1`, so it is the
/// default rendering; `__neuro_point` restores the fraction `%g` drops.
const DEFAULT_FLOAT_CONVERSION: &str = ".16g";

/// Significant digits for a `{x:e}` hole that named no precision: 16, matching
/// [`DEFAULT_FLOAT_CONVERSION`]. `%e` counts decimals rather than significant
/// digits, hence 15. `__neuro_exp` trims the zeros this pads with.
const DEFAULT_SCIENTIFIC_DECIMALS: u32 = 15;

fn llvm_err(e: inkwell::builder::BuilderError) -> CodegenError {
    CodegenError::LlvmError(e.to_string())
}

impl<'ctx> CodegenContext<'ctx> {
    /// Render and concatenate the parts of an interpolated literal into a new
    /// owned `string`.
    pub(crate) fn codegen_interp_string(
        &mut self,
        parts: &[HirInterpPart],
    ) -> CodegenResult<BasicValueEnum<'ctx>> {
        let mut pieces: Vec<(PointerValue<'ctx>, IntValue<'ctx>)> = Vec::with_capacity(parts.len());

        for part in parts {
            match part {
                HirInterpPart::Text(text) => {
                    let global = self
                        .builder
                        .build_global_string_ptr(text, "interp.text")
                        .map_err(llvm_err)?;
                    let len = self.context.i64_type().const_int(text.len() as u64, false);
                    pieces.push((global.as_pointer_value(), len));
                }
                HirInterpPart::Formatted { expr, spec } => {
                    let ty = Type::from_hir(&expr.ty);
                    let value = self.codegen_expr(expr)?;
                    let rendered = self.render_hole(value, &ty, spec)?;
                    pieces.push(self.split_string_value(rendered)?);
                }
            }
        }

        self.build_concat(&pieces)
    }

    /// Copy every piece into one buffer sized to their total length.
    fn build_concat(
        &self,
        pieces: &[(PointerValue<'ctx>, IntValue<'ctx>)],
    ) -> CodegenResult<BasicValueEnum<'ctx>> {
        let i64_type = self.context.i64_type();
        let mut total = i64_type.const_zero();
        for (_, len) in pieces {
            total = self
                .builder
                .build_int_add(total, *len, "interp.total")
                .map_err(llvm_err)?;
        }

        let buf = self.build_malloc(total, "interp.buf")?;
        let mut offset = i64_type.const_zero();
        for (ptr, len) in pieces {
            let dst = self.byte_offset(buf, offset, "interp.dst")?;
            self.build_memcpy_call(dst, *ptr, *len)?;
            offset = self
                .builder
                .build_int_add(offset, *len, "interp.offset")
                .map_err(llvm_err)?;
        }

        self.build_string_value(buf, total)
    }

    /// Split a `{ ptr, len }` value into its two fields.
    fn split_string_value(
        &self,
        value: BasicValueEnum<'ctx>,
    ) -> CodegenResult<(PointerValue<'ctx>, IntValue<'ctx>)> {
        let structure = value.into_struct_value();
        let ptr = self
            .builder
            .build_extract_value(structure, 0, "interp.piece.ptr")
            .map_err(llvm_err)?
            .into_pointer_value();
        let len = self
            .builder
            .build_extract_value(structure, 1, "interp.piece.len")
            .map_err(llvm_err)?
            .into_int_value();
        Ok((ptr, len))
    }

    /// Render one hole: pick the rendering from its type and spec, then apply the
    /// field width.
    fn render_hole(
        &self,
        value: BasicValueEnum<'ctx>,
        ty: &Type,
        spec: &FormatSpec,
    ) -> CodegenResult<BasicValueEnum<'ctx>> {
        let target = ty.referent().clone();
        // A `&mut T` hole is the referent's address; read through it before rendering.
        let value = match (ty, value) {
            (Type::Reference { .. }, BasicValueEnum::PointerValue(ptr)) => {
                let referent = self.type_mapper.map_type(&target)?;
                self.builder
                    .build_load(referent, ptr, "interp.deref")
                    .map_err(llvm_err)?
            }
            _ => value,
        };

        let rendered = match &target {
            Type::String => self.render_string(value, spec)?,
            Type::Bool => self.render_bool(value, spec)?,
            Type::Char => self.render_char(value, spec)?,
            other if other.is_integer() => self.render_integer(value, other, spec)?,
            other if other.is_float() => self.render_float(value, spec)?,
            other => {
                return Err(CodegenError::InternalError(format!(
                    "type {:?} reached interpolation codegen; semantic analysis rejects it",
                    other
                )))
            }
        };

        self.apply_width(rendered, spec)
    }

    fn render_string(
        &self,
        value: BasicValueEnum<'ctx>,
        spec: &FormatSpec,
    ) -> CodegenResult<BasicValueEnum<'ctx>> {
        if spec.kind == FormatKind::Debug {
            return self.build_quoted(value, b'"');
        }
        Ok(value)
    }

    fn render_bool(
        &self,
        value: BasicValueEnum<'ctx>,
        _spec: &FormatSpec,
    ) -> CodegenResult<BasicValueEnum<'ctx>> {
        let i64_type = self.context.i64_type();
        let truthy = self
            .builder
            .build_global_string_ptr("true", "interp.true")
            .map_err(llvm_err)?;
        let falsy = self
            .builder
            .build_global_string_ptr("false", "interp.false")
            .map_err(llvm_err)?;
        let flag = value.into_int_value();
        let ptr = self
            .builder
            .build_select(
                flag,
                truthy.as_pointer_value(),
                falsy.as_pointer_value(),
                "interp.bool.ptr",
            )
            .map_err(llvm_err)?
            .into_pointer_value();
        let len = self
            .builder
            .build_select(
                flag,
                i64_type.const_int(4, false),
                i64_type.const_int(5, false),
                "interp.bool.len",
            )
            .map_err(llvm_err)?
            .into_int_value();
        self.build_string_value(ptr, len)
    }

    fn render_char(
        &self,
        value: BasicValueEnum<'ctx>,
        spec: &FormatSpec,
    ) -> CodegenResult<BasicValueEnum<'ctx>> {
        let utf8 = self.get_or_define_utf8()?;
        let encoded = self
            .builder
            .build_call(utf8, &[value.into()], "interp.char")
            .map_err(llvm_err)?
            .try_as_basic_value()
            .basic()
            .ok_or_else(|| CodegenError::InternalError("utf8 helper returned void".to_string()))?;

        if spec.kind == FormatKind::Debug {
            return self.build_quoted(encoded, b'\'');
        }
        Ok(encoded)
    }

    fn render_integer(
        &self,
        value: BasicValueEnum<'ctx>,
        ty: &Type,
        spec: &FormatSpec,
    ) -> CodegenResult<BasicValueEnum<'ctx>> {
        let i64_type = self.context.i64_type();
        let raw = value.into_int_value();

        // Radix renderings show the value's own bits, so they widen by zero
        // extension: `-1i32` in hex is `ffffffff`, not sixteen `f`s.
        let widened_unsigned = self
            .builder
            .build_int_z_extend(raw, i64_type, "interp.int.bits")
            .map_err(llvm_err)?;

        if spec.kind == FormatKind::Binary {
            let helper = self.get_or_define_fmt_binary()?;
            return self
                .builder
                .build_call(helper, &[widened_unsigned.into()], "interp.bin")
                .map_err(llvm_err)?
                .try_as_basic_value()
                .basic()
                .ok_or_else(|| {
                    CodegenError::InternalError("binary helper returned void".to_string())
                });
        }

        let (operand, conversion) = match spec.kind {
            FormatKind::LowerHex => (widened_unsigned, "llx"),
            FormatKind::UpperHex => (widened_unsigned, "llX"),
            FormatKind::Octal => (widened_unsigned, "llo"),
            _ if ty.is_unsigned_int() => (widened_unsigned, "llu"),
            _ => {
                let signed = self
                    .builder
                    .build_int_s_extend(raw, i64_type, "interp.int")
                    .map_err(llvm_err)?;
                (signed, "lld")
            }
        };

        let format = self.format_string(spec, conversion, None)?;
        let helper = self.get_or_define_fmt_int()?;
        self.builder
            .build_call(helper, &[operand.into(), format.into()], "interp.int.text")
            .map_err(llvm_err)?
            .try_as_basic_value()
            .basic()
            .ok_or_else(|| CodegenError::InternalError("int format returned void".to_string()))
    }

    fn render_float(
        &self,
        value: BasicValueEnum<'ctx>,
        spec: &FormatSpec,
    ) -> CodegenResult<BasicValueEnum<'ctx>> {
        let f64_type = self.context.f64_type();
        let raw = value.into_float_value();
        // Variadic arguments promote to `double`; `f16`/`bf16`/`f32` widen here.
        let operand = if raw.get_type() == f64_type {
            raw
        } else {
            self.builder
                .build_float_ext(raw, f64_type, "interp.float")
                .map_err(llvm_err)?
        };

        let (conversion, precision) = match spec.kind {
            FormatKind::Fixed => ("f", spec.precision),
            FormatKind::Scientific => (
                "e",
                Some(spec.precision.unwrap_or(DEFAULT_SCIENTIFIC_DECIMALS)),
            ),
            _ => (DEFAULT_FLOAT_CONVERSION, None),
        };
        let format = self.format_string(spec, conversion, precision)?;
        let helper = self.get_or_define_fmt_float()?;
        let text = self
            .builder
            .build_call(
                helper,
                &[operand.into(), format.into()],
                "interp.float.text",
            )
            .map_err(llvm_err)?
            .try_as_basic_value()
            .basic()
            .ok_or_else(|| CodegenError::InternalError("float format returned void".to_string()))?;

        if spec.kind == FormatKind::Scientific {
            // A hole that named no precision asked for the value's own digits, so
            // the padding `%e` added is trimmed back off.
            let trim = self
                .context
                .bool_type()
                .const_int(u64::from(spec.precision.is_none()), false);
            let (ptr, len) = self.split_string_value(text)?;
            return self
                .builder
                .build_call(
                    self.get_or_define_normalize_exponent()?,
                    &[ptr.into(), len.into(), trim.into()],
                    "interp.float.sci",
                )
                .map_err(llvm_err)?
                .try_as_basic_value()
                .basic()
                .ok_or_else(|| {
                    CodegenError::InternalError("exponent fix-up returned void".to_string())
                });
        }

        if spec.kind == FormatKind::Fixed {
            return Ok(text);
        }

        let (ptr, len) = self.split_string_value(text)?;
        self.builder
            .build_call(
                self.get_or_define_ensure_point()?,
                &[ptr.into(), len.into()],
                "interp.float.point",
            )
            .map_err(llvm_err)?
            .try_as_basic_value()
            .basic()
            .ok_or_else(|| CodegenError::InternalError("float fix-up returned void".to_string()))
    }

    /// Build the `printf` conversion for a spec: sign flag, optional precision,
    /// and the conversion letters. Width and alignment are deliberately absent —
    /// `__neuro_pad` owns them, so one padding rule covers every type and centring
    /// works even though `printf` has no such flag.
    fn format_string(
        &self,
        spec: &FormatSpec,
        conversion: &str,
        precision: Option<u32>,
    ) -> CodegenResult<PointerValue<'ctx>> {
        let mut format = String::from("%");
        if spec.plus_sign {
            format.push('+');
        }
        if let Some(precision) = precision {
            format.push('.');
            format.push_str(&precision.to_string());
        }
        format.push_str(conversion);

        self.builder
            .build_global_string_ptr(&format, "interp.fmt")
            .map_err(llvm_err)
            .map(|global| global.as_pointer_value())
    }

    fn build_quoted(
        &self,
        value: BasicValueEnum<'ctx>,
        quote: u8,
    ) -> CodegenResult<BasicValueEnum<'ctx>> {
        let (ptr, len) = self.split_string_value(value)?;
        let helper = self.get_or_define_quote()?;
        let quote = self.context.i8_type().const_int(u64::from(quote), false);
        self.builder
            .build_call(
                helper,
                &[ptr.into(), len.into(), quote.into()],
                "interp.quoted",
            )
            .map_err(llvm_err)?
            .try_as_basic_value()
            .basic()
            .ok_or_else(|| CodegenError::InternalError("quote helper returned void".to_string()))
    }

    /// Pad the rendered text to the spec's field width. Right alignment is the
    /// default for every type; zero filling is only reachable for numeric holes, which
    /// semantic analysis has already restricted to right alignment.
    fn apply_width(
        &self,
        value: BasicValueEnum<'ctx>,
        spec: &FormatSpec,
    ) -> CodegenResult<BasicValueEnum<'ctx>> {
        let Some(width) = spec.width.filter(|width| *width > 0) else {
            return Ok(value);
        };

        let align = match spec.align {
            Some(FormatAlign::Left) => ALIGN_LEFT,
            Some(FormatAlign::Center) => ALIGN_CENTER,
            Some(FormatAlign::Right) | None => ALIGN_RIGHT,
        };
        let fill = if spec.zero_pad { b'0' } else { b' ' };

        let (ptr, len) = self.split_string_value(value)?;
        let helper = self.get_or_define_pad()?;
        self.builder
            .build_call(
                helper,
                &[
                    ptr.into(),
                    len.into(),
                    self.context
                        .i64_type()
                        .const_int(u64::from(width), false)
                        .into(),
                    self.context.i32_type().const_int(align, false).into(),
                    self.context
                        .i8_type()
                        .const_int(u64::from(fill), false)
                        .into(),
                ],
                "interp.padded",
            )
            .map_err(llvm_err)?
            .try_as_basic_value()
            .basic()
            .ok_or_else(|| CodegenError::InternalError("pad helper returned void".to_string()))
    }
}
