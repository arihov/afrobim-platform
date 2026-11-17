//! A DXF reader for attaching a drawing as a read-only underlay.
//!
//! Reads ASCII DXF from R12 through the current release as AutoCAD and Civil 3D write it, keeps
//! the geometry by layer in the file's own units, and produces an [`afrobim_draw::Drawing`] for
//! display plus snap points for object snap. Binary DXF is refused (see [`ReadError`]). A DWG
//! given to [`read_file`] is converted to DXF with ODA File Converter when it is installed, and
//! refused with what to do when it is not (see [`dwg`]). Nothing here edits or writes a DXF or a
//! DWG; the writer in the crate root is untouched.
//!
//! ```no_run
//! use afrobim_dxf::reader::{read_file, DisplayOptions, SnapOptions};
//!
//! let mut under = read_file(std::path::Path::new("site.dxf"))?;
//! println!("{} entities read, {} not read", under.report.total_read(), under.report.total_unread());
//! // Units from the header, overridable:
//! under.unit_scale_mm = 1000.0; // the file is in metres
//! let view = under.to_drawing(&DisplayOptions::default());
//! let snaps = under.snap_points(&DisplayOptions::default(), &SnapOptions::default());
//! # let _ = (view, snaps);
//! # Ok::<(), afrobim_dxf::reader::ReadError>(())
//! ```
//!
//! What is read: LINE, LWPOLYLINE, POLYLINE (2D and 3D plan, bulges), ARC, CIRCLE, ELLIPSE, SPLINE,
//! TEXT, MTEXT, ATTRIB, POINT, INSERT (scale, rotation, arrays, nesting, layer 0 and by-block
//! inheritance), DIMENSION (through its drawn block), SOLID, TRACE, 3DFACE, HATCH boundaries, the
//! LAYER table and the header units and extents, and the drawn form (proxy graphics) of
//! ACAD_PROXY_ENTITY, which is how a converter writes objects of classes it does not know, such
//! as Civil 3D's (see [`proxy`]). Everything else is counted by type in [`ReadReport::unread`].

mod aci;
pub mod dwg;
mod expand;
mod geom;
mod model;
mod mtext;
mod parse;
pub mod proxy;
mod scan;
mod snap;
mod units;

use std::collections::BTreeMap;
use std::io::{BufRead, BufReader, Read};
use std::path::Path;

use afrobim_draw::{Colour, Drawing, Extents, Primitive, Pt2};

pub use aci::ACI_RGB;
pub use expand::{aci_rgb, Affine, ExpandOptions, Expanded, Item, Resolved, Shape, Snap, SnapKind};
pub use geom::{arc_points, bulge_arc, bulge_included_angle_deg, ocs_axes, sweep, BulgeArc, Nurbs};
pub use model::*;
pub use mtext::{decode_text, strip_mtext};
pub use snap::{SnapOptions, SnapSet};
pub use units::insunits;

