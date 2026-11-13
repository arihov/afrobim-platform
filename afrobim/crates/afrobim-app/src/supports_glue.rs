//! Posts and foundations in the app: registering their modules, the generic definition editor,
//! and the hand-over to `afrobim-supports` for adding supports to a sign and measuring them.
//!
//! Supports are modelled from dimensions the engineer enters. Nothing here sizes a post or a
//! foundation: a new support starts with placeholder dimensions and says so until they are edited.

use afrobim_kernel::{Document, ElementId, ModuleRegistry};
use afrobim_supports::{FoundationDefinition, FoundationModule, PostDefinition, PostModule, QuantityOptions, SupportArrangement, SupportQuantities};
use serde_json::Value;

pub use afrobim_supports::{FOUNDATION_CATEGORY, POST_CATEGORY, PROP_MOUNTING_HEIGHT_MM, PROP_POSITION_ALONG_SIGN_MM, PROP_SETBACK_MM};

pub fn register(modules: &mut ModuleRegistry) {
    modules.register(Box::new(PostModule::default()));
    modules.register(Box::new(FoundationModule::default()));
}

pub fn is_support(category: &str) -> bool {
    category == POST_CATEGORY || category == FOUNDATION_CATEGORY
}

/// Adds posts and foundations to a sign, with placeholder dimensions for the engineer to set.
/// `mounting_height_mm` is the height of the sign's lower edge above ground, as entered.
pub fn add(doc: &mut Document, sign: ElementId, mounting_height_mm: f64, posts: u32, edge_inset_mm: f64) -> Result<usize, String> {
    let post = PostDefinition::placeholder();
    let mut arrangement = SupportArrangement::new(mounting_height_mm, posts, edge_inset_mm);
    // Stand the post behind the plate, touching it: its axis is half its depth behind the face.
    arrangement.setback_mm = post.section.bounding_mm().1 / 2.0;
    afrobim_supports::add_supports(doc, sign, &arrangement, &post, &FoundationDefinition::placeholder())
        .map(|ids| ids.len())
        .map_err(|e| e.to_string())
}

pub fn remove(doc: &mut Document, sign: ElementId) -> usize {
    afrobim_supports::remove_supports(doc, sign).len()
}

/// Post and foundation quantity CSVs for the whole document: (posts, foundations, summary), or
/// None when the document has no supports. Steel mass uses the nominal density, which is an
/// estimating convention and not a specification value.
pub fn quantities(doc: &Document) -> Option<Result<(String, String, String), String>> {
    if !doc.elements().iter().any(|e| is_support(&e.category)) {
        return None;
    }
    Some(
        SupportQuantities::measure(doc.elements().iter(), &QuantityOptions::default())
            .map(|q| (afrobim_supports::posts_to_csv(&q.posts), afrobim_supports::foundations_to_csv(&q.foundations), afrobim_supports::summary_to_csv(&q)))
            .map_err(|e| e.to_string()),
    )
}

fn label(key: &str) -> String {
    let text = key.trim_end_matches("_mm").replace('_', " ");
    let mut chars = text.chars();
    let first = chars.next().map(|c| c.to_uppercase().collect::<String>()).unwrap_or_default();
    format!("{first}{}{}", chars.as_str(), if key.ends_with("_mm") { " (mm)" } else { "" })
}

/// Edits a definition generically: every number, text and flag in it, nested groups indented.
/// The `type` tags that choose a section or shape are shown, not edited (changing a type needs
/// a different set of dimensions). Returns true when something changed.
fn edit_value(ui: &mut egui::Ui, path: &str, value: &mut Value) -> bool {
    let Some(map) = value.as_object_mut() else { return false };
    let mut changed = false;
    let keys: Vec<String> = map.keys().filter(|k| *k != "placeholder").cloned().collect();
    egui::Grid::new(format!("def-{path}")).num_columns(2).spacing([10.0, 6.0]).show(ui, |ui| {
        for key in &keys {
            let Some(v) = map.get_mut(key) else { continue };
            match v {
                Value::Number(n) => {
                    let mut x = n.as_f64().unwrap_or(0.0);
                    ui.label(label(key));
                    if ui.add(egui::DragValue::new(&mut x).speed(1.0)).changed() {
                        if let Some(n) = serde_json::Number::from_f64(x) {
                            *v = Value::Number(n);
                            changed = true;
                        }
                    }
                    ui.end_row();
                }
                Value::String(s) if key == "type" => {
                    ui.label("Type");
                    ui.label(s.as_str());
                    ui.end_row();
                }
                Value::String(s) => {
                    ui.label(label(key));
                    changed |= ui.add(egui::TextEdit::singleline(s).hint_text("not stated").desired_width(150.0)).changed();
                    ui.end_row();
                }
                Value::Bool(b) => {
                    ui.label(label(key));
                    changed |= ui.checkbox(b, "").changed();
                    ui.end_row();
                }
                _ => {}
            }
        }
    });
    for key in &keys {
        if let Some(v @ Value::Object(_)) = map.get_mut(key) {
            ui.add_space(4.0);
            ui.label(label(key));
            ui.indent(format!("{path}/{key}"), |ui| changed |= edit_value(ui, &format!("{path}/{key}"), v));
        }
    }
    changed
}

/// The post or foundation editor. Editing any value clears the placeholder flag: from then on
/// the dimensions are the engineer's.
pub fn edit_ui(ui: &mut egui::Ui, data: &mut Value) -> bool {
    let placeholder = data.get("placeholder").and_then(Value::as_bool).unwrap_or(false);
    if placeholder {
        ui.label(
            egui::RichText::new("Placeholder dimensions. They are not designed or checked: enter the engineer's values.")
                .color(egui::Color32::from_rgb(0xD9, 0x8A, 0x2B))
                .small(),
        );
    }
    let changed = edit_value(ui, "", data);
    if changed && placeholder {
        if let Some(map) = data.as_object_mut() {
            map.remove("placeholder");
        }
    }
    changed
}
