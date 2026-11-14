//! The surface and the corridor as kernel elements, and the helpers that put them in a document.
//!
//! A kernel module sees one element at a time, never the document. So:
//!
//! - a **surface** element (category `"surface"`) holds its whole TIN, and its module draws it;
//! - a **corridor** element (category `"corridor"`) holds the ids of its alignment and surface
//!   elements, the definition, and a summary of the last computation (plan lines and totals).
//!   Its module draws that summary. The summary is written by [`add_corridor`] and
//!   [`refresh_corridor`]; after the alignment, the surface or the definition changes it is stale
//!   until `refresh_corridor` is called again. Nothing detects that for you.

use std::collections::{BTreeMap, HashSet};
use std::path::Path;

use afrobim_alignment::Alignment;
use afrobim_draw::{Colour, Drawing, Extents, Mesh3, Primitive, Pt2};
use afrobim_kernel::{Document, Element, ElementId, Generated, Module, Note, Pt, Severity, SitePlacement};
use serde::{Deserialize, Serialize};

use crate::landxml::read_landxml_surfaces;
use crate::output::{plan_lines, PlanLines};
use crate::section::{Corridor, CorridorError};
use crate::template::CorridorDefinition;
use crate::tin::Tin;
use crate::volumes::earthworks;

/// The element category of ground surfaces.
pub const SURFACE_CATEGORY: &str = "surface";
/// The element category of corridors.
pub const CORRIDOR_CATEGORY: &str = "corridor";
/// The layer [`add_surface`] puts surfaces on.
pub const SURFACES_LAYER: &str = "Surfaces";
/// The layer [`add_corridor`] puts corridors on.
pub const CORRIDORS_LAYER: &str = "Corridors";

/// The most triangles a surface is shown with unless [`SurfaceModule::with_display_cap`] says
/// otherwise. A display limit only: calculations always use the whole TIN.
pub const DISPLAY_TRIANGLE_CAP: usize = 20_000;

/// Display colour of the ground mesh. For screens only.
const GROUND_RGB: [u8; 3] = [0x8A, 0x9A, 0x6B];

/// The `data` of a surface element.
///
/// The TIN is stored **inline**, in the compact encoding described on [`crate::TinEncoded`]:
/// 32 characters of JSON per point and 16 per triangle, not compressed. A 100 000-point,
/// 200 000-triangle survey adds about 6.4 MB to the document file.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SurfaceDefinition {
    pub tin: Tin,
    /// Where the surface came from (a file name), for the record. Not read back.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub source: Option<String>,
}

/// The `data` of a corridor element.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CorridorElement {
    /// The alignment element (category `"alignment"`); it must carry a profile.
    pub alignment: ElementId,
    /// The surface element (category `"surface"`).
    pub surface: ElementId,
    pub definition: CorridorDefinition,
    /// The last computation, see the module documentation.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub computed: Option<CorridorSummary>,
}

/// What is kept of a computed corridor in its element.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CorridorSummary {
    pub plan: PlanLines,
    pub start_station_m: f64,
    pub end_station_m: f64,
    pub sections: usize,
    /// Sections with no cut and fill areas (no ground or no daylight).
    pub sections_without_areas: usize,
    pub total_cut_m3: f64,
    pub total_fill_m3: f64,
    pub unmeasured_length_m: f64,
    pub notes: Vec<String>,
}

fn site_at(x_m: f64, y_m: f64) -> SitePlacement {
    SitePlacement { x: x_m * 1000.0, y: y_m * 1000.0, z: 0.0, rotation_deg: 0.0, alignment: None, station_m: None, offset_m: None }
}

fn note(severity: Severity, code: &str, message: String) -> Note {
    Note { severity, code: code.to_string(), message }
}

