//! Sign gantries (portal and cantilever) as AfroBIM model elements.
//!
//! A gantry stands at a station on an alignment, square or at an entered skew, and carries signs
//! over the carriageway. This crate models it, sets it out, draws it, measures it and checks it
//! against the dimensional requirements of the UK DMRB that can be implemented exactly.
//!
//! **This crate does not design the structure.** Members are *schematic envelopes*: the leg and
//! boom dimensions are whatever the engineer entered, and nothing here sizes a member, computes a
//! wind load, a deflection or a foundation. See [`design`] for what the requirement checks cover
//! and the crate README for the governing documents and their revisions.
//!
//! # Conventions
//!
//! * Offsets are metres, measured **square to the alignment** at the gantry's station, positive
//!   to the right looking up-station (as `afrobim-alignment`).
//! * The **gantry line** is the plan line the legs and boom stand on. With no skew it is square
//!   to the alignment. A skew of `s` degrees turns it counter-clockwise seen from above, so a
//!   point at square offset `o` lies `o·tan s` up-station of the gantry's station and `o / cos s`
//!   along the gantry line from the centreline.
//! * The **reference level** is the alignment's profile (centreline) level at the gantry's
//!   station. Every level of the gantry is entered as a height above it, so the gantry follows
//!   the profile. Without a profile the reference level is 0.
//! * Local frame of the element (solids and site placement): origin on the gantry line at its
//!   leftmost extent, at the reference level; +X along the gantry line towards increasing
//!   offset; +Y square to it (up-station for a square gantry); +Z up. Millimetres.
//! * The sheet drawing is an elevation seen looking along +Y (up-station for a square gantry):
//!   drawing X runs along the gantry line and drawing Y is height, lowest point on y = 0.

pub mod design;
mod geometry;
mod hosting;
mod module;
mod quantities;

pub use geometry::{SettingOut, SignSetting};
pub use hosting::{
    add_gantry, add_sign_to_gantry, definition_of, gantry_of, remove_gantry, remove_sign_from_gantry, set_definition, signs_of,
    support_conflicts, sync_gantry, SyncReport, GANTRY_LAYER, PROP_GANTRY,
};
pub use module::GantryModule;
pub use quantities::{gantries_to_csv, GantryQuantities, GANTRIES_CSV_HEADER};

use afrobim_kernel::{ElementId, Note, Severity};
use serde::{Deserialize, Serialize};

/// The element category this crate owns.
pub const GANTRY_CATEGORY: &str = "gantry";
/// The category of the elements a gantry can carry.
pub const SIGN_CATEGORY: &str = "sign";

/// Note code raised (Info) on a gantry whose dimensions are still the placeholders.
pub const NOTE_PLACEHOLDER: &str = "PLACEHOLDER_DIMENSIONS";
/// Note code raised (Info) on every gantry: the members are schematic envelopes.
pub const NOTE_SCHEMATIC: &str = "gantry.schematic";

/// PLACEHOLDER starting dimensions, used only so that a new gantry can be drawn before the
/// engineer has entered real values. They are round numbers, come from no standard and are not
/// adequate, typical or compliant values. A definition built from them has `placeholder` set.
pub const PLACEHOLDER_LEG_OFFSET_M: f64 = 10.0;
/// PLACEHOLDER, see [`PLACEHOLDER_LEG_OFFSET_M`].
pub const PLACEHOLDER_BOOM_UNDERSIDE_MM: f64 = 6000.0;
/// PLACEHOLDER, see [`PLACEHOLDER_LEG_OFFSET_M`].
pub const PLACEHOLDER_MEMBER_MM: f64 = 500.0;
/// PLACEHOLDER, see [`PLACEHOLDER_LEG_OFFSET_M`].
pub const PLACEHOLDER_BOOM_DEPTH_MM: f64 = 1000.0;
/// PLACEHOLDER side and depth of a foundation block, see [`PLACEHOLDER_LEG_OFFSET_M`].
pub const PLACEHOLDER_FOUNDATION_MM: f64 = 2000.0;

/// The structural form. CD 365 Revision 1 (Terms and definitions, "Gantry") names "single or
/// multiple portals, single and double cantilevers and combinations of same"; this first cut
/// models the single portal and the single cantilever only.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum GantryType {
    /// A boom spanning between two legs.
    Portal,
    /// A boom cantilevering from one leg.
    Cantilever,
}

impl GantryType {
    pub fn label(self) -> &'static str {
        match self {
            GantryType::Portal => "portal",
            GantryType::Cantilever => "cantilever",
        }
    }
}

/// The outside envelope of a member, as entered. A box or a circle that the real member (a
/// truss, a plate girder, a tube) fits inside: not a section, and not a design.
///
/// For a **leg**, `width_mm` is measured along the gantry line and `depth_mm` square to it.
/// For the **boom**, `width_mm` is its plan width (square to the gantry line) and `depth_mm` its
/// vertical depth.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(tag = "shape", rename_all = "lowercase")]
pub enum Envelope {
    Box { width_mm: f64, depth_mm: f64 },
    Chs { diameter_mm: f64 },
}

