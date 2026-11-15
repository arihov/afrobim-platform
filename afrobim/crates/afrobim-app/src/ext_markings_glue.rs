//! The second set of road markings in the app: transverse lines (stop, give way, with the
//! triangle), arrows, hatching and chevrons, yellow lines, road studs and worded markings, with a
//! choice of basis (UK Traffic Signs Manual Chapter 5, 2018, or the Uganda Traffic Signs Manual,
//! 2004).
//!
//! The catalogue, the dimensions and the lay-out are `afrobim-markings`' (`ExtMarkingModule`);
//! nothing here adds a dimension. Where the Uganda manual does not define a type the engine says
//! "not defined in the Uganda manual" and nothing is drawn; the engineer may choose the UK value
//! deliberately, with a reason that is stored on the element.

use afrobim_kernel::{Document, ElementId, ModuleRegistry};
use afrobim_markings::{self as markings, Basis, ExtDefinition, ExtSpec, ExtTakeoff, Family, GeometryStatus, Kind, Lookup, Setting, StudColour, TypeRef};

pub use afrobim_markings::CATEGORY_EXT;

/// The words shown wherever a placeholder block is chosen, laid or listed.
pub const PLACEHOLDER_WORDS: &str = "placeholder block: not the true shape; area is the block, not paint";

pub fn register(modules: &mut ModuleRegistry) {
    modules.register(Box::new(markings::ExtMarkingModule));
}

/// A group of markings, one per command of the Roads tab.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Group {
    /// Stop lines, give way lines and the give way triangle, and pedestrian crossings.
    Transverse,
    Arrows,
    /// Hatched markings, chevrons, island markings and zig-zags.
    Hatching,
    /// Yellow lines: waiting restrictions and yellow dividing lines.
    Yellow,
    Studs,
    Worded,
    /// Lane lines, edge lines and double lines of the Uganda manual (the UK ones are in MARKINGS).
    Longitudinal,
}

impl Group {
    pub const ALL: [Group; 7] = [Group::Transverse, Group::Arrows, Group::Hatching, Group::Yellow, Group::Studs, Group::Worded, Group::Longitudinal];

    pub fn title(self) -> &'static str {
        match self {
            Group::Transverse => "Transverse lines",
            Group::Arrows => "Arrows",
            Group::Hatching => "Hatching and chevrons",
            Group::Yellow => "Yellow lines",
            Group::Studs => "Road studs",
            Group::Worded => "Worded markings",
            Group::Longitudinal => "Uganda lines",
        }
    }

    pub fn families(self) -> &'static [Family] {
        match self {
            Group::Transverse => &[Family::StopLine, Family::GiveWayLine, Family::GiveWayTriangle, Family::PedestrianCrossing],
            Group::Arrows => &[Family::LaneArrow, Family::DeflectionArrow],
            Group::Hatching => &[Family::HatchedMarking, Family::ChevronMarking, Family::IslandMarking, Family::ZigZagLine],
            Group::Yellow => &[Family::WaitingRestriction, Family::DividingLine],
            Group::Studs => &[Family::RoadStuds],
            Group::Worded => &[Family::WordedMarking],
            Group::Longitudinal => &[Family::LaneLine, Family::EdgeLine, Family::DoubleWhiteLine, Family::ParkingBay],
        }
    }
}

pub fn family_label(f: Family) -> &'static str {
    match f {
        Family::DividingLine => "Dividing line",
        Family::LaneLine => "Lane line",
        Family::WarningLine => "Warning line",
        Family::EdgeLine => "Edge line",
        Family::DoubleWhiteLine => "Double white line",
        Family::WaitingRestriction => "Waiting restriction",
        Family::ParkingBay => "Parking bay",
        Family::StopLine => "Stop line",
        Family::GiveWayLine => "Give way line",
        Family::GiveWayTriangle => "Give way triangle",
        Family::PedestrianCrossing => "Pedestrian crossing",
        Family::LaneArrow => "Lane arrow",
        Family::DeflectionArrow => "Deflection arrow",
        Family::HatchedMarking => "Hatched marking",
        Family::ChevronMarking => "Chevron marking",
        Family::IslandMarking => "Island marking",
        Family::ZigZagLine => "Zig-zag line",
        Family::WordedMarking => "Worded marking",
        Family::RoadStuds => "Road studs",
    }
}

/// How a catalogue entry is placed: along the road, across it, or at a point.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Placement {
    /// Station range and offset (lines, hatching, chevrons, zig-zags, studs).
    Linear,
    /// One station, from one offset to another (transverse lines, crossings).
    Across,
    /// A station, an offset and a rotation (arrows, worded markings, the triangle).
    Point,
}

