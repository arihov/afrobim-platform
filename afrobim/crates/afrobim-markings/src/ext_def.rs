//! The element of the second set and of the Uganda manual: its definition, its checks against
//! the catalogue, and the laying out of its geometry.
//!
//! # Placement
//!
//! * **Linear items** (lines, hatching, chevrons, zig-zag, studs): `station_m` to
//!   `end_station_m` on an alignment, at `offset_m` (positive right looking up-station).
//! * **Items across the road** (stop and give way lines, crossings): at `station_m`, from
//!   `offset_m` to `across_to_offset_m`, the line running across the road between the two
//!   offsets (square to the alignment, turned by `rotation_deg` about the `offset_m` end).
//! * **Point items** (arrows, triangles, worded markings): the centre of the item's bounding
//!   block is at `station_m` and `offset_m`; the item points up-station at `rotation_deg` 0 and
//!   is turned anticlockwise by `rotation_deg` (180 points it down-station).
//!
//! Item frames: x across the item to the right of its direction, y along it. A point item's
//! polygons are mapped with one rigid transform at its centre, so on a bend the item is straight,
//! as a template laid on the road would be. Strip-like items (hatching, zig-zag) are mapped
//! vertex by vertex through the alignment.
//!
//! # Quantities
//!
//! Painted areas are those of the ideal shapes in the item frame (on a straight); the small
//! distortion of a hatch laid on a tight curve is not counted. A placeholder block has no
//! painted area, only the area of its block.

use afrobim_alignment::Alignment;
use afrobim_kernel::{Note, Severity};
use serde::{Deserialize, Serialize};

use crate::basis::Basis;
use crate::catalogue::{MarkingColour, Pattern};
use crate::definition::MarkingDefinition;
use crate::ext_catalogue::{ArrowDef, ArrowSize, CrossingDef, ExtSpec, GeometryStatus, HatchDef, HatchModule, Kind, LinesDef, Reference, StrandDef, TransverseDef, TriangleDef, WordDef, ZigZagDef};
use crate::ext_lookup::{lookup_type, StudColour, TypeRef, STUD_ID};
use crate::layout::{intervals, lay_out, stations_along, LayoutError, PatternAnchor, StrandSpec};

/// The element category of the extension markings. The app registers
/// [`crate::ExtMarkingModule`] for it next to [`crate::MarkingModule`].
pub const CATEGORY_EXT: &str = "marking_ext";

type P = (f64, f64);

/// Urban or rural, where the Uganda manual gives different marks (M03, M04, M12).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum Setting {
    Urban,
    Rural,
}

/// An extension marking: the `data` of an element of category [`CATEGORY_EXT`].
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ExtDefinition {
    /// A catalogue identifier ([`crate::ext_catalogue`]) or `"STUD"` for road studs.
    pub type_id: String,
    /// The standard the marking is taken from. Missing in a stored element means UK.
    #[serde(default)]
    pub basis: Basis,
    /// Set when a UK value is deliberately used under the Uganda basis: the caller's reason. It
    /// is recorded in the element and its output. Without it a UK type under the Uganda basis is
    /// an error ("not defined in the Uganda manual").
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub uk_fallback: Option<String>,
    #[serde(default)]
    pub alignment: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub geometry: Option<Alignment>,
    pub station_m: f64,
    /// Linear items: the end station.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub end_station_m: Option<f64>,
    #[serde(default)]
    pub offset_m: f64,
    /// Items across the road: the offset of the other end of the line.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub across_to_offset_m: Option<f64>,
    #[serde(default)]
    pub rotation_deg: f64,
    /// The speed limit (UK: mph; Uganda: km/h) where the entry depends on it. The caller maps a
    /// km/h limit to the mph value of a UK table and records why in `speed_note`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub speed: Option<u32>,
    /// The caller's note on the speed value used, recorded in the output.
    #[serde(default)]
    pub speed_note: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub setting: Option<Setting>,
    /// Width of the line, mm (each line of a double line; the stripe depth of M33).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub width_mm: Option<u32>,
    /// Double lines: the gap between the lines, mm.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub separation_mm: Option<u32>,
    /// A size alternative: the arrow length, the height of a worded marking, the amplitude of
    /// M05, mm.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub size_mm: Option<u32>,
    /// Hatching and chevrons: the clear width between the inner edges of the boundary lines, mm.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub strip_width_mm: Option<u32>,
    /// M33: the gap between stripes, mm.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub gap_mm: Option<u32>,
    /// M33: the number of stripes.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub count: Option<u32>,
    /// M34: the length of the crossing along the road, mm.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub length_mm: Option<u32>,
    /// Flips the lean of diagonals, turns chevrons to point up-station, mirrors a zig-zag, a
    /// tapered arrow or a triangle.
    #[serde(default)]
    pub mirror: bool,
    /// Road studs: the spacing along the line, mm (from [`crate::stud_rows`]).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub spacing_mm: Option<u32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub stud_colour: Option<StudColour>,
    /// The stud row the spacing was taken from, recorded in the output.
    #[serde(default)]
    pub stud_row: String,
    /// Overrides the catalogue colour (a Warning is raised).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub colour: Option<MarkingColour>,
    #[serde(default)]
    pub material: String,
    #[serde(default)]
    pub anchor: PatternAnchor,
}

impl ExtDefinition {
    fn blank(type_id: &str, basis: Basis, station_m: f64) -> Self {
        ExtDefinition {
            type_id: type_id.to_string(),
            basis,
            uk_fallback: None,
            alignment: String::new(),
            geometry: None,
            station_m,
            end_station_m: None,
            offset_m: 0.0,
            across_to_offset_m: None,
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
            spacing_mm: None,
            stud_colour: None,
            stud_row: String::new(),
            colour: None,
            material: String::new(),
            anchor: PatternAnchor::Start,
        }
    }

    /// A linear item between two stations at an offset (lines, hatching, chevrons, zig-zag,
    /// studs).
    pub fn linear(type_id: &str, basis: Basis, start_station_m: f64, end_station_m: f64, offset_m: f64) -> Self {
        ExtDefinition { end_station_m: Some(end_station_m), offset_m, ..Self::blank(type_id, basis, start_station_m) }
    }

    /// A line across the road at a station, from one offset to another (stop and give way lines,
    /// crossings).
    pub fn across(type_id: &str, basis: Basis, station_m: f64, from_offset_m: f64, to_offset_m: f64) -> Self {
        ExtDefinition { offset_m: from_offset_m, across_to_offset_m: Some(to_offset_m), ..Self::blank(type_id, basis, station_m) }
    }

