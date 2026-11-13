//! Minimal TrueType metric and outline reader for the Transport typefaces. Reads
//! head/maxp/hhea/hmtx/cmap/loca/glyf only. A line-for-line port of C# `FontMetrics`, kept as
//! its own reader (rather than a general font crate) so advances, the measured baseline and
//! the flattened outlines are the same numbers the reference engine produces.
//!
//! App.1 §1.1: characters sit on imaginary tiles whose widths vary by character.
//!
//! Both Transport TTFs are Windows SYMBOL-encoded fonts: every glyph lives in the Private Use
//! Area 0xF020-0xF10A, offset 0xF000 from the ASCII value it represents (e.g. 'K' 0x4B is not
//! in the cmap; 0xF04B maps to the glyph). Glyph lookup therefore tries the raw codepoint
//! first, then 0xF000 + c.
//!
//! OS/2 is version 0 in both fonts and carries no sxHeight field, so the x-height is measured
//! from glyph outlines instead. Both Transport TTFs draw their letters on a baseline raised
//! 214 units (of 2048) above the glyph origin: 'H' runs from 214 to 1742 and 'x' from 214 to
//! 1306. The baseline is therefore read from the flat foot of 'H' (falling back to the foot
//! of 'x'), and the x-height is the 'x' top minus that baseline (1092 units), which makes
//! capitals 1528 / 1092 = 1.40 x-heights as the TSM requires. For a conventional font the
//! baseline is 0 and nothing changes.

use std::collections::HashMap;
use std::path::Path;

use crate::SignError;

/// Straight segments each TrueType quadratic curve is flattened into (the C# default).
pub const SEGMENTS_PER_CURVE: usize = 8;

#[derive(Debug, Clone)]
pub struct FontMetrics {
    path: String,
    /// UTF-16 code unit to glyph id.
    cmap: HashMap<u16, u16>,
    /// Indexed by glyph id.
    advance_widths: Vec<u16>,
    data: Vec<u8>,
    loca: usize,
    glyf: usize,
    num_glyphs: usize,
    index_to_loc_format: i16,
    units_per_em: f64,
    x_height_em: f64,
    baseline_em: f64,
}

/// The affine transform (a, b, c, d, dx, dy) that maps (x, y) to (a·x + c·y + dx, b·x + d·y + dy).
#[derive(Clone, Copy)]
struct Affine {
    a: f64,
    b: f64,
    c: f64,
    d: f64,
    dx: f64,
    dy: f64,
}

impl Affine {
    fn map(&self, x: f64, y: f64) -> (f64, f64) {
        (
            self.a * x + self.c * y + self.dx,
            self.b * x + self.d * y + self.dy,
        )
    }
}

type RawContour = Vec<(f64, f64, bool)>;

impl FontMetrics {
    pub fn units_per_em(&self) -> f64 {
        self.units_per_em
    }

    /// x-height as a fraction of the em square: the top of the 'x' glyph minus the baseline.
    /// This is the length that is 4 sw, so it sets the scale of both glyph outlines and
    /// advance widths.
    pub fn x_height_em(&self) -> f64 {
        self.x_height_em
    }

    /// Height of the lettering baseline above the glyph origin, in em: the foot of 'H' (or of
    /// 'x' when the font has no 'H'). Zero for conventional fonts; the Transport TTFs place it
    /// 214/2048 em up. Outlines are placed so this sits on the run's baseline.
    pub fn baseline_em(&self) -> f64 {
        self.baseline_em
    }

    pub fn load(ttf_path: &Path) -> Result<FontMetrics, SignError> {
        let shown = ttf_path.display().to_string();
        let data = std::fs::read(ttf_path)
            .map_err(|e| SignError::Font(format!("Font not found: {shown} ({e})")))?;
        Self::from_bytes(data, &shown)
    }

