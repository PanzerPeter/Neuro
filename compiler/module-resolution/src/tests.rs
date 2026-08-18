//! Unit tests for discovery, collision detection, and qualifier stripping.
//!
//! The AST fixtures are built by hand rather than parsed: this slice must compile without
//! `syntax-parsing`, so the parser it is handed here is a stub that maps a marker line to
//! the item or reference it stands for.

use std::path::{Path, PathBuf};

use ast_types::{
    EnumPatternPayload, Expr, FunctionDef, ImportDef, ImportName, ImportSelection, Item, MatchArm,
    Parameter, Pattern, Stmt, Type,
};
use shared_types::{Identifier, Literal, Span};
use tempfile::TempDir;

use crate::{resolve_program, ModuleError, ResolvedProgram};

fn ident(name: &str) -> Identifier {
    Identifier {
        name: name.to_string(),
        span: Span::new(0, 0),
    }
}

fn function(name: &str, body: Vec<Stmt>) -> Item {
    Item::Function(FunctionDef {
        name: ident(name),
        generics: Vec::new(),
        lifetimes: Vec::new(),
        where_predicates: Vec::new(),
        params: Vec::new(),
        return_type: None,
        body,
        attributes: Vec::new(),
        span: Span::new(0, 0),
    })
}

/// A function whose one parameter is annotated with `ty` — the type-position site.
fn function_taking(name: &str, ty: &str) -> Item {
    let Item::Function(mut def) = function(name, Vec::new()) else {
        unreachable!("function() builds a function")
    };
    def.params.push(Parameter {
        name: ident("value"),
        ty: Type::Named(ident(ty)),
        span: Span::new(0, 0),
    });
    Item::Function(def)
}

/// A statement calling the path `qualifier::member`.
fn call_path(qualifier: &str, member: &str) -> Stmt {
    Stmt::Expr(Expr::Call {
        func: Box::new(Expr::Path {
            type_name: ident(qualifier),
            member: ident(member),
            span: Span::new(0, 0),
        }),
        type_args: Vec::new(),
        args: Vec::new(),
        span: Span::new(0, 0),
    })
}

/// A statement calling the bare name `callee`.
fn call_bare(callee: &str) -> Stmt {
    Stmt::Expr(Expr::Call {
        func: Box::new(Expr::Identifier(ident(callee))),
        type_args: Vec::new(),
        args: Vec::new(),
        span: Span::new(0, 0),
    })
}

/// A statement matching on `0` with `pattern` as its only arm.
fn match_on(pattern: Pattern) -> Stmt {
    Stmt::Expr(Expr::Match {
        scrutinee: Box::new(Expr::Literal(Literal::Integer(0, None), Span::new(0, 0))),
        arms: vec![MatchArm {
            patterns: vec![pattern],
            guard: None,
            body: Box::new(Expr::Literal(Literal::Integer(0, None), Span::new(0, 0))),
            span: Span::new(0, 0),
        }],
        span: Span::new(0, 0),
    })
}

/// Build one `import` item from the stub's `import <rest>` line.
fn import_item(rest: &str) -> Result<Item, String> {
    let (relative, rest) = match rest.strip_prefix("./") {
        Some(stripped) => (true, stripped),
        None => (false, rest),
    };

    let segments = |path: &str| path.split("::").map(ident).collect::<Vec<_>>();
    let (path, selection) = if let Some((head, list)) = rest.split_once("::{") {
        let list = list
            .strip_suffix('}')
            .ok_or_else(|| format!("unterminated import list: {}", rest))?;
        let names = list
            .split(',')
            .map(str::trim)
            .map(|entry| match entry.split_once(" as ") {
                Some((name, alias)) => ImportName {
                    name: ident(name),
                    alias: Some(ident(alias)),
                    span: Span::new(0, 0),
                },
                None => ImportName {
                    name: ident(entry),
                    alias: None,
                    span: Span::new(0, 0),
                },
            })
            .collect();
        (segments(head), ImportSelection::List(names))
    } else if let Some((path, alias)) = rest.split_once(" as ") {
        (segments(path), ImportSelection::Alias(ident(alias)))
    } else {
        (segments(rest), ImportSelection::Module)
    };

    Ok(Item::Import(ImportDef {
        relative,
        path,
        selection,
        span: Span::new(0, 0),
    }))
}