    /// A point item (arrow, triangle, worded marking) centred at a station and offset, turned
    /// by `rotation_deg` from the up-station direction.
    pub fn point(type_id: &str, basis: Basis, station_m: f64, offset_m: f64, rotation_deg: f64) -> Self {
        ExtDefinition { offset_m, rotation_deg, ..Self::blank(type_id, basis, station_m) }
    }

    /// Road studs along a line.
    pub fn studs(basis: Basis, start_station_m: f64, end_station_m: f64, offset_m: f64, spacing_mm: u32, colour: StudColour) -> Self {
        ExtDefinition { spacing_mm: Some(spacing_mm), stud_colour: Some(colour), ..Self::linear(STUD_ID, basis, start_station_m, end_station_m, offset_m) }
    }

    pub fn with_width(mut self, width_mm: u32) -> Self {
        self.width_mm = Some(width_mm);
        self
    }

    pub fn with_separation(mut self, separation_mm: u32) -> Self {
        self.separation_mm = Some(separation_mm);
        self
    }

    pub fn with_size(mut self, size_mm: u32) -> Self {
        self.size_mm = Some(size_mm);
        self
    }

    pub fn with_strip_width(mut self, strip_width_mm: u32) -> Self {
        self.strip_width_mm = Some(strip_width_mm);
        self
    }

    pub fn with_setting(mut self, setting: Setting) -> Self {
        self.setting = Some(setting);
        self
    }

    /// The speed value used (mph for a UK table, km/h for a Uganda one) and the caller's note on
    /// how it was arrived at, e.g. `"100 km/h taken as the 60 mph band by project rule PR-3"`.
    pub fn with_speed(mut self, speed: u32, note: &str) -> Self {
        self.speed = Some(speed);
        self.speed_note = note.to_string();
        self
    }

    /// Uses a UK catalogue value under the Uganda basis, with the reason (recorded).
    pub fn with_uk_fallback(mut self, reason: &str) -> Self {
        self.uk_fallback = Some(reason.to_string());
        self
    }

    pub fn with_material(mut self, material: &str) -> Self {
        self.material = material.to_string();
        self
    }

    pub fn from_value(data: &serde_json::Value) -> Result<Self, crate::MarkingError> {
        serde_json::from_value(data.clone()).map_err(|e| crate::MarkingError::InvalidDefinition(format!("the marking definition cannot be read: {e}")))
    }

    pub fn to_value(&self) -> serde_json::Value {
        serde_json::to_value(self).expect("a marking definition always serialises")
    }
}

/// One drawn piece: a strip along a curve (given by its two edges) or a convex polygon. All in
/// plan metres.
#[derive(Debug, Clone, PartialEq)]
pub enum Shape {
    Strip { left: Vec<P>, right: Vec<P>, start_station_m: f64, end_station_m: f64 },
    Poly(Vec<P>),
}

/// A piece with the solid it belongs to.
#[derive(Debug, Clone, PartialEq)]
pub struct Piece {
    pub group: usize,
    pub shape: Shape,
}

/// An extension marking laid out.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct ExtLaid {
    /// Names of the solids (one per strand of a line, otherwise one).
    pub groups: Vec<String>,
    pub pieces: Vec<Piece>,
    /// Painted area, m2. `None` for a placeholder block and for studs.
    pub painted_area_m2: Option<f64>,
    /// Area of the block of a placeholder, m2.
    pub bounding_area_m2: Option<f64>,
    /// Sum of the lengths of the marks (bars, diagonals and arms along their own length), m.
    pub painted_length_m: f64,
    /// Length of the reference line, m (linear items); 0 for point items.
    pub overall_length_m: f64,
    /// Whole marks, bars, stripes, arrows, studs: one per item for a point item.
    pub number: usize,
    /// Marks cut by the end of the length.
    pub part_marks: usize,
    pub placeholder: bool,
}

/// What was resolved, for the derived values and the schedule.
#[derive(Debug, Clone, PartialEq)]
pub struct ExtResolved {
    pub id: String,
    pub name: String,
    pub diagram: String,
    pub tsrgd: String,
    pub colour: MarkingColour,
    pub stud_colour: Option<StudColour>,
    pub citation: String,
    /// Values for the element's derived table, in order.
    pub derived: Vec<(String, String)>,
    pub geometry: Option<GeometryStatus>,
}

fn note(severity: Severity, code: &str, message: String) -> Note {
    Note { severity, code: code.to_string(), message }
}

macro_rules! bail {
    ($notes:ident, $code:expr, $($arg:tt)*) => {{
        $notes.push(note(Severity::Error, $code, format!($($arg)*)));
        return None;
    }};
}

fn list(values: &[u32]) -> String {
    values.iter().map(u32::to_string).collect::<Vec<_>>().join(", ")
}

/// A choice among prescribed alternatives: the value given if prescribed, the only one if
/// there is one, otherwise an error (inputs stay inputs).
fn pick(given: Option<u32>, allowed: &[u32], what: &str, cite: &str, notes: &mut Vec<Note>) -> Option<u32> {
    match given {
        Some(v) if allowed.contains(&v) => Some(v),
        Some(v) => bail!(notes, "marking.dimension.not_prescribed", "{v} mm is not a {what} prescribed here; the alternatives are {} mm ({cite}).", list(allowed)),
        None if allowed.len() == 1 => Some(allowed[0]),
        None => bail!(notes, "marking.dimension.required", "No {what} is set; choose one of {} mm ({cite}).", list(allowed)),
    }
}

/// A rigid frame at a plan point: x to the right of the item's direction, y along it.
struct Frame {
    x0: f64,
    y0: f64,
    theta: f64,
}

impl Frame {
    fn at(a: &Alignment, station: f64, offset: f64, rotation_deg: f64, notes: &mut Vec<Note>) -> Option<Frame> {
        let Some(p) = a.point_at(station) else {
            bail!(notes, "marking.stations.outside", "Station {station:.3} m is outside the alignment, which runs from {:.3} m to {:.3} m.", a.start_station_m, a.end_station_m());
        };
        let (x0, y0) = p.offset(offset);
        Some(Frame { x0, y0, theta: p.heading_rad + rotation_deg.to_radians() })
    }

    fn world(&self, x: f64, y: f64) -> P {
        (self.x0 + y * self.theta.cos() + x * self.theta.sin(), self.y0 + y * self.theta.sin() - x * self.theta.cos())
    }

    fn poly(&self, local: &[P]) -> Vec<P> {
        local.iter().map(|p| self.world(p.0, p.1)).collect()
    }
}