    /// Parses a TrueType file held in memory; `path` only names the font in error messages.
    pub fn from_bytes(d: Vec<u8>, path: &str) -> Result<FontMetrics, SignError> {
        // Every read below is bounds-checked; a short or corrupt file is reported, not a panic.
        Self::parse(d, path)
            .ok_or_else(|| SignError::Font(format!("Not a readable TrueType font: {path}")))?
    }

    fn parse(d: Vec<u8>, path: &str) -> Option<Result<FontMetrics, SignError>> {
        let tables = read_table_directory(&d)?;
        let table = |tag: &str| tables.get(tag).map(|o| *o as usize);
        let (
            Some(head),
            Some(maxp),
            Some(hhea),
            Some(hmtx),
            Some(cmap_off),
            Some(loca),
            Some(glyf),
        ) = (
            table("head"),
            table("maxp"),
            table("hhea"),
            table("hmtx"),
            table("cmap"),
            table("loca"),
            table("glyf"),
        )
        else {
            return Some(Err(SignError::Font(format!(
                "Font lacks a required TrueType table: {path}"
            ))));
        };

        let units_per_em = f64::from(u16_at(&d, head + 18)?);
        let index_to_loc_format = i16_at(&d, head + 50)?;
        let num_glyphs = usize::from(u16_at(&d, maxp + 4)?);
        let number_of_h_metrics = usize::from(u16_at(&d, hhea + 34)?);

        let mut advance_widths = Vec::with_capacity(number_of_h_metrics);
        for i in 0..number_of_h_metrics {
            advance_widths.push(u16_at(&d, hmtx + i * 4)?);
        }

        let cmap = match read_cmap(&d, cmap_off)? {
            Ok(c) => c,
            Err(e) => return Some(Err(SignError::Font(format!("{e}: {path}")))),
        };

        let mut font = FontMetrics {
            path: path.to_string(),
            cmap,
            advance_widths,
            data: d,
            loca,
            glyf,
            num_glyphs,
            index_to_loc_format,
            units_per_em,
            x_height_em: 0.0,
            baseline_em: 0.0,
        };

        let Some((x_bottom_em, x_top_em)) = font.measure_glyph_y_em('x') else {
            return Some(Err(SignError::Font(format!(
                "No outline for 'x', cannot measure x-height: {path}"
            ))));
        };
        // The baseline is the flat foot of 'H'; 'x' is the fallback (its foot may sit a few
        // units off the baseline, as Transport Medium's does: 218 against 214).
        let baseline_em = font
            .measure_glyph_y_em('H')
            .map(|(y_min, _)| y_min)
            .unwrap_or(x_bottom_em);
        font.x_height_em = x_top_em - baseline_em;
        font.baseline_em = baseline_em;
        if font.x_height_em.is_nan() || font.x_height_em <= 0.0 || units_per_em <= 0.0 {
            return Some(Err(SignError::Font(format!(
                "Font has no usable x-height: {path}"
            ))));
        }
        Some(Ok(font))
    }

    /// Resolves a char to a glyph id, trying the raw codepoint first and then falling back to
    /// the Windows-symbol PUA offset (0xF000 + c) for chars in 0x20-0xFF. Both Transport TTFs
    /// only populate the PUA range.
    fn glyph(&self, c: char) -> Option<u16> {
        let code = u16::try_from(c as u32).ok()?;
        if let Some(g) = self.cmap.get(&code) {
            return Some(*g);
        }
        if (0x20..=0xFF).contains(&code) {
            return self.cmap.get(&(0xF000 + code)).copied();
        }
        None
    }

    pub fn has_glyph(&self, c: char) -> bool {
        c == ' ' || self.glyph(c).is_some()
    }

    fn missing(&self, c: char) -> SignError {
        SignError::MissingGlyph {
            character: c,
            font: self.path.clone(),
        }
    }

    /// Advance width as a fraction of the em square. An error for a character with no glyph in
    /// the font: silent zero-width text would produce grossly undersized signs, so a missing
    /// glyph must be loud, not defaulted to 0.
    pub fn advance_width_em(&self, c: char) -> Result<f64, SignError> {
        let gid = usize::from(self.glyph(c).ok_or_else(|| self.missing(c))?);
        let idx = gid.min(self.advance_widths.len().saturating_sub(1));
        let advance = self.advance_widths.get(idx).copied().unwrap_or(0);
        Ok(f64::from(advance) / self.units_per_em)
    }

