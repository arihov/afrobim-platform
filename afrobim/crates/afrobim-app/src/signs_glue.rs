// SPDX-License-Identifier: LicenseRef-AfroBIM-Proprietary
// Copyright (c) 2026 AfroBIM. All rights reserved.
//! The traffic-signs module: wraps the sign engine as a kernel [`Module`] and provides the
//! definition editor shown in Properties.

use std::path::{Path, PathBuf};
use std::sync::Arc;

use afrobim_draw::Colour;
use afrobim_kernel::{Element, Generated, Module, Note, Severity};
use afrobim_signs::{ColourScheme, Destination, EmbeddedSign, EmbeddedType, Direction, RoadClass, RouteStatus, SignDefinition, SignEngine, SignKind, SignVariant};

use crate::icons::Icon;
use serde_json::Value;

pub const CATEGORY: &str = "sign";
pub const LAYER: &str = "Signs";
/// The design standards the app can switch between: id, label, path under `packs/`.
pub const STANDARDS: [(&str, &str, &str); 2] = [
    ("UK-TSM-2019", "UK Traffic Signs Manual (Chapter 7, TSRGD 2016)", "UK-TSM-2019.json"),
    ("UG-TSM-2004", "Uganda Traffic Signs Manual (2004)", "UG-TSM-2004/pack.xml"),
];
pub const DEFAULT_STANDARD: &str = "UK-TSM-2019";

struct SignsModule {
    engine: Arc<SignEngine>,
}

/// A sign module over a loaded engine: a worker thread builds its own, as modules are not `Send`.
pub fn module_for(engine: Arc<SignEngine>) -> Box<dyn Module> {
    Box::new(SignsModule { engine })
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
        Ok(Generated { drawing, notes, derived, solids: Vec::new() })
    }
}

/// Looks for `relative`, in this order: the path named by the environment variable `env_var`;
/// next to the executable; on macOS the bundle's resource directory (`exe_dir/../Resources`);
/// then every folder above the executable and above the working directory (so a source
/// checkout works).
fn find(env_var: &str, relative: &Path) -> Option<PathBuf> {
    let exe = std::env::current_exe().ok();
    let cwd = std::env::current_dir().ok();
    find_in(std::env::var(env_var).ok().as_deref(), exe.as_deref(), cwd.as_deref(), cfg!(target_os = "macos"), relative)
}

/// [`find`] with its inputs given, so that each step can be tested.
fn find_in(env_value: Option<&str>, exe: Option<&Path>, cwd: Option<&Path>, bundle: bool, relative: &Path) -> Option<PathBuf> {
    if let Some(dir) = env_value {
        let p = PathBuf::from(dir);
        if p.exists() {
            return Some(p);
        }
    }
    if let Some(dir) = exe.and_then(Path::parent) {
        let p = dir.join(relative);
        if p.exists() {
            return Some(p);
        }
        if bundle {
            let p = dir.join("..").join("Resources").join(relative);
            if p.exists() {
                return Some(p);
            }
        }
    }
    let mut roots: Vec<PathBuf> = Vec::new();
    if let Some(exe) = exe {
        roots.extend(exe.ancestors().skip(1).map(Path::to_path_buf));
    }
    if let Some(cwd) = cwd {
        roots.extend(cwd.ancestors().map(Path::to_path_buf));
    }
    roots.into_iter().map(|r| r.join(relative)).find(|p| p.exists())
}

/// The Uganda pack's override (`AFROBIM_PACK_UG`) may name the folder or its `pack.xml`.
fn ug_override(pack: PathBuf) -> PathBuf {
    if pack.is_dir() { pack.join("pack.xml") } else { pack }
}

/// One entry of the standard-sign library: a catalogued sign ready to insert.
pub struct LibraryItem {
    pub key: String,
    pub name: String,
    /// Group heading, e.g. "Warning".
    pub class: String,
    pub data: Value,
}

