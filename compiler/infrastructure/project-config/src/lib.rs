// NEURO Programming Language - Project Configuration
// Infrastructure component for project configuration management

use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use thiserror::Error;

/// Project configuration from neuro.toml
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProjectConfig {
    pub package: PackageConfig,
    #[serde(default)]
    pub dependencies: Vec<Dependency>,
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
}
