//! Identifier and symbol types

use serde::{Deserialize, Serialize};
use std::fmt;

/// An identifier in NEURO source code
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct Identifier {
    pub name: String,
}

impl Identifier {
    pub fn new(name: impl Into<String>) -> Self {
        Self { name: name.into() }
    }
}

impl fmt::Display for Identifier {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.name)
    }
}

impl From<String> for Identifier {
    fn from(name: String) -> Self {
        Self::new(name)
    }
}

impl From<&str> for Identifier {
    fn from(name: &str) -> Self {
        Self::new(name)
    }
}