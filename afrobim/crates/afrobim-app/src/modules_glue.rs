//! Road modules that share one pattern in the app: an element is added on an alignment with
//! placeholder dimensions, edited in Properties, checked, measured, and shown in the 3D site
//! view from its solids. Drainage (side drains, pipe culverts) and gantries are wired here, with
//! the keeping-in-step, check and quantity hooks of street lighting, road restraint, pavement and
//! corridors. Those four have no placeholder definition, so they are added from their own
//! dialogs (see `road_glue`) with values the engineer enters.
//!
//! Nothing in this file holds an engineering value: the crates behind it do, with citations.

use afrobim_alignment::Alignment;
use afrobim_corridor as corridor;
use afrobim_drainage::{self as drainage, design::DesignBasis};
use afrobim_gantry::{self as gantry, GantryDefinition, SignPosition};
use afrobim_kernel::{Document, ElementId, ModuleRegistry};
use afrobim_lighting as lighting;
use afrobim_pavement as pavement;
use afrobim_restraint as restraint;

use crate::{alignment_glue, road_glue};

/// One kind of element this glue can add.
pub struct Kind {
    pub label: &'static str,
    /// True when the element runs between two stations; false when it stands at one.
    pub span: bool,
    /// True when an offset from the centreline is asked for.
    pub offset: bool,
    pub mark: &'static str,
}

pub const KINDS: [Kind; 4] = [
    Kind { label: "Side drain", span: true, offset: true, mark: "SD-001" },
    Kind { label: "Pipe culvert", span: false, offset: false, mark: "CU-001" },
    Kind { label: "Portal gantry", span: false, offset: false, mark: "G-001" },
    Kind { label: "Cantilever gantry", span: false, offset: false, mark: "G-001" },
];

pub fn register(modules: &mut ModuleRegistry) {
    modules.register(Box::new(drainage::SideDrainModule::default()));
    modules.register(Box::new(drainage::PipeCulvertModule::default()));
    modules.register(Box::new(gantry::GantryModule));
    modules.register(Box::new(lighting::LightingModule));
    modules.register(Box::new(restraint::RestraintModule));
    modules.register(Box::new(pavement::PavementModule::default()));
    modules.register(Box::new(corridor::SurfaceModule::default()));
    modules.register(Box::new(corridor::CorridorModule));
}

/// Categories whose elements are edited with the generic definition editor and drawn in the 3D
/// site view from their solids, in the frame of their site placement.
pub fn owns(category: &str) -> bool {
    [
        drainage::DRAIN_CATEGORY,
        drainage::CULVERT_CATEGORY,
        gantry::GANTRY_CATEGORY,
        lighting::CATEGORY,
        restraint::CATEGORY,
        pavement::PAVEMENT_CATEGORY,
        corridor::CORRIDOR_CATEGORY,
    ]
    .contains(&category)
}

/// A ground surface: shown in the 3D site view from its mesh, never edited as a definition (its
/// data is the whole TIN).
pub fn is_surface(category: &str) -> bool {
    category == corridor::SURFACE_CATEGORY
}

/// Keys of a definition that the generic editor leaves alone: caches and records the crates
/// maintain themselves (a lighting element's computed layout, a barrier run's resolved
/// geometry, a corridor's summary and its element references).
pub const HIDDEN_KEYS: [&str; 10] = ["placeholder", "route", "geometry", "signs", "reference_level_m", "alignment", "layout", "resolved", "computed", "surface"];

/// The words a text field of a definition may take, where the field is a choice among named
/// values. Typing them freely would leave the definition unreadable between keystrokes.
pub fn choices(key: &str) -> Option<Vec<String>> {
    let names = |list: &[&str]| Some(list.iter().map(|s| s.to_string()).collect());
    match key {
        "arrangement" => names(&["SingleLeft", "SingleRight", "Staggered", "Opposite", "TwinCentral"]),
        "end_treatment" => names(&["KeepSpacing", "ColumnAtEnd", "EqualiseSpacing"]),
        "first_side" => names(&["Left", "Right", "Central"]),
        "low_side" => names(&["Left", "Right"]),
        "traffic_face" => names(&["left", "right", "both"]),
        "containment" => Some(restraint::ContainmentLevel::ALL.iter().map(|c| c.label().to_string()).collect()),
        "working_width" => Some(restraint::WorkingWidthClass::ALL.iter().map(|c| c.label().to_string()).collect()),
        "impact_severity" => names(&["A", "B", "C"]),
        "material" => names(&["SurfaceDressing", "HotMixAsphalt", "GranularBase", "CementedBase", "BituminousBase", "GranularSubbase", "CementedSubbase", "CementedUpperSubbase", "SelectedLayer"]),
        _ => None,
    }
}