    /// Advance width in sw. `x_height_em` is the font's x-height as a fraction of the em
    /// (normally [`FontMetrics::x_height_em`]); sw = x-height / 4, so 1 em = 4 / x_height_em
    /// stroke widths.
    pub fn advance_width_sw(&self, c: char, x_height_em: f64) -> Result<f64, SignError> {
        Ok(self.advance_width_em(c)? * (4.0 / x_height_em))
    }

    /// The bounding-box yMin and yMax of a glyph's outline, divided by unitsPerEm, or `None`
    /// when the font has no glyph or no outline for it.
    fn measure_glyph_y_em(&self, c: char) -> Option<(f64, f64)> {
        let gid = usize::from(self.glyph(c)?);
        let (glyph_offset, next_offset) = self.loca_pair(gid)?;
        if glyph_offset == next_offset {
            return None;
        }
        let record_start = self.glyf + glyph_offset;
        let y_min = i16_at(&self.data, record_start + 4)?;
        let y_max = i16_at(&self.data, record_start + 8)?;
        Some((
            f64::from(y_min) / self.units_per_em,
            f64::from(y_max) / self.units_per_em,
        ))
    }

    fn loca_pair(&self, gid: usize) -> Option<(usize, usize)> {
        if self.index_to_loc_format == 0 {
            // short format: offsets are stored /2 as uint16
            Some((
                usize::from(u16_at(&self.data, self.loca + gid * 2)?) * 2,
                usize::from(u16_at(&self.data, self.loca + (gid + 1) * 2)?) * 2,
            ))
        } else {
            // long format: offsets are stored directly as uint32
            Some((
                u32_at(&self.data, self.loca + gid * 4)? as usize,
                u32_at(&self.data, self.loca + (gid + 1) * 4)? as usize,
            ))
        }
    }

    // ─── glyph outlines ───────────────────────────────────────────────────────────

    /// The closed outline contours of a character's glyph, in em units with y up and the
    /// origin at the glyph's origin. TrueType quadratic curves are flattened into
    /// [`SEGMENTS_PER_CURVE`] straight segments. A space or a glyph with no outline returns no
    /// contours; a character missing from the font is an error, for the same reason
    /// [`FontMetrics::advance_width_em`] is.
    pub fn glyph_contours_em(&self, c: char) -> Result<Vec<Vec<(f64, f64)>>, SignError> {
        self.glyph_contours_em_with(c, SEGMENTS_PER_CURVE)
    }

    pub fn glyph_contours_em_with(
        &self,
        c: char,
        segments_per_curve: usize,
    ) -> Result<Vec<Vec<(f64, f64)>>, SignError> {
        if c == ' ' {
            return Ok(Vec::new());
        }
        let gid = self.glyph(c).ok_or_else(|| self.missing(c))?;
        let mut raw: Vec<RawContour> = Vec::new();
        let identity = Affine {
            a: 1.0,
            b: 0.0,
            c: 0.0,
            d: 1.0,
            dx: 0.0,
            dy: 0.0,
        };
        self.read_glyph(usize::from(gid), identity, &mut raw, 0)
            .ok_or_else(|| {
                SignError::Font(format!("Corrupt glyph for '{c}' in font '{}'", self.path))
            })?;
        Ok(raw
            .iter()
            .filter(|contour| contour.len() >= 2)
            .map(|contour| {
                flatten(contour, segments_per_curve)
                    .into_iter()
                    .map(|(x, y)| (x / self.units_per_em, y / self.units_per_em))
                    .collect()
            })
            .collect())
    }

    fn glyph_range(&self, gid: usize) -> Option<(usize, usize)> {
        if gid >= self.num_glyphs {
            return Some((0, 0));
        }
        let (start, end) = self.loca_pair(gid)?;
        Some((self.glyf + start, self.glyf + end))
    }