/// The kernel module for ground surfaces.
///
/// `generate` returns:
///
/// - the **plan drawing** in millimetres with the surface's south-west corner (`min_x`, `min_y`)
///   on the origin: the boundary edges (edges of one triangle only) as black hairlines, and, when
///   the surface has no more triangles than the display cap, every other triangle edge in grey;
/// - one **mesh**, `Ground surface`: X east and Y north in millimetres from the same corner, Z
///   the elevation itself in millimetres, triangles counter-clockwise seen from above. An open
///   surface with no thickness. With no more triangles than the cap it is the TIN itself.
///   Above the cap it is a **resampling**: the TIN's elevation at the nodes of a regular grid
///   over its bounds, sized to stay under the cap, two triangles per grid square whose four
///   corners are all on the surface. That mesh is for looking at only; an Info note says so;
/// - derived values: `points`, `triangles`, `plan_area_m2`, `min_level_m`, `max_level_m`,
///   `plan_origin_x_m`, `plan_origin_y_m`, `width_m`, `height_m`, `display_triangles`.
#[derive(Debug, Clone, Copy)]
pub struct SurfaceModule {
    display_cap: usize,
}

impl Default for SurfaceModule {
    fn default() -> Self {
        SurfaceModule { display_cap: DISPLAY_TRIANGLE_CAP }
    }
}

impl SurfaceModule {
    /// A module that shows at most `triangles` triangles of a surface (at least 2).
    pub fn with_display_cap(triangles: usize) -> SurfaceModule {
        SurfaceModule { display_cap: triangles.max(2) }
    }
}

/// The mesh a surface is shown with; see [`SurfaceModule`]. The flag is true when it is a
/// resampling and not the TIN itself.
pub fn surface_display_mesh(tin: &Tin, triangle_cap: usize) -> (Mesh3, bool) {
    let b = tin.bounds();
    let mm = |p: [f64; 3]| [(p[0] - b.min_x) * 1000.0, (p[1] - b.min_y) * 1000.0, p[2] * 1000.0];
    let mut mesh = Mesh3 { name: "Ground surface".to_string(), vertices: Vec::new(), triangles: Vec::new(), colour: GROUND_RGB };
    if tin.triangles().len() <= triangle_cap {
        mesh.vertices = tin.points().iter().map(|p| mm(*p)).collect();
        mesh.triangles = tin
            .triangles()
            .iter()
            .map(|t| {
                let [p, q, r] = t.map(|i| tin.points()[i as usize]);
                let ccw = (q[0] - p[0]) * (r[1] - p[1]) - (q[1] - p[1]) * (r[0] - p[0]) > 0.0;
                if ccw {
                    *t
                } else {
                    [t[0], t[2], t[1]]
                }
            })
            .collect();
        return (mesh, false);
    }
    let (w, h) = ((b.max_x - b.min_x).max(1e-9), (b.max_y - b.min_y).max(1e-9));
    let squares = (triangle_cap / 2).max(1) as f64;
    let nx = ((squares * w / h).sqrt().floor() as usize).max(1);
    let ny = ((squares / nx as f64).floor() as usize).max(1);
    let mut node: Vec<Option<u32>> = Vec::with_capacity((nx + 1) * (ny + 1));
    for j in 0..=ny {
        for i in 0..=nx {
            let (x, y) = (b.min_x + w * i as f64 / nx as f64, b.min_y + h * j as f64 / ny as f64);
            node.push(tin.elevation_at(x, y).map(|z| {
                mesh.vertices.push(mm([x, y, z]));
                mesh.vertices.len() as u32 - 1
            }));
        }
    }
    for j in 0..ny {
        for i in 0..nx {
            let at = |i: usize, j: usize| node[j * (nx + 1) + i];
            if let (Some(a), Some(b), Some(c), Some(d)) = (at(i, j), at(i + 1, j), at(i + 1, j + 1), at(i, j + 1)) {
                mesh.triangles.extend([[a, b, c], [a, c, d]]);
            }
        }
    }
    (mesh, true)
}

impl Module for SurfaceModule {
    fn category(&self) -> &str {
        SURFACE_CATEGORY
    }

    fn display_name(&self) -> &str {
        "Ground surfaces"
    }