/// The stub parser: each line is `func NAME`, `call QUALIFIER::MEMBER`, `bare NAME`,
/// `param NAME: TYPE`, `import ...`, `pat NAME(BIND)`, or `patbare NAME` — enough
/// surface to drive every resolution rule.
fn stub_parse(source: &str) -> Result<Vec<Item>, String> {
    let mut items = Vec::new();
    let mut calls = Vec::new();
    for line in source.lines().map(str::trim).filter(|l| !l.is_empty()) {
        if let Some(name) = line.strip_prefix("func ") {
            items.push(function(name, Vec::new()));
        } else if let Some(path) = line.strip_prefix("call ") {
            let (qualifier, member) = path
                .rsplit_once("::")
                .ok_or_else(|| format!("bad call line: {}", line))?;
            calls.push(call_path(qualifier, member));
        } else if let Some(name) = line.strip_prefix("bare ") {
            calls.push(call_bare(name));
        } else if let Some(rest) = line.strip_prefix("param ") {
            let (name, ty) = rest
                .split_once(": ")
                .ok_or_else(|| format!("bad param line: {}", line))?;
            items.push(function_taking(name, ty));
        } else if let Some(rest) = line.strip_prefix("import ") {
            items.push(import_item(rest)?);
        } else if let Some(rest) = line.strip_prefix("pat ") {
            let (variant, binding) = rest
                .split_once('(')
                .ok_or_else(|| format!("bad pat line: {}", line))?;
            let binding = binding
                .strip_suffix(')')
                .ok_or_else(|| format!("bad pat line: {}", line))?;
            calls.push(match_on(Pattern::UnqualifiedEnum {
                variant: ident(variant),
                payload: EnumPatternPayload::Tuple(vec![Pattern::Binding(ident(binding))]),
                span: Span::new(0, 0),
            }));
        } else if let Some(name) = line.strip_prefix("patbare ") {
            calls.push(match_on(Pattern::Binding(ident(name))));
        } else {
            return Err(format!("unparsable line: {}", line));
        }
    }
    if !calls.is_empty() {
        // Calls land in the last declared function so a fixture never has to name one; a
        // file that declares none gets an `entry` to hold them.
        match items.iter_mut().rev().find_map(|item| match item {
            Item::Function(def) => Some(def),
            _ => None,
        }) {
            Some(def) => def.body.extend(calls),
            None => items.push(function("entry", calls)),
        }
    }
    Ok(items)
}

fn write(dir: &Path, rel: &str, source: &str) -> PathBuf {
    let path = dir.join(rel);
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).expect("create module directory");
    }
    std::fs::write(&path, source).expect("write module file");
    path
}

fn resolve(root: &Path) -> Result<ResolvedProgram, ModuleError> {
    resolve_program(root, &stub_parse)
}

/// The name every function item declares, in program order.
fn declared_names(program: &ResolvedProgram) -> Vec<String> {
    program
        .items
        .iter()
        .filter_map(|item| match item {
            Item::Function(def) => Some(def.name.name.clone()),
            _ => None,
        })
        .collect()
}

/// The callee of the first call anywhere in the resolved program.
fn first_callee(program: &ResolvedProgram) -> Option<String> {
    program
        .items
        .iter()
        .filter_map(|item| match item {
            Item::Function(def) => Some(&def.body),
            _ => None,
        })
        .flatten()
        .find_map(|stmt| match stmt {
            Stmt::Expr(Expr::Call { func, .. }) => match func.as_ref() {
                Expr::Identifier(name) => Some(name.name.clone()),
                Expr::Path {
                    type_name, member, ..
                } => Some(format!("{}::{}", type_name.name, member.name)),
                _ => None,
            },
            _ => None,
        })
}

/// The first match-arm pattern anywhere in the resolved program.
fn first_pattern(program: &ResolvedProgram) -> Option<Pattern> {
    program
        .items
        .iter()
        .filter_map(|item| match item {
            Item::Function(def) => Some(&def.body),
            _ => None,
        })
        .flatten()
        .find_map(|stmt| match stmt {
            Stmt::Expr(Expr::Match { arms, .. }) => arms.first()?.patterns.first().cloned(),
            _ => None,
        })
}

