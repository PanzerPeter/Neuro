//! Template engine for NEURO macro system

use crate::{MacroError, MacroResult, TemplateValue};
use handlebars::{Handlebars, Context, RenderContext, RenderError, Helper, Output, HelperResult};
use std::collections::HashMap;
use serde_json::{Value, Map};

/// Template engine for macro expansion
pub struct TemplateEngine {
    handlebars: Handlebars<'static>,
}

impl TemplateEngine {
    pub fn new() -> Self {
        let mut handlebars = Handlebars::new();
        
        // Register custom helpers for NEURO-specific functionality
        handlebars.register_helper("camel_case", Box::new(camel_case_helper));
        handlebars.register_helper("snake_case", Box::new(snake_case_helper));
        handlebars.register_helper("upper_case", Box::new(upper_case_helper));
        handlebars.register_helper("tensor_shape", Box::new(tensor_shape_helper));
        handlebars.register_helper("neural_layer", Box::new(neural_layer_helper));
        handlebars.register_helper("type_size", Box::new(type_size_helper));
        handlebars.register_helper("repeat", Box::new(repeat_helper));
        handlebars.register_helper("range", Box::new(range_helper));
        
        // Configure handlebars
        handlebars.set_strict_mode(true);
        
        Self { handlebars }
    }
    
    /// Render a template with given data
    pub fn render(&self, template: &str, data: &HashMap<String, TemplateValue>) -> MacroResult<String> {
        let json_data = self.convert_to_json(data);
        
        self.handlebars
            .render_template(template, &json_data)
            .map_err(|e| MacroError::TemplateError {
                message: format!("Template rendering failed: {}", e),
                span: shared_types::Span::new(0, 0),
            })
    }
    
    /// Register a template for reuse
    pub fn register_template(&mut self, name: &str, source: &str) -> MacroResult<()> {
        self.handlebars
            .register_template_string(name, source)
            .map_err(|e| MacroError::TemplateError {
                message: format!("Template registration failed: {}", e),
                span: shared_types::Span::new(0, 0),
            })
    }
    
    /// Render a registered template
    pub fn render_registered(&self, name: &str, data: &HashMap<String, TemplateValue>) -> MacroResult<String> {
        let json_data = self.convert_to_json(data);
        
        self.handlebars
            .render(name, &json_data)
            .map_err(|e| MacroError::TemplateError {
                message: format!("Template rendering failed: {}", e),
                span: shared_types::Span::new(0, 0),
            })
    }
    
    /// Convert TemplateValue to JSON for handlebars
    fn convert_to_json(&self, data: &HashMap<String, TemplateValue>) -> Value {
        let mut map = Map::new();
        
        for (key, value) in data {
            map.insert(key.clone(), self.template_value_to_json(value));
        }
        
        Value::Object(map)
    }
    
    /// Convert a single TemplateValue to JSON
    fn template_value_to_json(&self, value: &TemplateValue) -> Value {
        match value {
            TemplateValue::String(s) => Value::String(s.clone()),
            TemplateValue::Integer(i) => Value::Number((*i).into()),
            TemplateValue::Float(f) => Value::Number(serde_json::Number::from_f64(*f).unwrap_or_else(|| 0.into())),
            TemplateValue::Boolean(b) => Value::Bool(*b),
            TemplateValue::Array(arr) => {
                Value::Array(arr.iter().map(|v| self.template_value_to_json(v)).collect())
            }
            TemplateValue::Object(obj) => {
                let mut map = Map::new();
                for (k, v) in obj {
                    map.insert(k.clone(), self.template_value_to_json(v));
                }
                Value::Object(map)
            }
        }
    }
}

impl Default for TemplateEngine {
    fn default() -> Self {
        Self::new()
    }
}

/// Simple template expansion function
pub fn expand_template(template: &str, bindings: &HashMap<String, String>) -> Result<String, Box<dyn std::error::Error>> {
    let engine = TemplateEngine::new();
    
    // Convert string bindings to TemplateValues
    let template_data: HashMap<String, TemplateValue> = bindings
        .iter()
        .map(|(k, v)| (k.clone(), TemplateValue::String(v.clone())))
        .collect();
    
    engine.render(template, &template_data)
        .map_err(|e| Box::new(e) as Box<dyn std::error::Error>)
}

