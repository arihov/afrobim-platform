//! The marking as a kernel element: category `"marking"`, its plan drawing and solids, and the
//! helpers that add markings to a document and keep them in step with their alignments.

use std::collections::BTreeMap;

use afrobim_alignment::{format_station, Alignment};
use afrobim_draw::{Colour, Drawing, Extents, Mesh3, Primitive, Pt2};
use afrobim_kernel::{Document, Element, ElementId, Generated, Module, Note, Pt, Severity, SitePlacement};

use crate::catalogue::{speed_label, MarkingColour, EDITION};
use crate::definition::{MarkingDefinition, Resolved};
use crate::layout::Layout;
use crate::quantities::fixed;
use crate::MarkingError;

/// The element category this crate owns.
pub const CATEGORY: &str = "marking";

/// The layer [`add_marking`] puts markings on.
pub const LAYER: &str = "Road markings";

/// Height of the solids above the road surface, mm, **for display only**: enough for a marking
/// to stand clear of a flat ground plane in the 3D view. It is not a thickness from the manual
/// or any specification, and no quantity uses it.
pub const DISPLAY_THICKNESS_MM: f64 = 5.0;

/// Most sections of one mark drawn as a single filled outline; a longer mark (a continuous line
/// round a curve) is drawn as several outlines that share their end sections.
const SECTIONS_PER_FILL: usize = 64;

/// A marking in plan, with what is needed to put it back on the map.
///
/// The same frame as `afrobim_alignment::PlanDrawing`: millimetres, X east, Y north, not
/// rotated, with the lower-left corner of everything drawn on the origin; that origin is the
/// plan point (`origin_x_m`, `origin_y_m`). The solids share the frame, with z = 0 on the road
/// surface (taken as ground level 0; the vertical profile is not used) and their top at
/// [`DISPLAY_THICKNESS_MM`]. A site placement at (`origin_x_m` × 1000, `origin_y_m` × 1000, 0)
/// with no rotation puts both where they belong.
#[derive(Debug, Clone, PartialEq)]
pub struct MarkingPlan {
    /// One `Primitive::Fill` per mark (more for a long curved mark), in the marking's colour.
    pub drawing: Drawing,
    /// One mesh per strand: every mark of the strand as a closed prism.
    pub solids: Vec<Mesh3>,
    pub origin_x_m: f64,
    pub origin_y_m: f64,
}

fn draw_colour(colour: MarkingColour) -> Colour {
    match colour {
        MarkingColour::White => Colour::White,
        MarkingColour::Yellow => Colour::Yellow,
    }
}

/// Adds the closed prism over a strip to a mesh. `left` and `right` are the strip's edges in
/// the direction of travel, point for point.
fn add_prism(mesh: &mut Mesh3, left: &[Pt2], right: &[Pt2], levels: &[f64], thickness: f64) {
    let base = mesh.vertices.len() as u32;
    for (i, (l, r)) in left.iter().zip(right).enumerate() {
        // Per section: 0 left bottom, 1 right bottom, 2 left top, 3 right top.
        let z0 = levels.get(i).copied().unwrap_or(0.0);
        let z1 = z0 + thickness;
        mesh.vertices.extend([[l.x, l.y, z0], [r.x, r.y, z0], [l.x, l.y, z1], [r.x, r.y, z1]]);
    }
    let n = left.len().min(right.len()) as u32;
    if n < 2 {
        return;
    }
    let v = |section: u32, corner: u32| base + 4 * section + corner;
    for i in 0..n - 1 {
        let j = i + 1;
        mesh.triangles.extend([
            [v(i, 3), v(j, 3), v(j, 2)], // top
            [v(i, 3), v(j, 2), v(i, 2)],
            [v(i, 1), v(j, 0), v(j, 1)], // bottom
            [v(i, 1), v(i, 0), v(j, 0)],
            [v(i, 1), v(j, 1), v(j, 3)], // right side
            [v(i, 1), v(j, 3), v(i, 3)],
            [v(j, 0), v(i, 0), v(i, 2)], // left side
            [v(j, 0), v(i, 2), v(j, 2)],
        ]);
    }
    let e = n - 1;
    mesh.triangles.extend([
        [v(0, 0), v(0, 1), v(0, 3)], // leading end
        [v(0, 0), v(0, 3), v(0, 2)],
        [v(e, 1), v(e, 0), v(e, 2)], // trailing end
        [v(e, 1), v(e, 2), v(e, 3)],
    ]);
}