/// Area of a simple polygon, m2 (shoelace), unsigned.
pub fn polygon_area(poly: &[P]) -> f64 {
    let n = poly.len();
    (0..n).map(|i| poly[i].0 * poly[(i + 1) % n].1 - poly[(i + 1) % n].0 * poly[i].1).sum::<f64>().abs() / 2.0
}

fn rect(x0: f64, x1: f64, y0: f64, y1: f64) -> Vec<P> {
    vec![(x0, y0), (x1, y0), (x1, y1), (x0, y1)]
}

fn layout_error(e: &LayoutError) -> (&'static str, String) {
    let code = match e {
        LayoutError::StationOutside { .. } => "marking.stations.outside",
        LayoutError::StationOrder { .. } => "marking.stations.order",
        LayoutError::InsideCurve { .. } => "marking.offset.inside_curve",
        LayoutError::EmptyAlignment => "marking.alignment.empty",
        LayoutError::BadWidth => "marking.width.invalid",
    };
    let mut message = e.to_string();
    message[..1].make_ascii_uppercase();
    (code, format!("{message}."))
}

impl ExtDefinition {
    /// Resolves the definition against the catalogue and lays it out.
    ///
    /// Returns what was resolved and laid (`None` when an Error note prevents it) and every
    /// note. Errors: unknown type; a type the basis does not define ("not defined in the Uganda
    /// manual") unless `uk_fallback` records the caller's choice; a dimension not prescribed or
    /// not chosen; stations outside the alignment. Warnings: a colour other than the catalogue's.
    /// Info: a UK value used by choice under the Uganda basis; placeholders.
    pub fn lay_out(&self) -> (Option<(ExtResolved, ExtLaid)>, Vec<Note>) {
        let mut notes = Vec::new();
        let out = self.lay_out_inner(&mut notes);
        (out, notes)
    }

    fn lay_out_inner(&self, notes: &mut Vec<Note>) -> Option<(ExtResolved, ExtLaid)> {
        let found = lookup_type(self.basis, &self.type_id);
        let mut fallback_note = None;
        let target = match found {
            crate::basis::Lookup::Found(t) => t,
            crate::basis::Lookup::NotDefined(nd) => {
                // The type exists in the other basis: only an explicit choice uses it.
                let other = match lookup_type(if self.basis == Basis::Uk { Basis::Uganda } else { Basis::Uk }, &self.type_id) {
                    crate::basis::Lookup::Found(t) => Some(t),
                    crate::basis::Lookup::NotDefined(_) => None,
                };
                match (other, &self.uk_fallback) {
                    (Some(t), Some(reason)) if self.basis == Basis::Uganda => {
                        fallback_note = Some(reason.clone());
                        t
                    }
                    (Some(_), _) if self.basis == Basis::Uganda => bail!(
                        notes,
                        "marking.basis.not_defined",
                        "{}. Choose a type the Uganda manual defines, or record the UK value as a deliberate choice (uk_fallback).",
                        nd.message()
                    ),
                    (Some(_), _) => bail!(notes, "marking.basis.not_defined", "{}. The marking is a Uganda manual sign; set the basis to Uganda.", nd.message()),
                    (None, _) => bail!(notes, "marking.type.unknown", "{}.", nd.message()),
                }
            }
        };
        let Some(alignment) = &self.geometry else {
            bail!(
                notes,
                "marking.alignment.missing",
                "{}",
                if self.alignment.is_empty() { "The marking is not tied to an alignment.".to_string() } else { format!("The marking has no geometry for alignment '{}'; synchronise it with the alignment.", self.alignment) }
            );
        };
        if let Some(reason) = &fallback_note {
            notes.push(note(Severity::Info, "marking.basis.uk_fallback", format!("UK value used under the Uganda basis by the caller's choice: {reason}")));
        }
        let mut derived: Vec<(String, String)> = vec![("Basis".into(), self.basis.label().into())];
        if let Some(reason) = &fallback_note {
            derived.push(("UK fallback (caller's choice)".into(), reason.clone()));
        }
        if let Some(speed) = self.speed {
            derived.push(("Speed used".into(), format!("{speed} {}", self.basis.speed_unit())));
            if !self.speed_note.is_empty() {
                derived.push(("Speed note (caller)".into(), self.speed_note.clone()));
            }
        }
        match target {
            TypeRef::Studs => self.lay_studs(alignment, notes, derived),
            TypeRef::Legacy(t) => {
                // A legacy longitudinal line asked for through this element (UK basis only).
                let mut def = MarkingDefinition::new(t, self.station_m, self.end_station_m.unwrap_or(self.station_m), self.offset_m, self.speed.unwrap_or(0));
                def.width_mm = self.width_mm;
                def.geometry = Some(alignment.clone());
                bail!(notes, "marking.type.legacy", "'{}' is a longitudinal line of the first catalogue: use category 'marking' (MarkingDefinition).", def.line_type);
            }
            TypeRef::Ext(spec) => self.ext(spec, alignment, notes, derived),
        }
    }

    fn colour_for(&self, spec: &ExtSpec, notes: &mut Vec<Note>) -> MarkingColour {
        match self.colour {
            Some(c) if c != spec.colour => {
                notes.push(note(Severity::Warning, "marking.colour", format!("{} is {} in {}; {} is set.", spec.name, spec.colour.label(), spec.citation, c.label())));
                c
            }
            _ => spec.colour,
        }
    }

    fn finish(&self, spec: &ExtSpec, colour: MarkingColour, derived: Vec<(String, String)>, laid: ExtLaid) -> (ExtResolved, ExtLaid) {
        let mut d = derived;
        d.push(("Source".into(), format!("{}; {}", spec.citation, self.basis.edition())));
        if !spec.notes.is_empty() {
            d.push(("Reading notes".into(), spec.notes.to_string()));
        }
        if let GeometryStatus::Placeholder(why) = spec.geometry {
            d.push(("Shape".into(), format!("placeholder block: {why}")));
        }
        (
            ExtResolved {
                id: spec.id.to_string(),
                name: spec.name.to_string(),
                diagram: spec.diagram.to_string(),
                tsrgd: spec.tsrgd.to_string(),
                colour,
                stud_colour: None,
                citation: spec.citation.to_string(),
                derived: d,
                geometry: Some(spec.geometry),
            },
            laid,
        )
    }

