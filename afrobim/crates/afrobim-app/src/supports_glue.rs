// SPDX-License-Identifier: LicenseRef-AfroBIM-Proprietary
// Copyright (c) 2026 AfroBIM. All rights reserved.
//! Posts and foundations in the app: registering their modules, the generic definition editor,
//! and the hand-over to `afrobim-supports` for adding supports to a sign and measuring them.
//!
//! `add` gives a sign supports with placeholder dimensions for the engineer to set. `design`
//! sizes steel CHS posts and planted foundations for wind to the UK method; see
//! `afrobim_supports::design` for what that covers and what it leaves to the engineer.

use afrobim_kernel::{Document, ElementId, ModuleRegistry};
use afrobim_supports::{design, FoundationDefinition, FoundationModule, PostDefinition, PostModule, QuantityOptions, SupportArrangement, SupportQuantities};
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

/// Where the basic wind pressure comes from.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum WindChoice {
    /// MoWT Road Design Manual Vol. 4 (2010), Table 2, open terrain (categories 1 and 2).
    MowtOpen,
    /// The same table, built-up areas (categories 3 and 4).
    MowtBuiltUp,
    /// A pressure the engineer enters, kN/m².
    Entered(f64),
}

/// What the "Design supports" dialog collects. Every ground value is the engineer's.
#[derive(Debug, Clone, PartialEq)]
pub struct DesignRequest {
    pub mounting_height_mm: f64,
    pub posts: u32,
    pub edge_inset_mm: f64,
    pub wind: WindChoice,
    pub ground_factor: f64,
    /// True for one spread foundation under the whole sign; false for a planted one per post.
    pub spread: bool,
    /// True designs to CD 354 (post diameter only); false to PD 6547 (concrete surround).
    pub cd354: bool,
    pub foundation_diameter_mm: f64,
    pub min_planting_depth_mm: f64,
    /// Spread foundation: assumed ground, breadth parallel to the face, thickness, and the
    /// depth of its top below ground.
    pub spread_ground: design::AssumedGround,
    pub spread_breadth_mm: f64,
    pub spread_thickness_mm: f64,
    pub spread_cover_mm: f64,
}

impl DesignRequest {
    /// Starting values for the dialog. The ground factor is the "poor" value and the
    /// foundation diameter and minimum depth are those of the IHE guide's Example 1; they are
    /// starting points to be confirmed, not recommendations.
    pub fn starting(mounting_height_mm: f64, posts: u32) -> Self {
        DesignRequest {
            mounting_height_mm,
            posts,
            edge_inset_mm: 150.0,
            wind: WindChoice::MowtOpen,
            ground_factor: design::GROUND_FACTOR_POOR_KN_M2_PER_M,
            cd354: false,
            foundation_diameter_mm: 400.0,
            min_planting_depth_mm: 800.0,
            spread: false,
            spread_ground: design::AssumedGround::Poor,
            spread_breadth_mm: 1000.0,
            spread_thickness_mm: 750.0,
            spread_cover_mm: 75.0,
        }
    }
}

/// The mounting height and post count of the sign's present supports, if it has any.
pub fn present(doc: &Document, sign: ElementId) -> Option<(f64, u32)> {
    let ids = afrobim_supports::supports_of(doc, sign);
    let posts: Vec<_> = ids.iter().filter_map(|id| doc.find(*id)).filter(|e| e.category == POST_CATEGORY).collect();
    let height = posts.first()?.properties.get(PROP_MOUNTING_HEIGHT_MM)?.parse().ok()?;
    Some((height, posts.len() as u32))
}

/// Why a support design gave no result: an entry it needs is missing, or the method finds no
/// design for the sign.
#[derive(Debug, Clone, PartialEq)]
pub enum DesignFailure {
    /// An input is missing or outside what the source covers (for example the sign is above the
    /// 30 m of the MoWT wind table).
    Input(String),
    /// The inputs are complete and no section or foundation of the method carries the sign.
    NoDesign(String),
}

impl std::fmt::Display for DesignFailure {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            DesignFailure::Input(m) | DesignFailure::NoDesign(m) => f.write_str(m),
        }
    }
}

/// A support design worked out for a sign, with where its wind pressure came from.
pub struct Solved {
    pub result: design::SupportDesign,
    pub wind_source: String,
    pub wind_kn_m2: f64,
}