    fn generate(&self, element: &Element) -> Result<Generated, String> {
        let def: SurfaceDefinition =
            serde_json::from_value(element.data.clone()).map_err(|e| format!("'{}' is not a valid surface: {e}", element.name))?;
        let tin = &def.tin;
        let b = tin.bounds();
        let pt = |i: u32| {
            let p = tin.points()[i as usize];
            Pt2::new((p[0] - b.min_x) * 1000.0, (p[1] - b.min_y) * 1000.0)
        };
        let boundary = tin.boundary_edges();
        let mut primitives: Vec<Primitive> = Vec::new();
        let whole = tin.triangles().len() <= self.display_cap;
        if whole {
            let outer: HashSet<[u32; 2]> = boundary.iter().copied().collect();
            let mut seen: HashSet<[u32; 2]> = HashSet::with_capacity(tin.triangles().len() * 2);
            for t in tin.triangles() {
                for k in 0..3 {
                    let e = [t[k].min(t[(k + 1) % 3]), t[k].max(t[(k + 1) % 3])];
                    if !outer.contains(&e) && seen.insert(e) {
                        primitives.push(Primitive::Line { from: pt(e[0]), to: pt(e[1]), width: 0.0, colour: Some(Colour::Grey) });
                    }
                }
            }
        }
        primitives.extend(boundary.iter().map(|e| Primitive::Line { from: pt(e[0]), to: pt(e[1]), width: 0.0, colour: None }));
        let (mesh, resampled) = surface_display_mesh(tin, self.display_cap);

        let mut derived = BTreeMap::new();
        let mut put = |k: &str, v: String| {
            derived.insert(k.to_string(), v);
        };
        put("points", tin.points().len().to_string());
        put("triangles", tin.triangles().len().to_string());
        put("plan_area_m2", format!("{:.3}", tin.plan_area()));
        put("min_level_m", format!("{:.3}", b.min_z));
        put("max_level_m", format!("{:.3}", b.max_z));
        put("plan_origin_x_m", format!("{:.6}", b.min_x));
        put("plan_origin_y_m", format!("{:.6}", b.min_y));
        put("width_m", format!("{:.3}", b.max_x - b.min_x));
        put("height_m", format!("{:.3}", b.max_y - b.min_y));
        put("display_triangles", mesh.triangles.len().to_string());
        let mut notes = Vec::new();
        if resampled {
            notes.push(note(
                Severity::Info,
                "surface.display_resampled",
                format!(
                    "The surface has {} triangles and is shown as a regular grid of {} triangles sampled from it, with its boundary only in plan. Calculations use the whole surface.",
                    tin.triangles().len(),
                    mesh.triangles.len()
                ),
            ));
        }
        let extents = Extents { min: Pt2::new(0.0, 0.0), max: Pt2::new((b.max_x - b.min_x) * 1000.0, (b.max_y - b.min_y) * 1000.0) };
        Ok(Generated { drawing: Drawing { primitives, extents }, notes, derived, solids: vec![mesh] })
    }
}

/// The kernel module for corridors.
///
/// `generate` draws the stored summary in plan, in millimetres with the lower-left of the lines
/// on the origin: the centreline in grey, the two template edges in black, the daylight lines in
/// green, all hairlines. Derived values: `template_regions`, `interval_m`, and from the summary
/// `start_station_m`, `end_station_m`, `sections`, `sections_without_areas`, `total_cut_m3`,
/// `total_fill_m3`, `unmeasured_length_m`, `plan_origin_x_m`, `plan_origin_y_m`. It makes no
/// solids: the 3D surfaces come from [`crate::corridor_meshes`], which needs the built corridor.
///
/// Notes: an Error when the definition is not valid; an Info when there is no summary yet; a
/// Warning for each note of the last computation (sections without daylight, limits applied).
#[derive(Debug, Clone, Copy, Default)]
pub struct CorridorModule;