/// Why a file was not read.
#[derive(Debug, thiserror::Error)]
pub enum ReadError {
    #[error("{0}")]
    Io(#[from] std::io::Error),
    #[error("This is a binary DXF file, which this program cannot read. Save it as an ASCII DXF from the CAD program and attach that.")]
    Binary,
    #[error("This is a DWG file ({0}). {hint}", hint = dwg::install_hint())]
    Dwg(String),
    #[error("{0}")]
    DwgConversion(#[from] dwg::DwgError),
    #[error("{0}")]
    NotDxf(String),
}

/// The drawing as read, in file units.
#[derive(Debug, Clone, PartialEq)]
pub struct Underlay {
    /// The layers, in table order; entities refer to them by index.
    pub layers: Vec<LayerInfo>,
    pub units: UnitsInfo,
    /// Millimetres per drawing unit used for display. Starts at the value proposed from the
    /// header (`units.mm_per_unit`); set it to override.
    pub unit_scale_mm: f64,
    /// `$EXTMIN` and `$EXTMAX` as the file states them, in file units.
    pub header_extents: Option<(Pt2, Pt2)>,
    /// Bounds of the geometry actually read (blocks expanded, text anchors included), in file units.
    pub bounds: Option<(Pt2, Pt2)>,
    pub blocks: BTreeMap<String, Block>,
    /// Model space entities, in file order.
    pub entities: Vec<Entity>,
    pub report: ReadReport,
}

/// Reads an ASCII DXF file, or a DWG through ODA File Converter (see [`dwg`]). Binary DXF, and a
/// DWG when there is no converter, are refused with a message that says what to do.
pub fn read_file(path: &Path) -> Result<Underlay, ReadError> {
    let mut f = std::fs::File::open(path)?;
    let mut head = [0u8; 6];
    let n = f.read(&mut head)?;
    if dwg::is_dwg(&head[..n]) {
        let code = String::from_utf8_lossy(&head).to_string();
        let program = dwg::find_converter().ok_or_else(|| ReadError::Dwg(format!("version {code}, {}", dwg_name(&code))))?;
        let dxf = dwg::convert_with(&program, path, &dwg::default_cache_root())?;
        let mut under = read(BufReader::with_capacity(1 << 20, std::fs::File::open(&dxf)?))?;
        under.report.warn(format!("Converted from DWG ({}, {}) with ODA File Converter.", code, dwg_name(&code)));
        return Ok(under);
    }
    drop(f);
    let f = std::fs::File::open(path)?;
    read(BufReader::with_capacity(1 << 20, f))
}

/// Reads an ASCII DXF from memory.
pub fn read_bytes(bytes: &[u8]) -> Result<Underlay, ReadError> {
    read(bytes)
}

/// Reads an ASCII DXF from any buffered reader.
pub fn read<R: BufRead>(mut r: R) -> Result<Underlay, ReadError> {
    {
        let head = r.fill_buf()?;
        if head.starts_with(b"AutoCAD Binary DXF") {
            return Err(ReadError::Binary);
        }
        if head.len() >= 6 && head.starts_with(b"AC10") && head[4..6].iter().all(u8::is_ascii_digit) {
            let code = String::from_utf8_lossy(&head[..6]).to_string();
            return Err(ReadError::Dwg(format!("version {code}, {}", dwg_name(&code))));
        }
        // Skip a UTF-8 byte order mark.
        if head.starts_with(&[0xEF, 0xBB, 0xBF]) {
            r.consume(3);
        }
    }
    parse::read_records(r)
}

/// Reads from anything `Read` (wraps it in a buffer).
pub fn read_from<R: Read>(r: R) -> Result<Underlay, ReadError> {
    read(BufReader::with_capacity(1 << 20, r))
}

fn dwg_name(code: &str) -> &'static str {
    // Release names as `ezdxf.lldxf.const.acad_release` gives them for these version tags.
    match code {
        "AC1004" => "release R9",
        "AC1006" => "release R10",
        "AC1009" => "release R12",
        "AC1012" => "release R13",
        "AC1014" => "release R14",
        "AC1015" => "release R2000",
        "AC1018" => "release R2004",
        "AC1021" => "release R2007",
        "AC1024" => "release R2010",
        "AC1027" => "release R2013",
        "AC1032" => "release R2018",
        _ => "an unrecognised release",
    }
}

/// How to turn an [`Underlay`] into a drawing.
#[derive(Debug, Clone)]
pub struct DisplayOptions {
    /// Millimetres per file unit; `None` uses [`Underlay::unit_scale_mm`].
    pub unit_scale_mm: Option<f64>,
    /// Added to every point after scaling, millimetres (places the underlay in the model).
    pub offset_mm: Pt2,
    /// Greatest distance between a drawn chord and the true curve, millimetres. Default 5, which
    /// is far below a pixel at site scales; a front end should pass a value from its zoom.
    pub tolerance_mm: f64,
    /// Nesting limit for blocks. Default 16.
    pub max_depth: usize,
    /// Layer names to leave out, in addition to the file's frozen and off layers.
    pub hidden_layers: Vec<String>,
    /// Draw layers the file freezes or switches off. Default false.
    pub show_file_hidden_layers: bool,
    /// Stop after this many items (guards against huge arrays). Default 5 000 000.
    pub max_items: usize,
}

impl Default for DisplayOptions {
    fn default() -> Self {
        DisplayOptions { unit_scale_mm: None, offset_mm: Pt2::new(0.0, 0.0), tolerance_mm: 5.0, max_depth: 16, hidden_layers: Vec::new(), show_file_hidden_layers: false, max_items: 5_000_000 }
    }
}

/// A text of the underlay for the front end to draw, in model millimetres.
#[derive(Debug, Clone, PartialEq)]
pub struct UnderlayLabel {
    pub at: Pt2,
    /// Plain text, lines separated by `\n`.
    pub text: String,
    pub height_mm: f64,
    pub rotation_deg: f64,
    pub width_factor: f64,
    pub h: HAlign,
    pub v: VAlign,
    pub layer: String,
    /// The text's colour as RGB.
    pub rgb: [u8; 3],
}

/// The drawing of an underlay.
#[derive(Debug, Clone, PartialEq)]
pub struct UnderlayDrawing {
    pub drawing: Drawing,
    pub labels: Vec<UnderlayLabel>,
    pub warnings: Vec<String>,
    pub items: usize,
    /// True when `max_items` stopped the expansion.
    pub truncated: bool,
}

impl Underlay {
    fn scale(&self, o: &DisplayOptions) -> f64 {
        o.unit_scale_mm.unwrap_or(self.unit_scale_mm)
    }

