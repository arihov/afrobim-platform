//! The drawn form of a proxy entity (`ACAD_PROXY_ENTITY`).
//!
//! A program that saves an object of its own class, such as a Civil 3D alignment, surface,
//! corridor, COGO point or label, can store with it a picture of the object as a list of simple
//! drawing commands: "proxy graphics". A DWG converter that does not know the class writes the
//! object out as `ACAD_PROXY_ENTITY` and keeps that picture. This module turns the picture into
//! the reader's own geometry, so such objects are seen as lines, arcs, circles, faces and text.
//! They are pictures, not design data: an alignment drawn this way is linework, with no stations
//! or geometry behind it.
//!
//! # Where the bytes are
//!
//! In the entity's record: group 92 (or 160) gives the size of the graphics in bytes, and the
//! group 310 values that follow are the bytes in hexadecimal, a chunk each. A group 93 starts the
//! object's own data (also in 310 chunks), which is not read.
//!
//! # The layout
//!
//! As decoded by `ezdxf` (`ezdxf/proxygraphic.py` and `ezdxf/tools/binarydata.py`, MIT licence),
//! which follows the Open Design Alliance's DWG specification. Little-endian; every field starts
//! on a multiple of four bytes from the start of its command.
//!
//! - Header: total size (u32), number of commands (u32).
//! - Each command: its size in bytes including this header (u32), its type (u32), its data.
//!
//! | type | command | data read |
//! |---|---|---|
//! | 2 | circle | centre (3 f64), radius, normal (3 f64) |
//! | 3 | circle through three points | 3 points |
//! | 4 | circular arc | centre, radius, normal, start vector (3 f64), sweep angle (radians) |
//! | 5 | arc through three points | start, middle, end |
//! | 6 | polyline | count (u32), points |
//! | 7 | polygon | count, points (closed) |
//! | 8 | mesh | rows, columns (u32), rows × columns points |
//! | 9 | shell | vertex count, vertices, face entry count, then per face: edge count (i32, sign ignored) and vertex indices |
//! | 10 | text | start, normal, direction, height, width factor, oblique, text (8-bit, null-ended, padded) |
//! | 11 | text 2 | start, normal, direction, text (8-bit), then sizes and fonts that are not needed |
//! | 14 | colour | ACI (u32): 0 by block, 256 by layer |
//! | 16 | layer | index into the LAYER table, in table order |
//! | 22 | true colour | raw colour: top byte 0xC0 by layer, 0xC1 by block, 0xC2 RGB, 0xC3 ACI |
//! | 29, 30 | push transform | 16 f64, row by row, applied as `M · p` |
//! | 31 | pop transform | — |
//! | 32 | polyline with normal | count, points, then one more point that is the normal |
//! | 33 | light-weight polyline | a DWG bit stream (see [`lwpolyline`]) |
//! | 36 | Unicode text | as 10, the text in UTF-16, ended by two zero bytes |
//! | 38 | Unicode text 2 | as 11, the text in UTF-16 |
//! | 44 | elliptic arc | centre, normal, major and minor axis lengths, start and end parameter, major axis angle |
//!
//! Other types (line type, marker, fill, line weight, clipping, materials …) change nothing that
//! is drawn here and are skipped. A command of a type not in this list is counted as not read.
//!
//! Points are in world coordinates and only their plan position is kept. A circle or arc whose
//! normal is neither up nor down is not drawn (its plan view is an ellipse) and is counted.

use afrobim_draw::Pt2;

use super::geom::pt;
use super::model::{ColourSpec, Geom, HAlign, TextGeom, VAlign, Vertex};

/// One drawn item of a proxy, with the layer and colour it is drawn in when the commands set
/// them (`None`: the proxy entity's own).
#[derive(Debug, Clone, PartialEq)]
pub struct ProxyItem {
    /// Index into the reader's layers (LAYER table order).
    pub layer: Option<u32>,
    pub colour: Option<ColourSpec>,
    pub geom: Geom,
}

/// What came of one proxy's graphics.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct Decoded {
    pub items: Vec<ProxyItem>,
    /// Commands that were not drawn, by a short name, with the count.
    pub skipped: Vec<(String, usize)>,
    /// True when the bytes ended inside a command or a size was impossible; what came before is kept.
    pub truncated: bool,
}

impl Decoded {
    fn skip(&mut self, what: &str) {
        match self.skipped.iter_mut().find(|(n, _)| n == what) {
            Some((_, c)) => *c += 1,
            None => self.skipped.push((what.to_string(), 1)),
        }
    }
}

/// The proxy graphics bytes of a record: the 310 chunks after a group 92 or 160, up to the next
/// group that is not 310. `None` when the record has none, or a chunk is not hexadecimal.
pub fn graphics_bytes(pairs: &[(i32, String)]) -> Option<Vec<u8>> {
    let start = pairs.iter().position(|(c, _)| *c == 92 || *c == 160)?;
    let declared = pairs[start].1.trim().parse::<usize>().ok();
    let mut out = Vec::with_capacity(declared.unwrap_or(0));
    for (code, value) in &pairs[start + 1..] {
        if *code != 310 {
            break;
        }
        hex_into(value.trim(), &mut out)?;
    }
    if out.is_empty() {
        return None;
    }
    if let Some(n) = declared {
        out.truncate(n);
    }
    Some(out)
}

fn hex_into(text: &str, out: &mut Vec<u8>) -> Option<()> {
    let bytes = text.as_bytes();
    if bytes.len() % 2 != 0 {
        return None;
    }
    let nibble = |c: u8| match c {
        b'0'..=b'9' => Some(c - b'0'),
        b'a'..=b'f' => Some(c - b'a' + 10),
        b'A'..=b'F' => Some(c - b'A' + 10),
        _ => None,
    };
    for pair in bytes.chunks_exact(2) {
        out.push(nibble(pair[0])? << 4 | nibble(pair[1])?);
    }
    Some(())
}