    /// Reads a simple or composite glyph's points, applying the affine transform `t`.
    /// `None` means the glyph data ran past the end of the file or nests too deeply.
    fn read_glyph(
        &self,
        gid: usize,
        t: Affine,
        into: &mut Vec<RawContour>,
        depth: u32,
    ) -> Option<()> {
        if depth > 8 {
            return None; // composite glyph nesting too deep
        }
        let (start, end) = self.glyph_range(gid)?;
        if end <= start {
            return Some(()); // no outline (e.g. space)
        }

        let d = &self.data;
        let contours = i16_at(d, start)?;
        let mut pos = start + 10;

        if contours >= 0 {
            let contours = contours as usize;
            let mut end_pts = Vec::with_capacity(contours);
            for _ in 0..contours {
                end_pts.push(usize::from(u16_at(d, pos)?));
                pos += 2;
            }
            let count = end_pts.last().map_or(0, |last| last + 1);
            let instruction_length = usize::from(u16_at(d, pos)?);
            pos += 2 + instruction_length;

            let mut flags = vec![0u8; count];
            let mut i = 0;
            while i < count {
                let f = *d.get(pos)?;
                pos += 1;
                flags[i] = f;
                i += 1;
                if f & 0x08 != 0 {
                    let mut r = *d.get(pos)?;
                    pos += 1;
                    while r > 0 && i < count {
                        flags[i] = f;
                        i += 1;
                        r -= 1;
                    }
                }
            }

            let mut xs = vec![0i32; count];
            let mut ys = vec![0i32; count];
            let mut v = 0i32;
            for i in 0..count {
                let f = flags[i];
                if f & 0x02 != 0 {
                    let dx = i32::from(*d.get(pos)?);
                    pos += 1;
                    v += if f & 0x10 != 0 { dx } else { -dx };
                } else if f & 0x10 == 0 {
                    v += i32::from(i16_at(d, pos)?);
                    pos += 2;
                }
                xs[i] = v;
            }
            v = 0;
            for i in 0..count {
                let f = flags[i];
                if f & 0x04 != 0 {
                    let dy = i32::from(*d.get(pos)?);
                    pos += 1;
                    v += if f & 0x20 != 0 { dy } else { -dy };
                } else if f & 0x20 == 0 {
                    v += i32::from(i16_at(d, pos)?);
                    pos += 2;
                }
                ys[i] = v;
            }

            let mut first = 0;
            for last in end_pts {
                let mut contour = RawContour::new();
                for i in first..=last {
                    let (mx, my) = t.map(f64::from(*xs.get(i)?), f64::from(ys[i]));
                    contour.push((mx, my, flags[i] & 0x01 != 0));
                }
                into.push(contour);
                first = last + 1;
            }
            return Some(());
        }

        // Composite glyph: a list of transformed component glyphs.
        const ARG_1_AND_2_ARE_WORDS: u16 = 0x0001;
        const ARGS_ARE_XY_VALUES: u16 = 0x0002;
        const WE_HAVE_A_SCALE: u16 = 0x0008;
        const MORE_COMPONENTS: u16 = 0x0020;
        const WE_HAVE_X_AND_Y_SCALE: u16 = 0x0040;
        const WE_HAVE_TWO_BY_TWO: u16 = 0x0080;
        loop {
            let comp_flags = u16_at(d, pos)?;
            let component = usize::from(u16_at(d, pos + 2)?);
            pos += 4;
            let (arg1, arg2);
            if comp_flags & ARG_1_AND_2_ARE_WORDS != 0 {
                arg1 = f64::from(i16_at(d, pos)?);
                arg2 = f64::from(i16_at(d, pos + 2)?);
                pos += 4;
            } else {
                let (b1, b2) = (*d.get(pos)?, *d.get(pos + 1)?);
                if comp_flags & ARGS_ARE_XY_VALUES != 0 {
                    arg1 = f64::from(b1 as i8);
                    arg2 = f64::from(b2 as i8);
                } else {
                    arg1 = f64::from(b1);
                    arg2 = f64::from(b2);
                }
                pos += 2;
            }
            let (mut a, mut b, mut c, mut dd) = (1.0, 0.0, 0.0, 1.0);
            let f2dot14 = |at: usize| i16_at(d, at).map(|v| f64::from(v) / 16384.0);
            if comp_flags & WE_HAVE_A_SCALE != 0 {
                a = f2dot14(pos)?;
                dd = a;
                pos += 2;
            } else if comp_flags & WE_HAVE_X_AND_Y_SCALE != 0 {
                a = f2dot14(pos)?;
                dd = f2dot14(pos + 2)?;
                pos += 4;
            } else if comp_flags & WE_HAVE_TWO_BY_TWO != 0 {
                a = f2dot14(pos)?;
                b = f2dot14(pos + 2)?;
                c = f2dot14(pos + 4)?;
                dd = f2dot14(pos + 6)?;
                pos += 8;
            }
            // Point-matched placement (args are point numbers) is not used by the Transport
            // fonts; such a component is placed without an offset.
            let (ox, oy) = if comp_flags & ARGS_ARE_XY_VALUES != 0 {
                (arg1, arg2)
            } else {
                (0.0, 0.0)
            };

            // Compose: component transform first, then the parent's.
            let (cdx, cdy) = t.map(ox, oy);
            let composed = Affine {
                a: t.a * a + t.c * b,
                b: t.b * a + t.d * b,
                c: t.a * c + t.c * dd,
                d: t.b * c + t.d * dd,
                dx: cdx,
                dy: cdy,
            };
            self.read_glyph(component, composed, into, depth + 1)?;
            if comp_flags & MORE_COMPONENTS == 0 {
                break;
            }
        }
        Some(())
    }
}

