//! Temporary stand-in for the traffic-signs module while the engine port lands.

use afrobim_draw::{Colour, Drawing, Extents, Primitive, Pt2};
use afrobim_kernel::{Element, Generated, Module};
use serde_json::{json, Value};

pub const CATEGORY: &str = "sign";
pub const LAYER: &str = "Signs";

struct Stub;

impl Module for Stub {
    fn category(&self) -> &str {
        CATEGORY
    }
    fn display_name(&self) -> &str {
        "Traffic signs"
    }
    fn generate(&self, _e: &Element) -> Result<Generated, String> {
        let (w, h) = (2000.0, 1000.0);
        let outline = vec![Pt2::new(0.0, 0.0), Pt2::new(w, 0.0), Pt2::new(w, h), Pt2::new(0.0, h)];
        Ok(Generated {
            drawing: Drawing {
                primitives: vec![Primitive::Fill { outline, colour: Colour::Green }],
                extents: Extents { min: Pt2::new(0.0, 0.0), max: Pt2::new(w, h) },
            },
            ..Default::default()
        })
    }
}

pub fn load_module() -> Result<(Box<dyn Module>, String), String> {
    Ok((Box::new(Stub), "stub".into()))
}

pub fn new_stack_data() -> Value {
    json!({})
}

pub fn edit_ui(_ui: &mut egui::Ui, _data: &mut Value) -> bool {
    false
}

pub fn demo_data() -> Vec<Value> {
    vec![json!({}), json!({})]
}
