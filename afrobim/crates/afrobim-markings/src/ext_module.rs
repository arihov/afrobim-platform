//! The kernel module for extension markings (category `marking_ext`), the helpers that add them
//! to a document, and their quantities.

use std::collections::BTreeMap;

use afrobim_alignment::{format_station, Alignment};
use afrobim_draw::{Colour, Drawing, Extents, Mesh3, Primitive, Pt2};
use afrobim_kernel::{Document, Element, ElementId, Generated, Module, Pt, SitePlacement};

use crate::basis::Basis;
use crate::catalogue::MarkingColour;
use crate::ext_def::{ExtDefinition, ExtLaid, ExtResolved, Piece, Shape, CATEGORY_EXT};
use crate::ext_lookup::StudColour;
use crate::module::{add_prism, alignment_of, errors, DISPLAY_THICKNESS_MM, LAYER};
use crate::quantities::fixed;
use crate::MarkingError;

const SECTIONS_PER_FILL: usize = 64;

/// An extension marking in plan, in the frame of [`crate::MarkingPlan`] (millimetres, X east,
/// Y north, lower-left corner of everything drawn on the origin, which is the plan point
/// (`origin_x_m`, `origin_y_m`); solids from z = 0 to [`DISPLAY_THICKNESS_MM`], for display only).
#[derive(Debug, Clone, PartialEq)]
pub struct ExtPlan {
    pub drawing: Drawing,
    pub solids: Vec<Mesh3>,
    pub origin_x_m: f64,
    pub origin_y_m: f64,
}

fn draw_colour(colour: MarkingColour, stud: Option<StudColour>) -> Colour {
    match stud {
        Some(StudColour::White) => Colour::White,
        Some(StudColour::Red) => Colour::Red,
        Some(StudColour::Amber) => Colour::Yellow,
        Some(StudColour::Green) => Colour::Green,
        None => match colour {
            MarkingColour::White => Colour::White,
            MarkingColour::Yellow => Colour::Yellow,
        },
    }
}

fn points(shape: &Shape) -> Vec<(f64, f64)> {
    match shape {
        Shape::Strip { left, right, .. } => left.iter().chain(right.iter()).copied().collect(),
        Shape::Poly(p) => p.clone(),
    }
}

fn add_convex_prism(mesh: &mut Mesh3, poly: &[Pt2], z0: f64, thickness: f64) {
    let n = poly.len();
    if n < 3 {
        return;
    }
    let area: f64 = (0..n).map(|i| poly[i].x * poly[(i + 1) % n].y - poly[(i + 1) % n].x * poly[i].y).sum();
    let ordered: Vec<Pt2> = if area < 0.0 { poly.iter().rev().copied().collect() } else { poly.to_vec() };
    let base = mesh.vertices.len() as u32;
    for p in &ordered {
        mesh.vertices.push([p.x, p.y, z0]);
    }
    for p in &ordered {
        mesh.vertices.push([p.x, p.y, z0 + thickness]);
    }
    let n = n as u32;
    for i in 1..n - 1 {
        mesh.triangles.push([base + n, base + n + i, base + n + i + 1]);
        mesh.triangles.push([base, base + i + 1, base + i]);
    }
    for i in 0..n {
        let j = (i + 1) % n;
        mesh.triangles.push([base + i, base + j, base + n + j]);
        mesh.triangles.push([base + i, base + n + j, base + n + i]);
    }
}