/// A loaded standard: the kernel module, plus what exports need beyond it.
pub struct Loaded {
    pub module: Box<dyn Module>,
    /// Status-bar label, e.g. "UK-TSM-2019 · UK Traffic Signs Manual (Chapter 7)".
    pub label: String,
    pub library: Vec<LibraryItem>,
    pub engine: Arc<SignEngine>,
    pub fonts_dir: PathBuf,
}

/// Loads a rule pack and the Transport fonts. `AFROBIM_PACK` overrides the UK pack's location (the
/// JSON file) and `AFROBIM_PACK_UG` the Uganda pack's (its folder or its pack.xml).
pub fn load_module(standard: &str) -> Result<Loaded, String> {
    let (_, _, file) = STANDARDS.iter().find(|(id, _, _)| *id == standard).ok_or_else(|| format!("unknown standard {standard}"))?;
    let relative = Path::new("packs").join(file);
    let pack = if standard == DEFAULT_STANDARD {
        find("AFROBIM_PACK", &relative)
    } else {
        // The override names the folder or its pack.xml; otherwise the usual lookup finds the file.
        match std::env::var("AFROBIM_PACK_UG").ok().map(PathBuf::from).filter(|p| p.exists()) {
            Some(p) => Some(ug_override(p)),
            None => find("AFROBIM_PACK_UG", &relative),
        }
    }
    .ok_or_else(|| format!("rule pack packs/{file} was not found"))?;
    let fonts_dir = find("AFROBIM_FONTS", Path::new("fonts"))
        .ok_or_else(|| "the fonts folder was not found (set AFROBIM_FONTS to the folder holding the Transport fonts)".to_string())?;
    let engine = Arc::new(SignEngine::load(&pack, &fonts_dir).map_err(|e| e.to_string())?);
    let label = format!("{} · {}", engine.pack_id(), engine.pack_name());
    let library = engine
        .catalog()
        .iter()
        .map(|entry| LibraryItem {
            key: entry.key.clone(),
            name: entry.name.clone(),
            class: format!("{:?}", entry.class),
            data: to_value(&engine.definition_for(entry)),
        })
        .collect();
    Ok(Loaded { module: Box::new(SignsModule { engine: engine.clone() }), label, library, engine, fonts_dir })
}

/// The sign definitions of the given elements, each named by its mark. Elements of other
/// categories, or whose definition cannot be read, are left out.
pub fn definitions(elements: &[&Element]) -> Vec<SignDefinition> {
    elements
        .iter()
        .filter(|e| e.category == CATEGORY)
        .filter_map(|e| {
            let mut def: SignDefinition = serde_json::from_value(e.data.clone()).ok()?;
            def.code = e.name.clone();
            Some(def)
        })
        .collect()
}

/// The take-off row of each sign the engine can lay out; the cost estimate reads the same rows.
pub fn measure_rows(engine: &SignEngine, signs: &[SignDefinition]) -> Vec<afrobim_fabrication::SignQuantity> {
    signs.iter().filter_map(|s| engine.layout(s).ok().map(|layout| afrobim_fabrication::measure(engine, s, &layout))).collect()
}

/// Sign-face quantities: the per-sign CSV and the per-ground-colour summary CSV, plus the
/// number of signs measured and the total face area in m².
pub fn quantities(engine: &SignEngine, signs: &[SignDefinition]) -> (String, String, usize, f64) {
    let rows = measure_rows(engine, signs);
    let summary = afrobim_fabrication::summarise(&rows);
    let area: f64 = rows.iter().map(|r| r.width_mm * r.height_mm / 1.0e6).sum();
    (afrobim_fabrication::to_csv(&rows), afrobim_fabrication::summary_to_csv(&summary), rows.len(), area)
}

