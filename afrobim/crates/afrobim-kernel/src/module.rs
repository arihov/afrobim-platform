use std::collections::BTreeMap;

use afrobim_draw::Drawing;
use serde::{Deserialize, Serialize};

use crate::Element;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum Severity {
    Info,
    Warning,
    Error,
}

/// A message a module attaches to an element: a rule check, a missing input, a provenance note.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Note {
    pub severity: Severity,
    pub code: String,
    pub message: String,
}

/// What a module derives from an element's definition.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct Generated {
    /// Geometry in millimetres, Y up, with the element's lower-left corner on the origin.
    pub drawing: Drawing,
    pub notes: Vec<Note>,
    /// Read-only derived values shown with the element (overall size, x-height, area...).
    pub derived: BTreeMap<String, String>,
}

/// A discipline plug-in. It owns one element category and is the only code that understands
/// that category's `data`.
pub trait Module {
    /// The category this module owns, e.g. `"sign"`.
    fn category(&self) -> &str;
    /// Name shown to the user, e.g. `"Traffic signs"`.
    fn display_name(&self) -> &str;
    /// Derives geometry and checks from the element's definition.
    fn generate(&self, element: &Element) -> Result<Generated, String>;
}

/// The modules available to a session, by category.
#[derive(Default)]
pub struct ModuleRegistry {
    modules: Vec<Box<dyn Module>>,
}

impl ModuleRegistry {
    pub fn register(&mut self, module: Box<dyn Module>) {
        self.modules.retain(|m| m.category() != module.category());
        self.modules.push(module);
    }

    pub fn get(&self, category: &str) -> Option<&dyn Module> {
        self.modules.iter().find(|m| m.category() == category).map(|m| m.as_ref())
    }

    pub fn generate(&self, element: &Element) -> Result<Generated, String> {
        match self.get(&element.category) {
            Some(m) => m.generate(element),
            None => Err(format!("No module is installed for category '{}'.", element.category)),
        }
    }

    pub fn categories(&self) -> impl Iterator<Item = (&str, &str)> {
        self.modules.iter().map(|m| (m.category(), m.display_name()))
    }
}
