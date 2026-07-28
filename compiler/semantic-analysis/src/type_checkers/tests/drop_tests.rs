#[allow(unused_imports)]
use super::{make_function, make_ident, make_type, semantic_errors};
use crate::errors::TypeError;

#[test]
fn valid_drop_impl_is_accepted() {
    // `impl Drop for T { func drop(&mut self) }` is the recognized lang-item shape.
    let errors = semantic_errors(
        r#"
struct Handle { id: i32 }

impl Drop for Handle {
    func drop(&mut self) { }
}

func main() -> i32 { 0 }
"#,
    );
    assert!(
        errors.is_empty(),
        "a well-formed Drop impl must type-check; got {errors:?}"
    );
}

#[test]
fn drop_type_cannot_be_copy() {
    let errors = semantic_errors(
        r#"
@derive(Copy)
struct Bad { x: i32 }

impl Drop for Bad {
    func drop(&mut self) { }
}

func main() -> i32 { 0 }
"#,
    );
    assert!(
        errors
            .iter()
            .any(|e| matches!(e, TypeError::DropTypeCannotBeCopy { .. })),
        "a Copy type may not implement Drop; got {errors:?}"
    );
}

#[test]
fn drop_with_ref_self_is_rejected() {
    // `Drop::drop` must take `&mut self` so the destructor can release resources.
    let errors = semantic_errors(
        r#"
struct H { x: i32 }

impl Drop for H {
    func drop(&self) { }
}

func main() -> i32 { 0 }
"#,
    );
    assert!(
        errors
            .iter()
            .any(|e| matches!(e, TypeError::InvalidDropImpl { .. })),
        "drop must take `&mut self`; got {errors:?}"
    );
}

#[test]
fn drop_with_extra_method_is_rejected() {
    let errors = semantic_errors(
        r#"
struct H { x: i32 }

impl Drop for H {
    func drop(&mut self) { }
    func other(&self) -> i32 { self.x }
}

func main() -> i32 { 0 }
"#,
    );
    assert!(
        errors
            .iter()
            .any(|e| matches!(e, TypeError::InvalidDropImpl { .. })),
        "an `impl Drop` block must contain only `drop`; got {errors:?}"
    );
}
