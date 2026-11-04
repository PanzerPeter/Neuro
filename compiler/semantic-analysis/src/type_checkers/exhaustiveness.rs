// Exhaustiveness of a `match`: Maranget's usefulness check over a pattern matrix, in the
// form that answers with a witness, a value no row matches. Each row is a guardless
// alternative and each column a part of the scrutinee, so a nested pattern splits into
// its own columns once its constructor is chosen. Integers and `char` are too wide to
// enumerate: only a row that matches anything there can cover them.

use ast_types::{EnumPatternPayload, MatchArm, Pattern};
use shared_types::{Literal, Span};

use super::{EnumVariantInfo, TypeChecker, VariantForm};
use crate::errors::TypeError;
use crate::types::{ArrayLen, Type};

/// Matrix cells one check may visit. Usefulness is NP-complete in the width of a payload,
/// and adversarial source must not hang the compiler; real matches stay far below this.
const MAX_STEPS: usize = 1 << 20;

/// A pattern reduced to what coverage needs.
enum Cover {
    Any,
    /// Tests a value of an open type (a literal or range), or is already an error.
    /// Either way it cannot complete a column.
    Opaque,
    Bool(bool),
    Variant {
        index: usize,
        fields: Vec<Cover>,
    },
    /// An array pattern: `prefix` elements, then anything when `open`.
    Seq {
        prefix: Vec<Cover>,
        open: bool,
    },
}

static ANY: Cover = Cover::Any;

type Row<'p> = Vec<&'p Cover>;

/// One matrix walk, charging every visit against [`MAX_STEPS`].
struct Search<'c> {
    checker: &'c TypeChecker,
    steps: usize,
}

impl TypeChecker {
    /// Verify the arms cover every possible scrutinee value.
    pub(super) fn check_exhaustive(&mut self, arms: &[MatchArm], scrut_ty: &Type, span: Span) {
        // A guarded arm may not fire, so it never contributes to exhaustiveness.
        let covers: Vec<Cover> = arms
            .iter()
            .filter(|arm| arm.guard.is_none())
            .flat_map(|arm| &arm.patterns)
            .map(|pattern| self.cover_of(pattern, scrut_ty))
            .collect();
        let rows: Vec<Row<'_>> = covers.iter().map(|cover| vec![cover]).collect();
        let mut search = Search {
            checker: self,
            steps: 0,
        };
        let witness = search.witness(&rows, std::slice::from_ref(scrut_ty));
        let exceeded = search.steps > MAX_STEPS;
        let reason = if exceeded {
            "these patterns are too complex to verify".to_string()
        } else {
            let Some(witness) = witness else {
                return;
            };
            self.missing_reason(&covers, scrut_ty, &witness.join(", "))
        };
        self.record_error(TypeError::NonExhaustiveMatch { reason, span });
    }

    /// The report for a scrutinee `witness` escapes: whole variants when some have no arm
    /// at all, else the value itself.
    fn missing_reason(&self, covers: &[Cover], scrut_ty: &Type, witness: &str) -> String {
        match scrut_ty {
            Type::Enum(name) => {
                let variants = self.enum_defs.get(name).map_or(&[][..], Vec::as_slice);
                let missing: Vec<&str> = variants
                    .iter()
                    .enumerate()
                    .filter(|(i, _)| {
                        !covers
                            .iter()
                            .any(|c| matches!(c, Cover::Variant { index, .. } if index == i))
                    })
                    .map(|(_, v)| v.name.as_str())
                    .collect();
                if missing.is_empty() {
                    format!("no arm matches `{witness}`")
                } else {
                    format!("unhandled variant(s): {}", missing.join(", "))
                }
            }
            Type::Bool => "not all `bool` values are handled".to_string(),
            Type::Array { .. } | Type::Reference { .. } => {
                format!("no arm matches `{witness}`")
            }
            // Integers and `char` have too many values to enumerate; they require a
            // wildcard arm.
            _ => format!("a `{}` match requires a `_` wildcard arm", scrut_ty),
        }
    }