impl Envelope {
    pub fn width_mm(&self) -> f64 {
        match *self {
            Envelope::Box { width_mm, .. } => width_mm,
            Envelope::Chs { diameter_mm } => diameter_mm,
        }
    }

    pub fn depth_mm(&self) -> f64 {
        match *self {
            Envelope::Box { depth_mm, .. } => depth_mm,
            Envelope::Chs { diameter_mm } => diameter_mm,
        }
    }

    pub fn label(&self) -> String {
        match *self {
            Envelope::Box { width_mm, depth_mm } => format!("box envelope {} x {} mm", fixed(width_mm, 0, 1), fixed(depth_mm, 0, 1)),
            Envelope::Chs { diameter_mm } => format!("circular envelope {} mm diameter", fixed(diameter_mm, 0, 1)),
        }
    }

    fn valid(&self) -> bool {
        positive(self.width_mm()) && positive(self.depth_mm())
    }
}

/// A foundation as an entered rectangular block under a leg. Its top is at the leg's base.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct FoundationBlock {
    /// Along the gantry line.
    pub length_mm: f64,
    /// Square to the gantry line.
    pub width_mm: f64,
    pub depth_mm: f64,
}

impl FoundationBlock {
    /// length × width × depth, m³.
    pub fn volume_m3(&self) -> f64 {
        self.length_mm * self.width_mm * self.depth_mm / 1.0e9
    }
}

/// One leg (support).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Leg {
    /// Offset of the leg's axis from the centreline, m, square to the alignment.
    pub offset_m: f64,
    pub envelope: Envelope,
    /// Level of the leg's base (top of plinth or foundation) above the reference level, mm.
    /// Negative when the verge is below the centreline.
    #[serde(default)]
    pub base_above_reference_mm: f64,
    /// Mass per metre of the real member, kg/m, when the engineer has one. Steel mass is
    /// reported only from this; it is never estimated from the envelope.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub mass_kg_per_m: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub foundation: Option<FoundationBlock>,
}

/// The boom (main horizontal member, a beam or a truss).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Boom {
    /// Level of the boom's underside above the reference level, mm.
    pub underside_above_reference_mm: f64,
    pub envelope: Envelope,
    /// See [`Leg::mass_kg_per_m`].
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub mass_kg_per_m: Option<f64>,
}

/// The direction of the traffic in a lane, relative to the alignment's stationing.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Direction {
    UpStation,
    DownStation,
}

/// One traffic lane.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct Lane {
    pub width_m: f64,
    pub direction: Direction,
}

/// One carriageway of the cross-section at the gantry, as entered. A single carriageway road has
/// one (its lanes running in both directions); a dual carriageway has two, entered left to right.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Carriageway {
    /// Offset of the carriageway's left edge, m.
    pub left_edge_offset_m: f64,
    /// Lanes from the left edge to the right.
    pub lanes: Vec<Lane>,
    /// Width of hard strip or hard shoulder outside the left edge, m. With the carriageway it
    /// makes up the paved width (CD 127 Version 1.0.1, Terms and definitions, "Paved width").
    #[serde(default)]
    pub left_hard_strip_m: f64,
    /// The same outside the right edge.
    #[serde(default)]
    pub right_hard_strip_m: f64,
}

impl Carriageway {
    /// A carriageway of equal lanes all running one way.
    pub fn one_way(left_edge_offset_m: f64, lanes: usize, lane_width_m: f64, direction: Direction) -> Carriageway {
        Carriageway { left_edge_offset_m, lanes: vec![Lane { width_m: lane_width_m, direction }; lanes], left_hard_strip_m: 0.0, right_hard_strip_m: 0.0 }
    }

    pub fn width_m(&self) -> f64 {
        self.lanes.iter().map(|l| l.width_m).sum()
    }

    pub fn right_edge_offset_m(&self) -> f64 {
        self.left_edge_offset_m + self.width_m()
    }

    /// Offsets of the two edges of the paved width (carriageway plus hard strips or shoulders).
    pub fn paved_edges_m(&self) -> (f64, f64) {
        (self.left_edge_offset_m - self.left_hard_strip_m, self.right_edge_offset_m() + self.right_hard_strip_m)
    }
}

/// Where a sign sits along the boom: the position of the **centre** of its face.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SignPosition {
    /// At a square offset from the centreline, m.
    Offset { offset_m: f64 },
    /// Over the centre of a lane. Lanes are numbered from 1 at the left of the first carriageway
    /// entered, across all carriageways.
    Lane { index: usize },
}

/// A sign carried by the gantry. The gantry's definition is the record of the hosting; the sign
/// element's `host` and `site` are derived from it by [`sync_gantry`].
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SignMount {
    pub sign: ElementId,
    /// The sign's mark, width and height, copied from the sign element at the last sync so the
    /// gantry can be drawn and measured on its own.
    #[serde(default)]
    pub mark: String,
    pub width_mm: f64,
    pub height_mm: f64,
    pub position: SignPosition,
    /// Height of the sign's lower edge above the boom's underside, mm. Negative when the sign
    /// hangs below the boom, in which case it governs the headroom.
    pub bottom_above_boom_underside_mm: f64,
    /// The traffic the sign serves. `None` takes the direction of the lane under the sign's
    /// centre; it must be given when no lane is entered there.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub serves: Option<Direction>,
}