/// A little-endian reader over one command's data, every field on a multiple of four bytes.
struct Bytes<'a> {
    b: &'a [u8],
    i: usize,
}

impl<'a> Bytes<'a> {
    fn align(&mut self) {
        self.i = self.i.div_ceil(4) * 4;
    }
    fn u32(&mut self) -> Option<u32> {
        let v = u32::from_le_bytes(self.b.get(self.i..self.i + 4)?.try_into().ok()?);
        self.i += 4;
        Some(v)
    }
    fn i32(&mut self) -> Option<i32> {
        self.u32().map(|v| v as i32)
    }
    fn f64(&mut self) -> Option<f64> {
        let v = f64::from_le_bytes(self.b.get(self.i..self.i + 8)?.try_into().ok()?);
        self.i += 8;
        v.is_finite().then_some(v)
    }
    fn xyz(&mut self) -> Option<[f64; 3]> {
        Some([self.f64()?, self.f64()?, self.f64()?])
    }
    /// An 8-bit string ended by a zero byte, then aligned.
    fn str8(&mut self, utf8: bool) -> Option<String> {
        let rest = self.b.get(self.i..)?;
        let end = rest.iter().position(|&c| c == 0)?;
        let text = if utf8 { String::from_utf8_lossy(&rest[..end]).into_owned() } else { rest[..end].iter().map(|&c| cp1252(c)).collect() };
        self.i += end + 1;
        self.align();
        Some(text)
    }
    /// A UTF-16LE string ended by two zero bytes at an even offset, then aligned.
    fn str16(&mut self) -> Option<String> {
        let rest = self.b.get(self.i..)?;
        let mut units = Vec::new();
        let mut k = 0;
        loop {
            let pair = rest.get(k..k + 2)?;
            let u = u16::from_le_bytes([pair[0], pair[1]]);
            k += 2;
            if u == 0 {
                break;
            }
            units.push(u);
        }
        self.i += k;
        self.align();
        Some(String::from_utf16_lossy(&units))
    }
}

/// Windows-1252 to a character (the 0x80 to 0x9F range by its table; the rest is Latin-1).
fn cp1252(c: u8) -> char {
    const HIGH: [char; 32] = [
        '€', '\u{81}', '‚', 'ƒ', '„', '…', '†', '‡', 'ˆ', '‰', 'Š', '‹', 'Œ', '\u{8D}', 'Ž', '\u{8F}', '\u{90}', '‘', '’', '“', '”', '•', '–', '—', '˜', '™', 'š', '›',
        'œ', '\u{9D}', 'ž', 'Ÿ',
    ];
    match c {
        0x80..=0x9F => HIGH[(c - 0x80) as usize],
        _ => c as char,
    }
}

type Matrix = [f64; 16];

fn apply(m: Option<&Matrix>, p: [f64; 3]) -> [f64; 3] {
    match m {
        None => p,
        Some(m) => [
            m[0] * p[0] + m[1] * p[1] + m[2] * p[2] + m[3],
            m[4] * p[0] + m[5] * p[1] + m[6] * p[2] + m[7],
            m[8] * p[0] + m[9] * p[1] + m[10] * p[2] + m[11],
        ],
    }
}

/// The direction of a vector under the linear part of a transform.
fn apply_dir(m: Option<&Matrix>, v: [f64; 3]) -> [f64; 3] {
    match m {
        None => v,
        Some(m) => [m[0] * v[0] + m[1] * v[1] + m[2] * v[2], m[4] * v[0] + m[5] * v[1] + m[6] * v[2], m[8] * v[0] + m[9] * v[1] + m[10] * v[2]],
    }
}

fn plan(p: [f64; 3]) -> Pt2 {
    pt(p[0], p[1])
}

/// +1 for a normal straight up, -1 for straight down, 0 for anything else.
fn up_or_down(n: [f64; 3]) -> i8 {
    let len = (n[0] * n[0] + n[1] * n[1] + n[2] * n[2]).sqrt();
    if len == 0.0 {
        return 0;
    }
    let tilt = (n[0] * n[0] + n[1] * n[1]).sqrt() / len;
    if tilt > 1e-9 {
        0
    } else if n[2] > 0.0 {
        1
    } else {
        -1
    }
}

/// The circle through three plan points: centre and radius, or `None` when they are on one line.
fn circle_3p(a: Pt2, b: Pt2, c: Pt2) -> Option<(Pt2, f64)> {
    let d = 2.0 * (a.x * (b.y - c.y) + b.x * (c.y - a.y) + c.x * (a.y - b.y));
    if d.abs() < 1e-12 * (1.0 + a.x.abs() + a.y.abs()).powi(2) {
        return None;
    }
    let sq = |p: Pt2| p.x * p.x + p.y * p.y;
    let ux = (sq(a) * (b.y - c.y) + sq(b) * (c.y - a.y) + sq(c) * (a.y - b.y)) / d;
    let uy = (sq(a) * (c.x - b.x) + sq(b) * (a.x - c.x) + sq(c) * (b.x - a.x)) / d;
    let centre = pt(ux, uy);
    Some((centre, ((a.x - ux).powi(2) + (a.y - uy).powi(2)).sqrt()))
}

fn polyline(points: Vec<Pt2>, closed: bool) -> Option<Geom> {
    match points.len() {
        0 => None,
        1 => Some(Geom::Point(points[0])),
        2 if !closed => Some(Geom::Line(points[0], points[1])),
        _ => Some(Geom::Poly { pts: points.into_iter().map(|p| Vertex { p, bulge: 0.0 }).collect(), closed }),
    }
}