pub fn placement_of(spec: &ExtSpec) -> Placement {
    match spec.kind {
        Kind::Lines(_) | Kind::Hatch(_) | Kind::ZigZag(_) => Placement::Linear,
        Kind::Transverse(_) | Kind::Crossing(_) => Placement::Across,
        _ => Placement::Point,
    }
}

/// True when the entry is drawn only as a block of its bounding size.
pub fn is_placeholder(spec: &ExtSpec) -> bool {
    matches!(spec.geometry, GeometryStatus::Placeholder(_))
}

/// What a family offers under a basis: the types, or the engine's "not defined" message.
pub fn types_of(basis: Basis, family: Family) -> Result<Vec<&'static ExtSpec>, String> {
    match markings::lookup_family(basis, family) {
        Lookup::Found(ids) => Ok(ids.iter().filter_map(|id| match markings::lookup_type(basis, id) {
            Lookup::Found(TypeRef::Ext(spec)) => Some(spec),
            _ => None,
        }).collect()),
        Lookup::NotDefined(nd) => Err(nd.message()),
    }
}

/// What the dialogs collect. Numbers the engine takes from a table stay `None` until the
/// engineer chooses; nothing here is a default value of a marking.
#[derive(Debug, Clone, PartialEq)]
pub struct ExtRequest {
    pub basis: Basis,
    /// The catalogue id, or `STUD`.
    pub type_id: String,
    /// The UK value is chosen under the Uganda basis; the reason is stored on the element.
    pub uk_fallback: Option<String>,
    pub station_m: f64,
    pub end_station_m: f64,
    /// Signed offsets, positive right looking up-station.
    pub offset_m: f64,
    pub to_offset_m: f64,
    pub rotation_deg: f64,
    /// The speed limit in the basis's own unit (UK mph, Uganda km/h) and the note on how it was arrived at.
    pub speed: Option<u32>,
    pub speed_note: String,
    pub setting: Option<Setting>,
    pub width_mm: Option<u32>,
    pub separation_mm: Option<u32>,
    pub size_mm: Option<u32>,
    pub strip_width_mm: Option<u32>,
    pub gap_mm: Option<u32>,
    pub count: Option<u32>,
    pub length_mm: Option<u32>,
    pub mirror: bool,
    pub material: String,
    /// Studs: spacing, colour and the table row they came from.
    pub stud_spacing_mm: Option<u32>,
    pub stud_colour: Option<StudColour>,
    pub stud_row: String,
}

impl ExtRequest {
    pub fn new(basis: Basis, type_id: &str, station_m: f64, end_station_m: f64) -> ExtRequest {
        ExtRequest {
            basis,
            type_id: type_id.to_string(),
            uk_fallback: None,
            station_m,
            end_station_m,
            offset_m: 0.0,
            to_offset_m: 0.0,
            rotation_deg: 0.0,
            speed: None,
            speed_note: String::new(),
            setting: None,
            width_mm: None,
            separation_mm: None,
            size_mm: None,
            strip_width_mm: None,
            gap_mm: None,
            count: None,
            length_mm: None,
            mirror: false,
            material: String::new(),
            stud_spacing_mm: None,
            stud_colour: None,
            stud_row: String::new(),
        }
    }