/// A gantry: the module-owned `data` of a `gantry` element.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct GantryDefinition {
    pub gantry_type: GantryType,
    /// Name of the alignment the gantry stands on.
    pub alignment: String,
    /// Internal (continuous) station of the gantry line's crossing of the centreline, m.
    pub station_m: f64,
    /// Skew of the gantry line, degrees counter-clockwise seen from above; 0 is square.
    #[serde(default)]
    pub skew_deg: f64,
    /// Two legs for a portal (any order), one for a cantilever.
    pub legs: Vec<Leg>,
    /// Cantilever only: the square offset of the boom's tip, m.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tip_offset_m: Option<f64>,
    pub boom: Boom,
    /// The carriageways under the gantry, left to right. Needed for lane positions, the drawing
    /// and the checks; may be empty.
    #[serde(default)]
    pub carriageways: Vec<Carriageway>,
    #[serde(default)]
    pub signs: Vec<SignMount>,
    /// The reference level (m) found at the last sync, kept so the element can report levels
    /// without the alignment. `None` when the alignment has no profile at the station.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub reference_level_m: Option<f64>,
    /// What the engineer has entered for the requirement checks.
    #[serde(default)]
    pub checks: design::CheckInputs,
    /// True while the dimensions are the `PLACEHOLDER_*` values.
    #[serde(default)]
    pub placeholder: bool,
}

/// Why a gantry could not be created, hosted on or measured.
#[derive(Debug, Clone, PartialEq, thiserror::Error)]
pub enum GantryError {
    #[error("element {0} is not in the document")]
    ElementNotFound(u64),
    #[error("'{0}' is not a gantry")]
    NotAGantry(String),
    #[error("'{0}' is not a sign")]
    NotASign(String),
    #[error("the gantry definition is not valid: {0}")]
    InvalidDefinition(String),
    #[error("'{0}' has post supports; a sign has either posts or a gantry, so remove its supports first")]
    SignHasPostSupports(String),
    #[error("'{0}' is already hosted by '{1}'")]
    AlreadyHosted(String, String),
    #[error("'{0}' is not carried by this gantry")]
    NotCarried(String),
    #[error("the sign position is not valid: {0}")]
    InvalidPosition(String),
    #[error("the gantry is on alignment '{expected}' but '{given}' was supplied")]
    WrongAlignment { expected: String, given: String },
    #[error("station {0} m is not on the alignment")]
    StationOffAlignment(f64),
}

impl GantryDefinition {
    /// A portal with PLACEHOLDER dimensions (see [`PLACEHOLDER_LEG_OFFSET_M`]) so that a new
    /// gantry can be drawn at once. Every dimension is to be replaced by the engineer.
    pub fn placeholder_portal(alignment: &str, station_m: f64) -> GantryDefinition {
        let leg = |offset_m: f64| Leg {
            offset_m,
            envelope: Envelope::Box { width_mm: PLACEHOLDER_MEMBER_MM, depth_mm: PLACEHOLDER_MEMBER_MM },
            base_above_reference_mm: 0.0,
            mass_kg_per_m: None,
            foundation: Some(FoundationBlock { length_mm: PLACEHOLDER_FOUNDATION_MM, width_mm: PLACEHOLDER_FOUNDATION_MM, depth_mm: PLACEHOLDER_FOUNDATION_MM }),
        };
        GantryDefinition {
            gantry_type: GantryType::Portal,
            alignment: alignment.to_string(),
            station_m,
            skew_deg: 0.0,
            legs: vec![leg(-PLACEHOLDER_LEG_OFFSET_M), leg(PLACEHOLDER_LEG_OFFSET_M)],
            tip_offset_m: None,
            boom: Boom {
                underside_above_reference_mm: PLACEHOLDER_BOOM_UNDERSIDE_MM,
                envelope: Envelope::Box { width_mm: PLACEHOLDER_MEMBER_MM, depth_mm: PLACEHOLDER_BOOM_DEPTH_MM },
                mass_kg_per_m: None,
            },
            carriageways: Vec::new(),
            signs: Vec::new(),
            reference_level_m: None,
            checks: design::CheckInputs::default(),
            placeholder: true,
        }
    }

    /// A cantilever with PLACEHOLDER dimensions: one leg at the placeholder offset on the left,
    /// its tip over the centreline.
    pub fn placeholder_cantilever(alignment: &str, station_m: f64) -> GantryDefinition {
        let mut d = GantryDefinition::placeholder_portal(alignment, station_m);
        d.gantry_type = GantryType::Cantilever;
        d.legs.truncate(1);
        d.tip_offset_m = Some(0.0);
        d
    }

    pub fn to_value(&self) -> serde_json::Value {
        serde_json::to_value(self).expect("a gantry definition is plain data")
    }

    pub fn from_value(value: &serde_json::Value) -> Result<GantryDefinition, GantryError> {
        serde_json::from_value(value.clone()).map_err(|e| GantryError::InvalidDefinition(e.to_string()))
    }

