//! The civil objects of the application in the model export, from a package the application
//! builds for each object: corridor shapes as pavement courses, kerbs and earthworks, pipe
//! networks as pipe segments, chambers and a distribution system.
//!
//! The exporter writes nothing it has not verified. Every entity and predefined type below was
//! looked up in the IFC4X3_ADD2 schema through IfcOpenShell (the read-back tests do it again for
//! every file they write), and so was every quantity set name and the quantities in it. A part
//! whose entity or predefined type is not in [`ENTITIES`], or a quantity set or quantity that is not in
//! [`QUANTITY_SETS`], is not written and the export says so.

use std::borrow::Cow;

use afrobim_draw::Mesh3;
use afrobim_kernel::Generated;
use serde::{Deserialize, Serialize};

use super::super::{valid_site, Model, Pose, Quantity};
use crate::step::refs;
use crate::{clip, step_string, IfcElementInput, Value};

/// Category of the package elements the application hands over (see [`CivilPackage`]).
pub const CIVIL_CATEGORY: &str = "civil-ifc";

/// The entities a part may be, with the predefined types the schema gives each. `IFCCIVILELEMENT`
/// has none.
pub const ENTITIES: &[(&str, &[&str])] = &[
    ("IFCPAVEMENT", &["FLEXIBLE", "RIGID", "USERDEFINED", "NOTDEFINED"]),
    ("IFCCOURSE", &["ARMOUR", "BALLASTBED", "CORE", "FILTER", "PAVEMENT", "PROTECTION", "USERDEFINED", "NOTDEFINED"]),
    ("IFCKERB", &["USERDEFINED", "NOTDEFINED"]),
    ("IFCEARTHWORKSCUT", &["BASE_EXCAVATION", "CUT", "DREDGING", "EXCAVATION", "OVEREXCAVATION", "PAVEMENTMILLING", "STEPEXCAVATION", "TOPSOILREMOVAL", "TRENCH", "USERDEFINED", "NOTDEFINED"]),
    ("IFCEARTHWORKSFILL", &["BACKFILL", "COUNTERWEIGHT", "EMBANKMENT", "SLOPEFILL", "SUBGRADE", "SUBGRADEBED", "TRANSITIONSECTION", "USERDEFINED", "NOTDEFINED"]),
    ("IFCPIPESEGMENT", &["CULVERT", "FLEXIBLESEGMENT", "GUTTER", "RIGIDSEGMENT", "SPOOL", "USERDEFINED", "NOTDEFINED"]),
    ("IFCDISTRIBUTIONCHAMBERELEMENT", &["FORMEDDUCT", "INSPECTIONCHAMBER", "INSPECTIONPIT", "MANHOLE", "METERCHAMBER", "SUMP", "TRENCH", "VALVECHAMBER", "USERDEFINED", "NOTDEFINED"]),
    ("IFCCIVILELEMENT", &[]),
];

/// The standard quantity sets a part may carry, with the quantities each has (IFC4X3 templates).
pub const QUANTITY_SETS: &[(&str, &[&str])] = &[
    ("Qto_CourseBaseQuantities", &["Length", "Width", "Thickness", "Volume", "GrossVolume", "Weight"]),
    ("Qto_PavementBaseQuantities", &["Length", "Width", "Depth", "GrossArea", "NetArea", "GrossVolume", "NetVolume", "Weight"]),
    ("Qto_KerbBaseQuantities", &["Length", "Width", "Height", "Depth", "Volume", "Weight"]),
    ("Qto_EarthworksCutBaseQuantities", &["Length", "Width", "Depth", "UndisturbedVolume", "LooseVolume", "Weight"]),
    ("Qto_EarthworksFillBaseQuantities", &["Length", "Width", "Depth", "CompactedVolume", "LooseVolume"]),
    ("Qto_PipeSegmentBaseQuantities", &["Length", "GrossCrossSectionArea", "NetCrossSectionArea", "OuterSurfaceArea", "GrossWeight", "NetWeight", "FootPrintArea"]),
    ("Qto_DistributionChamberElementBaseQuantities", &["GrossSurfaceArea", "NetSurfaceArea", "GrossVolume", "NetVolume", "Depth"]),
];

/// The system types the application's network types map to (IfcDistributionSystemEnum).
pub const SYSTEM_TYPES: &[&str] = &["DRAINAGE", "RAINWATER", "SEWAGE", "STORMWATER", "WASTEWATER", "USERDEFINED", "NOTDEFINED"];

