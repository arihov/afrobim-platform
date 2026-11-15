//! The road modules' elements in the model export: road markings, side drains, pipe culverts,
//! sign gantries, street lighting, road restraint, pavements, corridors and ground surfaces.
//!
//! Every one of these elements is modelled by its module in site coordinates (X east, Y north,
//! Z up) and drawn in the 3D view from the `Mesh3` solids the module generates, in the frame of
//! the element's `SitePlacement` (a corridor in the frame of its built surfaces). This file
//! writes the same solids as tessellated face sets at the same placement, so the exported
//! geometry sits where the 3D view draws it. The IFC entity of each category, with the
//! documentation it was checked against, is in the crate README.

use std::borrow::Cow;
use std::collections::{BTreeMap, BTreeSet, HashMap};

use afrobim_draw::Mesh3;
use afrobim_kernel::{Element, ElementId, Generated, Module, Severity};
use afrobim_markings::{ext_spec, ExtDefinition, Family, ExtMarkingModule, ExtQuantities, GeometryStatus, Kind, MarkingDefinition, MarkingModule, MarkingQuantities, Pattern, Setting, STUD_ID};
use afrobim_pavement::{measure_definition, Material, PavementDefinition, PavementModule};
use afrobim_restraint::{BarrierElement, EndTreatment, RestraintModule, RunQuantities};

use super::{is_closed, mm, valid_site, Model, Pose, Quantity, PROP_DESIGN_BASIS};
use crate::step::{opt_string, refs};
use crate::{clip, element_global_id, step_string, IfcElementInput, Value};

/// Category of the elements exported as `IfcSurfaceFeature` (`LINEMARKING`).
pub const MARKING_CATEGORY: &str = afrobim_markings::CATEGORY;
/// Category of the second marking set, exported as `IfcSurfaceFeature` (see [`ext_feature_type`]).
pub const MARKING_EXT_CATEGORY: &str = afrobim_markings::CATEGORY_EXT;
/// Category of the elements exported as `IfcPipeSegment` (`GUTTER`).
pub const DRAIN_CATEGORY: &str = afrobim_drainage::DRAIN_CATEGORY;
/// Category of the elements exported as `IfcPipeSegment` (`CULVERT`).
pub const CULVERT_CATEGORY: &str = afrobim_drainage::CULVERT_CATEGORY;
/// Category of the elements exported as `IfcElementAssembly` (`SIGNALASSEMBLY`).
pub const GANTRY_CATEGORY: &str = afrobim_gantry::GANTRY_CATEGORY;
/// Category of the elements exported as one `IfcElementAssembly` (`MAST`) per column.
pub const LIGHTING_CATEGORY: &str = afrobim_lighting::CATEGORY;
/// Category of the elements exported as `IfcRailing` (`GUARDRAIL`).
pub const RESTRAINT_CATEGORY: &str = afrobim_restraint::CATEGORY;
/// Category of the elements exported as `IfcPavement`.
pub const PAVEMENT_CATEGORY: &str = afrobim_pavement::PAVEMENT_CATEGORY;
/// Category of the elements exported as `IfcBuildingElementProxy` (corridor surfaces).
pub const CORRIDOR_CATEGORY: &str = afrobim_corridor::CORRIDOR_CATEGORY;
/// Category of the elements exported as `IfcGeographicElement` (`TERRAIN`).
pub const SURFACE_CATEGORY: &str = afrobim_corridor::SURFACE_CATEGORY;

/// Precision of a tolerance between stations, m.
const STATION_TOLERANCE_M: f64 = 1e-3;

/// True for a category this file exports.
pub(super) fn is_road_category(category: &str) -> bool {
    [
        MARKING_CATEGORY,
        MARKING_EXT_CATEGORY,
        DRAIN_CATEGORY,
        CULVERT_CATEGORY,
        GANTRY_CATEGORY,
        LIGHTING_CATEGORY,
        RESTRAINT_CATEGORY,
        PAVEMENT_CATEGORY,
        CORRIDOR_CATEGORY,
        SURFACE_CATEGORY,
    ]
    .contains(&category)
}

/// How a marking of the second set is written: the `IfcSurfaceFeature` predefined type it takes
/// and why.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct FeatureType {
    /// The `IfcSurfaceFeatureTypeEnum` enumerator.
    pub predefined: &'static str,
    /// `ObjectType`, which the schema requires when `predefined` is `USERDEFINED`.
    pub object_type: Option<&'static str>,
    /// The wording of the enumerator's documentation the choice rests on, or the reason no
    /// enumerator fits.
    pub because: &'static str,
}

const LINE: FeatureType = FeatureType { predefined: "LINEMARKING", object_type: None, because: "2D lines painted on pavement surfaces to form boundaries, centrelines and edge markings" };
const SYMBOL: FeatureType = FeatureType { predefined: "SYMBOLMARKING", object_type: None, because: "surface markings that convey information in the form of symbols and shapes such as arrows, text or pictorial symbols" };
const HATCH: FeatureType = FeatureType { predefined: "HATCHMARKING", object_type: None, because: "surface markings defined by enclosed 2d shape with defined hatch fillings" };
const SURFACE_MARKING: FeatureType = FeatureType { predefined: "PAVEMENTSURFACEMARKING", object_type: None, because: "painted or chemical lines or symbols on the surface of pavements" };
const STUD: FeatureType = FeatureType {
    predefined: "USERDEFINED",
    object_type: Some("ROAD_STUD"),
    because: "no enumerator names a raised stud fixed to the pavement (RUMBLESTRIP is a raised, textured strip; the marking enumerators are for painted or chemical markings)",
};

/// The `IfcSurfaceFeature` type of a second-set marking, from the kind of the catalogue entry:
/// lines, transverse lines and the zig-zag line are `LINEMARKING`; arrows, triangles and worded
/// markings are `SYMBOLMARKING`; hatching, chevrons and island markings are `HATCHMARKING`;
/// crossings with side lines are `LINEMARKING`, crossings of stripes are `PAVEMENTSURFACEMARKING`;
/// an entry with no drawn geometry takes the type of its family (chevrons hatch, worded markings
/// symbol, parking bay line, the hump marking `PAVEMENTSURFACEMARKING`); road studs are `USERDEFINED` (`ROAD_STUD`). `None` for
/// an identifier not in the catalogue.
pub fn ext_feature_type(type_id: &str) -> Option<FeatureType> {
    if type_id.eq_ignore_ascii_case(STUD_ID) {
        return Some(STUD);
    }
    let spec = ext_spec(type_id)?;
    Some(match spec.kind {
        Kind::Lines(_) | Kind::Transverse(_) | Kind::ZigZag(_) => LINE,
        Kind::Crossing(c) if !c.stripes => LINE,
        Kind::Crossing(_) => SURFACE_MARKING,
        Kind::Arrow(_) | Kind::Triangle(_) | Kind::Word(_) => SYMBOL,
        Kind::Hatch(_) => HATCH,
        // Catalogued only, nothing drawn: the family says what the marking is.
        Kind::None => match spec.family {
            Family::ChevronMarking | Family::HatchedMarking | Family::IslandMarking => HATCH,
            Family::WordedMarking | Family::LaneArrow | Family::DeflectionArrow | Family::GiveWayTriangle => SYMBOL,
            Family::ParkingBay | Family::StopLine | Family::GiveWayLine => LINE,
            _ => SURFACE_MARKING,
        },
    })
}

/// A pavement a marking can adhere to: its element, alignment, stations and the offsets it covers.
struct Cover {
    pavement: ElementId,
    alignment: String,
    start_m: f64,
    end_m: f64,
    left_m: f64,
    right_m: f64,
}

/// What the road elements leave for the end of the export.
#[derive(Default)]
pub(super) struct RoadState {
    /// The pavements of the document, read before anything is written.
    covers: Vec<Cover>,
    /// The `IfcPavement` written for each pavement element.
    pavement_products: BTreeMap<ElementId, u32>,
    /// Each `IfcSurfaceFeature` marking and the pavement element it adheres to.
    adhering: Vec<(u32, ElementId)>,
    /// The signs hung on gantries, which the gantries write as parts.
    pub(super) hung: BTreeSet<ElementId>,
    /// Products contained in the site, not the road (terrain).
    in_site: Vec<u32>,
}

impl RoadState {
    /// The pavement that carries a marking: on its alignment, over its stations and at its offset.
    fn carrying(&self, d: &MarkingDefinition) -> Option<ElementId> {
        self.carrying_range(&d.alignment, d.start_station_m, d.end_station_m, d.offset_m, d.offset_m)
    }

    /// The pavement that covers a range of stations and of offsets on an alignment.
    fn carrying_range(&self, alignment: &str, start_m: f64, end_m: f64, offset_lo_m: f64, offset_hi_m: f64) -> Option<ElementId> {
        self.covers
            .iter()
            .find(|p| {
                p.alignment == alignment
                    && p.start_m - STATION_TOLERANCE_M <= start_m
                    && end_m <= p.end_m + STATION_TOLERANCE_M
                    && p.left_m - STATION_TOLERANCE_M <= offset_lo_m
                    && offset_hi_m <= p.right_m + STATION_TOLERANCE_M
            })
            .map(|p| p.pavement)
    }
}

/// The module's own output for an element: what the caller handed over, or a fresh run of the
/// module when it handed over nothing.
fn generated<'a>(input: &'a IfcElementInput) -> Option<Cow<'a, Generated>> {
    if let Some(g) = input.generated {
        return Some(Cow::Borrowed(g));
    }
    let e = input.element;
    let result = match e.category.as_str() {
        c if c == MARKING_CATEGORY => MarkingModule.generate(e),
        c if c == MARKING_EXT_CATEGORY => ExtMarkingModule.generate(e),
        c if c == DRAIN_CATEGORY => afrobim_drainage::SideDrainModule::default().generate(e),
        c if c == CULVERT_CATEGORY => afrobim_drainage::PipeCulvertModule::default().generate(e),
        c if c == GANTRY_CATEGORY => afrobim_gantry::GantryModule.generate(e),
        c if c == LIGHTING_CATEGORY => afrobim_lighting::LightingModule.generate(e),
        c if c == RESTRAINT_CATEGORY => RestraintModule.generate(e),
        c if c == PAVEMENT_CATEGORY => PavementModule::default().generate(e),
        c if c == SURFACE_CATEGORY => afrobim_corridor::SurfaceModule::default().generate(e),
        c if c == CORRIDOR_CATEGORY => afrobim_corridor::CorridorModule.generate(e),
        _ => return None,
    };
    result.ok().map(Cow::Owned)
}

/// The mesh with its coincident vertices merged: the same triangles on fewer points, which is what
/// a surface built from separate quads (a corridor) needs to be small and to be seen as one
/// surface. A mesh with no repeated vertex is returned as it is.
fn weld(mesh: &Mesh3) -> Cow<'_, Mesh3> {
    let key = |v: &[f64; 3]| [(v[0] + 0.0).to_bits(), (v[1] + 0.0).to_bits(), (v[2] + 0.0).to_bits()];
    let mut seen: HashMap<[u64; 3], u32> = HashMap::with_capacity(mesh.vertices.len());
    let mut vertices = Vec::with_capacity(mesh.vertices.len());
    let mut map = Vec::with_capacity(mesh.vertices.len());
    for v in &mesh.vertices {
        let next = vertices.len() as u32;
        let at = *seen.entry(key(v)).or_insert_with(|| {
            vertices.push(*v);
            next
        });
        map.push(at);
    }
    if vertices.len() == mesh.vertices.len() {
        return Cow::Borrowed(mesh);
    }
    let triangles = mesh.triangles.iter().map(|t| t.map(|i| map.get(i as usize).copied().unwrap_or(u32::MAX))).collect();
    Cow::Owned(Mesh3 { name: mesh.name.clone(), vertices, triangles, colour: mesh.colour })
}