    /// The engine's definition for this request.
    pub fn definition(&self) -> Result<ExtDefinition, String> {
        let mut def = if self.type_id == markings::STUD_ID {
            ExtDefinition::studs(self.basis, self.station_m, self.end_station_m, self.offset_m, self.stud_spacing_mm.ok_or("Choose the stud spacing from a row of the table.")?, self.stud_colour.ok_or("Choose the stud colour from a row of the table.")?)
        } else {
            // The placement comes from the catalogue entry of whichever basis defines the type.
            let spec = match markings::lookup_type(self.basis, &self.type_id) {
                Lookup::Found(TypeRef::Ext(s)) => s,
                _ => match markings::lookup_type(Basis::Uk, &self.type_id) {
                    Lookup::Found(TypeRef::Ext(s)) if self.basis == Basis::Uganda => s,
                    _ => return Err(format!("'{}' is not a marking type of this set.", self.type_id)),
                },
            };
            match placement_of(spec) {
                Placement::Linear => ExtDefinition::linear(&self.type_id, self.basis, self.station_m, self.end_station_m, self.offset_m),
                Placement::Across => ExtDefinition::across(&self.type_id, self.basis, self.station_m, self.offset_m, self.to_offset_m),
                Placement::Point => ExtDefinition::point(&self.type_id, self.basis, self.station_m, self.offset_m, self.rotation_deg),
            }
        };
        if let Some(w) = self.width_mm {
            def = def.with_width(w);
        }
        if let Some(s) = self.separation_mm {
            def = def.with_separation(s);
        }
        if let Some(s) = self.size_mm {
            def = def.with_size(s);
        }
        if let Some(s) = self.strip_width_mm {
            def = def.with_strip_width(s);
        }
        if let Some(s) = self.setting {
            def = def.with_setting(s);
        }
        if let Some(speed) = self.speed {
            def = def.with_speed(speed, self.speed_note.trim());
        }
        def.gap_mm = self.gap_mm;
        def.count = self.count;
        def.length_mm = self.length_mm;
        def.mirror = self.mirror;
        def.stud_row = self.stud_row.clone();
        if let Some(reason) = &self.uk_fallback {
            if reason.trim().is_empty() {
                return Err("Using the UK value under the Uganda basis needs a reason. Write it in the box: it is stored with the marking.".into());
            }
            def = def.with_uk_fallback(reason.trim());
        }
        Ok(def.with_material(self.material.trim()))
    }
}

/// Adds a marking along an alignment. Returns its mark. The refusal is the engine's, in words
/// ("...: not defined in the Uganda manual" for a type the Uganda manual lacks).
pub fn add(doc: &mut Document, alignment: ElementId, req: &ExtRequest) -> Result<String, String> {
    let def = req.definition()?;
    let name = doc.unique_name("RX-001");
    let id = markings::add_ext_marking(doc, alignment, &name, &def).map_err(|e| e.to_string())?;
    let placeholder = markings::lookup_type(def.basis, &def.type_id).found().or_else(|| markings::lookup_type(Basis::Uk, &def.type_id).found()).is_some_and(|t| matches!(t, TypeRef::Ext(s) if is_placeholder(s)));
    if let Some(e) = doc.find_mut(id) {
        if placeholder {
            e.properties.insert("Shape".to_string(), PLACEHOLDER_WORDS.to_string());
        }
        if let Some(reason) = &def.uk_fallback {
            e.properties.insert("UK value used under the Uganda basis".to_string(), reason.clone());
        }
    }
    Ok(doc.find(id).map(|e| e.name.clone()).unwrap_or(name))
}

/// Adds the demo's second-set markings on the alignment, for `--demo`: a stop line, a lane arrow,
/// a hatched area and a run of studs. Returns what could not be added.
///
/// **Every number is an arbitrary demonstration value**: positions at fractions of the length of
/// the stretch the demo pavement covers, offsets of 3.5 m and 1.8 m, a hatched strip 3 m clear
/// width, 40 mph and 60 km/h limits. None is a design value.
pub fn demo(doc: &mut Document, alignment_id: ElementId, a: &afrobim_alignment::Alignment) -> Vec<String> {
    let (from, to) = crate::road_glue::DEMO_PAVEMENT_FRACTIONS;
    let at = |f: f64| a.start_station_m + a.length() * (from + (to - from) * f);
    let mut problems = Vec::new();
    // Demo: a stop line across the left lane at 20 % of the stretch (UK diagram 1002.1).
    let mut stop = ExtRequest::new(Basis::Uk, "1002.1", at(0.2), at(0.2));
    stop.offset_m = -3.5;
    // Demo: an Uganda lane arrow in the right lane at 35 % (a drawn outline, 60 km/h limit).
    let mut arrow = ExtRequest::new(Basis::Uganda, "M41", at(0.35), at(0.35));
    arrow.offset_m = 1.8;
    arrow.speed = Some(60);
    arrow.speed_note = "demo value".into();
    // Demo: a hatched area 3 m clear width, 100 mm boundaries, over 5 % of the stretch (UK diagram 1040, 40 mph).
    let mut hatch = ExtRequest::new(Basis::Uk, "1040", at(0.5), at(0.55));
    hatch.offset_m = 0.0;
    hatch.strip_width_mm = Some(3000);
    hatch.width_mm = Some(100);
    hatch.speed = Some(40);
    hatch.speed_note = "demo value".into();
    // Demo: studs along the centre line at the 9 m spacing of UK Table 2-1 (1008.1, 150 mm).
    let mut studs = ExtRequest::new(Basis::Uk, markings::STUD_ID, at(0.0), at(0.15));
    studs.stud_spacing_mm = Some(9000);
    studs.stud_colour = Some(StudColour::White);
    studs.stud_row = "UK-2-1-e".into();
    for r in [stop, arrow, hatch, studs] {
        if let Err(e) = add(doc, alignment_id, &r) {
            problems.push(format!("Demo marking {}: {e}", r.type_id));
        }
    }
    problems
}

