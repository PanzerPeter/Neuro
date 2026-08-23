//! Unit tests for discovery, collision detection, and qualifier stripping.
//!
//! The AST fixtures are built by hand rather than parsed: this slice must compile without
//! `syntax-parsing`, so the parser it is handed here is a stub that maps a marker line to
//! the item or reference it stands for.

use std::path::{Path, PathBuf};

use ast_types::{
    EnumPatternPayload, Expr, FunctionDef, ImportDef, ImportName, ImportSelection, Item, MatchArm,
    ModuleDef, Parameter, Pattern, Stmt, Type,
};
use shared_types::{Identifier, Literal, Span};
use tempfile::TempDir;

use crate::{resolve_program, ModuleError, PreludeVariant, ResolvedProgram};

fn ident(name: &str) -> Identifier {
    Identifier {
        name: name.to_string(),
        span: Span::new(0, 0),
    }
}

fn function(name: &str, exported: bool, body: Vec<Stmt>) -> Item {
    Item::Function(FunctionDef {
        name: ident(name),
        exported,
        // Resolution stamps the real module; the fixture's value is what it overwrites.
        module: 0,
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
    let Item::Function(mut def) = function(name, false, Vec::new()) else {
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
fn import_item(rest: &str, exported: bool) -> Result<Item, String> {
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
        exported,
        span: Span::new(0, 0),
    }))
}

/// The stub parser: each line is `func NAME`, `export func NAME`,
/// `call QUALIFIER::MEMBER`, `bare NAME`, `param NAME: TYPE`, `import ...`,
/// `export import ...`, `no_prelude`, `pat NAME(BIND)`, or `patbare NAME` — enough surface to drive
/// every resolution rule. A `module NAME` line opens an inline block that runs to a
/// matching `end`. `func` without `export` is private to its file, exactly as in real
/// source.
fn stub_parse(source: &str) -> Result<Vec<Item>, String> {
    let mut items = Vec::new();
    let mut calls = Vec::new();
    let mut lines = source.lines().map(str::trim).filter(|l| !l.is_empty());
    while let Some(line) = lines.next() {
        if let Some(name) = line.strip_prefix("module ") {
            items.push(Item::Module(ModuleDef {
                name: ident(name),
                items: stub_parse(&stub_block(&mut lines, name)?)?,
                span: Span::new(0, 0),
            }));
        } else if let Some(name) = line.strip_prefix("export func ") {
            items.push(function(name, true, Vec::new()));
        } else if let Some(name) = line.strip_prefix("func ") {
            items.push(function(name, false, Vec::new()));
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
        } else if let Some(rest) = line.strip_prefix("export import ") {
            items.push(import_item(rest, true)?);
        } else if let Some(rest) = line.strip_prefix("import ") {
            items.push(import_item(rest, false)?);
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
        } else if line == "no_prelude" {
            items.push(Item::NoPrelude(Span::new(0, 0)));
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
            None => items.push(function("entry", false, calls)),
        }
    }
    Ok(items)
}

/// Collect the body of a `module NAME` block, up to its matching `end`.
fn stub_block<'a>(lines: &mut impl Iterator<Item = &'a str>, name: &str) -> Result<String, String> {
    let mut depth = 1;
    let mut body = String::new();
    for line in lines {
        if line.starts_with("module ") {
            depth += 1;
        } else if line == "end" {
            depth -= 1;
            if depth == 0 {
                return Ok(body);
            }
        }
        body.push_str(line);
        body.push('\n');
    }
    Err(format!("unterminated module block: {}", name))
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
    resolve_program(root, &stub_parse, &[])
}

/// Resolve with the prelude the driver really supplies, so a fixture can be written the
/// way source is: `Some` and `Ok` bare, with no import above them.
fn resolve_with_prelude(root: &Path) -> Result<ResolvedProgram, ModuleError> {
    let prelude: Vec<PreludeVariant> = [
        ("Option", "Some"),
        ("Option", "None"),
        ("Result", "Ok"),
        ("Result", "Err"),
    ]
    .into_iter()
    .map(|(owner, variant)| PreludeVariant {
        owner: owner.to_string(),
        variant: variant.to_string(),
    })
    .collect();
    resolve_program(root, &stub_parse, &prelude)
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
    write(dir.path(), "math.nr", "export func sqrt\n");
    let root = write(dir.path(), "main.nr", "call math::sqrt\n");

    let program = resolve(&root).expect("resolution succeeds");

    assert_eq!(program.modules.len(), 2);
    assert!(declared_names(&program).contains(&"sqrt".to_string()));
    assert_eq!(first_callee(&program).as_deref(), Some("sqrt"));
}