/// Decodes one proxy's graphics. `layer_count` is the number of layers the reader knows, in
/// LAYER table order; `r2010` says whether the file is R2010 or later (for type 33).
pub fn decode(data: &[u8], layer_count: usize, r2010: bool) -> Decoded {
    let mut out = Decoded::default();
    if data.len() < 8 {
        out.truncated = true;
        return out;
    }
    let total = u32::from_le_bytes(data[0..4].try_into().unwrap()) as usize;
    let data = &data[..total.clamp(8, data.len())];
    let utf8 = r2010; // 8-bit text is UTF-8 from R2007 on; R2010 is the nearest test we have
    let mut layer: Option<u32> = None;
    let mut colour: Option<ColourSpec> = None;
    let mut stack: Vec<Matrix> = Vec::new();
    let mut at = 8;
    while at + 8 <= data.len() {
        let size = u32::from_le_bytes(data[at..at + 4].try_into().unwrap()) as usize;
        let kind = u32::from_le_bytes(data[at + 4..at + 8].try_into().unwrap());
        if size < 8 || at + size > data.len() {
            out.truncated = true;
            break;
        }
        let mut r = Bytes { b: &data[at + 8..at + size], i: 0 };
        at += size;
        let m = stack.last();
        let emit = |out: &mut Decoded, geom: Option<Geom>, what: &str| match geom {
            Some(geom) => out.items.push(ProxyItem { layer, colour, geom }),
            None => out.skip(&format!("{what} (unreadable)")),
        };
        match kind {
            2 => {
                let g = (|| {
                    let c = apply(m, r.xyz()?);
                    let radius = r.f64()?;
                    let n = apply_dir(m, r.xyz()?);
                    (up_or_down(n) != 0).then(|| Geom::Circle { c: plan(c), r: radius })
                })();
                emit(&mut out, g, "circle");
            }
            3 => {
                let g = (|| {
                    let (a, b, c) = (plan(apply(m, r.xyz()?)), plan(apply(m, r.xyz()?)), plan(apply(m, r.xyz()?)));
                    let (centre, radius) = circle_3p(a, b, c)?;
                    Some(Geom::Circle { c: centre, r: radius })
                })();
                emit(&mut out, g, "circle through three points");
            }
            4 => {
                let g = (|| {
                    let c = apply(m, r.xyz()?);
                    let radius = r.f64()?;
                    let n = apply_dir(m, r.xyz()?);
                    let s = apply_dir(m, r.xyz()?);
                    let sweep = r.f64()?.to_degrees();
                    let start = s[1].atan2(s[0]).to_degrees();
                    let (a0, a1) = match up_or_down(n) {
                        1 => (start, start + sweep),
                        -1 => (start - sweep, start),
                        _ => return None,
                    };
                    Some(Geom::Arc { c: plan(c), r: radius, a0, a1 })
                })();
                emit(&mut out, g, "arc");
            }
            5 => {
                let g = (|| {
                    let (a, b, c) = (plan(apply(m, r.xyz()?)), plan(apply(m, r.xyz()?)), plan(apply(m, r.xyz()?)));
                    let (centre, radius) = circle_3p(a, b, c)?;
                    let ang = |p: Pt2| (p.y - centre.y).atan2(p.x - centre.x).to_degrees();
                    // Anticlockwise when the middle point is to the left of start → end.
                    let left = (c.x - a.x) * (b.y - a.y) - (c.y - a.y) * (b.x - a.x) < 0.0;
                    let (a0, a1) = if left { (ang(a), ang(c)) } else { (ang(c), ang(a)) };
                    Some(Geom::Arc { c: centre, r: radius, a0, a1 })
                })();
                emit(&mut out, g, "arc through three points");
            }
            6 | 7 | 32 => {
                let g = (|| {
                    let n = r.u32()? as usize;
                    if n > r.b.len() / 24 + 1 {
                        return None;
                    }
                    let mut points = Vec::with_capacity(n);
                    for _ in 0..n {
                        points.push(plan(apply(m, r.xyz()?)));
                    }
                    polyline(points, kind == 7)
                })();
                emit(&mut out, g, "polyline");
            }
            8 => {
                let g = (|| {
                    let (rows, cols) = (r.u32()? as usize, r.u32()? as usize);
                    if rows == 0 || cols == 0 || rows.saturating_mul(cols) > r.b.len() / 24 {
                        return None;
                    }
                    let mut grid = Vec::with_capacity(rows * cols);
                    for _ in 0..rows * cols {
                        grid.push(plan(apply(m, r.xyz()?)));
                    }
                    Some((rows, cols, grid))
                })();
                match g {
                    Some((rows, cols, grid)) => {
                        // Rows and columns of the mesh as polylines.
                        for i in 0..rows {
                            if let Some(geom) = polyline((0..cols).map(|j| grid[i * cols + j]).collect(), false) {
                                out.items.push(ProxyItem { layer, colour, geom });
                            }
                        }
                        for j in 0..cols {
                            if let Some(geom) = polyline((0..rows).map(|i| grid[i * cols + j]).collect(), false) {
                                out.items.push(ProxyItem { layer, colour, geom });
                            }
                        }
                    }
                    None => out.skip("mesh (unreadable)"),
                }
            }
            9 => {
                let faces = (|| {
                    let nv = r.u32()? as usize;
                    if nv > r.b.len() / 24 {
                        return None;
                    }
                    let mut verts = Vec::with_capacity(nv);
                    for _ in 0..nv {
                        verts.push(plan(apply(m, r.xyz()?)));
                    }
                    let entries = r.u32()? as usize;
                    let mut faces = Vec::new();
                    let mut read = 0;
                    while read < entries {
                        let edges = r.i32()?.unsigned_abs() as usize;
                        read += 1 + edges;
                        if edges > nv.max(1) * 4 {
                            return None;
                        }
                        let mut face = Vec::with_capacity(edges);
                        for _ in 0..edges {
                            face.push(*verts.get(r.u32()? as usize)?);
                        }
                        faces.push(face);
                    }
                    Some(faces)
                })();
                match faces {
                    Some(faces) => {
                        for face in faces.into_iter().filter(|f| f.len() >= 2) {
                            out.items.push(ProxyItem { layer, colour, geom: Geom::Solid(face) });
                        }
                    }
                    None => out.skip("shell (unreadable)"),
                }
            }
            10 | 11 | 36 | 38 => {
                let g = (|| {
                    let start = apply(m, r.xyz()?);
                    let _normal = r.xyz()?;
                    let dir = apply_dir(m, r.xyz()?);
                    let (height, width_factor, text) = match kind {
                        10 => {
                            let (h, w, _oblique) = (r.f64()?, r.f64()?, r.f64()?);
                            (h, w, r.str8(utf8)?)
                        }
                        36 => {
                            let (h, w, _oblique) = (r.f64()?, r.f64()?, r.f64()?);
                            (h, w, r.str16()?)
                        }
                        _ => {
                            let text = if kind == 11 { r.str8(utf8)? } else { r.str16()? };
                            let (_len, _raw) = (r.i32()?, r.i32()?);
                            let (h, w) = (r.f64()?, r.f64()?);
                            (h, w, text)
                        }
                    };
                    if text.trim().is_empty() {
                        return Some(None);
                    }
                    Some(Some(Geom::Text(TextGeom {
                        at: plan(start),
                        height,
                        rotation_deg: dir[1].atan2(dir[0]).to_degrees(),
                        width_factor: if width_factor > 0.0 { width_factor } else { 1.0 },
                        text,
                        h: HAlign::Left,
                        v: VAlign::Baseline,
                        box_width: 0.0,
                    })))
                })();
                match g {
                    Some(Some(geom)) => out.items.push(ProxyItem { layer, colour, geom }),
                    Some(None) => {}
                    None => out.skip("text (unreadable)"),
                }
            }
            14 => {
                if let Some(aci) = r.u32() {
                    colour = Some(match aci {
                        0 => ColourSpec::ByBlock,
                        1..=255 => ColourSpec::Aci(aci as u8),
                        _ => ColourSpec::ByLayer,
                    });
                }
            }
            16 => {
                if let Some(i) = r.u32() {
                    if (i as usize) < layer_count {
                        layer = Some(i);
                    }
                }
            }
            22 => {
                if let Some(raw) = r.u32() {
                    colour = match raw >> 24 {
                        0xC0 => Some(ColourSpec::ByLayer),
                        0xC1 => Some(ColourSpec::ByBlock),
                        0xC2 => Some(ColourSpec::Rgb([(raw >> 16) as u8, (raw >> 8) as u8, raw as u8])),
                        0xC3 => match (raw & 0xFF) as u8 {
                            0 => Some(ColourSpec::ByBlock),
                            a => Some(ColourSpec::Aci(a)),
                        },
                        _ => colour,
                    };
                }
            }
            29 | 30 => {
                let mut mtx = [0.0; 16];
                let mut ok = true;
                for v in mtx.iter_mut() {
                    match r.f64() {
                        Some(x) => *v = x,
                        None => ok = false,
                    }
                }
                // A transform nested inside another applies after it.
                if ok {
                    let composed = match stack.last() {
                        Some(outer) => mul(outer, &mtx),
                        None => mtx,
                    };
                    stack.push(composed);
                } else {
                    out.skip("transform (unreadable)");
                    stack.push(stack.last().copied().unwrap_or(IDENTITY));
                }
            }
            31 => {
                stack.pop();
            }
            33 => {
                let g = lwpolyline(r.b, r2010).and_then(|(points, closed)| {
                    let points: Vec<Vertex> = points.into_iter().map(|(p, bulge)| Vertex { p: plan(apply(m, [p.x, p.y, 0.0])), bulge }).collect();
                    match points.len() {
                        0 => None,
                        1 => Some(Geom::Point(points[0].p)),
                        _ => Some(Geom::Poly { pts: points, closed }),
                    }
                });
                emit(&mut out, g, "light-weight polyline");
            }
            44 => {
                let g = (|| {
                    let c = apply(m, r.xyz()?);
                    let n = r.xyz()?;
                    let (major, minor) = (r.f64()?, r.f64()?);
                    let (t0, t1) = (r.f64()?, r.f64()?);
                    let angle = r.f64()?;
                    let sign = f64::from(up_or_down(n));
                    if sign == 0.0 || major <= 0.0 {
                        return None;
                    }
                    // Plan view of an up or down normal; the major axis at `angle` in its OCS.
                    let mx = pt(major * angle.cos() * sign, major * angle.sin());
                    let mn = pt(-mx.y * minor / major * sign, mx.x * minor / major * sign);
                    let mut t1 = t1;
                    while t1 <= t0 {
                        t1 += std::f64::consts::TAU;
                    }
                    Some(Geom::Ellipse { c: plan(c), major: mx, minor: mn, t0, t1 })
                })();
                emit(&mut out, g, "elliptic arc");
            }
            // Attributes that change nothing drawn here, and the extents record.
            1 | 18 | 19 | 20 | 23 | 24 | 25 | 26 | 27 | 28 | 34 | 35 | 21 | 15 | 17 | 37 => {}
            12 | 13 => out.skip("construction line"),
            other => out.skip(&format!("command {other}")),
        }
    }
    out
}

