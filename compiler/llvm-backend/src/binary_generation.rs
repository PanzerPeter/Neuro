//! Binary generation from LLVM IR
//! 
//! This module handles the compilation of LLVM IR to native executables
//! using the LLVM toolchain.

use crate::{LLVMError, CompilationResult};
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::io::Write;

/// Binary generation backend that converts LLVM IR to executable files
pub struct BinaryGenerator {
    output_dir: PathBuf,
    llvm_tools_path: Option<PathBuf>,
}

/// Options for binary generation
#[derive(Debug, Clone)]
pub struct BinaryOptions {
    pub output_path: PathBuf,
    pub optimization_level: OptimizationLevel,
    pub debug_info: bool,
    pub target_triple: Option<String>,
}

/// LLVM optimization levels
#[derive(Debug, Clone, Copy)]
pub enum OptimizationLevel {
    None,      // -O0
    Less,      // -O1
    Default,   // -O2
    Aggressive,// -O3
}

impl BinaryGenerator {
    /// Create a new binary generator
    pub fn new<P: AsRef<Path>>(output_dir: P) -> Result<Self, LLVMError> {
        let output_dir = output_dir.as_ref().to_path_buf();
        
        // Create output directory if it doesn't exist
        if !output_dir.exists() {
            fs::create_dir_all(&output_dir).map_err(|e| {
                LLVMError::ModuleGeneration {
                    message: format!("Failed to create output directory: {}", e),
                }
            })?;
        }
        
        // Try to find LLVM tools
        let llvm_tools_path = Self::find_llvm_tools();
        
        Ok(BinaryGenerator {
            output_dir,
            llvm_tools_path,
        })
    }
    
    /// Generate native executable from compilation result
    pub fn generate_executable(
        &self,
        compilation_result: &CompilationResult,
        options: &BinaryOptions,
    ) -> Result<PathBuf, LLVMError> {
        tracing::info!("Generating native executable for module: {}", compilation_result.module_name);
        
        // Write LLVM IR to temporary file
        let ir_file = self.output_dir.join(format!("{}.ll", compilation_result.module_name));
        self.write_ir_file(&ir_file, &compilation_result.ir_code)?;
        
        // Compile IR to object file using llc
        let obj_file = self.compile_ir_to_object(&ir_file, options)?;
        
        // Link object file to executable
        let exe_file = self.link_object_to_executable(&obj_file, options)?;
        
        tracing::info!("Successfully generated executable: {}", exe_file.display());
        Ok(exe_file)
    }
    
    /// Write LLVM IR to file
    fn write_ir_file(&self, ir_file: &Path, ir_code: &str) -> Result<(), LLVMError> {
        let mut file = fs::File::create(ir_file).map_err(|e| {
            LLVMError::ModuleGeneration {
                message: format!("Failed to create IR file: {}", e),
            }
        })?;
        
        file.write_all(ir_code.as_bytes()).map_err(|e| {
            LLVMError::ModuleGeneration {
                message: format!("Failed to write IR file: {}", e),
            }
        })?;
        
        Ok(())
    }
    
    /// Compile LLVM IR to object file using llc (LLVM static compiler)
    fn compile_ir_to_object(&self, ir_file: &Path, options: &BinaryOptions) -> Result<PathBuf, LLVMError> {
        let obj_file = ir_file.with_extension("o");
        
        let llc_path = self.find_llc_path()?;
        
        let mut cmd = Command::new(&llc_path);
        cmd.arg("-filetype=obj")
           .arg(&format!("-O{}", self.optimization_flag(options.optimization_level)))
           .arg("-o").arg(&obj_file)
           .arg(ir_file);
        
        // Add target triple if specified
        if let Some(ref target) = options.target_triple {
            cmd.arg(&format!("-mtriple={}", target));
        }
        
        tracing::debug!("Running llc command: {:?}", cmd);
        
        let output = cmd.output().map_err(|e| {
            LLVMError::ModuleGeneration {
                message: format!("Failed to run llc: {}. Make sure LLVM is installed and in PATH.", e),
            }
        })?;
        
        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            return Err(LLVMError::ModuleGeneration {
                message: format!("llc failed: {}", stderr),
            });
        }
        
