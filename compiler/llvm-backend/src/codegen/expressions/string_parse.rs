// Codegen for `string.parse::<T>()`: one call into the embedded text parser, then the
// `Result<T, ParseError>` built from the status it returns.
//
// The parser lives in `text_parse.c`, linked in only by a module that calls it. Every entry
// writes the value's bits to one `u64` slot, so this file narrows them to `T` and nothing
// more.

use inkwell::AddressSpace;
use inkwell::IntPredicate;
use inkwell::types::{BasicMetadataTypeEnum, BasicTypeEnum, IntType};
use inkwell::values::{BasicMetadataValueEnum, BasicValueEnum, IntValue, PointerValue};
use neuro_hir::HirExpr;

use crate::codegen::context::CodegenContext;
use crate::codegen::external_bodies::TEXT_PARSER_ENTRY_POINTS;
use crate::errors::{CodegenError, CodegenResult};
use crate::types::Type;

const PARSE_INT_FN: &str = "__neuro_parse_int";
const PARSE_FLOAT_FN: &str = "__neuro_parse_float";
const PARSE_BOOL_FN: &str = "__neuro_parse_bool";

const PARSE_ERROR_ENUM: &str = "ParseError";

/// The parser's status words, numbered in `text_parse.c`. Zero is success, and any other
/// value past these two is `OutOfRange`.
const STATUS_EMPTY: u64 = 1;
const STATUS_INVALID: u64 = 2;

impl<'ctx> CodegenContext<'ctx> {
    /// Whether this module calls the text parser, so it needs linking in.
    pub(crate) fn requires_text_parser(&self) -> bool {
        TEXT_PARSER_ENTRY_POINTS
            .iter()
            .any(|name| self.module.get_function(name).is_some())
    }

    /// Lower `string.parse::<T>()`. `result_ty` is the monomorphized `Result<T, ParseError>`;
    /// `T` is read from its `Ok` payload.
    pub(super) fn codegen_string_parse(
        &mut self,
        result_ty: &Type,
        receiver: &HirExpr,
    ) -> CodegenResult<BasicValueEnum<'ctx>> {
        let (enum_name, ok_tag, target) = self.parse_result_shape(result_ty)?;
        let err_tag = self.enum_variant_tag(&enum_name, "Err")?;

        let fat = self.string_receiver_struct(receiver)?;
        let text = self
            .builder
            .build_extract_value(fat, 0, "parse.text")?
            .into_pointer_value();
        let len = self
            .builder
            .build_extract_value(fat, 1, "parse.len")?
            .into_int_value();

        let i64_type = self.context.i64_type();
        let slot = self.entry_alloca(i64_type, "parse.bits")?;
        self.builder.build_store(slot, i64_type.const_zero())?;
        let status = self.call_text_parser(&target, text, len, slot)?;
        self.release_string_temporary(receiver, fat.into())?;

        let bits = self
            .builder
            .build_load(i64_type, slot, "parse.bits.read")?
            .into_int_value();
        let payload = self.narrow_parsed_bits(bits, &target)?;
        let ok = self.codegen_enum_value(&enum_name, ok_tag, &[(payload, &target)])?;

        let reason = self.parse_error_for(status)?;
        let error_ty = Type::Enum(PARSE_ERROR_ENUM.to_string());
        let err = self.codegen_enum_value(&enum_name, err_tag, &[(reason, &error_ty)])?;

