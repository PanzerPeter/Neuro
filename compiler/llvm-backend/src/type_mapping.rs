//! Type mapping from NEURO types to LLVM types
//! 
//! This module handles the conversion of NEURO's type system to LLVM IR types.

use shared_types::{Type, Span};
use crate::LLVMError;
// use llvm_sys::{LLVMType, core}; // Disabled for text-based implementation

/// Maps NEURO types to LLVM types
pub struct TypeMapper {
    // context: *mut llvm_sys::LLVMContext, // Disabled
}

impl TypeMapper {
    /// Create a new type mapper
    pub fn new(context: *mut llvm_sys::LLVMContext) -> Self {
        Self { context }
    }
    
    /// Convert a NEURO type to an LLVM type
    pub fn map_type(&self, neuro_type: &Type, span: Span) -> Result<*mut LLVMType, LLVMError> {
        unsafe {
            let llvm_type = match neuro_type {
                Type::Int => core::LLVMInt32TypeInContext(self.context),
                Type::Float => core::LLVMFloatTypeInContext(self.context),
                Type::Bool => core::LLVMInt1TypeInContext(self.context),
                Type::String => {
                    // String is represented as i8* (pointer to i8)
                    let i8_type = core::LLVMInt8TypeInContext(self.context);
                    core::LLVMPointerType(i8_type, 0)
                },
                Type::Void => core::LLVMVoidTypeInContext(self.context),
                Type::Unknown => {
                    return Err(LLVMError::TypeConversion {
                        message: "Cannot map unknown type".to_string(),
                        span,
                    });
                },
            };
            
            if llvm_type.is_null() {
                return Err(LLVMError::TypeConversion {
                    message: format!("Failed to create LLVM type for {:?}", neuro_type),
                    span,
                });
            }
            
            Ok(llvm_type)
        }
    }
    
    /// Get the function type for a NEURO function signature
    pub fn map_function_type(
        &self,
        param_types: &[Type],
        return_type: &Type,
        span: Span,
    ) -> Result<*mut LLVMType, LLVMError> {
        unsafe {
            // Convert parameter types
            let mut llvm_param_types: Vec<*mut LLVMType> = Vec::new();
            for param_type in param_types {
                let llvm_param_type = self.map_type(param_type, span)?;
                llvm_param_types.push(llvm_param_type);
            }
            
            // Convert return type
            let llvm_return_type = self.map_type(return_type, span)?;
            
            // Create function type
            let function_type = core::LLVMFunctionType(
                llvm_return_type,
                llvm_param_types.as_mut_ptr(),
                llvm_param_types.len() as u32,
                0, // Not variadic
            );
            
            if function_type.is_null() {
                return Err(LLVMError::TypeConversion {
                    message: "Failed to create LLVM function type".to_string(),
                    span,
                });
            }
            
            Ok(function_type)
        }
    }
    
    /// Get the default value for a given NEURO type
    pub fn get_default_value(&self, neuro_type: &Type, span: Span) -> Result<*mut llvm_sys::LLVMValue, LLVMError> {
        unsafe {
            let value = match neuro_type {
                Type::Int => {
                    let int_type = self.map_type(neuro_type, span)?;
                    core::LLVMConstInt(int_type, 0, 0)
                },
                Type::Float => {
                    let float_type = self.map_type(neuro_type, span)?;
                    core::LLVMConstReal(float_type, 0.0)
                },
                Type::Bool => {
                    let bool_type = self.map_type(neuro_type, span)?;
                    core::LLVMConstInt(bool_type, 0, 0) // false
                },
                Type::String => {
                    // Empty string constant
                    let empty_str = std::ffi::CString::new("").unwrap();
                    let global_string = core::LLVMConstStringInContext(
                        self.context,
                        empty_str.as_ptr(),
                        0,
                        0, // null terminated
                    );
                    global_string
                },
                Type::Void => {
                    return Err(LLVMError::TypeConversion {
                        message: "Void type has no default value".to_string(),
                        span,
                    });
                },
                Type::Unknown => {
                    return Err(LLVMError::TypeConversion {
                        message: "Cannot create default value for unknown type".to_string(),
                        span,
                    });
                },
            };
            
            if value.is_null() {
                return Err(LLVMError::TypeConversion {
                    message: format!("Failed to create default value for {:?}", neuro_type),
                    span,
                });
            }
            
            Ok(value)
        }
    }
    
    /// Check if a type is a pointer type
    pub fn is_pointer_type(&self, neuro_type: &Type) -> bool {
        matches!(neuro_type, Type::String)
    }
    
    /// Get the size of a type in bytes
    pub fn get_type_size(&self, neuro_type: &Type, span: Span) -> Result<u64, LLVMError> {
        match neuro_type {
            Type::Int => Ok(4),   // 32-bit integer
            Type::Float => Ok(4), // 32-bit float  
            Type::Bool => Ok(1),  // 1 bit, but stored as 1 byte
            Type::String => Ok(8), // Pointer size (64-bit)
            Type::Void => Ok(0),
            Type::Unknown => Err(LLVMError::TypeConversion {
                message: "Cannot determine size of unknown type".to_string(),
                span,
            }),
        }
    }
}