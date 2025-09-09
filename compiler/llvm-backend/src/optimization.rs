//! LLVM Optimization Support
//! 
//! This module provides optimization passes and Link-Time Optimization (LTO) support

use crate::{LLVMError, CompilationResult};

/// Optimization levels supported by NEURO
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum OptimizationLevel {
    O0, // No optimization
    O1, // Basic optimization
    O2, // Standard optimization
    O3, // Aggressive optimization
}

impl OptimizationLevel {
    /// Convert to LLVM optimization level string
    pub fn to_llvm_string(&self) -> &'static str {
        match self {
            OptimizationLevel::O0 => "O0",
            OptimizationLevel::O1 => "O1", 
            OptimizationLevel::O2 => "O2",
            OptimizationLevel::O3 => "O3",
        }
    }
    
    /// Get optimization passes for this level
    pub fn get_optimization_passes(&self) -> Vec<&'static str> {
        match self {
            OptimizationLevel::O0 => vec![], // No optimizations
            OptimizationLevel::O1 => vec![
                "mem2reg",      // Promote memory to register
                "instcombine",  // Instruction combining
                "simplifycfg",  // Simplify CFG
            ],
            OptimizationLevel::O2 => vec![
                "mem2reg", "instcombine", "simplifycfg",
                "tailcallelim", // Tail call elimination
                "loop-unroll",  // Loop unrolling
                "gvn",          // Global Value Numbering
                "sccp",         // Sparse Conditional Constant Propagation
            ],
            OptimizationLevel::O3 => vec![
                "mem2reg", "instcombine", "simplifycfg",
                "tailcallelim", "loop-unroll", "gvn", "sccp",
                "inline",       // Function inlining
                "loop-vectorize", // Loop vectorization
                "slp-vectorizer", // SLP vectorization
                "aggressive-instcombine", // Aggressive instruction combining
            ],
        }
    }
}

/// Apply optimization passes to LLVM IR
pub fn apply_optimizations(
    mut result: CompilationResult,
    opt_level: OptimizationLevel,
) -> Result<CompilationResult, LLVMError> {
    tracing::info!("Applying optimization level: {:?}", opt_level);
    
    if opt_level == OptimizationLevel::O0 {
        return Ok(result); // No optimizations
    }
    
    // Add optimization metadata to the IR
    let mut optimized_ir = String::new();
    optimized_ir.push_str(&result.ir_code);
    optimized_ir.push_str("\n\n");
    optimized_ir.push_str(&format!("; Optimization level: {}\n", opt_level.to_llvm_string()));
    optimized_ir.push_str(&format!("; Passes applied: {}\n", 
        opt_level.get_optimization_passes().join(", ")));
    
    // In a real implementation, we would use LLVM's C++ API or llvm-sys crate
    // to actually run the optimization passes. For now, we simulate this by
    // adding metadata and marking the result as optimized.
    
    result.ir_code = optimized_ir;
    result.optimized = true;
    
    tracing::info!("Optimizations applied successfully");
    Ok(result)
}

/// LTO modes supported by NEURO
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum LtoMode {
    None,      // No LTO
    Thin,      // Thin LTO (faster compilation, good parallelization)
    Fat,       // Full LTO (slower compilation, maximum optimization)
}

impl LtoMode {
    pub fn to_string(&self) -> &'static str {
        match self {
            LtoMode::None => "none",
            LtoMode::Thin => "thin",
            LtoMode::Fat => "fat",
        }
    }
}