/// True when the picture is only the stand-in a program writes when it has no picture of the
/// object to save: the class name and application in brackets (for example
/// `AeccDbFace (AeccLand130)`) in a box drawn as one polyline. Such a box sits at the origin,
/// not where the object is, so it is better left out.
pub fn is_placeholder(items: &[ProxyItem]) -> bool {
    let [a, b] = items else { return false };
    let (text, other) = match (&a.geom, &b.geom) {
        (Geom::Text(t), g) | (g, Geom::Text(t)) => (t, g),
        _ => return false,
    };
    let looks_like_class = text.text.split_once(" (").is_some_and(|(class, rest)| {
        let word = |s: &str| !s.is_empty() && s.chars().all(|c| c.is_ascii_alphanumeric() || c == '_');
        word(class) && rest.strip_suffix(')').is_some_and(word)
    });
    looks_like_class && matches!(other, Geom::Poly { .. })
}

const IDENTITY: Matrix = [1.0, 0.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 0.0, 1.0];

fn mul(a: &Matrix, b: &Matrix) -> Matrix {
    let mut out = [0.0; 16];
    for i in 0..4 {
        for j in 0..4 {
            out[i * 4 + j] = (0..4).map(|k| a[i * 4 + k] * b[k * 4 + j]).sum();
        }
    }
    out
}

