// `@kernel` form rules.
//
// `@kernel(threads: [...])` runs a free function's body once per thread of a launch grid:
// one thread per element of its first `KernelOut` tensor, in blocks shaped `threads`. What
// is checked here is the attribute, the signature the grid is read from, and the names the
// body alone can see: `thread_id`, `block_id` and its `KernelOut` handles. Which statements
// a body may use is the GPU backend's call, made per body when the program is compiled, as
// for `@gpu`.
//
// A kernel's tensors are borrowed at the call, never moved: a bare `Tensor<T, S>` input is
// checked as `&Tensor<T, S>` and a `KernelOut<Tensor<T, S>>` output as `&mut Tensor<T, S>`,
// so the borrow rules of ordinary code (a temporary, a moved binding, `k(r, &mut r)`) apply
// at the call unchanged.

use std::borrow::Cow;

use ast_types::{Attribute, Expr, FunctionDef, GenericArg, Item};
use shared_types::{Identifier, Literal, Span};

use crate::errors::TypeError;
use crate::types::{ArrayLen, TensorAxis, Type};

use super::TypeChecker;

pub(crate) const KERNEL_ATTRIBUTE: &str = "kernel";

/// The handle a kernel writes a tensor through. The compiler builds it at the call from a
/// `&mut` tensor, so no other position can name it.
const KERNEL_OUT_TYPE: &str = "KernelOut";

const THREADS_LABEL: &str = "threads";

/// The safe write form: `out.partition(|base, slice| { ... })`.
const PARTITION_METHOD: &str = "partition";

/// No NVIDIA or AMD GPU runs a block of more threads than this.
const MAX_THREADS_PER_BLOCK: u64 = 1024;

/// A launch grid has at most three axes, `x`, `y` and `z`.
const GRID_AXES: [&str; 3] = ["x", "y", "z"];

/// The names only a kernel body can see, each with one `u32` field per grid axis.
const GRID_NAMES: [&str; 2] = ["thread_id", "block_id"];

/// Attributes that cannot share a function with `@kernel`: `@gpu` lowers a whole-tensor
/// body a different way, and `@grad` would give it a derivative the host runs.
const EXCLUSIVE_ATTRIBUTES: [&str; 2] = ["gpu", "grad"];

impl TypeChecker {
    pub(crate) fn check_kernel_attributes(&mut self, items: &[Item]) {
        for item in items {
            match item {
                Item::Function(func) => self.check_kernel_function(func),
                Item::Impl(def) => {
                    for method in &def.methods {
                        if let Some(attr) = kernel_attribute(&method.attributes) {
                            self.kernel_error(
                                "on a method is not supported; move the work into a free `@kernel` function",
                                attr.span,
                            );
                        }
                    }
                }
                _ => {}
            }
        }
    }

    fn check_kernel_function(&mut self, func: &FunctionDef) {
        let Some(attr) = kernel_attribute(&func.attributes) else {
            return;
        };
        if let Some(other) = func
            .attributes
            .iter()
            .find(|a| EXCLUSIVE_ATTRIBUTES.contains(&a.name.name.as_str()))
        {
            let problem = format!("cannot share a function with `@{}`", other.name.name);
            self.kernel_error(&problem, attr.span);
        }
        let threads = match threads_argument(attr) {
            Ok(threads) => Some(threads),
            Err(problem) => {
                self.kernel_error(&problem, attr.span);
                None
            }
        };
        let Some((params, ret)) = self.lookup_registered_signature(func) else {
            return;
        };
        if !matches!(ret, Type::Void) {
            self.kernel_error(
                "returns nothing: a kernel hands back what it writes through its `KernelOut` tensors",
                func.name.span,
            );
        }
        for (param, ty) in func.params.iter().zip(&params) {
            if let Some(problem) = parameter_problem(&param.ty, ty) {
                let problem = format!("parameter '{}' {problem}", param.name.name);
                self.kernel_error(&problem, param.name.span);
            }
        }
        let grid = params.iter().find_map(|ty| match ty {
            Type::Reference {
                inner,
                mutable: true,
                ..
            } => match inner.as_ref() {
                Type::Tensor { shape, .. } => Some(shape.len()),
                _ => None,
            },
            _ => None,
        });
        match (grid, threads) {
            (None, _) => self.kernel_error(
                "needs a `KernelOut<Tensor<T, S>>` parameter: the grid runs one thread per element of the first one",
                func.name.span,
            ),
            (Some(rank), Some(threads)) if rank != threads.len() => {
                let problem = format!(
                    "gives `threads:` {} entries for a rank-{rank} grid tensor: one per axis",
                    threads.len()
                );
                self.kernel_error(&problem, attr.span);
            }
            _ => {}
        }
    }

