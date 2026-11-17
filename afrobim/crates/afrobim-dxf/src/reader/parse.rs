//! Records to entities.
//!
//! Group codes below were checked against `ezdxf` 1.4.5 (`DXFATTRIBS` of each entity class, the
//! loaders in `ezdxf/entities/boundary_paths.py` and `lwpolyline.py`, and the constants in
//! `ezdxf/lldxf/const.py`). The Autodesk DXF Reference pages on help.autodesk.com are rendered by
//! script and could not be fetched here, so they are not cited as verified.
//!
//! | entity | groups read |
//! |---|---|
//! | all | 8 layer, 62 colour (0 by block, 256 by layer), 420 true colour, 60 invisible, 67 paper space, 210/220/230 extrusion |
//! | LINE | 10 start, 11 end |
//! | CIRCLE | 10 centre, 40 radius, 38 elevation (via the OCS) |
//! | ARC | 10 centre, 40 radius, 50 start angle, 51 end angle (degrees, anticlockwise) |
//! | ELLIPSE | 10 centre, 11 major axis end point relative to the centre, 40 minor to major ratio, 41 and 42 start and end parameter (radians) |
//! | LWPOLYLINE | 70 flags (1 closed), 38 elevation, then per vertex 10 x, 20 y, 42 bulge |
//! | POLYLINE | 70 flags (1 closed, 2 curve fit, 4 spline fit, 8 3D, 16 mesh, 64 polyface), 66 vertices follow; VERTEX 10/20/30, 42 bulge, 70 flags (8 spline vertex, 16 spline frame point, 128 polyface) until SEQEND |
//! | SPLINE | 70 flags (1 closed, 2 periodic, 4 rational), 71 degree, 72 knot count, 73 control point count, 74 fit point count, 40 knots, 41 weights, 10 control points, 11 fit points |
//! | TEXT | 10 insert, 40 height, 1 text, 50 rotation, 41 width factor, 72 horizontal alignment (0 left, 1 centre, 2 right, 3 aligned, 4 middle, 5 fit), 73 vertical alignment (0 baseline, 1 bottom, 2 middle, 3 top), 11 alignment point |
//! | MTEXT | 10 insert, 40 height, 41 width, 71 attachment point (1 to 9), 11 direction vector, 50 rotation, 1 text and 3 earlier 250 character chunks |
//! | POINT | 10 |
//! | INSERT | 2 block name, 10 insert, 41/42 scale, 50 rotation, 70 columns, 71 rows, 44 column spacing, 45 row spacing, 66 attributes follow |
//! | DIMENSION | 2 name of the anonymous block that holds its drawn form, which is drawn as an INSERT at the origin |
//! | SOLID, TRACE | 10 to 13 corners (the file order of the last two is swapped against the outline) |
//! | 3DFACE | 10 to 13 corners (in outline order) |
//! | HATCH | 91 path count; per path 92 flags (2 polyline); polyline path: 72 has bulge, 73 closed, 93 vertices, 10/20 and 42; edge path: 93 edges, 72 edge type (1 line, 2 arc, 3 ellipse, 4 spline) with 10/11, 40, 50/51, 73, and for splines 94, 73, 74, 95, 96, 40, 10, 42; 97 source object count ends a path |
//! | LAYER | 2 name, 70 flags (1 frozen, 4 locked), 62 colour (negative: off), 420 true colour |
//! | BLOCK | 2 name, 10 base point; entities up to ENDBLK |
//! | ACAD_PROXY_ENTITY | 92 or 160 graphics size and the 310 chunks after it (see `proxy.rs`) |
//! | header | $ACADVER (1), $INSUNITS (70), $MEASUREMENT (70), $EXTMIN, $EXTMAX (10, 20) |

use std::collections::HashMap;
use std::io::BufRead;

use afrobim_draw::Pt2;

use super::geom::{is_minus_z, is_plain_z, ocs_to_wcs, pt};
use super::model::*;
use super::mtext::{decode_text, strip_mtext};
use super::scan::{Record, Records, View};
use super::units::units_from_header;
use super::{ReadError, Underlay};

struct PolyAcc {
    layer: u32,
    colour: ColourSpec,
    flags: i64,
    extrusion: Option<(f64, f64, f64)>,
    verts: Vec<(Pt2, f64, i64)>,
}