        let succeeded = self.builder.build_int_compare(
            IntPredicate::EQ,
            status,
            status.get_type().const_zero(),
            "parse.ok",
        )?;
        self.builder
            .build_select(succeeded, ok, err, "parse.result")
            .map_err(CodegenError::from)
    }

    /// The instance name, the `Ok` tag and `T` of a `Result<T, ParseError>`.
    fn parse_result_shape(&self, result_ty: &Type) -> CodegenResult<(String, u32, Type)> {
        let shape = match result_ty {
            Type::Enum(name) => {
                let ok_tag = self.enum_variant_tag(name, "Ok")?;
                self.type_mapper
                    .enum_payload_types(name)
                    .and_then(|variants| variants.get(ok_tag as usize))
                    .and_then(|fields| fields.first())
                    .map(|target| (name.clone(), ok_tag, target.clone()))
            }
            _ => None,
        };
        shape.ok_or_else(|| {
            CodegenError::InternalError(format!(
                "`.parse` result is not a `Result`: {:?}",
                result_ty
            ))
        })
    }

    /// Call the entry that reads `target`, answering the status word.
    fn call_text_parser(
        &mut self,
        target: &Type,
        text: PointerValue<'ctx>,
        len: IntValue<'ctx>,
        slot: PointerValue<'ctx>,
    ) -> CodegenResult<IntValue<'ctx>> {
        let i32_type = self.context.i32_type();
        let i64_type = self.context.i64_type();
        let ptr_type = self.context.ptr_type(AddressSpace::default());

        let (name, limits): (&str, Vec<IntValue<'ctx>>) = match target {
            Type::Bool => (PARSE_BOOL_FN, vec![]),
            Type::F16 => (PARSE_FLOAT_FN, float_format(i32_type, 10, 5)),
            Type::BF16 => (PARSE_FLOAT_FN, float_format(i32_type, 7, 8)),
            Type::F32 => (PARSE_FLOAT_FN, float_format(i32_type, 23, 8)),
            Type::F64 => (PARSE_FLOAT_FN, float_format(i32_type, 52, 11)),
            _ => {
                let bits = self
                    .type_mapper
                    .map_type(target)?
                    .into_int_type()
                    .get_bit_width();
                let (positive, negative) = integer_limits(bits, target.is_unsigned_int());
                (
                    PARSE_INT_FN,
                    vec![
                        i64_type.const_int(positive, false),
                        i64_type.const_int(negative, false),
                    ],
                )
            }
        };

        let mut params: Vec<BasicMetadataTypeEnum<'ctx>> = vec![ptr_type.into(), i64_type.into()];
        params.extend(
            limits
                .iter()
                .map(|limit| BasicMetadataTypeEnum::from(limit.get_type())),
        );
        params.push(ptr_type.into());
        let parser = self.extern_fn(name, i32_type.fn_type(&params, false));

        let mut args: Vec<BasicMetadataValueEnum<'ctx>> = vec![text.into(), len.into()];
        args.extend(
            limits
                .iter()
                .map(|&limit| BasicMetadataValueEnum::from(limit)),
        );
        args.push(slot.into());
        self.builder
            .build_call(parser, &args, "parse.status")?
            .try_as_basic_value()
            .basic()
            .map(BasicValueEnum::into_int_value)
            .ok_or_else(|| CodegenError::InternalError("text parser returned void".into()))
    }

    /// The `T` whose bits the parser wrote. A failed parse leaves zero here, which the
    /// `Ok` arm carries harmlessly: the select below takes the `Err` value.
    fn narrow_parsed_bits(
        &self,
        bits: IntValue<'ctx>,
        target: &Type,
    ) -> CodegenResult<BasicValueEnum<'ctx>> {
        let narrow = |int_type: IntType<'ctx>| -> CodegenResult<IntValue<'ctx>> {
            if int_type == bits.get_type() {
                return Ok(bits);
            }
            self.builder
                .build_int_truncate(bits, int_type, "parse.narrow")
                .map_err(CodegenError::from)
        };
        match self.type_mapper.map_type(target)? {
            BasicTypeEnum::IntType(int_type) => Ok(narrow(int_type)?.into()),
            BasicTypeEnum::FloatType(float_type) => {
                let carrier = match target {
                    Type::F16 | Type::BF16 => self.context.i16_type(),
                    Type::F32 => self.context.i32_type(),
                    _ => self.context.i64_type(),
                };
                self.builder
                    .build_bit_cast(narrow(carrier)?, float_type, "parse.float")
                    .map_err(CodegenError::from)
            }
            other => Err(CodegenError::InternalError(format!(
                "`.parse` target lowered to {:?}",
                other
            ))),
        }
    }

    /// The `ParseError` a failing status names. A success status lands on `OutOfRange`,
    /// which is never read.
    fn parse_error_for(&mut self, status: IntValue<'ctx>) -> CodegenResult<BasicValueEnum<'ctx>> {
        let empty_tag = self.enum_variant_tag(PARSE_ERROR_ENUM, "Empty")?;
        let invalid_tag = self.enum_variant_tag(PARSE_ERROR_ENUM, "Invalid")?;
        let range_tag = self.enum_variant_tag(PARSE_ERROR_ENUM, "OutOfRange")?;
        let empty = self.codegen_enum_value(PARSE_ERROR_ENUM, empty_tag, &[])?;
        let invalid = self.codegen_enum_value(PARSE_ERROR_ENUM, invalid_tag, &[])?;
        let out_of_range = self.codegen_enum_value(PARSE_ERROR_ENUM, range_tag, &[])?;

        let status_type = status.get_type();
        let is_empty = self.builder.build_int_compare(
            IntPredicate::EQ,
            status,
            status_type.const_int(STATUS_EMPTY, false),
            "parse.is.empty",
        )?;
        let is_invalid = self.builder.build_int_compare(
            IntPredicate::EQ,
            status,
            status_type.const_int(STATUS_INVALID, false),
            "parse.is.invalid",
        )?;
        let otherwise =
            self.builder
                .build_select(is_invalid, invalid, out_of_range, "parse.reason.rest")?;
        self.builder
            .build_select(is_empty, empty, otherwise, "parse.reason")
            .map_err(CodegenError::from)
    }
}

/// The two arguments that name a float format: its stored fraction and exponent widths.
fn float_format<'ctx>(
    i32_type: IntType<'ctx>,
    fraction_bits: u64,
    exponent_bits: u64,
) -> Vec<IntValue<'ctx>> {
    vec![
        i32_type.const_int(fraction_bits, false),
        i32_type.const_int(exponent_bits, false),
    ]
}

/// The largest magnitude an integer type holds on each side of zero.
fn integer_limits(bits: u32, unsigned: bool) -> (u64, u64) {
    if unsigned {
        (u64::MAX >> (64 - bits), 0)
    } else {
        let negative = 1u64 << (bits - 1);
        (negative - 1, negative)
    }
}

#[cfg(test)]
mod tests {
    use super::integer_limits;

    #[test]
    fn integer_limits_span_each_width() {
        assert_eq!(integer_limits(8, true), (255, 0));
        assert_eq!(integer_limits(64, true), (u64::MAX, 0));
        assert_eq!(integer_limits(8, false), (127, 128));
        assert_eq!(integer_limits(64, false), (i64::MAX as u64, 1 << 63));
    }
}
