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

/// Who owns the buffer behind a rendered piece.
///
/// A rendering either hands back bytes the program already had — a `.rodata` literal,
/// the caller's own string — or a buffer it allocated. The concatenation copies every
/// piece out and must then release exactly the allocated ones, so each piece carries
/// the answer rather than the copy loop guessing from the pointer.
#[derive(Clone, Copy)]
enum PieceOwner<'ctx> {
    /// Bytes owned elsewhere. Freeing these would release `.rodata` or the caller's
    /// string, so nothing is emitted for them.
    Borrowed,
    /// A buffer this rendering allocated and nothing else aliases.
    Owned,
    /// The result of a transform that returns its input untouched when the text is
    /// already in the requested shape — `__neuro_pad`, `__neuro_point`, and
    /// `__neuro_exp` all do. The buffer is ours only when it is not the borrowed one
    /// handed in, which is a runtime pointer comparison.
    OwnedUnlessSameAs(PointerValue<'ctx>),
}

/// A rendered piece: its `{ ptr, len }` fat pointer and who owns the bytes.
struct Piece<'ctx> {
    ptr: PointerValue<'ctx>,
    len: IntValue<'ctx>,
    owner: PieceOwner<'ctx>,
}

impl<'ctx> CodegenContext<'ctx> {
    /// Render and concatenate the parts of an interpolated literal into a new
    /// owned `string`.
    pub(crate) fn codegen_interp_string(
        &mut self,
        parts: &[HirInterpPart],
    ) -> CodegenResult<BasicValueEnum<'ctx>> {
        let mut pieces: Vec<Piece<'ctx>> = Vec::with_capacity(parts.len());

        for part in parts {
            match part {
                HirInterpPart::Text(text) => {
                    let global = self
                        .builder
                        .build_global_string_ptr(text, "interp.text")
                        .map_err(llvm_err)?;
                    let len = self.context.i64_type().const_int(text.len() as u64, false);
                    pieces.push(Piece {
                        ptr: global.as_pointer_value(),
                        len,
                        owner: PieceOwner::Borrowed,
                    });
                }
                HirInterpPart::Formatted { expr, spec } => {
                    let ty = Type::from_hir(&expr.ty);
                    let value = self.codegen_expr(expr)?;
                    // A hole holding a nested producer — `"{a + b}"` — hands us a buffer
                    // of our own, which a `string` rendering passes straight through.
                    let incoming = if Self::produces_owned_string(expr) {
                        PieceOwner::Owned
                    } else {
                        PieceOwner::Borrowed
                    };
                    let (rendered, owner) = self.render_hole(value, &ty, spec, incoming)?;
                    let (ptr, len) = self.split_string_value(rendered)?;
                    pieces.push(Piece { ptr, len, owner });
                }
            }
        }

        let joined = self.build_concat(&pieces)?;

        // Every rendering is dead once its bytes have been copied into the joined
        // buffer, and no piece outlives this expression, so the scratch buffers are
        // released here rather than living as long as the string they contributed to.
        for piece in &pieces {
            self.free_piece(piece)?;
        }

