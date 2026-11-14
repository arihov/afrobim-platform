//! The definitions stored in a lighting element's `data`. Everything here is entered data; no
//! field has a default taken from a standard.

use afrobim_kernel::{Note, Severity};
use serde::{Deserialize, Serialize};

use crate::layout::{Layout, Side};
use crate::{error_text, not_negative, note, positive, LightingError};

/// Where the columns of an installation stand relative to the carriageway.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum Arrangement {
    /// One row on the left looking up-station.
    SingleLeft,
    /// One row on the right looking up-station.
    SingleRight,
    /// Columns alternately on each side. The spacing is the distance along the alignment between
    /// one column and the next one on the other side, so columns on the same side are two
    /// spacings apart.
    Staggered,
    /// A column on each side at every station.
    Opposite,
    /// One column on the reference line (in the central reserve) carrying two luminaires, one
    /// towards each carriageway.
    TwinCentral,
}

impl Arrangement {
    pub fn label(self) -> &'static str {
        match self {
            Arrangement::SingleLeft => "single-sided left",
            Arrangement::SingleRight => "single-sided right",
            Arrangement::Staggered => "staggered",
            Arrangement::Opposite => "opposite",
            Arrangement::TwinCentral => "twin central",
        }
    }

    /// Luminaires carried by each column.
    pub fn luminaires_per_column(self) -> usize {
        if self == Arrangement::TwinCentral {
            2
        } else {
            1
        }
    }
}

/// What happens at the end station when the length is not a whole number of spacings.
///
/// TD 501 Version 0.1.0 says nothing about where the first column stands or how a short last bay
/// is treated, so this is the engineer's choice and it is recorded in the element. In every case
/// the first column stands at the start station.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
pub enum EndTreatment {
    /// Columns at the start station and every whole spacing after it. The remainder between the
    /// last column and the end station carries no column and is reported.
    #[default]
    KeepSpacing,
    /// As `KeepSpacing`, with one more column at the end station when there is a remainder. The
    /// last bay is then shorter than the spacing and is reported.
    ColumnAtEnd,
    /// The length is divided into the smallest number of equal bays that are no longer than the
    /// entered spacing, with a column at each end. The actual spacing is reported.
    EqualiseSpacing,
}

/// A luminaire as entered. Nothing is looked up from the product name.
#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
pub struct Luminaire {
    /// Product name or reference, free text.
    pub product: String,
    /// Luminous flux, lm.
    pub lumen_output_lm: f64,
    /// Power, W. The connected load is the sum of these.
    pub power_w: f64,
    /// Name of the photometric file (IES LM-63, EULUMDAT). A reference only: the file is not read.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub photometric_file: Option<String>,
    /// Luminous intensity class as entered, e.g. "G4" (TD 501 clause 6.54 item 2).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub luminous_intensity_class: Option<String>,
    /// Correlated colour temperature, K (TD 501 clause 6.54 item 3).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cct_k: Option<f64>,
    /// Colour rendering index (TD 501 clause 6.54 item 4).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cri: Option<f64>,
    /// Whether the light source is LED (TD 501 clause 3.14).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub led: Option<bool>,
    /// Photobiological risk group as entered, e.g. "RG0" (TD 501 clauses 1.4 and 3.15).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub risk_group: Option<String>,
    /// Mass, kg (TD 501 clause 6.53 item 4).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub mass_kg: Option<f64>,
    /// Windage area, m² (TD 501 clause 6.53 item 5).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub windage_m2: Option<f64>,
}

/// The column, bracket and luminaire shared by the columns of an installation.
#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
pub struct ColumnSpec {
    /// Column type, free text (material, shape, passive safety class...). Not interpreted.
    #[serde(default)]
    pub column_type: String,
    /// Height of the luminaire above the level at the column, m.
    pub mounting_height_m: f64,
    /// Horizontal distance from the column axis to the luminaire, m, towards the carriageway.
    #[serde(default)]
    pub bracket_outreach_m: f64,
    /// Tilt of the luminaire above horizontal, degrees.
    #[serde(default)]
    pub tilt_deg: f64,
    pub luminaire: Luminaire,
}

impl ColumnSpec {
    pub(crate) fn notes(&self, out: &mut Vec<Note>) {
        let err = |code: &str, m: String| note(Severity::Error, code, m);
        if !positive(self.mounting_height_m) {
            out.push(err("LIGHTING_MOUNTING_HEIGHT_NOT_POSITIVE", format!("The mounting height must be greater than zero (it is {}).", self.mounting_height_m)));
        }
        if !not_negative(self.bracket_outreach_m) {
            out.push(err("LIGHTING_OUTREACH_NEGATIVE", format!("The bracket outreach must not be negative (it is {}).", self.bracket_outreach_m)));
        }
        if !(self.tilt_deg.is_finite() && self.tilt_deg.abs() < 90.0) {
            out.push(err("LIGHTING_TILT_OUT_OF_RANGE", format!("The tilt must be between -90° and 90° (it is {}).", self.tilt_deg)));
        }
        if !not_negative(self.luminaire.power_w) {
            out.push(err("LIGHTING_POWER_NEGATIVE", format!("The luminaire power must not be negative (it is {}).", self.luminaire.power_w)));
        }
        if !not_negative(self.luminaire.lumen_output_lm) {
            out.push(err("LIGHTING_LUMENS_NEGATIVE", format!("The lumen output must not be negative (it is {}).", self.luminaire.lumen_output_lm)));
        }
        if self.luminaire.product.trim().is_empty() {
            out.push(note(Severity::Warning, "LIGHTING_NO_PRODUCT", "No luminaire product is entered; the schedule will show it blank.".to_string()));
        }
    }
}

