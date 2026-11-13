//! The traffic-signs module: wraps the sign engine as a kernel [`Module`] and provides the
//! definition editor shown in Properties.

use std::path::{Path, PathBuf};

use afrobim_draw::Colour;
use afrobim_kernel::{Element, Generated, Module, Note, Severity};
use afrobim_signs::{Destination, Direction, RoadClass, SignDefinition, SignEngine};
use serde_json::Value;

pub const CATEGORY: &str = "sign";
pub const LAYER: &str = "Signs";
const PACK_FILE: &str = "UK-TSM-2019.json";

struct SignsModule {
    engine: SignEngine,
}

impl Module for SignsModule {
    fn category(&self) -> &str {
        CATEGORY
    }

    fn display_name(&self) -> &str {
        "Traffic signs"
    }

    fn generate(&self, element: &Element) -> Result<Generated, String> {
        let mut def: SignDefinition =
            serde_json::from_value(element.data.clone()).map_err(|e| format!("The sign definition is not valid: {e}"))?;
        def.code = element.name.clone();
        let layout = self.engine.layout(&def).map_err(|e| e.to_string())?;

        // The engine works in stroke widths; the model is in millimetres.
        let drawing = layout.drawing.scaled_to_origin(layout.sw_mm);
        let (w, h) = (drawing.extents.width(), drawing.extents.height());
        let mut derived = std::collections::BTreeMap::new();
        derived.insert("x-height".to_string(), format!("{:.1} mm", layout.x_height_mm));
        derived.insert("Stroke width (sw)".to_string(), format!("{:.2} mm", layout.sw_mm));
        derived.insert("Overall size".to_string(), format!("{w:.0} × {h:.0} mm"));
        derived.insert("Bounding area".to_string(), format!("{:.3} m²", w * h / 1.0e6));

        let notes = layout
            .diagnostics
            .iter()
            .map(|d| Note {
                severity: match d.severity {
                    afrobim_signs::Severity::Error => Severity::Error,
                    afrobim_signs::Severity::Warning => Severity::Warning,
                    _ => Severity::Info,
                },
                code: d.code.clone(),
                message: d.message.clone(),
            })
            .collect();
        Ok(Generated { drawing, notes, derived })
    }
}

/// Looks for `relative` in the directories named by `env_var`, next to the executable and in
/// every folder above the executable and the working directory (so a source checkout works).
fn find(env_var: &str, relative: &Path) -> Option<PathBuf> {
    if let Ok(dir) = std::env::var(env_var) {
        let p = PathBuf::from(dir);
        if p.exists() {
            return Some(p);
        }
    }
    let mut roots: Vec<PathBuf> = Vec::new();
    if let Ok(exe) = std::env::current_exe() {
        roots.extend(exe.ancestors().skip(1).map(Path::to_path_buf));
    }
    if let Ok(cwd) = std::env::current_dir() {
        roots.extend(cwd.ancestors().map(Path::to_path_buf));
    }
    roots.into_iter().map(|r| r.join(relative)).find(|p| p.exists())
}

/// Loads the UK rule pack and the Transport fonts. Returns the module and a label for the status bar.
pub fn load_module() -> Result<(Box<dyn Module>, String), String> {
    let pack = find("AFROBIM_PACK", &Path::new("packs").join(PACK_FILE))
        .ok_or_else(|| format!("rule pack packs/{PACK_FILE} was not found (set AFROBIM_PACK to its path)"))?;
    let fonts = find("AFROBIM_FONTS", Path::new("fonts"))
        .ok_or_else(|| "the fonts folder was not found (set AFROBIM_FONTS to the folder holding the Transport fonts)".to_string())?;
    let engine = SignEngine::load(&pack, &fonts).map_err(|e| e.to_string())?;
    let label = format!("{} · {}", engine.pack_id(), engine.pack_name());
    Ok((Box::new(SignsModule { engine }), label))
}

fn to_value(def: &SignDefinition) -> Value {
    serde_json::to_value(def).expect("a sign definition always serialises")
}

pub fn new_stack_data() -> Value {
    to_value(&SignDefinition::stack("", 60, vec![Destination::new("New destination", Direction::Ahead)]))
}