/// The mesh moved by `d`.
fn shifted(mesh: &Mesh3, d: [f64; 3]) -> Cow<'_, Mesh3> {
    if d == [0.0; 3] {
        return Cow::Borrowed(mesh);
    }
    Cow::Owned(Mesh3 {
        name: mesh.name.clone(),
        vertices: mesh.vertices.iter().map(|v| [v[0] + d[0], v[1] + d[1], v[2] + d[2]]).collect(),
        triangles: mesh.triangles.clone(),
        colour: mesh.colour,
    })
}

fn text(v: &str) -> Value {
    Value::Text(v.to_string())
}

fn length_pos(v: f64) -> Value {
    Value::Measure("IFCPOSITIVELENGTHMEASURE", v)
}

fn length_non_negative(v: f64) -> Value {
    Value::Measure("IFCNONNEGATIVELENGTHMEASURE", v)
}

/// Mark, category and layer: what every road element's own property set starts with.
fn identity(e: &Element) -> Vec<(String, Value)> {
    vec![("Mark".to_string(), text(&e.name)), ("Category".to_string(), text(&e.category)), ("Layer".to_string(), text(&e.layer))]
}

/// The alignment, stations and offset of an element, where it has them.
fn stations(p: &mut Vec<(String, Value)>, alignment: &str, start_m: Option<f64>, end_m: Option<f64>, offset_m: Option<f64>) {
    if !alignment.is_empty() {
        p.push(("Alignment".to_string(), text(alignment)));
    }
    // One station is the station, two are a range.
    let first = if end_m.is_some() { "StartStationM" } else { "StationM" };
    for (name, v) in [(first, start_m), ("EndStationM", end_m), ("OffsetM", offset_m)] {
        if let Some(v) = v.filter(|v| v.is_finite()) {
            p.push((name.to_string(), Value::Real(v)));
        }
    }
}

/// The notes a module attached to an element as properties: the note's code names the
/// property, its severity and message are the value.
fn note_properties(g: &Generated) -> Vec<(String, Value)> {
    let mut seen: BTreeMap<String, usize> = BTreeMap::new();
    g.notes
        .iter()
        .map(|n| {
            let count = seen.entry(n.code.clone()).or_insert(0);
            *count += 1;
            let name = if *count == 1 { n.code.clone() } else { format!("{}#{}", n.code, count) };
            let severity = match n.severity {
                Severity::Info => "Info",
                Severity::Warning => "Warning",
                Severity::Error => "Error",
            };
            (name, Value::Text(format!("{severity}: {}", n.message)))
        })
        .collect()
}

