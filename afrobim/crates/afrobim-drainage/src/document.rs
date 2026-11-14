//! Adding drains and culverts to a document, and keeping them in step with their alignments.
//!
//! A drainage element names its alignment. The alignment is read from the document's element of
//! category `"alignment"` with that name (the `afrobim-alignment` module's category, whose
//! `data` is the serialised alignment). This crate only reads alignments.

use afrobim_alignment::Alignment;
use afrobim_kernel::{Document, ElementId, SitePlacement};

use crate::quantities::DrainageQuantities;
use crate::{DrainageError, PipeCulvertDefinition, SideDrainDefinition, CULVERT_CATEGORY, DRAIN_CATEGORY};

/// The layer side drains are put on.
pub const DRAINS_LAYER: &str = "Drains";
/// The layer culverts are put on.
pub const CULVERTS_LAYER: &str = "Culverts";

/// The alignment of the given name in the document.
pub fn alignment_named(doc: &Document, name: &str) -> Result<Alignment, DrainageError> {
    doc.elements()
        .iter()
        .find(|e| e.category == afrobim_alignment::CATEGORY && e.name == name)
        .and_then(|e| serde_json::from_value::<Alignment>(e.data.clone()).ok())
        .ok_or_else(|| DrainageError::NotAnAlignment(name.to_string()))
}

fn drain_site(def: &SideDrainDefinition) -> Option<SitePlacement> {
    let route = def.route.as_ref()?;
    let first = route.points.first()?;
    Some(SitePlacement {
        x: first.x_m * 1000.0,
        y: first.y_m * 1000.0,
        z: 0.0,
        rotation_deg: 0.0,
        alignment: Some(def.alignment.clone()),
        station_m: Some(route.start_station_m),
        offset_m: Some(route.offset_m),
    })
}

/// Adds a side drain and returns its id.
///
/// * The drain is routed along the alignment it names, which must be in the document; the
///   route is stored in the element's data. A definition with Error notes is refused.
/// * Layer [`DRAINS_LAYER`]; the name is `mark` made unique in the document.
/// * On the sheet the element is its cross-section, placed at the document's next free origin.
/// * Site placement: the drain's centreline at its start station, rotation 0, z = 0 (see the
///   crate documentation for the solids' frame).
pub fn add_side_drain(doc: &mut Document, mark: &str, definition: &SideDrainDefinition) -> Result<ElementId, DrainageError> {
    let alignment = alignment_named(doc, &definition.alignment)?;
    let mut def = definition.clone();
    def.route = None;
    def.check()?;
    def.route_along(&alignment)?;
    let extents = def.drawing().extents;
    let (w, h) = (extents.width(), extents.height());
    let name = doc.unique_name(mark);
    let origin = doc.next_free_origin(w, h);
    doc.ensure_layer(DRAINS_LAYER);
    let id = doc.add(DRAIN_CATEGORY, &name, origin, w, h, DRAINS_LAYER, def.to_value());
    let site = drain_site(&def);
    if let Some(e) = doc.find_mut(id) {
        e.site = site;
    }
    Ok(id)
}

/// Adds a pipe culvert and returns its id.
///
/// * The alignment it names must be in the document and hold its station. A definition with
///   Error notes is refused.
/// * Layer [`CULVERTS_LAYER`]; the name is `mark` made unique in the document.
/// * On the sheet the element is its elevation, placed at the document's next free origin.
/// * Site placement: [`PipeCulvertDefinition::site_placement`].
pub fn add_pipe_culvert(doc: &mut Document, mark: &str, definition: &PipeCulvertDefinition) -> Result<ElementId, DrainageError> {
    let alignment = alignment_named(doc, &definition.alignment)?;
    let site = definition.site_placement(&alignment)?;
    let extents = definition.drawing().extents;
    let (w, h) = (extents.width(), extents.height());
    let name = doc.unique_name(mark);
    let origin = doc.next_free_origin(w, h);
    doc.ensure_layer(CULVERTS_LAYER);
    let id = doc.add(CULVERT_CATEGORY, &name, origin, w, h, CULVERTS_LAYER, definition.to_value());
    if let Some(e) = doc.find_mut(id) {
        e.site = Some(site);
    }
    Ok(id)
}

