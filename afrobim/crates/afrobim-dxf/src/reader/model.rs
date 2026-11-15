//! What the reader keeps of a drawing, in the file's own units.

use std::collections::{BTreeMap, BTreeSet};

use afrobim_draw::Pt2;

/// A vertex of a polyline: the bulge belongs to the segment from this vertex to the next.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Vertex {
    pub p: Pt2,
    pub bulge: f64,
}

/// How an entity is coloured.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum ColourSpec {
    ByLayer,
    ByBlock,
    /// AutoCAD Colour Index 1 to 255.
    Aci(u8),
    Rgb([u8; 3]),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HAlign {
    Left,
    Centre,
    Right,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum VAlign {
    Baseline,
    Bottom,
    Middle,
    Top,
}

/// A text, TEXT or MTEXT, as plain text and where it goes. The draw crate has no text primitive,
/// so the reader hands these over as labels.
#[derive(Debug, Clone, PartialEq)]
pub struct TextGeom {
    pub at: Pt2,
    pub height: f64,
    pub rotation_deg: f64,
    pub width_factor: f64,
    /// Plain text; lines separated by `\n`.
    pub text: String,
    pub h: HAlign,
    pub v: VAlign,
    /// For MTEXT, the reference rectangle width (0 when unset).
    pub box_width: f64,
}

/// The geometry of one entity. Points are in WCS (plan only: Z is dropped), file units.
#[derive(Debug, Clone, PartialEq)]
pub enum Geom {
    Line(Pt2, Pt2),
    /// LWPOLYLINE and 2D POLYLINE, with bulges.
    Poly { pts: Vec<Vertex>, closed: bool },
    /// Anticlockwise from `a0` to `a1`, degrees.
    Arc { c: Pt2, r: f64, a0: f64, a1: f64 },
    Circle { c: Pt2, r: f64 },
    /// `centre + cos(t) major + sin(t) minor`, t from `t0` to `t1` radians.
    Ellipse { c: Pt2, major: Pt2, minor: Pt2, t0: f64, t1: f64 },
    Spline { degree: usize, knots: Vec<f64>, ctrl: Vec<Pt2>, weights: Vec<f64>, fit: Vec<Pt2>, closed: bool },
    Text(TextGeom),
    Point(Pt2),
    Insert { block: String, at: Pt2, sx: f64, sy: f64, rot_deg: f64, cols: u32, rows: u32, dx: f64, dy: f64 },
    /// SOLID and 3DFACE outlines, in drawing order.
    Solid(Vec<Pt2>),
    /// HATCH boundary loops, each a list of edges (Line, Arc, Ellipse, Spline or Poly).
    Hatch(Vec<Vec<Geom>>),
}

#[derive(Debug, Clone, PartialEq)]
pub struct Entity {
    /// Index into [`Underlay::layer_names`].
    pub layer: u32,
    pub colour: ColourSpec,
    pub geom: Geom,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Block {
    pub name: String,
    pub base: Pt2,
    pub entities: Vec<Entity>,
}

/// One row of the LAYER table.
#[derive(Debug, Clone, PartialEq)]
pub struct LayerInfo {
    pub name: String,
    /// AutoCAD Colour Index (always positive; see `off`), or `None` when the layer has a true colour.
    pub aci: Option<u8>,
    pub rgb: Option<[u8; 3]>,
    pub frozen: bool,
    pub locked: bool,
    /// Switched off: the colour number is negative in the file.
    pub off: bool,
}

impl LayerInfo {
    pub fn plain(name: &str) -> LayerInfo {
        LayerInfo { name: name.to_string(), aci: Some(7), rgb: None, frozen: false, locked: false, off: false }
    }

    /// Visible by the file's own settings.
    pub fn visible(&self) -> bool {
        !self.frozen && !self.off
    }
}

/// Units declared by the file.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct UnitsInfo {
    /// `$INSUNITS`, when present.
    pub insunits: Option<i32>,
    /// `$MEASUREMENT`: 0 English, 1 metric, when present.
    pub measurement: Option<i32>,
    /// Name of the unit `$INSUNITS` names.
    pub name: String,
    /// Millimetres per drawing unit proposed from the header.
    pub mm_per_unit: f64,
    /// True when the header does not say (unitless or absent) and `mm_per_unit` is an assumption.
    pub assumed: bool,
}

/// What was and was not read.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct ReadReport {
    pub dxf_version: Option<String>,
    /// Entities read, by DXF type (model space and blocks).
    pub read: BTreeMap<String, usize>,
    /// Entities not read (or read only in part), by DXF type, with the count.
    pub unread: BTreeMap<String, usize>,
    pub paper_space_skipped: usize,
    pub warnings: Vec<String>,
    /// MTEXT inline codes that were removed from text, by code (for example `\H`).
    pub mtext_stripped: BTreeSet<String>,
    pub records: usize,
}

impl ReadReport {
    pub fn count_read(&mut self, kind: &str) {
        *self.read.entry(kind.to_string()).or_default() += 1;
    }

    pub fn count_unread(&mut self, kind: &str) {
        *self.unread.entry(kind.to_string()).or_default() += 1;
    }

    pub fn warn(&mut self, msg: impl Into<String>) {
        let m = msg.into();
        if self.warnings.len() < 200 && !self.warnings.contains(&m) {
            self.warnings.push(m);
        }
    }

    pub fn total_read(&self) -> usize {
        self.read.values().sum()
    }

    pub fn total_unread(&self) -> usize {
        self.unread.values().sum()
    }
}