/// A run of columns along an alignment.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Installation {
    /// The unique reference of the installation; one installation is one road lighting zone in
    /// the sense of TD 501 clauses 6.47 to 6.49 because it has one lighting class.
    pub reference: String,
    /// Name of the alignment the stations are on.
    pub alignment: String,
    /// Internal station of the first column, m.
    pub start_station_m: f64,
    /// Internal station where the installation ends, m.
    pub end_station_m: f64,
    pub arrangement: Arrangement,
    /// Distance along the alignment between consecutive columns, m (see [`Arrangement::Staggered`]).
    /// Measured on the alignment, not along the column line: on a curve the columns on the
    /// inside are closer together than this and those on the outside further apart.
    pub spacing_m: f64,
    #[serde(default)]
    pub end_treatment: EndTreatment,
    /// For `Staggered`: the side of the first column.
    #[serde(default = "default_first_side")]
    pub first_side: Side,
    /// Width of the carriageway between its edges, m, taken as symmetrical about the reference
    /// line. For `TwinCentral` it is the width of one carriageway and does not position anything.
    pub carriageway_width_m: f64,
    /// Offset of the reference line (the carriageway centre; for `TwinCentral` the line the
    /// columns stand on) from the alignment, m, positive to the right. Zero when the alignment is
    /// the carriageway centreline.
    #[serde(default)]
    pub reference_offset_m: f64,
    /// Distance from the carriageway edge to the column axis, m. For `TwinCentral`, the distance
    /// from the column axis to the near edge of each carriageway; it does not position anything.
    pub setback_m: f64,
    pub column: ColumnSpec,
    /// Lighting class as a label, e.g. "M3" or "C2". Not interpreted: this crate holds no class
    /// table.
    #[serde(default)]
    pub lighting_class: String,
    /// Marks are this prefix, a hyphen and a three-digit number counted along the installation.
    #[serde(default = "default_mark_prefix")]
    pub mark_prefix: String,
    /// Number of the first mark.
    #[serde(default = "default_first_number")]
    pub first_number: u32,
}

fn default_first_side() -> Side {
    Side::Left
}

fn default_mark_prefix() -> String {
    "LC".to_string()
}

fn default_first_number() -> u32 {
    1
}

impl Installation {
    /// Offset of the column axis from the alignment on `side`, m, positive right.
    pub fn column_offset_m(&self, side: Side) -> f64 {
        let half = self.carriageway_width_m / 2.0 + self.setback_m;
        match side {
            Side::Left => self.reference_offset_m - half,
            Side::Right => self.reference_offset_m + half,
            Side::Central => self.reference_offset_m,
        }
    }

    /// How far the luminaire stands over the carriageway, m: outreach less set-back. Negative
    /// when it stays behind the carriageway edge.
    pub fn overhang_m(&self) -> f64 {
        self.column.bracket_outreach_m - self.setback_m
    }

    /// Validation notes. Error notes mean the installation cannot be laid out.
    pub fn notes(&self) -> Vec<Note> {
        let err = |code: &str, m: String| note(Severity::Error, code, m);
        let mut out = Vec::new();
        if self.reference.trim().is_empty() {
            out.push(note(Severity::Warning, "LIGHTING_NO_REFERENCE", "The installation has no reference.".to_string()));
        }
        if !(self.start_station_m.is_finite() && self.end_station_m.is_finite()) {
            out.push(err("LIGHTING_STATION_NOT_FINITE", "The start and end stations must be numbers.".to_string()));
        } else if self.end_station_m <= self.start_station_m {
            out.push(err(
                "LIGHTING_END_NOT_AFTER_START",
                format!("The end station ({}) must be greater than the start station ({}).", self.end_station_m, self.start_station_m),
            ));
        }
        if !positive(self.spacing_m) {
            out.push(err("LIGHTING_SPACING_NOT_POSITIVE", format!("The spacing must be greater than zero (it is {}).", self.spacing_m)));
        }
        if !positive(self.carriageway_width_m) {
            out.push(err("LIGHTING_WIDTH_NOT_POSITIVE", format!("The carriageway width must be greater than zero (it is {}).", self.carriageway_width_m)));
        }
        if !not_negative(self.setback_m) {
            out.push(err("LIGHTING_SETBACK_NEGATIVE", format!("The set-back must not be negative (it is {}).", self.setback_m)));
        }
        if !self.reference_offset_m.is_finite() {
            out.push(err("LIGHTING_OFFSET_NOT_FINITE", "The reference line offset must be a number.".to_string()));
        }
        if self.arrangement == Arrangement::Staggered && self.first_side == Side::Central {
            out.push(err("LIGHTING_STAGGER_SIDE", "A staggered arrangement starts on the left or the right.".to_string()));
        }
        self.column.notes(&mut out);
        if self.lighting_class.trim().is_empty() {
            out.push(note(Severity::Warning, "LIGHTING_NO_CLASS", "No lighting class is entered (TD 501 clause 3.6 takes it from BS 5489-1).".to_string()));
        }
        out
    }