// Custom Handlebars helpers

fn camel_case_helper(
    h: &Helper,
    _: &Handlebars,
    _: &Context,
    _: &mut RenderContext,
    out: &mut dyn Output,
) -> HelperResult {
    let param = h.param(0).and_then(|v| v.value().as_str())
        .ok_or_else(|| RenderError::new("camel_case helper requires a string parameter"))?;
    
    let camel_case = to_camel_case(param);
    out.write(&camel_case)?;
    Ok(())
}

fn snake_case_helper(
    h: &Helper,
    _: &Handlebars,
    _: &Context,
    _: &mut RenderContext,
    out: &mut dyn Output,
) -> HelperResult {
    let param = h.param(0).and_then(|v| v.value().as_str())
        .ok_or_else(|| RenderError::new("snake_case helper requires a string parameter"))?;
    
    let snake_case = to_snake_case(param);
    out.write(&snake_case)?;
    Ok(())
}

fn upper_case_helper(
    h: &Helper,
    _: &Handlebars,
    _: &Context,
    _: &mut RenderContext,
    out: &mut dyn Output,
) -> HelperResult {
    let param = h.param(0).and_then(|v| v.value().as_str())
        .ok_or_else(|| RenderError::new("upper_case helper requires a string parameter"))?;
    
    out.write(&param.to_uppercase())?;
    Ok(())
}

fn tensor_shape_helper(
    h: &Helper,
    _: &Handlebars,
    _: &Context,
    _: &mut RenderContext,
    out: &mut dyn Output,
) -> HelperResult {
    let dimensions = h.param(0).and_then(|v| v.value().as_array())
        .ok_or_else(|| RenderError::new("tensor_shape helper requires an array parameter"))?;
    
    let shape_str = dimensions.iter()
        .map(|dim| dim.as_u64().unwrap_or(0).to_string())
        .collect::<Vec<_>>()
        .join(", ");
    
    out.write(&format!("[{}]", shape_str))?;
    Ok(())
}

fn neural_layer_helper(
    h: &Helper,
    _: &Handlebars,
    _: &Context,
    _: &mut RenderContext,
    out: &mut dyn Output,
) -> HelperResult {
    let layer_type = h.param(0).and_then(|v| v.value().as_str())
        .ok_or_else(|| RenderError::new("neural_layer helper requires a layer type"))?;
    
    let input_size = h.param(1).and_then(|v| v.value().as_u64()).unwrap_or(128);
    let output_size = h.param(2).and_then(|v| v.value().as_u64()).unwrap_or(64);
    
    let layer_code = match layer_type {
        "dense" | "linear" => {
            format!(
                "LinearLayer::new({}, {}, ActivationFunction::ReLU)",
                input_size, output_size
            )
        }
        "conv2d" => {
            format!(
                "Conv2DLayer::new({}, {}, (3, 3), (1, 1))", 
                input_size, output_size
            )
        }
        "lstm" => {
            format!("LSTMLayer::new({}, {})", input_size, output_size)
        }
        _ => format!("CustomLayer::new(\"{}\", {}, {})", layer_type, input_size, output_size),
    };
    
    out.write(&layer_code)?;
    Ok(())
}

fn type_size_helper(
    h: &Helper,
    _: &Handlebars,
    _: &Context,
    _: &mut RenderContext,
    out: &mut dyn Output,
) -> HelperResult {
    let type_name = h.param(0).and_then(|v| v.value().as_str())
        .ok_or_else(|| RenderError::new("type_size helper requires a type name"))?;
    
    let size = match type_name {
        "f32" | "float32" => 4,
        "f64" | "float64" => 8,
        "i32" | "int32" => 4,
        "i64" | "int64" => 8,
        "i16" => 2,
        "i8" => 1,
        "u32" => 4,
        "u64" => 8,
        "u16" => 2,
        "u8" => 1,
        "bool" => 1,
        _ => 8, // Default size
    };
    
    out.write(&size.to_string())?;
    Ok(())
}