    fn ext(&self, spec: &'static ExtSpec, a: &Alignment, notes: &mut Vec<Note>, mut derived: Vec<(String, String)>) -> Option<(ExtResolved, ExtLaid)> {
        if let GeometryStatus::CatalogueOnly(why) = spec.geometry {
            bail!(notes, "marking.geometry.catalogue_only", "{} ({}) is catalogued only and cannot be laid out: {why}.", spec.name, spec.diagram);
        }
        let colour = self.colour_for(spec, notes);
        derived.push(("Marking".into(), format!("{}, {} {}", spec.name, if spec.basis == Basis::Uk { "diagram" } else { "sign" }, spec.diagram)));
        if !spec.tsrgd.is_empty() {
            derived.push(("TSRGD reference".into(), spec.tsrgd.to_string()));
        }
        let laid = match spec.kind {
            Kind::Lines(l) => self.lines(spec, &l, a, notes, &mut derived)?,
            Kind::Transverse(t) => self.transverse(spec, &t, a, notes, &mut derived)?,
            Kind::Triangle(t) => self.triangle(&t, a, notes, &mut derived)?,
            Kind::Arrow(ar) => self.arrow(spec, &ar, a, notes, &mut derived)?,
            Kind::Word(w) => self.word(spec, &w, a, notes, &mut derived)?,
            Kind::Hatch(h) => self.hatch(spec, &h, a, notes, &mut derived)?,
            Kind::ZigZag(z) => self.zigzag(&z, a, notes, &mut derived)?,
            Kind::Crossing(c) => self.crossing(spec, &c, a, notes, &mut derived)?,
            Kind::None => bail!(notes, "marking.geometry.catalogue_only", "{} cannot be laid out.", spec.name),
        };
        Some(self.finish(spec, colour, derived, laid))
    }

    fn end_station(&self, notes: &mut Vec<Note>) -> Option<f64> {
        match self.end_station_m {
            Some(e) => Some(e),
            None => bail!(notes, "marking.stations.order", "An end station is needed for a linear marking."),
        }
    }

    // ------------------------------------------------------------------------------ lines
    fn lines(&self, spec: &ExtSpec, l: &LinesDef, a: &Alignment, notes: &mut Vec<Note>, derived: &mut Vec<(String, String)>) -> Option<ExtLaid> {
        let end = self.end_station(notes)?;
        let w = pick(self.width_mm, l.widths_mm, "line width", spec.citation, notes)?;
        let double = l.strands.len() == 2;
        let sep = if double { Some(pick(self.separation_mm, l.separations_mm, "gap between the lines", spec.citation, notes)?) } else { None };
        let urban = match (l.by_setting, self.setting) {
            (true, None) => bail!(notes, "marking.setting.required", "{} has different marks in an urban and a rural setting ({}); set the setting.", spec.diagram, spec.citation),
            (true, Some(s)) => s == Setting::Urban,
            (false, _) => true,
        };
        let pattern_of = |s: &StrandDef| if urban { s.urban } else { s.rural };
        let wm = w as f64 / 1000.0;
        let (laterals, names): (Vec<f64>, Vec<String>) = match l.reference {
            Reference::Centre if !double => (vec![self.offset_m], vec!["line".into()]),
            Reference::Centre => {
                let shift = (sep.unwrap_or(0) as f64 + w as f64) / 2000.0;
                (vec![self.offset_m - shift, self.offset_m + shift], vec!["left line".into(), "right line".into()])
            }
            Reference::Edge { clear_mm } => {
                if self.offset_m == 0.0 {
                    bail!(notes, "marking.offset.required", "The offset of {} is the edge of carriageway or kerb; it cannot be 0 (the line lies inward of it).", spec.diagram);
                }
                let inward = if self.offset_m > 0.0 { -1.0 } else { 1.0 };
                let first = self.offset_m + inward * (clear_mm as f64 + w as f64 / 2.0) / 1000.0;
                if double {
                    let second = self.offset_m + inward * (clear_mm as f64 + w as f64 + sep.unwrap_or(0) as f64 + w as f64 / 2.0) / 1000.0;
                    (vec![first, second], vec!["near line".into(), "far line".into()])
                } else {
                    (vec![first], vec!["line".into()])
                }
            }
        };
        let strands: Vec<StrandSpec> = l
            .strands
            .iter()
            .zip(&laterals)
            .zip(&names)
            .map(|((s, lat), name)| StrandSpec { name: name.clone(), lateral_m: *lat, width_m: wm, pattern: pattern_of(s) })
            .collect();
        let reference = laterals.iter().sum::<f64>() / laterals.len() as f64;
        let layout = match lay_out(a, self.station_m, end, reference, &strands, self.anchor) {
            Ok(l) => l,
            Err(e) => {
                let (code, message) = layout_error(&e);
                bail!(notes, code, "{message}");
            }
        };
        derived.push(("Width".into(), format!("{w} mm{}", if double { " each line" } else { "" })));
        if let Some(s) = sep {
            derived.push(("Line separation".into(), format!("{s} mm")));
        }
        derived.push(("Pattern".into(), strands.iter().map(|s| format!("{}: {}", s.name, s.pattern.label())).collect::<Vec<_>>().join("; ")));
        for s in &layout.strands {
            if let (Pattern::Broken { .. }, Some(part)) = (s.pattern, s.marks.iter().find(|m| !m.whole)) {
                notes.push(note(Severity::Info, "marking.part_mark", format!("The {} ends in a part mark {:.3} m long.", s.name, part.length_m())));
            }
        }
        let mut laid = ExtLaid {
            groups: names,
            painted_area_m2: Some(layout.painted_area_m2()),
            painted_length_m: layout.painted_length_m(),
            overall_length_m: layout.overall_length_m,
            number: layout.whole_marks(),
            part_marks: layout.part_marks(),
            ..Default::default()
        };
        for (g, strand) in layout.strands.iter().enumerate() {
            for m in &strand.marks {
                laid.pieces.push(Piece { group: g, shape: Shape::Strip { left: m.left.clone(), right: m.right.clone(), start_station_m: m.start_station_m, end_station_m: m.end_station_m } });
            }
        }
        Some(laid)
    }

    // ------------------------------------------------------------------------- transverse
    fn across_span(&self, notes: &mut Vec<Note>) -> Option<f64> {
        let Some(to) = self.across_to_offset_m else {
            bail!(notes, "marking.across.required", "The offset of the far end of the line (across_to_offset_m) is needed.");
        };
        let span = to - self.offset_m;
        if span.abs() < 1e-6 {
            bail!(notes, "marking.across.zero", "The two ends of the line are at the same offset.");
        }
        Some(span)
    }