/// Draws a layout in plan and builds its solids. `None` when the layout has no marks.
pub fn plan(layout: &Layout, colour: MarkingColour, name: &str) -> Option<MarkingPlan> {
    plan_at_level(layout, colour, name, &|_| 0.0)
}

/// As [`plan`], with the solids raised to `level_mm(station)`: the road level at a station, mm.
/// The level of a point inside a mark is taken by spreading the mark's stations evenly over its
/// plan points, which is for display only; no quantity uses the solids.
pub fn plan_at_level(layout: &Layout, colour: MarkingColour, name: &str, level_mm: &dyn Fn(f64) -> f64) -> Option<MarkingPlan> {
    let ((min_x, min_y), (max_x, max_y)) = layout.bounds()?;
    // Subtract in metres before scaling, so large map coordinates lose nothing.
    let mm = |p: &(f64, f64)| Pt2::new((p.0 - min_x) * 1000.0, (p.1 - min_y) * 1000.0);
    let fill = draw_colour(colour);
    let mut primitives = Vec::new();
    let mut solids = Vec::new();
    for strand in &layout.strands {
        let label = if layout.strands.len() > 1 { format!("{name}, {}", strand.name) } else { name.to_string() };
        let mut mesh = Mesh3 { name: label, vertices: Vec::new(), triangles: Vec::new(), colour: fill.display_rgb() };
        for mark in &strand.marks {
            let left: Vec<Pt2> = mark.left.iter().map(mm).collect();
            let right: Vec<Pt2> = mark.right.iter().map(mm).collect();
            let last = (left.len().max(2) - 1) as f64;
            let levels: Vec<f64> = (0..left.len()).map(|i| level_mm(mark.start_station_m + (mark.end_station_m - mark.start_station_m) * i as f64 / last)).collect();
            add_prism(&mut mesh, &left, &right, &levels, DISPLAY_THICKNESS_MM);
            let mut from = 0;
            while from + 1 < left.len() {
                let to = (from + SECTIONS_PER_FILL).min(left.len() - 1);
                let outline = right[from..=to].iter().chain(left[from..=to].iter().rev()).copied().collect();
                primitives.push(Primitive::Fill { outline, colour: fill });
                from = to;
            }
        }
        solids.push(mesh);
    }
    let extents = Extents { min: Pt2::new(0.0, 0.0), max: Pt2::new((max_x - min_x) * 1000.0, (max_y - min_y) * 1000.0) };
    Some(MarkingPlan { drawing: Drawing { primitives, extents }, solids, origin_x_m: min_x, origin_y_m: min_y })
}

/// The kernel module for road markings.
///
/// `generate` checks the element's [`MarkingDefinition`] against the catalogue, lays the marks
/// along the copy of the alignment it carries and returns the [`MarkingPlan`] drawing and
/// solids. When a check raises an Error note it returns the notes with no geometry.
///
/// Derived values: `Line type`, `TSRGD reference`, `Pattern`, `Width`, `Colour`, `Speed limit`
/// (mph with the km/h equivalent), `Alignment`, `Stations`, `Offset`, `Overall length`, `Marks`,
/// `Part marks`, `Painted length`, `Painted area`, `Source`, and `plan_origin_x_m`,
/// `plan_origin_y_m` (plan coordinates of the drawing origin, m, 6 decimals), plus
/// `Line separation` for a double line.
#[derive(Debug, Clone, Copy, Default)]
pub struct MarkingModule;

impl Module for MarkingModule {
    fn category(&self) -> &str {
        CATEGORY
    }

    fn display_name(&self) -> &str {
        "Road markings"
    }

    fn generate(&self, element: &Element) -> Result<Generated, String> {
        let def = MarkingDefinition::from_value(&element.data).map_err(|e| format!("'{}': {e}", element.name))?;
        let (laid, notes) = def.lay_out();
        let Some((resolved, layout)) = laid else {
            return Ok(Generated { notes, ..Generated::default() });
        };
        // Where the alignment has a profile the solids lie at its level (clamped to the
        // profile's ends), so that they sit with the rest of the site model.
        let profile = def.geometry.as_ref().and_then(|a| a.profile.as_ref());
        let level = |station: f64| profile.and_then(|p| p.range().and_then(|(lo, hi)| p.elevation_at(station.clamp(lo, hi)).ok())).unwrap_or(0.0) * 1000.0;
        let Some(plan) = plan_at_level(&layout, def.colour, resolved.spec.name, &level) else {
            return Ok(Generated { notes, ..Generated::default() });
        };
        Ok(Generated { drawing: plan.drawing, notes, derived: derived(&def, &resolved, &layout, plan.origin_x_m, plan.origin_y_m), solids: plan.solids })
    }
}