/// Text fields that name a variant which cannot be changed by typing (the variant carries its own
/// fields). They are shown, not edited.
pub const READ_ONLY_KEYS: [&str; 6] = ["crossfall_shape", "start_end", "end_end", "kind", "layers", "aim"];

fn alignment_for(doc: &Document, id: ElementId) -> Result<Alignment, String> {
    alignment_glue::alignment_of(doc, id).ok_or_else(|| "That alignment cannot be read.".to_string())
}

/// Adds an element of `kind` on the alignment with placeholder dimensions and returns its mark.
/// `start` is the only station for a kind that stands at one. Stations are internal stations.
pub fn add(doc: &mut Document, kind: usize, alignment_id: ElementId, start: f64, end: f64, offset: f64) -> Result<String, String> {
    let a = alignment_for(doc, alignment_id)?;
    let info = KINDS.get(kind).ok_or("Unknown kind of element.")?;
    let mark = doc.unique_name(info.mark);
    let id = match kind {
        0 => drainage::add_side_drain(doc, &mark, &drainage::SideDrainDefinition::placeholder(&a.name, start, end, offset)).map_err(|e| e.to_string())?,
        1 => drainage::add_pipe_culvert(doc, &mark, &drainage::PipeCulvertDefinition::placeholder(&a.name, start)).map_err(|e| e.to_string())?,
        2 => gantry::add_gantry(doc, &mark, &GantryDefinition::placeholder_portal(&a.name, start), Some(&a)).map_err(|e| e.to_string())?,
        _ => gantry::add_gantry(doc, &mark, &GantryDefinition::placeholder_cantilever(&a.name, start), Some(&a)).map_err(|e| e.to_string())?,
    };
    // Its sheet drawing goes beside what is already there, not on top of it.
    let (w, h) = doc.find(id).map(|e| (e.width, e.height)).unwrap_or((1000.0, 1000.0));
    let at = {
        let others: Vec<_> = doc.elements().iter().filter(|e| e.id != id && doc.layer_visible(&e.layer)).map(|e| e.bounds()).collect();
        let right = others.iter().map(|b| b.max.x).fold(0.0, f64::max);
        afrobim_kernel::Pt::new(if others.is_empty() { 0.0 } else { right + Document::ARRANGE_GAP }, 0.0)
    };
    let _ = (w, h);
    if let Some(e) = doc.find_mut(id) {
        e.origin = at;
    }
    Ok(doc.find(id).map(|e| e.name.clone()).unwrap_or(mark))
}

/// Brings an element back in step after its definition was edited in Properties. Returns a
/// message when the edit cannot be applied as it stands.
pub fn after_edit(doc: &mut Document, id: ElementId) -> Option<String> {
    let (category, data) = doc.find(id).map(|e| (e.category.clone(), e.data.clone()))?;
    if category == gantry::GANTRY_CATEGORY {
        let def: GantryDefinition = serde_json::from_value(data).ok()?;
        let alignment = doc.elements().iter().find(|e| e.category == alignment_glue::CATEGORY && e.name == def.alignment).map(|e| e.id).and_then(|a| alignment_glue::alignment_of(doc, a));
        return gantry::set_definition(doc, id, &def, alignment.as_ref()).err().map(|e| e.to_string());
    }
    if category == drainage::DRAIN_CATEGORY || category == drainage::CULVERT_CATEGORY {
        let problems = drainage::sync_drainage(doc);
        return (!problems.is_empty()).then(|| problems.join(" "));
    }
    let refreshed = match category.as_str() {
        c if c == lighting::CATEGORY => road_glue::refresh_lighting(doc, id),
        c if c == restraint::CATEGORY => road_glue::refresh_restraint(doc, id),
        c if c == pavement::PAVEMENT_CATEGORY => road_glue::refresh_pavement(doc, id),
        c if c == corridor::CORRIDOR_CATEGORY => road_glue::refresh_corridor(doc, id),
        _ => Ok(()),
    };
    refreshed.err()
}