    fn transverse(&self, spec: &ExtSpec, t: &TransverseDef, a: &Alignment, notes: &mut Vec<Note>, derived: &mut Vec<(String, String)>) -> Option<ExtLaid> {
        let w = pick(self.width_mm, t.widths_mm, "line width", spec.citation, notes)?;
        let span = self.across_span(notes)?;
        let frame = Frame::at(a, self.station_m, self.offset_m, self.rotation_deg, notes)?;
        let sign = span.signum();
        let wm = w as f64 / 1000.0;
        let gap = t.row_gap_mm as f64 / 1000.0;
        let depth = t.rows as f64 * wm + (t.rows as f64 - 1.0) * gap;
        let marks = intervals(t.pattern, span.abs(), PatternAnchor::Start);
        let mut laid = ExtLaid { groups: vec![spec.name.to_string()], overall_length_m: span.abs(), ..Default::default() };
        let mut area = 0.0;
        for r in 0..t.rows {
            let y0 = -depth / 2.0 + r as f64 * (wm + gap);
            for (from, to, whole) in &marks {
                let local = rect(sign * from, sign * to, y0, y0 + wm);
                area += (to - from) * wm;
                laid.painted_length_m += to - from;
                if *whole {
                    laid.number += 1;
                } else {
                    laid.part_marks += 1;
                }
                laid.pieces.push(Piece { group: 0, shape: Shape::Poly(frame.poly(&local)) });
            }
        }
        laid.painted_area_m2 = Some(area);
        derived.push(("Width".into(), format!("{w} mm")));
        derived.push(("Pattern".into(), format!("{}{}", t.pattern.label(), if t.rows > 1 { format!(", {} rows {} mm apart", t.rows, t.row_gap_mm) } else { String::new() })));
        derived.push(("Length across the road".into(), format!("{:.3} m", span.abs())));
        if marks.iter().any(|m| !m.2) {
            notes.push(note(Severity::Info, "marking.part_mark", "The line ends in a part mark across the road.".to_string()));
        }
        Some(laid)
    }

    // --------------------------------------------------------------------------- triangle
    fn triangle(&self, t: &TriangleDef, a: &Alignment, notes: &mut Vec<Note>, derived: &mut Vec<(String, String)>) -> Option<ExtLaid> {
        let frame = Frame::at(a, self.station_m, self.offset_m, self.rotation_deg, notes)?;
        let parts = triangle_parts(t);
        let sx = if self.mirror { -1.0 } else { 1.0 };
        let mut laid = ExtLaid { groups: vec!["Triangle".into()], number: 1, ..Default::default() };
        let mut area = 0.0;
        for p in &parts {
            area += polygon_area(p);
            let local: Vec<P> = p.iter().map(|q| (q.0 * sx, q.1)).collect();
            laid.pieces.push(Piece { group: 0, shape: Shape::Poly(frame.poly(&local)) });
        }
        laid.painted_area_m2 = Some(area);
        laid.bounding_area_m2 = Some(t.base_mm as f64 * t.height_mm as f64 / 1e6);
        laid.painted_length_m = 0.0;
        derived.push(("Size".into(), format!("base {} mm, height {} mm, side border {} mm, base border {} mm", t.base_mm, t.height_mm, t.side_border_mm, t.base_border_mm)));
        Some(laid)
    }

    // ------------------------------------------------------------------------ arrows, words
    fn block(&self, name: &str, length_mm: u32, width_mm: u32, a: &Alignment, notes: &mut Vec<Note>) -> Option<ExtLaid> {
        let frame = Frame::at(a, self.station_m, self.offset_m, self.rotation_deg, notes)?;
        let (hw, hl) = (width_mm as f64 / 2000.0, length_mm as f64 / 2000.0);
        let poly = frame.poly(&rect(-hw, hw, -hl, hl));
        Some(ExtLaid {
            groups: vec![name.to_string()],
            pieces: vec![Piece { group: 0, shape: Shape::Poly(poly) }],
            painted_area_m2: None,
            bounding_area_m2: Some(length_mm as f64 * width_mm as f64 / 1e6),
            number: 1,
            placeholder: true,
            ..Default::default()
        })
    }

    fn arrow(&self, spec: &ExtSpec, ar: &ArrowDef, a: &Alignment, notes: &mut Vec<Note>, derived: &mut Vec<(String, String)>) -> Option<ExtLaid> {
        let size = pick_arrow_size(self.size_mm, self.speed, ar.sizes, spec, notes)?;
        derived.push(("Length".into(), format!("{} mm", size.length_mm)));
        for (name, v) in size.aux {
            derived.push((format!("Dimension: {name}"), format!("{v} mm")));
        }
        if let Some(t) = ar.tapered {
            let frame = Frame::at(a, self.station_m, self.offset_m, self.rotation_deg, notes)?;
            let parts = tapered_arrow_parts(&t, size.length_mm);
            let sx = if self.mirror { -1.0 } else { 1.0 };
            let mut laid = ExtLaid { groups: vec![spec.name.to_string()], number: 1, ..Default::default() };
            let mut area = 0.0;
            for p in &parts {
                area += polygon_area(p);
                let local: Vec<P> = p.iter().map(|q| (q.0 * sx, q.1)).collect();
                laid.pieces.push(Piece { group: 0, shape: Shape::Poly(frame.poly(&local)) });
            }
            laid.painted_area_m2 = Some(area);
            laid.bounding_area_m2 = Some(size.length_mm as f64 * t.head_width_mm as f64 / 1e6);
            laid.painted_length_m = size.length_mm as f64 / 1000.0;
            return Some(laid);
        }
        let width = match (size.width_mm, self.width_mm) {
            (Some(w), _) => w,
            (None, Some(w)) => w,
            (None, None) => bail!(notes, "marking.dimension.required", "The overall width of {} cannot be read from its figure; set width_mm ({}).", spec.diagram, spec.citation),
        };
        derived.push(("Block width".into(), format!("{width} mm")));
        notes.push(note(Severity::Info, "marking.placeholder", format!("{} is drawn as a bounding block {} mm by {} mm; its outline is not dimensioned in the sources used. No painted area is reported.", spec.name, size.length_mm, width)));
        self.block(spec.name, size.length_mm, width, a, notes)
    }

    fn word(&self, spec: &ExtSpec, w: &WordDef, a: &Alignment, notes: &mut Vec<Note>, derived: &mut Vec<(String, String)>) -> Option<ExtLaid> {
        let candidates: Vec<u32> = match (self.size_mm, self.speed) {
            (Some(s), _) => {
                if !w.heights.iter().any(|(_, h)| *h == s) {
                    bail!(notes, "marking.dimension.not_prescribed", "{s} mm is not a size of {}: the sizes are {} mm ({}).", spec.diagram, list(&w.heights.iter().map(|h| h.1).collect::<Vec<_>>()), spec.citation);
                }
                vec![s]
            }
            (None, Some(v)) => w.heights.iter().filter(|(sel, _)| sel.contains(v)).map(|(_, h)| *h).collect(),
            (None, None) => w.heights.iter().map(|(_, h)| *h).collect(),
        };
        let height = match candidates.as_slice() {
            [one] => *one,
            _ => bail!(notes, "marking.dimension.required", "Choose the size of {} (size_mm: {} mm); the speed alone does not decide it ({}).", spec.diagram, list(&w.heights.iter().map(|h| h.1).collect::<Vec<_>>()), spec.citation),
        };
        derived.push(("Legend".into(), w.text.to_string()));
        derived.push(("Length along the road".into(), format!("{height} mm")));
        derived.push(("Width across the road".into(), format!("{} mm", w.width_mm)));
        notes.push(note(Severity::Info, "marking.placeholder", format!("The word {} is drawn as a bounding block {height} mm by {} mm; letter shapes are not drawn. No painted area is reported.", w.text, w.width_mm)));
        self.block(spec.name, height, w.width_mm, a, notes)
    }