/// A reader over a DWG bit stream: bits taken from the most significant end of each byte.
struct Bits<'a> {
    b: &'a [u8],
    bit: usize,
}

impl<'a> Bits<'a> {
    fn bits(&mut self, count: usize) -> Option<u64> {
        let mut v = 0u64;
        for _ in 0..count {
            let byte = *self.b.get(self.bit >> 3)?;
            v = v << 1 | u64::from(byte >> (7 - (self.bit & 7)) & 1);
            self.bit += 1;
        }
        Some(v)
    }
    fn byte(&mut self) -> Option<u8> {
        self.bits(8).map(|v| v as u8)
    }
    fn raw_u32(&mut self) -> Option<u32> {
        Some(u32::from_le_bytes([self.byte()?, self.byte()?, self.byte()?, self.byte()?]))
    }
    fn raw_f64(&mut self) -> Option<f64> {
        let mut b = [0u8; 8];
        for x in b.iter_mut() {
            *x = self.byte()?;
        }
        Some(f64::from_le_bytes(b))
    }
    /// BS: 00 a short follows, 01 an unsigned byte, 10 zero, 11 256.
    fn bit_short(&mut self) -> Option<i32> {
        Some(match self.bits(2)? {
            0 => i16::from_le_bytes([self.byte()?, self.byte()?]) as i32,
            1 => self.byte()? as i32,
            2 => 0,
            _ => 256,
        })
    }
    /// BL: 00 a long follows, 01 an unsigned byte, 10 zero.
    fn bit_long(&mut self) -> Option<i64> {
        Some(match self.bits(2)? {
            0 => self.raw_u32()? as i32 as i64,
            1 => self.byte()? as i64,
            2 => 0,
            _ => 256,
        })
    }
    /// BD: 00 a double follows, 01 one, 10 zero.
    fn bit_double(&mut self) -> Option<f64> {
        Some(match self.bits(2)? {
            0 => self.raw_f64()?,
            1 => 1.0,
            _ => 0.0,
        })
    }
    /// DD: a double that defaults to `default`: 00 the default, 01 its low four bytes replaced,
    /// 10 bytes 4 and 5 and then 0 to 3 replaced, 11 a whole double.
    fn bit_double_default(&mut self, default: f64) -> Option<f64> {
        let mut d = default.to_le_bytes();
        Some(match self.bits(2)? {
            0 => default,
            1 => {
                for k in 0..4 {
                    d[k] = self.byte()?;
                }
                f64::from_le_bytes(d)
            }
            2 => {
                d[4] = self.byte()?;
                d[5] = self.byte()?;
                for k in 0..4 {
                    d[k] = self.byte()?;
                }
                f64::from_le_bytes(d)
            }
            _ => self.raw_f64()?,
        })
    }
}

/// A light-weight polyline stored as in a DWG (ODA specification, LWPLINE, as `ezdxf` reads it):
/// a raw u32 byte count, then a BS flag word (4 constant width, 8 elevation, 2 thickness,
/// 1 extrusion, 16 bulges follow, 512 closed; from R2010, 1024 vertex ids and 32 widths follow),
/// BL point count, BL bulge count, the first point as two raw doubles and the rest as DD against
/// the previous point, then the bulges as BD. Returns the plan vertices with bulges, and closed.
pub fn lwpolyline(data: &[u8], r2010: bool) -> Option<(Vec<(Pt2, f64)>, bool)> {
    let mut b = Bits { b: data, bit: 0 };
    let _bytes = b.raw_u32()?;
    let flag = b.bit_short()?;
    if flag & 4 != 0 {
        b.bit_double()?;
    }
    if flag & 8 != 0 {
        b.bit_double()?;
    }
    if flag & 2 != 0 {
        b.bit_double()?;
    }
    if flag & 1 != 0 {
        for _ in 0..3 {
            b.bit_double()?;
        }
    }
    let closed = flag & 512 != 0;
    let n = b.bit_long()?;
    if n <= 0 || n as usize > data.len() * 4 {
        return None;
    }
    let n = n as usize;
    let nb = if flag & 16 != 0 { b.bit_long()?.max(0) as usize } else { 0 };
    if r2010 {
        if flag & 1024 != 0 {
            b.bit_long()?;
        }
        if flag & 32 != 0 {
            b.bit_long()?;
        }
    }
    let mut pts = Vec::with_capacity(n);
    let (mut x, mut y) = (b.raw_f64()?, b.raw_f64()?);
    pts.push(pt(x, y));
    for _ in 1..n {
        x = b.bit_double_default(x)?;
        y = b.bit_double_default(y)?;
        pts.push(pt(x, y));
    }
    let mut bulges = vec![0.0; n];
    for k in 0..nb.min(n) {
        bulges[k] = b.bit_double()?;
    }
    if pts.iter().any(|p| !p.x.is_finite() || !p.y.is_finite()) {
        return None;
    }
    Some((pts.into_iter().zip(bulges).collect(), closed))
}

#[cfg(test)]
pub(crate) mod build {
    //! Writes proxy graphics, for tests: the layout of the module documentation.

    pub struct Graphics {
        cmds: Vec<u8>,
        count: u32,
    }

    impl Graphics {
        pub fn new() -> Graphics {
            Graphics { cmds: Vec::new(), count: 0 }
        }

        pub fn command(&mut self, kind: u32, data: &[u8]) -> &mut Graphics {
            let size = 8 + data.len();
            assert_eq!(size % 4, 0, "command data must be a multiple of four bytes");
            self.cmds.extend_from_slice(&(size as u32).to_le_bytes());
            self.cmds.extend_from_slice(&kind.to_le_bytes());
            self.cmds.extend_from_slice(data);
            self.count += 1;
            self
        }