/// Writes an A3 fabrication sheet per sign (DXF and PDF) and the schedule PDF into `out_dir`.
/// Returns a one-line summary, or the reason nothing could be written.
pub fn fabrication_sheets(engine: &SignEngine, fonts_dir: &Path, signs: Vec<SignDefinition>, out_dir: &Path, drawn_by: &str) -> Result<String, String> {
    let fonts = afrobim_fabrication::PdfFonts::from_folder(fonts_dir, engine).map_err(|e| e.to_string())?;
    let mut options = afrobim_fabrication::FabricationOptions::new(out_dir);
    options.sheet.drawn_by = drawn_by.to_string();
    let sheet_signs: Vec<afrobim_fabrication::SheetSign> = signs.into_iter().map(afrobim_fabrication::SheetSign::new).collect();
    let result = afrobim_fabrication::run(engine, &fonts, &sheet_signs, &options);
    if result.files_written.is_empty() {
        return Err(result.errors.join("; "));
    }
    let mut summary = format!("Wrote {} file(s) for {} sign(s) to {}.", result.files_written.len(), result.rows.len(), out_dir.display());
    if result.error_count > 0 {
        summary.push_str(&format!(" {} layout error(s): check before cutting.", result.error_count));
    }
    Ok(summary)
}

fn to_value(def: &SignDefinition) -> Value {
    serde_json::to_value(def).expect("a sign definition always serialises")
}

/// A kind of sign the user can insert: palette entry, command name and starting definition.
pub struct Template {
    pub label: &'static str,
    pub command: &'static str,
    pub tip: &'static str,
    pub icon: Icon,
    /// Mark given to a new sign of this kind; made unique in the document.
    pub mark: &'static str,
    pub data: Value,
}

fn with(kind: SignKind, speed: u32, background: Colour, scheme: ColourScheme, key: Option<&str>, dests: Vec<Destination>) -> SignDefinition {
    SignDefinition {
        kind,
        background,
        scheme,
        catalog_key: key.map(str::to_string),
        ..SignDefinition::stack("", speed, dests)
    }
}