    fn expand_options(&self, o: &DisplayOptions, snaps: bool) -> ExpandOptions {
        let k = self.scale(o);
        let hidden = o
            .hidden_layers
            .iter()
            .filter_map(|n| self.layers.iter().position(|l| l.name.eq_ignore_ascii_case(n)))
            .map(|i| i as u32)
            .collect();
        ExpandOptions { tol: o.tolerance_mm / k.max(1e-12), max_depth: o.max_depth, hidden, honour_file_layers: !o.show_file_hidden_layers, max_items: o.max_items, snaps }
    }

    /// The index of a layer by name, ignoring case.
    pub fn layer_index(&self, name: &str) -> Option<usize> {
        self.layers.iter().position(|l| l.name.eq_ignore_ascii_case(name))
    }

    /// The model-space geometry as polylines, in file units, for comparison and tests: each item
    /// as (DXF type, layer, points, closed). Arcs and circles are flattened to `tol` file units.
    pub fn flatten(&self, tol: f64) -> Vec<(String, String, Vec<Pt2>, bool)> {
        let ex = self.expand(&ExpandOptions { tol, ..ExpandOptions::default() });
        ex.items
            .iter()
            .map(|it| {
                let (pts, closed) = match &it.shape {
                    Shape::Line(a, b) => (vec![*a, *b], false),
                    Shape::Arc { c, r, a0, a1 } => (geom::arc_points(*c, *r, *a0, geom::sweep(*a0, *a1), tol), false),
                    Shape::Circle { c, r } => (geom::arc_points(*c, *r, 0.0, 360.0, tol), true),
                    Shape::Poly { pts, closed } => (pts.clone(), *closed),
                    Shape::Solid(p) => (p.clone(), true),
                    Shape::Point(p) => (vec![*p], false),
                    Shape::Text(t) => (vec![t.at], false),
                };
                (it.kind.to_string(), self.layers[it.layer as usize].name.clone(), pts, closed)
            })
            .collect()
    }

