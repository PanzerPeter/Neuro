//! NEURO Package Manager (neurpm)
//! 
//! A package manager for the NEURO programming language, providing functionality
//! for managing neural network libraries, models, and dependencies.

pub mod config;
pub mod registry;
pub mod package;
pub mod resolver;
pub mod installer;
pub mod cache;
pub mod commands;

use thiserror::Error;
use semver::Version;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::PathBuf;

#[derive(Debug, Error)]
pub enum NeuropmError {
    #[error("Package not found: {name}")]
    PackageNotFound { name: String },
    
    #[error("Version conflict: {package} requires {required} but {actual} is installed")]
    VersionConflict {
        package: String,
        required: String,
        actual: String,
    },
    
    #[error("Invalid package specification: {spec}")]
    InvalidPackageSpec { spec: String },
    
    #[error("Registry error: {message}")]
    RegistryError { message: String },
    
    #[error("Installation failed: {package} - {reason}")]
    InstallationError { package: String, reason: String },
    
    #[error("Dependency resolution failed: {message}")]
    ResolutionError { message: String },
    
    #[error("IO error: {0}")]
    IoError(#[from] std::io::Error),
    
    #[error("Network error: {0}")]
    NetworkError(#[from] reqwest::Error),
    
    #[error("Serialization error: {0}")]
    SerializationError(#[from] serde_json::Error),
    
    #[error("TOML parsing error: {0}")]
    TomlError(#[from] toml::de::Error),
}

pub type NeuropmResult<T> = Result<T, NeuropmError>;

/// Package manifest (neuro.toml)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PackageManifest {
    pub package: PackageInfo,
    pub dependencies: Option<HashMap<String, DependencySpec>>,
    pub dev_dependencies: Option<HashMap<String, DependencySpec>>,
    pub build_dependencies: Option<HashMap<String, DependencySpec>>,
    pub features: Option<HashMap<String, Vec<String>>>,
    pub targets: Option<Vec<Target>>,
    pub neural_config: Option<NeuralConfig>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PackageInfo {
    pub name: String,
    pub version: Version,
    pub authors: Option<Vec<String>>,
    pub description: Option<String>,
    pub license: Option<String>,
    pub repository: Option<String>,
    pub homepage: Option<String>,
    pub documentation: Option<String>,
    pub keywords: Option<Vec<String>>,
    pub categories: Option<Vec<String>>,
    pub edition: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(untagged)]
pub enum DependencySpec {
    Simple(String),
    Detailed {
        version: Option<String>,
        features: Option<Vec<String>>,
        optional: Option<bool>,
        default_features: Option<bool>,
        registry: Option<String>,
        path: Option<PathBuf>,
        git: Option<String>,
        branch: Option<String>,
        tag: Option<String>,
        rev: Option<String>,
    },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Target {
    pub name: String,
    pub target_type: TargetType,
    pub path: Option<PathBuf>,
    pub dependencies: Option<HashMap<String, DependencySpec>>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum TargetType {
    Library,
    Binary,
    Example,
    Test,
    Benchmark,
    NeuralModel,
    Dataset,
}

/// Neural network specific configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NeuralConfig {
    pub framework: Option<String>,
    pub model_type: Option<String>,
    pub input_shape: Option<Vec<usize>>,
    pub output_shape: Option<Vec<usize>>,
    pub precision: Option<String>,
    pub accelerator: Option<AcceleratorConfig>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AcceleratorConfig {
    pub cuda: Option<bool>,
    pub opencl: Option<bool>,
    pub vulkan: Option<bool>,
    pub metal: Option<bool>,
}

/// Package identifier
#[derive(Debug, Clone, Hash, PartialEq, Eq, Serialize, Deserialize)]
pub struct PackageId {
    pub name: String,
    pub version: Version,
    pub source: PackageSource,
}

#[derive(Debug, Clone, Hash, PartialEq, Eq, Serialize, Deserialize)]
pub enum PackageSource {
    Registry { registry: String },
    Git { url: String, rev: String },
    Path { path: PathBuf },
    Local,
}

/// Installed package information
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InstalledPackage {
    pub id: PackageId,
    pub manifest: PackageManifest,
    pub install_path: PathBuf,
    pub files: Vec<PathBuf>,
    pub checksum: String,
    pub installed_at: chrono::DateTime<chrono::Utc>,
}

/// Package registry configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RegistryConfig {
    pub name: String,
    pub url: String,
    pub auth_token: Option<String>,
    pub trusted: bool,
}

/// NEURPM configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NeuropmConfig {
    pub registries: HashMap<String, RegistryConfig>,
    pub cache_dir: PathBuf,
    pub install_dir: PathBuf,
    pub neural_models_dir: PathBuf,
    pub default_registry: String,
    pub offline: bool,
    pub verify_checksums: bool,
}

impl Default for NeuropmConfig {
    fn default() -> Self {
        let home_dir = dirs::home_dir().unwrap_or_else(|| PathBuf::from("."));
        let neurpm_dir = home_dir.join(".neurpm");
        
        Self {
            registries: {
                let mut registries = HashMap::new();
                registries.insert("official".to_string(), RegistryConfig {
                    name: "official".to_string(),
                    url: "https://registry.neuro-lang.org".to_string(),
                    auth_token: None,
                    trusted: true,
                });
                registries
            },
            cache_dir: neurpm_dir.join("cache"),
            install_dir: neurpm_dir.join("packages"),
            neural_models_dir: neurpm_dir.join("models"),
            default_registry: "official".to_string(),
            offline: false,
            verify_checksums: true,
        }
    }
}

/// Context for package operations
pub struct PackageContext {
    pub config: NeuropmConfig,
    pub registry: registry::PackageRegistry,
    pub resolver: resolver::DependencyResolver,
    pub installer: installer::PackageInstaller,
    pub cache: cache::PackageCache,
}

impl PackageContext {
    pub fn new(config: NeuropmConfig) -> NeuropmResult<Self> {
        let registry = registry::PackageRegistry::new(&config)?;
        let resolver = resolver::DependencyResolver::new();
        let installer = installer::PackageInstaller::new(&config)?;
        let cache = cache::PackageCache::new(&config)?;
        
        Ok(Self {
            config,
            registry,
            resolver,
            installer,
            cache,
        })
    }
    
    pub async fn install_package(&mut self, spec: &str) -> NeuropmResult<InstalledPackage> {
        // Parse package specification
        let package_spec = self.parse_package_spec(spec)?;
        
        // Resolve dependencies
        let resolved = self.resolver.resolve(&package_spec).await?;
        
        // Install packages
        self.installer.install_packages(&resolved).await
    }
    
    pub async fn remove_package(&mut self, name: &str) -> NeuropmResult<()> {
        self.installer.remove_package(name).await
    }
    
    pub async fn list_packages(&self) -> NeuropmResult<Vec<InstalledPackage>> {
        self.installer.list_installed().await
    }
    
    pub async fn update_package(&mut self, name: &str) -> NeuropmResult<InstalledPackage> {
        // Remove old version and install latest
        self.remove_package(name).await?;
        self.install_package(name).await
    }
    
    fn parse_package_spec(&self, spec: &str) -> NeuropmResult<PackageSpec> {
        // Parse specifications like:
        // - "package_name"
        // - "package_name@1.0.0"  
        // - "package_name@^1.0"
        // - "git+https://github.com/user/repo"
        
        if spec.starts_with("git+") {
            return Ok(PackageSpec::Git {
                url: spec[4..].to_string(),
                rev: None,
            });
        }
        
        if let Some(at_pos) = spec.find('@') {
            let name = spec[..at_pos].to_string();
            let version_req = spec[at_pos + 1..].to_string();
            Ok(PackageSpec::Registry { name, version_req })
        } else {
            Ok(PackageSpec::Registry {
                name: spec.to_string(),
                version_req: "*".to_string(),
            })
        }
    }
}

#[derive(Debug, Clone)]
pub enum PackageSpec {
    Registry { name: String, version_req: String },
    Git { url: String, rev: Option<String> },
    Path { path: PathBuf },
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_package_spec_parsing() {
        let ctx = PackageContext::new(NeuropmConfig::default()).unwrap();
        
        let spec = ctx.parse_package_spec("mypackage").unwrap();
        if let PackageSpec::Registry { name, version_req } = spec {
            assert_eq!(name, "mypackage");
            assert_eq!(version_req, "*");
        } else {
            panic!("Expected registry spec");
        }
        
        let spec = ctx.parse_package_spec("mypackage@1.0.0").unwrap();
        if let PackageSpec::Registry { name, version_req } = spec {
            assert_eq!(name, "mypackage");
            assert_eq!(version_req, "1.0.0");
        } else {
            panic!("Expected registry spec with version");
        }
    }
    
    #[test]
    fn test_default_config() {
        let config = NeuropmConfig::default();
        assert_eq!(config.default_registry, "official");
        assert!(config.verify_checksums);
        assert!(!config.offline);
    }
}