fn plan_bounds(plan: &PlanLines) -> Option<(f64, f64, f64, f64)> {
    let all = plan.centreline.iter().chain(&plan.left_edge).chain(&plan.right_edge).chain(plan.left_daylight.iter().flatten()).chain(plan.right_daylight.iter().flatten());
    let mut b: Option<(f64, f64, f64, f64)> = None;
    for &(x, y) in all {
        let (x0, y0, x1, y1) = b.unwrap_or((x, y, x, y));
        b = Some((x0.min(x), y0.min(y), x1.max(x), y1.max(y)));
    }
    b
}

impl Module for CorridorModule {
    fn category(&self) -> &str {
        CORRIDOR_CATEGORY
    }

    fn display_name(&self) -> &str {
        "Corridors"
    }

    fn generate(&self, element: &Element) -> Result<Generated, String> {
        let data: CorridorElement =
            serde_json::from_value(element.data.clone()).map_err(|e| format!("'{}' is not a valid corridor: {e}", element.name))?;
        let mut generated = Generated::default();
        let mut put = |k: &str, v: String| {
            generated.derived.insert(k.to_string(), v);
        };
        put("template_regions", data.definition.regions.len().to_string());
        put("interval_m", format!("{}", data.definition.interval_m));
        if let Err(why) = data.definition.check() {
            generated.notes.push(note(Severity::Error, "corridor.definition", format!("Corridor '{}': {why}.", element.name)));
        }
        let Some(summary) = &data.computed else {
            generated.notes.push(note(
                Severity::Info,
                "corridor.not_computed",
                format!("Corridor '{}' has not been computed; nothing is drawn until it is.", element.name),
            ));
            return Ok(generated);
        };
        put("start_station_m", format!("{:.3}", summary.start_station_m));
        put("end_station_m", format!("{:.3}", summary.end_station_m));
        put("sections", summary.sections.to_string());
        put("sections_without_areas", summary.sections_without_areas.to_string());
        put("total_cut_m3", format!("{:.3}", summary.total_cut_m3));
        put("total_fill_m3", format!("{:.3}", summary.total_fill_m3));
        put("unmeasured_length_m", format!("{:.3}", summary.unmeasured_length_m));
        generated.notes.extend(summary.notes.iter().map(|n| note(Severity::Warning, "corridor.computation", n.clone())));
        let Some((x0, y0, x1, y1)) = plan_bounds(&summary.plan) else { return Ok(generated) };
        put("plan_origin_x_m", format!("{x0:.6}"));
        put("plan_origin_y_m", format!("{y0:.6}"));
        let pt = |p: (f64, f64)| Pt2::new((p.0 - x0) * 1000.0, (p.1 - y0) * 1000.0);
        let mut polyline = |line: &[(f64, f64)], colour: Option<Colour>| {
            generated.drawing.primitives.extend(line.windows(2).map(|w| Primitive::Line { from: pt(w[0]), to: pt(w[1]), width: 0.0, colour }));
        };
        polyline(&summary.plan.centreline, Some(Colour::Grey));
        polyline(&summary.plan.left_edge, None);
        polyline(&summary.plan.right_edge, None);
        for piece in summary.plan.left_daylight.iter().chain(&summary.plan.right_daylight) {
            polyline(piece, Some(Colour::Green));
        }
        generated.drawing.extents = Extents { min: Pt2::new(0.0, 0.0), max: pt((x1, y1)) };
        Ok(generated)
    }
}

/// Adds a surface to a document as an element on the `Surfaces` layer and returns its id.
///
/// The element's site placement is the surface's south-west corner, so its plan drawing and its
/// mesh sit in site coordinates. Like alignments, the layer is hidden on the design sheet: a
/// surface is kilometres from where sign faces are laid out. `source` is recorded only.
pub fn add_surface(doc: &mut Document, name: &str, tin: &Tin, source: Option<&str>) -> Result<ElementId, CorridorError> {
    let b = tin.bounds();
    let name = doc.unique_name(if name.trim().is_empty() { "Surface" } else { name.trim() });
    let data = serde_json::to_value(SurfaceDefinition { tin: tin.clone(), source: source.map(str::to_string) })
        .map_err(|e| CorridorError::Document(format!("the surface cannot be stored: {e}")))?;
    let (w, h) = ((b.max_x - b.min_x) * 1000.0, (b.max_y - b.min_y) * 1000.0);
    let id = doc.add(SURFACE_CATEGORY, &name, Pt::new(0.0, 0.0), w.max(1.0), h.max(1.0), SURFACES_LAYER, data);
    if let Some(e) = doc.find_mut(id) {
        e.site = Some(site_at(b.min_x, b.min_y));
    }
    hide_layer(doc, SURFACES_LAYER);
    Ok(id)
}