    /// Fails with the Error notes joined when the definition has any.
    pub fn check(&self) -> Result<(), GantryError> {
        let errors: Vec<String> = self.notes().into_iter().filter(|n| n.severity == Severity::Error).map(|n| n.message).collect();
        if errors.is_empty() {
            Ok(())
        } else {
            Err(GantryError::InvalidDefinition(errors.join(" ")))
        }
    }

    /// Validation notes: Errors make the gantry undrawable; Warnings and Infos do not.
    pub fn notes(&self) -> Vec<Note> {
        let mut notes = Vec::new();
        let mut error = |code: &str, message: String| notes.push(Note { severity: Severity::Error, code: code.to_string(), message });
        if !self.station_m.is_finite() {
            error("gantry.station", "The station is not a number.".into());
        }
        if !(self.skew_deg.is_finite() && self.skew_deg.abs() < 90.0) {
            error("gantry.skew", format!("The skew must be between -90 and 90 degrees (got {}).", self.skew_deg));
        }
        let wanted = match self.gantry_type {
            GantryType::Portal => 2,
            GantryType::Cantilever => 1,
        };
        if self.legs.len() != wanted {
            error("gantry.legs", format!("A {} gantry has {wanted} leg(s); {} entered.", self.gantry_type.label(), self.legs.len()));
        }
        for (i, leg) in self.legs.iter().enumerate() {
            if !leg.offset_m.is_finite() || !leg.base_above_reference_mm.is_finite() {
                error("gantry.leg", format!("Leg {}: the offset and base level must be numbers.", i + 1));
            }
            if !leg.envelope.valid() {
                error("gantry.leg", format!("Leg {}: the envelope dimensions must be greater than zero.", i + 1));
            }
            if leg.mass_kg_per_m.is_some_and(|m| !positive(m)) {
                error("gantry.leg", format!("Leg {}: the mass per metre must be greater than zero.", i + 1));
            }
            if leg.foundation.is_some_and(|f| !(positive(f.length_mm) && positive(f.width_mm) && positive(f.depth_mm))) {
                error("gantry.foundation", format!("Leg {}: the foundation block dimensions must be greater than zero.", i + 1));
            }
        }
        if !self.boom.envelope.valid() || !self.boom.underside_above_reference_mm.is_finite() {
            error("gantry.boom", "The boom needs a level and envelope dimensions greater than zero.".into());
        }
        if self.boom.mass_kg_per_m.is_some_and(|m| !positive(m)) {
            error("gantry.boom", "The boom's mass per metre must be greater than zero.".into());
        }
        match self.gantry_type {
            GantryType::Cantilever if !self.tip_offset_m.is_some_and(f64::is_finite) => {
                error("gantry.tip", "A cantilever needs the offset of its tip.".into());
            }
            _ => {}
        }
        let mut last_right = f64::NEG_INFINITY;
        for (i, c) in self.carriageways.iter().enumerate() {
            let ok = c.left_edge_offset_m.is_finite()
                && !c.lanes.is_empty()
                && c.lanes.iter().all(|l| positive(l.width_m))
                && c.left_hard_strip_m.is_finite()
                && c.left_hard_strip_m >= 0.0
                && c.right_hard_strip_m.is_finite()
                && c.right_hard_strip_m >= 0.0;
            if !ok {
                error("gantry.carriageway", format!("Carriageway {}: it needs an edge offset, at least one lane of positive width and hard strips of zero or more.", i + 1));
                continue;
            }
            let (left, right) = c.paved_edges_m();
            if left < last_right {
                error("gantry.carriageway", format!("Carriageway {} overlaps the one before it; enter carriageways left to right.", i + 1));
            }
            last_right = right;
        }
        if !notes.is_empty() {
            return notes;
        }
        // Geometry that needs valid numbers.
        if self.boom_length_mm() <= 0.0 {
            notes.push(Note {
                severity: Severity::Error,
                code: "gantry.span".into(),
                message: match self.gantry_type {
                    GantryType::Portal => "The legs overlap: there is no clear span between them.".into(),
                    GantryType::Cantilever => "The tip is inside the leg: there is no outreach.".into(),
                },
            });
            return notes;
        }
        for (i, leg) in self.legs.iter().enumerate() {
            if self.boom.underside_above_reference_mm <= leg.base_above_reference_mm {
                notes.push(Note { severity: Severity::Error, code: "gantry.leg".into(), message: format!("Leg {}: its base is not below the boom.", i + 1) });
            }
            let (near, far) = self.leg_faces_offset_m(i);
            if self.carriageways.iter().any(|c| {
                let (l, r) = c.paved_edges_m();
                near.min(far) < r && near.max(far) > l
            }) {
                notes.push(Note { severity: Severity::Error, code: "gantry.leg".into(), message: format!("Leg {} stands in the paved width.", i + 1) });
            }
        }
        let (u0, u1) = self.boom_range_mm();
        for m in &self.signs {
            if !(positive(m.width_mm) && positive(m.height_mm) && m.bottom_above_boom_underside_mm.is_finite()) {
                notes.push(Note { severity: Severity::Error, code: "gantry.sign".into(), message: format!("Sign '{}': it needs a width and height greater than zero and a vertical position.", m.mark) });
                continue;
            }
            match self.sign_centre_offset_m(m) {
                Err(e) => notes.push(Note { severity: Severity::Error, code: "gantry.sign".into(), message: format!("Sign '{}': {e}", m.mark) }),
                Ok(o) => {
                    if self.sign_direction(m).is_err() {
                        notes.push(Note {
                            severity: Severity::Error,
                            code: "gantry.sign".into(),
                            message: format!("Sign '{}': no lane is entered under it, so state which traffic it serves.", m.mark),
                        });
                    }
                    let u = self.along_mm(o);
                    if u - m.width_mm / 2.0 < u0 - 1e-6 || u + m.width_mm / 2.0 > u1 + 1e-6 {
                        notes.push(Note {
                            severity: Severity::Warning,
                            code: "gantry.sign.overhang".into(),
                            message: format!("Sign '{}' extends beyond the boom.", m.mark),
                        });
                    }
                }
            }
        }
        notes.push(Note {
            severity: Severity::Info,
            code: NOTE_SCHEMATIC.into(),
            message: "Legs, boom and foundations are schematic envelopes of entered dimensions; this is not a structural design.".into(),
        });
        if self.placeholder {
            notes.push(Note {
                severity: Severity::Info,
                code: NOTE_PLACEHOLDER.into(),
                message: "The dimensions are placeholders from no standard; enter the real ones.".into(),
            });
        }
        notes
    }