fn derived(def: &MarkingDefinition, resolved: &Resolved, layout: &Layout, origin_x_m: f64, origin_y_m: f64) -> BTreeMap<String, String> {
    let spec = resolved.spec;
    let mut out = BTreeMap::new();
    let mut put = |k: &str, v: String| {
        out.insert(k.to_string(), v);
    };
    put("Line type", format!("{}, diagram {}", spec.name, spec.diagram));
    put("TSRGD reference", spec.tsrgd.to_string());
    put(
        "Pattern",
        match spec.double {
            Some(d) => format!("left line {}; right line {}", d.left.label(), d.right.label()),
            None => spec.pattern.label(),
        },
    );
    put("Width", format!("{} mm{}", resolved.width_mm, if spec.double.is_some() { " each line" } else { "" }));
    if let Some(separation) = resolved.separation_mm {
        put("Line separation", format!("{separation} mm ({} mm overall)", 2 * resolved.width_mm + separation));
    }
    put("Colour", def.colour.label().to_string());
    put("Speed limit", speed_label(def.speed_limit_mph));
    put("Alignment", def.alignment.clone());
    put("Stations", format!("{} to {}", format_station(def.start_station_m, 3), format_station(def.end_station_m, 3)));
    put("Offset", format!("{} m ({})", fixed(def.offset_m.abs(), 3, 3), if def.offset_m < 0.0 { "left" } else if def.offset_m > 0.0 { "right" } else { "on the alignment" }));
    put("Overall length", format!("{} m", fixed(layout.overall_length_m, 3, 3)));
    put("Marks", layout.whole_marks().to_string());
    put("Part marks", layout.part_marks().to_string());
    put("Painted length", format!("{} m", fixed(layout.painted_length_m(), 3, 3)));
    put("Painted area", format!("{} m²", fixed(layout.painted_area_m2(), 3, 3)));
    put("Source", format!("{}; {EDITION}", spec.citation));
    put("plan_origin_x_m", format!("{origin_x_m:.6}"));
    put("plan_origin_y_m", format!("{origin_y_m:.6}"));
    out
}

/// The alignment stored in an element of category `"alignment"`.
fn alignment_of(doc: &Document, id: ElementId) -> Result<Alignment, MarkingError> {
    let e = doc.find(id).ok_or(MarkingError::ElementNotFound(id.0))?;
    if e.category != afrobim_alignment::CATEGORY {
        return Err(MarkingError::NotAnAlignment(e.name.clone()));
    }
    let mut alignment: Alignment = serde_json::from_value(e.data.clone()).map_err(|_| MarkingError::NotAnAlignment(e.name.clone()))?;
    alignment.name = e.name.clone();
    Ok(alignment)
}

/// The Error notes of a list as one message.
fn errors(notes: &[Note]) -> Option<String> {
    let messages: Vec<&str> = notes.iter().filter(|n| n.severity == Severity::Error).map(|n| n.message.as_str()).collect();
    (!messages.is_empty()).then(|| messages.join(" "))
}

/// Size and site placement of a marking element from its definition. Fails on any Error note.
fn footprint(def: &MarkingDefinition) -> Result<(f64, f64, SitePlacement), MarkingError> {
    let (laid, notes) = def.lay_out();
    let (resolved, layout) = match (laid, errors(&notes)) {
        (Some(laid), None) => laid,
        (_, message) => return Err(MarkingError::InvalidDefinition(message.unwrap_or_else(|| "the marking cannot be laid out".to_string()))),
    };
    let plan = plan(&layout, def.colour, resolved.spec.name).ok_or_else(|| MarkingError::InvalidDefinition("the marking has no marks".to_string()))?;
    let site = SitePlacement {
        x: plan.origin_x_m * 1000.0,
        y: plan.origin_y_m * 1000.0,
        z: 0.0,
        rotation_deg: 0.0,
        alignment: Some(def.alignment.clone()),
        station_m: Some(def.start_station_m),
        offset_m: Some(def.offset_m),
    };
    Ok((plan.drawing.extents.width().max(1.0), plan.drawing.extents.height().max(1.0), site))
}

