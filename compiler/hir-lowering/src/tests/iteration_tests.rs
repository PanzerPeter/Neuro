//! The `for`-loop protocol desugar: the shape it lowers to, and the fast paths it
//! must not disturb. No HIR node is added for the protocol, so these tests assert on
//! the `VarDecl` + `While` + `Match` the desugar builds out of existing ones.

use super::{function_body, lower};
use neuro_hir::{HirBindingSource, HirExprKind, HirMatchTest, HirStmt, HirType};

/// The protocol traits and a source iterator, declared exactly as the prelude declares
/// them. The lowering slice sees no prelude, so each program brings its own.
const PROTOCOL_DECLS: &str = r#"
enum Option<T> { Some(T), None }

trait Iterator {
    type Item
    func next(&mut self) -> Option<Self::Item>
}

trait IntoIterator {
    type Item
    type Iter
    func into_iter(self) -> Self::Iter
}

@derive(Copy, Clone)
struct CountIter { at: i32, end: i32 }

impl Iterator for CountIter {
    type Item = i32
    func next(&mut self) -> Option<i32> {
        if self.at >= self.end { return Option::None }
        val current = self.at
        self.at = self.at + 1
        Option::Some(current)
    }
}

@derive(Copy, Clone)
struct Count { end: i32 }

impl IntoIterator for Count {
    type Item = i32
    type Iter = CountIter
    func into_iter(self) -> CountIter {
        CountIter { at: 0, end: self.end }
    }
}
"#;

/// The `Block` a protocol `for` lowers to, taken from the last statement of `main`.
fn protocol_loop(body: &[HirStmt]) -> &[HirStmt] {
    for stmt in body {
        let HirStmt::Expr(expr) = stmt else { continue };
        if let HirExprKind::Block { stmts } = &expr.kind {
            return stmts;
        }
    }
    panic!("no protocol loop block in {body:?}");
}

#[test]
fn a_container_head_calls_into_iter_once_before_the_loop() {
    let program = lower(&format!(
        "{PROTOCOL_DECLS}
         func main() -> i32 {{
            val c = Count {{ end: 3 }}
            for v in c {{ }}
            0
         }}"
    ));
    let stmts = protocol_loop(function_body(&program, "main"));

    let HirStmt::VarDecl {
        name,
        init,
        mutable,
        ..
    } = &stmts[0]
    else {
        panic!(
            "the desugar must open with the iterator binding, got {:?}",
            stmts[0]
        );
    };
    assert!(name.starts_with("__iter_"), "generated name, got {name}");
    assert!(
        mutable,
        "`next(&mut self)` needs a mutable iterator binding"
    );

    let init = init.as_ref().expect("the iterator binding is initialized");
    assert_eq!(init.ty, HirType::Struct("CountIter".to_string()));
    let HirExprKind::Call { callee, args } = &init.kind else {
        panic!(
            "the container is routed through a call, got {:?}",
            init.kind
        );
    };
    assert!(args.is_empty(), "`into_iter` takes no arguments");
    let HirExprKind::FieldAccess { field, .. } = &callee.kind else {
        panic!("the callee names the method, got {:?}", callee.kind);
    };
    assert_eq!(field, "into_iter");
}

/// A type that is already an `Iterator` needs no `into_iter` call: the head IS the
/// iterator, and inserting a call would look for a method that does not exist.
#[test]
fn an_iterator_head_is_used_directly() {
    let program = lower(&format!(
        "{PROTOCOL_DECLS}
         func main() -> i32 {{
            val it = CountIter {{ at: 0, end: 3 }}
            for v in it {{ }}
            0
         }}"
    ));
    let stmts = protocol_loop(function_body(&program, "main"));

    let HirStmt::VarDecl { init, .. } = &stmts[0] else {
        panic!("expected the iterator binding, got {:?}", stmts[0]);
    };
    let init = init.as_ref().expect("the iterator binding is initialized");
    assert!(
        matches!(init.kind, HirExprKind::Variable(_)),
        "the head is the iterator, got {:?}",
        init.kind
    );
}