fn u16_at(d: &[u8], at: usize) -> Option<u16> {
    Some(u16::from_be_bytes([*d.get(at)?, *d.get(at + 1)?]))
}

fn i16_at(d: &[u8], at: usize) -> Option<i16> {
    u16_at(d, at).map(|v| v as i16)
}

fn u32_at(d: &[u8], at: usize) -> Option<u32> {
    Some(u32::from_be_bytes([
        *d.get(at)?,
        *d.get(at + 1)?,
        *d.get(at + 2)?,
        *d.get(at + 3)?,
    ]))
}

fn read_table_directory(d: &[u8]) -> Option<HashMap<String, u32>> {
    let num_tables = usize::from(u16_at(d, 4)?);
    let mut tables = HashMap::with_capacity(num_tables);
    for i in 0..num_tables {
        let rec = 12 + i * 16;
        let tag = String::from_utf8_lossy(d.get(rec..rec + 4)?).into_owned();
        tables.insert(tag, u32_at(d, rec + 8)?);
    }
    Some(tables)
}

/// Reads a format 4 cmap subtable, preferring platform 3 (Windows) but accepting platform 0
/// (Unicode) too. The outer `None` is a read past the end of the file.
fn read_cmap(d: &[u8], cmap_offset: usize) -> Option<Result<HashMap<u16, u16>, String>> {
    let num_subtables = usize::from(u16_at(d, cmap_offset + 2)?);
    let mut best = None;
    for i in 0..num_subtables {
        let rec = cmap_offset + 4 + i * 8;
        let platform = u16_at(d, rec)?;
        let encoding = u16_at(d, rec + 2)?;
        let offset = u32_at(d, rec + 4)? as usize;
        // Windows symbol (3,0) and Windows Unicode BMP (3,1) are the primary targets for the
        // Transport fonts; also accept platform 0 (Unicode). The last match wins.
        if (platform == 3 && (encoding == 0 || encoding == 1)) || platform == 0 {
            best = Some(cmap_offset + offset);
        }
    }
    let Some(best) = best else {
        return Some(Err("No Windows or Unicode cmap subtable found".to_string()));
    };

    let format = u16_at(d, best)?;
    if format != 4 {
        return Some(Err(format!("Unsupported cmap format {format}")));
    }

    let seg_count_x2 = usize::from(u16_at(d, best + 6)?);
    let seg_count = seg_count_x2 / 2;
    let end_off = best + 14;
    let start_off = end_off + seg_count_x2 + 2;
    let delta_off = start_off + seg_count_x2;
    let range_off = delta_off + seg_count_x2;

    let mut map = HashMap::new();
    for s in 0..seg_count {
        let end = u16_at(d, end_off + s * 2)?;
        let start = u16_at(d, start_off + s * 2)?;
        let delta = i16_at(d, delta_off + s * 2)?;
        let range = usize::from(u16_at(d, range_off + s * 2)?);
        if start == 0xFFFF {
            continue;
        }

        let mut c = u32::from(start);
        while c <= u32::from(end) && c != 0xFFFF {
            let code = c as u16;
            let gid = if range == 0 {
                code.wrapping_add(delta as u16)
            } else {
                let gi = range_off + s * 2 + range + (c - u32::from(start)) as usize * 2;
                match u16_at(d, gi) {
                    None => {
                        c += 1;
                        continue;
                    }
                    Some(0) => 0,
                    Some(g) => g.wrapping_add(delta as u16),
                }
            };
            if gid != 0 {
                map.insert(code, gid);
            }
            c += 1;
        }
    }
    Some(Ok(map))
}