struct Builder {
    layers: Vec<LayerInfo>,
    layer_ix: HashMap<String, u32>,
    model: Vec<Entity>,
    blocks: std::collections::BTreeMap<String, Block>,
    cur_block: Option<Block>,
    skip_block: bool,
    poly: Option<PolyAcc>,
    /// An INSERT with attributes: ATTRIB records that follow are its texts, up to SEQEND.
    in_attribs: bool,
    report: ReadReport,
    header: Vec<(String, Vec<(i32, String)>)>,
}

impl Builder {
    fn layer(&mut self, name: &str) -> u32 {
        if let Some(&i) = self.layer_ix.get(&name.to_ascii_uppercase()) {
            return i;
        }
        let i = self.layers.len() as u32;
        self.layers.push(LayerInfo::plain(name));
        self.layer_ix.insert(name.to_ascii_uppercase(), i);
        i
    }

    fn push(&mut self, e: Entity) {
        match &mut self.cur_block {
            Some(b) => b.entities.push(e),
            None => self.model.push(e),
        }
    }
}

fn colour_of(v: &View) -> ColourSpec {
    if let Some(rgb) = v.i(420) {
        return ColourSpec::Rgb([((rgb >> 16) & 255) as u8, ((rgb >> 8) & 255) as u8, (rgb & 255) as u8]);
    }
    match v.i(62) {
        None | Some(256) => ColourSpec::ByLayer,
        Some(0) => ColourSpec::ByBlock,
        Some(n) => ColourSpec::Aci(n.unsigned_abs().min(255) as u8),
    }
}

fn extrusion(v: &View) -> Option<(f64, f64, f64)> {
    let n = (v.f(210)?, v.f(220).unwrap_or(0.0), v.f(230).unwrap_or(1.0));
    Some(n)
}

/// Reads an ASCII DXF stream into an [`Underlay`].
pub fn read_records<R: BufRead>(reader: R) -> Result<Underlay, ReadError> {
    let mut recs = Records::new(reader);
    let mut b = Builder {
        layers: vec![LayerInfo::plain("0")],
        layer_ix: HashMap::from([("0".to_string(), 0)]),
        model: Vec::new(),
        blocks: Default::default(),
        cur_block: None,
        skip_block: false,
        poly: None,
        in_attribs: false,
        report: ReadReport::default(),
        header: Vec::new(),
    };
    let mut section = String::new();
    let mut table = String::new();
    let mut seen_section = false;
    while let Some(rec) = recs.next_record()? {
        b.report.records += 1;
        match rec.kind.as_str() {
            "SECTION" => {
                seen_section = true;
                section = View(&rec.pairs).s(2).unwrap_or("").trim().to_ascii_uppercase();
                if section == "HEADER" {
                    read_header(&rec, &mut b);
                }
            }
            "ENDSEC" => section.clear(),
            "EOF" => break,
            "TABLE" => table = View(&rec.pairs).s(2).unwrap_or("").trim().to_ascii_uppercase(),
            "ENDTAB" => table.clear(),
            _ => match section.as_str() {
                "TABLES" => {
                    if rec.kind == "LAYER" && table == "LAYER" {
                        read_layer(&rec, &mut b);
                    }
                }
                "BLOCKS" => match rec.kind.as_str() {
                    "BLOCK" => {
                        let v = View(&rec.pairs);
                        let name = v.s(2).unwrap_or("").to_string();
                        let up = name.to_ascii_uppercase();
                        b.skip_block = up.starts_with("*MODEL_SPACE") || up.starts_with("*PAPER_SPACE");
                        let base = v.xy(10).map(|(x, y)| pt(x, y)).unwrap_or(pt(0.0, 0.0));
                        b.cur_block = Some(Block { name, base, entities: Vec::new() });
                    }
                    "ENDBLK" => {
                        if let Some(bl) = b.cur_block.take() {
                            if !b.skip_block {
                                b.blocks.insert(bl.name.to_ascii_uppercase(), bl);
                            }
                        }
                        b.skip_block = false;
                    }
                    _ => {
                        if !b.skip_block {
                            entity(&rec, &mut b);
                        }
                    }
                },
                "ENTITIES" => entity(&rec, &mut b),
                _ => {}
            },
        }
    }
    if !seen_section {
        return Err(ReadError::NotDxf("the file has no SECTION: it is not a DXF".into()));
    }
    if b.poly.is_some() {
        b.report.warn("The file ends inside a POLYLINE (no SEQEND); the vertices read so far are kept.");
        finish_poly(&mut b);
    }
    for w in recs.warnings {
        b.report.warn(w);
    }
    // Header.
    let mut units = UnitsInfo::default();
    let mut extmin = None;
    let mut extmax = None;
    for (name, pairs) in &b.header {
        let v = View(pairs);
        match name.as_str() {
            "$ACADVER" => b.report.dxf_version = v.s(1).map(|s| s.trim().to_string()),
            "$INSUNITS" => units.insunits = v.i(70).map(|n| n as i32),
            "$MEASUREMENT" => units.measurement = v.i(70).map(|n| n as i32),
            "$EXTMIN" => extmin = v.xy(10),
            "$EXTMAX" => extmax = v.xy(10),
            _ => {}
        }
    }
    units_from_header(&mut units);
    let header_extents = match (extmin, extmax) {
        // An empty drawing has EXTMIN above EXTMAX (ezdxf's default header is 1e20 and -1e20).
        (Some(a), Some(c)) if a.0.abs() < 1e19 && c.0.abs() < 1e19 && a.0 <= c.0 && a.1 <= c.1 => Some((pt(a.0, a.1), pt(c.0, c.1))),
        _ => None,
    };
    let scale = units.mm_per_unit;
    let mut u = Underlay { layers: b.layers, units, unit_scale_mm: scale, header_extents, blocks: b.blocks, entities: b.model, report: b.report, bounds: None };
    u.bounds = u.compute_bounds();
    Ok(u)
}

