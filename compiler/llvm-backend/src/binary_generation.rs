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
        // On Windows, try clang from LLVM installation first, then fallback to system linker
        if cfg!(windows) {
            // Try clang from our LLVM installation
            if let Ok(exe_file) = self.try_link_with_tool("clang", obj_file, options) {
                return Ok(exe_file);
            }

            // Fallback to Windows system linker
            return self.try_system_link(obj_file, options);
        } else {
            // On Unix-like systems, try different linkers in order of preference
            let linkers = vec!["clang", "gcc", "ld"];

            for linker in linkers {
                if let Ok(exe_file) = self.try_link_with_tool(linker, obj_file, options) {
                    return Ok(exe_file);
                }
            }

            Err(LLVMError::ModuleGeneration {
                message: "No suitable linker found".to_string(),
            })
        }
    }
    
    /// Try linking with a specific tool
    fn try_link_with_tool(&self, tool: &str, obj_file: &Path, options: &BinaryOptions) -> Result<PathBuf, LLVMError> {
        let exe_file = if options.output_path.is_absolute() {
            options.output_path.clone()
        } else {
            // Check if output_path already contains the output_dir to avoid double-joining
            if options.output_path.starts_with(&self.output_dir) {
                options.output_path.clone()
            } else {
                self.output_dir.join(&options.output_path)
            }
        };

        let mut cmd = Command::new(tool);

        // Use the clang from the LLVM installation we found
        if tool == "clang" {
            let clang_name = if cfg!(windows) { "clang.exe" } else { "clang" };

            // Try multiple paths to find clang
            let clang_paths = vec![
                // User's specific LLVM installation
                PathBuf::from("C:\\LLVM-191\\bin").join(clang_name),
                // Standard LLVM installation paths
                PathBuf::from("C:\\Program Files\\LLVM\\bin").join(clang_name),
                PathBuf::from("C:\\LLVM\\bin").join(clang_name),
            ];

            // Also check the detected LLVM tools path
            if let Some(ref llvm_path) = self.llvm_tools_path {
                let clang_path = llvm_path.join(clang_name);
                if clang_path.exists() {
                    cmd = Command::new(clang_path);
                }
            } else {
                // Try the predefined paths
                for clang_path in &clang_paths {
                    if clang_path.exists() {
                        cmd = Command::new(clang_path);
                        break;
                    }
                }
            }
        }

        cmd.arg("-o").arg(&exe_file)
           .arg(obj_file);

        // Add debug info if requested
        if options.debug_info {
            cmd.arg("-g");
        }

        // On Windows with clang, ensure proper C runtime linking
        if tool == "clang" && cfg!(windows) {
            cmd.arg("-lmsvcrt")  // Link Microsoft C runtime
               .arg("-lkernel32")  // Windows kernel API
               .arg("-luser32")    // Windows user API
               .arg("-llegacy_stdio_definitions");  // For printf compatibility
        }

        tracing::debug!("Trying to link with {}: {:?}", tool, cmd);
        
        let output = cmd.output().map_err(|e| {
            LLVMError::ModuleGeneration {
                message: format!("Failed to execute linker '{}': {}\n\nEnsure {} is installed and available in PATH", tool, e, tool),
            }
        })?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            let stdout = String::from_utf8_lossy(&output.stdout);
            return Err(LLVMError::ModuleGeneration {
                message: format!(
                    "Linking failed with {} (exit code: {})\n\nSTDERR:\n{}\n\nSTDOUT:\n{}\n\nObject file: {}\nOutput path: {}\n\nCommon causes:\n• Undefined symbols (missing function implementations)\n• Missing runtime libraries\n• Incompatible object file format",
                    tool,
                    output.status.code().unwrap_or(-1),
                    stderr,
                    stdout,
                    obj_file.display(),
                    exe_file.display()
                ),
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
                // Check if output_path already contains the output_dir to avoid double-joining
                if options.output_path.starts_with(&self.output_dir) {
                    options.output_path.clone()
                } else {
                    self.output_dir.join(&options.output_path)
                }
            };

            // Ensure .exe extension
            let exe_file = if exe_file.extension().is_none() {
                exe_file.with_extension("exe")
            } else {
                exe_file
            };

            // Try to find Microsoft's link.exe using multiple strategies
            let link_path = Self::find_msvc_link()
                .ok_or_else(|| LLVMError::ModuleGeneration {
                    message: "Microsoft Visual Studio Build Tools not found. Please install Visual Studio Build Tools or use clang for linking. The system 'link' command appears to be the GNU coreutils version which is incompatible with MSVC object files.".to_string(),
                })?;

            let mut cmd = Command::new(&link_path);
            cmd.arg("/OUT:".to_string() + &exe_file.to_string_lossy())
               .arg(obj_file)
               .arg("/SUBSYSTEM:CONSOLE")
               .arg("/ENTRY:main")
               .arg("/DEFAULTLIB:kernel32.lib")    // Windows kernel API
               .arg("/DEFAULTLIB:user32.lib")      // Windows user API
               .arg("/DEFAULTLIB:msvcrt.lib")      // Microsoft C runtime
               .arg("/DEFAULTLIB:legacy_stdio_definitions.lib"); // For printf compatibility

            tracing::debug!("Trying Windows system link: {:?}", cmd);

            let output = cmd.output().map_err(|e| {
                LLVMError::ModuleGeneration {
                    message: format!("Microsoft link.exe failed: {}. Make sure Visual Studio Build Tools are installed.", e),
                }
            })?;

            if !output.status.success() {
                let stderr = String::from_utf8_lossy(&output.stderr);
                let stdout = String::from_utf8_lossy(&output.stdout);
                return Err(LLVMError::ModuleGeneration {
                    message: format!(
                        "Windows linking failed (exit code: {})\n\nSTDERR:\n{}\n\nSTDOUT:\n{}\n\nObject file: {}\nOutput: {}\n\nTip: Install Visual Studio Build Tools or use clang for linking.",
                        output.status.code().unwrap_or(-1),
                        stderr,
                        stdout,
                        obj_file.display(),
                        exe_file.display()
                    ),
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
            "C:\\LLVM-191\\bin",  // User's specific LLVM path - try first
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

    /// Find Microsoft Visual Studio link.exe
    #[cfg(target_os = "windows")]
    fn find_msvc_link() -> Option<PathBuf> {
        // First try using vswhere.exe to find Visual Studio installations
        if let Some(link_path) = Self::find_link_with_vswhere() {
            return Some(link_path);
        }

        // Fallback to hard-coded paths for common VS installations
        let vs_paths = vec![
            // VS 2022
            "C:\\Program Files\\Microsoft Visual Studio\\2022\\BuildTools\\VC\\Tools\\MSVC",
            "C:\\Program Files\\Microsoft Visual Studio\\2022\\Community\\VC\\Tools\\MSVC",
            "C:\\Program Files\\Microsoft Visual Studio\\2022\\Professional\\VC\\Tools\\MSVC",
            "C:\\Program Files\\Microsoft Visual Studio\\2022\\Enterprise\\VC\\Tools\\MSVC",
            "C:\\Program Files (x86)\\Microsoft Visual Studio\\2022\\BuildTools\\VC\\Tools\\MSVC",
            // VS 2019
            "C:\\Program Files (x86)\\Microsoft Visual Studio\\2019\\BuildTools\\VC\\Tools\\MSVC",
            "C:\\Program Files (x86)\\Microsoft Visual Studio\\2019\\Community\\VC\\Tools\\MSVC",
            "C:\\Program Files (x86)\\Microsoft Visual Studio\\2019\\Professional\\VC\\Tools\\MSVC",
            "C:\\Program Files (x86)\\Microsoft Visual Studio\\2019\\Enterprise\\VC\\Tools\\MSVC",
        ];

        for vs_path in vs_paths {
            if let Some(link_path) = Self::find_link_in_vs_path(vs_path) {
                return Some(link_path);
            }
        }

        None
    }

    #[cfg(target_os = "windows")]
    fn find_link_with_vswhere() -> Option<PathBuf> {
        let vswhere_path = "C:\\Program Files (x86)\\Microsoft Visual Studio\\Installer\\vswhere.exe";

        if !PathBuf::from(vswhere_path).exists() {
            return None;
        }

        // Use vswhere to find the latest VS installation
        let output = Command::new(vswhere_path)
            .args(&[
                "-latest",
                "-products", "*",
                "-requires", "Microsoft.VisualStudio.Component.VC.Tools.x86.x64",
                "-property", "installationPath"
            ])
            .output()
            .ok()?;

        if output.status.success() {
            let install_path = String::from_utf8_lossy(&output.stdout).trim().to_string();
            if !install_path.is_empty() {
                let vs_tools_path = PathBuf::from(install_path).join("VC\\Tools\\MSVC");
                return Self::find_link_in_vs_path(&vs_tools_path.to_string_lossy());
            }
        }

        None
    }

    #[cfg(target_os = "windows")]
    fn find_link_in_vs_path(vs_path: &str) -> Option<PathBuf> {
        let vs_path = PathBuf::from(vs_path);
        if !vs_path.exists() {
            return None;
        }

        // Look for version directories (e.g., 14.29.30133, 14.30.30705, etc.)
        if let Ok(entries) = std::fs::read_dir(&vs_path) {
            for entry in entries.flatten() {
                if entry.file_type().map(|ft| ft.is_dir()).unwrap_or(false) {
                    // Try both x64 and x86 host architectures
                    let potential_paths = vec![
                        entry.path().join("bin\\Hostx64\\x64\\link.exe"),
                        entry.path().join("bin\\Hostx86\\x64\\link.exe"),
                        entry.path().join("bin\\Hostx64\\x86\\link.exe"),
                        entry.path().join("bin\\Hostx86\\x86\\link.exe"),
                    ];

                    for link_path in potential_paths {
                        if link_path.exists() {
                            return Some(link_path);
                        }
                    }
                }
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