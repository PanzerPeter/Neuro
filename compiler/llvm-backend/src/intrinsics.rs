//! LLVM intrinsics for NEURO built-in functions

use crate::LLVMError;
use std::ffi::CString;

/// Intrinsic function manager
pub struct IntrinsicsManager {
    context: *mut llvm_sys::LLVMContext,
    module: *mut llvm_sys::LLVMModule,
}

impl IntrinsicsManager {
    /// Create a new intrinsics manager
    pub fn new(context: *mut llvm_sys::LLVMContext, module: *mut llvm_sys::LLVMModule) -> Self {
        Self { context, module }
    }
    
    /// Declare all intrinsic functions in the module
    pub fn declare_all_intrinsics(&self) -> Result<(), LLVMError> {
        self.declare_math_intrinsics()?;
        self.declare_memory_intrinsics()?;
        Ok(())
    }
    
    /// Declare math intrinsics
    fn declare_math_intrinsics(&self) -> Result<(), LLVMError> {
        unsafe {
            let float_type = llvm_sys::core::LLVMFloatTypeInContext(self.context);
            let int_type = llvm_sys::core::LLVMInt32TypeInContext(self.context);
            
            // sin(float) -> float
            let sin_type = llvm_sys::core::LLVMFunctionType(float_type, [float_type].as_mut_ptr(), 1, 0);
            let sin_name = CString::new("sin").unwrap();
            llvm_sys::core::LLVMAddFunction(self.module, sin_name.as_ptr(), sin_type);
            
            // cos(float) -> float
            let cos_type = llvm_sys::core::LLVMFunctionType(float_type, [float_type].as_mut_ptr(), 1, 0);
            let cos_name = CString::new("cos").unwrap();
            llvm_sys::core::LLVMAddFunction(self.module, cos_name.as_ptr(), cos_type);
            
            // sqrt(float) -> float
            let sqrt_type = llvm_sys::core::LLVMFunctionType(float_type, [float_type].as_mut_ptr(), 1, 0);
            let sqrt_name = CString::new("sqrt").unwrap();
            llvm_sys::core::LLVMAddFunction(self.module, sqrt_name.as_ptr(), sqrt_type);
            
            // pow(float, float) -> float
            let pow_args = [float_type, float_type];
            let pow_type = llvm_sys::core::LLVMFunctionType(float_type, pow_args.as_ptr() as *mut _, 2, 0);
            let pow_name = CString::new("pow").unwrap();
            llvm_sys::core::LLVMAddFunction(self.module, pow_name.as_ptr(), pow_type);
            
            // abs(int) -> int
            let abs_int_type = llvm_sys::core::LLVMFunctionType(int_type, [int_type].as_mut_ptr(), 1, 0);
            let abs_int_name = CString::new("abs").unwrap();
            llvm_sys::core::LLVMAddFunction(self.module, abs_int_name.as_ptr(), abs_int_type);
        }
        
        Ok(())
    }
    
    /// Declare memory intrinsics
    fn declare_memory_intrinsics(&self) -> Result<(), LLVMError> {
        unsafe {
            let void_type = llvm_sys::core::LLVMVoidTypeInContext(self.context);
            let i8_ptr_type = llvm_sys::core::LLVMPointerType(llvm_sys::core::LLVMInt8TypeInContext(self.context), 0);
            let size_type = llvm_sys::core::LLVMInt64TypeInContext(self.context);
            
            // malloc(size_t) -> void*
            let malloc_type = llvm_sys::core::LLVMFunctionType(i8_ptr_type, [size_type].as_mut_ptr(), 1, 0);
            let malloc_name = CString::new("malloc").unwrap();
            llvm_sys::core::LLVMAddFunction(self.module, malloc_name.as_ptr(), malloc_type);
            
            // free(void*) -> void
            let free_type = llvm_sys::core::LLVMFunctionType(void_type, [i8_ptr_type].as_mut_ptr(), 1, 0);
            let free_name = CString::new("free").unwrap();
            llvm_sys::core::LLVMAddFunction(self.module, free_name.as_ptr(), free_type);
        }
        
        Ok(())
    }
    
    /// Check if a function is an intrinsic
    pub fn is_intrinsic(&self, function_name: &str) -> bool {
        matches!(function_name, 
            "sin" | "cos" | "tan" | "sqrt" | "log" | "exp" | "pow" | "abs" |
            "malloc" | "free" | "memcpy" | "memset"
        )
    }
}