#[test]
fn descends_into_a_directory_module_through_its_mod_file() {
    let dir = TempDir::new().expect("temp dir");
    write(dir.path(), "utils/mod.nr", "export func helper\n");
    write(dir.path(), "utils/io.nr", "export func read\n");
    let root = write(dir.path(), "main.nr", "call utils::io::read\n");

    let program = resolve(&root).expect("resolution succeeds");

    assert_eq!(program.modules.len(), 3);
    assert_eq!(first_callee(&program).as_deref(), Some("read"));
    assert_eq!(program.modules[2].path, "utils::io");
}

#[test]
fn a_qualified_type_annotation_loses_its_module_prefix() {
    let dir = TempDir::new().expect("temp dir");
    write(dir.path(), "geometry.nr", "export func Point\n");
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
    let root = write(
        dir.path(),
        "main.nr",
        "export func Point\ncall Point::new\n",
    );

    let program = resolve(&root).expect("resolution succeeds");

    // No `Point.nr` exists, so `Point::new` is an ordinary associated-function path and
    // must survive untouched for the type checker.
    assert_eq!(first_callee(&program).as_deref(), Some("Point::new"));
}

#[test]
fn a_module_reference_never_pulls_in_unrelated_neighbours() {
    let dir = TempDir::new().expect("temp dir");
    write(dir.path(), "math.nr", "export func sqrt\n");
    write(dir.path(), "unrelated.nr", "export func sqrt\n");
    let root = write(dir.path(), "main.nr", "call math::sqrt\n");

    // `unrelated.nr` declares a colliding name; it is not loaded, so it cannot collide.
    let program = resolve(&root).expect("resolution succeeds");
    assert_eq!(program.modules.len(), 2);
}

#[test]
fn mutually_referencing_modules_terminate() {
    let dir = TempDir::new().expect("temp dir");
    write(dir.path(), "b.nr", "export func twice\ncall a::base\n");
    let root = write(dir.path(), "a.nr", "export func base\ncall b::twice\n");

    let program = resolve(&root).expect("resolution succeeds");
    assert_eq!(program.modules.len(), 2);
}

#[test]
fn an_item_the_module_does_not_declare_is_rejected() {
    let dir = TempDir::new().expect("temp dir");
    write(dir.path(), "math.nr", "export func sqrt\n");
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
    write(dir.path(), "other.nr", "export func shared\n");
    let root = write(
        dir.path(),
        "main.nr",
        "export func shared\ncall other::shared\n",
    );

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
    write(dir.path(), "math.nr", "export func sqrt\n");
    write(dir.path(), "io.nr", "export func read\n");
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
    write(dir.path(), "math.nr", "export func sqrt\n");
    let root = write(dir.path(), "main.nr", "import math\nexport func main\n");

    let program = resolve(&root).expect("resolution succeeds");

    // Discovery is reference-driven, so without the import nothing would reach `math.nr`.
    assert_eq!(program.modules.len(), 2);
    assert!(declared_names(&program).contains(&"sqrt".to_string()));
}

#[test]
fn an_imported_item_is_usable_unqualified() {
    let dir = TempDir::new().expect("temp dir");
    write(dir.path(), "math.nr", "export func sqrt\n");
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
    write(dir.path(), "math.nr", "export func sqrt\n");
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
    write(dir.path(), "math/mod.nr", "export func dummy\n");
    write(dir.path(), "math/matrix.nr", "export func mul\n");
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
    write(dir.path(), "utils/mod.nr", "export func helper\n");
    write(dir.path(), "utils/io.nr", "export func read\n");
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
    let root = write(dir.path(), "main.nr", "export func main\npat Some(v)\n");

    match resolve(&root) {
        Err(ModuleError::UnimportedVariant { variant, .. }) => assert_eq!(variant, "Some"),
        other => panic!("expected UnimportedVariant, got {:?}", other.map(|_| ())),
    }
}