/// The insertable sign kinds, with the starting values the C# SignCore catalog gives each.
pub fn templates(standard: &str) -> Vec<Template> {
    // Catalogue keys and the explicit primary scheme below are UK entries; another standard
    // resolves its own scheme from the background.
    let uk = standard == "UK-TSM-2019";
    use Direction::{Ahead, Left, Right};
    let t = |label, command, tip, icon, mark, mut def: SignDefinition| {
        if !uk {
            def.catalog_key = None;
            if def.scheme == ColourScheme::Primary {
                def.scheme = ColourScheme::Auto;
            }
        }
        Template { label, command, tip, icon, mark, data: to_value(&def) }
    };
    vec![
        t(
            "Stack-type direction",
            "STACK",
            "Advance direction sign, one panel per direction (TSM Ch. 7 §4)",
            Icon::Stack,
            "ADS-001",
            SignDefinition::stack("", 60, vec![Destination::new("New destination", Ahead)]),
        ),
        t(
            "Direction",
            "DIRECTION",
            "Rectangular direction sign at the junction (TSM Ch. 7 §7.11)",
            Icon::Direction,
            "DS-001",
            with(
                SignKind::DirectionSign,
                60,
                Colour::Green,
                ColourScheme::Primary,
                Some("UK06.1"),
                vec![Destination::new("Kasese", Ahead), Destination::new("Masheruka", Right)],
            ),
        ),
        t(
            "Route confirmatory",
            "CONFIRM",
            "Route confirmatory sign with distances (TSM Ch. 7 §8)",
            Icon::Confirmatory,
            "RC-001",
            with(
                SignKind::Confirmatory,
                100,
                Colour::Green,
                ColourScheme::Primary,
                Some("UK07.1"),
                vec![Destination::new("Kampala", Ahead).with_distance("40 km")],
            ),
        ),
        t("Place identification", "PLACE", "Place name sign, entry or exit", Icon::Supplementary, "PL-001", {
            let mut d = with(SignKind::PlaceIdentification, 100, Colour::White, ColourScheme::Auto, None, vec![Destination::new("Kampala", Ahead)]);
            d.variant = SignVariant::Primary; // entry; Secondary draws the exit band
            d
        }),
        t(
            "Lane pre-selection",
            "LANE",
            "One arrow and name per lane",
            Icon::Lane,
            "LP-001",
            with(
                SignKind::LanePreselection,
                80,
                Colour::Green,
                ColourScheme::Auto,
                None,
                vec![
                    Destination::new("Left", Left).with_order(0),
                    Destination::new("Ahead", Ahead).with_order(1),
                    Destination::new("Right", Right).with_order(2),
                ],
            ),
        ),
        t(
            "Diversion (temporary)",
            "DIVERSION",
            "Temporary diversion sign, black on yellow",
            Icon::Warning,
            "DV-001",
            with(
                SignKind::Diversion,
                60,
                Colour::Yellow,
                ColourScheme::Temporary,
                None,
                vec![Destination::new("Forward route", Ahead), Destination::new("Diversion route", Right)],
            ),
        ),
        t(
            "Map-type crossroads",
            "MAP",
            "Map-type advance direction sign for a crossroads (TSM Ch. 7 §5, Figure 5-7)",
            Icon::Map,
            "ADS-001",
            with(
                SignKind::MapTypeCrossroads,
                60,
                Colour::Green,
                ColourScheme::Primary,
                Some("UK02.1"),
                vec![Destination::new("Left", Left), Destination::new("Ahead", Ahead), Destination::new("Right", Right)],
            ),
        ),
        t(
            "Map-type T-junction",
            "TEE",
            "Map-type advance direction sign for a T-junction (TSM Ch. 7 §5)",
            Icon::Tee,
            "ADS-001",
            // A T-junction has no ahead arm: an ahead destination is laid out but flagged.
            with(
                SignKind::MapTypeTee,
                60,
                Colour::Green,
                ColourScheme::Primary,
                Some("UK03.1"),
                vec![Destination::new("Left", Left), Destination::new("Right", Right)],
            ),
        ),
        t(
            "Map-type roundabout",
            "ROUNDABOUT",
            "Map-type advance direction sign for a roundabout (TSM Ch. 7 §5, Figure 5-13)",
            Icon::Roundabout,
            "ADS-001",
            with(
                SignKind::MapTypeRoundabout,
                50,
                Colour::Green,
                ColourScheme::Primary,
                Some("UK01.1"),
                vec![Destination::new("Left", Left), Destination::new("Ahead", Ahead), Destination::new("Right", Right)],
            ),
        ),
    ]
}

/// Sample content for `--demo`: one sign of every insertable kind.
pub fn demo_data(standard: &str) -> Vec<(String, Value)> {
    let mut out: Vec<(String, Value)> = vec![(
        "ADS-001".to_string(),
        to_value(&SignDefinition::stack(
            "",
            80,
            vec![
                Destination::new("Mbarara", Direction::Left).with_distance("42"),
                Destination::new("Kampala", Direction::Ahead).with_distance("270"),
                Destination::new("Fort Portal", Direction::Right),
            ],
        )),
    )];
    out.extend(templates(standard).into_iter().skip(1).map(|t| (t.mark.to_string(), t.data)));
    out
}