        pub fn bytes(&self) -> Vec<u8> {
            let mut out = Vec::new();
            out.extend_from_slice(&((8 + self.cmds.len()) as u32).to_le_bytes());
            out.extend_from_slice(&self.count.to_le_bytes());
            out.extend_from_slice(&self.cmds);
            out
        }

        /// The DXF groups: 92 size, then 310 chunks of at most 127 bytes in hexadecimal.
        pub fn dxf_groups(&self) -> String {
            let b = self.bytes();
            let mut s = format!("92\n{}\n", b.len());
            for chunk in b.chunks(127) {
                s.push_str("310\n");
                for x in chunk {
                    s.push_str(&format!("{x:02X}"));
                }
                s.push('\n');
            }
            s
        }
    }

    pub fn f(values: &[f64]) -> Vec<u8> {
        values.iter().flat_map(|v| v.to_le_bytes()).collect()
    }

    pub fn u(values: &[u32]) -> Vec<u8> {
        values.iter().flat_map(|v| v.to_le_bytes()).collect()
    }

    pub fn utf16(text: &str) -> Vec<u8> {
        let mut b: Vec<u8> = text.encode_utf16().flat_map(|u| u.to_le_bytes()).collect();
        b.extend_from_slice(&[0, 0]);
        while b.len() % 4 != 0 {
            b.push(0);
        }
        b
    }

    pub fn ascii(text: &str) -> Vec<u8> {
        let mut b = text.as_bytes().to_vec();
        b.push(0);
        while b.len() % 4 != 0 {
            b.push(0);
        }
        b
    }

    pub fn cat(parts: &[Vec<u8>]) -> Vec<u8> {
        parts.concat()
    }
}

#[cfg(test)]
mod tests {
    use super::build::*;
    use super::*;

    fn close(a: f64, b: f64) -> bool {
        (a - b).abs() < 1e-9
    }

    #[test]
    fn the_bytes_are_the_310_chunks_after_92_cut_to_its_size() {
        let pairs = vec![
            (8, "C-ROAD".to_string()),
            (92, "5".to_string()),
            (310, "0102".to_string()),
            (310, "030405FF".to_string()),
            (93, "64".to_string()),
            (310, "AAAA".to_string()),
        ];
        assert_eq!(graphics_bytes(&pairs), Some(vec![1, 2, 3, 4, 5]));
        assert_eq!(graphics_bytes(&[(160, "2".to_string()), (310, "ABCD".to_string())]), Some(vec![0xAB, 0xCD]));
        assert_eq!(graphics_bytes(&[(8, "0".to_string())]), None);
        assert_eq!(graphics_bytes(&[(92, "2".to_string()), (310, "XYZW".to_string())]), None);
    }

    #[test]
    fn lines_circles_arcs_polygons_and_text_are_decoded() {
        let up = [0.0, 0.0, 1.0];
        let mut g = Graphics::new();
        g.command(6, &cat(&[u(&[2]), f(&[1.0, 2.0, 0.0, 4.0, 6.0, 0.0])]))
            .command(6, &cat(&[u(&[3]), f(&[0.0, 0.0, 5.0, 10.0, 0.0, 5.0, 10.0, 10.0, 5.0])]))
            .command(7, &cat(&[u(&[3]), f(&[0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 1.0, 0.0])]))
            .command(2, &cat(&[f(&[100.0, 200.0, 0.0, 2.5]), f(&up)]))
            // A quarter arc from east, sweep 90 degrees, about +Z.
            .command(4, &cat(&[f(&[0.0, 0.0, 0.0, 3.0]), f(&up), f(&[1.0, 0.0, 0.0, std::f64::consts::FRAC_PI_2])]))
            // The same about -Z: clockwise from east, so anticlockwise from -90 to 0.
            .command(4, &cat(&[f(&[0.0, 0.0, 0.0, 3.0]), f(&[0.0, 0.0, -1.0]), f(&[1.0, 0.0, 0.0, std::f64::consts::FRAC_PI_2])]))
            .command(36, &cat(&[f(&[5.0, 6.0, 0.0]), f(&up), f(&[0.0, 1.0, 0.0]), f(&[2.0, 0.8, 0.0]), utf16("CH 1+250 Ø")]))
            .command(10, &cat(&[f(&[7.0, 8.0, 0.0]), f(&up), f(&[1.0, 0.0, 0.0]), f(&[1.5, 1.0, 0.0]), ascii("PI 3")]));
        let d = decode(&g.bytes(), 1, true);
        assert!(!d.truncated && d.skipped.is_empty(), "{d:?}");
        let geoms: Vec<&Geom> = d.items.iter().map(|i| &i.geom).collect();
        assert_eq!(geoms[0], &Geom::Line(pt(1.0, 2.0), pt(4.0, 6.0)));
        assert!(matches!(geoms[1], Geom::Poly { pts, closed: false } if pts.len() == 3 && pts[2].p == pt(10.0, 10.0)));
        assert!(matches!(geoms[2], Geom::Poly { closed: true, .. }));
        assert_eq!(geoms[3], &Geom::Circle { c: pt(100.0, 200.0), r: 2.5 });
        match geoms[4] {
            Geom::Arc { c, r, a0, a1 } => assert!(*c == pt(0.0, 0.0) && close(*r, 3.0) && close(*a0, 0.0) && close(*a1, 90.0), "{a0} {a1}"),
            g => panic!("{g:?}"),
        }
        match geoms[5] {
            Geom::Arc { a0, a1, .. } => assert!(close(*a0, -90.0) && close(*a1, 0.0), "{a0} {a1}"),
            g => panic!("{g:?}"),
        }
        match geoms[6] {
            Geom::Text(t) => assert!(t.text == "CH 1+250 Ø" && t.at == pt(5.0, 6.0) && close(t.height, 2.0) && close(t.rotation_deg, 90.0) && close(t.width_factor, 0.8), "{t:?}"),
            g => panic!("{g:?}"),
        }
        match geoms[7] {
            Geom::Text(t) => assert!(t.text == "PI 3" && t.at == pt(7.0, 8.0) && close(t.rotation_deg, 0.0)),
            g => panic!("{g:?}"),
        }
        assert!(d.items.iter().all(|i| i.layer.is_none() && i.colour.is_none()));
    }