impl Model<'_> {
    /// Writes the road element and returns the products the road is to contain. A marking that a
    /// pavement carries is not among them: it adheres to the pavement instead.
    pub(super) fn road_element(&mut self, input: &IfcElementInput, notes: &mut Vec<String>) -> Vec<u32> {
        let e = input.element;
        let g = generated(input);
        let g = g.as_deref();
        let out = match e.category.as_str() {
            c if c == MARKING_CATEGORY => self.marking(input, g, notes),
            c if c == MARKING_EXT_CATEGORY => self.ext_marking(input, g, notes),
            c if c == DRAIN_CATEGORY => self.side_drain(input, g, notes),
            c if c == CULVERT_CATEGORY => self.culvert(input, g, notes),
            c if c == GANTRY_CATEGORY => self.gantry(input, g, notes),
            c if c == LIGHTING_CATEGORY => self.lighting(input, g, notes),
            c if c == RESTRAINT_CATEGORY => self.restraint(input, g, notes),
            c if c == PAVEMENT_CATEGORY => self.pavement(input, g, notes),
            c if c == CORRIDOR_CATEGORY => self.corridor(input, g, notes),
            _ => self.surface(input, g, notes),
        };
        self.w.frame = None;
        out
    }

    /// Reads the pavements of the document before anything is written: a road marking is an
    /// `IfcSurfaceFeature` only when a pavement carries it.
    pub(super) fn prepare_road(&mut self, elements: &[IfcElementInput]) {
        for input in elements.iter().filter(|i| i.element.category == PAVEMENT_CATEGORY) {
            let Some(d) = PavementDefinition::from_value(&input.element.data).ok().filter(|d| d.check().is_ok()) else { continue };
            let strips = d.strips();
            let (Some(first), Some(last)) = (strips.first(), strips.last()) else { continue };
            self.road.covers.push(Cover {
                pavement: input.element.id,
                alignment: d.alignment.name.clone(),
                start_m: d.start_station_m,
                end_m: d.end_station_m,
                left_m: first.left_offset_m(),
                right_m: last.right_offset_m(),
            });
        }
    }

    /// Adds the relationships that need every element written first: each marking adheres to the
    /// pavement that carries it (`IfcRelAdheresToElement`), and terrain is contained in the site.
    pub(super) fn finish_road(&mut self) {
        let mut by_pavement: BTreeMap<ElementId, Vec<u32>> = BTreeMap::new();
        for (marking, pavement) in std::mem::take(&mut self.road.adhering) {
            by_pavement.entry(pavement).or_default().push(marking);
        }
        for (pavement, features) in by_pavement {
            let Some(product) = self.road.pavement_products.get(&pavement).copied() else { continue };
            let guid = step_string(&self.w.guid(&format!("element/{}/rel-adheres", pavement.0)));
            self.w.data.add("IFCRELADHERESTOELEMENT", &format!("{guid},$,$,$,#{product},{}", refs(&features)));
        }
        let in_site = std::mem::take(&mut self.road.in_site);
        if !in_site.is_empty() {
            let args = format!("{},$,$,$,{},#{}", step_string(&self.w.guid("rel-contained-site")), refs(&in_site), self.w.site);
            self.w.data.add("IFCRELCONTAINEDINSPATIALSTRUCTURE", &args);
        }
    }

    // ─── shared writers ─────────────────────────────────────────────────────────────────────

    /// The Body of meshes, each with the name of its surface style: a tessellation of triangulated
    /// face sets, closed when every edge of the mesh is matched. A mesh that is not a sound
    /// triangle mesh is left out and reported.
    fn body_of(&mut self, meshes: &[(&str, Cow<'_, Mesh3>)], who: &str, notes: &mut Vec<String>) -> Option<u32> {
        let mut items = Vec::new();
        for (kind, mesh) in meshes {
            let mesh = &weld(mesh);
            let n = mesh.vertices.len() as u32;
            let sound = !mesh.triangles.is_empty()
                && mesh.triangles.iter().all(|t| t.iter().all(|i| *i < n))
                && mesh.vertices.iter().all(|v| v.iter().all(|c| c.is_finite()));
            if !sound {
                notes.push(format!("{who}: solid '{}' is not a sound triangle mesh; not exported", mesh.name));
                continue;
            }
            let style = self.w.solid_style(kind, mesh.colour);
            items.push(self.w.face_set(&mesh.vertices, &mesh.triangles, is_closed(mesh), style));
        }
        (!items.is_empty()).then(|| self.w.body("Tessellation", &items))
    }

    /// An `IfcElement` subtype with the attributes every one of them shares. `assembly` adds the
    /// `AssemblyPlace` attribute `IfcElementAssembly` has before its predefined type.
    #[allow(clippy::too_many_arguments)]
    fn make(
        &mut self,
        entity: &str,
        guid: &str,
        name: &str,
        description: Option<&str>,
        object_type: Option<&str>,
        placement: u32,
        shape: Option<u32>,
        tag: &str,
        assembly: bool,
        predefined: &str,
    ) -> u32 {
        let args = format!(
            "{},$,{},{},{},#{placement},{},{},{}.{predefined}.",
            step_string(guid),
            step_string(&clip(name)),
            opt_string(description.map(clip).as_deref()),
            opt_string(object_type),
            shape.map_or("$".to_string(), |s| format!("#{s}")),
            step_string(tag),
            if assembly { "$," } else { "" },
        );
        self.w.data.add(entity, &args)
    }

    /// The placement of an element in its module's frame, relative to the road: the site
    /// placement, or the file origin when it has none.
    fn road_pose(e: &Element, notes: &mut Vec<String>) -> Pose {
        match valid_site(e) {
            Some(s) => Pose::site(s, 0.0),
            None => {
                notes.push(format!("{}: it has no site placement, and its geometry is in site coordinates; exported at the origin of the file", e.name));
                Pose { x: 0.0, y: 0.0, z: 0.0, rotation_deg: 0.0 }
            }
        }
    }

    /// The identity set, the element's own properties, the module's derived values and notes,
    /// and the site placement.
    fn element_sets(&mut self, product: u32, e: &Element, g: Option<&Generated>, set: &str, identity: Vec<(String, Value)>, basis: Option<&str>) {
        self.element_sets_at(product, e, g, set, identity, basis, true);
    }

    /// As [`Model::element_sets`]; `site` says whether the product stands at the element's site
    /// placement (a group does not stand anywhere).
    #[allow(clippy::too_many_arguments)]
    fn element_sets_at(&mut self, product: u32, e: &Element, g: Option<&Generated>, set: &str, mut identity: Vec<(String, Value)>, basis: Option<&str>, site: bool) {
        let recorded = e.properties.get(PROP_DESIGN_BASIS).filter(|b| !b.is_empty()).map(String::as_str).or(basis);
        if let Some(b) = recorded {
            identity.push(("DesignBasis".to_string(), text(b)));
        }
        self.w.property_set(product, e.id, set, &identity);
        let as_text = |map: &BTreeMap<String, String>| map.iter().map(|(k, v)| (k.clone(), Value::Text(v.clone()))).collect::<Vec<_>>();
        self.w.property_set(product, e.id, "AfroBIM_Properties", &as_text(&e.properties));
        if let Some(g) = g {
            self.w.property_set(product, e.id, "AfroBIM_Derived", &as_text(&g.derived));
            self.w.property_set(product, e.id, "AfroBIM_Notes", &note_properties(g));
        }
        if site {
            self.site_properties(product, &format!("element/{}", e.id.0), e);
        }
    }

    /// `AfroBIM_SitePlacement` of a part whose position the element does not carry: where it
    /// stands in the site, and the alignment, station and offset it was laid out at.
    #[allow(clippy::too_many_arguments)]
    fn place_properties(&mut self, product: u32, owner: &str, pose: &Pose, alignment: &str, station_m: Option<f64>, offset_m: Option<f64>) {
        let mut p = vec![
            ("X".to_string(), Value::Length(pose.x)),
            ("Y".to_string(), Value::Length(pose.y)),
            ("Z".to_string(), Value::Length(pose.z)),
            ("RotationDeg".to_string(), Value::Real(pose.rotation_deg)),
        ];
        if !alignment.is_empty() {
            p.push(("Alignment".to_string(), text(alignment)));
        }
        if let Some(v) = station_m.filter(|v| v.is_finite()) {
            p.push(("StationM".to_string(), Value::Real(v)));
        }
        if let Some(v) = offset_m.filter(|v| v.is_finite()) {
            p.push(("OffsetM".to_string(), Value::Real(v)));
        }
        self.w.property_set_of(product, owner, "AfroBIM_SitePlacement", &p);
    }

    /// `IfcRelAggregates` of `parts` under `whole`.
    fn aggregate(&mut self, owner: &str, whole: u32, parts: &[u32]) {
        if parts.is_empty() {
            return;
        }
        let guid = step_string(&self.w.guid(&format!("{owner}/rel-aggregates")));
        self.w.data.add("IFCRELAGGREGATES", &format!("{guid},$,$,$,#{whole},{}", refs(parts)));
    }

    /// An `IfcGroup` named after the element, with `members`: one element that is several
    /// products. Returns the group.
    fn group(&mut self, e: &Element, description: &str, members: &[u32]) -> u32 {
        let guid = step_string(&element_global_id(&self.w.options.project_name, e.id));
        let group = self.w.data.add("IFCGROUP", &format!("{guid},$,{},{},$", step_string(&clip(&e.name)), opt_string(Some(description))));
        let rel = step_string(&self.w.guid(&format!("element/{}/rel-group", e.id.0)));
        self.w.data.add("IFCRELASSIGNSTOGROUP", &format!("{rel},$,$,$,{},$,#{group}", refs(members)));
        group
    }

    /// The position of an element along its alignment, for `IfcRelPositions`.
    fn position_on(&mut self, product: u32, alignment: &str) {
        if !alignment.is_empty() {
            self.positioned.push((product, alignment.to_string()));
        }
    }

    // ─── road markings ──────────────────────────────────────────────────────────────────────

    /// A marking: `IfcSurfaceFeature` `LINEMARKING`, its strands as one tessellated Body, adhering
    /// to the pavement that carries it. `IfcSurfaceFeature` must adhere to an element, so a
    /// marking no pavement carries is an `IfcBuildingElementProxy` instead, contained in the road.
    fn marking(&mut self, input: &IfcElementInput, g: Option<&Generated>, notes: &mut Vec<String>) -> Vec<u32> {
        let e = input.element;
        let def = MarkingDefinition::from_value(&e.data).ok();
        let resolved = def.as_ref().and_then(|d| d.resolve().0);
        let host = def.as_ref().and_then(|d| self.road.carrying(d));
        if host.is_none() {
            notes.push(format!(
                "{}: no pavement in the document carries this marking (same alignment, stations and offset), and an IfcSurfaceFeature must adhere to an element; exported as IfcBuildingElementProxy",
                e.name
            ));
        }
        let pose = Self::road_pose(e, notes);
        let placement = self.root_placement(e, pose);
        let meshes: Vec<(&str, Cow<Mesh3>)> = g.iter().flat_map(|g| g.solids.iter()).map(|m| ("Road marking", Cow::Borrowed(m))).collect();
        let shape = self.body_of(&meshes, &e.name, notes);
        if shape.is_none() {
            notes.push(format!("{}: no geometry exported, the marking has no solids", e.name));
        }
        let description = resolved.as_ref().map(|r| format!("{}, diagram {}", r.spec.name, r.spec.diagram));
        let (entity, predefined, object_type) = if host.is_some() { ("IFCSURFACEFEATURE", "LINEMARKING", None) } else { ("IFCBUILDINGELEMENTPROXY", "USERDEFINED", Some("marking")) };
        let product = self.make(
            entity,
            &element_global_id(&self.w.options.project_name, e.id),
            &e.name,
            description.as_deref(),
            object_type,
            placement,
            shape,
            &e.id.0.to_string(),
            false,
            predefined,
        );
        let mut p = identity(e);
        let mut basis = None;
        if let (Some(d), Some(r)) = (&def, &resolved) {
            p.push(("LineType".to_string(), text(r.spec.id)));
            p.push(("Diagram".to_string(), text(r.spec.diagram)));
            p.push(("TsrgdReference".to_string(), text(r.spec.tsrgd)));
            p.push(("Pattern".to_string(), text(&pattern_label(r.spec))));
            p.push(("WidthMm".to_string(), Value::Length(f64::from(r.width_mm))));
            if let Some(s) = r.separation_mm {
                p.push(("SeparationMm".to_string(), Value::Length(f64::from(s))));
            }
            p.push(("Colour".to_string(), text(d.colour.label())));
            p.push(("SpeedLimitMph".to_string(), Value::Integer(i64::from(d.speed_limit_mph))));
            if !d.material.trim().is_empty() {
                p.push(("Material".to_string(), text(&d.material)));
            }
            p.push(("Source".to_string(), text(r.spec.citation)));
            stations(&mut p, &d.alignment, Some(d.start_station_m), Some(d.end_station_m), Some(d.offset_m));
            basis = Some(afrobim_markings::EDITION);
        } else {
            notes.push(format!("{}: its definition cannot be read or has errors; exported without its marking data", e.name));
        }
        self.element_sets(product, e, g, "AfroBIM_RoadMarking", p, basis);

        if let (Some(d), Some(r)) = (&def, &resolved) {
            // Pset_RoadMarkingCommon, Pset_MarkingLinesCommon (IFC 4.3, IfcSurfaceFeature LINEMARKING).
            if host.is_some() {
            let mut common = vec![("DiagramNumber".to_string(), text(r.spec.diagram)), ("MaterialColour".to_string(), text(d.colour.label()))];
            if !d.material.trim().is_empty() {
                common.push(("MaterialType".to_string(), text(&d.material)));
            }
            self.w.property_set(product, e.id, "Pset_RoadMarkingCommon", &common);
            let broken = matches!(r.spec.pattern, Pattern::Broken { .. })
                || r.spec.double.is_some_and(|dl| matches!(dl.left, Pattern::Broken { .. }) || matches!(dl.right, Pattern::Broken { .. }));
            // The nominal overall width of a double line is both lines and the gap between them.
            let overall = f64::from(r.width_mm) * if r.spec.double.is_some() { 2.0 } else { 1.0 } + f64::from(r.separation_mm.unwrap_or(0));
            let lines = vec![
                ("DashedLine".to_string(), Value::Boolean(broken)),
                ("DashedLinePattern".to_string(), text(&pattern_label(r.spec))),
                ("NominalWidth".to_string(), length_non_negative(overall)),
            ];
            self.w.property_set(product, e.id, "Pset_MarkingLinesCommon", &lines);
            }
            if let Ok(q) = MarkingQuantities::measure(&e.name, d) {
                if host.is_some() {
                    self.w.quantities(
                        product,
                        e.id,
                        "Qto_SurfaceFeatureBaseQuantities",
                        &[Quantity::Area("Area", q.painted_area_m2), Quantity::Length("Length", q.painted_length_m * 1000.0)],
                    );
                }
                self.w.quantities(
                    product,
                    e.id,
                    "AfroBIM_Quantities",
                    &[
                        Quantity::Length("OverallLength", q.overall_length_m * 1000.0),
                        Quantity::Length("PaintedLength", q.painted_length_m * 1000.0),
                        Quantity::Area("PaintedArea", q.painted_area_m2),
                        Quantity::Count("Marks", q.marks as i64),
                        Quantity::Count("PartMarks", q.part_marks as i64),
                    ],
                );
            }
        }
        if let Some(a) = valid_site(e).and_then(|s| s.alignment.clone()) {
            self.position_on(product, &a);
        }
        match host {
            Some(pavement) => {
                self.road.adhering.push((product, pavement));
                Vec::new()
            }
            None => vec![product],
        }
    }

    // ─── road markings of the second set ────────────────────────────────────────────────────

    /// A marking of the second set (stop and give way lines, triangle, arrows, hatching and
    /// chevrons, yellow lines, studs, worded blocks, the Uganda types): an `IfcSurfaceFeature` of
    /// the type [`ext_feature_type`] gives, adhering to the pavement that carries it, otherwise
    /// the documented `IfcBuildingElementProxy` contained in the road. A placeholder block is
    /// written as the block it is and says so in its properties.
    fn ext_marking(&mut self, input: &IfcElementInput, g: Option<&Generated>, notes: &mut Vec<String>) -> Vec<u32> {
        let e = input.element;
        let def = ExtDefinition::from_value(&e.data).ok();
        let resolved = def.as_ref().and_then(|d| d.lay_out().0).map(|(r, _)| r);
        let feature = resolved.as_ref().and_then(|r| ext_feature_type(&r.id));
        let host = match (&def, feature) {
            (Some(d), Some(_)) => {
                let (lo, hi) = match d.across_to_offset_m {
                    Some(to) => (d.offset_m.min(to), d.offset_m.max(to)),
                    None => (d.offset_m, d.offset_m),
                };
                self.road.carrying_range(&d.alignment, d.station_m, d.end_station_m.unwrap_or(d.station_m), lo, hi)
            }
            _ => None,
        };
        if host.is_none() {
            notes.push(format!(
                "{}: no pavement in the document carries this marking (same alignment, stations and offsets), and an IfcSurfaceFeature must adhere to an element; exported as IfcBuildingElementProxy",
                e.name
            ));
        }
        let pose = Self::road_pose(e, notes);
        let placement = self.root_placement(e, pose);
        let meshes: Vec<(&str, Cow<Mesh3>)> = g.iter().flat_map(|g| g.solids.iter()).map(|m| ("Road marking", Cow::Borrowed(m))).collect();
        let shape = self.body_of(&meshes, &e.name, notes);
        if shape.is_none() {
            notes.push(format!("{}: no geometry exported, the marking has no solids", e.name));
        }
        let placeholder = match resolved.as_ref().and_then(|r| r.geometry) {
            Some(GeometryStatus::Placeholder(why)) => Some(why),
            _ => None,
        };
        let description = resolved.as_ref().map(|r| {
            let base = format!("{}, {}", r.name, r.diagram);
            if placeholder.is_some() { format!("{base} (placeholder block, not the true shape)") } else { base }
        });
        let (entity, predefined, object_type) = match (host, feature) {
            (Some(_), Some(f)) => ("IFCSURFACEFEATURE", f.predefined, f.object_type),
            _ => ("IFCBUILDINGELEMENTPROXY", "USERDEFINED", Some("marking")),
        };
        let product = self.make(
            entity,
            &element_global_id(&self.w.options.project_name, e.id),
            &e.name,
            description.as_deref(),
            object_type,
            placement,
            shape,
            &e.id.0.to_string(),
            false,
            predefined,
        );
        let mut p = identity(e);
        let mut basis = None;
        let quantities = def.as_ref().and_then(|d| ExtQuantities::measure(&e.name, d).ok());
        if let (Some(d), Some(r)) = (&def, &resolved) {
            p.push(("TypeId".to_string(), text(&r.id)));
            p.push(("Marking".to_string(), text(&r.name)));
            p.push(("Basis".to_string(), text(d.basis.label())));
            p.push(("Diagram".to_string(), text(&r.diagram)));
            if !r.tsrgd.is_empty() {
                p.push(("TsrgdReference".to_string(), text(&r.tsrgd)));
            }
            p.push(("Colour".to_string(), text(quantities.as_ref().map_or(r.colour.label(), |q| q.colour.as_str()))));
            if !d.material.trim().is_empty() {
                p.push(("Material".to_string(), text(&d.material)));
            }
            if let Some(f) = feature {
                p.push(("IfcMapping".to_string(), text(&format!("{} {}: {}", entity_label(host.is_some()), f.predefined, f.because))));
            }
            if let Some(why) = placeholder {
                p.push(("PlaceholderBlock".to_string(), Value::Boolean(true)));
                p.push((
                    "ShapeStatus".to_string(),
                    text(&format!("Placeholder block: the bounding block of the marking, not its true shape. {why}")),
                ));
            } else {
                p.push(("PlaceholderBlock".to_string(), Value::Boolean(false)));
            }
            if let Some(s) = d.speed {
                p.push(("Speed".to_string(), Value::Integer(i64::from(s))));
            }
            if !d.speed_note.trim().is_empty() {
                p.push(("SpeedNote".to_string(), text(&d.speed_note)));
            }
            if let Some(why) = d.uk_fallback.as_deref().filter(|w| !w.trim().is_empty()) {
                p.push(("UkFallback".to_string(), text(why)));
            }
            p.push(("Source".to_string(), text(&r.citation)));
            stations(&mut p, &d.alignment, Some(d.station_m), d.end_station_m, Some(d.offset_m));
            basis = Some(d.basis.edition());
        } else {
            notes.push(format!("{}: its definition cannot be read or has errors; exported without its marking data", e.name));
        }
        self.element_sets(product, e, g, "AfroBIM_RoadMarkingExt", p, basis);

        if let (Some(r), Some(q)) = (&resolved, &quantities) {
            // Pset_RoadMarkingCommon applies to IfcSurfaceFeature with HATCHMARKING, LINEMARKING,
            // PAVEMENTSURFACEMARKING or SYMBOLMARKING (the IFC4X3 property set template); a stud is
            // USERDEFINED and a proxy is not an IfcSurfaceFeature, so neither carries it.
            let standard_type = feature.is_some_and(|f| f.predefined != "USERDEFINED");
            if host.is_some() && standard_type {
                let mut common = vec![("DiagramNumber".to_string(), text(&r.diagram)), ("MaterialColour".to_string(), text(&q.colour))];
                if !q.material.trim().is_empty() {
                    common.push(("MaterialType".to_string(), text(&q.material)));
                }
                self.w.property_set(product, e.id, "Pset_RoadMarkingCommon", &common);
            }
            // Pset_MarkingLinesCommon (IfcSurfaceFeature LINEMARKING in the IFC4X3 property set
            // templates: DashedLine IfcBoolean, DashedLinePattern IfcLabel, NominalWidth
            // IfcNonNegativeLengthMeasure). Written only where the dash pattern holds for the whole
            // element; otherwise left out, with a note saying why.
            if host.is_some() && feature.is_some_and(|f| f.predefined == "LINEMARKING") {
                match ext_line_pset(def.as_ref(), &r.id) {
                    Ok(lines) => self.w.property_set(product, e.id, "Pset_MarkingLinesCommon", &lines),
                    Err(why) => notes.push(format!("{}: Pset_MarkingLinesCommon not written: {why}", e.name)),
                }
            }
            if host.is_some() {
                // A placeholder block has no painted area and a stud has none: only what is known.
                let mut base = Vec::new();
                if let Some(a) = q.painted_area_m2 {
                    base.push(Quantity::Area("Area", a));
                }
                if q.painted_length_m > 0.0 {
                    base.push(Quantity::Length("Length", q.painted_length_m * 1000.0));
                }
                if !base.is_empty() {
                    self.w.quantities(product, e.id, "Qto_SurfaceFeatureBaseQuantities", &base);
                }
            }
            let mut own = vec![
                Quantity::Length("OverallLength", q.overall_length_m * 1000.0),
                Quantity::Length("PaintedLength", q.painted_length_m * 1000.0),
                Quantity::Count("Number", q.number as i64),
                Quantity::Count("PartMarks", q.part_marks as i64),
            ];
            if let Some(a) = q.painted_area_m2 {
                own.push(Quantity::Area("PaintedArea", a));
            }
            if let Some(a) = q.block_area_m2.filter(|_| q.placeholder) {
                own.push(Quantity::Area("PlaceholderBlockArea", a));
            }
            self.w.quantities(product, e.id, "AfroBIM_Quantities", &own);
        }
        if let Some(a) = valid_site(e).and_then(|s| s.alignment.clone()) {
            self.position_on(product, &a);
        }
        match host {
            Some(pavement) => {
                self.road.adhering.push((product, pavement));
                Vec::new()
            }
            None => vec![product],
        }
    }

    // ─── side drains and culverts ───────────────────────────────────────────────────────────

    /// A side drain: `IfcPipeSegment` `GUTTER`, "a continuous open-channel segment of pipe".
    fn side_drain(&mut self, input: &IfcElementInput, g: Option<&Generated>, notes: &mut Vec<String>) -> Vec<u32> {
        let e = input.element;
        let def = afrobim_drainage::SideDrainDefinition::from_value(&e.data).ok();
        let pose = Self::road_pose(e, notes);
        let placement = self.root_placement(e, pose);
        let meshes: Vec<(&str, Cow<Mesh3>)> = g.iter().flat_map(|g| g.solids.iter()).map(|m| ("Side drain", Cow::Borrowed(m))).collect();
        let shape = self.body_of(&meshes, &e.name, notes);
        if shape.is_none() {
            notes.push(format!("{}: no geometry exported, the drain has no solids (it may not be routed along its alignment)", e.name));
        }
        let description = def.as_ref().map(|d| format!("{}, {}", d.section.label(), d.lining.label()));
        let product = self.make(
            "IFCPIPESEGMENT",
            &element_global_id(&self.w.options.project_name, e.id),
            &e.name,
            description.as_deref(),
            None,
            placement,
            shape,
            &e.id.0.to_string(),
            false,
            "GUTTER",
        );
        let mut p = identity(e);
        if let Some(d) = &def {
            p.push(("Section".to_string(), text(&d.section.label())));
            p.push(("Lining".to_string(), text(&d.lining.label())));
            p.push(("LiningThicknessMm".to_string(), Value::Length(d.lining_mm())));
            p.push(("GradientMPerM".to_string(), Value::Real(d.gradient_m_per_m)));
            if let Some(n) = d.manning_n {
                p.push(("ManningN".to_string(), Value::Real(n)));
            }
            let (start, end, offset) = d.route.as_ref().map_or((Some(d.start_station_m), Some(d.end_station_m), Some(d.offset_m)), |r| (Some(r.start_station_m), Some(r.end_station_m), Some(r.offset_m)));
            stations(&mut p, &d.alignment, start, end, offset);
            if d.placeholder {
                p.push(("PlaceholderDimensions".to_string(), Value::Boolean(true)));
                notes.push(format!("{}: its dimensions are placeholders, not design values", e.name));
            }
        } else {
            notes.push(format!("{}: its definition cannot be read; exported without its drain data", e.name));
        }
        self.element_sets(product, e, g, "AfroBIM_SideDrain", p, None);
        if let Some(d) = &def {
            // Pset_PipeSegmentTypeGutter and Pset_PipeSegmentOccurrence (IFC 4.3, IfcPipeSegment).
            self.w.property_set(product, e.id, "Pset_PipeSegmentTypeGutter", &[("IsCovered".to_string(), Value::Boolean(false))]);
            if d.gradient_m_per_m > 0.0 {
                self.w.property_set(product, e.id, "Pset_PipeSegmentOccurrence", &[("Gradient".to_string(), Value::Measure("IFCPOSITIVERATIOMEASURE", d.gradient_m_per_m))]);
            }
            if let Ok(q) = afrobim_drainage::DrainQuantities::measure(&e.name, d) {
                self.w.quantities(product, e.id, "Qto_PipeSegmentBaseQuantities", &[Quantity::Length("Length", q.length_m * 1000.0)]);
                self.w.quantities(
                    product,
                    e.id,
                    "AfroBIM_Quantities",
                    &[
                        Quantity::Length("CentrelineLength", q.centreline_length_m * 1000.0),
                        Quantity::Length("DrainLength", q.length_m * 1000.0),
                        Quantity::Area("FinishedArea", q.finished_area_m2),
                        Quantity::Area("ExcavationArea", q.excavation_area_m2),
                        Quantity::Volume("Excavation", q.excavation_m3),
                        Quantity::Area("LiningArea", q.lining_area_m2),
                        Quantity::Volume("LiningVolume", q.lining_volume_m3),
                    ],
                );
            }
        }
        if let Some(a) = valid_site(e).and_then(|s| s.alignment.clone()) {
            self.position_on(product, &a);
        }
        vec![product]
    }

    /// A pipe culvert: one `IfcPipeSegment` `CULVERT` per barrel, grouped when there are several.
    fn culvert(&mut self, input: &IfcElementInput, g: Option<&Generated>, notes: &mut Vec<String>) -> Vec<u32> {
        let e = input.element;
        let def = afrobim_drainage::PipeCulvertDefinition::from_value(&e.data).ok();
        let pose = Self::road_pose(e, notes);
        let barrels: Vec<&Mesh3> = g.iter().flat_map(|g| g.solids.iter()).collect();
        let count = barrels.len().max(1);
        let mut products = Vec::new();
        for i in 0..count {
            let placement = self.root_placement(e, pose);
            let meshes: Vec<(&str, Cow<Mesh3>)> = barrels.get(i).map(|m| ("Culvert barrel", Cow::Borrowed(*m))).into_iter().collect();
            let shape = self.body_of(&meshes, &e.name, notes);
            if shape.is_none() {
                notes.push(format!("{}: no geometry exported for barrel {}", e.name, i + 1));
            }
            let name = if count > 1 { format!("{}, barrel {}", e.name, i + 1) } else { e.name.clone() };
            let (guid, owner) = if count == 1 {
                (element_global_id(&self.w.options.project_name, e.id), format!("element/{}", e.id.0))
            } else {
                (self.w.guid(&format!("element/{}/barrel/{}", e.id.0, i + 1)), format!("element/{}/barrel/{}", e.id.0, i + 1))
            };
            let description = def.as_ref().map(|d| format!("{} mm internal diameter, {} mm wall, {} mm long", mm(d.internal_diameter_mm), mm(d.wall_thickness_mm), mm(d.length_mm)));
            let product = self.make("IFCPIPESEGMENT", &guid, &name, description.as_deref(), None, placement, shape, &e.id.0.to_string(), false, "CULVERT");
            let mut p = identity(e);
            if count > 1 {
                p.push(("Barrel".to_string(), Value::Integer(i as i64 + 1)));
            }
            if let Some(d) = &def {
                culvert_properties(&mut p, d, notes, &e.name, i == 0);
            }
            if count == 1 {
                self.element_sets(product, e, g, "AfroBIM_PipeCulvert", p, None);
            } else {
                self.w.property_set_of(product, &owner, "AfroBIM_PipeCulvert", &p);
                self.site_properties(product, &owner, e);
            }
            if let Some(d) = &def {
                let a = vec![("InnerDiameter".to_string(), length_pos(d.internal_diameter_mm)), ("OuterDiameter".to_string(), length_pos(d.outside_diameter_mm())), ("Length".to_string(), length_pos(d.length_mm))];
                self.w.property_set_of(product, &owner, "Pset_PipeSegmentTypeCommon", &a);
                // A circular barrel is as deep as it is wide.
                let c = vec![("InternalWidth".to_string(), length_pos(d.internal_diameter_mm)), ("ClearDepth".to_string(), length_pos(d.internal_diameter_mm))];
                self.w.property_set_of(product, &owner, "Pset_PipeSegmentTypeCulvert", &c);
                if d.fall_m() > 0.0 && d.plan_length_mm() > 0.0 {
                    let gradient = d.fall_m() * 1000.0 / d.plan_length_mm();
                    self.w.property_set_of(product, &owner, "Pset_PipeSegmentOccurrence", &[("Gradient".to_string(), Value::Measure("IFCPOSITIVERATIOMEASURE", gradient))]);
                }
                if let Ok(q) = afrobim_drainage::CulvertQuantities::measure(&e.name, d) {
                    self.w.quantities_of(product, &owner, "Qto_PipeSegmentBaseQuantities", &[Quantity::Length("Length", q.length_per_barrel_m * 1000.0)]);
                    self.w.quantities_of(
                        product,
                        &owner,
                        "AfroBIM_Quantities",
                        &[
                            Quantity::Count("Barrels", i64::from(q.barrels)),
                            Quantity::Length("LengthPerBarrel", q.length_per_barrel_m * 1000.0),
                            Quantity::Length("TotalLength", q.total_length_m * 1000.0),
                        ],
                    );
                }
            }
            if let Some(m) = def.as_ref().map(|d| d.material.trim()).filter(|m| !m.is_empty()) {
                self.materials.entry(m.to_string()).or_default().push(product);
            }
            if let Some(a) = valid_site(e).and_then(|s| s.alignment.clone()) {
                self.position_on(product, &a);
            }
            products.push(product);
        }
        if count > 1 {
            self.group(e, &format!("Culvert with {count} barrels"), &products);
        }
        products
    }

    // ─── gantries ───────────────────────────────────────────────────────────────────────────

    /// A gantry: `IfcElementAssembly` `SIGNALASSEMBLY` aggregating its legs (`IfcColumn`), boom
    /// (`IfcBeam`), foundations (`IfcFooting`) and the signs hung on it.
    fn gantry(&mut self, input: &IfcElementInput, g: Option<&Generated>, notes: &mut Vec<String>) -> Vec<u32> {
        let e = input.element;
        let def = afrobim_gantry::GantryDefinition::from_value(&e.data).ok();
        let pose = Self::road_pose(e, notes);
        let placement = self.root_placement(e, pose);
        let quantities = def.as_ref().and_then(|d| afrobim_gantry::GantryQuantities::measure(&e.name, d).ok());
        let description = def.as_ref().map(|d| format!("{} gantry carrying {} sign(s)", d.gantry_type.label(), d.signs.len()));
        let assembly = self.make(
            "IFCELEMENTASSEMBLY",
            &element_global_id(&self.w.options.project_name, e.id),
            &e.name,
            description.as_deref(),
            None,
            placement,
            None,
            &e.id.0.to_string(),
            true,
            "SIGNALASSEMBLY",
        );
        let mut p = identity(e);
        if let Some(d) = &def {
            p.push(("Type".to_string(), text(d.gantry_type.label())));
            p.push(("SkewDeg".to_string(), Value::Real(d.skew_deg)));
            p.push(("BoomUndersideAboveReferenceMm".to_string(), Value::Length(d.boom.underside_above_reference_mm)));
            if let Some(v) = d.reference_level_m {
                p.push(("ReferenceLevelM".to_string(), Value::Real(v)));
            }
            stations(&mut p, &d.alignment, Some(d.station_m), None, None);
            if d.placeholder {
                p.push(("PlaceholderDimensions".to_string(), Value::Boolean(true)));
                notes.push(format!("{}: its dimensions are placeholders, not design values", e.name));
            }
        } else {
            notes.push(format!("{}: its definition cannot be read; exported without its gantry data", e.name));
        }
        if let Some(q) = &quantities {
            p.push(("Signs".to_string(), Value::Integer(q.sign_count as i64)));
            if let Some(m) = q.steel_mass_kg {
                p.push(("SteelMassKg".to_string(), Value::Real(m)));
            }
        }
        self.element_sets(assembly, e, g, "AfroBIM_Gantry", p, None);
        self.w.property_set(assembly, e.id, "Pset_ElementAssemblyCommon", &[("Reference".to_string(), Value::Identifier(e.name.clone()))]);
        if let Some(q) = &quantities {
            self.w.quantities(
                assembly,
                e.id,
                "AfroBIM_Quantities",
                &[
                    Quantity::Length(if def.as_ref().is_some_and(|d| d.gantry_type == afrobim_gantry::GantryType::Cantilever) { "Outreach" } else { "ClearSpan" }, q.span_or_reach_mm),
                    Quantity::Length("BoomLength", q.boom_length_mm),
                    Quantity::Count("Signs", q.sign_count as i64),
                    Quantity::Area("SignArea", q.sign_area_m2),
                    Quantity::Volume("FoundationConcrete", q.foundation_concrete_m3),
                ],
            );
        }

        // Parts: the module's solids are "Leg n", "Foundation n" and "Boom", all in the frame of
        // the assembly (the gantry's own).
        let legs = def.as_ref().map(|d| {
            let mut legs: Vec<&afrobim_gantry::Leg> = d.legs.iter().collect();
            legs.sort_by(|a, b| a.offset_m.total_cmp(&b.offset_m));
            legs
        });
        let mut members = Vec::new();
        for mesh in g.iter().flat_map(|g| g.solids.iter()) {
            let index: usize = mesh.name.rsplit(' ').next().and_then(|n| n.parse().ok()).unwrap_or(1);
            let leg = legs.as_ref().and_then(|l| l.get(index.saturating_sub(1)).copied());
            let (entity, predefined, kind, role) = if mesh.name.starts_with("Leg") {
                ("IFCCOLUMN", "COLUMN", "Gantry leg", "Leg")
            } else if mesh.name.starts_with("Foundation") {
                ("IFCFOOTING", "PAD_FOOTING", "Gantry foundation", "Foundation")
            } else {
                ("IFCBEAM", "BEAM", "Gantry boom", "Boom")
            };
            let part_placement = self.w.placement(placement, Pose { x: 0.0, y: 0.0, z: 0.0, rotation_deg: 0.0 });
            let shape = self.body_of(&[(kind, Cow::Borrowed(mesh))], &e.name, notes);
            let owner = format!("element/{}/part/{}", e.id.0, mesh.name.replace(' ', "-").to_lowercase());
            let guid = self.w.guid(&owner);
            let name = format!("{}, {}", e.name, mesh.name.to_lowercase());
            let part = self.make(entity, &guid, &name, None, None, part_placement, shape, &e.id.0.to_string(), false, predefined);
            let mut props = vec![("Mark".to_string(), text(&e.name)), ("Role".to_string(), text(role)), ("Part".to_string(), text(&mesh.name))];
            match (role, def.as_ref(), leg) {
                ("Leg", Some(d), Some(l)) => {
                    props.push(("OffsetM".to_string(), Value::Real(l.offset_m)));
                    props.push(("Envelope".to_string(), text(&l.envelope.label())));
                    props.push(("BaseAboveReferenceMm".to_string(), Value::Length(l.base_above_reference_mm)));
                    props.push(("SupportHeightMm".to_string(), Value::Length(d.support_height_mm(l))));
                    if let Some(m) = l.mass_kg_per_m {
                        props.push(("MassKgPerM".to_string(), Value::Real(m)));
                    }
                    self.w.quantities_of(part, &owner, "Qto_ColumnBaseQuantities", &[Quantity::Length("Length", d.leg_length_mm(l))]);
                }
                ("Foundation", _, Some(l)) => {
                    if let Some(f) = l.foundation {
                        props.push(("OffsetM".to_string(), Value::Real(l.offset_m)));
                        self.w.quantities_of(
                            part,
                            &owner,
                            "Qto_FootingBaseQuantities",
                            &[
                                Quantity::Length("Length", f.length_mm),
                                Quantity::Length("Width", f.width_mm),
                                Quantity::Length("Height", f.depth_mm),
                                Quantity::Volume("GrossVolume", f.volume_m3()),
                            ],
                        );
                    }
                }
                ("Boom", Some(d), _) => {
                    props.push(("Envelope".to_string(), text(&d.boom.envelope.label())));
                    props.push(("UndersideAboveReferenceMm".to_string(), Value::Length(d.boom.underside_above_reference_mm)));
                    if let Some(m) = d.boom.mass_kg_per_m {
                        props.push(("MassKgPerM".to_string(), Value::Real(m)));
                    }
                    self.w.quantities_of(part, &owner, "Qto_BeamBaseQuantities", &[Quantity::Length("Length", d.boom_length_mm())]);
                }
                _ => {}
            }
            self.w.property_set_of(part, &owner, "AfroBIM_GantryMember", &props);
            members.push(part);
        }

        // The signs hung on the gantry: the signs whose host is this gantry, placed on site.
        let by_id = self.by_id;
        let hung: Vec<&IfcElementInput> = by_id.values().copied().filter(|i| i.element.host == Some(e.id) && self.road.hung.contains(&i.element.id)).collect();
        for sign in hung {
            let s = sign.element;
            let Some(site) = valid_site(s) else { continue };
            // A sign without posts: the placement's z is the level of its lower edge.
            let relative = Pose::site(site, 0.0).relative_to(&pose);
            let sign_placement = self.w.placement(placement, relative);
            let product = self.w.product(self.document, sign, sign_placement, notes);
            self.site_properties(product, &format!("element/{}", s.id.0), s);
            let mut mount = vec![("GantryMark".to_string(), text(&e.name))];
            if let Some(m) = def.as_ref().and_then(|d| d.signs.iter().find(|m| m.sign == s.id)) {
                match m.position {
                    afrobim_gantry::SignPosition::Lane { index } => mount.push(("Lane".to_string(), Value::Integer(index as i64))),
                    afrobim_gantry::SignPosition::Offset { offset_m } => mount.push(("OffsetM".to_string(), Value::Real(offset_m))),
                }
                mount.push(("BottomAboveBoomUndersideMm".to_string(), Value::Length(m.bottom_above_boom_underside_mm)));
                if let Some(d) = m.serves {
                    mount.push(("Serves".to_string(), text(match d { afrobim_gantry::Direction::UpStation => "up-station traffic", afrobim_gantry::Direction::DownStation => "down-station traffic" })));
                }
            }
            self.w.property_set(product, s.id, "AfroBIM_GantryMount", &mount);
            members.push(product);
        }
        self.aggregate(&format!("element/{}", e.id.0), assembly, &members);
        if let Some(a) = valid_site(e).and_then(|s| s.alignment.clone()) {
            self.position_on(assembly, &a);
        }
        vec![assembly]
    }

    // ─── street lighting ────────────────────────────────────────────────────────────────────

    /// Street lighting: one `IfcElementAssembly` `MAST` per column, aggregating its pole
    /// (`IfcMember` `POST`), brackets (`IfcMember`) and luminaires (`IfcLightFixture`); an
    /// installation of several columns is also an `IfcGroup`.
    fn lighting(&mut self, input: &IfcElementInput, g: Option<&Generated>, notes: &mut Vec<String>) -> Vec<u32> {
        let e = input.element;
        let Ok(data) = afrobim_lighting::LightingData::from_value(&e.data) else {
            notes.push(format!("{}: its lighting data cannot be read; not exported", e.name));
            return Vec::new();
        };
        let Some(site) = valid_site(e) else {
            notes.push(format!("{}: it has no site placement, so its columns cannot be placed; not exported", e.name));
            return Vec::new();
        };
        let spec = data.definition.column();
        let solids: Vec<&Mesh3> = g.iter().flat_map(|g| g.solids.iter()).collect();
        let alignment = data.layout.alignment.clone();
        let installation = matches!(data.definition, afrobim_lighting::LightingDefinition::Installation(_));
        let mut columns = Vec::new();
        for (index, c) in data.layout.columns.iter().enumerate() {
            let base = Pose { x: c.easting_m * 1000.0, y: c.northing_m * 1000.0, z: c.level_m.unwrap_or(0.0) * 1000.0, rotation_deg: 0.0 };
            let link = (!alignment.is_empty()).then(|| super::alignment::Link { alignment: alignment.clone(), station_m: c.station_m, offset_m: c.offset_m });
            let placement = self.root_placement_on(link, base);
            let owner = format!("element/{}/column/{}", e.id.0, index + 1);
            let assembly = self.make("IFCELEMENTASSEMBLY", &self.w.guid(&owner), &c.mark, Some(&format!("Lighting column, {}", c.side.label())), None, placement, None, &format!("{}.{}", e.id.0, index + 1), true, "MAST");
            let luminaires = c.bracket_directions_rad.len();
            let mut p = vec![
                ("Mark".to_string(), text(&c.mark)),
                ("Category".to_string(), text(&e.category)),
                ("Layer".to_string(), text(&e.layer)),
                ("Installation".to_string(), text(&e.name)),
                ("Side".to_string(), text(c.side.label())),
                ("StationLabel".to_string(), text(&c.station_label)),
                ("BearingDeg".to_string(), Value::Real(c.bearing_deg())),
                ("MountingHeightM".to_string(), Value::Real(spec.mounting_height_m)),
                ("BracketOutreachM".to_string(), Value::Real(spec.bracket_outreach_m)),
                ("TiltDeg".to_string(), Value::Real(spec.tilt_deg)),
                ("Luminaires".to_string(), Value::Integer(luminaires as i64)),
            ];
            if let Some(l) = c.level_m {
                p.push(("LevelM".to_string(), Value::Real(l)));
            }
            if !spec.column_type.trim().is_empty() {
                p.push(("ColumnType".to_string(), text(&spec.column_type)));
            }
            if !data.definition.lighting_class().trim().is_empty() {
                p.push(("LightingClass".to_string(), text(data.definition.lighting_class())));
            }
            stations(&mut p, &alignment, Some(c.station_m), None, Some(c.offset_m));
            if let Some(b) = e.properties.get(PROP_DESIGN_BASIS).filter(|b| !b.is_empty()) {
                p.push(("DesignBasis".to_string(), text(b)));
            }
            self.w.property_set_of(assembly, &owner, "AfroBIM_LightingColumn", &p);
            self.w.property_set_of(assembly, &owner, "Pset_ElementAssemblyCommon", &[("Reference".to_string(), Value::Identifier(c.mark.clone()))]);
            self.place_properties(assembly, &owner, &base, &alignment, Some(c.station_m), Some(c.offset_m));
            self.position_on(assembly, &alignment);

            // The solids of this column, moved from the element's frame to the column's base.
            let shift = [site.x - base.x, site.y - base.y, site.z - base.z];
            let mut parts = Vec::new();
            let (mut brackets, mut fixtures) = (0, 0);
            for mesh in solids.iter().filter(|m| m.name.ends_with(&format!(" {}", c.mark))) {
                let kind = mesh.name.split(' ').next().unwrap_or("Part");
                let part_placement = self.w.placement(placement, Pose { x: 0.0, y: 0.0, z: 0.0, rotation_deg: 0.0 });
                let (entity, predefined, role, style) = match kind {
                    "Column" => ("IFCMEMBER", "POST", "Pole", "Lighting column"),
                    "Bracket" => ("IFCMEMBER", "NOTDEFINED", "Bracket", "Lighting bracket"),
                    _ => ("IFCLIGHTFIXTURE", "NOTDEFINED", "Luminaire", "Luminaire"),
                };
                let n = match kind {
                    "Bracket" => {
                        brackets += 1;
                        brackets
                    }
                    "Luminaire" => {
                        fixtures += 1;
                        fixtures
                    }
                    _ => 1,
                };
                let part_owner = format!("{owner}/{}/{n}", role.to_lowercase());
                let shape = self.body_of(&[(style, shifted(mesh, shift))], &e.name, notes);
                let name = if n > 1 || (luminaires > 1 && role != "Pole") { format!("{} {} {n}", c.mark, role.to_lowercase()) } else { format!("{} {}", c.mark, role.to_lowercase()) };
                let part = self.make(entity, &self.w.guid(&part_owner), &name, None, None, part_placement, shape, &format!("{}.{}", e.id.0, index + 1), false, predefined);
                let mut props = vec![("Mark".to_string(), text(&c.mark)), ("Role".to_string(), text(role))];
                match role {
                    "Pole" => {
                        self.w.property_set_of(part, &part_owner, "Pset_MemberCommon", &[("IsExternal".to_string(), Value::Boolean(true))]);
                    }
                    "Bracket" => props.push(("OutreachM".to_string(), Value::Real(spec.bracket_outreach_m))),
                    _ => {
                        let l = &spec.luminaire;
                        props.push(("Product".to_string(), text(&l.product)));
                        props.push(("LumenOutputLm".to_string(), Value::Real(l.lumen_output_lm)));
                        props.push(("PowerW".to_string(), Value::Real(l.power_w)));
                        props.push(("TiltDeg".to_string(), Value::Real(spec.tilt_deg)));
                        if let Some(v) = &l.photometric_file {
                            props.push(("PhotometricFile".to_string(), text(v)));
                        }
                        if let Some(v) = &l.luminous_intensity_class {
                            props.push(("LuminousIntensityClass".to_string(), text(v)));
                        }
                        if let Some(v) = l.cct_k {
                            props.push(("CctK".to_string(), Value::Real(v)));
                        }
                        if let Some(v) = l.cri {
                            props.push(("Cri".to_string(), Value::Real(v)));
                        }
                        if let Some(v) = l.led {
                            props.push(("Led".to_string(), Value::Boolean(v)));
                        }
                        if let Some(v) = &l.risk_group {
                            props.push(("RiskGroup".to_string(), text(v)));
                        }
                        if let Some(v) = l.mass_kg {
                            props.push(("MassKg".to_string(), Value::Real(v)));
                        }
                        if let Some(v) = l.windage_m2 {
                            props.push(("WindageM2".to_string(), Value::Real(v)));
                        }
                        // Pset_LightFixtureTypeCommon (IFC 4.3, IfcLightFixture).
                        let mut common = vec![("TotalWattage".to_string(), Value::Measure("IFCPOWERMEASURE", l.power_w))];
                        if !l.product.trim().is_empty() {
                            common.push(("Reference".to_string(), Value::Identifier(l.product.clone())));
                        }
                        self.w.property_set_of(part, &part_owner, "Pset_LightFixtureTypeCommon", &common);
                    }
                }
                self.w.property_set_of(part, &part_owner, "AfroBIM_LightingPart", &props);
                parts.push(part);
            }
            if parts.is_empty() {
                notes.push(format!("{}: no solids for column {}; its assembly has no geometry", e.name, c.mark));
            }
            self.aggregate(&owner, assembly, &parts);
            columns.push(assembly);
        }
        if columns.is_empty() {
            notes.push(format!("{}: no column positions; nothing exported", e.name));
            return columns;
        }
        if installation && columns.len() > 1 {
            let group = self.group(e, &format!("Lighting installation of {} columns", columns.len()), &columns);
            let mut p = identity(e);
            stations(&mut p, &alignment, data.layout.columns.first().map(|c| c.station_m), data.layout.columns.last().map(|c| c.station_m), None);
            p.push(("Arrangement".to_string(), text(data.definition.arrangement_label())));
            p.push(("Columns".to_string(), Value::Integer(data.layout.columns.len() as i64)));
            p.push(("Luminaires".to_string(), Value::Integer(data.luminaire_count() as i64)));
            p.push(("ConnectedLoadW".to_string(), Value::Real(data.connected_load_w())));
            if !data.definition.lighting_class().trim().is_empty() {
                p.push(("LightingClass".to_string(), text(data.definition.lighting_class())));
            }
            self.element_sets_at(group, e, g, "AfroBIM_LightingInstallation", p, None, false);
        }
        columns
    }

    // ─── road restraint ─────────────────────────────────────────────────────────────────────

    /// A barrier run: `IfcRailing` `GUARDRAIL`, with its terminals and transitions as parts.
    fn restraint(&mut self, input: &IfcElementInput, g: Option<&Generated>, notes: &mut Vec<String>) -> Vec<u32> {
        let e = input.element;
        let barrier = BarrierElement::from_value(&e.data).ok();
        let pose = Self::road_pose(e, notes);
        let placement = self.root_placement(e, pose);
        let meshes: Vec<(&str, Cow<Mesh3>)> = g
            .iter()
            .flat_map(|g| g.solids.iter())
            .map(|m| (if m.name.contains("posts") { "Barrier posts" } else { "Barrier beam" }, Cow::Borrowed(m)))
            .collect();
        let shape = self.body_of(&meshes, &e.name, notes);
        if shape.is_none() {
            notes.push(format!("{}: no geometry exported, the run has no solids", e.name));
        }
        let description = barrier.as_ref().map(|b| format!("{} barrier run, {}", b.run.containment.label(), b.run.system));
        let run = self.make(
            "IFCRAILING",
            &element_global_id(&self.w.options.project_name, e.id),
            &e.name,
            description.as_deref().map(str::trim),
            None,
            placement,
            shape,
            &e.id.0.to_string(),
            false,
            "GUARDRAIL",
        );
        let mut p = identity(e);
        let mut parts = Vec::new();
        if let Some(b) = &barrier {
            let (r, res) = (&b.run, &b.resolved);
            p.push(("StartStation".to_string(), text(&res.start_label)));
            p.push(("EndStation".to_string(), text(&res.end_label)));
            p.push(("TrafficFace".to_string(), text(r.traffic_face.label())));
            p.push(("ContainmentLevel".to_string(), text(r.containment.label())));
            p.push(("WorkingWidthClass".to_string(), text(r.working_width.label())));
            p.push(("ImpactSeverityLevel".to_string(), text(r.impact_severity.label())));
            if let Some(v) = r.vehicle_intrusion {
                p.push(("VehicleIntrusionClass".to_string(), text(v.label())));
            }
            if !r.system.trim().is_empty() {
                p.push(("System".to_string(), text(&r.system)));
            }
            if let Some(s) = r.post_spacing_m {
                p.push(("PostSpacingM".to_string(), Value::Real(s)));
            }
            p.push(("Posts".to_string(), Value::Integer(res.posts.len() as i64)));
            p.push(("BeamHeightM".to_string(), Value::Real(r.beam_height_m)));
            p.push(("StartEnd".to_string(), text(&r.start_end.label())));
            p.push(("EndEnd".to_string(), text(&r.end_end.label())));
            if let Some(v) = r.set_back_m {
                p.push(("SetBackM".to_string(), Value::Real(v)));
            }
            stations(&mut p, &r.alignment, Some(r.start_station_m), Some(r.end_station_m), Some(r.offset_m));
            // Terminals and transitions, at the two ends of the barrier line.
            for (which, end, point) in [("start", &r.start_end, res.line.first()), ("end", &r.end_end, res.line.last())] {
                if let Some(part) = self.barrier_end(e, placement, &pose, which, end, point, &r.alignment, notes) {
                    parts.push(part);
                }
            }
        } else {
            notes.push(format!("{}: its barrier data cannot be read; exported without it", e.name));
        }
        self.element_sets(run, e, g, "AfroBIM_BarrierRun", p, None);
        // Pset_RoadGuardElement and Pset_RailingCommon (IFC 4.3, IfcRailing GUARDRAIL): only what
        // the model states; a barrier run is neither a terminal nor a transition, and stands outdoors.
        self.w.property_set(
            run,
            e.id,
            "Pset_RoadGuardElement",
            &[("IsTerminal".to_string(), Value::Boolean(false)), ("IsTransition".to_string(), Value::Boolean(false))],
        );
        self.w.property_set(run, e.id, "Pset_RailingCommon", &[("IsExternal".to_string(), Value::Boolean(true))]);
        if let Some(b) = &barrier {
            let q = RunQuantities::measure(&b.run, &b.resolved);
            self.w.quantities(run, e.id, "Qto_RailingBaseQuantities", &[Quantity::Length("Length", q.length_m * 1000.0)]);
            let terminals = [&b.run.start_end, &b.run.end_end].iter().filter(|t| matches!(t, EndTreatment::Terminal { .. })).count();
            self.w.quantities(
                run,
                e.id,
                "AfroBIM_Quantities",
                &[
                    Quantity::Length("Length", q.length_m * 1000.0),
                    Quantity::Length("CentrelineLength", q.centreline_length_m * 1000.0),
                    Quantity::Count("Posts", q.posts as i64),
                    Quantity::Count("Terminals", terminals as i64),
                ],
            );
        }
        self.aggregate(&format!("element/{}", e.id.0), run, &parts);
        if let Some(a) = valid_site(e).and_then(|s| s.alignment.clone()) {
            self.position_on(run, &a);
        }
        vec![run]
    }

    /// A terminal or a transition at one end of a run: `IfcRailing` `GUARDRAIL` with
    /// `Pset_RoadGuardElement` `IsTerminal` or `IsTransition`. The module draws no geometry for
    /// either, so the product has a position and properties but no shape.
    #[allow(clippy::too_many_arguments)]
    fn barrier_end(
        &mut self,
        e: &Element,
        run_placement: u32,
        run_pose: &Pose,
        which: &str,
        end: &EndTreatment,
        point: Option<&afrobim_restraint::RunPoint>,
        alignment: &str,
        notes: &mut Vec<String>,
    ) -> Option<u32> {
        let (role, terminal_type, mut props) = match end {
            EndTreatment::Terminal { performance_class, energy_absorbing, description } => {
                let mut props = vec![("PerformanceClass".to_string(), text(&performance_class.label()))];
                if let Some(v) = energy_absorbing {
                    props.push(("EnergyAbsorbing".to_string(), Value::Boolean(*v)));
                }
                if !description.trim().is_empty() {
                    props.push(("Description".to_string(), text(description)));
                }
                ("terminal", Some(performance_class.label()), props)
            }
            EndTreatment::ConnectsTo { system, name, transition: Some(t) } => {
                let mut props = vec![("ConnectedSystem".to_string(), text(system.label())), ("ConnectedTo".to_string(), text(name.trim()))];
                if !t.description.trim().is_empty() {
                    props.push(("Description".to_string(), text(&t.description)));
                }
                if let Some(c) = t.containment {
                    props.push(("ContainmentLevel".to_string(), text(c.label())));
                }
                if let Some(c) = t.impact_severity {
                    props.push(("ImpactSeverityLevel".to_string(), text(c.label())));
                }
                if let Some(c) = t.connected_containment {
                    props.push(("ConnectedContainmentLevel".to_string(), text(c.label())));
                }
                ("transition", None, props)
            }
            _ => return None,
        };
        let owner = format!("element/{}/{which}-{role}", e.id.0);
        // The placement is the point of the barrier line the end is at, in the axes of the run.
        let pose = match point {
            Some(p) => Pose { x: p.x * 1000.0, y: p.y * 1000.0, z: p.z * 1000.0, rotation_deg: p.heading_rad.to_degrees() }.relative_to(run_pose),
            None => Pose { x: 0.0, y: 0.0, z: 0.0, rotation_deg: 0.0 },
        };
        let placement = self.w.placement(run_placement, pose);
        let name = format!("{}, {which} {role}", e.name);
        let part = self.make("IFCRAILING", &self.w.guid(&owner), &name, Some(&format!("{} at the {which} of the run (no geometry modelled)", if role == "terminal" { "Terminal" } else { "Transition" })), None, placement, None, &e.id.0.to_string(), false, "GUARDRAIL");
        let mut guard = vec![("IsTerminal".to_string(), Value::Boolean(role == "terminal")), ("IsTransition".to_string(), Value::Boolean(role == "transition"))];
        if let Some(t) = terminal_type.filter(|t| !t.is_empty()) {
            guard.push(("TerminalType".to_string(), Value::Text(t)));
        }
        self.w.property_set_of(part, &owner, "Pset_RoadGuardElement", &guard);
        props.insert(0, ("Mark".to_string(), text(&e.name)));
        props.insert(1, ("Role".to_string(), text(&format!("{which} {role}"))));
        self.w.property_set_of(part, &owner, "AfroBIM_BarrierEnd", &props);
        if let Some(p) = point {
            self.place_properties(part, &owner, &Pose { x: p.x * 1000.0, y: p.y * 1000.0, z: p.z * 1000.0, rotation_deg: p.heading_rad.to_degrees() }, alignment, Some(p.station_m), None);
        }
        notes.push(format!("{}: the {which} {role} has no geometry in the model; exported as a product with a position and properties only", e.name));
        Some(part)
    }

    // ─── pavements ──────────────────────────────────────────────────────────────────────────

    /// A pavement: `IfcPavement` aggregating one `IfcCourse` `PAVEMENT` per layer slab.
    fn pavement(&mut self, input: &IfcElementInput, g: Option<&Generated>, notes: &mut Vec<String>) -> Vec<u32> {
        let e = input.element;
        let def = PavementDefinition::from_value(&e.data).ok();
        let pose = Self::road_pose(e, notes);
        let placement = self.root_placement(e, pose);
        let rows = def.as_ref().and_then(|d| measure_definition(&e.name, d).ok()).unwrap_or_default();
        // FLEXIBLE is "pavement with a bituminous surfacing": claimed only when the top layer of
        // the carriageway is one of the two bituminous surfacings the model names.
        let flexible = def.as_ref().and_then(|d| d.layers.first()).is_some_and(|l| matches!(l.material, Material::HotMixAsphalt | Material::SurfaceDressing));
        let description = def.as_ref().map(|d| format!("{} layer(s), carriageway {} m wide", d.layers.len(), mm(d.carriageway_width_m)));
        let pavement = self.make(
            "IFCPAVEMENT",
            &element_global_id(&self.w.options.project_name, e.id),
            &e.name,
            description.as_deref(),
            None,
            placement,
            None,
            &e.id.0.to_string(),
            false,
            if flexible { "FLEXIBLE" } else { "NOTDEFINED" },
        );
        let mut p = identity(e);
        let total_volume: f64 = rows.iter().map(|r| r.volume_m3).sum();
        if let Some(d) = &def {
            p.push(("CarriagewayWidthM".to_string(), Value::Real(d.carriageway_width_m)));
            p.push(("CrossfallPercent".to_string(), Value::Real(d.crossfall_percent)));
            p.push(("CrossfallShape".to_string(), text(&format!("{:?}", d.crossfall_shape))));
            p.push(("Layers".to_string(), Value::Integer(d.layers.len() as i64)));
            p.push(("LeftShoulder".to_string(), Value::Boolean(d.left_shoulder.is_some())));
            p.push(("RightShoulder".to_string(), Value::Boolean(d.right_shoulder.is_some())));
            let area_only: Vec<String> = rows.iter().filter(|r| r.thickness_mm <= 0.0).map(|r| format!("{}: {}", r.strip, r.layer)).collect();
            if !area_only.is_empty() {
                p.push(("AreaOnlyLayers".to_string(), text(&area_only.join("; "))));
            }
            stations(&mut p, &d.alignment.name, Some(d.start_station_m), Some(d.end_station_m), Some(d.centre_offset_m));
        } else {
            notes.push(format!("{}: its definition cannot be read; exported without its pavement data", e.name));
        }
        self.element_sets(pavement, e, g, "AfroBIM_Pavement", p, None);
        if let Some(d) = &def {
            // Pset_PavementCommon, Qto_PavementBaseQuantities (IFC 4.3, IfcPavement).
            let thickness: f64 = d.layers.iter().map(|l| l.thickness_mm).sum();
            let common = vec![
                ("Reference".to_string(), Value::Identifier(e.name.clone())),
                ("NominalLength".to_string(), length_non_negative(d.length_m() * 1000.0)),
                ("NominalWidth".to_string(), length_non_negative(d.carriageway_width_m * 1000.0)),
                ("NominalThickness".to_string(), length_non_negative(thickness)),
            ];
            self.w.property_set(pavement, e.id, "Pset_PavementCommon", &common);
            self.w.quantities(pavement, e.id, "Qto_PavementBaseQuantities", &[Quantity::Length("Length", d.length_m() * 1000.0), Quantity::Volume("NetVolume", total_volume)]);
            let carriageway_area = rows.iter().find(|r| r.strip == "Carriageway").map_or(0.0, |r| r.area_m2);
            self.w.quantities(
                pavement,
                e.id,
                "AfroBIM_Quantities",
                &[
                    Quantity::Length("Length", d.length_m() * 1000.0),
                    Quantity::Area("CarriagewayArea", carriageway_area),
                    Quantity::Volume("TotalVolume", total_volume),
                    Quantity::Count("Slabs", rows.iter().filter(|r| r.thickness_mm > 0.0).count() as i64),
                ],
            );
            self.road.pavement_products.insert(e.id, pavement);
        }

        // One course per slab. The slabs are named "<strip>: <layer>", and the measured rows with
        // a thickness come in the same order (a layer of no thickness has no slab).
        let mut rows = rows.iter().filter(|r| r.thickness_mm > 0.0);
        let mut courses = Vec::new();
        for (i, mesh) in g.iter().flat_map(|g| g.solids.iter()).enumerate() {
            let row = rows.next().filter(|r| format!("{}: {}", r.strip, r.layer) == mesh.name);
            let part_placement = self.w.placement(placement, Pose { x: 0.0, y: 0.0, z: 0.0, rotation_deg: 0.0 });
            let shape = self.body_of(&[(mesh.name.as_str(), Cow::Borrowed(mesh))], &e.name, notes);
            let owner = format!("element/{}/course/{}", e.id.0, i + 1);
            let name = format!("{}, {}", e.name, mesh.name);
            let course = self.make("IFCCOURSE", &self.w.guid(&owner), &name, None, None, part_placement, shape, &e.id.0.to_string(), false, "PAVEMENT");
            let mut props = vec![("Mark".to_string(), text(&e.name)), ("Slab".to_string(), text(&mesh.name))];
            if let Some(r) = row {
                props.push(("Strip".to_string(), text(&r.strip)));
                props.push(("Layer".to_string(), text(&r.layer)));
                props.push(("Material".to_string(), text(&r.material)));
                props.push(("ThicknessMm".to_string(), Value::Length(r.thickness_mm)));
                props.push(("MeanOffsetM".to_string(), Value::Real(r.mean_offset_m)));
                if !r.material.trim().is_empty() {
                    self.materials.entry(r.material.trim().to_string()).or_default().push(course);
                }
                let common = vec![
                    ("NominalLength".to_string(), length_non_negative(r.strip_length_m * 1000.0)),
                    ("NominalWidth".to_string(), length_non_negative(r.width_m * 1000.0)),
                    ("NominalThickness".to_string(), length_non_negative(r.thickness_mm)),
                ];
                self.w.property_set_of(course, &owner, "Pset_CourseCommon", &common);
                self.w.quantities_of(
                    course,
                    &owner,
                    "Qto_CourseBaseQuantities",
                    &[
                        Quantity::Length("Length", r.strip_length_m * 1000.0),
                        Quantity::Length("Width", r.width_m * 1000.0),
                        Quantity::Length("Thickness", r.thickness_mm),
                        Quantity::Volume("Volume", r.volume_m3),
                    ],
                );
                self.w.quantities_of(course, &owner, "AfroBIM_Quantities", &[Quantity::Area("PlanArea", r.area_m2)]);
            } else {
                notes.push(format!("{}: slab '{}' has no matching measured layer; exported without quantities", e.name, mesh.name));
            }
            self.w.property_set_of(course, &owner, "AfroBIM_PavementLayer", &props);
            courses.push(course);
        }
        if courses.is_empty() {
            notes.push(format!("{}: no slabs; the pavement has no geometry", e.name));
        }
        self.aggregate(&format!("element/{}", e.id.0), pavement, &courses);
        if let Some(a) = valid_site(e).and_then(|s| s.alignment.clone()) {
            self.position_on(pavement, &a);
        }
        vec![pavement]
    }

    // ─── corridors and ground surfaces ──────────────────────────────────────────────────────

    /// A corridor: `IfcBuildingElementProxy` (`USERDEFINED`, `ObjectType` `corridor`) with its
    /// finished surface and its cut and fill slopes as open tessellated surfaces.
    fn corridor(&mut self, input: &IfcElementInput, g: Option<&Generated>, notes: &mut Vec<String>) -> Vec<u32> {
        let e = input.element;
        let data = afrobim_corridor::corridor_element(self.document, e.id);
        let built = afrobim_corridor::corridor_of(self.document, e.id);
        let (pose, meshes) = match &built {
            Ok(c) if !c.sections.is_empty() => {
                let m = afrobim_corridor::corridor_meshes(c);
                // The built surfaces have their own origin, the least plan point of the sections.
                (
                    Pose { x: m.origin_x_m * 1000.0, y: m.origin_y_m * 1000.0, z: 0.0, rotation_deg: 0.0 },
                    [("Corridor top", m.top), ("Cut slope", m.cut_slopes), ("Fill slope", m.fill_slopes)].into_iter().filter(|(_, m)| !m.triangles.is_empty()).collect::<Vec<_>>(),
                )
            }
            other => {
                let why = match other {
                    Ok(_) => "it has no sections".to_string(),
                    Err(error) => error.to_string(),
                };
                notes.push(format!("{}: its surfaces cannot be built ({why}); exported without geometry", e.name));
                (Self::road_pose(e, notes), Vec::new())
            }
        };
        let placement = self.root_placement(e, pose);
        let items: Vec<(&str, Cow<Mesh3>)> = meshes.iter().map(|(k, m)| (*k, Cow::Borrowed(m))).collect();
        let shape = self.body_of(&items, &e.name, notes);
        let description = data.as_ref().and_then(|d| d.computed.as_ref().map(|c| format!("Corridor '{}': finished surface and side slopes, stations {} to {} m", d.definition.name, mm(c.start_station_m), mm(c.end_station_m))));
        let product = self.make(
            "IFCBUILDINGELEMENTPROXY",
            &element_global_id(&self.w.options.project_name, e.id),
            &e.name,
            description.as_deref(),
            Some("corridor"),
            placement,
            shape,
            &e.id.0.to_string(),
            false,
            "USERDEFINED",
        );
        notes.push(format!("{}: category 'corridor' has no specific IFC entity; exported as IfcBuildingElementProxy", e.name));
        let mut p = identity(e);
        if let Some(d) = &data {
            let name_of = |id: ElementId| self.by_id.get(&id).map(|i| i.element.name.clone());
            if let Some(n) = name_of(d.alignment) {
                p.push(("Alignment".to_string(), text(&n)));
            }
            if let Some(n) = name_of(d.surface) {
                p.push(("GroundSurface".to_string(), text(&n)));
            }
            p.push(("Definition".to_string(), text(&d.definition.name)));
            p.push(("TemplateRegions".to_string(), Value::Integer(d.definition.regions.len() as i64)));
            p.push(("IntervalM".to_string(), Value::Real(d.definition.interval_m)));
            if let Some(c) = &d.computed {
                p.push(("StartStationM".to_string(), Value::Real(c.start_station_m)));
                p.push(("EndStationM".to_string(), Value::Real(c.end_station_m)));
            }
        } else {
            notes.push(format!("{}: its corridor data cannot be read; exported without it", e.name));
        }
        self.element_sets(product, e, g, "AfroBIM_Corridor", p, None);
        if let Some(c) = data.as_ref().and_then(|d| d.computed.as_ref()) {
            self.w.quantities(
                product,
                e.id,
                "AfroBIM_Quantities",
                &[
                    Quantity::Volume("TotalCut", c.total_cut_m3),
                    Quantity::Volume("TotalFill", c.total_fill_m3),
                    Quantity::Length("UnmeasuredLength", c.unmeasured_length_m * 1000.0),
                    Quantity::Count("Sections", c.sections as i64),
                    Quantity::Count("SectionsWithoutAreas", c.sections_without_areas as i64),
                ],
            );
        }
        vec![product]
    }

    /// A ground surface: `IfcGeographicElement` `TERRAIN`, contained in the site, as the surface
    /// the module shows (the whole TIN, or a regular resampling above its display cap).
    fn surface(&mut self, input: &IfcElementInput, g: Option<&Generated>, notes: &mut Vec<String>) -> Vec<u32> {
        let e = input.element;
        let pose = Self::road_pose(e, notes);
        let placement = self.w.placement(self.w.site_placement, pose);
        let meshes: Vec<(&str, Cow<Mesh3>)> = g.iter().flat_map(|g| g.solids.iter()).map(|m| ("Ground surface", Cow::Borrowed(m))).collect();
        let shape = self.body_of(&meshes, &e.name, notes);
        if shape.is_none() {
            notes.push(format!("{}: no geometry exported, the surface has no mesh", e.name));
        }
        if g.is_some_and(|g| g.notes.iter().any(|n| n.code == "surface.display_resampled")) {
            notes.push(format!("{}: the surface is exported as the regular grid the module resamples it to for display, not as its TIN", e.name));
        }
        let product = self.make("IFCGEOGRAPHICELEMENT", &element_global_id(&self.w.options.project_name, e.id), &e.name, None, None, placement, shape, &e.id.0.to_string(), false, "TERRAIN");
        let mut p = identity(e);
        if let Ok(d) = serde_json::from_value::<afrobim_corridor::SurfaceDefinition>(e.data.clone()) {
            if let Some(s) = d.source.filter(|s| !s.is_empty()) {
                p.push(("Source".to_string(), text(&s)));
            }
        }
        self.element_sets(product, e, g, "AfroBIM_Surface", p, None);
        self.road.in_site.push(product);
        Vec::new()
    }
}

/// The culvert's own properties, shared by its barrels.
fn culvert_properties(p: &mut Vec<(String, Value)>, d: &afrobim_drainage::PipeCulvertDefinition, notes: &mut Vec<String>, mark: &str, first: bool) {
    p.push(("Barrels".to_string(), Value::Integer(i64::from(d.barrels))));
    p.push(("InternalDiameterMm".to_string(), Value::Length(d.internal_diameter_mm)));
    p.push(("WallThicknessMm".to_string(), Value::Length(d.wall_thickness_mm)));
    p.push(("LengthPerBarrelMm".to_string(), Value::Length(d.length_mm)));
    p.push(("SkewDeg".to_string(), Value::Real(d.skew_deg)));
    p.push(("InvertLevelInletM".to_string(), Value::Real(d.invert_level_inlet_m)));
    p.push(("InvertLevelOutletM".to_string(), Value::Real(d.invert_level_outlet_m)));
    if let Some(v) = d.road_level_m {
        p.push(("RoadLevelM".to_string(), Value::Real(v)));
    }
    if !d.material.trim().is_empty() {
        p.push(("Material".to_string(), text(&d.material)));
    }
    stations(p, &d.alignment, Some(d.station_m), None, Some(d.centre_offset_m));
    if d.placeholder {
        p.push(("PlaceholderDimensions".to_string(), Value::Boolean(true)));
        if first {
            notes.push(format!("{mark}: its dimensions are placeholders, not design values"));
        }
    }
}

/// The pattern of a line type: both lines of a double line.
fn entity_label(feature: bool) -> &'static str {
    if feature { "IfcSurfaceFeature" } else { "IfcBuildingElementProxy (no pavement carries it);" }
}