    /// Reduce `pattern`, read at `ty`, to its coverage. A pattern that does not fit `ty`
    /// was reported when it was checked.
    fn cover_of(&self, pattern: &Pattern, ty: &Type) -> Cover {
        match pattern {
            Pattern::Wildcard(_) | Pattern::Binding(_) => Cover::Any,
            Pattern::At { pattern, .. } => self.cover_of(pattern, ty),
            Pattern::Literal(Literal::Boolean(b), _) => Cover::Bool(*b),
            Pattern::Literal(_, _) | Pattern::Range { .. } | Pattern::UnqualifiedEnum { .. } => {
                Cover::Opaque
            }
            Pattern::Enum {
                variant, payload, ..
            } => self.variant_cover(&variant.name, payload, ty),
            Pattern::Array { elements, rest, .. } => {
                let Some(element) = sequence_element(ty) else {
                    return Cover::Opaque;
                };
                Cover::Seq {
                    prefix: elements.iter().map(|e| self.cover_of(e, element)).collect(),
                    open: rest.is_some(),
                }
            }
        }
    }

    fn variant_cover(&self, variant: &str, payload: &EnumPatternPayload, ty: &Type) -> Cover {
        let Type::Enum(name) = ty.referent() else {
            return Cover::Opaque;
        };
        let variants = self.enum_defs.get(name).map_or(&[][..], Vec::as_slice);
        let Some(index) = variants.iter().position(|v| v.name == variant) else {
            return Cover::Opaque;
        };
        let info = &variants[index];
        let fields = info
            .fields
            .iter()
            .enumerate()
            .map(|(slot, (field_name, field_ty))| {
                let sub = match payload {
                    EnumPatternPayload::Unit => None,
                    EnumPatternPayload::Tuple(subs) => subs.get(slot),
                    EnumPatternPayload::Struct(field_pats) => field_pats
                        .iter()
                        .find(|fp| Some(fp.field.name.as_str()) == field_name.as_deref())
                        .map(|fp| &fp.pattern),
                };
                sub.map_or(Cover::Any, |sub| self.cover_of(sub, field_ty))
            })
            .collect();
        Cover::Variant { index, fields }
    }
}