    #[test]
    fn layer_colour_and_transforms_apply_to_what_follows() {
        let up = [0.0, 0.0, 1.0];
        // Move by (1000, 2000) then rotate 90 degrees about Z: x' = -y + 1000, y' = x + 2000.
        let m = [0.0, -1.0, 0.0, 1000.0, 1.0, 0.0, 0.0, 2000.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 0.0, 1.0];
        let mut g = Graphics::new();
        g.command(16, &u(&[2]))
            .command(14, &u(&[1]))
            .command(29, &f(&m))
            .command(6, &cat(&[u(&[2]), f(&[1.0, 0.0, 0.0, 2.0, 0.0, 0.0])]))
            .command(2, &cat(&[f(&[3.0, 0.0, 0.0, 1.0]), f(&up)]))
            .command(31, &[])
            .command(22, &u(&[0xC2_10_20_30]))
            .command(16, &u(&[9])) // no such layer: the last one stays
            .command(6, &cat(&[u(&[2]), f(&[1.0, 0.0, 0.0, 2.0, 0.0, 0.0])]))
            .command(22, &u(&[0xC0_00_00_00]))
            .command(6, &cat(&[u(&[1]), f(&[5.0, 5.0, 0.0])]));
        let d = decode(&g.bytes(), 3, true);
        assert!(d.skipped.is_empty(), "{:?}", d.skipped);
        assert_eq!(d.items[0].geom, Geom::Line(pt(1000.0, 2001.0), pt(1000.0, 2002.0)));
        assert_eq!((d.items[0].layer, d.items[0].colour), (Some(2), Some(ColourSpec::Aci(1))));
        assert_eq!(d.items[1].geom, Geom::Circle { c: pt(1000.0, 2003.0), r: 1.0 });
        // After the pop: untransformed, true colour, still layer 2.
        assert_eq!(d.items[2].geom, Geom::Line(pt(1.0, 0.0), pt(2.0, 0.0)));
        assert_eq!((d.items[2].layer, d.items[2].colour), (Some(2), Some(ColourSpec::Rgb([0x10, 0x20, 0x30]))));
        // One point is a point; by layer again.
        assert_eq!((d.items[3].geom.clone(), d.items[3].colour), (Geom::Point(pt(5.0, 5.0)), Some(ColourSpec::ByLayer)));
    }

    #[test]
    fn shells_become_faces_and_unknown_or_tilted_commands_are_counted() {
        let mut g = Graphics::new();
        // A unit square as two triangles; face entry count 8 = 2 × (1 + 3).
        g.command(9, &cat(&[u(&[4]), f(&[0.0, 0.0, 1.0, 1.0, 0.0, 1.0, 1.0, 1.0, 1.0, 0.0, 1.0, 1.0]), u(&[8, 3, 0, 1, 2, 3, 0, 2, 3])]))
            // A circle standing on edge: its plan view is not a circle.
            .command(2, &cat(&[f(&[0.0, 0.0, 0.0, 1.0]), f(&[1.0, 0.0, 0.0])]))
            .command(99, &u(&[0]))
            .command(12, &f(&[0.0, 0.0, 0.0, 1.0, 1.0, 0.0]));
        let d = decode(&g.bytes(), 1, true);
        assert_eq!(d.items.len(), 2);
        assert_eq!(d.items[0].geom, Geom::Solid(vec![pt(0.0, 0.0), pt(1.0, 0.0), pt(1.0, 1.0)]));
        assert_eq!(d.items[1].geom, Geom::Solid(vec![pt(0.0, 0.0), pt(1.0, 1.0), pt(0.0, 1.0)]));
        assert_eq!(d.skipped, vec![("circle (unreadable)".to_string(), 1), ("command 99".to_string(), 1), ("construction line".to_string(), 1)]);
    }

    #[test]
    fn the_stand_in_box_of_an_object_with_no_picture_is_recognised() {
        let up = [0.0, 0.0, 1.0];
        // As Civil 3D wrote it in a real file: text, a five-point box, then type 21.
        let mut g = Graphics::new();
        g.command(36, &cat(&[f(&[2.5, 2.5, 0.0]), f(&up), f(&[1.0, 0.0, 0.0]), f(&[5.0, 1.0, 0.0]), utf16("AeccDbFace (AeccLand130)")]))
            .command(6, &cat(&[u(&[5]), f(&[0.0, 0.0, 0.0, 125.0, 0.0, 0.0, 125.0, 10.0, 0.0, 0.0, 10.0, 0.0, 0.0, 0.0, 0.0])]))
            .command(21, &[]);
        assert!(is_placeholder(&decode(&g.bytes(), 1, true).items));
        // A real label of two items is not one.
        let mut g = Graphics::new();
        g.command(36, &cat(&[f(&[2.5, 2.5, 0.0]), f(&up), f(&[1.0, 0.0, 0.0]), f(&[5.0, 1.0, 0.0]), utf16("PVI STA 0+250.00")]))
            .command(6, &cat(&[u(&[3]), f(&[0.0, 0.0, 0.0, 5.0, 0.0, 0.0, 5.0, 5.0, 0.0])]));
        assert!(!is_placeholder(&decode(&g.bytes(), 1, true).items));
    }