    pub fn check(&self) -> Result<(), LightingError> {
        error_text(&self.notes()).map_or(Ok(()), |m| Err(LightingError::InvalidDefinition(m)))
    }
}

/// Where the bracket of a single column points.
#[derive(Debug, Clone, Copy, PartialEq, Default, Serialize, Deserialize)]
pub enum BracketAim {
    /// Square to the alignment, towards it. A column standing on the alignment (offset zero)
    /// gets two luminaires, one to each side.
    #[default]
    TowardAlignment,
    /// A whole-circle bearing in degrees clockwise from north.
    Bearing(f64),
}

/// One column placed on its own at a station and offset.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SingleColumn {
    /// The column's mark.
    pub mark: String,
    pub alignment: String,
    /// Internal station, m.
    pub station_m: f64,
    /// Offset of the column axis from the alignment, m, positive to the right.
    pub offset_m: f64,
    #[serde(default)]
    pub aim: BracketAim,
    pub column: ColumnSpec,
    #[serde(default)]
    pub lighting_class: String,
}

impl SingleColumn {
    pub fn notes(&self) -> Vec<Note> {
        let mut out = Vec::new();
        if self.mark.trim().is_empty() {
            out.push(note(Severity::Warning, "LIGHTING_NO_REFERENCE", "The column has no mark.".to_string()));
        }
        if !(self.station_m.is_finite() && self.offset_m.is_finite()) {
            out.push(note(Severity::Error, "LIGHTING_STATION_NOT_FINITE", "The station and offset must be numbers.".to_string()));
        }
        if let BracketAim::Bearing(b) = self.aim {
            if !b.is_finite() {
                out.push(note(Severity::Error, "LIGHTING_BEARING_NOT_FINITE", "The bracket bearing must be a number.".to_string()));
            }
        }
        self.column.notes(&mut out);
        out
    }

    pub fn check(&self) -> Result<(), LightingError> {
        error_text(&self.notes()).map_or(Ok(()), |m| Err(LightingError::InvalidDefinition(m)))
    }
}

/// What a lighting element is.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum LightingDefinition {
    Installation(Installation),
    Column(SingleColumn),
}

impl LightingDefinition {
    pub fn column(&self) -> &ColumnSpec {
        match self {
            LightingDefinition::Installation(i) => &i.column,
            LightingDefinition::Column(c) => &c.column,
        }
    }

    pub fn reference(&self) -> &str {
        match self {
            LightingDefinition::Installation(i) => &i.reference,
            LightingDefinition::Column(c) => &c.mark,
        }
    }

    pub fn alignment(&self) -> &str {
        match self {
            LightingDefinition::Installation(i) => &i.alignment,
            LightingDefinition::Column(c) => &c.alignment,
        }
    }

    pub fn lighting_class(&self) -> &str {
        match self {
            LightingDefinition::Installation(i) => &i.lighting_class,
            LightingDefinition::Column(c) => &c.lighting_class,
        }
    }

    /// The arrangement label used in the quantities: that of the installation, or "single column".
    pub fn arrangement_label(&self) -> &'static str {
        match self {
            LightingDefinition::Installation(i) => i.arrangement.label(),
            LightingDefinition::Column(_) => "single column",
        }
    }

    pub fn notes(&self) -> Vec<Note> {
        match self {
            LightingDefinition::Installation(i) => i.notes(),
            LightingDefinition::Column(c) => c.notes(),
        }
    }
}

/// The content of a lighting element's `data`: the definition and the positions computed from
/// the alignment when the element was added or last refreshed.
///
/// The positions are a stored result, because a kernel module sees only its own element and
/// cannot read the alignment. Call [`crate::refresh_layout`] after the alignment or the
/// definition changes.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct LightingData {
    pub definition: LightingDefinition,
    pub layout: Layout,
}

impl LightingData {
    pub fn from_value(data: &serde_json::Value) -> Result<Self, LightingError> {
        serde_json::from_value(data.clone()).map_err(|e| LightingError::InvalidDefinition(format!("the lighting definition cannot be read: {e}")))
    }

    pub fn to_value(&self) -> serde_json::Value {
        serde_json::to_value(self).expect("a lighting definition always serialises")
    }

    /// Number of luminaires on all columns.
    pub fn luminaire_count(&self) -> usize {
        self.layout.columns.iter().map(|c| c.bracket_directions_rad.len()).sum()
    }

    /// Sum of the entered luminaire powers, W.
    pub fn connected_load_w(&self) -> f64 {
        self.luminaire_count() as f64 * self.definition.column().luminaire.power_w
    }
}