fn repeat_helper(
    h: &Helper,
    _: &Handlebars,
    _: &Context,
    _: &mut RenderContext,
    out: &mut dyn Output,
) -> HelperResult {
    let count = h.param(0).and_then(|v| v.value().as_u64())
        .ok_or_else(|| RenderError::new("repeat helper requires a count parameter"))? as usize;
    
    let template = h.param(1).and_then(|v| v.value().as_str())
        .ok_or_else(|| RenderError::new("repeat helper requires a template parameter"))?;
    
    for i in 0..count {
        let rendered = template.replace("{{@index}}", &i.to_string());
        out.write(&rendered)?;
        if i < count - 1 {
            out.write("\n")?;
        }
    }
    
    Ok(())
}

fn range_helper(
    h: &Helper,
    _: &Handlebars,
    _: &Context,
    _: &mut RenderContext,
    out: &mut dyn Output,
) -> HelperResult {
    let start = h.param(0).and_then(|v| v.value().as_u64()).unwrap_or(0) as usize;
    let end = h.param(1).and_then(|v| v.value().as_u64())
        .ok_or_else(|| RenderError::new("range helper requires an end parameter"))? as usize;
    
    let range_str = (start..end)
        .map(|i| i.to_string())
        .collect::<Vec<_>>()
        .join(", ");
    
    out.write(&range_str)?;
    Ok(())
}

// Utility functions

fn to_camel_case(s: &str) -> String {
    let mut result = String::new();
    let mut capitalize_next = false;
    
    for c in s.chars() {
        if c == '_' || c == '-' || c == ' ' {
            capitalize_next = true;
        } else if capitalize_next {
            result.push(c.to_uppercase().next().unwrap_or(c));
            capitalize_next = false;
        } else {
            result.push(c.to_lowercase().next().unwrap_or(c));
        }
    }
    
    result
}

fn to_snake_case(s: &str) -> String {
    let mut result = String::new();
    
    for (i, c) in s.chars().enumerate() {
        if c.is_uppercase() && i > 0 {
            result.push('_');
        }
        result.push(c.to_lowercase().next().unwrap_or(c));
    }
    
    result
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_simple_template_expansion() {
        let template = "Hello, {{name}}!";
        let mut data = HashMap::new();
        data.insert("name".to_string(), "World".to_string());
        
        let result = expand_template(template, &data).unwrap();
        assert_eq!(result, "Hello, World!");
    }
    
    #[test]
    fn test_template_engine_creation() {
        let engine = TemplateEngine::new();
        
        let mut data = HashMap::new();
        data.insert("test".to_string(), TemplateValue::String("value".to_string()));
        
        let result = engine.render("{{test}}", &data).unwrap();
        assert_eq!(result, "value");
    }
    
    #[test]
    fn test_camel_case_conversion() {
        assert_eq!(to_camel_case("hello_world"), "helloWorld");
        assert_eq!(to_camel_case("test-case"), "testCase");
        assert_eq!(to_camel_case("simple"), "simple");
    }
    
    #[test]
    fn test_snake_case_conversion() {
        assert_eq!(to_snake_case("HelloWorld"), "hello_world");
        assert_eq!(to_snake_case("TestCase"), "test_case");
        assert_eq!(to_snake_case("simple"), "simple");
    }
    
    #[test]
    fn test_template_with_array() {
        let engine = TemplateEngine::new();
        
        let mut data = HashMap::new();
        let array = vec![
            TemplateValue::Integer(1),
            TemplateValue::Integer(2),
            TemplateValue::Integer(3),
        ];
        data.insert("numbers".to_string(), TemplateValue::Array(array));
        
        let result = engine.render("{{#each numbers}}{{this}}{{#unless @last}}, {{/unless}}{{/each}}", &data).unwrap();
        assert_eq!(result, "1, 2, 3");
    }
}