/// Draws a laid-out marking in plan and builds its solids; `None` when nothing was laid.
/// `level_mm(station)` raises solids of strips to the road level (display only).
pub fn ext_plan(laid: &ExtLaid, colour: Colour, name: &str, level_mm: &dyn Fn(f64) -> f64) -> Option<ExtPlan> {
    let mut pts = laid.pieces.iter().flat_map(|p| points(&p.shape));
    let first = pts.next()?;
    let ((min_x, min_y), (max_x, max_y)) = pts.fold((first, first), |(lo, hi), p| ((lo.0.min(p.0), lo.1.min(p.1)), (hi.0.max(p.0), hi.1.max(p.1))));
    let mm = |p: &(f64, f64)| Pt2::new((p.0 - min_x) * 1000.0, (p.1 - min_y) * 1000.0);
    let mut primitives = Vec::new();
    let mut solids: Vec<Mesh3> = laid
        .groups
        .iter()
        .map(|g| Mesh3 { name: if laid.groups.len() > 1 { format!("{name}, {g}") } else { name.to_string() }, vertices: Vec::new(), triangles: Vec::new(), colour: colour.display_rgb() })
        .collect();
    for Piece { group, shape } in &laid.pieces {
        let mesh = solids.get_mut(*group)?;
        match shape {
            Shape::Strip { left, right, start_station_m, end_station_m } => {
                let (l, r): (Vec<Pt2>, Vec<Pt2>) = (left.iter().map(mm).collect(), right.iter().map(mm).collect());
                let last = (l.len().max(2) - 1) as f64;
                let levels: Vec<f64> = (0..l.len()).map(|i| level_mm(start_station_m + (end_station_m - start_station_m) * i as f64 / last)).collect();
                add_prism(mesh, &l, &r, &levels, DISPLAY_THICKNESS_MM);
                let mut from = 0;
                while from + 1 < l.len() {
                    let to = (from + SECTIONS_PER_FILL).min(l.len() - 1);
                    primitives.push(Primitive::Fill { outline: r[from..=to].iter().chain(l[from..=to].iter().rev()).copied().collect(), colour });
                    from = to;
                }
            }
            Shape::Poly(p) => {
                let poly: Vec<Pt2> = p.iter().map(mm).collect();
                add_convex_prism(mesh, &poly, 0.0, DISPLAY_THICKNESS_MM);
                primitives.push(Primitive::Fill { outline: poly, colour });
            }
        }
    }
    solids.retain(|m| !m.vertices.is_empty());
    let extents = Extents { min: Pt2::new(0.0, 0.0), max: Pt2::new((max_x - min_x) * 1000.0, (max_y - min_y) * 1000.0) };
    Some(ExtPlan { drawing: Drawing { primitives, extents }, solids, origin_x_m: min_x, origin_y_m: min_y })
}

fn derived(def: &ExtDefinition, resolved: &ExtResolved, laid: &ExtLaid, origin_x_m: f64, origin_y_m: f64) -> BTreeMap<String, String> {
    let mut out = BTreeMap::new();
    for (k, v) in &resolved.derived {
        out.insert(k.clone(), v.clone());
    }
    out.insert("Alignment".into(), def.alignment.clone());
    out.insert("Station".into(), match def.end_station_m {
        Some(e) => format!("{} to {}", format_station(def.station_m, 3), format_station(e, 3)),
        None => format_station(def.station_m, 3),
    });
    out.insert("Offset".into(), format!("{} m", fixed(def.offset_m, 3, 3)));
    if let Some(to) = def.across_to_offset_m {
        out.insert("Across to offset".into(), format!("{} m", fixed(to, 3, 3)));
    }
    if def.rotation_deg != 0.0 {
        out.insert("Rotation".into(), format!("{} deg", fixed(def.rotation_deg, 0, 3)));
    }
    out.insert("Colour".into(), resolved.stud_colour.map_or(resolved.colour.label(), StudColour::label).to_string());
    out.insert("Number".into(), laid.number.to_string());
    out.insert("Part marks".into(), laid.part_marks.to_string());
    out.insert("Painted length".into(), format!("{} m", fixed(laid.painted_length_m, 3, 3)));
    out.insert("Painted area".into(), laid.painted_area_m2.map_or("not available (placeholder block or studs)".to_string(), |a| format!("{} m²", fixed(a, 3, 3))));
    if let Some(b) = laid.bounding_area_m2 {
        out.insert("Block area".into(), format!("{} m²", fixed(b, 3, 3)));
    }
    out.insert("plan_origin_x_m".into(), format!("{origin_x_m:.6}"));
    out.insert("plan_origin_y_m".into(), format!("{origin_y_m:.6}"));
    out
}