fn hide_layer(doc: &mut Document, name: &str) {
    if let Some(layer) = doc.layers_mut().iter_mut().find(|l| l.name == name) {
        layer.visible = false;
    }
}

/// Imports every TIN surface of a LandXML file as an element. Returns the ids added and the
/// reader's notes (surfaces left out, faces skipped).
pub fn import_landxml_surfaces(doc: &mut Document, path: &Path) -> Result<(Vec<ElementId>, Vec<String>), String> {
    let import = read_landxml_surfaces(path).map_err(|e| e.to_string())?;
    let source = path.file_name().map(|n| n.to_string_lossy().to_string());
    let mut ids = Vec::new();
    for s in &import.surfaces {
        ids.push(add_surface(doc, &s.name, &s.tin, source.as_deref()).map_err(|e| e.to_string())?);
    }
    Ok((ids, import.notes))
}

/// The surfaces in a document: element id and name.
pub fn surfaces(doc: &Document) -> Vec<(ElementId, String)> {
    doc.elements().iter().filter(|e| e.category == SURFACE_CATEGORY).map(|e| (e.id, e.name.clone())).collect()
}

/// The TIN of a surface element. Decoding a large surface takes time: keep the result.
pub fn surface_of(doc: &Document, id: ElementId) -> Option<Tin> {
    let e = doc.find(id).filter(|e| e.category == SURFACE_CATEGORY)?;
    serde_json::from_value::<SurfaceDefinition>(e.data.clone()).ok().map(|d| d.tin)
}

/// The stored data of a corridor element.
pub fn corridor_element(doc: &Document, id: ElementId) -> Option<CorridorElement> {
    let e = doc.find(id).filter(|e| e.category == CORRIDOR_CATEGORY)?;
    serde_json::from_value(e.data.clone()).ok()
}

fn alignment_of(doc: &Document, id: ElementId) -> Result<Alignment, CorridorError> {
    let e = doc
        .find(id)
        .filter(|e| e.category == afrobim_alignment::CATEGORY)
        .ok_or_else(|| CorridorError::Document(format!("element {} is not an alignment in the document", id.0)))?;
    serde_json::from_value(e.data.clone()).map_err(|err| CorridorError::Document(format!("alignment '{}' cannot be read: {err}", e.name)))
}

/// Builds the corridor of a corridor element from the alignment and surface it refers to.
/// Nothing is stored. Decoding the surface and building take time on a large survey: keep the
/// result and use [`build_corridor`] with a TIN you already hold when you can.
pub fn corridor_of(doc: &Document, id: ElementId) -> Result<Corridor, CorridorError> {
    let data = corridor_element(doc, id).ok_or_else(|| CorridorError::Document(format!("element {} is not a corridor in the document", id.0)))?;
    let tin = surface_of(doc, data.surface).ok_or_else(|| CorridorError::Document(format!("element {} is not a surface in the document", data.surface.0)))?;
    Corridor::build(&data.definition, &alignment_of(doc, data.alignment)?, &tin)
}

/// The same as [`corridor_of`] with the ground supplied by the caller, for a front end that
/// keeps decoded surfaces.
pub fn build_corridor(doc: &Document, id: ElementId, ground: &Tin) -> Result<Corridor, CorridorError> {
    let data = corridor_element(doc, id).ok_or_else(|| CorridorError::Document(format!("element {} is not a corridor in the document", id.0)))?;
    Corridor::build(&data.definition, &alignment_of(doc, data.alignment)?, ground)
}