#[test]
fn the_loop_steps_through_a_two_arm_match_on_next() {
    let program = lower(&format!(
        "{PROTOCOL_DECLS}
         func main() -> i32 {{
            val c = Count {{ end: 3 }}
            for v in c {{ }}
            0
         }}"
    ));
    let stmts = protocol_loop(function_body(&program, "main"));

    let HirStmt::While {
        condition, body, ..
    } = stmts.last().expect("the loop follows")
    else {
        panic!("the desugar emits a while loop, got {:?}", stmts.last());
    };
    assert_eq!(condition.ty, HirType::Bool);

    let [HirStmt::Expr(step)] = &body[..] else {
        panic!("the loop body is one match, got {body:?}");
    };
    let HirExprKind::Match { scrutinee, arms } = &step.kind else {
        panic!("the step is a match, got {:?}", step.kind);
    };

    let HirExprKind::Call { callee, .. } = &scrutinee.kind else {
        panic!("the scrutinee is a call, got {:?}", scrutinee.kind);
    };
    let HirExprKind::FieldAccess { object, field } = &callee.kind else {
        panic!("the callee names the method, got {:?}", callee.kind);
    };
    assert_eq!(field, "next");
    assert_eq!(
        object.ty,
        HirType::Struct("CountIter".to_string()),
        "the receiver carries the iterator's type, not the call's result"
    );

    assert_eq!(arms.len(), 2);
    assert_eq!(arms[0].tests, vec![HirMatchTest::Tag { tag: 0 }]);
    assert_eq!(arms[0].bindings.len(), 1);
    assert_eq!(arms[0].bindings[0].name, "v");
    assert_eq!(arms[0].bindings[0].ty, HirType::I32);
    assert_eq!(
        arms[0].bindings[0].source,
        HirBindingSource::EnumPayload { slot: 0 }
    );

    assert_eq!(arms[1].tests, vec![HirMatchTest::Wildcard]);
    let HirExprKind::Block { stmts: exit } = &arms[1].body.kind else {
        panic!("the exhausted arm is a block, got {:?}", arms[1].body.kind);
    };
    assert!(
        matches!(exit[..], [HirStmt::Break { .. }]),
        "the exhausted arm leaves the loop, got {exit:?}"
    );
}

/// The label rides on the emitted `while`, which is what a `break outer` inside the
/// body resolves against.
#[test]
fn a_label_lands_on_the_emitted_loop() {
    let program = lower(&format!(
        "{PROTOCOL_DECLS}
         func main() -> i32 {{
            val c = Count {{ end: 3 }}
            outer: for v in c {{ break outer }}
            0
         }}"
    ));
    let stmts = protocol_loop(function_body(&program, "main"));
    let HirStmt::While { label, .. } = stmts.last().expect("the loop follows") else {
        panic!("expected the while loop, got {:?}", stmts.last());
    };
    assert_eq!(label.as_deref(), Some("outer"));
}

/// An enumerated head gets its own cursor binding, advanced inside the arm so a
/// `continue` cannot skip it.
#[test]
fn an_enumerated_head_declares_and_advances_a_cursor() {
    let program = lower(&format!(
        "{PROTOCOL_DECLS}
         func main() -> i32 {{
            val c = Count {{ end: 3 }}
            for (i, v) in c.enumerate() {{ }}
            0
         }}"
    ));
    let stmts = protocol_loop(function_body(&program, "main"));

    let HirStmt::VarDecl { name, ty, .. } = &stmts[1] else {
        panic!(
            "the cursor follows the iterator binding, got {:?}",
            stmts[1]
        );
    };
    assert!(
        name.starts_with("__iter_pos_"),
        "generated name, got {name}"
    );
    assert_eq!(*ty, HirType::U64);

    let HirStmt::While { body, .. } = stmts.last().expect("the loop follows") else {
        panic!("expected the while loop");
    };
    let [HirStmt::Expr(step)] = &body[..] else {
        panic!("the loop body is one match");
    };
    let HirExprKind::Match { arms, .. } = &step.kind else {
        panic!("the step is a match");
    };
    let HirExprKind::Block { stmts: arm } = &arms[0].body.kind else {
        panic!("the yielding arm is a block");
    };
    let HirStmt::VarDecl { name: bound, .. } = &arm[0] else {
        panic!("the position is read out first, got {:?}", arm[0]);
    };
    assert_eq!(bound, "i");
    assert!(
        matches!(arm[1], HirStmt::Assignment { .. }),
        "the cursor advances before the user's statements, got {:?}",
        arm[1]
    );
}