fn read_header(rec: &Record, b: &mut Builder) {
    // The HEADER section record holds `9, $NAME` then the value pairs, repeated.
    let mut cur: Option<(String, Vec<(i32, String)>)> = None;
    for (c, v) in &rec.pairs {
        if *c == 9 {
            if let Some(x) = cur.take() {
                b.header.push(x);
            }
            cur = Some((v.trim().to_ascii_uppercase(), Vec::new()));
        } else if let Some((_, p)) = cur.as_mut() {
            p.push((*c, v.clone()));
        }
    }
    if let Some(x) = cur {
        b.header.push(x);
    }
}

fn read_layer(rec: &Record, b: &mut Builder) {
    let v = View(&rec.pairs);
    let Some(name) = v.s(2) else { return };
    let ix = b.layer(name) as usize;
    let flags = v.i(70).unwrap_or(0);
    let colour = v.i(62).unwrap_or(7);
    let l = &mut b.layers[ix];
    l.name = name.to_string();
    l.frozen = flags & 1 != 0;
    l.locked = flags & 4 != 0;
    l.off = colour < 0;
    l.aci = Some(colour.unsigned_abs().clamp(1, 255) as u8);
    l.rgb = v.i(420).map(|rgb| [((rgb >> 16) & 255) as u8, ((rgb >> 8) & 255) as u8, (rgb & 255) as u8]);
    if l.rgb.is_some() {
        l.aci = None;
    }
}

fn finish_poly(b: &mut Builder) {
    let Some(p) = b.poly.take() else { return };
    let flags = p.flags;
    if flags & (16 | 64) != 0 {
        b.report.count_unread(if flags & 64 != 0 { "POLYLINE (polyface mesh)" } else { "POLYLINE (3D mesh)" });
        return;
    }
    // A fit-curve polyline stores the curve points as vertices flagged 8 (or 1 for curve fit)
    // and, for splines, the frame points flagged 16: draw the curve points when present.
    let has_curve = p.verts.iter().any(|v| v.2 & (8 | 1) != 0);
    let used: Vec<&(Pt2, f64, i64)> = if has_curve { p.verts.iter().filter(|v| v.2 & 16 == 0).collect() } else { p.verts.iter().collect() };
    if flags & (2 | 4) != 0 && !has_curve {
        b.report.warn("A curve-fit or spline-fit POLYLINE has no curve vertices: its control polygon is drawn.");
    }
    if used.len() < 2 {
        b.report.count_unread("POLYLINE (fewer than two vertices)");
        return;
    }
    let is_3d = flags & 8 != 0;
    let mirror = !is_3d && is_minus_z(p.extrusion);
    if !is_3d && !is_plain_z(p.extrusion) && !mirror {
        b.report.count_unread("POLYLINE (oblique extrusion)");
        return;
    }
    let pts = used
        .iter()
        .map(|(q, bulge, _)| Vertex { p: if mirror { pt(-q.x, q.y) } else { *q }, bulge: if mirror { -bulge } else { *bulge } })
        .collect();
    b.report.count_read("POLYLINE");
    b.push(Entity { layer: p.layer, colour: p.colour, geom: Geom::Poly { pts, closed: flags & 1 != 0 } });
}