    // ---- geometry along the gantry line (valid definitions only) ----

    pub(crate) fn skew_cos(&self) -> f64 {
        self.skew_deg.to_radians().cos()
    }

    /// Distance along the gantry line from the centreline of a point at a square offset, mm.
    pub fn along_mm(&self, offset_m: f64) -> f64 {
        offset_m * 1000.0 / self.skew_cos()
    }

    /// Legs in order of increasing offset.
    pub(crate) fn legs_sorted(&self) -> Vec<&Leg> {
        let mut legs: Vec<&Leg> = self.legs.iter().collect();
        legs.sort_by(|a, b| a.offset_m.total_cmp(&b.offset_m));
        legs
    }

    /// The two ends of the boom along the gantry line, mm from the centreline, left then right.
    /// Portal: between the legs' inner faces. Cantilever: from the leg's traffic face (the face
    /// towards the tip) to the tip.
    pub fn boom_range_mm(&self) -> (f64, f64) {
        match self.gantry_type {
            GantryType::Portal => {
                let legs = self.legs_sorted();
                (self.along_mm(legs[0].offset_m) + legs[0].envelope.width_mm() / 2.0, self.along_mm(legs[1].offset_m) - legs[1].envelope.width_mm() / 2.0)
            }
            GantryType::Cantilever => {
                let leg = &self.legs[0];
                let (u, tip) = (self.along_mm(leg.offset_m), self.along_mm(self.tip_offset_m.unwrap_or(leg.offset_m)));
                let half = leg.envelope.width_mm() / 2.0;
                if tip >= u {
                    (u + half, tip)
                } else {
                    (tip, u - half)
                }
            }
        }
    }

    /// Length of the boom envelope along the gantry line, mm: the clear span between the legs'
    /// inner faces for a portal, the outreach "from traffic face of support to tip" (CD 365
    /// Revision 1, Terms and definitions, "Outreach of cantilever") for a cantilever.
    pub fn boom_length_mm(&self) -> f64 {
        let (a, b) = self.boom_range_mm();
        b - a
    }

    /// Portal: the clear span between the legs' inner faces along the gantry line, mm.
    pub fn clear_span_mm(&self) -> Option<f64> {
        (self.gantry_type == GantryType::Portal).then(|| self.boom_length_mm())
    }

    /// Cantilever: the outreach from the traffic face of the leg to the tip, mm.
    pub fn outreach_mm(&self) -> Option<f64> {
        (self.gantry_type == GantryType::Cantilever).then(|| self.boom_length_mm())
    }

    /// The overall extent along the gantry line, mm from the centreline: outer faces of the legs
    /// and the tip.
    pub fn extent_mm(&self) -> (f64, f64) {
        let (mut lo, mut hi) = self.boom_range_mm();
        for leg in &self.legs {
            let u = self.along_mm(leg.offset_m);
            lo = lo.min(u - leg.envelope.width_mm() / 2.0);
            hi = hi.max(u + leg.envelope.width_mm() / 2.0);
        }
        (lo, hi)
    }

    /// Level of the top of the boom above the reference level, mm.
    pub fn boom_top_mm(&self) -> f64 {
        self.boom.underside_above_reference_mm + self.boom.envelope.depth_mm()
    }

    /// Length of a leg's envelope, mm: from its base to the top of the boom (the leg runs past
    /// the end of the boom in this schematic).
    pub fn leg_length_mm(&self, leg: &Leg) -> f64 {
        self.boom_top_mm() - leg.base_above_reference_mm
    }