        Ok(obj_file)
    }
    
    /// Link object file to executable
    fn link_object_to_executable(&self, obj_file: &Path, options: &BinaryOptions) -> Result<PathBuf, LLVMError> {
        // Try different linkers in order of preference
        let linkers = vec!["clang", "gcc", "ld"];
        
        for linker in linkers {
            if let Ok(exe_file) = self.try_link_with_tool(linker, obj_file, options) {
                return Ok(exe_file);
            }
        }
        
        // If all linkers fail, try a direct system call
        self.try_system_link(obj_file, options)
    }
    
    /// Try linking with a specific tool
    fn try_link_with_tool(&self, tool: &str, obj_file: &Path, options: &BinaryOptions) -> Result<PathBuf, LLVMError> {
        let exe_file = if options.output_path.is_absolute() {
            options.output_path.clone()
        } else {
            self.output_dir.join(&options.output_path)
        };
        
        let mut cmd = Command::new(tool);
        cmd.arg("-o").arg(&exe_file)
           .arg(obj_file);
        
        // Add debug info if requested
        if options.debug_info {
            cmd.arg("-g");
        }
        
        tracing::debug!("Trying to link with {}: {:?}", tool, cmd);
        
        let output = cmd.output().map_err(|_| {
            LLVMError::ModuleGeneration {
                message: format!("{} not found or failed", tool),
            }
        })?;
        
        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            return Err(LLVMError::ModuleGeneration {
                message: format!("{} linking failed: {}", tool, stderr),
            });
        }
        
        Ok(exe_file)
    }
    
    /// Try system-specific linking as fallback
    fn try_system_link(&self, obj_file: &Path, options: &BinaryOptions) -> Result<PathBuf, LLVMError> {
        #[cfg(target_os = "windows")]
        {
            // Try using Microsoft link.exe on Windows
            let exe_file = if options.output_path.is_absolute() {
                options.output_path.clone()
            } else {
                self.output_dir.join(&options.output_path)
            }.with_extension("exe");
            
            let mut cmd = Command::new("link");
            cmd.arg("/OUT:".to_string() + &exe_file.to_string_lossy())
               .arg(obj_file)
               .arg("/SUBSYSTEM:CONSOLE")
               .arg("/ENTRY:main");
            
            let output = cmd.output().map_err(|e| {
                LLVMError::ModuleGeneration {
                    message: format!("Windows link.exe failed: {}", e),
                }
            })?;
            
            if !output.status.success() {
                let stderr = String::from_utf8_lossy(&output.stderr);
                return Err(LLVMError::ModuleGeneration {
                    message: format!("Windows linking failed: {}", stderr),
                });
            }
            
            Ok(exe_file)
        }
        
        #[cfg(not(target_os = "windows"))]
        {
            Err(LLVMError::ModuleGeneration {
                message: "No suitable linker found".to_string(),
            })
        }
    }
    
    /// Find LLVM tools installation
    fn find_llvm_tools() -> Option<PathBuf> {
        // Try common LLVM installation paths
        let common_paths = vec![
            "/usr/bin",
            "/usr/local/bin", 
            "/opt/homebrew/bin",
            "C:\\Program Files\\LLVM\\bin",
            "C:\\LLVM\\bin",
        ];
        
        for path in common_paths {
            let llc_path = PathBuf::from(path).join(if cfg!(windows) { "llc.exe" } else { "llc" });
            if llc_path.exists() {
                return Some(PathBuf::from(path));
            }
        }
        
        None
    }
    
    /// Find llc (LLVM static compiler) path
    fn find_llc_path(&self) -> Result<PathBuf, LLVMError> {
        let llc_name = if cfg!(windows) { "llc.exe" } else { "llc" };
        
        // Try user-specified LLVM tools path
        if let Some(ref tools_path) = self.llvm_tools_path {
            let llc_path = tools_path.join(llc_name);
            if llc_path.exists() {
                return Ok(llc_path);
            }
        }
        
        // Try PATH
        if let Ok(output) = Command::new("where").arg(llc_name).output() {
            if output.status.success() {
                let path_str = String::from_utf8_lossy(&output.stdout);
                if let Some(first_line) = path_str.lines().next() {
                    return Ok(PathBuf::from(first_line.trim()));
                }
            }
        }
        
        // Try 'which' on Unix-like systems
        if let Ok(output) = Command::new("which").arg(llc_name).output() {
            if output.status.success() {
                let path_str = String::from_utf8_lossy(&output.stdout);
                return Ok(PathBuf::from(path_str.trim()));
            }
        }
        
        // Fallback to just the name and hope it's in PATH
        Ok(PathBuf::from(llc_name))
    }
    
    /// Convert optimization level to llc flag
    fn optimization_flag(&self, level: OptimizationLevel) -> &'static str {
        match level {
            OptimizationLevel::None => "0",
            OptimizationLevel::Less => "1", 
            OptimizationLevel::Default => "2",
            OptimizationLevel::Aggressive => "3",
        }
    }
}

impl Default for BinaryOptions {
    fn default() -> Self {
        Self {
            output_path: PathBuf::from("output"),
            optimization_level: OptimizationLevel::Default,
            debug_info: false,
            target_triple: None,
        }
    }
}