/// Turns one TrueType contour (on- and off-curve points) into a polyline. Two off-curve
/// points in a row imply an on-curve point midway between them.
fn flatten(pts: &[(f64, f64, bool)], segments: usize) -> Vec<(f64, f64)> {
    // Start on an on-curve point; if there is none, on the midpoint of the first two.
    let n = pts.len();
    let mut ring: Vec<(f64, f64, bool)> = Vec::with_capacity(n + 1);
    match pts.iter().position(|p| p.2) {
        Some(on_idx) => {
            for i in 0..n {
                ring.push(pts[(on_idx + i) % n]);
            }
        }
        None => {
            ring.push((
                (pts[0].0 + pts[1].0) / 2.0,
                (pts[0].1 + pts[1].1) / 2.0,
                true,
            ));
            for i in 1..=n {
                ring.push(pts[i % n]);
            }
        }
    }

    let mut out = vec![(ring[0].0, ring[0].1)];
    let mut cur = (ring[0].0, ring[0].1);
    let mut control: Option<(f64, f64)> = None;
    for i in 1..=ring.len() {
        let p = ring[i % ring.len()]; // the last step closes back onto the start
        if p.2 {
            match control {
                Some(ctl) => quad(&mut out, cur, ctl, (p.0, p.1), segments),
                None => out.push((p.0, p.1)),
            }
            cur = (p.0, p.1);
            control = None;
        } else {
            if let Some(ctl) = control {
                let mid = ((ctl.0 + p.0) / 2.0, (ctl.1 + p.1) / 2.0);
                quad(&mut out, cur, ctl, mid, segments);
                cur = mid;
            }
            control = Some((p.0, p.1));
        }
    }
    if out.len() > 1 && out[out.len() - 1] == out[0] {
        out.pop();
    }
    out
}