/// Refreshes every second-set marking's copy of its alignment.
pub fn sync(doc: &mut Document) -> usize {
    markings::sync_ext_markings(doc)
}

/// The take-off of the second set.
pub fn takeoff(doc: &Document) -> ExtTakeoff {
    ExtTakeoff::measure(doc.elements())
}

/// The schedule CSV of the second set, or None when the document has none.
pub fn quantities(doc: &Document) -> Option<(String, usize)> {
    if !doc.elements().iter().any(|e| e.category == CATEGORY_EXT) {
        return None;
    }
    let t = takeoff(doc);
    Some((markings::ext_markings_to_csv(&t), t.rows.len()))
}

/// Quantity lines for the cost collector. `afrobim-cost` collects the first set only, so these
/// are added in the app's collection path: `marking_ext.length` m, `marking_ext.painted_length` m,
/// `marking_ext.painted_area` m2 (not for a placeholder block or studs), `marking_ext.number` nr
/// and, for a placeholder block, `marking_ext.block_area` m2 (the block, not paint).
pub fn collect_into(doc: &Document, out: &mut afrobim_cost::QuantitySet) {
    for e in doc.elements().iter().filter(|e| e.category == CATEGORY_EXT) {
        let t = ExtTakeoff::measure(std::slice::from_ref(e));
        for s in &t.skipped {
            out.skipped.push(afrobim_cost::SkippedSource { source: "marking_ext".to_string(), mark: s.mark.clone(), reason: s.reason.clone() });
        }
        for r in &t.rows {
            let attrs = [
                ("type", r.type_id.clone()),
                ("basis", r.basis.label().to_string()),
                ("diagram", r.diagram.clone()),
                ("colour", r.colour.clone()),
                ("material", r.material.clone()),
                ("alignment", r.alignment.clone()),
                ("uk_fallback", r.uk_fallback.clone()),
            ];
            let d = format!("{} ({}) {}", r.description, r.basis.label(), r.colour);
            if r.overall_length_m > 0.0 {
                out.push(Some(e.id), &r.mark, "marking_ext.length", &format!("{d}, overall length"), r.overall_length_m, "m", &attrs);
            }
            if r.painted_length_m > 0.0 {
                out.push(Some(e.id), &r.mark, "marking_ext.painted_length", &format!("{d}, painted length"), r.painted_length_m, "m", &attrs);
            }
            if let (Some(area), false) = (r.painted_area_m2, r.placeholder) {
                out.push(Some(e.id), &r.mark, "marking_ext.painted_area", &format!("{d}, painted area"), area, "m2", &attrs);
            }
            if let (Some(area), true) = (r.block_area_m2, r.placeholder) {
                out.push(Some(e.id), &r.mark, "marking_ext.block_area", &format!("{d}, block area ({PLACEHOLDER_WORDS})"), area, "m2", &attrs);
            }
            out.push_decimal(Some(e.id), &r.mark, "marking_ext.number", &format!("{d}, number"), afrobim_cost::Decimal::from_int(r.number as i64), "nr", &attrs);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use afrobim_alignment::{Alignment, Pi};

    fn doc_with_alignment() -> (Document, ElementId) {
        let mut doc = Document::default();
        let a = Alignment::from_pis("MC10", 0.0, &[Pi::point(0.0, 0.0), Pi::point(600.0, 0.0)]).unwrap();
        let id = crate::alignment_glue::add(&mut doc, &a).unwrap();
        (doc, id)
    }

    // Arbitrary test values throughout.
    fn requests() -> Vec<ExtRequest> {
        let mut stop = ExtRequest::new(Basis::Uk, "1002.1", 100.0, 100.0);
        stop.offset_m = -3.5;
        stop.to_offset_m = 0.0;
        let mut tri = ExtRequest::new(Basis::Uk, "1023A", 90.0, 90.0);
        tri.offset_m = -1.8;
        let mut arrow = ExtRequest::new(Basis::Uganda, "M41", 150.0, 150.0);
        arrow.offset_m = 1.8;
        arrow.speed = Some(80);
        let mut hatch = ExtRequest::new(Basis::Uk, "1040", 200.0, 260.0);
        hatch.strip_width_mm = Some(3000);
        hatch.width_mm = Some(100);
        hatch.speed = Some(40);
        hatch.speed_note = "test".into();
        let mut yellow = ExtRequest::new(Basis::Uk, "1017", 300.0, 360.0);
        yellow.offset_m = 4.0;
        yellow.width_mm = Some(100);
        let mut word = ExtRequest::new(Basis::Uk, "1024", 400.0, 400.0);
        word.offset_m = -1.8;
        word.speed = Some(30);
        let mut studs = ExtRequest::new(Basis::Uk, markings::STUD_ID, 0.0, 90.0);
        studs.stud_spacing_mm = Some(18_000);
        studs.stud_colour = Some(StudColour::White);
        studs.stud_row = "UK-2-2-b".into();
        vec![stop, tri, arrow, hatch, yellow, word, studs]
    }

    #[test]
    fn every_new_marking_type_is_made_through_the_glue_and_reaches_quantities_cost_and_the_3d_scene() {
        let (mut doc, a) = doc_with_alignment();
        let mut made = Vec::new();
        for r in requests() {
            let name = add(&mut doc, a, &r).unwrap_or_else(|e| panic!("{}: {e}", r.type_id));
            made.push(name);
        }
        assert_eq!(made.len(), 7);
        let (csv, rows) = quantities(&doc).unwrap();
        assert_eq!(rows, 7, "{csv}");
        for m in &made {
            assert!(csv.contains(m.as_str()));
        }
        // The cost collector sees them through the app's collection path, not the cost crate.
        let mut set = afrobim_cost::QuantitySet::new();
        collect_into(&doc, &mut set);
        assert!(set.lines.iter().any(|l| l.category == "marking_ext.painted_area"));
        assert!(set.lines.iter().any(|l| l.category == "marking_ext.number"));
        // A placeholder block has no painted area line: its block area is a separate, labelled line.
        let word_mark = &made[5];
        assert!(!set.lines.iter().any(|l| &l.mark == word_mark && l.category == "marking_ext.painted_area"));
        assert!(set.lines.iter().any(|l| &l.mark == word_mark && l.category == "marking_ext.block_area" && l.description.contains("not paint")));
        // The 3D site view and the clash scene take it: the category is a site solid and has solids.
        assert!(crate::view3d::is_site_solid(CATEGORY_EXT));
        let mut modules = ModuleRegistry::default();
        register(&mut modules);
        for e in doc.elements().iter().filter(|e| e.category == CATEGORY_EXT) {
            let g = modules.generate(e).unwrap();
            assert!(!g.solids.is_empty(), "{} has solids for the 3D view", e.name);
        }
        // The placeholder is labelled in Properties.
        let e = doc.elements().iter().find(|e| &e.name == word_mark).unwrap();
        assert_eq!(e.properties.get("Shape").map(String::as_str), Some(PLACEHOLDER_WORDS));
    }

    #[test]
    fn the_uganda_basis_refuses_a_type_it_does_not_define_until_the_uk_value_is_chosen_with_a_reason() {
        let (mut doc, a) = doc_with_alignment();
        // The Uganda manual defines no hatched marking: the engine says so.
        assert!(types_of(Basis::Uganda, Family::HatchedMarking).unwrap_err().contains("not defined in the Uganda manual"));
        let mut r = ExtRequest::new(Basis::Uganda, "1040", 200.0, 260.0);
        r.strip_width_mm = Some(3000);
        r.width_mm = Some(100);
        r.speed = Some(50);
        let why = add(&mut doc, a, &r).unwrap_err();
        assert!(why.contains("not defined in the Uganda manual"), "{why}");
        assert!(doc.elements().iter().all(|e| e.category != CATEGORY_EXT));
        // The UK value needs a reason, and the reason is stored.
        r.uk_fallback = Some(String::new());
        assert!(add(&mut doc, a, &r).unwrap_err().contains("needs a reason"));
        r.uk_fallback = Some("no Uganda equivalent; UK 1040 adopted for this junction".into());
        let mark = add(&mut doc, a, &r).unwrap();
        let e = doc.elements().iter().find(|e| e.name == mark).unwrap();
        assert_eq!(e.properties.get("UK value used under the Uganda basis").map(String::as_str), Some("no Uganda equivalent; UK 1040 adopted for this junction"));
        let stored = markings::ext_marking_of(&doc, e.id).unwrap();
        assert_eq!(stored.uk_fallback.as_deref(), Some("no Uganda equivalent; UK 1040 adopted for this junction"));
        assert!(quantities(&doc).unwrap().0.contains("no Uganda equivalent"));
    }
}