/// Works out the support design of a sign for the entries, without touching the document.
pub fn solve(doc: &Document, sign: ElementId, req: &DesignRequest) -> Result<Solved, DesignFailure> {
    let e = doc.find(sign).ok_or_else(|| DesignFailure::Input("the sign is not in the document".into()))?;
    let mut geometry = design::SignGeometry {
        width_mm: e.width,
        height_mm: e.height,
        mounting_height_mm: req.mounting_height_mm,
        face_area_m2: None,
        buried_above_foundation_mm: 0.0,
        post_count: req.posts,
    };
    let height = geometry.total_height_m();
    let table = |terrain, name: &str| {
        design::mowt_velocity_pressure(height, terrain)
            .map(|wb| (wb, format!("MoWT Road Design Manual Vol. 4 (2010) Table 2, {name}, height to top {height:.2} m")))
            .ok_or_else(|| DesignFailure::Input(format!("MoWT Table 2 stops at 30 m and the sign's top is at {height:.2} m")))
    };
    let (wb, source) = match req.wind {
        WindChoice::MowtOpen => table(design::MowtTerrain::Open, "open terrain")?,
        WindChoice::MowtBuiltUp => table(design::MowtTerrain::BuiltUp, "built-up area")?,
        WindChoice::Entered(wb) => (wb, "entered by the engineer".to_string()),
    };
    geometry.buried_above_foundation_mm = if req.spread { req.spread_cover_mm } else { 0.0 };
    let request = if req.spread {
        design::FoundationRequest::Spread { breadth_mm: req.spread_breadth_mm, thickness_mm: req.spread_thickness_mm, ground: req.spread_ground }
    } else {
        design::FoundationRequest::Planted(design::PlantedGround {
            method: if req.cd354 { design::PlantedMethod::Cd354 } else { design::PlantedMethod::Pd6547 { foundation_diameter_mm: req.foundation_diameter_mm } },
            ground_factor_kn_m2_per_m: req.ground_factor,
            min_planting_depth_mm: req.min_planting_depth_mm,
        })
    };
    let result = design::design_supports(&geometry, wb, &design::COMMON_CHS_MM, design::S355_YIELD_N_MM2, &request, &design::DesignFactors::default()).map_err(|e| match e {
        afrobim_supports::SupportError::InvalidOption(m) if m.starts_with("no ") => DesignFailure::NoDesign(m),
        other => DesignFailure::Input(other.to_string()),
    })?;
    Ok(Solved { result, wind_source: source, wind_kn_m2: wb })
}

/// Sizes steel CHS posts and planted foundations for the sign to the UK method (IHE Sign
/// Structures Guide 2021, BS EN 12899-1) and replaces the sign's supports with them. Returns
/// the calculation, line by line. The face is taken as its full bounding rectangle. The entries
/// are kept on the sign (`check.support_design`) so the model-wide check can repeat them.
pub fn design(doc: &mut Document, sign: ElementId, req: &DesignRequest) -> Result<Vec<String>, String> {
    let Solved { result, wind_source: source, wind_kn_m2: wb } = solve(doc, sign, req).map_err(|e| e.to_string())?;

    let section = result.post.section;
    let mut arrangement = SupportArrangement::new(req.mounting_height_mm, req.posts, req.edge_inset_mm);
    arrangement.setback_mm = section.outside_diameter_mm / 2.0;
    // Planted: the post goes to the bottom of a concrete surround, modelled in both methods
    // (CD 354 only leaves it out of Mg). Spread: the post is shown stopping at the top of the
    // block; how it is fixed into the block is not designed.
    let (embedment, shape, top, ground_text) = match &result.foundation {
        design::FoundationCheck::Planted(f) => (
            f.planting_depth_mm,
            afrobim_supports::FoundationShape::Cylinder { diameter_mm: req.foundation_diameter_mm.max(section.outside_diameter_mm), depth_mm: f.planting_depth_mm },
            0.0,
            format!("G = {} kN/m² per m", req.ground_factor),
        ),
        design::FoundationCheck::Spread(f) => {
            arrangement.shared_foundation = true;
            (
                req.spread_cover_mm,
                afrobim_supports::FoundationShape::Block { length_mm: f.breadth_mm, width_mm: f.length_mm, depth_mm: f.thickness_mm },
                req.spread_cover_mm,
                format!("{} ground assumed (Table 5.2)", f.ground.label()),
            )
        }
    };
    let post = PostDefinition {
        section: afrobim_supports::Section::Chs { outside_diameter_mm: section.outside_diameter_mm, wall_mm: section.wall_mm },
        length_mm: 0.0, // set from the arrangement when the supports are added
        embedment_mm: embedment,
        material: "S355".into(),
        finish: String::new(),
        placeholder: false,
    };
    let foundation = FoundationDefinition { shape, top_below_ground_mm: top, concrete: String::new(), blinding_mm: None, socket: None, placeholder: false };
    let ids = afrobim_supports::replace_supports(doc, sign, &arrangement, &post, &foundation).map_err(|e| e.to_string())?;
    let basis = format!("IHE Sign Structures Guide 2021; wb = {wb} kN/m² ({source}); {ground_text}");
    for id in ids {
        if let Some(e) = doc.find_mut(id) {
            e.properties.insert("design_basis".into(), basis.clone());
        }
    }
    let lines = result.calculation(&source);
    crate::check_inputs::save(doc, sign, crate::check_inputs::KEY_SUPPORT, &StoredSupportDesign::from(req));
    Ok(lines)
}