    /// `thread_id.x` or `block_id.z` inside a kernel body, where no binding of that name
    /// shadows it, as the `u32` it reads. `None` when `object` is anything else.
    pub(crate) fn check_grid_position(
        &mut self,
        object: &Expr,
        field: &Identifier,
    ) -> Option<Type> {
        let Expr::Identifier(name) = object else {
            return None;
        };
        if !self.is_grid_name(&name.name) {
            return None;
        }
        if !GRID_AXES.contains(&field.name.as_str()) {
            let problem = format!(
                "body has no `{}.{}`: the grid axes are `x`, `y` and `z`",
                name.name, field.name
            );
            self.kernel_error(&problem, field.span);
        }
        Some(Type::U32)
    }

    /// Whether `name` is `thread_id` or `block_id` as the kernel body sees them: inside a
    /// `@kernel` function and not shadowed by a binding of its own.
    pub(crate) fn is_grid_name(&self, name: &str) -> bool {
        self.in_kernel && GRID_NAMES.contains(&name) && self.symbols.lookup(name).is_none()
    }

    /// A kernel-only name read whole rather than one axis at a time.
    pub(crate) fn refuse_whole_grid_name(&mut self, name: &Identifier) {
        let problem = format!(
            "body reads `{}` one axis at a time: `.x`, `.y` or `.z`",
            name.name
        );
        self.kernel_error(&problem, name.span);
    }

    /// A parameter type as a kernel's body and callers see it: a bare `Tensor<T, S>` is
    /// the `&Tensor<T, S>` the call lends it, and `KernelOut<Tensor<T, S>>` the
    /// `&mut Tensor<T, S>` the call builds it from. Anything else resolves as written.
    pub(crate) fn resolve_kernel_param(&mut self, ty: &ast_types::Type) -> Option<Type> {
        let (written, mutable) = match ty {
            ast_types::Type::Tensor { .. } => (ty, false),
            ast_types::Type::Generic { name, args, span } if self.is_kernel_out(&name.name) => {
                let [GenericArg::Type(inner @ ast_types::Type::Tensor { .. })] = args.as_slice()
                else {
                    self.kernel_error(
                        "output `KernelOut<T>` wraps one tensor type: `KernelOut<Tensor<T, S>>`",
                        *span,
                    );
                    return None;
                };
                (inner, true)
            }
            _ => return self.resolve_type(ty),
        };
        let inner = self.resolve_type(written)?;
        Some(Type::Reference {
            inner: Box::new(inner),
            mutable,
        })
    }

    /// Whether `name` in a type position means the kernel output handle rather than a
    /// generic type the program declares under that name.
    pub(crate) fn is_kernel_out(&self, name: &str) -> bool {
        name == KERNEL_OUT_TYPE && !self.is_generic_struct(name) && !self.is_generic_enum(name)
    }

    /// `KernelOut` named anywhere but as a kernel parameter's whole type.
    pub(crate) fn refuse_kernel_out_type(&mut self, span: Span) {
        self.kernel_error(
            "output `KernelOut<T>` is only a kernel parameter's type; the call builds it from a `&mut` tensor",
            span,
        );
    }

    /// Remember which of the kernel being checked's parameters are `KernelOut` handles,
    /// once they are bound in the body's scope.
    pub(crate) fn enter_kernel_outs(&mut self, func: &FunctionDef) {
        self.kernel_outs = func
            .params
            .iter()
            .filter(|param| {
                matches!(&param.ty, ast_types::Type::Generic { name, .. } if self.is_kernel_out(&name.name))
            })
            .map(|param| {
                let tensor = self
                    .symbols
                    .lookup(&param.name.name)
                    .map_or(Type::Unknown, |info| info.ty.referent().clone());
                (param.name.name.clone(), tensor)
            })
            .collect();
        self.kernel_out_scope = self.symbols.depth().saturating_sub(1);
    }

    /// Check the object of an index, the one place a `KernelOut` handle may be named:
    /// `out[i, j]` reaches a single element, never the handle.
    pub(crate) fn check_index_base(&mut self, object: &Expr) -> Option<Type> {
        if let Expr::Identifier(name) = object {
            self.indexed_kernel_out = Some(name.span);
        }
        let ty = self.check_expr(object, None);
        self.indexed_kernel_out = None;
        ty
    }

    /// Whether `name`, read here, is one of the kernel's `KernelOut` parameters rather
    /// than a binding that shadows one.
    pub(crate) fn names_kernel_out(&self, name: &str) -> bool {
        self.kernel_outs.iter().any(|(out, _)| out == name)
            && self.symbols.defining_depth(name) == Some(self.kernel_out_scope)
    }

