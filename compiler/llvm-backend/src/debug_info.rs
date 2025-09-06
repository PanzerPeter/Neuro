//! Debug information generation for LLVM backend

use crate::LLVMError;
use shared_types::Span;
use std::ffi::CString;

/// Debug information builder
pub struct DebugInfoBuilder {
    context: *mut llvm_sys::LLVMContext,
    module: *mut llvm_sys::LLVMModule,
    _builder: *mut llvm_sys::core::LLVMDIBuilder,
}

impl DebugInfoBuilder {
    /// Create a new debug info builder
    pub fn new(
        context: *mut llvm_sys::LLVMContext,
        module: *mut llvm_sys::LLVMModule,
        filename: &str,
    ) -> Result<Self, LLVMError> {
        unsafe {
            // Create debug info builder
            let builder = llvm_sys::debuginfo::LLVMCreateDIBuilder(module);
            if builder.is_null() {
                return Err(LLVMError::ModuleGeneration {
                    message: "Failed to create debug info builder".to_string(),
                });
            }
            
            // Create compile unit
            let filename_cstr = CString::new(filename).unwrap();
            let directory_cstr = CString::new(".").unwrap();
            let producer_cstr = CString::new("NEURO Compiler v0.1.0").unwrap();
            
            let _compile_unit = llvm_sys::debuginfo::LLVMDIBuilderCreateCompileUnit(
                builder,
                llvm_sys::debuginfo::LLVMDWARFSourceLanguage::DW_LANG_C, // Use C for now
                llvm_sys::debuginfo::LLVMDIBuilderCreateFile(
                    builder,
                    filename_cstr.as_ptr(),
                    filename.len(),
                    directory_cstr.as_ptr(),
                    1,
                ),
                producer_cstr.as_ptr(),
                producer_cstr.as_str().len(),
                0, // optimized
                std::ptr::null(), // flags
                0, // flags len
                0, // runtime version
                std::ptr::null(), // split debug filename
                0, // split debug filename len
                llvm_sys::debuginfo::LLVMDWARFEmissionKind::LLVMDWARFEmissionFull,
                0, // dwo id
                0, // split debug inlining
                0, // debug info for profiling
                std::ptr::null(), // sys root
                0, // sys root len
                std::ptr::null(), // sdk
                0, // sdk len
            );
            
            Ok(Self {
                context,
                module,
                _builder: builder,
            })
        }
    }
    
    /// Add debug location for a span
    pub fn add_debug_location(&self, _span: Span) -> Result<(), LLVMError> {
        // For now, this is a placeholder
        // In a full implementation, we would create debug metadata
        // and attach it to LLVM instructions
        Ok(())
    }
    
    /// Finalize debug information
    pub fn finalize(&self) -> Result<(), LLVMError> {
        unsafe {
            llvm_sys::debuginfo::LLVMDIBuilderFinalize(self._builder);
        }
        Ok(())
    }
}

impl Drop for DebugInfoBuilder {
    fn drop(&mut self) {
        unsafe {
            if !self._builder.is_null() {
                llvm_sys::debuginfo::LLVMDisposeDIBuilder(self._builder);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_debug_info_creation() {
        unsafe {
            let context = llvm_sys::core::LLVMContextCreate();
            let module_name = CString::new("test").unwrap();
            let module = llvm_sys::core::LLVMModuleCreateWithNameInContext(
                module_name.as_ptr(),
                context
            );
            
            let debug_builder = DebugInfoBuilder::new(context, module, "test.nr");
            assert!(debug_builder.is_ok());
            
            // Clean up
            let builder = debug_builder.unwrap();
            drop(builder);
            llvm_sys::core::LLVMDisposeModule(module);
            llvm_sys::core::LLVMContextDispose(context);
        }
    }
}