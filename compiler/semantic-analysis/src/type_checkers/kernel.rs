// `@kernel` form rules.
//
// `@kernel(threads: [...])` runs a free function's body once per thread of a launch grid:
// one thread per element of its first `&mut` tensor, in blocks shaped `threads`. What is
// checked here is the attribute, the signature the grid is read from, and the two names
// the body alone can see, `thread_id` and `block_id`. Which statements a body may use is
// the GPU backend's call, made per body when the program is compiled, as for `@gpu`.

use ast_types::{Attribute, Expr, FunctionDef, Item};
use shared_types::{Identifier, Literal, Span};

use crate::errors::TypeError;
use crate::types::{ArrayLen, Type};

use super::TypeChecker;

pub(crate) const KERNEL_ATTRIBUTE: &str = "kernel";

const THREADS_LABEL: &str = "threads";

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
                "returns nothing: a kernel hands back what it writes through its `&mut` tensors",
                func.name.span,
            );
        }
        for (param, ty) in func.params.iter().zip(&params) {
            if let Some(problem) = parameter_problem(ty) {
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
                "needs a `&mut Tensor` parameter: the grid runs one thread per element of the first one",
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

/// Why a kernel cannot take a parameter of type `ty`, if it cannot. A kernel reads
/// scalars by value and tensors through `&`, and writes only through `&mut` tensors; each
/// tensor's shape must be known when the program is compiled, since the grid is built then.
fn parameter_problem(ty: &Type) -> Option<&'static str> {
    match ty {
        Type::Reference { inner, .. } => match inner.as_ref() {
            Type::Tensor { element, shape } => {
                if !element.is_numeric() {
                    Some("is a tensor of a non-numeric element")
                } else if shape.iter().any(|axis| axis.extent == ArrayLen::Dynamic) {
                    Some("has a `?` extent; a kernel's tensors have static shapes")
                } else {
                    None
                }
            }
            _ => Some("is a reference to something other than a tensor"),
        },
        Type::Tensor { .. } => Some(
            "takes a tensor by value; pass it as `&Tensor` to read it, `&mut Tensor` to write it",
        ),
        scalar if scalar.is_numeric() || matches!(scalar, Type::Bool) => None,
        _ => {
            Some("has a type a kernel cannot take: a number, a `bool`, `&Tensor` or `&mut Tensor`")
        }
    }
}