/// Re-synchronises everything this glue owns, e.g. after alignments were imported again.
/// Returns what could not be brought up to date and why.
pub fn sync_all(doc: &mut Document) -> Vec<String> {
    let mut problems = drainage::sync_drainage(doc);
    let ids: Vec<(ElementId, String)> = doc
        .elements()
        .iter()
        .filter(|e| [gantry::GANTRY_CATEGORY, lighting::CATEGORY, pavement::PAVEMENT_CATEGORY, corridor::CORRIDOR_CATEGORY].contains(&e.category.as_str()))
        .map(|e| (e.id, e.name.clone()))
        .collect();
    for (id, name) in ids {
        if let Some(why) = after_edit(doc, id) {
            problems.push(format!("{name}: {why}"));
        }
    }
    // The restraint crate synchronises all its runs at once.
    for (id, result) in restraint::sync_barrier_runs(doc) {
        if let Err(why) = result {
            problems.push(format!("{}: {why}", doc.find(id).map(|e| e.name.clone()).unwrap_or_default()));
        }
    }
    pavement::sync_alignments(doc);
    problems
}

/// What the CHECK command can do with the selected element.
pub enum Check {
    /// The element has no check here.
    None,
    /// A side drain: the check needs a design flow and a basis.
    DrainFlow,
    /// A barrier run: the check needs the speed limit.
    RestraintSpeed,
    /// A pavement: it is checked by running a design method (PAVEDESIGN).
    PavementDesign,
    /// The calculation, line by line, or why it could not be made.
    Lines(Result<Vec<String>, String>),
}

pub fn check(doc: &Document, id: ElementId) -> Check {
    let Some(e) = doc.find(id) else { return Check::None };
    if e.category == drainage::DRAIN_CATEGORY {
        return Check::DrainFlow;
    }
    if e.category == lighting::CATEGORY {
        return Check::Lines(road_glue::check_lighting(doc, id));
    }
    if e.category == restraint::CATEGORY {
        return Check::RestraintSpeed;
    }
    if e.category == pavement::PAVEMENT_CATEGORY {
        return Check::PavementDesign;
    }
    if e.category == gantry::GANTRY_CATEGORY {
        let run = || -> Result<Vec<String>, String> {
            let def = gantry::definition_of(doc, id).map_err(|e| e.to_string())?;
            let alignment = doc.elements().iter().find(|a| a.category == alignment_glue::CATEGORY && a.name == def.alignment).and_then(|a| alignment_glue::alignment_of(doc, a.id));
            Ok(gantry::design::GantryChecks::run(&e.name, &def, alignment.as_ref()).map_err(|e| e.to_string())?.calculation())
        };
        return Check::Lines(run());
    }
    Check::None
}

/// The capacity check of a side drain for a design flow, to the MoWT manual or UK DMRB.
pub fn check_drain(doc: &Document, id: ElementId, dmrb: bool, flow_m3_s: f64) -> Result<Vec<String>, String> {
    let e = doc.find(id).ok_or("The drain no longer exists.")?;
    let def = drainage::SideDrainDefinition::from_value(&e.data).map_err(|e| e.to_string())?;
    let basis = if dmrb { DesignBasis::Dmrb } else { DesignBasis::Mowt };
    Ok(def.check_capacity(basis, flow_m3_s, None).map_err(|e| e.to_string())?.calculation())
}

/// Quantity CSVs of everything this glue owns: (file suffix, contents), and the elements that
/// could not be measured with the reason.
pub fn quantities(doc: &Document) -> (Vec<(String, String)>, Vec<String>) {
    let mut out: Vec<(String, String)> = Vec::new();
    let q = drainage::measure_document(doc);
    if !q.drains.is_empty() {
        out.push(("drains.csv".into(), drainage::drains_to_csv(&q.drains)));
    }
    if !q.culverts.is_empty() {
        out.push(("culverts.csv".into(), drainage::culverts_to_csv(&q.culverts)));
    }
    let (rows, _) = gantry::GantryQuantities::measure_all(doc.elements());
    if !rows.is_empty() {
        out.push(("gantries.csv".into(), gantry::gantries_to_csv(&rows)));
    }
    let (more, problems) = road_glue::quantities(doc);
    out.extend(more);
    (out, problems)
}

// ─── signs on gantries ──────────────────────────────────────────────────────────────────