/// The DESIGN dialog's entries as kept on the sign (`check.support_design`). The foreign
/// enumeration of the assumed ground is kept by name.
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct StoredSupportDesign {
    pub mounting_height_mm: f64,
    pub posts: u32,
    pub edge_inset_mm: f64,
    /// `mowt_open`, `mowt_built_up` or `entered`.
    pub wind: String,
    /// kN/m², when `wind` is `entered`.
    pub entered_wind_kn_m2: f64,
    pub ground_factor: f64,
    pub spread: bool,
    pub cd354: bool,
    pub foundation_diameter_mm: f64,
    pub min_planting_depth_mm: f64,
    /// `Poor`, `Average` or `Good`.
    pub spread_ground: String,
    pub spread_breadth_mm: f64,
    pub spread_thickness_mm: f64,
    pub spread_cover_mm: f64,
}

impl From<&DesignRequest> for StoredSupportDesign {
    fn from(r: &DesignRequest) -> Self {
        let (wind, entered) = match r.wind {
            WindChoice::MowtOpen => ("mowt_open", 0.0),
            WindChoice::MowtBuiltUp => ("mowt_built_up", 0.0),
            WindChoice::Entered(v) => ("entered", v),
        };
        StoredSupportDesign {
            mounting_height_mm: r.mounting_height_mm,
            posts: r.posts,
            edge_inset_mm: r.edge_inset_mm,
            wind: wind.to_string(),
            entered_wind_kn_m2: entered,
            ground_factor: r.ground_factor,
            spread: r.spread,
            cd354: r.cd354,
            foundation_diameter_mm: r.foundation_diameter_mm,
            min_planting_depth_mm: r.min_planting_depth_mm,
            spread_ground: format!("{:?}", r.spread_ground),
            spread_breadth_mm: r.spread_breadth_mm,
            spread_thickness_mm: r.spread_thickness_mm,
            spread_cover_mm: r.spread_cover_mm,
        }
    }
}