fn quad(
    into: &mut Vec<(f64, f64)>,
    p0: (f64, f64),
    c: (f64, f64),
    p1: (f64, f64),
    segments: usize,
) {
    for i in 1..=segments {
        let t = i as f64 / segments as f64;
        let u = 1.0 - t;
        into.push((
            u * u * p0.0 + 2.0 * u * t * c.0 + t * t * p1.0,
            u * u * p0.1 + 2.0 * u * t * c.1 + t * t * p1.1,
        ));
    }
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;

    pub(crate) fn fonts_dir() -> std::path::PathBuf {
        Path::new(env!("CARGO_MANIFEST_DIR")).join("../../../fonts")
    }

    pub(crate) fn medium() -> FontMetrics {
        FontMetrics::load(&fonts_dir().join("tranm___.TTF")).expect("Transport Medium loads")
    }

    pub(crate) fn heavy() -> FontMetrics {
        FontMetrics::load(&fonts_dir().join("tranh___.TTF")).expect("Transport Heavy loads")
    }

    /// The Transport TTFs draw on a baseline 214/2048 em up; x runs to 1306.
    #[test]
    fn transport_baseline_and_x_height_are_measured_from_the_outlines() {
        for f in [medium(), heavy()] {
            assert_eq!(f.units_per_em(), 2048.0);
            assert_eq!(f.baseline_em(), 214.0 / 2048.0);
            assert_eq!(f.x_height_em(), 1092.0 / 2048.0);
        }
    }

    #[test]
    fn symbol_encoded_glyphs_resolve_through_the_private_use_area() {
        let f = medium();
        assert!(f.has_glyph('K') && f.has_glyph(' ') && !f.has_glyph('\u{4E00}'));
        assert!(f.advance_width_em('K').unwrap() > 0.0);
        assert!(matches!(
            f.advance_width_em('\u{4E00}'),
            Err(SignError::MissingGlyph {
                character: '\u{4E00}',
                ..
            })
        ));
        assert!(matches!(
            f.glyph_contours_em('\u{1F600}'),
            Err(SignError::MissingGlyph { .. })
        ));
    }

    /// Capitals are 1.4 x-heights tall (5.6 sw on a 4 sw x-height).
    #[test]
    fn capital_h_is_one_point_four_x_heights() {
        let f = medium();
        let contours = f.glyph_contours_em('H').unwrap();
        assert_eq!(contours.len(), 1);
        let ys: Vec<f64> = contours[0].iter().map(|p| p.1).collect();
        let (lo, hi) = (
            ys.iter().cloned().fold(f64::MAX, f64::min),
            ys.iter().cloned().fold(f64::MIN, f64::max),
        );
        assert_eq!(lo, f.baseline_em());
        assert!(((hi - lo) / f.x_height_em() - 1.4).abs() < 0.005);
    }

    #[test]
    fn an_o_has_an_outer_contour_and_a_counter_and_space_has_none() {
        let f = medium();
        assert_eq!(f.glyph_contours_em('o').unwrap().len(), 2);
        assert!(f.glyph_contours_em(' ').unwrap().is_empty());
    }

    #[test]
    fn curves_are_flattened_into_the_asked_number_of_segments() {
        // on, off, on, on: one quadratic then two straight edges (the last closes the ring).
        let pts = [
            (0.0, 0.0, true),
            (1.0, 1.0, false),
            (2.0, 0.0, true),
            (1.0, -1.0, true),
        ];
        let out = flatten(&pts, 4);
        assert_eq!(out.len(), 1 + 4 + 1);
        assert_eq!(out[2], (1.0, 0.5)); // the curve's midpoint
        assert_eq!(out[4], (2.0, 0.0));
    }

    #[test]
    fn two_off_curve_points_imply_a_midpoint() {
        let pts = [
            (0.0, 0.0, true),
            (0.0, 2.0, false),
            (2.0, 2.0, false),
            (2.0, 0.0, true),
        ];
        let out = flatten(&pts, 2);
        // start, 2 points of the first curve (ending on the implied (1,2)), 2 of the second.
        assert_eq!(
            out,
            vec![(0.0, 0.0), (0.25, 1.5), (1.0, 2.0), (1.75, 1.5), (2.0, 0.0)]
        );
    }

    #[test]
    fn a_truncated_file_is_an_error_not_a_panic() {
        let bytes = std::fs::read(fonts_dir().join("tranm___.TTF")).unwrap();
        assert!(matches!(
            FontMetrics::from_bytes(bytes[..200].to_vec(), "cut.ttf"),
            Err(SignError::Font(_))
        ));
        assert!(matches!(
            FontMetrics::from_bytes(Vec::new(), "empty.ttf"),
            Err(SignError::Font(_))
        ));
    }
}