        Ok(joined)
    }

    /// Copy every piece into one buffer sized to their total length.
    fn build_concat(&self, pieces: &[Piece<'ctx>]) -> CodegenResult<BasicValueEnum<'ctx>> {
        let i64_type = self.context.i64_type();
        let mut total = i64_type.const_zero();
        for piece in pieces {
            total = self
                .builder
                .build_int_add(total, piece.len, "interp.total")
                .map_err(llvm_err)?;
        }

        let buf = self.build_malloc(total, "interp.buf")?;
        let mut offset = i64_type.const_zero();
        for piece in pieces {
            let dst = self.byte_offset(buf, offset, "interp.dst")?;
            self.build_memcpy_call(dst, piece.ptr, piece.len)?;
            offset = self
                .builder
                .build_int_add(offset, piece.len, "interp.offset")
                .map_err(llvm_err)?;
        }

        self.build_string_value(buf, total)
    }

    /// Release a rendered piece's buffer, if the rendering allocated it.
    fn free_piece(&self, piece: &Piece<'ctx>) -> CodegenResult<()> {
        match piece.owner {
            PieceOwner::Borrowed => Ok(()),
            PieceOwner::Owned => {
                let free_fn = self.get_or_declare_free();
                self.builder
                    .build_call(free_fn, &[piece.ptr.into()], "")
                    .map_err(llvm_err)?;
                Ok(())
            }
            PieceOwner::OwnedUnlessSameAs(source) => self.free_if_distinct(piece.ptr, source),
        }
    }

    /// `if candidate != other { free(candidate) }`.
    ///
    /// The pass-through transforms return their input when the text already has the
    /// requested shape, so the two pointers can name the same buffer. Comparing them is
    /// what separates "this transform allocated" from "this transform did nothing", and
    /// it is the only way to tell, since the choice is made at runtime.
    fn free_if_distinct(
        &self,
        candidate: PointerValue<'ctx>,
        other: PointerValue<'ctx>,
    ) -> CodegenResult<()> {
        let parent_fn = self.current_function.ok_or_else(|| {
            CodegenError::InternalError("interpolation emitted outside function".to_string())
        })?;
        let same = self
            .builder
            .build_int_compare(inkwell::IntPredicate::EQ, candidate, other, "interp.same")
            .map_err(llvm_err)?;
        let free_bb = self.context.append_basic_block(parent_fn, "interp.free");
        let cont_bb = self.context.append_basic_block(parent_fn, "interp.kept");
        self.builder
            .build_conditional_branch(same, cont_bb, free_bb)
            .map_err(llvm_err)?;

        self.builder.position_at_end(free_bb);
        let free_fn = self.get_or_declare_free();
        self.builder
            .build_call(free_fn, &[candidate.into()], "")
            .map_err(llvm_err)?;
        self.builder
            .build_unconditional_branch(cont_bb)
            .map_err(llvm_err)?;

        self.builder.position_at_end(cont_bb);
        Ok(())
    }

    /// Apply a transform that may return its input untouched, folding the input's
    /// ownership into the result's.
    ///
    /// When the input was ours, it is released here unless the transform handed it
    /// straight back — in which case it *is* the result and stays ours. When the input
    /// was borrowed, the result is ours only if the transform allocated, which the
    /// consumer settles by the same pointer comparison.
    fn chain_transform(
        &self,
        result: BasicValueEnum<'ctx>,
        source: PointerValue<'ctx>,
        source_owner: PieceOwner<'ctx>,
    ) -> CodegenResult<(BasicValueEnum<'ctx>, PieceOwner<'ctx>)> {
        match source_owner {
            PieceOwner::Owned => {
                let (result_ptr, _) = self.split_string_value(result)?;
                self.free_if_distinct(source, result_ptr)?;
                Ok((result, PieceOwner::Owned))
            }
            _ => Ok((result, PieceOwner::OwnedUnlessSameAs(source))),
        }
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
        incoming: PieceOwner<'ctx>,
    ) -> CodegenResult<(BasicValueEnum<'ctx>, PieceOwner<'ctx>)> {
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

        let (rendered, owner) = match &target {
            Type::String => self.render_string(value, spec, incoming)?,
            Type::Bool => self.render_bool(value, spec)?,
            Type::Char => self.render_char(value, spec)?,
            other if other.is_integer() => self.render_integer(value, other, spec)?,
            other if other.is_float() => self.render_float(value, spec)?,
            Type::Struct(name) => self.render_struct_debug(value, name)?,
            other => {
                return Err(CodegenError::InternalError(format!(
                    "type {:?} reached interpolation codegen; semantic analysis rejects it",
                    other
                )))
            }
        };

        self.apply_width(rendered, owner, spec)
    }

    /// Render a `@derive(Debug)` struct as `Name { field: value, ... }`.
    ///
    /// Only reachable under a `{x:?}` hole — a struct has no `Display` form — and every
    /// field is rendered with the same Debug kind, which is what quotes a nested
    /// `string` and recurses into a nested struct. A field-less struct renders as its
    /// bare name, with no braces to hold nothing.
    fn render_struct_debug(
        &self,
        value: BasicValueEnum<'ctx>,
        name: &str,
    ) -> CodegenResult<(BasicValueEnum<'ctx>, PieceOwner<'ctx>)> {
        let fields = self
            .struct_defs
            .get(name)
            .ok_or_else(|| CodegenError::UnsupportedType(format!("unknown struct '{}'", name)))?
            .clone();
        if fields.is_empty() {
            return self.render_literal_text(self.written_struct_name(name));
        }

        let aggregate = value.into_struct_value();
        let field_spec = FormatSpec {
            kind: FormatKind::Debug,
            ..FormatSpec::default()
        };

        let mut pieces: Vec<Piece<'ctx>> = Vec::with_capacity(fields.len() * 2 + 2);
        pieces.push(self.text_piece(&format!("{} {{ ", self.written_struct_name(name)))?);
        for (index, (field_name, field_ty)) in fields.iter().enumerate() {
            if index > 0 {
                pieces.push(self.text_piece(", ")?);
            }
            pieces.push(self.text_piece(&format!("{}: ", field_name))?);
            let field_value = self
                .builder
                .build_extract_value(aggregate, index as u32, &format!("dbg.{}", field_name))
                .map_err(llvm_err)?;
            let (rendered, owner) =
                self.render_hole(field_value, field_ty, &field_spec, PieceOwner::Borrowed)?;
            let (ptr, len) = self.split_string_value(rendered)?;
            pieces.push(Piece { ptr, len, owner });
        }
        pieces.push(self.text_piece(" }")?);

        let joined = self.build_concat(&pieces)?;
        // Each field rendering is dead once its bytes are in the joined buffer, exactly
        // as they are for the holes of the literal this struct sits in.
        for piece in &pieces {
            self.free_piece(piece)?;
        }
        Ok((joined, PieceOwner::Owned))
    }

    /// The name the programmer wrote for struct key `name`. The two differ only for a
    /// monomorphized generic instance, whose mangled key appears in no source text and
    /// must not appear in a rendering of the value either.
    fn written_struct_name<'a>(&'a self, name: &'a str) -> &'a str {
        self.struct_written_names
            .get(name)
            .map(String::as_str)
            .unwrap_or(name)
    }

    /// A `.rodata` literal as a rendered piece — the punctuation a struct's debug form
    /// is framed with, and the whole rendering of a field-less one.
    fn text_piece(&self, text: &str) -> CodegenResult<Piece<'ctx>> {
        let global = self
            .builder
            .build_global_string_ptr(text, "interp.dbg.text")
            .map_err(llvm_err)?;
        Ok(Piece {
            ptr: global.as_pointer_value(),
            len: self.context.i64_type().const_int(text.len() as u64, false),
            owner: PieceOwner::Borrowed,
        })
    }

    fn render_literal_text(
        &self,
        text: &str,
    ) -> CodegenResult<(BasicValueEnum<'ctx>, PieceOwner<'ctx>)> {
        let piece = self.text_piece(text)?;
        Ok((
            self.build_string_value(piece.ptr, piece.len)?,
            PieceOwner::Borrowed,
        ))
    }

    /// A `string` hole borrows the caller's bytes. Debug quoting is the exception: it
    /// builds a new buffer around them.
    fn render_string(
        &self,
        value: BasicValueEnum<'ctx>,
        spec: &FormatSpec,
        incoming: PieceOwner<'ctx>,
    ) -> CodegenResult<(BasicValueEnum<'ctx>, PieceOwner<'ctx>)> {
        if spec.kind == FormatKind::Debug {
            let (source, _) = self.split_string_value(value)?;
            let quoted = self.build_quoted(value, b'"')?;
            // `__neuro_quote` always allocates, so a buffer we brought in is now dead.
            if matches!(incoming, PieceOwner::Owned) {
                let free_fn = self.get_or_declare_free();
                self.builder
                    .build_call(free_fn, &[source.into()], "")
                    .map_err(llvm_err)?;
            }
            return Ok((quoted, PieceOwner::Owned));
        }
        Ok((value, incoming))
    }

    /// Both spellings are `.rodata` globals selected between, so nothing is allocated.
    fn render_bool(
        &self,
        value: BasicValueEnum<'ctx>,
        _spec: &FormatSpec,
    ) -> CodegenResult<(BasicValueEnum<'ctx>, PieceOwner<'ctx>)> {
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
        Ok((self.build_string_value(ptr, len)?, PieceOwner::Borrowed))
    }

    /// The UTF-8 encoding is always a fresh buffer; quoting replaces it with another,
    /// so the encoding is released as soon as the quoted copy exists.
    fn render_char(
        &self,
        value: BasicValueEnum<'ctx>,
        spec: &FormatSpec,
    ) -> CodegenResult<(BasicValueEnum<'ctx>, PieceOwner<'ctx>)> {
        let utf8 = self.get_or_define_utf8()?;
        let encoded = self
            .builder
            .build_call(utf8, &[value.into()], "interp.char")
            .map_err(llvm_err)?
            .try_as_basic_value()
            .basic()
            .ok_or_else(|| CodegenError::InternalError("utf8 helper returned void".to_string()))?;

        if spec.kind == FormatKind::Debug {
            let (encoded_ptr, _) = self.split_string_value(encoded)?;
            let quoted = self.build_quoted(encoded, b'\'')?;
            // `__neuro_quote` always allocates, so the encoding it read is now dead.
            let free_fn = self.get_or_declare_free();
            self.builder
                .build_call(free_fn, &[encoded_ptr.into()], "")
                .map_err(llvm_err)?;
            return Ok((quoted, PieceOwner::Owned));
        }
        Ok((encoded, PieceOwner::Owned))
    }

    /// Every integer rendering goes through a helper that allocates its result.
    fn render_integer(
        &self,
        value: BasicValueEnum<'ctx>,
        ty: &Type,
        spec: &FormatSpec,
    ) -> CodegenResult<(BasicValueEnum<'ctx>, PieceOwner<'ctx>)> {
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
            let text = self
                .builder
                .build_call(helper, &[widened_unsigned.into()], "interp.bin")
                .map_err(llvm_err)?
                .try_as_basic_value()
                .basic()
                .ok_or_else(|| {
                    CodegenError::InternalError("binary helper returned void".to_string())
                })?;
            return Ok((text, PieceOwner::Owned));
        }

        // The renderer takes a magnitude and a sign byte rather than a printf
        // conversion. Only signed decimal can carry a sign at all: the checker rejects
        // `+` on an unsigned value and on every radix conversion, and a radix rendering
        // shows the bit pattern, which is never negative.
        let i8_type = self.context.i8_type();
        let no_sign = i8_type.const_zero();
        let (magnitude, sign, radix, upper) = match spec.kind {
            FormatKind::LowerHex => (widened_unsigned, no_sign, 16, false),
            FormatKind::UpperHex => (widened_unsigned, no_sign, 16, true),
            FormatKind::Octal => (widened_unsigned, no_sign, 8, false),
            _ if ty.is_unsigned_int() => (widened_unsigned, no_sign, 10, false),
            _ => {
                let signed = self
                    .builder
                    .build_int_s_extend(raw, i64_type, "interp.int")
                    .map_err(llvm_err)?;
                let negative = self
                    .builder
                    .build_int_compare(
                        inkwell::IntPredicate::SLT,
                        signed,
                        i64_type.const_zero(),
                        "interp.int.neg",
                    )
                    .map_err(llvm_err)?;
                // Plain wrapping negation: `0 - i64::MIN` is `i64::MIN`'s own bit
                // pattern, which read as unsigned is the magnitude wanted.
                let negated = self
                    .builder
                    .build_int_sub(i64_type.const_zero(), signed, "interp.int.abs")
                    .map_err(llvm_err)?;
                let magnitude = self
                    .builder
                    .build_select(negative, negated, signed, "interp.int.mag")
                    .map_err(llvm_err)?
                    .into_int_value();
                let positive = if spec.plus_sign {
                    i8_type.const_int(u64::from(b'+'), false)
                } else {
                    no_sign
                };
                let sign = self
                    .builder
                    .build_select(
                        negative,
                        i8_type.const_int(u64::from(b'-'), false),
                        positive,
                        "interp.int.sign",
                    )
                    .map_err(llvm_err)?
                    .into_int_value();
                (magnitude, sign, 10, false)
            }
        };

        let helper = self.get_or_define_fmt_int(radix, upper)?;
        let text = self
            .builder
            .build_call(helper, &[magnitude.into(), sign.into()], "interp.int.text")
            .map_err(llvm_err)?
            .try_as_basic_value()
            .basic()
            .ok_or_else(|| CodegenError::InternalError("int format returned void".to_string()))?;
        Ok((text, PieceOwner::Owned))
    }

    /// `snprintf` renders into a fresh buffer; the two fix-ups that follow may replace
    /// it with another or hand it straight back, so ownership is chained through them.
    fn render_float(
        &self,
        value: BasicValueEnum<'ctx>,
        spec: &FormatSpec,
    ) -> CodegenResult<(BasicValueEnum<'ctx>, PieceOwner<'ctx>)> {
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
            let fixed = self
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
                })?;
            return self.chain_transform(fixed, ptr, PieceOwner::Owned);
        }

        if spec.kind == FormatKind::Fixed {
            return Ok((text, PieceOwner::Owned));
        }

        let (ptr, len) = self.split_string_value(text)?;
        let pointed = self
            .builder
            .build_call(
                self.get_or_define_ensure_point()?,
                &[ptr.into(), len.into()],
                "interp.float.point",
            )
            .map_err(llvm_err)?
            .try_as_basic_value()
            .basic()
            .ok_or_else(|| CodegenError::InternalError("float fix-up returned void".to_string()))?;
        self.chain_transform(pointed, ptr, PieceOwner::Owned)
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
        owner: PieceOwner<'ctx>,
        spec: &FormatSpec,
    ) -> CodegenResult<(BasicValueEnum<'ctx>, PieceOwner<'ctx>)> {
        let Some(width) = spec.width.filter(|width| *width > 0) else {
            return Ok((value, owner));
        };

        let align = match spec.align {
            Some(FormatAlign::Left) => ALIGN_LEFT,
            Some(FormatAlign::Center) => ALIGN_CENTER,
            Some(FormatAlign::Right) | None => ALIGN_RIGHT,
        };
        let fill = if spec.zero_pad { b'0' } else { b' ' };

        let (ptr, len) = self.split_string_value(value)?;
        let helper = self.get_or_define_pad()?;
        let padded = self
            .builder
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
            .ok_or_else(|| CodegenError::InternalError("pad helper returned void".to_string()))?;
        self.chain_transform(padded, ptr, owner)
    }
}