#[test]
fn loads_a_sibling_module_and_strips_the_qualifier() {
    let dir = TempDir::new().expect("temp dir");
    write(dir.path(), "math.nr", "func sqrt\n");
    let root = write(dir.path(), "main.nr", "call math::sqrt\n");

    let program = resolve(&root).expect("resolution succeeds");

    assert_eq!(program.modules.len(), 2);
    assert!(declared_names(&program).contains(&"sqrt".to_string()));
    assert_eq!(first_callee(&program).as_deref(), Some("sqrt"));
}

#[test]
fn descends_into_a_directory_module_through_its_mod_file() {
    let dir = TempDir::new().expect("temp dir");
    write(dir.path(), "utils/mod.nr", "func helper\n");
    write(dir.path(), "utils/io.nr", "func read\n");
    let root = write(dir.path(), "main.nr", "call utils::io::read\n");

    let program = resolve(&root).expect("resolution succeeds");

    assert_eq!(program.modules.len(), 3);
    assert_eq!(first_callee(&program).as_deref(), Some("read"));
    assert_eq!(program.modules[2].path, "utils::io");
}

#[test]
fn a_qualified_type_annotation_loses_its_module_prefix() {
    let dir = TempDir::new().expect("temp dir");
    write(dir.path(), "geometry.nr", "func Point\n");
    let root = write(dir.path(), "main.nr", "param take: geometry::Point\n");

    let program = resolve(&root).expect("resolution succeeds");

    let annotated = program.items.iter().find_map(|item| match item {
        Item::Function(def) if def.name.name == "take" => def.params.first(),
        _ => None,
    });
    match annotated.map(|param| &param.ty) {
        Some(Type::Named(name)) => assert_eq!(name.name, "Point"),
        other => panic!("expected a bare named type, got {:?}", other),
    }
}

#[test]
fn an_unqualified_associated_path_is_left_alone() {
    let dir = TempDir::new().expect("temp dir");
    let root = write(dir.path(), "main.nr", "func Point\ncall Point::new\n");

    let program = resolve(&root).expect("resolution succeeds");

    // No `Point.nr` exists, so `Point::new` is an ordinary associated-function path and
    // must survive untouched for the type checker.
    assert_eq!(first_callee(&program).as_deref(), Some("Point::new"));
}

#[test]
fn a_module_reference_never_pulls_in_unrelated_neighbours() {
    let dir = TempDir::new().expect("temp dir");
    write(dir.path(), "math.nr", "func sqrt\n");
    write(dir.path(), "unrelated.nr", "func sqrt\n");
    let root = write(dir.path(), "main.nr", "call math::sqrt\n");

    // `unrelated.nr` declares a colliding name; it is not loaded, so it cannot collide.
    let program = resolve(&root).expect("resolution succeeds");
    assert_eq!(program.modules.len(), 2);
}

#[test]
fn mutually_referencing_modules_terminate() {
    let dir = TempDir::new().expect("temp dir");
    write(dir.path(), "b.nr", "func twice\ncall a::base\n");
    let root = write(dir.path(), "a.nr", "func base\ncall b::twice\n");

    let program = resolve(&root).expect("resolution succeeds");
    assert_eq!(program.modules.len(), 2);
}

#[test]
fn an_item_the_module_does_not_declare_is_rejected() {
    let dir = TempDir::new().expect("temp dir");
    write(dir.path(), "math.nr", "func sqrt\n");
    let root = write(dir.path(), "main.nr", "call math::cbrt\n");

    match resolve(&root) {
        Err(ModuleError::UndeclaredItem { module, item, .. }) => {
            assert_eq!(module, "math");
            assert_eq!(item, "cbrt");
        }
        other => panic!("expected UndeclaredItem, got {:?}", other.map(|_| ())),
    }
}