/// Adds a marking that follows the alignment element `alignment_id` and returns its id.
///
/// * `definition.alignment` and `definition.geometry` are filled in from the alignment element;
///   everything else is used as given.
/// * Nothing is added when the definition has an Error note (unknown type, stations outside the
///   alignment, a width not prescribed for the diagram...); the error carries the messages.
///   Warnings do not stop it: they appear as notes when the module generates the element.
/// * `name` is made unique in the document (`RM-001` → `RM-002`).
/// * The element goes on layer [`LAYER`] at the sheet origin, sized to its plan, with a site
///   placement at the plan position of its drawing origin (see [`MarkingPlan`]). A marking is in
///   plan coordinates, far from the design sheet where sign faces are laid out, so the layer is
///   hidden on the sheet when this call creates it, as the app does for alignments.
pub fn add_marking(doc: &mut Document, alignment_id: ElementId, name: &str, definition: &MarkingDefinition) -> Result<ElementId, MarkingError> {
    let alignment = alignment_of(doc, alignment_id)?;
    let mut def = definition.clone();
    def.alignment = alignment.name.clone();
    def.geometry = Some(alignment);
    let (width, height, site) = footprint(&def)?;
    let new_layer = !doc.layers().iter().any(|l| l.name == LAYER);
    let name = doc.unique_name(if name.trim().is_empty() { "RM-001" } else { name.trim() });
    let id = doc.add(CATEGORY, &name, Pt::new(0.0, 0.0), width, height, LAYER, def.to_value());
    if let Some(e) = doc.find_mut(id) {
        e.site = Some(site);
    }
    if new_layer {
        if let Some(layer) = doc.layers_mut().iter_mut().find(|l| l.name == LAYER) {
            layer.visible = false;
        }
    }
    Ok(id)
}

/// The definition of a marking element; `None` when `id` is not a readable marking.
pub fn marking_of(doc: &Document, id: ElementId) -> Option<MarkingDefinition> {
    let e = doc.find(id)?;
    if e.category != CATEGORY {
        return None;
    }
    MarkingDefinition::from_value(&e.data).ok()
}

/// Replaces the definition of an existing marking (after an edit in a dialog), keeping its
/// alignment, and updates its size and site placement. Nothing changes when the new definition
/// has an Error note.
pub fn set_marking(doc: &mut Document, id: ElementId, definition: &MarkingDefinition) -> Result<(), MarkingError> {
    let current = marking_of(doc, id).ok_or(MarkingError::NotAMarking(id.0))?;
    let mut def = definition.clone();
    if def.geometry.is_none() {
        def.alignment = current.alignment;
        def.geometry = current.geometry;
    }
    let (width, height, site) = footprint(&def)?;
    let e = doc.find_mut(id).ok_or(MarkingError::ElementNotFound(id.0))?;
    e.data = def.to_value();
    e.width = width;
    e.height = height;
    e.site = Some(site);
    Ok(())
}