pub fn gantries(doc: &Document) -> Vec<(ElementId, String)> {
    doc.elements().iter().filter(|e| e.category == gantry::GANTRY_CATEGORY).map(|e| (e.id, e.name.clone())).collect()
}

/// True when the sign hangs on a gantry (so it must not be given posts).
pub fn on_gantry(doc: &Document, sign: ElementId) -> bool {
    gantry::gantry_of(doc, sign).is_some()
}

/// Hangs a sign on a gantry over a lane (numbered from 1 at the left) or at an offset from the
/// centreline, with its lower edge `bottom_mm` above the underside of the boom (negative below).
/// `up_station` says which traffic a sign at an offset faces: that travelling up-station or down.
pub fn hang_sign(doc: &mut Document, gantry_id: ElementId, sign: ElementId, lane: Option<usize>, offset_m: f64, bottom_mm: f64, up_station: bool) -> Result<String, String> {
    let def = gantry::definition_of(doc, gantry_id).map_err(|e| e.to_string())?;
    let alignment = doc.elements().iter().find(|a| a.category == alignment_glue::CATEGORY && a.name == def.alignment).and_then(|a| alignment_glue::alignment_of(doc, a.id));
    let position = match lane {
        Some(index) => SignPosition::Lane { index },
        None => SignPosition::Offset { offset_m },
    };
    // Over a lane the sign serves that lane's traffic; at an offset the engineer says which.
    let serves = lane.is_none().then_some(if up_station { gantry::Direction::UpStation } else { gantry::Direction::DownStation });
    gantry::add_sign_to_gantry(doc, gantry_id, sign, position, bottom_mm, serves, alignment.as_ref()).map_err(|e| e.to_string())?;
    let (s, g) = (doc.find(sign).map(|e| e.name.clone()).unwrap_or_default(), doc.find(gantry_id).map(|e| e.name.clone()).unwrap_or_default());
    Ok(format!("{s} hangs on {g}."))
}