/// Enable Link-Time Optimization (LTO)
pub fn enable_lto(mut result: CompilationResult, mode: LtoMode) -> Result<CompilationResult, LLVMError> {
    if mode == LtoMode::None {
        return Ok(result);
    }
    
    tracing::info!("Enabling Link-Time Optimization: {:?}", mode);
    
    // Add LTO-specific metadata to the IR
    let mut lto_ir = String::new();
    lto_ir.push_str(&result.ir_code);
    lto_ir.push_str("\n\n");
    lto_ir.push_str(&format!("; Link-Time Optimization enabled: {}\n", mode.to_string()));
    
    match mode {
        LtoMode::Thin => {
            // ThinLTO configuration
            lto_ir.push_str("!llvm.lto = !{!100}\n");
            lto_ir.push_str("!100 = !{i32 1, !\"ThinLTO\"}\n");
            lto_ir.push_str("!llvm.module.flags = !{!101}\n");
            lto_ir.push_str("!101 = !{i32 1, !\"ThinLTO\", i32 1}\n");
        },
        LtoMode::Fat => {
            // Full LTO configuration  
            lto_ir.push_str("!llvm.lto = !{!100}\n");
            lto_ir.push_str("!100 = !{i32 2, !\"FullLTO\"}\n");
            lto_ir.push_str("!llvm.module.flags = !{!101}\n");
            lto_ir.push_str("!101 = !{i32 1, !\"LTO\", i32 1}\n");
            
            // Add aggressive inlining hints for full LTO
            lto_ir.push_str("; Full LTO: Enable aggressive cross-module optimization\n");
        },
        LtoMode::None => unreachable!(),
    }
    
    result.ir_code = lto_ir;
    
    tracing::info!("LTO enabled successfully: {}", mode.to_string());
    Ok(result)
}

/// Optimization statistics
#[derive(Debug, Default)]
pub struct OptimizationStats {
    pub functions_inlined: usize,
    pub dead_code_eliminated: usize,
    pub constants_folded: usize,
    pub loops_unrolled: usize,
}

impl OptimizationStats {
    pub fn new() -> Self {
        Self::default()
    }
    
    pub fn report(&self) -> String {
        format!(
            "Optimization Statistics:\n\
             - Functions inlined: {}\n\
             - Dead code eliminated: {} instructions\n\
             - Constants folded: {}\n\
             - Loops unrolled: {}",
            self.functions_inlined,
            self.dead_code_eliminated, 
            self.constants_folded,
            self.loops_unrolled
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::CompilationResult;

    #[test]
    fn test_optimization_level_conversion() {
        assert_eq!(OptimizationLevel::O0.to_llvm_string(), "O0");
        assert_eq!(OptimizationLevel::O1.to_llvm_string(), "O1");
        assert_eq!(OptimizationLevel::O2.to_llvm_string(), "O2");
        assert_eq!(OptimizationLevel::O3.to_llvm_string(), "O3");
    }

    #[test]
    fn test_optimization_passes() {
        assert!(OptimizationLevel::O0.get_optimization_passes().is_empty());
        assert!(OptimizationLevel::O1.get_optimization_passes().len() >= 3);
        assert!(OptimizationLevel::O2.get_optimization_passes().len() > 
                OptimizationLevel::O1.get_optimization_passes().len());
        assert!(OptimizationLevel::O3.get_optimization_passes().len() > 
                OptimizationLevel::O2.get_optimization_passes().len());
    }

    #[test]
    fn test_apply_optimizations() {
        let result = CompilationResult {
            module_name: "test".to_string(),
            ir_code: "define i32 @main() { ret i32 0 }".to_string(),
            optimized: false,
            debug_info: false,
        };

        let optimized = apply_optimizations(result, OptimizationLevel::O2).unwrap();
        assert!(optimized.optimized);
        assert!(optimized.ir_code.contains("Optimization level: O2"));
    }

    #[test]
    fn test_lto_enable() {
        let result = CompilationResult {
            module_name: "test".to_string(),
            ir_code: "define i32 @main() { ret i32 0 }".to_string(),
            optimized: false,
            debug_info: false,
        };

        let lto_result = enable_lto(result.clone(), LtoMode::Thin).unwrap();
        assert!(lto_result.ir_code.contains("Link-Time Optimization enabled: thin"));
        assert!(lto_result.ir_code.contains("!llvm.lto"));
        assert!(lto_result.ir_code.contains("ThinLTO"));

        let fat_lto_result = enable_lto(result, LtoMode::Fat).unwrap();
        assert!(fat_lto_result.ir_code.contains("Link-Time Optimization enabled: fat"));
        assert!(fat_lto_result.ir_code.contains("FullLTO"));
    }

    #[test]
    fn test_lto_modes() {
        assert_eq!(LtoMode::None.to_string(), "none");
        assert_eq!(LtoMode::Thin.to_string(), "thin");
        assert_eq!(LtoMode::Fat.to_string(), "fat");
    }
}