impl<'c> Search<'c> {
    /// A value of the columns `tys` that no row matches, one rendered pattern per column,
    /// or `None` when the rows cover them all.
    fn witness<'p>(&mut self, rows: &[Row<'p>], tys: &[Type]) -> Option<Vec<String>> {
        self.steps += 1 + rows.len();
        if self.steps > MAX_STEPS {
            return None;
        }
        let Some((head, rest)) = tys.split_first() else {
            return rows.is_empty().then(Vec::new);
        };
        if rows.is_empty() {
            return Some(vec!["_".to_string(); tys.len()]);
        }
        let checker = self.checker;
        match head.referent() {
            Type::Enum(name) => {
                let variants = checker.enum_defs.get(name).map_or(&[][..], Vec::as_slice);
                self.enum_witness(rows, name, variants, rest)
            }
            Type::Bool => self.bool_witness(rows, rest),
            Type::Array { element, size } => {
                let fixed = match size {
                    ArrayLen::Fixed(n) => Some(*n),
                    _ => None,
                };
                self.seq_witness(rows, element, fixed, rest)
            }
            Type::Slice(element) => self.seq_witness(rows, element, None, rest),
            _ => self.default_witness(rows, rest, "_".to_string()),
        }
    }

    /// The rows that hold whatever the head column is, with that column gone. A column
    /// some constructor is absent from reduces to this: only these rows reach that value.
    fn default_witness<'p>(
        &mut self,
        rows: &[Row<'p>],
        rest: &[Type],
        head: String,
    ) -> Option<Vec<String>> {
        let remaining: Vec<Row<'p>> = rows
            .iter()
            .filter(|row| matches!(row[0], Cover::Any))
            .map(|row| row[1..].to_vec())
            .collect();
        let mut witness = self.witness(&remaining, rest)?;
        witness.insert(0, head);
        Some(witness)
    }

    fn enum_witness<'p>(
        &mut self,
        rows: &[Row<'p>],
        name: &str,
        variants: &[EnumVariantInfo],
        rest: &[Type],
    ) -> Option<Vec<String>> {
        let has_row = |i: usize| {
            rows.iter()
                .any(|row| matches!(row[0], Cover::Variant { index, .. } if *index == i))
        };
        if let Some(absent) = (0..variants.len()).find(|i| !has_row(*i)) {
            let info = &variants[absent];
            let fields = vec!["_".to_string(); info.fields.len()];
            let head = self.render_variant(name, info, &fields);
            return self.default_witness(rows, rest, head);
        }
        for (i, info) in variants.iter().enumerate() {
            let specialized: Vec<Row<'p>> = rows
                .iter()
                .filter_map(|row| match row[0] {
                    Cover::Variant { index, fields } if *index == i => {
                        Some(fields.iter().chain(row[1..].iter().copied()).collect())
                    }
                    Cover::Any => {
                        let wild = std::iter::repeat_n(&ANY, info.fields.len());
                        Some(wild.chain(row[1..].iter().copied()).collect())
                    }
                    _ => None,
                })
                .collect();
            let tys: Vec<Type> = info
                .fields
                .iter()
                .map(|(_, ty)| ty.clone())
                .chain(rest.iter().cloned())
                .collect();
            if let Some(mut witness) = self.witness(&specialized, &tys) {
                let tail = witness.split_off(info.fields.len());
                let head = self.render_variant(name, info, &witness);
                return Some(std::iter::once(head).chain(tail).collect());
            }
        }
        None
    }

    fn bool_witness<'p>(&mut self, rows: &[Row<'p>], rest: &[Type]) -> Option<Vec<String>> {
        for value in [true, false] {
            let specialized: Vec<Row<'p>> = rows
                .iter()
                .filter(|row| match row[0] {
                    Cover::Bool(b) => *b == value,
                    other => matches!(other, Cover::Any),
                })
                .map(|row| row[1..].to_vec())
                .collect();
            if let Some(mut witness) = self.witness(&specialized, rest) {
                witness.insert(0, value.to_string());
                return Some(witness);
            }
        }
        None
    }

    /// An array of known length is one constructor, wide enough for the longest prefix: a
    /// rest covers everything past it. A slice is one constructor per length, and every
    /// length past the longest pattern behaves alike, so one more stands for all of them.
    fn seq_witness<'p>(
        &mut self,
        rows: &[Row<'p>],
        element: &Type,
        fixed: Option<usize>,
        rest: &[Type],
    ) -> Option<Vec<String>> {
        let longest = rows
            .iter()
            .map(|row| match row[0] {
                Cover::Seq { prefix, .. } => prefix.len(),
                _ => 0,
            })
            .max()
            .unwrap_or(0);
        let lengths = match fixed {
            Some(_) => longest..=longest,
            None => 0..=longest + 1,
        };
        for len in lengths {
            let specialized: Vec<Row<'p>> = rows
                .iter()
                .filter_map(|row| {
                    let (prefix, open) = match row[0] {
                        Cover::Seq { prefix, open } => (prefix.as_slice(), *open),
                        Cover::Any => (&[][..], true),
                        _ => return None,
                    };
                    let fits =
                        fixed.is_some() || prefix.len() == len || (open && prefix.len() < len);
                    fits.then(|| {
                        let wild = std::iter::repeat_n(&ANY, len - prefix.len());
                        prefix
                            .iter()
                            .chain(wild)
                            .chain(row[1..].iter().copied())
                            .collect()
                    })
                })
                .collect();
            let tys: Vec<Type> = std::iter::repeat_n(element.clone(), len)
                .chain(rest.iter().cloned())
                .collect();
            if let Some(mut witness) = self.witness(&specialized, &tys) {
                let tail = witness.split_off(len);
                let closed = fixed == Some(len) || (fixed.is_none() && len <= longest);
                if !closed {
                    witness.push("..".to_string());
                }
                let head = format!("[{}]", witness.join(", "));
                return Some(std::iter::once(head).chain(tail).collect());
            }
        }
        None
    }

    fn render_variant(&self, name: &str, info: &EnumVariantInfo, fields: &[String]) -> String {
        let base = self
            .checker
            .enum_instance_base(name)
            .unwrap_or_else(|| name.to_string());
        let path = format!("{base}::{}", info.name);
        match info.form {
            VariantForm::Unit => path,
            VariantForm::Tuple => format!("{path}({})", fields.join(", ")),
            VariantForm::Struct => {
                let named: Vec<String> = info
                    .fields
                    .iter()
                    .zip(fields)
                    .map(|((field, _), value)| {
                        format!("{}: {value}", field.as_deref().unwrap_or("_"))
                    })
                    .collect();
                format!("{path} {{ {} }}", named.join(", "))
            }
        }
    }
}

/// The element type of the array or borrowed slice `ty`.
fn sequence_element(ty: &Type) -> Option<&Type> {
    match ty.referent() {
        Type::Array { element, .. } | Type::Slice(element) => Some(element),
        _ => None,
    }
}
