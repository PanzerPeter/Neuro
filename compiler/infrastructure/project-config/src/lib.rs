//! NEURO Programming Language - Project Configuration
//!
//! Infrastructure component for parsing and managing project configuration from `neuro.toml` files.
//!
//! # Overview
//!
//! This crate provides:
//! - Project configuration data structures
//! - TOML parsing for `neuro.toml` files
//! - Package metadata (name, version, authors, license)
//! - Build configuration (optimization levels, target platforms)
//! - Dependency management structures (future: full dependency resolution)
//!
//! # Architecture
//!
//! Pure infrastructure using serde for serialization/deserialization.
//! No business logic - just data structures and parsing.
//!
//! # Examples
//!
//! ```
//! use project_config::ProjectConfig;
//! use std::path::PathBuf;
//!
//! # fn example() -> Result<(), Box<dyn std::error::Error>> {
//! // Load configuration from a neuro.toml file
//! let config = ProjectConfig::load(PathBuf::from("neuro.toml"))?;
//!
//! println!("Package: {} v{}", config.package.name, config.package.version);
//! println!("Optimization: {:?}", config.build.optimization_level);
//! # Ok(())
//! # }
//! ```

use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use thiserror::Error;

/// Project configuration from neuro.toml
///
/// This represents the complete configuration for a NEURO project.
/// In Phase 1, only package metadata and basic build settings are used.
/// Dependencies will be fully utilized in Phase 2 when the module system is implemented.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProjectConfig {
    /// Package metadata (name, version, authors, license)
    pub package: PackageConfig,
    /// Project dependencies (Phase 2+)
    #[serde(default)]
    pub dependencies: Vec<Dependency>,
    /// Build configuration (optimization, target platform)
    #[serde(default)]
    pub build: BuildConfig,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PackageConfig {
    pub name: String,
    pub version: String,
    #[serde(default)]
    pub authors: Vec<String>,
    #[serde(default)]
    pub license: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Dependency {
    pub name: String,
    pub version: String,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct BuildConfig {
    #[serde(default)]
    pub optimization_level: OptimizationLevel,
    #[serde(default)]
    pub target: Option<String>,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub enum OptimizationLevel {
    O0,
    O1,
    O2,
    O3,
}

impl Default for OptimizationLevel {
    fn default() -> Self {
        Self::O0
    }
}

#[derive(Debug, Error)]
pub enum ConfigError {
    #[error("failed to read config file: {0}")]
    IoError(#[from] std::io::Error),

    #[error("failed to parse config: {0}")]
    ParseError(#[from] toml::de::Error),

    #[error("config file not found")]
    NotFound,
}

impl ProjectConfig {
    pub fn load(path: PathBuf) -> Result<Self, ConfigError> {
        let content = std::fs::read_to_string(path)?;
        let config: ProjectConfig = toml::from_str(&content)?;
        Ok(config)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_basic_config() {
        let toml = r#"
            [package]
            name = "test-project"
            version = "0.1.0"
        "#;

        let config: ProjectConfig = toml::from_str(toml).unwrap();
        assert_eq!(config.package.name, "test-project");
        assert_eq!(config.package.version, "0.1.0");
    }

    #[test]
    fn parse_config_with_defaults() {
        let toml = r#"
            [package]
            name = "minimal"
            version = "1.0.0"
        "#;

        let config: ProjectConfig = toml::from_str(toml).unwrap();
        assert_eq!(config.dependencies.len(), 0);
        assert_eq!(config.package.authors.len(), 0);
        assert_eq!(config.package.license, None);
    }

    #[test]
    fn parse_config_with_authors() {
        let toml = r#"
            [package]
            name = "with-authors"
            version = "1.0.0"
            authors = ["Alice <alice@example.com>", "Bob"]
        "#;

        let config: ProjectConfig = toml::from_str(toml).unwrap();
        assert_eq!(config.package.authors.len(), 2);
        assert_eq!(config.package.authors[0], "Alice <alice@example.com>");
    }

    #[test]
    fn parse_config_with_license() {
        let toml = r#"
            [package]
            name = "with-license"
            version = "1.0.0"
            license = "GPL-3.0"
        "#;

        let config: ProjectConfig = toml::from_str(toml).unwrap();
        assert_eq!(config.package.license, Some("GPL-3.0".to_string()));
    }

    #[test]
    fn parse_config_with_optimization() {
        let toml = r#"
            [package]
            name = "optimized"
            version = "1.0.0"

            [build]
            optimization_level = "O2"
        "#;

        let config: ProjectConfig = toml::from_str(toml).unwrap();
        assert!(matches!(
            config.build.optimization_level,
            OptimizationLevel::O2
        ));
    }

    #[test]
    fn parse_invalid_toml() {
        let toml = "this is not valid toml [[[";
        let result: Result<ProjectConfig, _> = toml::from_str(toml);
        assert!(result.is_err());
    }

    #[test]
    fn parse_missing_required_fields() {
        let toml = r#"
            [package]
            name = "missing-version"
        "#;
        let result: Result<ProjectConfig, _> = toml::from_str(toml);
        assert!(result.is_err());
    }

    #[test]
    fn optimization_level_default() {
        let level = OptimizationLevel::default();
        assert!(matches!(level, OptimizationLevel::O0));
    }
}
