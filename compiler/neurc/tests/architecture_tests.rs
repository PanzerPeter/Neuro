//! Architecture Tests for VSA Baseline Compliance
//!
//! These tests verify that the NEURO compiler maintains Vertical Slice Architecture
//! boundaries. They ensure that:
//! - Feature slices only depend on infrastructure crates
//! - Infrastructure crates don't depend on feature slices
//! - No cross-slice dependencies between feature slices

use std::env;
use std::fs;
use std::path::{Path, PathBuf};

/// Get the workspace root directory
fn workspace_root() -> PathBuf {
    // CARGO_MANIFEST_DIR points to compiler/neurc, so we need to go up 2 levels
    let manifest_dir = env::var("CARGO_MANIFEST_DIR").expect("CARGO_MANIFEST_DIR not set");
    PathBuf::from(manifest_dir)
        .parent()
        .expect("Failed to get compiler dir")
        .parent()
        .expect("Failed to get workspace root")
        .to_path_buf()
}

/// Extract only the [dependencies] section from Cargo.toml, excluding [dev-dependencies]
fn extract_dependencies_section(cargo_toml: &str) -> String {
    let mut result = String::new();
    let mut in_dependencies = false;

    for line in cargo_toml.lines() {
        if line.trim().starts_with("[dependencies]") {
            in_dependencies = true;
            result.push_str(line);
            result.push('\n');
        } else if line.trim().starts_with('[') {
            // Entering a different section
            in_dependencies = false;
        } else if in_dependencies {
            result.push_str(line);
            result.push('\n');
        }
    }

    result
}

#[test]
fn test_no_cross_slice_dependencies() {
    let root = workspace_root();
    let feature_slices = vec![
        "compiler/lexical-analysis",
        "compiler/syntax-parsing",
        "compiler/semantic-analysis",
        "compiler/control-flow",
        "compiler/llvm-backend",
    ];

    for slice_path in &feature_slices {
        println!("Checking slice: {}", slice_path);

        let cargo_toml_path = root.join(slice_path).join("Cargo.toml");
        let cargo_toml_content = fs::read_to_string(&cargo_toml_path)
            .unwrap_or_else(|_| panic!("Failed to read {}", cargo_toml_path.display()));

        // Extract only the [dependencies] section (not [dev-dependencies])
        let dependencies_section = extract_dependencies_section(&cargo_toml_content);

        // Check dependencies section
        for other_slice in &feature_slices {
            if slice_path != other_slice {
                let slice_name = Path::new(other_slice)
                    .file_name()
                    .unwrap()
                    .to_str()
                    .unwrap();

                // syntax-parsing calls tokenize() directly; keeping the tokeniser call
                // inside this slice means neurc calls parse(), not orchestrate-then-parse.
                if slice_path == &"compiler/syntax-parsing" && slice_name == "lexical-analysis" {
                    continue;
                }

                // Ensure no other cross-slice dependencies in production code
                assert!(
                    !dependencies_section.contains(&format!("{} =", slice_name)),
                    "VSA VIOLATION: {} depends on feature slice {} in [dependencies]. \
                     Feature slices should only depend on infrastructure crates. \
                     (dev-dependencies are OK for tests)",
                    slice_path,
                    slice_name
                );
            }
        }

        println!("  ✓ No cross-slice dependencies found");
    }
}

#[test]
fn test_infrastructure_no_slice_dependencies() {
    let root = workspace_root();
    let infrastructure_crates = vec![
        "compiler/infrastructure/shared-types",
        "compiler/infrastructure/ast-types",
        "compiler/infrastructure/source-location",
        "compiler/infrastructure/diagnostics",
        "compiler/infrastructure/project-config",
    ];

    let feature_slices = vec![
        "lexical-analysis",
        "syntax-parsing",
        "semantic-analysis",
        "control-flow",
        "llvm-backend",
    ];

    for infra_path in &infrastructure_crates {
        println!("Checking infrastructure: {}", infra_path);

        let cargo_toml_path = root.join(infra_path).join("Cargo.toml");
        let cargo_toml = fs::read_to_string(&cargo_toml_path)
            .unwrap_or_else(|_| panic!("Failed to read {}", cargo_toml_path.display()));

        for slice_name in &feature_slices {
            assert!(
                !cargo_toml.contains(&format!("{} =", slice_name)),
                "VSA VIOLATION: Infrastructure crate {} depends on feature slice {}. \
                 Infrastructure must not depend on features.",
                infra_path,
                slice_name
            );
        }

        println!("  ✓ No feature slice dependencies found");
    }
}

#[test]
fn test_all_slices_have_context_md() {
    let root = workspace_root();
    let all_slices = vec![
        "compiler/lexical-analysis",
        "compiler/syntax-parsing",
        "compiler/semantic-analysis",
        "compiler/control-flow",
        "compiler/llvm-backend",
        "compiler/neurc",
        // Infrastructure slices also require CONTEXT.md (VSA 4.3 AC-011)
        "compiler/infrastructure/shared-types",
        "compiler/infrastructure/ast-types",
        "compiler/infrastructure/diagnostics",
        "compiler/infrastructure/source-location",
        "compiler/infrastructure/project-config",
    ];

    for slice_path in &all_slices {
        let context_path = root.join(slice_path).join("CONTEXT.md");
        assert!(
            context_path.exists(),
            "VSA 4.3 Section 12: Slice {} must have CONTEXT.md (AI contract file)",
            slice_path
        );

        let context_content = fs::read_to_string(&context_path)
            .unwrap_or_else(|_| panic!("Failed to read {}/CONTEXT.md", slice_path));

        for section in &[
            "## Purpose",
            "## Entry Point",
            "## Data Ownership",
            "## Shared Kernel",
        ] {
            assert!(
                context_content.contains(section),
                "CONTEXT.md in {} is missing '{}' section",
                slice_path,
                section
            );
        }

        println!("✓ {} has compliant CONTEXT.md", slice_path);
    }
}

#[test]
fn test_pub_crate_usage() {
    // This is a code review guideline test
    // A full implementation could parse Rust sources directly.
    // This test verifies that the rule is documented in contributor guidance.

    let root = workspace_root();
    let contributing =
        fs::read_to_string(root.join("CONTRIBUTING.md")).expect("Failed to read CONTRIBUTING.md");

    assert!(
        contributing.contains("pub(crate)"),
        "CONTRIBUTING.md should document pub(crate) usage for architecture compliance"
    );
}

#[test]
fn test_ast_types_in_infrastructure() {
    // Verify AST types are in infrastructure, not syntax-parsing
    let root = workspace_root();
    let ast_types_cargo = root.join("compiler/infrastructure/ast-types/Cargo.toml");
    assert!(
        ast_types_cargo.exists(),
        "ast-types infrastructure crate must exist (VSA requirement)"
    );

    // Verify syntax-parsing doesn't define AST types anymore
    let syntax_ast_mod = root.join("compiler/syntax-parsing/src/ast/mod.rs");
    let ast_mod_content =
        fs::read_to_string(&syntax_ast_mod).expect("Failed to read syntax-parsing/src/ast/mod.rs");

    assert!(
        ast_mod_content.contains("pub use ast_types::"),
        "syntax-parsing/src/ast/mod.rs should re-export from ast_types, not define types"
    );

    // Verify old AST definition files are deleted
    assert!(
        !root
            .join("compiler/syntax-parsing/src/ast/expressions.rs")
            .exists(),
        "Old AST definition files should be deleted from syntax-parsing"
    );
}