#[test]
fn an_import_of_a_name_the_module_does_not_declare_is_rejected() {
    let dir = TempDir::new().expect("temp dir");
    write(dir.path(), "math.nr", "export func sqrt\n");
    let root = write(
        dir.path(),
        "main.nr",
        "import math::{cbrt}\nexport func main\n",
    );

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
    let root = write(dir.path(), "main.nr", "import nosuch\nexport func main\n");

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

#[test]
fn a_private_item_cannot_be_reached_by_a_qualified_path() {
    let dir = TempDir::new().expect("temp dir");
    write(dir.path(), "math.nr", "func sqrt\n");
    let root = write(dir.path(), "main.nr", "call math::sqrt\n");

    match resolve(&root) {
        Err(ModuleError::PrivateItem { module, item, .. }) => {
            assert_eq!(module, "math");
            assert_eq!(item, "sqrt");
        }
        other => panic!("expected PrivateItem, got {:?}", other.map(|_| ())),
    }
}

#[test]
fn a_private_item_cannot_be_imported() {
    let dir = TempDir::new().expect("temp dir");
    write(dir.path(), "math.nr", "func sqrt\n");
    let root = write(dir.path(), "main.nr", "import math::{sqrt}\n");

    match resolve(&root) {
        Err(ModuleError::PrivateItem { item, .. }) => assert_eq!(item, "sqrt"),
        other => panic!("expected PrivateItem, got {:?}", other.map(|_| ())),
    }
}

#[test]
fn a_private_item_cannot_be_imported_under_an_alias() {
    let dir = TempDir::new().expect("temp dir");
    write(dir.path(), "math.nr", "func sqrt\n");
    let root = write(dir.path(), "main.nr", "import math::sqrt as root\n");

    match resolve(&root) {
        Err(ModuleError::PrivateItem { item, .. }) => assert_eq!(item, "sqrt"),
        other => panic!("expected PrivateItem, got {:?}", other.map(|_| ())),
    }
}

#[test]
fn a_private_item_is_still_usable_inside_its_own_module() {
    let dir = TempDir::new().expect("temp dir");
    // `helper` is private, and `math` names itself in the qualifier — a module is never
    // closed to itself.
    write(
        dir.path(),
        "math.nr",
        "func helper\nexport func sqrt\ncall math::helper\n",
    );
    let root = write(dir.path(), "main.nr", "call math::sqrt\n");

    let program = resolve(&root).expect("resolution succeeds");
    assert!(declared_names(&program).contains(&"helper".to_string()));
}

#[test]
fn every_item_carries_the_module_it_was_loaded_from() {
    let dir = TempDir::new().expect("temp dir");
    write(dir.path(), "math.nr", "export func sqrt\n");
    let root = write(dir.path(), "main.nr", "call math::sqrt\n");

    let program = resolve(&root).expect("resolution succeeds");

    let module_of = |name: &str| {
        program.items.iter().find_map(|item| match item {
            Item::Function(def) if def.name.name == name => Some(def.module),
            _ => None,
        })
    };
    // The root is module 0 and every other file gets its own id, which is what the type
    // checker measures a private field against.
    assert_eq!(module_of("entry"), Some(0));
    assert_eq!(module_of("sqrt"), Some(1));
}

#[test]
fn an_inline_block_is_reached_like_any_other_module() {
    let dir = TempDir::new().expect("temp dir");
    let root = write(
        dir.path(),
        "main.nr",
        "module geometry\nexport func area\nend\ncall geometry::area\n",
    );

    let program = resolve(&root).expect("resolution succeeds");

    // The block's items join the same flat namespace, so the qualifier is gone.
    assert_eq!(first_callee(&program).as_deref(), Some("area"));
    assert!(declared_names(&program).contains(&"area".to_string()));
}

#[test]
fn an_inline_block_item_is_private_to_the_block() {
    let dir = TempDir::new().expect("temp dir");
    // The file declaring the block is still outside it: `export` is the only way in.
    let root = write(
        dir.path(),
        "main.nr",
        "module geometry\nfunc scale\nend\ncall geometry::scale\n",
    );

    match resolve(&root) {
        Err(ModuleError::PrivateItem { item, .. }) => assert_eq!(item, "scale"),
        other => panic!("expected PrivateItem, got {:?}", other.map(|_| ())),
    }
}

#[test]
fn an_inline_block_wins_over_a_same_named_file() {
    let dir = TempDir::new().expect("temp dir");
    write(dir.path(), "geometry.nr", "export func area\n");
    let root = write(
        dir.path(),
        "main.nr",
        "module geometry\nexport func inline_area\nend\ncall geometry::inline_area\n",
    );

    let program = resolve(&root).expect("resolution succeeds");

    assert_eq!(first_callee(&program).as_deref(), Some("inline_area"));
    // The sibling file was never a candidate, so it was never loaded.
    assert!(!declared_names(&program).contains(&"area".to_string()));
}

#[test]
fn inline_blocks_nest() {
    let dir = TempDir::new().expect("temp dir");
    let root = write(
        dir.path(),
        "main.nr",
        "module outer\nmodule inner\nexport func deep\nend\nend\ncall outer::inner::deep\n",
    );

    let program = resolve(&root).expect("resolution succeeds");
    assert_eq!(first_callee(&program).as_deref(), Some("deep"));
}

#[test]
fn two_inline_blocks_may_not_share_a_name() {
    let dir = TempDir::new().expect("temp dir");
    let root = write(
        dir.path(),
        "main.nr",
        "module m\nexport func first\nend\nmodule m\nexport func second\nend\n",
    );

    match resolve(&root) {
        Err(ModuleError::DuplicateInlineModule { name, .. }) => assert_eq!(name, "m"),
        other => panic!(
            "expected DuplicateInlineModule, got {:?}",
            other.map(|_| ())
        ),
    }
}

#[test]
fn export_import_makes_a_name_reachable_through_the_importer() {
    let dir = TempDir::new().expect("temp dir");
    write(dir.path(), "internal.nr", "export func parse\n");
    write(
        dir.path(),
        "facade.nr",
        "export import ./internal::{parse}\n",
    );
    let root = write(dir.path(), "main.nr", "call facade::parse\n");

    let program = resolve(&root).expect("resolution succeeds");
    assert_eq!(first_callee(&program).as_deref(), Some("parse"));
}

#[test]
fn a_plain_import_does_not_re_export() {
    let dir = TempDir::new().expect("temp dir");
    write(dir.path(), "internal.nr", "export func parse\n");
    write(dir.path(), "facade.nr", "import ./internal::{parse}\n");
    let root = write(dir.path(), "main.nr", "call facade::parse\n");

    match resolve(&root) {
        Err(ModuleError::UndeclaredItem { item, .. }) => assert_eq!(item, "parse"),
        other => panic!("expected UndeclaredItem, got {:?}", other.map(|_| ())),
    }
}

#[test]
fn export_import_carries_the_rename_back_to_the_declaration() {
    let dir = TempDir::new().expect("temp dir");
    write(dir.path(), "internal.nr", "export func parse_config\n");
    write(
        dir.path(),
        "facade.nr",
        "export import ./internal::{parse_config as build}\n",
    );
    let root = write(dir.path(), "main.nr", "call facade::build\n");

    let program = resolve(&root).expect("resolution succeeds");
    // The flat namespace holds the declaration's own name; only the route was renamed.
    assert_eq!(first_callee(&program).as_deref(), Some("parse_config"));
}

#[test]
fn a_chain_of_re_exports_resolves_to_the_declaration() {
    let dir = TempDir::new().expect("temp dir");
    write(dir.path(), "deep.nr", "export func value\n");
    write(dir.path(), "mid.nr", "export import ./deep::{value}\n");
    write(dir.path(), "top.nr", "export import ./mid::{value as v}\n");
    let root = write(dir.path(), "main.nr", "call top::v\n");

    let program = resolve(&root).expect("resolution succeeds");
    assert_eq!(first_callee(&program).as_deref(), Some("value"));
}

#[test]
fn export_import_of_a_module_is_rejected() {
    let dir = TempDir::new().expect("temp dir");
    write(dir.path(), "internal.nr", "export func parse\n");
    let root = write(
        dir.path(),
        "main.nr",
        "export import ./internal\ncall internal::parse\n",
    );

    match resolve(&root) {
        Err(ModuleError::ExportImportNotItem { name, what, .. }) => {
            assert_eq!(name, "internal");
            assert_eq!(what, "a module");
        }
        other => panic!("expected ExportImportNotItem, got {:?}", other.map(|_| ())),
    }
}

#[test]
fn a_re_export_may_not_open_a_private_declaration() {
    let dir = TempDir::new().expect("temp dir");
    write(dir.path(), "internal.nr", "func parse\n");
    write(
        dir.path(),
        "facade.nr",
        "export import ./internal::{parse}\n",
    );
    let root = write(dir.path(), "main.nr", "call facade::parse\n");

    match resolve(&root) {
        Err(ModuleError::PrivateItem { item, .. }) => assert_eq!(item, "parse"),
        other => panic!("expected PrivateItem, got {:?}", other.map(|_| ())),
    }
}

#[test]
fn the_prelude_binds_a_variant_no_import_mentions() {
    let dir = TempDir::new().expect("temp dir");
    let root = write(dir.path(), "main.nr", "export func main\npat Some(v)\n");

    let program = resolve_with_prelude(&root).expect("resolution succeeds");

    assert!(!program.no_prelude);
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
fn a_prelude_binding_reaches_a_non_root_module() {
    let dir = TempDir::new().expect("temp dir");
    write(dir.path(), "parse.nr", "export func run\npat Ok(v)\n");
    let root = write(dir.path(), "main.nr", "call parse::run\n");

    let program = resolve_with_prelude(&root).expect("resolution succeeds");

    match first_pattern(&program) {
        Some(Pattern::Enum {
            enum_name, variant, ..
        }) => {
            assert_eq!(enum_name.name, "Result");
            assert_eq!(variant.name, "Ok");
        }
        other => panic!("expected a resolved enum pattern, got {:?}", other),
    }
}

#[test]
fn an_explicit_import_of_a_prelude_name_wins_over_it() {
    let dir = TempDir::new().expect("temp dir");
    let root = write(
        dir.path(),
        "main.nr",
        "import Reading::{Some}\nexport func main\npat Some(v)\n",
    );

    let program = resolve_with_prelude(&root).expect("resolution succeeds");

    match first_pattern(&program) {
        Some(Pattern::Enum { enum_name, .. }) => assert_eq!(enum_name.name, "Reading"),
        other => panic!("expected a resolved enum pattern, got {:?}", other),
    }
}

#[test]
fn a_local_declaration_shadows_a_prelude_binding() {
    let dir = TempDir::new().expect("temp dir");
    let root = write(dir.path(), "main.nr", "export func None\npatbare None\n");

    let program = resolve_with_prelude(&root).expect("resolution succeeds");

    match first_pattern(&program) {
        Some(Pattern::Binding(name)) => assert_eq!(name.name, "None"),
        other => panic!("expected the name to stay a binding, got {:?}", other),
    }
}

#[test]
fn no_prelude_leaves_a_bare_variant_unbound() {
    let dir = TempDir::new().expect("temp dir");
    let root = write(
        dir.path(),
        "main.nr",
        "no_prelude\nexport func main\npat Some(v)\n",
    );

    match resolve_with_prelude(&root) {
        Err(ModuleError::UnimportedVariant { variant, .. }) => assert_eq!(variant, "Some"),
        other => panic!("expected UnimportedVariant, got {:?}", other.map(|_| ())),
    }
}

#[test]
fn no_prelude_on_the_root_is_reported_to_the_driver() {
    let dir = TempDir::new().expect("temp dir");
    let root = write(dir.path(), "main.nr", "no_prelude\nexport func main\n");

    let program = resolve_with_prelude(&root).expect("resolution succeeds");

    assert!(program.no_prelude);
}

#[test]
fn an_inline_block_inherits_its_files_opt_out() {
    let dir = TempDir::new().expect("temp dir");
    let root = write(
        dir.path(),
        "main.nr",
        "no_prelude\nmodule inner\nexport func run\npat Some(v)\nend\nexport func main\n",
    );

    match resolve_with_prelude(&root) {
        Err(ModuleError::UnimportedVariant { variant, .. }) => assert_eq!(variant, "Some"),
        other => panic!("expected UnimportedVariant, got {:?}", other.map(|_| ())),
    }
}

#[test]
fn a_module_that_kept_the_prelude_still_gets_it_beside_one_that_did_not() {
    let dir = TempDir::new().expect("temp dir");
    write(dir.path(), "strict.nr", "no_prelude\nexport func run\n");
    let root = write(
        dir.path(),
        "main.nr",
        "call strict::run\nexport func main\npat Err(e)\n",
    );

    let program = resolve_with_prelude(&root).expect("resolution succeeds");

    assert!(!program.no_prelude);
    match first_pattern(&program) {
        Some(Pattern::Enum { enum_name, .. }) => assert_eq!(enum_name.name, "Result"),
        other => panic!("expected a resolved enum pattern, got {:?}", other),
    }
}