/// Refreshes every marking's copy of its alignment from the alignment element of the same name,
/// and with it the marking's size and site placement. Call it after an alignment is edited or
/// re-imported. Returns the number of markings whose copy changed.
///
/// A marking whose alignment is no longer in the document keeps its old copy. One whose stations
/// no longer fit the new geometry still takes the new copy: the module then reports the error.
pub fn sync_markings(doc: &mut Document) -> usize {
    let alignments: Vec<(String, Alignment)> = doc
        .elements()
        .iter()
        .filter(|e| e.category == afrobim_alignment::CATEGORY)
        .filter_map(|e| serde_json::from_value::<Alignment>(e.data.clone()).ok().map(|a| (e.name.clone(), a)))
        .collect();
    let mut updates = Vec::new();
    for e in doc.elements().iter().filter(|e| e.category == CATEGORY) {
        let Ok(mut def) = MarkingDefinition::from_value(&e.data) else { continue };
        let Some((name, alignment)) = alignments.iter().find(|(name, _)| *name == def.alignment) else { continue };
        let mut fresh = alignment.clone();
        fresh.name = name.clone();
        if def.geometry.as_ref() == Some(&fresh) {
            continue;
        }
        def.geometry = Some(fresh);
        updates.push((e.id, def));
    }
    for (id, def) in &updates {
        let footprint = footprint(def).ok();
        if let Some(e) = doc.find_mut(*id) {
            e.data = def.to_value();
            if let Some((width, height, site)) = footprint {
                e.width = width;
                e.height = height;
                e.site = Some(site);
            }
        }
    }
    updates.len()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::catalogue::LineType;
    use afrobim_alignment::{AlignmentBuilder, Turn};
    use afrobim_kernel::ModuleRegistry;

    /// A document holding one alignment: 200 m north from (1000, 5000), then 100 m of
    /// right-hand curve of radius 300 m. First station 0.
    fn document() -> (Document, ElementId) {
        let alignment = AlignmentBuilder::new("MC10", 0.0, 1000.0, 5000.0, std::f64::consts::FRAC_PI_2).line(200.0).arc(300.0, 100.0, Turn::Right).build().unwrap();
        let mut doc = Document::default();
        let id = doc.add(afrobim_alignment::CATEGORY, "MC10", Pt::new(0.0, 0.0), 1.0, 1.0, "Alignments", serde_json::to_value(&alignment).unwrap());
        (doc, id)
    }

    fn generate(doc: &Document, id: ElementId) -> Generated {
        let mut modules = ModuleRegistry::default();
        modules.register(Box::new(MarkingModule));
        modules.generate(doc.find(id).unwrap()).unwrap()
    }

    #[test]
    fn a_marking_is_added_drawn_and_placed() {
        let (mut doc, alignment) = document();
        // Centre line 1008, 100 mm, on the alignment from 0 to 60: ten 2 m marks.
        let def = MarkingDefinition::new(LineType::Centre1008, 0.0, 60.0, 0.0, 30).with_width(100).with_material("thermoplastic");
        let id = add_marking(&mut doc, alignment, "RM-001", &def).unwrap();
        let e = doc.find(id).unwrap().clone();
        assert_eq!((e.category.as_str(), e.name.as_str(), e.layer.as_str()), (CATEGORY, "RM-001", LAYER));
        assert!(!doc.layer_visible(LAYER));
        // Heading north: the marks span x 999.95..1000.05, y 5000..5056.
        assert!((e.width - 100.0).abs() < 1e-6 && (e.height - 56_000.0).abs() < 1e-6);
        let site = e.site.clone().unwrap();
        assert!((site.x - 999_950.0).abs() < 1e-6 && (site.y - 5_000_000.0).abs() < 1e-6);
        assert_eq!((site.z, site.rotation_deg), (0.0, 0.0));
        assert_eq!((site.alignment.as_deref(), site.station_m, site.offset_m), (Some("MC10"), Some(0.0), Some(0.0)));

        let g = generate(&doc, id);
        assert!(g.notes.is_empty(), "{:?}", g.notes);
        assert_eq!(g.drawing.primitives.len(), 10);
        match &g.drawing.primitives[1] {
            Primitive::Fill { outline, colour } => {
                assert_eq!(*colour, Colour::White);
                // The second mark: 6 m to 8 m up the line, 100 mm wide. Right of north is east.
                let expected = [(100.0, 6000.0), (100.0, 8000.0), (0.0, 8000.0), (0.0, 6000.0)];
                assert_eq!(outline.len(), 4);
                for (p, q) in outline.iter().zip(expected) {
                    assert!((p.x - q.0).abs() < 1e-6 && (p.y - q.1).abs() < 1e-6, "{outline:?}");
                }
            }
            other => panic!("{other:?}"),
        }
        assert_eq!(g.drawing.extents.max, Pt2::new(e.width, e.height));
        assert_eq!(g.derived["Line type"], "Centre line, diagram 1008");
        assert_eq!(g.derived["TSRGD reference"], "S11-4-6");
        assert_eq!(g.derived["Pattern"], "2000 mm mark, 4000 mm gap");
        assert_eq!(g.derived["Speed limit"], "30 mph (48 km/h)");
        assert_eq!((g.derived["Marks"].as_str(), g.derived["Part marks"].as_str()), ("10", "0"));
        assert_eq!((g.derived["Painted length"].as_str(), g.derived["Painted area"].as_str()), ("20.000 m", "2.000 m²"));
        assert_eq!(g.derived["Stations"], "0+000.000 to 0+060.000");
        assert_eq!((g.derived["plan_origin_x_m"].as_str(), g.derived["plan_origin_y_m"].as_str()), ("999.950000", "5000.000000"));
        assert!(g.derived["Source"].contains("Figure 2-1") && g.derived["Source"].contains("2018"));
    }

    #[test]
    fn the_solids_are_closed_prisms_of_the_painted_area() {
        let (mut doc, alignment) = document();
        // Edge line 1012.1, 150 mm, 3.65 m right, across the tangent point and round the curve.
        let def = MarkingDefinition::new(LineType::Edge1012_1, 150.0, 300.0, 3.65, 60).with_width(150);
        let id = add_marking(&mut doc, alignment, "RM-001", &def).unwrap();
        let g = generate(&doc, id);
        assert_eq!(g.solids.len(), 1);
        let mesh = &g.solids[0];
        assert_eq!(mesh.colour, Colour::White.display_rgb());
        assert!(mesh.vertices.iter().all(|v| v[2] == 0.0 || v[2] == DISPLAY_THICKNESS_MM));
        // Inside of a right-hand curve: 50 + 100 x (300 - 3.65) / 300 m long.
        let length_m = 50.0 + 100.0 * 296.35 / 300.0;
        assert_eq!(g.derived["Painted length"], format!("{length_m:.3} m"));
        // The mesh volume is the chorded area x thickness: positive (faces outwards) and within
        // 0.01 % of the exact strip.
        let exact_mm3 = length_m * 1000.0 * 150.0 * DISPLAY_THICKNESS_MM;
        let volume = mesh.volume();
        assert!(volume > 0.0 && (volume - exact_mm3).abs() < exact_mm3 * 1e-4, "{volume} {exact_mm3}");
        // A long curved mark is drawn as several outlines.
        assert!(g.drawing.primitives.len() > 1);
        // A straight mark alone is an exact box.
        let id = add_marking(&mut doc, alignment, "RM-002", &MarkingDefinition::new(LineType::Edge1012_1, 0.0, 40.0, 3.65, 60).with_width(150)).unwrap();
        let g = generate(&doc, id);
        assert!((g.solids[0].volume() - 40_000.0 * 150.0 * DISPLAY_THICKNESS_MM).abs() < 1e-3);
        assert_eq!(g.solids[0].triangles.len(), 12);
    }

    #[test]
    fn a_double_line_has_a_solid_per_line() {
        let (mut doc, alignment) = document();
        let id = add_marking(&mut doc, alignment, "DWL", &MarkingDefinition::new(LineType::Double1013_1BrokenRight, 0.0, 60.0, 0.0, 60)).unwrap();
        let g = generate(&doc, id);
        assert_eq!(g.solids.iter().map(|m| m.name.as_str()).collect::<Vec<_>>(), ["Double white lines, right line broken, left line", "Double white lines, right line broken, right line"]);
        // One continuous outline and ten 1 m marks.
        assert_eq!(g.drawing.primitives.len(), 11);
        assert_eq!(g.derived["Line separation"], "175 mm (475 mm overall)");
        assert_eq!(g.derived["Width"], "150 mm each line");
        assert!((e_width(&doc, id) - 475.0).abs() < 1e-6);
    }

    fn e_width(doc: &Document, id: ElementId) -> f64 {
        doc.find(id).unwrap().width
    }

    #[test]
    fn invalid_markings_are_refused_or_reported() {
        let (mut doc, alignment) = document();
        let outside = MarkingDefinition::new(LineType::Centre1008, 250.0, 350.0, 0.0, 30).with_width(100);
        match add_marking(&mut doc, alignment, "RM-001", &outside) {
            Err(MarkingError::InvalidDefinition(message)) => assert!(message.contains("350.000") && message.contains("300.000"), "{message}"),
            other => panic!("{other:?}"),
        }
        assert_eq!(doc.elements().len(), 1);
        assert!(matches!(add_marking(&mut doc, ElementId(99), "RM-001", &outside), Err(MarkingError::ElementNotFound(99))));

        // An element that reaches the module with an error gives notes and no geometry.
        let mut bad = outside.clone();
        bad.alignment = "MC10".into();
        bad.geometry = marking_geometry(&doc, alignment);
        let id = doc.add(CATEGORY, "RM-BAD", Pt::new(0.0, 0.0), 1.0, 1.0, LAYER, bad.to_value());
        let g = generate(&doc, id);
        assert!(g.drawing.primitives.is_empty() && g.solids.is_empty());
        assert_eq!(g.notes.iter().map(|n| (n.severity, n.code.as_str())).collect::<Vec<_>>(), [(Severity::Error, "marking.stations.outside")]);

        // Data that is not a marking definition at all is an error from the module.
        let id = doc.add(CATEGORY, "RM-JUNK", Pt::new(0.0, 0.0), 1.0, 1.0, LAYER, serde_json::json!({"nonsense": true}));
        let mut modules = ModuleRegistry::default();
        modules.register(Box::new(MarkingModule));
        assert!(modules.generate(doc.find(id).unwrap()).is_err());
        // The marking is not added twice under one name.
        let ok = MarkingDefinition::new(LineType::Centre1008, 0.0, 60.0, 0.0, 30).with_width(100);
        let a = add_marking(&mut doc, alignment, "RM-001", &ok).unwrap();
        let b = add_marking(&mut doc, alignment, "RM-001", &ok).unwrap();
        assert_eq!((doc.find(a).unwrap().name.as_str(), doc.find(b).unwrap().name.as_str()), ("RM-001", "RM-002"));
    }

    fn marking_geometry(doc: &Document, alignment: ElementId) -> Option<Alignment> {
        alignment_of(doc, alignment).ok()
    }

    #[test]
    fn markings_follow_their_alignment_when_it_changes() {
        let (mut doc, alignment) = document();
        let id = add_marking(&mut doc, alignment, "RM-001", &MarkingDefinition::new(LineType::Centre1008, 0.0, 60.0, 0.0, 30).with_width(100)).unwrap();
        assert_eq!(sync_markings(&mut doc), 0);
        // The alignment is re-imported 10 m further east.
        let moved = AlignmentBuilder::new("MC10", 0.0, 1010.0, 5000.0, std::f64::consts::FRAC_PI_2).line(300.0).build().unwrap();
        doc.find_mut(alignment).unwrap().data = serde_json::to_value(&moved).unwrap();
        assert_eq!(sync_markings(&mut doc), 1);
        assert_eq!(sync_markings(&mut doc), 0);
        assert!((doc.find(id).unwrap().site.as_ref().unwrap().x - 1_009_950.0).abs() < 1e-6);
        assert_eq!(marking_of(&doc, id).unwrap().geometry.unwrap().elements.len(), 1);

        // An edit keeps the alignment and updates the footprint.
        let mut edit = MarkingDefinition::new(LineType::Centre1008, 0.0, 120.0, 0.0, 30).with_width(150);
        edit.material = "paint".into();
        set_marking(&mut doc, id, &edit).unwrap();
        let e = doc.find(id).unwrap();
        assert!((e.width - 150.0).abs() < 1e-6 && (e.height - 116_000.0).abs() < 1e-6);
        assert_eq!(marking_of(&doc, id).unwrap().alignment, "MC10");
        // A bad edit changes nothing.
        let before = doc.find(id).unwrap().clone();
        assert!(set_marking(&mut doc, id, &MarkingDefinition::new(LineType::Centre1008, 0.0, 60.0, 0.0, 30).with_width(125)).is_err());
        assert_eq!(*doc.find(id).unwrap(), before);
        assert!(matches!(set_marking(&mut doc, alignment, &edit), Err(MarkingError::NotAMarking(_))));
    }

    #[test]
    fn the_element_round_trips_through_a_saved_document() {
        let (mut doc, alignment) = document();
        let def = MarkingDefinition::new(LineType::Warning1004_1, 100.0, 290.0, -1.5, 60).with_width(150).with_material("thermoplastic");
        let id = add_marking(&mut doc, alignment, "RM-001", &def).unwrap();
        let before = generate(&doc, id);
        let element = doc.find(id).unwrap().clone();
        // Through serde values the element is identical.
        let back: Element = serde_json::from_value(serde_json::to_value(&element).unwrap()).unwrap();
        assert_eq!(back, element);
        let mut modules = ModuleRegistry::default();
        modules.register(Box::new(MarkingModule));
        assert_eq!(modules.generate(&back).unwrap(), before);
        // Through text (as saved to disk) it generates the same marking.
        let back: Element = serde_json::from_str(&serde_json::to_string(&element).unwrap()).unwrap();
        let again = modules.generate(&back).unwrap();
        assert_eq!((again.derived, again.notes, again.drawing.primitives.len()), (before.derived, before.notes, before.drawing.primitives.len()));
        assert_eq!(MarkingDefinition::from_value(&back.data).unwrap().material, "thermoplastic");
    }
}