/// The kernel module for extension markings (category [`CATEGORY_EXT`]).
///
/// `generate` resolves the element's [`ExtDefinition`] against the catalogue, lays it out, and
/// returns the plan drawing, the solids and the derived values: `Basis`, `Marking`,
/// `TSRGD reference`, dimensions, `Alignment`, `Station`, `Offset`, `Colour`, `Number`,
/// `Part marks`, `Painted length`, `Painted area`, `Source`, `Reading notes`, the caller's speed
/// note and any UK fallback note, `plan_origin_x_m` and `plan_origin_y_m`.
#[derive(Debug, Clone, Copy, Default)]
pub struct ExtMarkingModule;

impl Module for ExtMarkingModule {
    fn category(&self) -> &str {
        CATEGORY_EXT
    }

    fn display_name(&self) -> &str {
        "Road markings (second set)"
    }

    fn generate(&self, element: &Element) -> Result<Generated, String> {
        let def = ExtDefinition::from_value(&element.data).map_err(|e| format!("'{}': {e}", element.name))?;
        let (laid, notes) = def.lay_out();
        let Some((resolved, laid)) = laid else {
            return Ok(Generated { notes, ..Generated::default() });
        };
        let profile = def.geometry.as_ref().and_then(|a| a.profile.as_ref());
        let level = |station: f64| profile.and_then(|p| p.range().and_then(|(lo, hi)| p.elevation_at(station.clamp(lo, hi)).ok())).unwrap_or(0.0) * 1000.0;
        let colour = draw_colour(resolved.colour, resolved.stud_colour);
        let Some(plan) = ext_plan(&laid, colour, &resolved.name, &level) else {
            return Ok(Generated { notes, ..Generated::default() });
        };
        Ok(Generated { drawing: plan.drawing, notes, derived: derived(&def, &resolved, &laid, plan.origin_x_m, plan.origin_y_m), solids: plan.solids })
    }
}

fn footprint(def: &ExtDefinition) -> Result<(f64, f64, SitePlacement), MarkingError> {
    let (laid, notes) = def.lay_out();
    let (resolved, laid): (ExtResolved, ExtLaid) = match (laid, errors(&notes)) {
        (Some(l), None) => l,
        (_, message) => return Err(MarkingError::InvalidDefinition(message.unwrap_or_else(|| "the marking cannot be laid out".to_string()))),
    };
    let plan = ext_plan(&laid, draw_colour(resolved.colour, resolved.stud_colour), &resolved.name, &|_| 0.0).ok_or_else(|| MarkingError::InvalidDefinition("the marking has no marks".to_string()))?;
    let site = SitePlacement {
        x: plan.origin_x_m * 1000.0,
        y: plan.origin_y_m * 1000.0,
        z: 0.0,
        rotation_deg: 0.0,
        alignment: Some(def.alignment.clone()),
        station_m: Some(def.station_m),
        offset_m: Some(def.offset_m),
    };
    Ok((plan.drawing.extents.width().max(1.0), plan.drawing.extents.height().max(1.0), site))
}

