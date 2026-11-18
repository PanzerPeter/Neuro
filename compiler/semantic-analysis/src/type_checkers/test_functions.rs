// `@test` form rules.
//
// `neuroc test` calls each test from a generated entry point with no arguments and ignores what
// it returns, so a test is a free, non-generic function of no parameters returning unit. How
// tests are found and run is the driver's; what is checked here is only the attribute's shape
// and the places it may not go.

use ast_types::{Attribute, Item};

use crate::errors::TypeError;
use crate::types::Type;

use super::TypeChecker;

const TEST_ATTRIBUTE: &str = "test";

impl TypeChecker {
    /// Run once every free function's signature is registered, so the return type is read
    /// resolved: `-> ()` is unit as surely as no annotation is.
    pub(crate) fn check_test_attributes(&mut self, items: &[Item]) {
        for item in items {
            let (problem, span) = match item {
                Item::Function(func) => {
                    let Some(attr) = test_attribute(&func.attributes) else {
                        continue;
                    };
                    let problem = if !attr.args.is_empty() || !attr.named.is_empty() {
                        "takes no arguments".to_string()
                    } else if !func.generics.is_empty() {
                        "function may not be generic".to_string()
                    } else if !func.params.is_empty() {
                        format!("function takes no parameters, found {}", func.params.len())
                    } else {
                        match self.functions.get(&func.name.name) {
                            Some(Type::Function { ret, .. })
                                if !matches!(**ret, Type::Void | Type::Unknown) =>
                            {
                                format!("function returns nothing, found `{ret}`")
                            }
                            _ => continue,
                        }
                    };
                    (problem, attr.span)
                }
                Item::Impl(def) => {
                    for attr in def
                        .methods
                        .iter()
                        .filter_map(|method| test_attribute(&method.attributes))
                    {
                        self.record_error(TypeError::TestForm {
                            problem: "marks a free function, not a method".to_string(),
                            span: attr.span,
                        });
                    }
                    continue;
                }
                Item::Struct(def) => match test_attribute(&def.attributes) {
                    Some(attr) => ("marks a function, not a struct".to_string(), attr.span),
                    None => continue,
                },
                _ => continue,
            };
            self.record_error(TypeError::TestForm { problem, span });
        }
    }
}

fn test_attribute(attributes: &[Attribute]) -> Option<&Attribute> {
    attributes
        .iter()
        .find(|attr| attr.name.name == TEST_ATTRIBUTE)
}