fn summarise(corridor: &Corridor, definition: &CorridorDefinition) -> CorridorSummary {
    let e = earthworks(&corridor.sections, &definition.volumes);
    CorridorSummary {
        plan: plan_lines(corridor),
        start_station_m: corridor.start_station_m,
        end_station_m: corridor.end_station_m,
        sections: corridor.sections.len(),
        sections_without_areas: corridor.sections.iter().filter(|s| s.cut_area_m2.is_none()).count(),
        total_cut_m3: e.total_cut_m3,
        total_fill_m3: e.total_fill_m3,
        unmeasured_length_m: e.unmeasured_length_m,
        notes: corridor.notes.clone(),
    }
}

fn store(doc: &mut Document, id: ElementId, mut data: CorridorElement, corridor: &Corridor) -> Result<(), CorridorError> {
    let summary = summarise(corridor, &data.definition);
    let bounds = plan_bounds(&summary.plan);
    data.computed = Some(summary);
    let value = serde_json::to_value(&data).map_err(|e| CorridorError::Document(format!("the corridor cannot be stored: {e}")))?;
    let e = doc.find_mut(id).ok_or_else(|| CorridorError::Document(format!("element {} is not in the document", id.0)))?;
    e.data = value;
    if let Some((x0, y0, x1, y1)) = bounds {
        e.width = ((x1 - x0) * 1000.0).max(1.0);
        e.height = ((y1 - y0) * 1000.0).max(1.0);
        e.site = Some(site_at(x0, y0));
    }
    Ok(())
}

/// Builds a corridor on an alignment element and a surface element and adds it to the document
/// as an element on the `Corridors` layer (hidden on the design sheet, like alignments), with
/// its summary stored. Returns the element id and the built corridor. Nothing is added when the
/// corridor cannot be built.
pub fn add_corridor(
    doc: &mut Document,
    alignment: ElementId,
    surface: ElementId,
    definition: CorridorDefinition,
) -> Result<(ElementId, Corridor), CorridorError> {
    let tin = surface_of(doc, surface).ok_or_else(|| CorridorError::Document(format!("element {} is not a surface in the document", surface.0)))?;
    let corridor = Corridor::build(&definition, &alignment_of(doc, alignment)?, &tin)?;
    let name = doc.unique_name(if definition.name.trim().is_empty() { "Corridor" } else { definition.name.trim() });
    let id = doc.add(CORRIDOR_CATEGORY, &name, Pt::new(0.0, 0.0), 1.0, 1.0, CORRIDORS_LAYER, serde_json::Value::Null);
    store(doc, id, CorridorElement { alignment, surface, definition, computed: None }, &corridor)?;
    hide_layer(doc, CORRIDORS_LAYER);
    Ok((id, corridor))
}

/// Rebuilds a corridor element from its alignment and surface as they are now and stores the
/// new summary. Call it after any of them, or the definition, has changed.
pub fn refresh_corridor(doc: &mut Document, id: ElementId) -> Result<Corridor, CorridorError> {
    let corridor = corridor_of(doc, id)?;
    let data = corridor_element(doc, id).ok_or_else(|| CorridorError::Document(format!("element {} is not a corridor in the document", id.0)))?;
    store(doc, id, data, &corridor)?;
    Ok(corridor)
}

/// Replaces the definition of a corridor element and rebuilds it. On an error the element is
/// left as it was.
pub fn set_corridor_definition(doc: &mut Document, id: ElementId, definition: CorridorDefinition) -> Result<Corridor, CorridorError> {
    let mut data = corridor_element(doc, id).ok_or_else(|| CorridorError::Document(format!("element {} is not a corridor in the document", id.0)))?;
    let tin = surface_of(doc, data.surface).ok_or_else(|| CorridorError::Document(format!("element {} is not a surface in the document", data.surface.0)))?;
    let corridor = Corridor::build(&definition, &alignment_of(doc, data.alignment)?, &tin)?;
    data.definition = definition;
    store(doc, id, data, &corridor)?;
    Ok(corridor)
}