fn entity(rec: &Record, b: &mut Builder) {
    let v = View(&rec.pairs);
    let kind = rec.kind.as_str();
    // Polyline accumulation.
    if b.poly.is_some() {
        match kind {
            "VERTEX" => {
                let (x, y, _z) = v.xyz(10).unwrap_or((0.0, 0.0, 0.0));
                let (flags, bulge) = (v.i(70).unwrap_or(0), v.f(42).unwrap_or(0.0));
                if let Some(p) = b.poly.as_mut() {
                    // 2D vertices are in the polyline's OCS (mirrored later when it is -Z); 3D ones are WCS.
                    p.verts.push((pt(x, y), bulge, flags));
                }
                return;
            }
            "SEQEND" => {
                finish_poly(b);
                return;
            }
            _ => {
                b.report.warn("A POLYLINE was not closed by SEQEND before the next entity; it is kept as read.");
                finish_poly(b);
            }
        }
    }
    if kind == "SEQEND" {
        b.in_attribs = false;
        return;
    }
    if b.in_attribs && kind != "ATTRIB" {
        b.in_attribs = false;
    }
    if v.i(67) == Some(1) && b.cur_block.is_none() {
        b.report.paper_space_skipped += 1;
        return;
    }
    if v.i(60) == Some(1) {
        b.report.count_unread(&format!("{kind} (invisible)"));
        return;
    }
    let layer = {
        let name = v.s(8).unwrap_or("0").to_string();
        b.layer(&name)
    };
    let colour = colour_of(&v);
    let n = extrusion(&v);
    let add = |b: &mut Builder, geom: Geom| {
        b.report.count_read(kind);
        b.push(Entity { layer, colour, geom });
    };
    // Entities with an OCS: plain Z, or exactly -Z handled by mirroring x; others are counted unread.
    let ocs_ok = is_plain_z(n) || is_minus_z(n);
    let oblique = |b: &mut Builder| b.report.count_unread(&format!("{kind} (oblique extrusion)"));
    let mx = |x: f64| if is_minus_z(n) { -x } else { x };

    match kind {
        "LINE" => {
            let (Some(a), Some(c)) = (v.xyz(10), v.xyz(11)) else {
                b.report.count_unread("LINE (missing point)");
                return;
            };
            add(b, Geom::Line(pt(a.0, a.1), pt(c.0, c.1)));
        }
        "CIRCLE" | "ARC" => {
            if !ocs_ok {
                return oblique(b);
            }
            let (Some(c), Some(r)) = (v.xy(10), v.f(40)) else {
                b.report.count_unread(&format!("{kind} (missing centre or radius)"));
                return;
            };
            let centre = pt(mx(c.0), c.1);
            if kind == "CIRCLE" {
                add(b, Geom::Circle { c: centre, r });
            } else {
                let (a0, a1) = (v.f(50).unwrap_or(0.0), v.f(51).unwrap_or(360.0));
                // Mirroring x sends angle a to 180 - a and reverses the sense: the arc from a0
                // to a1 anticlockwise becomes the arc from 180 - a1 to 180 - a0.
                let (a0, a1) = if is_minus_z(n) { (180.0 - a1, 180.0 - a0) } else { (a0, a1) };
                add(b, Geom::Arc { c: centre, r, a0, a1 });
            }
        }
        "ELLIPSE" => {
            let (Some(c), Some(m)) = (v.xyz(10), v.xyz(11)) else {
                b.report.count_unread("ELLIPSE (missing centre or axis)");
                return;
            };
            let ratio = v.f(40).unwrap_or(1.0);
            let nrm = n.unwrap_or((0.0, 0.0, 1.0));
            // Minor axis = ratio * (N x major), plan components.
            let minor = pt(ratio * (nrm.1 * m.2 - nrm.2 * m.1), ratio * (nrm.2 * m.0 - nrm.0 * m.2));
            let t0 = v.f(41).unwrap_or(0.0);
            let mut t1 = v.f(42).unwrap_or(std::f64::consts::TAU);
            while t1 <= t0 {
                t1 += std::f64::consts::TAU;
            }
            add(b, Geom::Ellipse { c: pt(c.0, c.1), major: pt(m.0, m.1), minor, t0, t1 });
        }
        "LWPOLYLINE" => {
            if !ocs_ok {
                return oblique(b);
            }
            let mut pts: Vec<Vertex> = Vec::new();
            let mut pending_x: Option<f64> = None;
            for (c, val) in &rec.pairs {
                let f = super::scan::parse_f64(val);
                match (c, f) {
                    (10, Some(x)) => pending_x = Some(x),
                    (20, Some(y)) => {
                        if let Some(x) = pending_x.take() {
                            pts.push(Vertex { p: pt(mx(x), y), bulge: 0.0 });
                        }
                    }
                    (42, Some(bu)) => {
                        if let Some(last) = pts.last_mut() {
                            last.bulge = if is_minus_z(n) { -bu } else { bu };
                        }
                    }
                    _ => {}
                }
            }
            if pts.len() < 2 {
                b.report.count_unread("LWPOLYLINE (fewer than two vertices)");
                return;
            }
            add(b, Geom::Poly { pts, closed: v.i(70).unwrap_or(0) & 1 != 0 });
        }
        "POLYLINE" => {
            let flags = v.i(70).unwrap_or(0);
            b.poly = Some(PolyAcc { layer, colour, flags, extrusion: n, verts: Vec::new() });
        }
        "SPLINE" => {
            let (ctrl, fit) = {
                let mut ctrl = Vec::new();
                let mut fit = Vec::new();
                let (mut cx, mut fx): (Option<f64>, Option<f64>) = (None, None);
                for (c, val) in &rec.pairs {
                    let Some(f) = super::scan::parse_f64(val) else { continue };
                    match c {
                        10 => cx = Some(f),
                        20 => {
                            if let Some(x) = cx.take() {
                                ctrl.push(pt(x, f));
                            }
                        }
                        11 => fx = Some(f),
                        21 => {
                            if let Some(x) = fx.take() {
                                fit.push(pt(x, f));
                            }
                        }
                        _ => {}
                    }
                }
                (ctrl, fit)
            };
            let flags = v.i(70).unwrap_or(0);
            add(
                b,
                Geom::Spline {
                    degree: v.i(71).unwrap_or(3).max(1) as usize,
                    knots: v.all_f(40),
                    ctrl,
                    weights: v.all_f(41),
                    fit,
                    closed: flags & 1 != 0,
                },
            );
        }
        "TEXT" | "ATTRIB" => {
            let Some(ins) = v.xyz(10) else { return };
            let h72 = v.i(72).unwrap_or(0);
            let v73 = v.i(73).unwrap_or(0);
            // With any alignment other than baseline-left the alignment point (11) is the anchor.
            let at = if h72 != 0 || v73 != 0 { v.xy(11).unwrap_or((ins.0, ins.1)) } else { (ins.0, ins.1) };
            let mut stripped = std::mem::take(&mut b.report.mtext_stripped);
            let text = decode_text(v.s(1).unwrap_or(""), &mut stripped);
            b.report.mtext_stripped = stripped;
            let (h, vv) = match (h72, v73) {
                (4, _) => (HAlign::Centre, VAlign::Middle), // MIDDLE
                (3 | 5, _) => (HAlign::Centre, VAlign::Baseline), // ALIGNED / FIT approximated at the middle of the baseline
                (1, v) => (HAlign::Centre, valign(v)),
                (2, v) => (HAlign::Right, valign(v)),
                (_, v) => (HAlign::Left, valign(v)),
            };
            if h72 == 3 || h72 == 5 {
                b.report.warn("TEXT aligned or fit to two points is placed at the alignment point at its own height.");
            }
            let (x, y) = (mx(at.0), at.1);
            let rot = v.f(50).unwrap_or(0.0);
            let rot = if is_minus_z(n) { 180.0 - rot } else { rot };
            add(
                b,
                Geom::Text(TextGeom { at: pt(x, y), height: v.f(40).unwrap_or(0.0), rotation_deg: rot, width_factor: v.f(41).unwrap_or(1.0), text, h, v: vv, box_width: 0.0 }),
            );
        }
        "MTEXT" => {
            let Some(ins) = v.xyz(10) else { return };
            let mut content = String::new();
            for (c, val) in &rec.pairs {
                if *c == 3 {
                    content.push_str(val);
                }
            }
            content.push_str(v.s(1).unwrap_or(""));
            let mut stripped = std::mem::take(&mut b.report.mtext_stripped);
            let text = strip_mtext(&content, &mut stripped);
            b.report.mtext_stripped = stripped;
            let rot = match v.xy(11) {
                Some((dx, dy)) if dx.hypot(dy) > 1e-12 => dy.atan2(dx).to_degrees(),
                _ => v.f(50).unwrap_or(0.0),
            };
            let (h, vv) = match v.i(71).unwrap_or(1) {
                2 => (HAlign::Centre, VAlign::Top),
                3 => (HAlign::Right, VAlign::Top),
                4 => (HAlign::Left, VAlign::Middle),
                5 => (HAlign::Centre, VAlign::Middle),
                6 => (HAlign::Right, VAlign::Middle),
                7 => (HAlign::Left, VAlign::Bottom),
                8 => (HAlign::Centre, VAlign::Bottom),
                9 => (HAlign::Right, VAlign::Bottom),
                _ => (HAlign::Left, VAlign::Top),
            };
            add(
                b,
                Geom::Text(TextGeom { at: pt(ins.0, ins.1), height: v.f(40).unwrap_or(0.0), rotation_deg: rot, width_factor: 1.0, text, h, v: vv, box_width: v.f(41).unwrap_or(0.0) }),
            );
        }
        "POINT" => {
            if let Some(p) = v.xy(10) {
                add(b, Geom::Point(pt(p.0, p.1)));
            }
        }
        "INSERT" => {
            let Some(p) = v.xy(10) else { return };
            let Some(name) = v.s(2) else { return };
            if v.i(66) == Some(1) {
                b.in_attribs = true;
            }
            let (sx, sy) = (v.f(41).unwrap_or(1.0), v.f(42).unwrap_or(1.0));
            let (mut rot, mut ix) = (v.f(50).unwrap_or(0.0), mx(p.0));
            let mut sxx = sx;
            if is_minus_z(n) {
                // Inserted in a mirrored OCS: mirror the whole placement about the Y axis.
                rot = 180.0 - rot;
                sxx = -sx;
            } else if !is_plain_z(n) {
                ix = p.0;
                b.report.warn("An INSERT with an oblique extrusion is placed as if its extrusion were Z.");
            }
            add(
                b,
                Geom::Insert {
                    block: name.to_string(),
                    at: pt(ix, p.1),
                    sx: sxx,
                    sy,
                    rot_deg: rot,
                    cols: v.i(70).unwrap_or(1).clamp(1, 100_000) as u32,
                    rows: v.i(71).unwrap_or(1).clamp(1, 100_000) as u32,
                    dx: v.f(44).unwrap_or(0.0),
                    dy: v.f(45).unwrap_or(0.0),
                },
            );
        }
        "DIMENSION" => match v.s(2) {
            Some(name) if !name.is_empty() => add(b, Geom::Insert { block: name.to_string(), at: pt(0.0, 0.0), sx: 1.0, sy: 1.0, rot_deg: 0.0, cols: 1, rows: 1, dx: 0.0, dy: 0.0 }),
            _ => b.report.count_unread("DIMENSION (no drawn block)"),
        },
        "SOLID" | "TRACE" => {
            let cs: Vec<Pt2> = [10, 11, 12, 13].iter().filter_map(|c| v.xyz(*c)).map(|p| if kind == "TRACE" { ocs_to_wcs(n, p.0, p.1, p.2) } else { pt(p.0, p.1) }).collect();
            if cs.len() < 3 {
                b.report.count_unread(&format!("{kind} (fewer than three corners)"));
                return;
            }
            // File order is 0, 1, 3, 2 around the outline; with three corners the fourth repeats the third.
            let outline = if cs.len() == 4 { vec![cs[0], cs[1], cs[3], cs[2]] } else { cs };
            add(b, Geom::Solid(outline));
        }
        "3DFACE" => {
            let cs: Vec<Pt2> = [10, 11, 12, 13].iter().filter_map(|c| v.xyz(*c)).map(|p| pt(p.0, p.1)).collect();
            if cs.len() < 3 {
                b.report.count_unread("3DFACE (fewer than three corners)");
                return;
            }
            let mut outline = cs;
            if outline.len() == 4 && outline[3] == outline[2] {
                outline.pop();
            }
            add(b, Geom::Solid(outline));
        }
        "HATCH" => {
            if !ocs_ok {
                return oblique(b);
            }
            let loops = hatch_loops(&rec.pairs, is_minus_z(n));
            if loops.is_empty() {
                b.report.count_unread("HATCH (no boundary read)");
                return;
            }
            add(b, Geom::Hatch(loops));
        }
        "ACAD_PROXY_ENTITY" => {
            let Some(bytes) = super::proxy::graphics_bytes(&rec.pairs) else {
                b.report.count_unread("ACAD_PROXY_ENTITY (no proxy graphics)");
                return;
            };
            let r2010 = b.report.dxf_version.as_deref().is_some_and(|v| v >= "AC1024");
            let decoded = super::proxy::decode(&bytes, b.layers.len(), r2010);
            for (what, n) in &decoded.skipped {
                *b.report.unread.entry(format!("ACAD_PROXY_ENTITY ({what})")).or_default() += n;
            }
            if decoded.truncated {
                b.report.warn("Some proxy graphics ended inside a drawing command; what came before it is drawn.");
            }
            if decoded.items.is_empty() {
                b.report.count_unread("ACAD_PROXY_ENTITY (empty picture)");
                return;
            }
            if super::proxy::is_placeholder(&decoded.items) {
                b.report.count_unread("ACAD_PROXY_ENTITY (no picture saved, only the stand-in box)");
                return;
            }
            b.report.count_read(kind);
            for item in decoded.items {
                let entity = Entity { layer: item.layer.unwrap_or(layer), colour: item.colour.unwrap_or(colour), geom: item.geom };
                b.push(entity);
            }
        }
        "ATTDEF" => b.report.count_unread("ATTDEF (definition, not drawn)"),
        "VERTEX" => b.report.warn("A VERTEX outside a POLYLINE was ignored."),
        other => b.report.count_unread(other),
    }
}