#[test]
fn a_name_declared_by_two_modules_collides() {
    let dir = TempDir::new().expect("temp dir");
    write(dir.path(), "other.nr", "func shared\n");
    let root = write(dir.path(), "main.nr", "func shared\ncall other::shared\n");

    match resolve(&root) {
        Err(ModuleError::DuplicateItem { name, .. }) => assert_eq!(name, "shared"),
        other => panic!("expected DuplicateItem, got {:?}", other.map(|_| ())),
    }
}

#[test]
fn a_directory_without_a_mod_file_is_not_a_module() {
    let dir = TempDir::new().expect("temp dir");
    std::fs::create_dir_all(dir.path().join("utils")).expect("create directory");
    let root = write(dir.path(), "main.nr", "call utils::read\n");

    match resolve(&root) {
        Err(ModuleError::MissingModFile { name, .. }) => assert_eq!(name, "utils"),
        other => panic!("expected MissingModFile, got {:?}", other.map(|_| ())),
    }
}

#[test]
fn a_deep_path_with_no_module_head_is_rejected() {
    let dir = TempDir::new().expect("temp dir");
    let root = write(dir.path(), "main.nr", "call nope::inner::thing\n");

    match resolve(&root) {
        Err(ModuleError::UnknownModule { head, .. }) => assert_eq!(head, "nope"),
        other => panic!("expected UnknownModule, got {:?}", other.map(|_| ())),
    }
}

#[test]
fn a_leaf_module_has_no_children() {
    let dir = TempDir::new().expect("temp dir");
    write(dir.path(), "math.nr", "func sqrt\n");
    write(dir.path(), "io.nr", "func read\n");
    // `math` is a leaf file, so `math::io` must not reach `io.nr` beside it.
    let root = write(dir.path(), "main.nr", "call math::io::read\n");

    match resolve(&root) {
        Err(ModuleError::UndeclaredItem { module, item, .. }) => {
            assert_eq!(module, "math");
            assert_eq!(item, "io");
        }
        other => panic!("expected UndeclaredItem, got {:?}", other.map(|_| ())),
    }
}

#[test]
fn an_import_pulls_in_a_module_nothing_else_references() {
    let dir = TempDir::new().expect("temp dir");
    write(dir.path(), "math.nr", "func sqrt\n");
    let root = write(dir.path(), "main.nr", "import math\nfunc main\n");

    let program = resolve(&root).expect("resolution succeeds");

    // Discovery is reference-driven, so without the import nothing would reach `math.nr`.
    assert_eq!(program.modules.len(), 2);
    assert!(declared_names(&program).contains(&"sqrt".to_string()));
}

#[test]
fn an_imported_item_is_usable_unqualified() {
    let dir = TempDir::new().expect("temp dir");
    write(dir.path(), "math.nr", "func sqrt\n");
    let root = write(
        dir.path(),
        "main.nr",
        "import math::{sqrt}\nfunc main\nbare sqrt\n",
    );

    let program = resolve(&root).expect("resolution succeeds");

    assert_eq!(first_callee(&program).as_deref(), Some("sqrt"));
}

#[test]
fn an_aliased_item_resolves_to_its_original_name() {
    let dir = TempDir::new().expect("temp dir");
    write(dir.path(), "math.nr", "func sqrt\n");
    let root = write(
        dir.path(),
        "main.nr",
        "import math::sqrt as root\nfunc main\nbare root\n",
    );

    let program = resolve(&root).expect("resolution succeeds");

    assert_eq!(first_callee(&program).as_deref(), Some("sqrt"));
}

#[test]
fn an_aliased_module_qualifies_a_path() {
    let dir = TempDir::new().expect("temp dir");
    write(dir.path(), "math/mod.nr", "func dummy\n");
    write(dir.path(), "math/matrix.nr", "func mul\n");
    let root = write(
        dir.path(),
        "main.nr",
        "import math::matrix as mat\nfunc main\ncall mat::mul\n",
    );

    let program = resolve(&root).expect("resolution succeeds");

    assert_eq!(first_callee(&program).as_deref(), Some("mul"));
}