/// Brings every drain and culvert of the document back in step with its definition and its
/// alignment: drains are routed again, site placements recomputed and sheet sizes updated. Call
/// it after an alignment changes or a definition is edited.
///
/// Returns one message per element that could not be brought up to date (its alignment is
/// missing, or no longer holds its stations); such an element keeps what it had.
pub fn sync_drainage(doc: &mut Document) -> Vec<String> {
    let mut problems = Vec::new();
    let ids: Vec<(ElementId, String, String)> = doc
        .elements()
        .iter()
        .filter(|e| e.category == DRAIN_CATEGORY || e.category == CULVERT_CATEGORY)
        .map(|e| (e.id, e.category.clone(), e.name.clone()))
        .collect();
    for (id, category, name) in ids {
        let Some(data) = doc.find(id).map(|e| e.data.clone()) else { continue };
        let result: Result<(serde_json::Value, Option<SitePlacement>, f64, f64), DrainageError> = if category == DRAIN_CATEGORY {
            SideDrainDefinition::from_value(&data).and_then(|mut def| {
                let alignment = alignment_named(doc, &def.alignment)?;
                def.route_along(&alignment)?;
                let extents = def.drawing().extents;
                Ok((def.to_value(), drain_site(&def), extents.width(), extents.height()))
            })
        } else {
            PipeCulvertDefinition::from_value(&data).and_then(|def| {
                let alignment = alignment_named(doc, &def.alignment)?;
                let site = def.site_placement(&alignment)?;
                let extents = def.drawing().extents;
                Ok((def.to_value(), Some(site), extents.width(), extents.height()))
            })
        };
        match result {
            Ok((value, site, w, h)) => {
                if let Some(e) = doc.find_mut(id) {
                    e.data = value;
                    e.site = site;
                    e.width = w;
                    e.height = h;
                }
            }
            Err(err) => problems.push(format!("{name}: {err}")),
        }
    }
    problems
}