    /// The drawing for display: millimetres, with lines as hairlines of the entity's colour
    /// snapped to the nearest named colour (white and black become `None`, the front end's ink),
    /// arcs and circles as `Arc` primitives, solids as `Fill`. Texts come back as labels.
    pub fn to_drawing(&self, o: &DisplayOptions) -> UnderlayDrawing {
        let k = self.scale(o);
        let ex = self.expand(&self.expand_options(o, false));
        let tx = |p: Pt2| Pt2::new(p.x * k + o.offset_mm.x, p.y * k + o.offset_mm.y);
        let mut prims: Vec<Primitive> = Vec::with_capacity(ex.items.len());
        let mut labels = Vec::new();
        let (mut lo, mut hi) = (Pt2::new(f64::INFINITY, f64::INFINITY), Pt2::new(f64::NEG_INFINITY, f64::NEG_INFINITY));
        let mut ext = |p: Pt2| {
            lo = Pt2::new(lo.x.min(p.x), lo.y.min(p.y));
            hi = Pt2::new(hi.x.max(p.x), hi.y.max(p.y));
        };
        for it in &ex.items {
            let colour = ink_colour(it.colour.rgb);
            match &it.shape {
                Shape::Line(a, b) => {
                    let (a, b) = (tx(*a), tx(*b));
                    ext(a);
                    ext(b);
                    prims.push(Primitive::Line { from: a, to: b, width: 0.0, colour });
                }
                Shape::Poly { pts, closed } => {
                    let q: Vec<Pt2> = pts.iter().map(|p| tx(*p)).collect();
                    q.iter().for_each(|p| ext(*p));
                    let n = q.len();
                    for i in 0..if *closed { n } else { n.saturating_sub(1) } {
                        prims.push(Primitive::Line { from: q[i], to: q[(i + 1) % n], width: 0.0, colour });
                    }
                }
                Shape::Arc { c, r, a0, a1 } => {
                    let (cc, rr) = (tx(*c), r * k);
                    ext(Pt2::new(cc.x - rr, cc.y - rr));
                    ext(Pt2::new(cc.x + rr, cc.y + rr));
                    prims.push(Primitive::Arc { centre: cc, radius: rr, start_deg: *a0, end_deg: *a1, colour });
                }
                Shape::Circle { c, r } => {
                    let (cc, rr) = (tx(*c), r * k);
                    ext(Pt2::new(cc.x - rr, cc.y - rr));
                    ext(Pt2::new(cc.x + rr, cc.y + rr));
                    prims.push(Primitive::Arc { centre: cc, radius: rr, start_deg: 0.0, end_deg: 360.0, colour });
                }
                Shape::Solid(pts) => {
                    let q: Vec<Pt2> = pts.iter().map(|p| tx(*p)).collect();
                    q.iter().for_each(|p| ext(*p));
                    prims.push(Primitive::Fill { outline: q, colour: colour.unwrap_or(Colour::Black) });
                }
                Shape::Point(p) => {
                    // A small cross, 1 mm each way.
                    let p = tx(*p);
                    ext(p);
                    prims.push(Primitive::Line { from: Pt2::new(p.x - 1.0, p.y), to: Pt2::new(p.x + 1.0, p.y), width: 0.0, colour });
                    prims.push(Primitive::Line { from: Pt2::new(p.x, p.y - 1.0), to: Pt2::new(p.x, p.y + 1.0), width: 0.0, colour });
                }
                Shape::Text(t) => {
                    let at = tx(t.at);
                    ext(at);
                    labels.push(UnderlayLabel {
                        at,
                        text: t.text.clone(),
                        height_mm: t.height * k,
                        rotation_deg: t.rotation_deg,
                        width_factor: t.width_factor,
                        h: t.h,
                        v: t.v,
                        layer: self.layers[it.layer as usize].name.clone(),
                        rgb: it.colour.rgb,
                    });
                }
            }
        }
        let extents = if lo.x.is_finite() { Extents { min: lo, max: hi } } else { Extents::default() };
        UnderlayDrawing { drawing: Drawing { primitives: prims, extents }, labels, warnings: ex.warnings, items: ex.items.len(), truncated: ex.truncated }
    }

    /// Snap points in model millimetres: endpoints, midpoints, centres, quadrants, nodes and
    /// intersections of straight segments, within the budgets of `snap`.
    pub fn snap_points(&self, display: &DisplayOptions, snap: &SnapOptions) -> SnapSet {
        let k = self.scale(display);
        let ex = self.expand(&self.expand_options(display, true));
        snap::collect(&ex, k, display.offset_mm, snap)
    }
}

/// The nearest named colour; white and black (and unknown) become `None`, the ink colour.
fn ink_colour(rgb: [u8; 3]) -> Option<Colour> {
    const ALL: [Colour; 8] = [Colour::White, Colour::Black, Colour::Green, Colour::Blue, Colour::Brown, Colour::Yellow, Colour::Red, Colour::Grey];
    let d = |c: Colour| {
        let r = c.display_rgb();
        (0..3).map(|i| (r[i] as i32 - rgb[i] as i32).pow(2)).sum::<i32>()
    };
    let best = ALL.into_iter().min_by_key(|c| d(*c)).unwrap_or(Colour::Black);
    // Plain white, black and the very dark and very light greys are the front end's ink.
    let lum = rgb.iter().map(|c| *c as u32).sum::<u32>() / 3;
    if matches!(best, Colour::White | Colour::Black) || !(40..=235).contains(&lum) {
        None
    } else {
        Some(best)
    }
}