    /// Height of a support as CD 365 Revision 1 clause 6.9 defines it for the deformation
    /// limits: "from the top of any support plinth to the underside of the main horizontal
    /// structural member", mm. (The clause adds "or signs/signal"; a sign lower than the boom is
    /// not taken here.)
    pub fn support_height_mm(&self, leg: &Leg) -> f64 {
        self.boom.underside_above_reference_mm - leg.base_above_reference_mm
    }

    /// Square offsets (m) of leg `index`'s two faces across the gantry line, at the gantry line.
    pub(crate) fn leg_faces_offset_m(&self, index: usize) -> (f64, f64) {
        let leg = &self.legs[index];
        let half = leg.envelope.width_mm() / 2.0 * self.skew_cos() / 1000.0;
        (leg.offset_m - half, leg.offset_m + half)
    }

    /// Every lane with the offsets of its two edges, in numbering order.
    pub fn lanes(&self) -> Vec<(f64, f64, Direction)> {
        let mut out = Vec::new();
        for c in &self.carriageways {
            let mut left = c.left_edge_offset_m;
            for lane in &c.lanes {
                out.push((left, left + lane.width_m, lane.direction));
                left += lane.width_m;
            }
        }
        out
    }

    /// The square offset of the centre of a carried sign, m.
    pub fn sign_centre_offset_m(&self, mount: &SignMount) -> Result<f64, GantryError> {
        match mount.position {
            SignPosition::Offset { offset_m } if offset_m.is_finite() => Ok(offset_m),
            SignPosition::Offset { .. } => Err(GantryError::InvalidPosition("the offset is not a number".into())),
            SignPosition::Lane { index } => {
                let lanes = self.lanes();
                match index.checked_sub(1).and_then(|i| lanes.get(i)) {
                    Some(&(l, r, _)) => Ok((l + r) / 2.0),
                    None => Err(GantryError::InvalidPosition(format!("there is no lane {index}; {} lane(s) are entered", lanes.len()))),
                }
            }
        }
    }

    /// The traffic a carried sign serves: as stated on the mount, else the direction of the lane
    /// under the sign's centre.
    pub fn sign_direction(&self, mount: &SignMount) -> Result<Direction, GantryError> {
        if let Some(d) = mount.serves {
            return Ok(d);
        }
        let o = self.sign_centre_offset_m(mount)?;
        self.lanes()
            .into_iter()
            .find(|&(l, r, _)| o >= l && o <= r)
            .map(|(_, _, d)| d)
            .ok_or_else(|| GantryError::InvalidPosition("no lane is entered under the sign, so state which traffic it serves".into()))
    }

    /// Level of the lowest point of the boom, the carried signs and any other entered equipment
    /// above the reference level, mm.
    pub fn lowest_point_mm(&self) -> f64 {
        let boom = self.boom.underside_above_reference_mm;
        self.signs.iter().map(|m| boom + m.bottom_above_boom_underside_mm).fold(boom - self.checks.equipment_below_boom_mm.max(0.0), f64::min)
    }

    /// Level of the highest point of the boom and the carried signs above the reference level, mm.
    pub fn highest_point_mm(&self) -> f64 {
        let boom = self.boom.underside_above_reference_mm;
        self.signs.iter().map(|m| boom + m.bottom_above_boom_underside_mm + m.height_mm).fold(self.boom_top_mm(), f64::max)
    }
}

pub(crate) fn positive(v: f64) -> bool {
    v.is_finite() && v > 0.0
}

/// `v` rounded to `max_decimals`, with trailing zeros removed down to `min_decimals`.
pub(crate) fn fixed(v: f64, min_decimals: usize, max_decimals: usize) -> String {
    let mut s = format!("{v:.max_decimals$}");
    if let Some(dot) = s.find('.') {
        while s.len() > dot + 1 + min_decimals && s.ends_with('0') {
            s.pop();
        }
        if s.ends_with('.') {
            s.pop();
        }
    }
    if s.starts_with('-') && s.chars().all(|c| matches!(c, '-' | '0' | '.')) {
        s.remove(0);
    }
    s
}

#[cfg(test)]
pub(crate) mod testing {
    use super::*;

    pub fn leg(offset_m: f64, base_mm: f64) -> Leg {
        Leg {
            offset_m,
            envelope: Envelope::Box { width_mm: 600.0, depth_mm: 400.0 },
            base_above_reference_mm: base_mm,
            mass_kg_per_m: None,
            foundation: Some(FoundationBlock { length_mm: 3000.0, width_mm: 2000.0, depth_mm: 1500.0 }),
        }
    }

