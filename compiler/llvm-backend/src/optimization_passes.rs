//! LLVM optimization passes for NEURO
//! 
//! This module provides optimization pass management for LLVM IR.

use crate::LLVMError;

/// Optimization level
#[derive(Debug, Clone, Copy)]
pub enum OptimizationLevel {
    O0, // No optimization
    O1, // Basic optimization
    O2, // Moderate optimization
    O3, // Aggressive optimization
}

impl From<u8> for OptimizationLevel {
    fn from(level: u8) -> Self {
        match level {
            0 => OptimizationLevel::O0,
            1 => OptimizationLevel::O1,
            2 => OptimizationLevel::O2,
            _ => OptimizationLevel::O3,
        }
    }
}

/// Optimization pass manager
pub struct OptimizationPassManager {
    level: OptimizationLevel,
}

impl OptimizationPassManager {
    /// Create a new optimization pass manager
    pub fn new(level: OptimizationLevel) -> Self {
        Self { level }
    }
    
    /// Apply optimization passes to a module
    pub fn optimize_module(&self, module: *mut llvm_sys::LLVMModule) -> Result<(), LLVMError> {
        if matches!(self.level, OptimizationLevel::O0) {
            // No optimization
            return Ok(());
        }
        
        tracing::debug!("Applying optimization level {:?}", self.level);
        
        unsafe {
            // Create pass manager
            let pm = llvm_sys::core::LLVMCreatePassManager();
            if pm.is_null() {
                return Err(LLVMError::ModuleGeneration {
                    message: "Failed to create pass manager".to_string(),
                });
            }
            
            // Add optimization passes based on level
            self.add_passes(pm);
            
            // Run the passes
            llvm_sys::core::LLVMRunPassManager(pm, module);
            
            // Clean up
            llvm_sys::core::LLVMDisposePassManager(pm);
        }
        
        Ok(())
    }
    
    /// Add passes to the pass manager
    unsafe fn add_passes(&self, pm: *mut llvm_sys::LLVMPassManager) {
        match self.level {
            OptimizationLevel::O0 => {
                // No passes
            },
            OptimizationLevel::O1 => {
                // Basic optimizations
                llvm_sys::transforms::LLVMAddInstructionCombiningPass(pm);
                llvm_sys::transforms::LLVMAddReassociatePass(pm);
                llvm_sys::transforms::LLVMAddGVNPass(pm);
                llvm_sys::transforms::LLVMAddCFGSimplificationPass(pm);
            },
            OptimizationLevel::O2 => {
                // O1 passes plus more
                llvm_sys::transforms::LLVMAddInstructionCombiningPass(pm);
                llvm_sys::transforms::LLVMAddReassociatePass(pm);
                llvm_sys::transforms::LLVMAddGVNPass(pm);
                llvm_sys::transforms::LLVMAddCFGSimplificationPass(pm);
                llvm_sys::transforms::LLVMAddMemCpyOptPass(pm);
                llvm_sys::transforms::LLVMAddSCCPPass(pm);
            },
            OptimizationLevel::O3 => {
                // All optimizations
                llvm_sys::transforms::LLVMAddInstructionCombiningPass(pm);
                llvm_sys::transforms::LLVMAddReassociatePass(pm);
                llvm_sys::transforms::LLVMAddGVNPass(pm);
                llvm_sys::transforms::LLVMAddCFGSimplificationPass(pm);
                llvm_sys::transforms::LLVMAddMemCpyOptPass(pm);
                llvm_sys::transforms::LLVMAddSCCPPass(pm);
                llvm_sys::transforms::LLVMAddPromoteMemoryToRegisterPass(pm);
                llvm_sys::transforms::LLVMAddAggressiveDCEPass(pm);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_optimization_level_conversion() {
        assert!(matches!(OptimizationLevel::from(0), OptimizationLevel::O0));
        assert!(matches!(OptimizationLevel::from(1), OptimizationLevel::O1));
        assert!(matches!(OptimizationLevel::from(2), OptimizationLevel::O2));
        assert!(matches!(OptimizationLevel::from(3), OptimizationLevel::O3));
        assert!(matches!(OptimizationLevel::from(99), OptimizationLevel::O3));
    }
    
    #[test]
    fn test_pass_manager_creation() {
        let pm = OptimizationPassManager::new(OptimizationLevel::O2);
        assert!(matches!(pm.level, OptimizationLevel::O2));
    }
}