impl StoredSupportDesign {
    /// The request these entries make, or why they cannot be read.
    pub fn request(&self) -> Result<DesignRequest, String> {
        use design::AssumedGround::*;
        let wind = match self.wind.as_str() {
            "mowt_open" => WindChoice::MowtOpen,
            "mowt_built_up" => WindChoice::MowtBuiltUp,
            "entered" => WindChoice::Entered(self.entered_wind_kn_m2),
            other => return Err(format!("the stored wind basis '{other}' is not known")),
        };
        let spread_ground = [Poor, Average, Good].into_iter().find(|g| format!("{g:?}") == self.spread_ground).ok_or_else(|| format!("the stored ground '{}' is not known", self.spread_ground))?;
        Ok(DesignRequest {
            mounting_height_mm: self.mounting_height_mm,
            posts: self.posts,
            edge_inset_mm: self.edge_inset_mm,
            wind,
            ground_factor: self.ground_factor,
            spread: self.spread,
            cd354: self.cd354,
            foundation_diameter_mm: self.foundation_diameter_mm,
            min_planting_depth_mm: self.min_planting_depth_mm,
            spread_ground,
            spread_breadth_mm: self.spread_breadth_mm,
            spread_thickness_mm: self.spread_thickness_mm,
            spread_cover_mm: self.spread_cover_mm,
        })
    }
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
    let keys: Vec<String> = map.keys().filter(|k| !crate::modules_glue::HIDDEN_KEYS.contains(&k.as_str())).cloned().collect();
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
                Value::String(s) if crate::modules_glue::READ_ONLY_KEYS.contains(&key.as_str()) => {
                    ui.label(label(key));
                    ui.label(s.as_str());
                    ui.end_row();
                }
                Value::String(s) if crate::modules_glue::choices(key).is_some() => {
                    ui.label(label(key));
                    let options = crate::modules_glue::choices(key).unwrap_or_default();
                    egui::ComboBox::from_id_salt(format!("choice-{path}-{key}")).selected_text(s.clone()).show_ui(ui, |ui| {
                        for option in &options {
                            if ui.selectable_label(s == option, option).clicked() && s != option {
                                *s = option.clone();
                                changed = true;
                            }
                        }
                    });
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
        match map.get_mut(key) {
            Some(v @ Value::Object(_)) => {
                ui.add_space(4.0);
                ui.label(label(key));
                ui.indent(format!("{path}/{key}"), |ui| changed |= edit_value(ui, &format!("{path}/{key}"), v));
            }
            // A list of groups (legs, carriageways, lanes, layers): each item in turn.
            Some(Value::Array(items)) if items.iter().all(Value::is_object) && !items.is_empty() => {
                for (i, item) in items.iter_mut().enumerate() {
                    ui.add_space(4.0);
                    ui.label(format!("{} {}", label(key).trim_end_matches('s'), i + 1));
                    ui.indent(format!("{path}/{key}/{i}"), |ui| changed |= edit_value(ui, &format!("{path}/{key}/{i}"), item));
                }
            }
            _ => {}
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
                .color(crate::theme::warn())
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

#[cfg(test)]
mod tests {
    use super::*;
    use afrobim_kernel::Pt;

    #[test]
    fn design_replaces_placeholders_with_sized_supports() {
        let mut doc = Document::default();
        let sign = doc.add("sign", "S1", Pt::new(0.0, 0.0), 900.0, 900.0, "Signs", serde_json::json!({}));
        add(&mut doc, sign, 2000.0, 1, 150.0).unwrap();
        assert_eq!(present(&doc, sign), Some((2000.0, 1)));
        let mut req = DesignRequest::starting(2000.0, 1);
        req.wind = WindChoice::Entered(1.0);
        let lines = design(&mut doc, sign, &req).unwrap();
        assert!(lines.iter().any(|l| l.contains("CHS 76.1 × 3.2")), "{lines:#?}");
        assert!(!lines.iter().any(|l| l.contains("FAILS")));
        let ids = afrobim_supports::supports_of(&doc, sign);
        assert_eq!(ids.len(), 2);
        for id in ids {
            let e = doc.find(id).unwrap();
            assert!(e.data.get("placeholder").is_none());
            assert!(e.properties["design_basis"].contains("entered by the engineer"));
        }
        // A spread foundation: one block under the whole sign.
        req.spread = true;
        let lines = design(&mut doc, sign, &req).unwrap();
        assert!(lines.iter().any(|l| l.starts_with("Spread foundation")) && !lines.iter().any(|l| l.contains("FAILS")), "{lines:#?}");
        let ids = afrobim_supports::supports_of(&doc, sign);
        let blocks: Vec<_> = ids.iter().filter_map(|id| doc.find(*id)).filter(|e| e.category == FOUNDATION_CATEGORY).collect();
        assert_eq!(blocks.len(), 1);
        assert_eq!(blocks[0].data["shape"]["type"], "Block");
        req.spread = false;
        // MoWT open terrain, 5 m row.
        req.wind = WindChoice::MowtOpen;
        let lines = design(&mut doc, sign, &req).unwrap();
        assert!(lines.iter().any(|l| l.contains("wb = 0.81 kN/m²") && l.contains("Table 2")), "{lines:#?}");
    }
}