    /// `out.partition(|base, slice| { ... })` on one of the kernel's output handles, the
    /// write form whose disjointness the compiler proves: every thread that owns an
    /// element of the grid tensor gets its own run of `out`, all runs equal. `None` when
    /// the call is not one, so it is checked as any other method call.
    pub(crate) fn check_kernel_partition(
        &mut self,
        object: &Expr,
        method: &Identifier,
        args: &[Expr],
        span: Span,
    ) -> Option<Type> {
        let Expr::Identifier(out) = object else {
            return None;
        };
        if method.name != PARTITION_METHOD || !self.in_kernel || !self.names_kernel_out(&out.name) {
            return None;
        }
        let form = format!(
            "output '{}' is partitioned by one closure of two parameters, `{}.partition(|base, slice| {{ ... }})`",
            out.name, out.name
        );
        let [
            Expr::Closure {
                params,
                ret,
                body,
                span: closure_span,
                ..
            },
        ] = args
        else {
            self.kernel_error(&form, span);
            return Some(Type::Void);
        };
        if params.len() != 2 {
            self.kernel_error(&form, *closure_span);
            return Some(Type::Void);
        }
        let tensor = self
            .kernel_outs
            .iter()
            .find(|(name, _)| *name == out.name)
            .map(|(_, tensor)| tensor.clone());
        let Some(Type::Tensor { element, shape }) = tensor else {
            return Some(Type::Void);
        };
        let fixed = [
            Type::U64,
            Type::Reference {
                inner: Box::new(Type::Slice(element)),
                mutable: true,
            },
        ];
        self.check_closure(
            params,
            ret.as_ref(),
            body,
            Some((&fixed, &Type::Void)),
            *closure_span,
        );
        self.check_partition_share(&out.name, &shape, span);
        Some(Type::Void)
    }

    /// Refuse a partition of `out` whose elements the grid's threads cannot share
    /// equally. A shape a generic parameter sizes is only known per instance, so the GPU
    /// lowering checks that one.
    fn check_partition_share(&mut self, out: &str, shape: &[TensorAxis], span: Span) {
        let grid = match self.kernel_outs.first() {
            Some((_, Type::Tensor { shape, .. })) => element_count(shape),
            _ => None,
        };
        let (Some(elements), Some(threads)) = (element_count(shape), grid) else {
            return;
        };
        if threads == 0 || elements.is_multiple_of(threads) {
            return;
        }
        let problem = format!(
            "output '{out}' has {elements} elements, which the grid's {threads} threads cannot share equally: `partition` gives each thread the same number"
        );
        self.kernel_error(&problem, span);
    }

    /// Refuse a `KernelOut` handle read other than as an index base: bound, returned,
    /// passed, borrowed or called on, it would carry a write path out of the thread
    /// that owns it.
    pub(crate) fn check_kernel_out_read(&mut self, name: &Identifier) {
        if self.names_kernel_out(&name.name) && self.indexed_kernel_out != Some(name.span) {
            self.refuse_kernel_out_use(&name.name, name.span);
        }
    }

    pub(crate) fn refuse_kernel_out_use(&mut self, name: &str, span: Span) {
        let problem = format!(
            "output '{name}' is written one element at a time, `unsafe {{ {name}[i] = v }}`, or through `{name}.partition(...)`; the handle cannot be bound, returned, passed, borrowed or captured"
        );
        self.kernel_error(&problem, span);
    }

    /// Refuse a store through a `KernelOut` handle's index outside `unsafe`: the compiler
    /// cannot prove that no two threads write one element, so the block marks where the
    /// programmer vouches for it.
    pub(crate) fn check_kernel_out_write(&mut self, object: &Expr) {
        let Expr::Identifier(out) = object else {
            return;
        };
        if self.unsafe_depth > 0 || !self.names_kernel_out(&out.name) {
            return;
        }
        let problem = format!(
            "output '{0}' is written by index only inside `unsafe {{ }}`, which vouches that no two threads write one element; `{0}.partition(...)` needs no `unsafe`",
            out.name
        );
        self.kernel_error(&problem, out.span);
    }