    /// A portal on alignment "A1" at station 100: legs at −9 m and +9 m (600 × 400 boxes, bases
    /// 200 mm below and 300 mm above the reference level), boom underside 6000, 800 deep, 500
    /// wide in plan. One carriageway from −7.3 m, four 3.65 m lanes: two up-station on the left,
    /// two down-station on the right (keep left), with 1 m hard strips.
    pub fn portal() -> GantryDefinition {
        GantryDefinition {
            gantry_type: GantryType::Portal,
            alignment: "A1".into(),
            station_m: 100.0,
            skew_deg: 0.0,
            legs: vec![leg(-9.0, -200.0), leg(9.0, 300.0)],
            tip_offset_m: None,
            boom: Boom { underside_above_reference_mm: 6000.0, envelope: Envelope::Box { width_mm: 500.0, depth_mm: 800.0 }, mass_kg_per_m: None },
            carriageways: vec![Carriageway {
                left_edge_offset_m: -7.3,
                lanes: vec![
                    Lane { width_m: 3.65, direction: Direction::UpStation },
                    Lane { width_m: 3.65, direction: Direction::UpStation },
                    Lane { width_m: 3.65, direction: Direction::DownStation },
                    Lane { width_m: 3.65, direction: Direction::DownStation },
                ],
                left_hard_strip_m: 1.0,
                right_hard_strip_m: 1.0,
            }],
            signs: Vec::new(),
            reference_level_m: None,
            checks: design::CheckInputs::default(),
            placeholder: false,
        }
    }

    /// A cantilever: one CHS leg (500 dia) at −9 m, base at the reference level, tip at −1 m.
    pub fn cantilever() -> GantryDefinition {
        let mut d = portal();
        d.gantry_type = GantryType::Cantilever;
        d.legs = vec![Leg { offset_m: -9.0, envelope: Envelope::Chs { diameter_mm: 500.0 }, base_above_reference_mm: 0.0, mass_kg_per_m: None, foundation: None }];
        d.tip_offset_m = Some(-1.0);
        d
    }

    pub fn mount(sign: u64, position: SignPosition, bottom: f64) -> SignMount {
        SignMount { sign: ElementId(sign), mark: format!("S{sign}"), width_mm: 3000.0, height_mm: 2000.0, position, bottom_above_boom_underside_mm: bottom, serves: None }
    }

    pub fn close(a: f64, b: f64) -> bool {
        (a - b).abs() < 1e-6
    }
}

#[cfg(test)]
mod tests {
    use super::testing::*;
    use super::*;

    #[test]
    fn portal_span_square_and_skewed() {
        let mut d = portal();
        // Square: legs 18 m apart on centres, 600 wide each: clear span 18000 − 600 = 17400.
        assert!(close(d.clear_span_mm().unwrap(), 17400.0));
        assert_eq!(d.outreach_mm(), None);
        assert!(close(d.extent_mm().0, -9300.0) && close(d.extent_mm().1, 9300.0));
        // 60° skew: cos = 0.5, so the legs are 36 m apart along the gantry line: 36000 − 600.
        d.skew_deg = 60.0;
        assert!(close(d.clear_span_mm().unwrap(), 35400.0));
        assert!(close(d.along_mm(9.0), 18000.0));
        // Leg faces across the road: half width 300 mm along the line is 150 mm square.
        let (a, b) = d.leg_faces_offset_m(1);
        assert!(close(a, 8.85) && close(b, 9.15));
    }

    #[test]
    fn cantilever_outreach_is_from_the_traffic_face_to_the_tip() {
        let mut d = cantilever();
        // Leg axis −9 m, 500 dia: traffic face at −8.75 m; tip at −1 m: outreach 7750.
        assert!(close(d.outreach_mm().unwrap(), 7750.0));
        assert_eq!(d.boom_range_mm(), (-8750.0, -1000.0));
        assert_eq!(d.clear_span_mm(), None);
        // A leg on the right reaching left.
        d.legs[0].offset_m = 9.0;
        d.tip_offset_m = Some(2.0);
        assert_eq!(d.boom_range_mm(), (2000.0, 8750.0));
        assert!(d.check().is_ok());
    }

    #[test]
    fn leg_lengths_and_levels() {
        let d = portal();
        // Boom top = 6000 + 800 = 6800. Leg 1 base −200: 7000 long; leg 2 base +300: 6500.
        assert_eq!(d.boom_top_mm(), 6800.0);
        assert_eq!((d.leg_length_mm(&d.legs[0]), d.leg_length_mm(&d.legs[1])), (7000.0, 6500.0));
        // CD 365 Revision 1 cl. 6.9 height: plinth top to boom underside.
        assert_eq!((d.support_height_mm(&d.legs[0]), d.support_height_mm(&d.legs[1])), (6200.0, 5700.0));
    }