    #[test]
    fn a_cut_short_buffer_keeps_what_came_before() {
        let mut g = Graphics::new();
        g.command(6, &cat(&[u(&[2]), f(&[0.0, 0.0, 0.0, 1.0, 1.0, 0.0])])).command(6, &cat(&[u(&[2]), f(&[2.0, 2.0, 0.0, 3.0, 3.0, 0.0])]));
        let mut b = g.bytes();
        let full = b.len();
        b.truncate(full - 10);
        // The header still says the full size: the reader stops at the broken command.
        let d = decode(&b, 1, true);
        assert!(d.truncated);
        assert_eq!(d.items.len(), 1);
        // A polyline that claims more points than it holds is unreadable, not a crash.
        let mut g = Graphics::new();
        g.command(6, &cat(&[u(&[1_000_000]), f(&[0.0, 0.0, 0.0])]));
        assert_eq!(decode(&g.bytes(), 1, true).skipped, vec![("polyline (unreadable)".to_string(), 1)]);
        assert!(decode(&[1, 2, 3], 1, true).truncated);
    }

    /// A whole file as ODA File Converter writes a Civil 3D object it does not know.
    #[test]
    fn a_proxy_entity_in_a_dxf_is_drawn_on_its_layer_and_counted() {
        let up = [0.0, 0.0, 1.0];
        let mut g = Graphics::new();
        g.command(6, &cat(&[u(&[2]), f(&[213000.0, 9887000.0, 1300.0, 213010.0, 9887000.0, 1300.0])]))
            .command(16, &u(&[1]))
            .command(36, &cat(&[f(&[213000.0, 9887001.0, 0.0]), f(&up), f(&[1.0, 0.0, 0.0]), f(&[1.0, 1.0, 0.0]), utf16("0+000")]))
            .command(99, &u(&[0]));
        let text = format!(
            "0\nSECTION\n2\nHEADER\n9\n$ACADVER\n1\nAC1032\n0\nENDSEC\n\
             0\nSECTION\n2\nTABLES\n0\nTABLE\n2\nLAYER\n0\nLAYER\n2\n0\n70\n0\n62\n7\n0\nLAYER\n2\nC-ROAD-TEXT\n70\n0\n62\n3\n0\nENDTAB\n0\nENDSEC\n\
             0\nSECTION\n2\nENTITIES\n0\nACAD_PROXY_ENTITY\n8\nC-ROAD\n62\n1\n90\n498\n91\n500\n{}93\n32\n310\nDEADBEEF\n0\nACAD_PROXY_ENTITY\n8\nC-ROAD\n90\n498\n0\nENDSEC\n0\nEOF\n",
            g.dxf_groups()
        );
        let under = crate::reader::read_bytes(text.as_bytes()).unwrap();
        assert_eq!(under.entities.len(), 2, "{:?}", under.entities);
        let name = |e: &crate::reader::Entity| under.layers[e.layer as usize].name.clone();
        assert_eq!(under.entities[0].geom, Geom::Line(pt(213000.0, 9887000.0), pt(213010.0, 9887000.0)));
        assert_eq!((name(&under.entities[0]), under.entities[0].colour), ("C-ROAD".to_string(), ColourSpec::Aci(1)));
        assert!(matches!(&under.entities[1].geom, Geom::Text(t) if t.text == "0+000"));
        assert_eq!(name(&under.entities[1]), "C-ROAD-TEXT");
        assert_eq!(under.report.read.get("ACAD_PROXY_ENTITY"), Some(&1));
        assert_eq!(under.report.unread.get("ACAD_PROXY_ENTITY (command 99)"), Some(&1));
        assert_eq!(under.report.unread.get("ACAD_PROXY_ENTITY (no proxy graphics)"), Some(&1));
    }

    #[test]
    fn a_light_weight_polyline_in_the_dwg_bit_stream_is_read() {
        // Written by hand: byte count (raw u32), flag BS = 16 (bulges) as 01 + byte, point count
        // BL = 3 as 01 + byte, bulge count BL = 1 as 01 + byte, first point as raw doubles,
        // second point x as DD 11 + raw double and y as DD 00 (same as before), third point as
        // DD 00 and DD 11 + raw double, one bulge BD 01 (= 1.0).
        let mut bits: Vec<bool> = Vec::new();
        let push = |bits: &mut Vec<bool>, v: u64, n: usize| (0..n).rev().for_each(|k| bits.push(v >> k & 1 == 1));
        let push_bytes = |bits: &mut Vec<bool>, b: &[u8]| b.iter().for_each(|&x| (0..8).rev().for_each(|k| bits.push(x >> k & 1 == 1)));
        push_bytes(&mut bits, &0u32.to_le_bytes());
        push(&mut bits, 0b01, 2);
        push_bytes(&mut bits, &[16]);
        push(&mut bits, 0b01, 2);
        push_bytes(&mut bits, &[3]);
        push(&mut bits, 0b01, 2);
        push_bytes(&mut bits, &[1]);
        push_bytes(&mut bits, &10.0f64.to_le_bytes());
        push_bytes(&mut bits, &20.0f64.to_le_bytes());
        push(&mut bits, 0b11, 2);
        push_bytes(&mut bits, &15.0f64.to_le_bytes());
        push(&mut bits, 0b00, 2);
        push(&mut bits, 0b00, 2);
        push(&mut bits, 0b11, 2);
        push_bytes(&mut bits, &25.0f64.to_le_bytes());
        push(&mut bits, 0b01, 2);
        while bits.len() % 8 != 0 {
            bits.push(false);
        }
        let bytes: Vec<u8> = bits.chunks(8).map(|c| c.iter().fold(0u8, |a, &b| a << 1 | b as u8)).collect();
        let (pts, closed) = lwpolyline(&bytes, true).unwrap();
        assert!(!closed);
        assert_eq!(pts, vec![(pt(10.0, 20.0), 1.0), (pt(15.0, 20.0), 0.0), (pt(15.0, 25.0), 0.0)]);
        // Through `decode`, padded to a multiple of four.
        let mut padded = bytes.clone();
        while padded.len() % 4 != 0 {
            padded.push(0);
        }
        let mut g = Graphics::new();
        g.command(33, &padded);
        let d = decode(&g.bytes(), 1, true);
        assert!(matches!(&d.items[0].geom, Geom::Poly { pts, closed: false } if pts.len() == 3 && pts[0].bulge == 1.0));
    }
}