fn route_label(r: RouteStatus) -> &'static str {
    match r {
        RouteStatus::Unspecified => "Route: as sign",
        RouteStatus::Motorway => "Motorway",
        RouteStatus::Primary => "Primary",
        RouteStatus::NonPrimary => "Non-primary",
    }
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

    crate::ui_kit::grid(ui, "sign-def", |ui| {
        ui.label("Type");
        ui.label(format!("{:?}", def.kind));
        ui.end_row();

        if def.kind == SignKind::PlaceIdentification {
            ui.label("Variant");
            egui::ComboBox::from_id_salt("variant")
                .selected_text(if def.variant == SignVariant::Primary { "Entry" } else { "Exit" })
                .show_ui(ui, |ui| {
                    changed |= ui.selectable_value(&mut def.variant, SignVariant::Primary, "Entry").changed();
                    changed |= ui.selectable_value(&mut def.variant, SignVariant::Secondary, "Exit").changed();
                });
            ui.end_row();
        }

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

    let has_destinations = !matches!(
        def.kind,
        SignKind::WarningSign
            | SignKind::RegulatoryProhibitory
            | SignKind::RegulatoryMandatory
            | SignKind::Stop
            | SignKind::GiveWay
            | SignKind::SupplementaryPlate
            | SignKind::InformationSign
    );
    if !has_destinations {
        // Standard signs are defined by a diagram or symbol and an optional legend.
        crate::ui_kit::grid(ui, "sign-std", |ui| {
            ui.label(if def.kind == SignKind::SupplementaryPlate { "Plate type" } else { "Diagram / symbol" });
            changed |= optional_text(ui, &mut def.symbol_id, "e.g. 504.1 or 670:30", 150.0);
            ui.end_row();
            ui.label(if def.kind == SignKind::SupplementaryPlate { "Legend" } else { "Plate legend" });
            changed |= optional_text(ui, &mut def.info_distance, "lines separated by |", 150.0);
            ui.end_row();
            ui.label("Temporary");
            let mut temporary = def.scheme == ColourScheme::Temporary;
            if ui.checkbox(&mut temporary, "yellow ground").changed() {
                def.scheme = if temporary { ColourScheme::Temporary } else { ColourScheme::Auto };
                changed = true;
            }
            ui.end_row();
        });
        if changed {
            *data = to_value(&def);
        }
        return changed;
    }

    // Warning triangles and regulatory roundels carried on the sign (drawn as the standard
    // frame; the label is a note for the schedule and does not change the geometry).
    if matches!(def.kind, SignKind::DirectionSign | SignKind::MapTypeCrossroads | SignKind::MapTypeTee | SignKind::MapTypeRoundabout) {
        const MAX_EMBEDDED: usize = 2; // the engine sets no limit; more than two overlap on most signs
        ui.add_space(6.0);
        ui.label("Embedded signs");
        let mut list = def.embedded.clone().unwrap_or_default();
        let mut remove = None;
        for (i, e) in list.iter_mut().enumerate() {
            ui.push_id(("embedded", i), |ui| {
                ui.horizontal(|ui| {
                    let label = |t: EmbeddedType| if t == EmbeddedType::WarningTriangle { "Warning triangle" } else { "Regulatory roundel" };
                    egui::ComboBox::from_id_salt("type").width(130.0).selected_text(label(e.kind)).show_ui(ui, |ui| {
                        for t in [EmbeddedType::WarningTriangle, EmbeddedType::RegulatoryRoundel] {
                            changed |= ui.selectable_value(&mut e.kind, t, label(t)).changed();
                        }
                    });
                    changed |= ui.add(egui::TextEdit::singleline(&mut e.symbol_id).hint_text("Label").desired_width(96.0)).changed();
                    if crate::ui_kit::icon_button(ui, crate::glyphs::ui::TRASH, "Remove this embedded sign").clicked() {
                        remove = Some(i);
                    }
                });
            });
        }
        if let Some(i) = remove {
            list.remove(i);
            changed = true;
        }
        if ui.add_enabled(list.len() < MAX_EMBEDDED, egui::Button::new("Add embedded sign")).clicked() {
            list.push(EmbeddedSign { symbol_id: String::new(), kind: EmbeddedType::WarningTriangle });
            changed = true;
        }
        def.embedded = if list.is_empty() { None } else { Some(list) };
    }

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
                if ui.add_enabled_ui(i > 0, |ui| crate::ui_kit::icon_button(ui, crate::glyphs::ui::ARROW_UP, "Move up")).inner.clicked() {
                    swap = Some((i, i - 1));
                }
                if ui.add_enabled_ui(i + 1 < count, |ui| crate::ui_kit::icon_button(ui, crate::glyphs::ui::ARROW_DOWN, "Move down")).inner.clicked() {
                    swap = Some((i, i + 1));
                }
                if ui.add_enabled_ui(count > 1, |ui| crate::ui_kit::icon_button(ui, crate::glyphs::ui::TRASH, "Remove this destination")).inner.clicked() {
                    remove = Some(i);
                }
            });
            ui.horizontal(|ui| {
                ui.add_space(12.0);
                changed |= optional_text(ui, &mut d.distance, "Distance", 64.0);
                changed |= optional_text(ui, &mut d.route_number, "Route no.", 70.0);
                egui::ComboBox::from_id_salt("route").width(86.0).selected_text(route_label(d.route)).show_ui(ui, |ui| {
                    for r in [RouteStatus::Unspecified, RouteStatus::Motorway, RouteStatus::Primary, RouteStatus::NonPrimary] {
                        changed |= ui.selectable_value(&mut d.route, r, route_label(r)).changed();
                    }
                });
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

#[cfg(test)]
mod lookup_tests {
    use super::*;

    fn tmp(name: &str) -> PathBuf {
        let d = std::env::temp_dir().join(format!("afrobim-find-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&d);
        std::fs::create_dir_all(&d).unwrap();
        d
    }

    fn touch(p: &Path) {
        std::fs::create_dir_all(p.parent().unwrap()).unwrap();
        std::fs::write(p, "x").unwrap();
    }

    #[test]
    fn the_lookup_tries_the_variable_then_the_exe_folder_then_the_bundle_then_the_ancestors() {
        let root = tmp("order");
        let other = tmp("cwd");
        let rel = Path::new("packs/p.json");
        let exe = root.join("app/bin/afrobim");
        std::fs::create_dir_all(root.join("app/bin")).unwrap();
        // Nothing anywhere.
        assert_eq!(find_in(None, Some(&exe), Some(&other), true, rel), None);
        // The folders above the executable (a source checkout).
        touch(&root.join("app/packs/p.json"));
        assert_eq!(find_in(None, Some(&exe), None, true, rel), Some(root.join("app/packs/p.json")));
        // The working directory's ancestors.
        std::fs::remove_file(root.join("app/packs/p.json")).unwrap();
        touch(&other.join("packs/p.json"));
        assert_eq!(find_in(None, Some(&exe), Some(&other.join("sub")), true, rel), Some(other.join("packs/p.json")));
        // The bundle resource directory beats the ancestors, on macOS only.
        touch(&root.join("app/Resources/packs/p.json"));
        let found = find_in(None, Some(&exe), Some(&other), true, rel).unwrap();
        assert!(found.ends_with("Resources/packs/p.json"), "{found:?}");
        assert_eq!(find_in(None, Some(&exe), Some(&other), false, rel), Some(other.join("packs/p.json")));
        // Next to the executable beats the bundle directory.
        touch(&root.join("app/bin/packs/p.json"));
        assert_eq!(find_in(None, Some(&exe), Some(&other), true, rel), Some(root.join("app/bin/packs/p.json")));
        // The environment variable beats everything; a path that does not exist falls through.
        let env_file = other.join("env.json");
        touch(&env_file);
        assert_eq!(find_in(env_file.to_str(), Some(&exe), Some(&other), true, rel), Some(env_file.clone()));
        assert_eq!(find_in(Some("/no/such/path"), Some(&exe), None, true, rel), Some(root.join("app/bin/packs/p.json")));
        let _ = std::fs::remove_dir_all(&root);
        let _ = std::fs::remove_dir_all(&other);
    }

    #[test]
    fn the_uganda_override_accepts_the_folder_or_the_file() {
        let d = tmp("ug");
        touch(&d.join("pack.xml"));
        assert_eq!(ug_override(d.clone()), d.join("pack.xml"));
        assert_eq!(ug_override(d.join("pack.xml")), d.join("pack.xml"));
        let _ = std::fs::remove_dir_all(&d);
    }
}