#[test]
fn a_relative_import_reaches_a_child_module_in_its_list() {
    let dir = TempDir::new().expect("temp dir");
    write(dir.path(), "utils/mod.nr", "func helper\n");
    write(dir.path(), "utils/io.nr", "func read\n");
    let root = write(
        dir.path(),
        "main.nr",
        "import ./utils::{io}\nfunc main\ncall io::read\n",
    );

    let program = resolve(&root).expect("resolution succeeds");

    // `io` names a child module rather than an item of `utils`, so the list entry binds a
    // module and the qualified call resolves through it.
    assert_eq!(program.modules.len(), 3);
    assert_eq!(first_callee(&program).as_deref(), Some("read"));
}

#[test]
fn an_imported_variant_resolves_in_expression_position() {
    let dir = TempDir::new().expect("temp dir");
    let root = write(
        dir.path(),
        "main.nr",
        "import Option::{Some, None}\nfunc main\nbare None\n",
    );

    // `Option` names no module — it is an enum the prelude supplies, invisible to this
    // pass — so the variant is qualified for the type checker rather than resolved here.
    let program = resolve(&root).expect("resolution succeeds");
    assert_eq!(first_callee(&program).as_deref(), Some("Option::None"));
}

#[test]
fn an_imported_variant_resolves_in_pattern_position() {
    let dir = TempDir::new().expect("temp dir");
    let root = write(
        dir.path(),
        "main.nr",
        "import Option::{Some}\nfunc main\npat Some(v)\n",
    );

    let program = resolve(&root).expect("resolution succeeds");

    match first_pattern(&program) {
        Some(Pattern::Enum {
            enum_name, variant, ..
        }) => {
            assert_eq!(enum_name.name, "Option");
            assert_eq!(variant.name, "Some");
        }
        other => panic!("expected a resolved enum pattern, got {:?}", other),
    }
}

#[test]
fn a_payloadless_imported_variant_stops_reading_as_a_binding() {
    let dir = TempDir::new().expect("temp dir");
    let root = write(
        dir.path(),
        "main.nr",
        "import Option::{None}\nfunc main\npatbare None\n",
    );

    let program = resolve(&root).expect("resolution succeeds");

    match first_pattern(&program) {
        Some(Pattern::Enum { variant, .. }) => assert_eq!(variant.name, "None"),
        other => panic!("expected a resolved enum pattern, got {:?}", other),
    }
}

#[test]
fn a_variant_pattern_no_import_accounts_for_is_rejected() {
    let dir = TempDir::new().expect("temp dir");
    let root = write(dir.path(), "main.nr", "func main\npat Some(v)\n");

    match resolve(&root) {
        Err(ModuleError::UnimportedVariant { variant, .. }) => assert_eq!(variant, "Some"),
        other => panic!("expected UnimportedVariant, got {:?}", other.map(|_| ())),
    }
}

#[test]
fn an_import_of_a_name_the_module_does_not_declare_is_rejected() {
    let dir = TempDir::new().expect("temp dir");
    write(dir.path(), "math.nr", "func sqrt\n");
    let root = write(dir.path(), "main.nr", "import math::{cbrt}\nfunc main\n");

    match resolve(&root) {
        Err(ModuleError::UndeclaredItem { module, item, .. }) => {
            assert_eq!(module, "math");
            assert_eq!(item, "cbrt");
        }
        other => panic!("expected UndeclaredItem, got {:?}", other.map(|_| ())),
    }
}

#[test]
fn an_import_naming_no_module_is_rejected() {
    let dir = TempDir::new().expect("temp dir");
    let root = write(dir.path(), "main.nr", "import nosuch\nfunc main\n");

    match resolve(&root) {
        Err(ModuleError::UnknownModule { head, .. }) => assert_eq!(head, "nosuch"),
        other => panic!("expected UnknownModule, got {:?}", other.map(|_| ())),
    }
}

#[test]
fn one_name_may_not_be_imported_twice() {
    let dir = TempDir::new().expect("temp dir");
    let root = write(
        dir.path(),
        "main.nr",
        "import Option::{Some}\nimport Maybe::{Some}\nfunc main\n",
    );

    // Two imports binding `Some` would leave the name meaning whichever came last.
    match resolve(&root) {
        Err(ModuleError::DuplicateImport { name, .. }) => assert_eq!(name, "Some"),
        other => panic!("expected DuplicateImport, got {:?}", other.map(|_| ())),
    }
}