    // ------------------------------------------------------------------------- hatching
    fn hatch(&self, spec: &ExtSpec, h: &HatchDef, a: &Alignment, notes: &mut Vec<Note>, derived: &mut Vec<(String, String)>) -> Option<ExtLaid> {
        let end = self.end_station(notes)?;
        let strip = match self.strip_width_mm {
            Some(s) => s,
            None => bail!(notes, "marking.strip.required", "The clear width between the boundary lines (strip_width_mm) is needed ({}).", spec.citation),
        };
        if h.strip_min_mm.is_some_and(|m| strip < m) || h.strip_max_mm.is_some_and(|m| strip > m) {
            bail!(
                notes,
                "marking.strip.range",
                "{strip} mm between the boundary lines is outside the range of {} ({} to {} mm).",
                spec.citation,
                h.strip_min_mm.map_or("no minimum".to_string(), |m| m.to_string()),
                h.strip_max_mm.map_or("no maximum".to_string(), |m| m.to_string())
            );
        }
        let bw = pick(self.width_mm, h.boundary_widths_mm, "boundary line width", spec.citation, notes)?;
        let module: HatchModule = match (h.short, h.split) {
            (Some(short), Some(split)) => match self.speed {
                Some(v) if v <= split => short,
                Some(_) => h.long,
                None => bail!(notes, "marking.speed.missing", "The speed limit is needed: {} has a shorter module at {split} {} or less ({}).", spec.diagram, self.basis.speed_unit(), spec.citation),
            },
            _ => h.long,
        };
        let (sm, bwm) = (strip as f64 / 1000.0, bw as f64 / 1000.0);
        let pattern = if h.boundary_continuous { Pattern::Continuous } else { Pattern::Broken { mark_mm: module.boundary_mark_mm, gap_mm: module.boundary_gap_mm } };
        let off = sm / 2.0 + bwm / 2.0;
        let strands = vec![
            StrandSpec { name: "left boundary".into(), lateral_m: self.offset_m - off, width_m: bwm, pattern },
            StrandSpec { name: "right boundary".into(), lateral_m: self.offset_m + off, width_m: bwm, pattern },
        ];
        let layout = match lay_out(a, self.station_m, end, self.offset_m, &strands, self.anchor) {
            Ok(l) => l,
            Err(e) => {
                let (code, message) = layout_error(&e);
                bail!(notes, code, "{message}");
            }
        };
        let mut laid = ExtLaid {
            groups: vec!["left boundary".into(), "right boundary".into(), "marks".into()],
            painted_area_m2: Some(layout.painted_area_m2()),
            painted_length_m: layout.painted_length_m(),
            overall_length_m: layout.overall_length_m,
            number: 0,
            part_marks: layout.part_marks(),
            ..Default::default()
        };
        for (g, strand) in layout.strands.iter().enumerate() {
            for m in &strand.marks {
                laid.pieces.push(Piece { group: g, shape: Shape::Strip { left: m.left.clone(), right: m.right.clone(), start_station_m: m.start_station_m, end_station_m: m.end_station_m } });
            }
        }
        // Diagonals or chevrons, positioned by station along the alignment.
        let total = end - self.station_m;
        let (dw, sp) = (module.mark_width_mm as f64 / 1000.0, module.spacing_mm as f64 / 1000.0);
        let mark_w = if h.chevrons && h.split.is_none() { self.size_mm.map_or(dw, |s| s as f64 / 1000.0) } else { dw };
        let v = mark_w / std::f64::consts::SQRT_2;
        let mut count = 0usize;
        let mut area = 0.0;
        let mut length = 0.0;
        let mut k = 0usize;
        loop {
            let base = k as f64 * sp;
            let polys: Vec<Vec<P>> = if h.chevrons {
                let ext = sm / 2.0 + 2.0 * v;
                if base + ext > total + 1e-9 {
                    break;
                }
                let arm = |s: f64| -> Vec<P> {
                    let local = [(0.0, 0.0), (0.0, 2.0 * v), (sm / 2.0, sm / 2.0 + 2.0 * v), (sm / 2.0, sm / 2.0)];
                    local.iter().map(|p| (p.0 * s, if self.mirror { ext - p.1 } else { p.1 } + base)).collect()
                };
                length += sm * std::f64::consts::SQRT_2;
                vec![arm(1.0), arm(-1.0)]
            } else {
                let ext = sm + 2.0 * v;
                if base + ext > total + 1e-9 {
                    break;
                }
                let s = if self.mirror { -1.0 } else { 1.0 };
                let ya = base + v;
                length += sm * std::f64::consts::SQRT_2;
                vec![[(sm / 2.0, ya - v), (sm / 2.0, ya + v), (-sm / 2.0, ya + sm + v), (-sm / 2.0, ya + sm - v)].iter().map(|p| (p.0 * s, p.1)).collect()]
            };
            for p in polys {
                area += polygon_area(&p);
                let mut world = Vec::with_capacity(p.len());
                for q in &p {
                    let Some(pt) = a.point_at((self.station_m + q.1).min(a.end_station_m())) else {
                        bail!(notes, "marking.stations.outside", "A mark lies outside the alignment.");
                    };
                    world.push(pt.offset(self.offset_m + q.0));
                }
                laid.pieces.push(Piece { group: 2, shape: Shape::Poly(world) });
            }
            count += 1;
            k += 1;
        }
        laid.painted_area_m2 = laid.painted_area_m2.map(|b| b + area);
        laid.painted_length_m += length;
        laid.number = layout.whole_marks() + count;
        derived.push(("Boundary lines".into(), format!("{bw} mm wide, {}", pattern.label())));
        derived.push(("Width between boundary lines".into(), format!("{strip} mm (clear, inner edges)")));
        derived.push((if h.chevrons { "Chevrons" } else { "Diagonals" }.into(), format!("{count} at {} mm spacing, {} mm wide", module.spacing_mm, (mark_w * 1000.0).round())));
        notes.push(note(Severity::Info, "marking.hatch.start", "The first diagonal or chevron starts at the start station; the manual does not say where the first one starts. Only whole diagonals fit in the length are drawn.".to_string()));
        Some(laid)
    }