/// A value of a property or a quantity. Lengths are in millimetres, as the file's unit.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", content = "value", rename_all = "snake_case")]
pub enum CivilValue {
    Text(String),
    Identifier(String),
    Real(f64),
    Integer(i64),
    Bool(bool),
    Length(f64),
    /// IfcNonNegativeLengthMeasure, millimetres (for `Pset_CourseCommon`).
    NonNegativeLength(f64),
    /// IfcPositiveLengthMeasure, millimetres (for `Pset_PipeSegmentTypeCommon`).
    PositiveLength(f64),
    Area(f64),
    Volume(f64),
    Count(i64),
}

#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
pub struct CivilSet {
    pub name: String,
    pub items: Vec<(String, CivilValue)>,
}

/// What a group of parts becomes.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum GroupKind {
    /// An `IfcPavement` that aggregates its parts (courses).
    Pavement,
    /// An `IfcRoadPart` of the road, which contains its parts.
    RoadPart,
    /// An `IfcDistributionSystem` the parts are assigned to.
    System,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CivilGroup {
    pub name: String,
    pub kind: GroupKind,
    /// `PredefinedType` (pavement, road part or system), checked against the schema's enumeration.
    pub predefined: String,
    /// The road part that contains this group's product, an index into `groups`.
    #[serde(default)]
    pub parent: Option<usize>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CivilPart {
    pub name: String,
    /// Upper case entity name, from [`ENTITIES`].
    pub entity: String,
    pub predefined: String,
    pub material: Option<String>,
    /// The solid, an index into the `Generated::solids` of the package element.
    pub mesh: Option<usize>,
    /// The group it belongs to, an index into `groups`.
    pub group: Option<usize>,
    pub psets: Vec<CivilSet>,
    pub qtos: Vec<CivilSet>,
}

/// What the application hands the exporter for one civil object, as the `data` of an element of
/// [`CIVIL_CATEGORY`]; the solids are in the element's `Generated`, in the frame of its site placement.
#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
pub struct CivilPackage {
    pub title: String,
    pub groups: Vec<CivilGroup>,
    pub parts: Vec<CivilPart>,
    /// What the application could not map, reported with the export.
    pub not_mapped: Vec<String>,
    /// The element id of the ground surface the package was cut against: an earthworks cut is a
    /// subtraction feature, which the schema requires to void an element, and voids this one.
    #[serde(default)]
    pub ground: Option<u64>,
}

fn value_of(v: &CivilValue) -> Value {
    match v {
        CivilValue::Text(s) => Value::Text(s.clone()),
        CivilValue::Identifier(s) => Value::Identifier(s.clone()),
        CivilValue::Real(r) => Value::Real(*r),
        CivilValue::Integer(i) | CivilValue::Count(i) => Value::Integer(*i),
        CivilValue::Bool(b) => Value::Boolean(*b),
        CivilValue::Length(l) => Value::Length(*l),
        CivilValue::NonNegativeLength(l) => Value::Measure("IFCNONNEGATIVELENGTHMEASURE", *l),
        CivilValue::PositiveLength(l) => Value::Measure("IFCPOSITIVELENGTHMEASURE", *l),
        CivilValue::Area(a) => Value::Real(*a),
        CivilValue::Volume(v) => Value::Real(*v),
    }
}

/// True when `entity` with `predefined` is one the schema has.
pub fn entity_ok(entity: &str, predefined: &str) -> bool {
    ENTITIES.iter().any(|(e, p)| *e == entity && (p.is_empty() || p.contains(&predefined)))
}

impl Model<'_> {
    /// Writes a civil package: its groups and its parts. Returns the products the road contains.
    pub(super) fn civil(&mut self, input: &IfcElementInput, g: Option<&Generated>, notes: &mut Vec<String>) -> Vec<u32> {
        let e = input.element;
        let package: CivilPackage = match serde_json::from_value(e.data.clone()) {
            Ok(p) => p,
            Err(error) => {
                notes.push(format!("{}: its civil data cannot be read ({error}); not exported", e.name));
                return Vec::new();
            }
        };
        notes.extend(package.not_mapped.iter().map(|n| format!("{}: not mapped: {n}", e.name)));
        let pose = match valid_site(e) {
            Some(s) => Pose::site(s, 0.0),
            None => Pose { x: 0.0, y: 0.0, z: 0.0, rotation_deg: 0.0 },
        };
        let placement = self.root_placement(e, pose);
        let solids: &[Mesh3] = g.map_or(&[], |g| g.solids.as_slice());
        // The groups first: a part is written into one.
        let mut group_ids: Vec<Option<u32>> = Vec::new();
        for (gi, grp) in package.groups.iter().enumerate() {
            let owner = format!("element/{}/group/{gi}", e.id.0);
            let guid = self.w.guid(&owner);
            let id = match grp.kind {
                GroupKind::Pavement if entity_ok("IFCPAVEMENT", &grp.predefined) => Some(self.make("IFCPAVEMENT", &guid, &grp.name, None, None, placement, None, &e.id.0.to_string(), false, &grp.predefined)),
                GroupKind::RoadPart if ["CARRIAGEWAY", "ROADSEGMENT", "ROADSIDE", "SHOULDER", "INTERSECTION", "ROUNDABOUT", "USERDEFINED", "NOTDEFINED"].contains(&grp.predefined.as_str()) => {
                    // UsageType is not optional: a segment along the alignment is LONGITUDINAL (IfcFacilityUsageEnum), any other part says nothing.
                    let usage = if grp.predefined == "ROADSEGMENT" { "LONGITUDINAL" } else { "NOTDEFINED" };
                    let args = format!("{},$,{},$,$,#{placement},$,$,.ELEMENT.,.{usage}.,.{}.", step_string(&guid), step_string(&clip(&grp.name)), grp.predefined);
                    Some(self.w.data.add("IFCROADPART", &args))
                }
                GroupKind::System if SYSTEM_TYPES.contains(&grp.predefined.as_str()) => {
                    let args = format!("{},$,{},$,$,$,.{}.", step_string(&guid), step_string(&clip(&grp.name)), grp.predefined);
                    Some(self.w.data.add("IFCDISTRIBUTIONSYSTEM", &args))
                }
                _ => {
                    notes.push(format!("{}: group '{}' ({:?} {}) has no verified IFC entity; its parts are written without it", e.name, grp.name, grp.kind, grp.predefined));
                    None
                }
            };
            group_ids.push(id);
        }
        let mut in_group: Vec<Vec<u32>> = vec![Vec::new(); package.groups.len()];
        // A subtraction feature (an excavation) cannot be contained in a spatial element: it is aggregated into its group instead.
        let mut features: Vec<Vec<u32>> = vec![Vec::new(); package.groups.len()];
        let mut roots: Vec<u32> = Vec::new();
        for (pi, part) in package.parts.iter().enumerate() {
            if !entity_ok(&part.entity, &part.predefined) {
                notes.push(format!("{}: part '{}' ({} {}) is not an entity and type the IFC4X3_ADD2 schema has; not written", e.name, part.name, part.entity, part.predefined));
                continue;
            }
            // An earthworks cut is a subtraction feature: the schema requires it to void an element, here the ground surface it was cut against.
            let cut_terrain = if part.entity == "IFCEARTHWORKSCUT" { package.ground.and_then(|id| self.road.terrain_products.get(&afrobim_kernel::ElementId(id)).copied()) } else { None };
            if part.entity == "IFCEARTHWORKSCUT" && cut_terrain.is_none() {
                notes.push(format!("{}: the cut '{}' is not written: no ground surface is exported for it to void, which the schema requires of an earthworks cut", e.name, part.name));
                continue;
            }
            let shape = match part.mesh.and_then(|m| solids.get(m)) {
                Some(mesh) => self.body_of(&[(part.name.as_str(), Cow::Borrowed(mesh))], &part.name, notes),
                None => None,
            };
            let owner = format!("element/{}/part/{pi}", e.id.0);
            let guid = self.w.guid(&owner);
            let tag = e.id.0.to_string();
            // A part of a pavement stands at the placement of its whole, relative to it.
            let in_pavement = part.group.and_then(|gi| package.groups.get(gi)).is_some_and(|gr| gr.kind == GroupKind::Pavement);
            let placement = if in_pavement { self.w.placement(placement, Pose { x: 0.0, y: 0.0, z: 0.0, rotation_deg: 0.0 }) } else { placement };
            let product = if part.entity == "IFCCIVILELEMENT" {
                let args = format!("{},$,{},$,{},#{placement},{},{}", step_string(&guid), step_string(&clip(&part.name)), step_string(&clip(&part.predefined)), shape.map_or("$".to_string(), |s| format!("#{s}")), step_string(&tag));
                self.w.data.add("IFCCIVILELEMENT", &args)
            } else {
                self.make(&part.entity, &guid, &part.name, None, None, placement, shape, &tag, false, &part.predefined)
            };
            if shape.is_none() {
                notes.push(format!("{}: part '{}' has no solid; written without geometry", e.name, part.name));
            }
            if let Some(terrain) = cut_terrain {
                let guid = step_string(&self.w.guid(&format!("{owner}/rel-voids")));
                self.w.data.add("IFCRELVOIDSELEMENT", &format!("{guid},$,$,$,#{terrain},#{product}"));
            }
            for set in &part.psets {
                let props: Vec<(String, Value)> = set.items.iter().map(|(k, v)| (k.clone(), value_of(v))).collect();
                self.w.property_set_of(product, &owner, &set.name, &props);
            }
            for set in &part.qtos {
                let Some((_, allowed)) = QUANTITY_SETS.iter().find(|(n, _)| *n == set.name) else {
                    notes.push(format!("{}: quantity set '{}' of '{}' is not one the schema has; not written", e.name, set.name, part.name));
                    continue;
                };
                let mut items: Vec<Quantity> = Vec::new();
                for (k, v) in &set.items {
                    if !allowed.contains(&k.as_str()) {
                        notes.push(format!("{}: '{k}' is not a quantity of {}; not written", e.name, set.name));
                        continue;
                    }
                    items.push(match v {
                        CivilValue::Length(l) | CivilValue::NonNegativeLength(l) | CivilValue::PositiveLength(l) => Quantity::Length(k, *l),
                        CivilValue::Area(a) => Quantity::Area(k, *a),
                        CivilValue::Volume(v) => Quantity::Volume(k, *v),
                        CivilValue::Count(c) | CivilValue::Integer(c) => Quantity::Count(k, *c),
                        CivilValue::Real(_) | CivilValue::Text(_) | CivilValue::Identifier(_) | CivilValue::Bool(_) => continue,
                    });
                }
                self.w.quantities_of(product, &owner, &set.name, &items);
            }
            if let Some(m) = part.material.as_ref().map(|m| m.trim()).filter(|m| !m.is_empty()) {
                self.materials.entry(m.to_string()).or_default().push(product);
            }
            match part.group.and_then(|gi| group_ids.get(gi).copied().flatten().map(|id| (gi, id))) {
                Some((gi, _)) if part.entity == "IFCEARTHWORKSCUT" => features[gi].push(product),
                Some((gi, _)) => in_group[gi].push(product),
                None => roots.push(product),
            }
        }
        // Wire the groups: courses aggregate into their pavement, parts are contained in their road
        // part (which belongs to the road), parts are assigned to their system.
        let mut out = roots;
        // Road parts last: pavements and systems inside them have put their products in by then.
        let order: Vec<usize> = (0..package.groups.len()).filter(|i| package.groups[*i].kind != GroupKind::RoadPart).chain((0..package.groups.len()).filter(|i| package.groups[*i].kind == GroupKind::RoadPart)).collect();
        for gi in order {
            let grp = &package.groups[gi];
            let Some(id) = group_ids[gi] else { continue };
            let members = std::mem::take(&mut in_group[gi]);
            let owner = format!("element/{}/group/{gi}", e.id.0);
            match grp.kind {
                GroupKind::Pavement => {
                    self.aggregate(&owner, id, &members);
                    match grp.parent.filter(|p| group_ids.get(*p).copied().flatten().is_some()) {
                        Some(p) => in_group[p].push(id),
                        None => out.push(id),
                    }
                }
                GroupKind::RoadPart => {
                    let road = self.w.container;
                    let guid = step_string(&self.w.guid(&format!("{owner}/rel-road-part")));
                    self.w.data.add("IFCRELAGGREGATES", &format!("{guid},$,$,$,#{road},(#{id})"));
                    let cuts = std::mem::take(&mut features[gi]);
                    if !cuts.is_empty() {
                        let guid = step_string(&self.w.guid(&format!("{owner}/rel-features")));
                        self.w.data.add("IFCRELAGGREGATES", &format!("{guid},$,$,$,#{id},{}", refs(&cuts)));
                    }
                    if !members.is_empty() {
                        let guid = step_string(&self.w.guid(&format!("{owner}/rel-contained")));
                        self.w.data.add("IFCRELCONTAINEDINSPATIALSTRUCTURE", &format!("{guid},$,$,$,{},#{id}", refs(&members)));
                    }
                }
                GroupKind::System => {
                    if !members.is_empty() {
                        let guid = step_string(&self.w.guid(&format!("{owner}/rel-system")));
                        self.w.data.add("IFCRELASSIGNSTOGROUP", &format!("{guid},$,$,$,{},$,#{id}", refs(&members)));
                        // The parts of a system are contained in the road like any other element.
                        out.extend(members);
                    }
                }
            }
        }
        out
    }
}