fn valign(v: i64) -> VAlign {
    match v {
        1 => VAlign::Bottom,
        2 => VAlign::Middle,
        3 => VAlign::Top,
        _ => VAlign::Baseline,
    }
}

/// HATCH boundary paths from the record's pairs (see the table at the top of this file).
fn hatch_loops(pairs: &[(i32, String)], mirror: bool) -> Vec<Vec<Geom>> {
    let f = |s: &str| super::scan::parse_f64(s);
    let i = |s: &str| s.trim().parse::<i64>().unwrap_or(0);
    let mx = |x: f64| if mirror { -x } else { x };
    let mut k = pairs.iter().position(|p| p.0 == 91).map(|p| p + 1).unwrap_or(pairs.len());
    let mut loops = Vec::new();
    let take = |k: &mut usize, code: i32| -> Option<&str> {
        let p = pairs.get(*k)?;
        if p.0 == code {
            *k += 1;
            Some(p.1.as_str())
        } else {
            None
        }
    };
    while k < pairs.len() {
        if pairs[k].0 != 92 {
            // Past the boundary section (75 style, 76 pattern type, ...).
            if matches!(pairs[k].0, 75 | 76 | 98) {
                break;
            }
            k += 1;
            continue;
        }
        let flags = i(&pairs[k].1);
        k += 1;
        let mut edges: Vec<Geom> = Vec::new();
        if flags & 2 != 0 {
            let has_bulge = take(&mut k, 72).map(i).unwrap_or(0) != 0;
            let closed = take(&mut k, 73).map(i).unwrap_or(1) != 0;
            let n = take(&mut k, 93).map(i).unwrap_or(0).max(0) as usize;
            let mut verts = Vec::with_capacity(n);
            for _ in 0..n {
                let (Some(x), Some(y)) = (take(&mut k, 10).and_then(f), take(&mut k, 20).and_then(f)) else { break };
                let bulge = if has_bulge { take(&mut k, 42).and_then(f).unwrap_or(0.0) } else { 0.0 };
                verts.push(Vertex { p: pt(mx(x), y), bulge: if mirror { -bulge } else { bulge } });
            }
            if verts.len() >= 2 {
                edges.push(Geom::Poly { pts: verts, closed });
            }
        } else {
            let n = take(&mut k, 93).map(i).unwrap_or(0).max(0) as usize;
            for _ in 0..n {
                let Some(t) = take(&mut k, 72).map(i) else { break };
                match t {
                    1 => {
                        let (x0, y0, x1, y1) = (take(&mut k, 10).and_then(f), take(&mut k, 20).and_then(f), take(&mut k, 11).and_then(f), take(&mut k, 21).and_then(f));
                        if let (Some(x0), Some(y0), Some(x1), Some(y1)) = (x0, y0, x1, y1) {
                            edges.push(Geom::Line(pt(mx(x0), y0), pt(mx(x1), y1)));
                        }
                    }
                    2 => {
                        let (cx, cy, r) = (take(&mut k, 10).and_then(f), take(&mut k, 20).and_then(f), take(&mut k, 40).and_then(f));
                        let (a0, a1, ccw) = (take(&mut k, 50).and_then(f), take(&mut k, 51).and_then(f), take(&mut k, 73).map(i).unwrap_or(1) != 0);
                        if let (Some(cx), Some(cy), Some(r), Some(a0), Some(a1)) = (cx, cy, r, a0, a1) {
                            // A clockwise edge stores 360 - angle, swapped (ezdxf ArcEdge.load_tags).
                            let (s, e) = if ccw { (a0, a1) } else { (360.0 - a1, 360.0 - a0) };
                            let (s, e) = if mirror { (180.0 - e, 180.0 - s) } else { (s, e) };
                            edges.push(Geom::Arc { c: pt(mx(cx), cy), r, a0: s, a1: e });
                        }
                    }
                    3 => {
                        let (cx, cy, mxx, myy) = (take(&mut k, 10).and_then(f), take(&mut k, 20).and_then(f), take(&mut k, 11).and_then(f), take(&mut k, 21).and_then(f));
                        let (ratio, a0, a1, ccw) = (take(&mut k, 40).and_then(f), take(&mut k, 50).and_then(f), take(&mut k, 51).and_then(f), take(&mut k, 73).map(i).unwrap_or(1) != 0);
                        if let (Some(cx), Some(cy), Some(mxx), Some(myy), Some(ratio), Some(a0), Some(a1)) = (cx, cy, mxx, myy, ratio, a0, a1) {
                            let (s, e) = if ccw { (a0, a1) } else { (360.0 - a1, 360.0 - a0) };
                            // The stored angles are geometric angles from the major axis; convert to parameters.
                            let param = |deg: f64| (deg.to_radians().sin() / ratio).atan2(deg.to_radians().cos());
                            let (t0, mut t1) = (param(s), param(e));
                            while t1 <= t0 {
                                t1 += std::f64::consts::TAU;
                            }
                            // Mirroring about the Y axis maps both axes, so the parameters are unchanged.
                            let (major, minor) = (pt(mx(mxx), myy), pt(mx(-myy * ratio), mxx * ratio));
                            edges.push(Geom::Ellipse { c: pt(mx(cx), cy), major, minor, t0, t1 });
                        }
                    }
                    4 => {
                        let degree = take(&mut k, 94).map(i).unwrap_or(3).max(1) as usize;
                        let rational = take(&mut k, 73).map(i).unwrap_or(0) != 0;
                        let _periodic = take(&mut k, 74);
                        let nk = take(&mut k, 95).map(i).unwrap_or(0).max(0) as usize;
                        let nc = take(&mut k, 96).map(i).unwrap_or(0).max(0) as usize;
                        let knots: Vec<f64> = (0..nk).filter_map(|_| take(&mut k, 40).and_then(f)).collect();
                        let (mut ctrl, mut weights) = (Vec::new(), Vec::new());
                        for _ in 0..nc {
                            let (Some(x), Some(y)) = (take(&mut k, 10).and_then(f), take(&mut k, 20).and_then(f)) else { break };
                            ctrl.push(pt(mx(x), y));
                            if rational {
                                weights.push(take(&mut k, 42).and_then(f).unwrap_or(1.0));
                            }
                        }
                        // Optional fit data: 97 count then 11/21 points, then tangents 12/22, 13/23.
                        if let Some(nf) = take(&mut k, 97).map(i) {
                            for _ in 0..nf.max(0) {
                                let _ = (take(&mut k, 11), take(&mut k, 21));
                            }
                        }
                        let _ = (take(&mut k, 12), take(&mut k, 22), take(&mut k, 13), take(&mut k, 23));
                        edges.push(Geom::Spline { degree, knots, ctrl, weights, fit: Vec::new(), closed: false });
                    }
                    _ => break,
                }
            }
        }
        // Source boundary objects: 97 count then 330 handles.
        if let Some(n) = take(&mut k, 97).map(i) {
            for _ in 0..n.max(0) {
                let _ = take(&mut k, 330);
            }
        }
        if !edges.is_empty() {
            loops.push(edges);
        }
    }
    loops
}