    #[test]
    fn lanes_number_from_the_left_and_give_sign_positions_and_directions() {
        let mut d = portal();
        let lanes = d.lanes();
        assert_eq!(lanes.len(), 4);
        assert!(close(lanes[0].0, -7.3) && close(lanes[3].1, 7.3));
        // Lane 2 runs −3.65..0: centre −1.825, up-station. Lane 3 centre +1.825, down-station.
        let m2 = mount(1, SignPosition::Lane { index: 2 }, 0.0);
        let m3 = mount(2, SignPosition::Lane { index: 3 }, 0.0);
        assert!(close(d.sign_centre_offset_m(&m2).unwrap(), -1.825));
        assert_eq!(d.sign_direction(&m2).unwrap(), Direction::UpStation);
        assert!(close(d.sign_centre_offset_m(&m3).unwrap(), 1.825));
        assert_eq!(d.sign_direction(&m3).unwrap(), Direction::DownStation);
        assert!(d.sign_centre_offset_m(&mount(3, SignPosition::Lane { index: 5 }, 0.0)).is_err());
        assert!(d.sign_centre_offset_m(&mount(3, SignPosition::Lane { index: 0 }, 0.0)).is_err());
        // By offset over the verge there is no lane: the direction must be stated.
        let mut verge = mount(4, SignPosition::Offset { offset_m: 8.5 }, 0.0);
        assert!(d.sign_direction(&verge).is_err());
        verge.serves = Some(Direction::DownStation);
        assert_eq!(d.sign_direction(&verge).unwrap(), Direction::DownStation);
        // Paved width: carriageway ± 1 m hard strips.
        let (l, r) = d.carriageways[0].paved_edges_m();
        assert!(close(l, -8.3) && close(r, 8.3));
        // Lowest and highest points with a sign hanging 150 below the boom, 2000 high.
        d.signs.push(mount(1, SignPosition::Lane { index: 2 }, -150.0));
        assert_eq!(d.lowest_point_mm(), 5850.0);
        assert_eq!(d.highest_point_mm(), 7850.0);
        d.checks.equipment_below_boom_mm = 400.0;
        assert_eq!(d.lowest_point_mm(), 5600.0);
    }

    #[test]
    fn validation_notes() {
        let d = portal();
        let notes = d.notes();
        assert!(d.check().is_ok());
        assert!(notes.iter().any(|n| n.code == NOTE_SCHEMATIC && n.severity == Severity::Info));
        assert!(!notes.iter().any(|n| n.code == NOTE_PLACEHOLDER));

        let p = GantryDefinition::placeholder_portal("A1", 0.0);
        assert!(p.check().is_ok());
        assert!(p.notes().iter().any(|n| n.code == NOTE_PLACEHOLDER));
        assert!(GantryDefinition::placeholder_cantilever("A1", 0.0).check().is_ok());

        let bad = |f: &dyn Fn(&mut GantryDefinition)| {
            let mut d = portal();
            f(&mut d);
            d.check().is_err()
        };
        assert!(bad(&|d| d.legs.truncate(1)));
        assert!(bad(&|d| d.skew_deg = 90.0));
        assert!(bad(&|d| d.legs[1].offset_m = -8.6)); // legs overlap
        assert!(bad(&|d| d.legs[1].offset_m = 8.0)); // leg in the paved width (edge at 8.3)
        assert!(bad(&|d| d.boom.underside_above_reference_mm = 200.0)); // leg 2 base at 300
        assert!(bad(&|d| d.boom.mass_kg_per_m = Some(0.0)));
        assert!(bad(&|d| d.carriageways[0].lanes.clear()));
        assert!(bad(&|d| d.signs.push(mount(1, SignPosition::Lane { index: 9 }, 0.0))));
        assert!(bad(&|d| d.signs.push(mount(1, SignPosition::Offset { offset_m: 8.5 }, 0.0)))); // no lane, no direction
        let mut c = cantilever();
        c.tip_offset_m = None;
        assert!(c.check().is_err());
        // A sign past the end of the boom is drawable but warned about.
        let mut d = portal();
        let mut m = mount(1, SignPosition::Offset { offset_m: 8.0 }, 0.0);
        m.serves = Some(Direction::UpStation);
        d.signs.push(m);
        assert!(d.check().is_ok());
        assert!(d.notes().iter().any(|n| n.code == "gantry.sign.overhang" && n.severity == Severity::Warning));
    }

    #[test]
    fn serde_round_trip() {
        let mut d = portal();
        d.skew_deg = 12.5;
        d.legs[0].mass_kg_per_m = Some(155.0);
        d.signs.push(mount(7, SignPosition::Lane { index: 1 }, -100.0));
        d.signs.push(SignMount { serves: Some(Direction::DownStation), ..mount(8, SignPosition::Offset { offset_m: 2.0 }, 50.0) });
        d.checks.sag_radius_m = Some(1500.0);
        d.reference_level_m = Some(1182.25);
        let back = GantryDefinition::from_value(&d.to_value()).unwrap();
        assert_eq!(back, d);
        let c = cantilever();
        assert_eq!(GantryDefinition::from_value(&c.to_value()).unwrap(), c);
        // Optional parts default when absent.
        let minimal = serde_json::json!({
            "gantry_type": "cantilever", "alignment": "A1", "station_m": 5.0,
            "legs": [{"offset_m": -9.0, "envelope": {"shape": "chs", "diameter_mm": 500.0}}],
            "tip_offset_m": -1.0,
            "boom": {"underside_above_reference_mm": 6000.0, "envelope": {"shape": "box", "width_mm": 400.0, "depth_mm": 600.0}}
        });
        let m = GantryDefinition::from_value(&minimal).unwrap();
        assert_eq!((m.skew_deg, m.signs.len(), m.placeholder), (0.0, 0, false));
        assert_eq!(m.checks, design::CheckInputs::default());
        assert!(m.check().is_ok());
    }
}