/// Adds an extension marking on the alignment element `alignment_id` and returns its id.
/// `definition.alignment` and `definition.geometry` are filled in; nothing is added when the
/// definition has an Error note (the error carries the messages, among them "not defined in the
/// Uganda manual" for a type the Uganda basis does not define). The element goes on layer
/// [`LAYER`], hidden on the sheet when this call creates the layer, with a site placement at the
/// plan position of its drawing origin.
pub fn add_ext_marking(doc: &mut Document, alignment_id: ElementId, name: &str, definition: &ExtDefinition) -> Result<ElementId, MarkingError> {
    let alignment = alignment_of(doc, alignment_id)?;
    let mut def = definition.clone();
    def.alignment = alignment.name.clone();
    def.geometry = Some(alignment);
    let (width, height, site) = footprint(&def)?;
    let new_layer = !doc.layers().iter().any(|l| l.name == LAYER);
    let name = doc.unique_name(if name.trim().is_empty() { "RX-001" } else { name.trim() });
    let id = doc.add(CATEGORY_EXT, &name, Pt::new(0.0, 0.0), width, height, LAYER, def.to_value());
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

/// The definition of an extension marking element.
pub fn ext_marking_of(doc: &Document, id: ElementId) -> Option<ExtDefinition> {
    let e = doc.find(id)?;
    if e.category != CATEGORY_EXT {
        return None;
    }
    ExtDefinition::from_value(&e.data).ok()
}

/// Replaces the definition of an extension marking, keeping its alignment. Nothing changes when
/// the new definition has an Error note.
pub fn set_ext_marking(doc: &mut Document, id: ElementId, definition: &ExtDefinition) -> Result<(), MarkingError> {
    let current = ext_marking_of(doc, id).ok_or(MarkingError::NotAMarking(id.0))?;
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

/// Refreshes every extension marking's copy of its alignment (call after alignments change).
/// Returns the number of markings whose copy changed.
pub fn sync_ext_markings(doc: &mut Document) -> usize {
    let alignments: Vec<(String, Alignment)> = doc
        .elements()
        .iter()
        .filter(|e| e.category == afrobim_alignment::CATEGORY)
        .filter_map(|e| serde_json::from_value::<Alignment>(e.data.clone()).ok().map(|a| (e.name.clone(), a)))
        .collect();
    let mut updates = Vec::new();
    for e in doc.elements().iter().filter(|e| e.category == CATEGORY_EXT) {
        let Ok(mut def) = ExtDefinition::from_value(&e.data) else { continue };
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
        let fp = footprint(def).ok();
        if let Some(e) = doc.find_mut(*id) {
            e.data = def.to_value();
            if let Some((width, height, site)) = fp {
                e.width = width;
                e.height = height;
                e.site = Some(site);
            }
        }
    }
    updates.len()
}

// ------------------------------------------------------------------------------ quantities

/// One extension marking measured.
#[derive(Debug, Clone, PartialEq)]
pub struct ExtQuantities {
    pub mark: String,
    pub type_id: String,
    pub basis: Basis,
    pub diagram: String,
    pub description: String,
    pub colour: String,
    pub material: String,
    pub alignment: String,
    pub station_m: f64,
    pub end_station_m: Option<f64>,
    pub offset_m: f64,
    pub overall_length_m: f64,
    /// Whole marks, bars, stripes, arrows or studs.
    pub number: usize,
    pub part_marks: usize,
    pub painted_length_m: f64,
    /// `None` for a placeholder block and for studs.
    pub painted_area_m2: Option<f64>,
    /// Area of the block of a placeholder.
    pub block_area_m2: Option<f64>,
    pub placeholder: bool,
    /// The caller's speed note and any UK fallback reason, as written on the element.
    pub speed_note: String,
    pub uk_fallback: String,
}

impl ExtQuantities {
    pub fn measure(mark: &str, def: &ExtDefinition) -> Result<ExtQuantities, MarkingError> {
        let (laid, notes) = def.lay_out();
        let Some((resolved, laid)) = laid else {
            return Err(MarkingError::InvalidDefinition(errors(&notes).unwrap_or_else(|| "the marking cannot be laid out".to_string())));
        };
        Ok(ExtQuantities {
            mark: mark.to_string(),
            type_id: resolved.id.clone(),
            basis: def.basis,
            diagram: resolved.diagram.clone(),
            description: resolved.name.clone(),
            colour: resolved.stud_colour.map_or(resolved.colour.label(), StudColour::label).to_string(),
            material: def.material.clone(),
            alignment: def.alignment.clone(),
            station_m: def.station_m,
            end_station_m: def.end_station_m,
            offset_m: def.offset_m,
            overall_length_m: laid.overall_length_m,
            number: laid.number,
            part_marks: laid.part_marks,
            painted_length_m: laid.painted_length_m,
            painted_area_m2: laid.painted_area_m2,
            block_area_m2: laid.bounding_area_m2,
            placeholder: laid.placeholder,
            speed_note: def.speed_note.clone(),
            uk_fallback: def.uk_fallback.clone().unwrap_or_default(),
        })
    }
}

/// A marking that could not be measured.
#[derive(Debug, Clone, PartialEq)]
pub struct SkippedExt {
    pub mark: String,
    pub reason: String,
}

/// The take-off of the extension markings of a document.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct ExtTakeoff {
    pub rows: Vec<ExtQuantities>,
    pub skipped: Vec<SkippedExt>,
}

impl ExtTakeoff {
    /// Measures every element of category `marking_ext`; other elements are ignored.
    pub fn measure(elements: &[Element]) -> ExtTakeoff {
        let mut out = ExtTakeoff::default();
        for e in elements.iter().filter(|e| e.category == CATEGORY_EXT) {
            match ExtDefinition::from_value(&e.data).and_then(|def| ExtQuantities::measure(&e.name, &def)) {
                Ok(row) => out.rows.push(row),
                Err(reason) => out.skipped.push(SkippedExt { mark: e.name.clone(), reason: reason.to_string() }),
            }
        }
        out
    }

    /// Painted area of every row that has one, m2.
    pub fn painted_area_m2(&self) -> f64 {
        self.rows.iter().filter_map(|r| r.painted_area_m2).sum()
    }

    pub fn painted_length_m(&self) -> f64 {
        self.rows.iter().map(|r| r.painted_length_m).sum()
    }

    pub fn number(&self) -> usize {
        self.rows.iter().map(|r| r.number).sum()
    }
}

pub const EXT_CSV_HEADER: &str = "mark,type,basis,diagram,description,colour,material,alignment,station_m,end_station_m,offset_m,overall_length_m,number,part_marks,painted_length_m,painted_area_m2,block_area_m2,placeholder,speed_note,uk_fallback";

fn csv(s: &str) -> String {
    if s.contains([',', '"', '\n']) {
        format!("\"{}\"", s.replace('"', "\"\""))
    } else {
        s.to_string()
    }
}

/// Schedule of extension markings, one row per marking, then a `TOTAL` row (count of markings,
/// summed length, number, painted area of the rows that have one) and a `NOT MEASURED` row when
/// any marking was skipped. Lines end with `\n`; lengths and areas to 3 decimals; an empty
/// painted area means none is available (placeholder block or studs).
pub fn ext_markings_to_csv(t: &ExtTakeoff) -> String {
    let mut out = String::from(EXT_CSV_HEADER);
    out.push('\n');
    for r in &t.rows {
        let fields = [
            csv(&r.mark),
            csv(&r.type_id),
            csv(r.basis.label()),
            csv(&r.diagram),
            csv(&r.description),
            r.colour.clone(),
            csv(&r.material),
            csv(&r.alignment),
            fixed(r.station_m, 3, 3),
            r.end_station_m.map_or(String::new(), |e| fixed(e, 3, 3)),
            fixed(r.offset_m, 3, 3),
            fixed(r.overall_length_m, 3, 3),
            r.number.to_string(),
            r.part_marks.to_string(),
            fixed(r.painted_length_m, 3, 3),
            r.painted_area_m2.map_or(String::new(), |a| fixed(a, 3, 3)),
            r.block_area_m2.map_or(String::new(), |a| fixed(a, 3, 3)),
            r.placeholder.to_string(),
            csv(&r.speed_note),
            csv(&r.uk_fallback),
        ];
        out.push_str(&fields.join(","));
        out.push('\n');
    }
    out.push_str(&format!("TOTAL,{},,,,,,,,,,,{},,{},{},,,,\n", t.rows.len(), t.number(), fixed(t.painted_length_m(), 3, 3), fixed(t.painted_area_m2(), 3, 3)));
    if !t.skipped.is_empty() {
        out.push_str(&format!("NOT MEASURED (invalid definition),{},,,,,,,,,,,,,,,,,,\n", t.skipped.len()));
    }
    out
}