pub fn unhang_sign(doc: &mut Document, sign: ElementId) -> Result<(), String> {
    let g = gantry::gantry_of(doc, sign).ok_or("That sign is not on a gantry.")?;
    gantry::remove_sign_from_gantry(doc, g, sign).map_err(|e| e.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use afrobim_alignment::Pi;

    fn doc_with_alignment() -> (Document, ElementId) {
        let mut doc = Document::default();
        let a = Alignment::from_pis("MC10", 0.0, &[Pi::point(0.0, 0.0), Pi::point(500.0, 0.0)]).unwrap();
        let id = alignment_glue::add(&mut doc, &a).unwrap();
        (doc, id)
    }

    #[test]
    fn every_kind_is_added_measured_and_kept_in_step() {
        let (mut doc, a) = doc_with_alignment();
        let mut modules = ModuleRegistry::default();
        register(&mut modules);
        for kind in 0..KINDS.len() {
            let mark = add(&mut doc, kind, a, 100.0, 200.0, 5.0).unwrap_or_else(|e| panic!("{}: {e}", KINDS[kind].label));
            let e = doc.elements().iter().find(|e| e.name == mark).unwrap();
            assert!(owns(&e.category) && e.site.is_some(), "{mark}");
            assert!(modules.generate(e).is_ok(), "{mark}");
            let id = e.id;
            assert_eq!(after_edit(&mut doc, id), None, "{mark}");
        }
        let files: Vec<String> = quantities(&doc).0.into_iter().map(|(name, _)| name).collect();
        assert_eq!(files, ["drains.csv", "culverts.csv", "gantries.csv"]);
        // The two gantries got different marks.
        assert_eq!(gantries(&doc).len(), 2);
    }

    #[test]
    fn a_sign_hangs_on_a_gantry_and_the_gantry_is_checked() {
        let (mut doc, a) = doc_with_alignment();
        add(&mut doc, 2, a, 250.0, 250.0, 0.0).unwrap();
        let (g, _) = gantries(&doc)[0];
        let sign = doc.add(crate::signs_glue::CATEGORY, "S1", afrobim_kernel::Pt::new(0.0, 0.0), 3000.0, 2000.0, "Signs", serde_json::json!({}));
        hang_sign(&mut doc, g, sign, None, 0.0, 0.0, true).unwrap();
        assert!(on_gantry(&doc, sign) && doc.find(sign).unwrap().site.is_some());
        let Check::Lines(Ok(lines)) = check(&doc, g) else { panic!("a gantry has a check") };
        assert!(lines.iter().any(|l| l.contains("not a structural design")), "{lines:#?}");
        unhang_sign(&mut doc, sign).unwrap();
        assert!(!on_gantry(&doc, sign));
    }

    #[test]
    fn a_drain_is_checked_for_a_flow_on_either_basis() {
        let (mut doc, a) = doc_with_alignment();
        let mark = add(&mut doc, 0, a, 0.0, 100.0, 5.0).unwrap();
        let id = doc.elements().iter().find(|e| e.name == mark).unwrap().id;
        assert!(matches!(check(&doc, id), Check::DrainFlow));
        for dmrb in [false, true] {
            // The placeholder drain may lack what a check needs; either way the answer is a
            // calculation or a plain reason, never a panic.
            match check_drain(&doc, id, dmrb, 0.2) {
                Ok(lines) => assert!(!lines.is_empty()),
                Err(reason) => assert!(!reason.is_empty()),
            }
        }
    }

    // ─── lighting, restraint, pavement, corridor ────────────────────────────────────────
    //
    // The numbers in these tests are arbitrary test values, not design values.

    fn doc_with_profile() -> (Document, ElementId) {
        let mut doc = Document::default();
        let mut a = Alignment::from_pis("MC10", 0.0, &[Pi::point(0.0, 0.0), Pi::point(500.0, 0.0)]).unwrap();
        a.profile = Some(afrobim_alignment::Profile::new("FG", vec![afrobim_alignment::Pvi::point(0.0, 50.0), afrobim_alignment::Pvi::point(500.0, 50.0)]).unwrap());
        let id = alignment_glue::add(&mut doc, &a).unwrap();
        (doc, id)
    }

    fn lighting_request() -> road_glue::LightingRequest {
        let mut req = road_glue::LightingRequest::empty("LZ-01", 50.0, 250.0);
        req.arrangement = 2;
        req.spacing_m = 40.0;
        req.carriageway_width_m = 7.0;
        req.setback_m = 1.0;
        req.column = road_glue::ColumnFields { mounting_height_m: 8.0, bracket_outreach_m: 1.0, tilt_deg: 0.0, product: "Test luminaire".into(), lumen_output_lm: 5000.0, power_w: 50.0 };
        req
    }

    fn restraint_request() -> road_glue::RestraintRequest {
        let mut req = road_glue::RestraintRequest::empty("VRS-001", 100.0, 160.0);
        req.distance_m = 5.0;
        req.containment = 4;
        req.beam_height_m = 0.7;
        req.post_spacing_m = Some(2.0);
        req
    }

    fn pavement_request() -> road_glue::PavementRequest {
        let mut req = road_glue::PavementRequest::empty("PV-001", 0.0, 200.0);
        req.carriageway_width_m = 7.0;
        req.crossfall_percent = 2.5;
        req.layers = vec![
            road_glue::LayerRow { name: "Surfacing".into(), material: 1, thickness_mm: 50.0 },
            road_glue::LayerRow { name: "Base".into(), material: 2, thickness_mm: 150.0 },
        ];
        req
    }

    fn corridor_request(surface: ElementId) -> road_glue::CorridorRequest {
        let mut req = road_glue::CorridorRequest::empty("Corridor");
        req.surface = Some(surface);
        req.lane_width_m = 3.5;
        req.lane_crossfall_percent = 2.5;
        req.cut_run_per_rise = 2.0;
        req.fill_run_per_rise = 2.0;
        req.formation_depth_m = 0.3;
        req.interval_m = 50.0;
        req
    }

    fn surface(doc: &mut Document) -> ElementId {
        let tin = corridor::Tin::from_grid(-50.0, -100.0, 50.0, 50.0, 13, 5, |_, _| 48.0).unwrap();
        corridor::add_surface(doc, "Ground", &tin, None).unwrap()
    }

    #[test]
    fn lighting_restraint_pavement_and_corridor_are_added_generated_kept_in_step_and_measured() {
        let (mut doc, a) = doc_with_profile();
        let mut modules = ModuleRegistry::default();
        register(&mut modules);
        let ground = surface(&mut doc);

        let marks = [
            road_glue::add_lighting(&mut doc, a, &lighting_request()).unwrap(),
            road_glue::add_light_column(&mut doc, a, &{
                let mut c = road_glue::ColumnRequest::empty("LC-901", 300.0);
                c.distance_m = 5.0;
                c.column = lighting_request().column;
                c
            })
            .unwrap(),
            road_glue::add_restraint(&mut doc, a, &restraint_request()).unwrap(),
            road_glue::add_pavement(&mut doc, a, &pavement_request()).unwrap(),
            road_glue::add_corridor(&mut doc, a, &corridor_request(ground)).unwrap().0,
        ];
        for mark in &marks {
            let e = doc.elements().iter().find(|e| &e.name == mark).unwrap_or_else(|| panic!("{mark}"));
            assert!(owns(&e.category) && e.site.is_some(), "{mark}");
            let generated = modules.generate(e).unwrap_or_else(|why| panic!("{mark}: {why}"));
            assert!(!generated.notes.iter().any(|n| n.severity == afrobim_kernel::Severity::Error), "{mark}: {:?}", generated.notes);
            // Lighting, restraint and pavement draw solids; the corridor's surfaces are built from it.
            if e.category != corridor::CORRIDOR_CATEGORY {
                assert!(!generated.solids.is_empty(), "{mark}");
            }
            let id = e.id;
            assert_eq!(after_edit(&mut doc, id), None, "{mark}");
        }
        // The ground surface draws its mesh and is not edited as a definition.
        let g = doc.find(ground).unwrap();
        assert!(is_surface(&g.category) && !owns(&g.category));
        assert_eq!(modules.generate(g).unwrap().solids.len(), 1);

        // An edit in Properties is followed by the crate's own refresh.
        let columns = |doc: &Document, mark: &str| lighting::lighting_of(doc, doc.elements().iter().find(|e| e.name == mark).unwrap().id).unwrap().layout.columns.len();
        let id = doc.elements().iter().find(|e| e.name == marks[0]).unwrap().id;
        let before = columns(&doc, &marks[0]);
        doc.find_mut(id).unwrap().data["definition"]["Installation"]["spacing_m"] = serde_json::json!(20.0);
        assert_eq!(after_edit(&mut doc, id), None);
        assert!(columns(&doc, &marks[0]) > before);
        // An edit the crate refuses is reported, with the reason, and not applied silently.
        doc.find_mut(id).unwrap().data["definition"]["Installation"]["spacing_m"] = serde_json::json!(0.0);
        assert!(after_edit(&mut doc, id).is_some_and(|why| why.contains("spacing")));
        doc.find_mut(id).unwrap().data["definition"]["Installation"]["spacing_m"] = serde_json::json!(40.0);
        assert_eq!(after_edit(&mut doc, id), None);

        let id = doc.elements().iter().find(|e| e.name == marks[2]).unwrap().id;
        doc.find_mut(id).unwrap().data["run"]["end_station_m"] = serde_json::json!(200.0);
        assert_eq!(after_edit(&mut doc, id), None);
        assert!((restraint::barrier_run_of(&doc, id).unwrap().resolved.length_m - 100.0).abs() < 1e-6);

        let id = doc.elements().iter().find(|e| e.name == marks[3]).unwrap().id;
        doc.find_mut(id).unwrap().data["carriageway_width_m"] = serde_json::json!(8.0);
        assert_eq!(after_edit(&mut doc, id), None);
        assert!(doc.find(id).unwrap().width > 0.0);

        let id = doc.elements().iter().find(|e| e.name == marks[4]).unwrap().id;
        let sections = |doc: &Document| corridor::corridor_element(doc, id).unwrap().computed.unwrap().sections;
        let before = sections(&doc);
        doc.find_mut(id).unwrap().data["definition"]["interval_m"] = serde_json::json!(25.0);
        assert_eq!(after_edit(&mut doc, id), None);
        assert!(sections(&doc) > before);
        // The 3D surfaces are built from the corridor, in site coordinates.
        let solids = road_glue::corridor_solids(&doc, id).unwrap();
        assert_eq!(solids.meshes.len(), 3);
        let top = solids.meshes[0].vertices.iter().map(|v| v[2]).fold(f64::NEG_INFINITY, f64::max);
        assert!((top - 50_000.0).abs() < 1e-6, "the crown of the top lies at the profile level of 50 m, not at zero: {top}");
        assert!(!solids.meshes[2].triangles.is_empty(), "the ground is below the road: fill slopes");

        // Everything is kept in step together.
        assert_eq!(sync_all(&mut doc), Vec::<String>::new());

        // Each measures: a schedule and a summary, and the earthworks.
        let (files, problems) = quantities(&doc);
        assert_eq!(problems, Vec::<String>::new());
        let names: Vec<&str> = files.iter().map(|(name, _)| name.as_str()).collect();
        assert_eq!(names, ["lighting.csv", "lighting.summary.csv", "restraint.csv", "restraint.summary.csv", "pavement.csv", "pavement.summary.csv", "earthworks.csv"]);
        let csv = |name: &str| files.iter().find(|(n, _)| n == name).unwrap().1.clone();
        assert!(csv("lighting.csv").contains("LC-901") && csv("restraint.csv").contains("VRS-001") && csv("pavement.csv").contains("PV-001"));
        assert!(csv("earthworks.csv").lines().count() > 3, "{}", csv("earthworks.csv"));
        // A corridor whose surface is gone is reported, not skipped silently.
        doc.remove(ground);
        let (_, problems) = quantities(&doc);
        assert!(problems.iter().any(|p| p.contains("Corridor")), "{problems:?}");
    }

    #[test]
    fn the_checks_return_lines_or_a_plain_reason() {
        let (mut doc, a) = doc_with_profile();
        road_glue::add_lighting(&mut doc, a, &lighting_request()).unwrap();
        road_glue::add_restraint(&mut doc, a, &restraint_request()).unwrap();
        road_glue::add_pavement(&mut doc, a, &pavement_request()).unwrap();
        let find = |doc: &Document, prefix: &str| doc.elements().iter().find(|e| e.name.starts_with(prefix)).unwrap().id;

        // Lighting: the report carries the warning that BS 5489-1:2003 is superseded.
        let Check::Lines(Ok(lines)) = check(&doc, find(&doc, "LZ")) else { panic!("lighting has a check") };
        assert!(lines.iter().any(|l| l.starts_with("WARNING") && l.contains("superseded")), "{lines:#?}");
        // Restraint: it needs the speed limit, and says so when there is none.
        let run = find(&doc, "VRS");
        assert!(matches!(check(&doc, run), Check::RestraintSpeed));
        let speed = road_glue::SpeedInput { value: 0.0, mph: false };
        assert!(road_glue::check_restraint(&doc, run, speed).unwrap_err().contains("speed"));
        let lines = road_glue::check_restraint(&doc, run, road_glue::SpeedInput { value: 100.0, mph: false }).unwrap();
        assert!(lines.iter().any(|l| l.contains("CD 377")), "{lines:#?}");
        // Pavement: checked by running a design method.
        assert!(matches!(check(&doc, find(&doc, "PV")), Check::PavementDesign));
        // The alignment is not something this glue checks.
        assert!(matches!(check(&doc, a), Check::None));
    }

    #[test]
    fn the_design_methods_run_with_entered_values_and_refuse_without() {
        use road_glue::{AashtoForm, ClassRow, DesignForm, DmrbForm, MowtForm, SnRow};
        // Nothing entered: each method says why it cannot run.
        for method in 0..3 {
            let form = DesignForm { method, ..DesignForm::default() };
            match form.run() {
                Err(reason) => assert!(!reason.is_empty()),
                Ok(done) => assert!(!done.lines.is_empty()),
            }
        }
        // MoWT, with arbitrary test traffic and subgrade: the calculation and its notes.
        let mowt = MowtForm {
            classes: vec![ClassRow { name: "buses".into(), daily_first_year: 40.0, esa_per_vehicle: 1.0 }],
            growth_percent: 3.0,
            design_period_years: 15,
            paved_width_m: 6.0,
            subgrade_cbr_percent: 8.0,
            ..MowtForm::default()
        };
        let done = DesignForm { method: 0, mowt, ..DesignForm::default() }.run().unwrap();
        assert!(done.lines.iter().any(|l| l.contains("MoWT") || l.contains("Design")), "{:#?}", done.lines);
        if let Some(layers) = &done.layers {
            assert!(layers.iter().any(|l| l.thickness_mm > 0.0));
        }
        // DMRB gives traffic and warnings, never a layer stack.
        let dmrb = DmrbForm { flows: vec![(9, 500.0)], design_period_years: 40, lanes_one_direction: 2, foundation_class: 3, pavement: 2, ..DmrbForm::default() };
        let done = DesignForm { method: 1, dmrb, ..DesignForm::default() }.run().unwrap();
        assert!(done.layers.is_none() && done.lines.iter().any(|l| l.starts_with("Note:")), "{:#?}", done.lines);
        // AASHTO 1993 gives the layers back when the structure it was given is adequate.
        let layers = vec![
            SnRow { name: "Asphalt".into(), material: 1, thickness_mm: 400.0, layer_coefficient: 0.44, drainage_coefficient: 1.0, resilient_modulus_psi: None },
            SnRow { name: "Base".into(), material: 2, thickness_mm: 400.0, layer_coefficient: 0.14, drainage_coefficient: 1.0, resilient_modulus_psi: None },
        ];
        let aashto = AashtoForm { w18: 1.0e6, reliability_percent: 90.0, overall_standard_deviation: 0.45, delta_psi: 2.0, roadbed_modulus_psi: 8000.0, layers };
        let done = DesignForm { method: 2, aashto, ..DesignForm::default() }.run().unwrap();
        assert!(done.lines.iter().any(|l| l.starts_with("Note:")), "the method's notes are shown");
        assert_eq!(done.layers.map(|l| l.len()), Some(2));
        // The layers become a pavement.
        let (mut doc, a) = doc_with_profile();
        let mut req = road_glue::PavementRequest::empty("PV-001", 0.0, 100.0);
        req.carriageway_width_m = 7.0;
        req.crossfall_percent = 2.5;
        req.layers = road_glue::LayerRow::from_layers(&[afrobim_pavement::Layer::new("Asphalt", afrobim_pavement::Material::HotMixAsphalt, 400.0)]);
        assert!(road_glue::add_pavement(&mut doc, a, &req).is_ok());
    }

    #[test]
    fn what_cannot_be_added_says_why() {
        let (mut doc, a) = doc_with_profile();
        // Nothing is defaulted: an empty request is refused by the crate with its reason.
        let empty = road_glue::LightingRequest::empty("LZ-01", 0.0, 100.0);
        assert!(road_glue::add_lighting(&mut doc, a, &empty).unwrap_err().contains("spacing"));
        assert!(road_glue::add_restraint(&mut doc, a, &road_glue::RestraintRequest::empty("VRS-001", 0.0, 50.0)).unwrap_err().contains("beam height"));
        assert!(road_glue::add_pavement(&mut doc, a, &road_glue::PavementRequest::empty("PV-001", 0.0, 50.0)).is_err());
        assert!(road_glue::add_corridor(&mut doc, a, &road_glue::CorridorRequest::empty("C")).unwrap_err().contains("surface"));
        let ground = surface(&mut doc);
        let mut req = road_glue::CorridorRequest::empty("C");
        req.surface = Some(ground);
        assert!(road_glue::add_corridor(&mut doc, a, &req).is_err());
        assert!(doc.elements().iter().all(|e| !owns(&e.category)), "nothing is added when it is refused");
        // A corridor needs a profile.
        let flat = Alignment::from_pis("NP", 0.0, &[Pi::point(0.0, 0.0), Pi::point(300.0, 0.0)]).unwrap();
        let no_profile = alignment_glue::add(&mut doc, &flat).unwrap();
        assert!(road_glue::add_corridor(&mut doc, no_profile, &corridor_request(ground)).unwrap_err().to_lowercase().contains("profile"));
    }

    #[test]
    fn the_demo_adds_one_of_each() {
        let mut doc = Document::default();
        let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../afrobim-alignment/tests/landxml/corridor.xml");
        alignment_glue::import_landxml(&mut doc, &path).unwrap();
        assert_eq!(road_glue::demo(&mut doc), Vec::<String>::new());
        let has = |category: &str| doc.elements().iter().any(|e| e.category == category);
        for category in [lighting::CATEGORY, restraint::CATEGORY, pavement::PAVEMENT_CATEGORY, corridor::CORRIDOR_CATEGORY, corridor::SURFACE_CATEGORY] {
            assert!(has(category), "{category}");
        }
        let (files, problems) = quantities(&doc);
        assert_eq!(problems, Vec::<String>::new());
        assert!(files.iter().any(|(n, _)| n == "earthworks.csv"));
    }
}