    /// The arguments of a call to `callee`, with each one a kernel borrows written as the
    /// borrow it is; unchanged when `callee` is not a kernel.
    pub(crate) fn borrow_kernel_inputs<'a>(
        &mut self,
        callee: &str,
        args: &'a [Expr],
    ) -> Cow<'a, [Expr]> {
        let Some(inputs) = self.kernel_inputs.get(callee).cloned() else {
            return Cow::Borrowed(args);
        };
        let mut borrowed = args.to_vec();
        for (arg, input) in borrowed.iter_mut().zip(inputs) {
            if !input {
                continue;
            }
            if let Expr::Reference { span, .. } = arg {
                let problem = format!(
                    "call borrows its `Tensor` inputs itself: pass the tensor, not `&`, to '{callee}'"
                );
                self.kernel_error(&problem, *span);
                continue;
            }
            let span = arg.span();
            *arg = Expr::Reference {
                operand: Box::new(arg.clone()),
                mutable: false,
                span,
            };
        }
        Cow::Owned(borrowed)
    }

    /// A kernel named as a value. A function type cannot say that its tensors are
    /// borrowed at the call and its outputs built from `&mut`, so a kernel is only called
    /// by name.
    pub(crate) fn refuse_kernel_value(&mut self, name: &Identifier) {
        let problem = format!(
            "function '{}' is called by name; it is not a function value",
            name.name
        );
        self.kernel_error(&problem, name.span);
    }

    fn kernel_error(&mut self, problem: &str, span: Span) {
        self.record_error(TypeError::KernelForm {
            problem: problem.to_string(),
            span,
        });
    }
}

pub(crate) fn kernel_attribute(attributes: &[Attribute]) -> Option<&Attribute> {
    attributes
        .iter()
        .find(|attr| attr.name.name == KERNEL_ATTRIBUTE)
}

/// `threads:` given once, as an array literal of one to three positive integer literals
/// whose product a GPU block can hold. The launch is built at compile time, so nothing
/// the program computes can size it.
fn threads_argument(attr: &Attribute) -> Result<Vec<u64>, String> {
    const FORM: &str =
        "takes `threads:` once, as an array of one to three positive integer literals";
    let [arg] = attr.named.as_slice() else {
        return Err(FORM.to_string());
    };
    let Expr::ArrayLiteral { elements, .. } = &arg.value else {
        return Err(FORM.to_string());
    };
    if !attr.args.is_empty()
        || arg.label.name != THREADS_LABEL
        || elements.is_empty()
        || elements.len() > GRID_AXES.len()
    {
        return Err(FORM.to_string());
    }
    let threads = elements
        .iter()
        .map(|element| match element {
            Expr::Literal(Literal::Integer(n, _), _) if *n > 0 => {
                u64::try_from(*n).map_err(|_| FORM.to_string())
            }
            _ => Err(FORM.to_string()),
        })
        .collect::<Result<Vec<u64>, _>>()?;
    let per_block = threads
        .iter()
        .try_fold(1u64, |product, &n| product.checked_mul(n))
        .unwrap_or(u64::MAX);
    if per_block > MAX_THREADS_PER_BLOCK {
        return Err(format!(
            "asks for {per_block} threads in a block; no GPU runs more than {MAX_THREADS_PER_BLOCK}"
        ));
    }
    Ok(threads)
}

/// How many elements a tensor of `shape` holds, when every extent is a number.
fn element_count(shape: &[TensorAxis]) -> Option<u64> {
    shape
        .iter()
        .try_fold(1u64, |count, axis| match axis.extent {
            ArrayLen::Fixed(extent) => count.checked_mul(extent as u64),
            _ => None,
        })
}

/// Why a kernel cannot take a parameter written `written` and resolved to `ty`, if it
/// cannot. A kernel reads scalars by value and tensors as `Tensor<T, S>`, and writes only
/// through `KernelOut<Tensor<T, S>>`; each tensor's shape must be known when the program
/// is compiled, since the grid is built then.
fn parameter_problem(written: &ast_types::Type, ty: &Type) -> Option<&'static str> {
    match ty {
        // Already reported where it failed to resolve.
        Type::Unknown => None,
        Type::Reference { inner, .. }
            if matches!(
                written,
                ast_types::Type::Tensor { .. } | ast_types::Type::Generic { .. }
            ) =>
        {
            match inner.as_ref() {
                Type::Tensor { element, .. } if !element.is_numeric() => {
                    Some("is a tensor of a non-numeric element")
                }
                Type::Tensor { shape, .. }
                    if shape.iter().any(|axis| axis.extent == ArrayLen::Dynamic) =>
                {
                    Some("has a `?` extent; a kernel's tensors have static shapes")
                }
                Type::Tensor { .. } => None,
                _ => Some(TAKES),
            }
        }
        Type::Reference { .. } => Some(
            "is a reference; a kernel reads a `Tensor<T, S>` and writes a `KernelOut<Tensor<T, S>>`, both borrowed at the call",
        ),
        scalar if scalar.is_numeric() || matches!(scalar, Type::Bool) => None,
        _ => Some(TAKES),
    }
}

const TAKES: &str = "has a type a kernel cannot take: a number, a `bool`, `Tensor<T, S>` or `KernelOut<Tensor<T, S>>`";
