// `@track_caller` form rules.
//
// A `@track_caller` function takes its caller's location as a hidden trailing argument, which
// every direct call supplies, so it must be reached by name: a function value carries no
// location to pass. Device code prints no diagnostic, and a test has no caller to name.

use ast_types::{Attribute, Item};
use shared_types::Identifier;

use crate::errors::TypeError;

use super::TypeChecker;

const TRACK_CALLER_ATTRIBUTE: &str = "track_caller";

/// Attributes whose functions have no host caller for a location to come from.
const EXCLUSIVE_WITH: [&str; 3] = ["gpu", "kernel", "test"];

impl TypeChecker {
    pub(crate) fn check_track_caller_attributes(&mut self, items: &[Item]) {
        for item in items {
            match item {
                Item::Function(func) => {
                    let Some(attr) = track_caller_attribute(&func.attributes) else {
                        continue;
                    };
                    let _ = self.track_caller_fns.insert(func.name.name.clone());
                    if !attr.args.is_empty() || !attr.named.is_empty() {
                        self.track_caller_error("takes no arguments".to_string(), attr.span);
                    }
                    for other in func
                        .attributes
                        .iter()
                        .filter(|a| EXCLUSIVE_WITH.contains(&a.name.name.as_str()))
                    {
                        let problem = format!("cannot mark a `@{}` function", other.name.name);
                        self.track_caller_error(problem, attr.span);
                    }
                }
                Item::Impl(def) => {
                    for attr in def
                        .methods
                        .iter()
                        .filter_map(|method| track_caller_attribute(&method.attributes))
                    {
                        let problem = "marks a free function, not a method".to_string();
                        self.track_caller_error(problem, attr.span);
                    }
                }
                Item::Struct(def) => {
                    if let Some(attr) = track_caller_attribute(&def.attributes) {
                        let problem = "marks a function, not a struct".to_string();
                        self.track_caller_error(problem, attr.span);
                    }
                }
                _ => {}
            }
        }
    }

    /// Refuse `name`, a `@track_caller` function, read as a value.
    pub(crate) fn refuse_track_caller_value(&mut self, name: &Identifier) {
        let problem = format!(
            "function '{0}' is called by name; it is not a function value, so wrap the call in \
             a closure (`|x| {0}(x)`), whose call then names the closure",
            name.name
        );
        self.track_caller_error(problem, name.span);
    }

    fn track_caller_error(&mut self, problem: String, span: shared_types::Span) {
        self.record_error(TypeError::TrackCallerForm { problem, span });
    }
}

fn track_caller_attribute(attributes: &[Attribute]) -> Option<&Attribute> {
    attributes
        .iter()
        .find(|attr| attr.name.name == TRACK_CALLER_ATTRIBUTE)
}