    // -------------------------------------------------------------------------- zig-zag
    fn zigzag(&self, z: &ZigZagDef, a: &Alignment, notes: &mut Vec<Note>, derived: &mut Vec<(String, String)>) -> Option<ExtLaid> {
        let end = self.end_station(notes)?;
        let amp = pick(self.size_mm.or(Some(z.amplitudes_mm[0])), z.amplitudes_mm, "lateral extent", "Uganda TSM Vol.2 (2004) App.3 sign page M05 (PDF p.165)", notes)?;
        if end - self.station_m <= 0.0 {
            bail!(notes, "marking.stations.order", "The end station must be after the start station.");
        }
        let (w, e) = (z.width_mm as f64 / 1000.0, (amp as f64 - z.width_mm as f64) / 1000.0);
        let (run, gap) = (z.mark_mm as f64 / 1000.0, z.gap_mm as f64 / 1000.0);
        let total = end - self.station_m;
        let t = w * (run * run + e * e).sqrt() / run;
        let mut laid = ExtLaid { groups: vec!["Zig-zag".into()], overall_length_m: total, ..Default::default() };
        let mut k = 0usize;
        let mut area = 0.0;
        let mut length = 0.0;
        loop {
            let y0 = k as f64 * (run + gap);
            if y0 + run > total + 1e-9 {
                break;
            }
            let dir = if (k % 2 == 0) != self.mirror { 1.0 } else { -1.0 };
            let (xa, xb) = (-dir * e / 2.0, dir * e / 2.0);
            let local = [(xa - t / 2.0, y0), (xa + t / 2.0, y0), (xb + t / 2.0, y0 + run), (xb - t / 2.0, y0 + run)];
            area += polygon_area(&local);
            length += (run * run + e * e).sqrt();
            let mut world = Vec::new();
            for q in &local {
                let Some(pt) = a.point_at((self.station_m + q.1).min(a.end_station_m())) else {
                    bail!(notes, "marking.stations.outside", "A mark lies outside the alignment.");
                };
                world.push(pt.offset(self.offset_m + q.0));
            }
            laid.pieces.push(Piece { group: 0, shape: Shape::Poly(world) });
            k += 1;
        }
        if k == 0 {
            bail!(notes, "marking.stations.order", "The length is shorter than one mark ({} mm).", z.mark_mm);
        }
        laid.painted_area_m2 = Some(area);
        laid.painted_length_m = length;
        laid.number = k;
        derived.push(("Width".into(), format!("{} mm", z.width_mm)));
        derived.push(("Pattern".into(), format!("{} mm marks along the road, {} mm gaps, lateral extent {amp} mm", z.mark_mm, z.gap_mm)));
        Some(laid)
    }

    // ------------------------------------------------------------------------- crossings
    fn crossing(&self, spec: &ExtSpec, c: &CrossingDef, a: &Alignment, notes: &mut Vec<Note>, derived: &mut Vec<(String, String)>) -> Option<ExtLaid> {
        let span = self.across_span(notes)?;
        let width_mm = span.abs() * 1000.0;
        if width_mm < c.min_width_mm as f64 - 1e-6 || width_mm > c.max_width_mm as f64 + 1e-6 {
            bail!(notes, "marking.crossing.width", "The crossing is {:.0} mm wide; {} gives {} to {} mm.", width_mm, spec.citation, c.min_width_mm, c.max_width_mm);
        }
        let frame = Frame::at(a, self.station_m, self.offset_m, self.rotation_deg, notes)?;
        let sign = span.signum();
        let mut laid = ExtLaid { groups: vec![spec.name.to_string()], overall_length_m: span.abs(), ..Default::default() };
        if c.stripes {
            let depth = match self.width_mm {
                Some(d) if d >= c.mark_min_mm && d <= c.mark_max_mm => d,
                Some(d) => bail!(notes, "marking.dimension.not_prescribed", "A stripe {d} mm deep is outside {} to {} mm ({}).", c.mark_min_mm, c.mark_max_mm, spec.citation),
                None => bail!(notes, "marking.dimension.required", "Set the stripe depth (width_mm, {} to {} mm) ({}).", c.mark_min_mm, c.mark_max_mm, spec.citation),
            };
            let gap = match self.gap_mm {
                Some(g) if g >= c.gap_min_mm && g <= c.gap_max_mm => g,
                Some(g) => bail!(notes, "marking.dimension.not_prescribed", "A gap of {g} mm is outside {} to {} mm ({}).", c.gap_min_mm, c.gap_max_mm, spec.citation),
                None => bail!(notes, "marking.dimension.required", "Set the gap between stripes (gap_mm, {} to {} mm) ({}).", c.gap_min_mm, c.gap_max_mm, spec.citation),
            };
            let Some(n) = self.count.filter(|n| *n > 0) else {
                bail!(notes, "marking.dimension.required", "Set the number of stripes (count).");
            };
            let (d, g) = (depth as f64 / 1000.0, gap as f64 / 1000.0);
            for k in 0..n {
                let y0 = k as f64 * (d + g);
                laid.pieces.push(Piece { group: 0, shape: Shape::Poly(frame.poly(&rect(0.0, sign * span.abs(), y0, y0 + d))) });
            }
            laid.painted_area_m2 = Some(n as f64 * d * span.abs());
            laid.painted_length_m = n as f64 * span.abs();
            laid.number = n as usize;
            derived.push(("Stripes".into(), format!("{n}, each {depth} mm deep and {:.3} m long, {gap} mm gaps", span.abs())));
        } else {
            let Some(length) = self.length_mm.filter(|l| *l > 0) else {
                bail!(notes, "marking.dimension.required", "Set the length of the crossing along the road (length_mm).");
            };
            let (lw, mark) = (c.line_width_mm as f64 / 1000.0, c.mark_min_mm as f64 / 1000.0);
            let pattern = Pattern::Broken { mark_mm: c.mark_min_mm, gap_mm: c.gap_min_mm };
            let marks = intervals(pattern, length as f64 / 1000.0, PatternAnchor::Start);
            let mut area = 0.0;
            for side in 0..2 {
                let x0 = if side == 0 { 0.0 } else { sign * (span.abs() + lw) };
                let x1 = x0 + sign * lw;
                for (from, to, whole) in &marks {
                    laid.pieces.push(Piece { group: 0, shape: Shape::Poly(frame.poly(&rect(x0, x1, *from, *to))) });
                    area += (to - from) * lw;
                    laid.painted_length_m += to - from;
                    if *whole {
                        laid.number += 1;
                    } else {
                        laid.part_marks += 1;
                    }
                }
            }
            let _ = mark;
            laid.painted_area_m2 = Some(area);
            derived.push(("Side lines".into(), format!("two {} mm lines, {}, {} m apart (inner edges), {length} mm long", c.line_width_mm, pattern.label(), span.abs())));
        }
        Some(laid)
    }