pub fn demo_data() -> Vec<Value> {
    vec![
        to_value(&SignDefinition::stack(
            "",
            60,
            vec![Destination::new("Kasese", Direction::Ahead), Destination::new("Masheruka", Direction::Right)],
        )),
        to_value(&SignDefinition::stack(
            "",
            80,
            vec![
                Destination::new("Mbarara", Direction::Left).with_distance("42"),
                Destination::new("Kampala", Direction::Ahead).with_distance("270"),
                Destination::new("Fort Portal", Direction::Right),
            ],
        )),
    ]
}

fn optional_text(ui: &mut egui::Ui, value: &mut Option<String>, hint: &str, width: f32) -> bool {
    let mut text = value.clone().unwrap_or_default();
    let changed = ui.add(egui::TextEdit::singleline(&mut text).hint_text(hint).desired_width(width)).changed();
    if changed {
        *value = if text.trim().is_empty() { None } else { Some(text) };
    }
    changed
}

/// The sign definition editor. Returns true when the definition was changed.
pub fn edit_ui(ui: &mut egui::Ui, data: &mut Value) -> bool {
    let Ok(mut def) = serde_json::from_value::<SignDefinition>(data.clone()) else {
        ui.label("This element's definition cannot be read as a sign.");
        return false;
    };
    let mut changed = false;

    egui::Grid::new("sign-def").num_columns(2).spacing([10.0, 6.0]).show(ui, |ui| {
        ui.label("Type");
        ui.label(format!("{:?}", def.kind));
        ui.end_row();

        ui.label("Design speed");
        changed |= ui.add(egui::DragValue::new(&mut def.speed_kph).range(10..=130).suffix(" km/h")).changed();
        ui.end_row();

        ui.label("Approach class");
        egui::ComboBox::from_id_salt("approach").selected_text(format!("{:?}", def.approach_class)).show_ui(ui, |ui| {
            for c in [RoadClass::Primary, RoadClass::Secondary, RoadClass::Local] {
                changed |= ui.selectable_value(&mut def.approach_class, c, format!("{c:?}")).changed();
            }
        });
        ui.end_row();

        ui.label("Background");
        egui::ComboBox::from_id_salt("background").selected_text(format!("{:?}", def.background)).show_ui(ui, |ui| {
            for c in [Colour::Green, Colour::Blue, Colour::White, Colour::Brown, Colour::Yellow] {
                changed |= ui.selectable_value(&mut def.background, c, format!("{c:?}")).changed();
            }
        });
        ui.end_row();
    });

    ui.add_space(6.0);
    ui.label("Destinations");
    let mut remove = None;
    let mut swap = None;
    let count = def.destinations.len();
    for (i, d) in def.destinations.iter_mut().enumerate() {
        ui.push_id(i, |ui| {
            ui.horizontal(|ui| {
                changed |= ui.add(egui::TextEdit::singleline(&mut d.name).hint_text("Name").desired_width(120.0)).changed();
                egui::ComboBox::from_id_salt("dir").width(62.0).selected_text(format!("{:?}", d.direction)).show_ui(ui, |ui| {
                    for dir in [Direction::Ahead, Direction::Left, Direction::Right] {
                        changed |= ui.selectable_value(&mut d.direction, dir, format!("{dir:?}")).changed();
                    }
                });
                if ui.add_enabled(i > 0, egui::Button::new("Up").small()).on_hover_text("Move up").clicked() {
                    swap = Some((i, i - 1));
                }
                if ui.add_enabled(i + 1 < count, egui::Button::new("Dn").small()).on_hover_text("Move down").clicked() {
                    swap = Some((i, i + 1));
                }
                if ui.add_enabled(count > 1, egui::Button::new("x").small()).on_hover_text("Remove this destination").clicked() {
                    remove = Some(i);
                }
            });
            ui.horizontal(|ui| {
                ui.add_space(12.0);
                changed |= optional_text(ui, &mut d.distance, "Distance", 64.0);
                changed |= optional_text(ui, &mut d.route_number, "Route no.", 70.0);
            });
        });
    }
    if let Some((a, b)) = swap {
        def.destinations.swap(a, b);
        changed = true;
    }
    if let Some(i) = remove {
        def.destinations.remove(i);
        changed = true;
    }
    if ui.button("Add destination").clicked() {
        def.destinations.push(Destination::new("New destination", Direction::Ahead));
        changed = true;
    }

    if changed {
        *data = to_value(&def);
    }
    changed
}