/// A width from the element, or the only one the catalogue allows; `None` when neither (an
/// element that cannot be laid out has no width to give).
fn chosen_mm(given: Option<u32>, allowed: &[u32]) -> Option<u32> {
    match given {
        Some(v) if allowed.contains(&v) => Some(v),
        None if allowed.len() == 1 => Some(allowed[0]),
        _ => None,
    }
}

/// The properties of `Pset_MarkingLinesCommon` for a second-set line marking, or the reason it is
/// left out. The dash pattern must be the same for the whole element: a continuous line, or one
/// mark and gap module. So:
///
/// * a longitudinal line (or a double line whose two lines have the same pattern) qualifies, with
///   the pattern of its urban or rural setting;
/// * a transverse line qualifies, its pattern being the one catalogued for the line;
/// * a zig-zag line (its marks follow the zig-zag, not a straight dash module), a pedestrian
///   crossing with side lines (the side-line mark lengths are chosen within a range), a double line
///   of two different patterns and a catalogue entry with no layout are left out.
///
/// `NominalWidth` is the overall width as in the first set: every line and the gap between lines.
fn ext_line_pset(def: Option<&ExtDefinition>, type_id: &str) -> Result<Vec<(String, Value)>, String> {
    let (Some(d), Some(spec)) = (def, ext_spec(type_id)) else { return Err("the definition has no catalogue entry".into()) };
    let (pattern, overall_mm) = match spec.kind {
        Kind::Lines(l) => {
            let urban = match (l.by_setting, d.setting) {
                (true, None) => return Err("it has different marks in an urban and a rural setting and none is set".into()),
                (true, Some(s)) => s == Setting::Urban,
                (false, _) => true,
            };
            let patterns: Vec<Pattern> = l.strands.iter().map(|s| if urban { s.urban } else { s.rural }).collect();
            let Some(first) = patterns.first().copied() else { return Err("it has no lines".into()) };
            if patterns.iter().any(|p| *p != first) {
                return Err("its two lines have different dash patterns, so there is no single pattern for the element".into());
            }
            let Some(w) = chosen_mm(d.width_mm, l.widths_mm) else { return Err("its line width is not set".into()) };
            let sep = if l.strands.len() == 2 {
                match chosen_mm(d.separation_mm, l.separations_mm) {
                    Some(s) => s,
                    None => return Err("the gap between its lines is not set".into()),
                }
            } else {
                0
            };
            (first, w * l.strands.len() as u32 + sep)
        }
        Kind::Transverse(t) => {
            let Some(w) = chosen_mm(d.width_mm, t.widths_mm) else { return Err("its line width is not set".into()) };
            (t.pattern, w * t.rows + t.row_gap_mm * t.rows.saturating_sub(1))
        }
        Kind::ZigZag(_) => return Err("the marks of a zig-zag line follow the zig-zag, not one straight mark and gap module".into()),
        Kind::Crossing(_) => return Err("the side lines of a crossing have mark lengths chosen within a range, not one module".into()),
        _ => return Err("the catalogue entry has no line layout to take a pattern from".into()),
    };
    let dashed = matches!(pattern, Pattern::Broken { .. });
    Ok(vec![
        ("DashedLine".to_string(), Value::Boolean(dashed)),
        ("DashedLinePattern".to_string(), text(&pattern.label())),
        ("NominalWidth".to_string(), length_non_negative(f64::from(overall_mm))),
    ])
}

fn pattern_label(spec: &afrobim_markings::LineSpec) -> String {
    match spec.double {
        Some(d) => format!("left line {}; right line {}", d.left.label(), d.right.label()),
        None => spec.pattern.label(),
    }
}