    // ----------------------------------------------------------------------------- studs
    fn lay_studs(&self, a: &Alignment, notes: &mut Vec<Note>, mut derived: Vec<(String, String)>) -> Option<(ExtResolved, ExtLaid)> {
        let end = self.end_station(notes)?;
        let Some(spacing) = self.spacing_mm.filter(|s| *s > 0) else {
            bail!(notes, "marking.stud.spacing", "The stud spacing (spacing_mm) is needed: take it from stud_rows() for the line type.");
        };
        let Some(colour) = self.stud_colour else {
            bail!(notes, "marking.stud.colour", "The stud colour is needed (white, red, amber or green).");
        };
        let (stations, length) = match stations_along(a, self.station_m, end, self.offset_m, spacing as f64 / 1000.0) {
            Ok(r) => r,
            Err(e) => {
                let (code, message) = layout_error(&e);
                bail!(notes, code, "{message}");
            }
        };
        // Plan size of the display marker: for display only; the sources give a height only.
        let half = 0.05;
        let mut laid = ExtLaid { groups: vec!["Road studs".into()], overall_length_m: length, number: stations.len(), ..Default::default() };
        for s in &stations {
            let p = a.point_at(*s)?;
            let (cx, cy) = p.offset(self.offset_m);
            let (c, sn) = (p.heading_rad.cos(), p.heading_rad.sin());
            let corner = |x: f64, y: f64| (cx + y * c + x * sn, cy + y * sn - x * c);
            laid.pieces.push(Piece { group: 0, shape: Shape::Poly(vec![corner(-half, -half), corner(half, -half), corner(half, half), corner(-half, half)]) });
        }
        derived.push(("Marking".into(), "Road studs".into()));
        derived.push(("Stud spacing".into(), format!("{spacing} mm along the line")));
        derived.push(("Stud colour".into(), colour.label().into()));
        if !self.stud_row.is_empty() {
            derived.push(("Stud row".into(), self.stud_row.clone()));
        }
        derived.push(("Studs".into(), format!("{} (one at the start station and one at each whole spacing)", stations.len())));
        derived.push(("Source".into(), self.basis.edition().to_string()));
        notes.push(note(Severity::Info, "marking.stud.display", "The stud marker is drawn 100 mm square and 5 mm high for display only; no source gives a plan size, and the manual's maximum height is 20 mm (non-depressible) or 25 mm (depressible), UK TSM Chapter 5 (2018) 4.3.2.".to_string()));
        Some((
            ExtResolved {
                id: STUD_ID.to_string(),
                name: "Road studs".to_string(),
                diagram: "studs".to_string(),
                tsrgd: String::new(),
                colour: MarkingColour::White,
                stud_colour: Some(colour),
                citation: self.basis.edition().to_string(),
                derived,
                geometry: Some(GeometryStatus::Full),
            },
            laid,
        ))
    }
}

fn pick_arrow_size<'a>(size_mm: Option<u32>, speed: Option<u32>, sizes: &'a [ArrowSize], spec: &ExtSpec, notes: &mut Vec<Note>) -> Option<&'a ArrowSize> {
    let lengths = list(&sizes.iter().map(|s| s.length_mm).collect::<Vec<_>>());
    if let Some(l) = size_mm {
        match sizes.iter().find(|s| s.length_mm == l) {
            Some(s) => return Some(s),
            None => bail!(notes, "marking.dimension.not_prescribed", "{l} mm is not a length of {}: the lengths are {lengths} mm ({}).", spec.diagram, spec.citation),
        }
    }
    match speed {
        Some(v) => match sizes.iter().find(|s| s.sel.contains(v)) {
            Some(s) => Some(s),
            None => bail!(notes, "marking.speed.no_row", "{} has no row for a speed of {v}; the lengths are {lengths} mm ({}). Set size_mm to choose one.", spec.diagram, spec.citation),
        },
        None if sizes.len() == 1 => Some(&sizes[0]),
        None => bail!(notes, "marking.speed.missing", "The speed limit (or size_mm) is needed to choose the length of {}: {lengths} mm ({}).", spec.diagram, spec.citation),
    }
}

/// The polygons of the hollow triangle in its own frame (mm converted to m), centred on its
/// bounding block, base at +y.
pub(crate) fn triangle_parts(t: &TriangleDef) -> Vec<Vec<P>> {
    let (b, h) = (t.base_mm as f64 / 1000.0, t.height_mm as f64 / 1000.0);
    let (side, base) = (t.side_border_mm as f64 / 1000.0, t.base_border_mm as f64 / 1000.0);
    let half = b / 2.0;
    let tan = half / h;
    let sin = half / (half * half + h * h).sqrt();
    let apex_out = -h / 2.0;
    let apex_in = apex_out + side / sin;
    let base_out = h / 2.0;
    let base_in = base_out - base;
    let hw = (base_in - apex_in) * tan;
    vec![
        vec![(-half, base_out), (half, base_out), (hw, base_in), (-hw, base_in)],
        vec![(-half, base_out), (0.0, apex_out), (0.0, apex_in), (-hw, base_in)],
        vec![(half, base_out), (0.0, apex_out), (0.0, apex_in), (hw, base_in)],
    ]
}

/// The polygons of the Uganda M41 arrow in its own frame, centred on its bounding block, head
/// at +y. Lengths of head and shaft are scaled with `length_mm / base_length_mm`; widths are not.
pub(crate) fn tapered_arrow_parts(t: &crate::ext_catalogue::TaperedArrow, length_mm: u32) -> Vec<Vec<P>> {
    let l = length_mm as f64 / 1000.0;
    let head = t.head_length_mm as f64 / 1000.0 * length_mm as f64 / t.base_length_mm as f64;
    let (tail, neck, hw) = (t.tail_width_mm as f64 / 2000.0, t.neck_width_mm as f64 / 2000.0, t.head_width_mm as f64 / 2000.0);
    let (y_tail, y_neck, y_tip) = (-l / 2.0, l / 2.0 - head, l / 2.0);
    vec![vec![(-tail, y_tail), (tail, y_tail), (neck, y_neck), (-neck, y_neck)], vec![(-hw, y_neck), (hw, y_neck), (0.0, y_tip)]]
}