/// The take-off of every drain and culvert in the document, in document order.
pub fn measure_document(doc: &Document) -> DrainageQuantities {
    DrainageQuantities::measure(doc.elements())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::design::mowt::ChannelMaterial;
    use crate::{ChannelSection, Lining, PipeCulvertModule, Side, SideDrainModule};
    use afrobim_alignment::{AlignmentBuilder, Turn};
    use afrobim_kernel::{Module, ModuleRegistry, Pt};

    fn close(actual: f64, expected: f64, tolerance: f64) {
        assert!((actual - expected).abs() <= tolerance, "expected {expected} ± {tolerance}, got {actual}");
    }

    fn add_alignment(doc: &mut Document, a: &Alignment) -> ElementId {
        doc.add(afrobim_alignment::CATEGORY, &a.name, Pt::new(0.0, 0.0), 1.0, 1.0, "Alignments", serde_json::to_value(a).unwrap())
    }

    fn doc_with_alignment() -> (Document, ElementId) {
        let a = AlignmentBuilder::new("MC10", 0.0, 1000.0, 2000.0, 0.0).line(100.0).arc(200.0, 100.0, Turn::Left).build().unwrap();
        let mut doc = Document::default();
        let id = add_alignment(&mut doc, &a);
        (doc, id)
    }

    fn drain() -> SideDrainDefinition {
        SideDrainDefinition {
            alignment: "MC10".into(),
            start_station_m: 100.0,
            end_station_m: 200.0,
            offset_m: 5.0,
            section: ChannelSection::Trapezoidal { bottom_width_mm: 500.0, depth_mm: 600.0, left_side_slope: 1.5, right_side_slope: 1.5 },
            lining: Lining::Table(ChannelMaterial::Masonry),
            lining_thickness_mm: Some(100.0),
            gradient_m_per_m: 0.01,
            manning_n: Some(0.02),
            route: None,
            placeholder: false,
        }
    }

    fn culvert() -> PipeCulvertDefinition {
        PipeCulvertDefinition {
            alignment: "MC10".into(),
            station_m: 50.0,
            skew_deg: 0.0,
            barrels: 1,
            internal_diameter_mm: 900.0,
            wall_thickness_mm: 100.0,
            length_mm: 12_000.0,
            barrel_spacing_mm: None,
            invert_level_inlet_m: 10.24,
            invert_level_outlet_m: 10.0,
            inlet_side: Side::Left,
            centre_offset_m: 0.0,
            road_level_m: Some(12.5),
            material: "as specified".into(),
            placeholder: false,
        }
    }

    #[test]
    fn adding_a_drain_routes_it_and_places_it_on_site() {
        let (mut doc, _) = doc_with_alignment();
        let id = add_side_drain(&mut doc, "SD-01", &drain()).unwrap();
        let e = doc.find(id).unwrap().clone();
        assert_eq!((e.category.as_str(), e.name.as_str(), e.layer.as_str()), (DRAIN_CATEGORY, "SD-01", DRAINS_LAYER));
        let def = SideDrainDefinition::from_value(&e.data).unwrap();
        close(def.route.as_ref().unwrap().length_m, 102.5, 1e-9);
        // The arc starts at (1100, 2000) heading east; 5 m right of it is (1100, 1995).
        let site = e.site.as_ref().unwrap();
        close(site.x, 1_100_000.0, 1e-6);
        close(site.y, 1_995_000.0, 1e-6);
        assert_eq!((site.rotation_deg, site.z, site.alignment.as_deref(), site.station_m, site.offset_m), (0.0, 0.0, Some("MC10"), Some(100.0), Some(5.0)));
        // The sheet size is the cross-section's.
        let g = SideDrainModule::default().generate(&e).unwrap();
        assert_eq!((e.width, e.height), (g.drawing.extents.width(), g.drawing.extents.height()));
        assert!(!g.solids.is_empty());
        // A second drain with the same mark gets a unique name.
        let second = add_side_drain(&mut doc, "SD-01", &drain()).unwrap();
        assert_ne!(doc.find(second).unwrap().name, "SD-01");
    }

    #[test]
    fn adding_a_culvert_places_its_inlet() {
        let (mut doc, _) = doc_with_alignment();
        let id = add_pipe_culvert(&mut doc, "C-01", &culvert()).unwrap();
        let e = doc.find(id).unwrap().clone();
        assert_eq!((e.category.as_str(), e.layer.as_str()), (CULVERT_CATEGORY, CULVERTS_LAYER));
        let site = e.site.as_ref().unwrap();
        close(site.x, 1_050_000.0, 1e-6);
        close(site.rotation_deg, 270.0, 1e-9);
        close(site.z, -2260.0, 1e-6);
        let g = PipeCulvertModule::default().generate(&e).unwrap();
        assert_eq!((e.width, e.height), (g.drawing.extents.width(), g.drawing.extents.height()));
    }

    #[test]
    fn elements_that_cannot_be_placed_are_refused() {
        let (mut doc, _) = doc_with_alignment();
        let before = doc.elements().len();
        assert_eq!(add_side_drain(&mut doc, "SD", &SideDrainDefinition { alignment: "MC99".into(), ..drain() }), Err(DrainageError::NotAnAlignment("MC99".into())));
        assert!(matches!(add_side_drain(&mut doc, "SD", &SideDrainDefinition { end_station_m: 500.0, ..drain() }), Err(DrainageError::Route(_))));
        assert!(matches!(add_side_drain(&mut doc, "SD", &SideDrainDefinition { gradient_m_per_m: -1.0, ..drain() }), Err(DrainageError::InvalidDefinition(_))));
        assert!(matches!(add_pipe_culvert(&mut doc, "C", &PipeCulvertDefinition { station_m: 999.0, ..culvert() }), Err(DrainageError::Route(_))));
        assert!(matches!(add_pipe_culvert(&mut doc, "C", &PipeCulvertDefinition { barrels: 0, ..culvert() }), Err(DrainageError::InvalidDefinition(_))));
        assert_eq!(doc.elements().len(), before);
    }

    #[test]
    fn sync_follows_a_changed_alignment_and_reports_what_it_cannot_place() {
        let (mut doc, alignment_id) = doc_with_alignment();
        let drain_id = add_side_drain(&mut doc, "SD-01", &drain()).unwrap();
        let culvert_id = add_pipe_culvert(&mut doc, "C-01", &culvert()).unwrap();
        assert!(sync_drainage(&mut doc).is_empty());

        // The curve is tightened to R = 100 m: the drain outside it is now 100 × 105/100 = 105 m.
        let tighter = AlignmentBuilder::new("MC10", 0.0, 1000.0, 2000.0, 0.0).line(100.0).arc(100.0, 100.0, Turn::Left).build().unwrap();
        doc.find_mut(alignment_id).unwrap().data = serde_json::to_value(&tighter).unwrap();
        assert!(sync_drainage(&mut doc).is_empty());
        let def = SideDrainDefinition::from_value(&doc.find(drain_id).unwrap().data).unwrap();
        close(def.route.unwrap().length_m, 105.0, 1e-9);

        // An edited definition is picked up: the drain moves to the other side.
        let mut edited = SideDrainDefinition::from_value(&doc.find(drain_id).unwrap().data).unwrap();
        edited.offset_m = -5.0;
        doc.find_mut(drain_id).unwrap().data = edited.to_value();
        assert!(sync_drainage(&mut doc).is_empty());
        let e = doc.find(drain_id).unwrap();
        close(SideDrainDefinition::from_value(&e.data).unwrap().route.unwrap().length_m, 95.0, 1e-9);
        close(e.site.as_ref().unwrap().y, 2_005_000.0, 1e-6);

        // The alignment is cut back to 80 m: neither element fits, both keep what they had.
        let short = AlignmentBuilder::new("MC10", 0.0, 1000.0, 2000.0, 0.0).line(40.0).build().unwrap();
        doc.find_mut(alignment_id).unwrap().data = serde_json::to_value(&short).unwrap();
        let problems = sync_drainage(&mut doc);
        assert_eq!(problems.len(), 2);
        assert!(problems[0].starts_with("SD-01:") && problems[1].starts_with("C-01:"));
        assert!(doc.find(culvert_id).unwrap().site.is_some());
    }

    #[test]
    fn document_take_off_and_module_registration() {
        let (mut doc, _) = doc_with_alignment();
        add_side_drain(&mut doc, "SD-01", &drain()).unwrap();
        add_side_drain(&mut doc, "SD-02", &SideDrainDefinition { offset_m: -5.0, ..drain() }).unwrap();
        add_pipe_culvert(&mut doc, "C-01", &culvert()).unwrap();
        // A drain whose stored data is broken is listed as skipped, not dropped.
        let broken = add_side_drain(&mut doc, "SD-03", &drain()).unwrap();
        doc.find_mut(broken).unwrap().data = serde_json::json!({"alignment": 3});
        let q = measure_document(&doc);
        assert_eq!((q.drains.len(), q.culverts.len(), q.skipped.len()), (2, 1, 1));
        assert_eq!(q.skipped[0].mark, "SD-03");
        // 102.5 + 97.5 = 200 m of drain; excavation 1.127389 × 200 = 225.478 m³; 12 m of pipe.
        close(q.total_drain_length_m(), 200.0, 1e-9);
        close(q.total_excavation_m3(), 225.4778, 1e-3);
        close(q.total_pipe_length_m(), 12.0, 1e-12);

        let mut modules = ModuleRegistry::default();
        modules.register(Box::new(SideDrainModule::default()));
        modules.register(Box::new(PipeCulvertModule::default()));
        for e in doc.elements().iter().filter(|e| e.name != "SD-03" && e.category != afrobim_alignment::CATEGORY) {
            let g = modules.generate(e).unwrap();
            assert!(!g.solids.is_empty() && !g.drawing.primitives.is_empty(), "{}", e.name);
        }
        // The whole document survives a save and load: the same elements measure the same.
        let again = Document::from_json(&doc.to_json()).unwrap();
        assert_eq!(again.elements().len(), doc.elements().len());
        let q2 = measure_document(&again);
        assert_eq!((q2.drains.len(), q2.culverts.len(), q2.skipped.len()), (2, 1, 1));
        close(q2.total_drain_length_m(), 200.0, 1e-9);
        close(q2.total_excavation_m3(), q.total_excavation_m3(), 1e-9);
        assert_eq!(q2.culverts, q.culverts);
    }

    /// Test values: a profile climbing from 50 m at station 0 to 60 m at station 1000 (arbitrary).
    fn graded_doc() -> (Document, Alignment) {
        use afrobim_alignment::{Profile, Pvi};
        let mut a = AlignmentBuilder::new("MC10", 0.0, 1000.0, 2000.0, 0.0).line(100.0).arc(200.0, 100.0, Turn::Left).build().unwrap();
        a.profile = Some(Profile::new("FG", vec![Pvi::point(0.0, 50.0), Pvi::point(1000.0, 60.0)]).unwrap());
        let mut doc = Document::default();
        add_alignment(&mut doc, &a);
        (doc, a)
    }

    #[test]
    fn a_drain_on_a_graded_alignment_follows_the_profile_and_one_without_stays_level() {
        let (mut doc, a) = graded_doc();
        let id = add_side_drain(&mut doc, "SD-01", &drain()).unwrap();
        let g = SideDrainModule::default().generate(doc.find(id).unwrap()).unwrap();
        let zs: Vec<f64> = g.solids.iter().flat_map(|m| m.vertices.iter().map(|v| v[2])).collect();
        // The top edges are at the profile level at the start (100 m: 50 + 10 * 100 / 1000) and
        // at the end (200 m: 52 m), not at zero.
        let top = |at: f64| a.profile.as_ref().unwrap().elevation_at(at).unwrap() * 1000.0;
        close(top(100.0), 51_000.0, 1e-6);
        let max = zs.iter().cloned().fold(f64::NEG_INFINITY, f64::max);
        close(max, top(200.0), 1e-6);
        let min_top_ring = g.solids[0].vertices.iter().take(8).map(|v| v[2]).fold(f64::NEG_INFINITY, f64::max);
        close(min_top_ring, top(100.0), 1e-6);
        assert!(zs.iter().any(|z| *z > 40_000.0), "the drain is at the level of the road, not at zero");
        // The route survives a save and open with its levels.
        let again = Document::from_json(&doc.to_json()).unwrap();
        let def = SideDrainDefinition::from_value(&again.find(id).unwrap().data).unwrap();
        assert!(def.route.unwrap().points.iter().all(|p| p.level_m.is_some()));
        // No profile: level at zero, as before.
        let (mut flat, _) = doc_with_alignment();
        let id = add_side_drain(&mut flat, "SD-01", &drain()).unwrap();
        let g = SideDrainModule::default().generate(flat.find(id).unwrap()).unwrap();
        let max = g.solids[0].vertices.iter().map(|v| v[2]).fold(f64::NEG_INFINITY, f64::max);
        close(max, 0.0, 1e-9);
    }

    #[test]
    fn a_culvert_on_a_graded_alignment_sits_at_its_datum_level() {
        let (mut doc, _) = graded_doc();
        let id = add_pipe_culvert(&mut doc, "C-01", &culvert()).unwrap();
        close(doc.find(id).unwrap().site.as_ref().unwrap().z, 10_240.0, 1e-6);
        // With no profile and a road level entered the invert is below a road at level zero.
        let (mut flat, _) = doc_with_alignment();
        let id = add_pipe_culvert(&mut flat, "C-01", &culvert()).unwrap();
        close(flat.find(id).unwrap().site.as_ref().unwrap().z, (10.24 - 12.5) * 1000.0, 1e-6);
    }
}