/// The built-in sequence heads keep their counted-loop nodes: the protocol adds a path,
/// it does not replace theirs.
#[test]
fn array_and_range_heads_keep_their_counted_loops() {
    let program = lower(&format!(
        "{PROTOCOL_DECLS}
         func main() -> i32 {{
            val xs: [i32; 3] = [1, 2, 3]
            for x in xs {{ }}
            for i in 0..3 {{ }}
            0
         }}"
    ));
    let body = function_body(&program, "main");
    assert!(
        body.iter().any(|s| matches!(s, HirStmt::ForEach { .. })),
        "an array head stays a ForEach, got {body:?}"
    );
    assert!(
        body.iter().any(|s| matches!(s, HirStmt::ForRange { .. })),
        "a range head stays a ForRange, got {body:?}"
    );
}

/// The prelude's codepoint iterator, declared here because the lowering slice sees no
/// prelude. `.chars()` builds one, so its shape has to match what the lowering emits.
const CHARS_DECLS: &str = r#"
struct Chars { source: &string, offset: u64 }

impl Iterator for Chars {
    type Item = char
    func next(&mut self) -> Option<char> {
        if self.offset >= self.source.len() { return Option::None }
        val scalar = self.source.__char_at(self.offset)
        self.offset = self.offset + 1
        Option::Some(scalar)
    }
}
"#;

/// `.chars()` never reaches a backend as a call: it is the iterator's own literal, so
/// the receiver's borrow and the zero cursor are visible right here.
#[test]
fn chars_lowers_to_the_iterator_literal() {
    let program = lower(&format!(
        "{PROTOCOL_DECLS}{CHARS_DECLS}
         func main() -> i32 {{
            val s = \"hi\"
            val it = s.chars()
            0
         }}"
    ));
    let init = super::binding_init(function_body(&program, "main"), "it");
    let HirExprKind::StructLiteral { name, fields, .. } = &init.kind else {
        panic!("`.chars()` builds the iterator, got {:?}", init.kind);
    };
    assert_eq!(name, "Chars");
    assert_eq!(fields[0].name, "source");
    assert!(
        matches!(fields[0].value.kind, HirExprKind::Reference { .. }),
        "the text is borrowed, not moved, got {:?}",
        fields[0].value.kind
    );
    assert_eq!(fields[1].name, "offset");
    assert_eq!(fields[1].value.ty, HirType::U64);
}

/// `.char_indices()` binds byte offsets, and a byte offset lives on the iterator: the
/// loop samples it before stepping, and never counts steps of its own.
#[test]
fn char_indices_samples_the_iterator_cursor_before_each_step() {
    let program = lower(&format!(
        "{PROTOCOL_DECLS}{CHARS_DECLS}
         func main() -> i32 {{
            val s = \"hi\"
            for (o, c) in s.char_indices() {{ }}
            0
         }}"
    ));
    let stmts = protocol_loop(function_body(&program, "main"));
    let HirStmt::While { body, .. } = stmts.last().expect("the loop is the last statement") else {
        panic!("the desugar ends in a while, got {:?}", stmts.last());
    };

    let HirStmt::Assignment { target, value, .. } = &body[0] else {
        panic!("the cursor is sampled first, got {:?}", body[0]);
    };
    assert!(target.starts_with("__iter_pos_"), "got {target}");
    let HirExprKind::FieldAccess { field, .. } = &value.kind else {
        panic!("the sample reads the iterator, got {:?}", value.kind);
    };
    assert_eq!(field, "offset");

    let HirStmt::Expr(step) = &body[1] else {
        panic!("the step follows the sample, got {:?}", body[1]);
    };
    let HirExprKind::Match { arms, .. } = &step.kind else {
        panic!("the step is a match");
    };
    let HirExprKind::Block { stmts: arm } = &arms[0].body.kind else {
        panic!("the yielding arm is a block");
    };
    assert!(
        !arm.iter().any(|s| matches!(s, HirStmt::Assignment { .. })),
        "a byte cursor is advanced by `next`, not by the loop, got {arm:?}"
    );
}
