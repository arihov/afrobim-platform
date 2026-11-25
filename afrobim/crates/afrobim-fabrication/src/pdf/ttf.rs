// SPDX-License-Identifier: LicenseRef-AfroBIM-Proprietary
// Copyright (c) 2026 AfroBIM. All rights reserved.
//! The little of TrueType the PDF writer needs: character-to-glyph mapping, advance widths, the
//! metrics a PDF font descriptor quotes, and a subset font program holding only the glyphs a
//! document uses.
//!
//! The subset is built the way PDFsharp 6.2.4 builds it, so the embedded font program is the
//! same file byte for byte: the tables `cmap`, `cvt `, `fpgm`, `head`, `hhea`, `hmtx`, `maxp` and
//! `prep` are copied unchanged, `glyf` keeps only the used glyphs (glyph numbers do not change),
//! and `loca` is rewritten to match.

use std::collections::{BTreeMap, BTreeSet, HashMap};

use crate::FabricationError;

/// A parsed TrueType font file.
#[derive(Debug, Clone)]
pub struct TrueTypeFont {
    data: Vec<u8>,
    /// Tag → (offset, length) of each table.
    tables: BTreeMap<[u8; 4], (usize, usize)>,
    /// Tag → the checksum the font's own table directory gives.
    checksums: BTreeMap<[u8; 4], u32>,
    units_per_em: u16,
    /// Unicode (BMP) → glyph, from the Windows Unicode `cmap` subtable.
    cmap: HashMap<u16, u16>,
    advances: Vec<u16>,
    /// Offsets into `glyf`, one more than there are glyphs.
    loca: Vec<u32>,
    short_loca: bool,
    family: String,
    win_ascent: u16,
    win_descent: u16,
    cap_height: i16,
    x_height: i16,
    bbox: [i16; 4],
}

fn bad(what: &str) -> FabricationError {
    FabricationError::Font(format!("not a usable TrueType font: {what}"))
}

fn u16_at(d: &[u8], at: usize) -> Result<u16, FabricationError> {
    d.get(at..at + 2).map(|b| u16::from_be_bytes([b[0], b[1]])).ok_or_else(|| bad("truncated table"))
}

fn i16_at(d: &[u8], at: usize) -> Result<i16, FabricationError> {
    u16_at(d, at).map(|v| v as i16)
}

fn u32_at(d: &[u8], at: usize) -> Result<u32, FabricationError> {
    d.get(at..at + 4).map(|b| u32::from_be_bytes([b[0], b[1], b[2], b[3]])).ok_or_else(|| bad("truncated table"))
}

impl TrueTypeFont {
    /// Reads a font file. It must have the tables a text font has (`OS/2`, `name`, a Windows
    /// Unicode `cmap`) besides its outlines.
    pub fn parse(data: Vec<u8>) -> Result<TrueTypeFont, FabricationError> {
        Self::read(data, true)
    }

    /// Reads an embedded font program, which has no `OS/2` or `name` table.
    #[cfg(test)]
    fn parse_program(data: Vec<u8>) -> Result<TrueTypeFont, FabricationError> {
        Self::read(data, false)
    }

    fn read(data: Vec<u8>, complete: bool) -> Result<TrueTypeFont, FabricationError> {
        let num_tables = u16_at(&data, 4)? as usize;
        let mut tables = BTreeMap::new();
        let mut checksums = BTreeMap::new();
        for i in 0..num_tables {
            let rec = 12 + 16 * i;
            let tag: [u8; 4] = data.get(rec..rec + 4).ok_or_else(|| bad("truncated table directory"))?.try_into().expect("four bytes");
            let (offset, length) = (u32_at(&data, rec + 8)? as usize, u32_at(&data, rec + 12)? as usize);
            if offset.checked_add(length).is_none_or(|end| end > data.len()) {
                return Err(bad("a table lies outside the file"));
            }
            tables.insert(tag, (offset, length));
            checksums.insert(tag, u32_at(&data, rec + 4)?);
        }
        let table = |tag: &[u8; 4]| -> Result<&[u8], FabricationError> {
            let (o, l) = *tables.get(tag).ok_or_else(|| bad(&format!("no '{}' table", String::from_utf8_lossy(tag))))?;
            Ok(&data[o..o + l])
        };

        let head = table(b"head")?;
        let units_per_em = u16_at(head, 18)?;
        if units_per_em == 0 {
            return Err(bad("unitsPerEm is zero"));
        }
        let bbox = [i16_at(head, 36)?, i16_at(head, 38)?, i16_at(head, 40)?, i16_at(head, 42)?];
        let short_loca = i16_at(head, 50)? == 0;

        let num_glyphs = u16_at(table(b"maxp")?, 4)? as usize;
        let number_of_h_metrics = u16_at(table(b"hhea")?, 34)? as usize;
        let hmtx = table(b"hmtx")?;
        let advances = (0..number_of_h_metrics).map(|i| u16_at(hmtx, 4 * i)).collect::<Result<Vec<_>, _>>()?;
        if advances.is_empty() {
            return Err(bad("no horizontal metrics"));
        }

        let loca_table = table(b"loca")?;
        let loca = (0..=num_glyphs)
            .map(|i| if short_loca { u16_at(loca_table, 2 * i).map(|v| 2 * u32::from(v)) } else { u32_at(loca_table, 4 * i) })
            .collect::<Result<Vec<_>, _>>()?;
        let glyf_len = tables.get(b"glyf").ok_or_else(|| bad("no 'glyf' table"))?.1;
        if loca.windows(2).any(|w| w[1] < w[0]) || loca.last().is_some_and(|&end| end as usize > glyf_len) {
            return Err(bad("the glyph index does not match the glyph table"));
        }

        let (mut win_ascent, mut win_descent, mut x_height, mut cap_height, mut family) = (0, 0, 0, 0, String::new());
        if complete {
            let os2 = table(b"OS/2")?;
            (win_ascent, win_descent) = (u16_at(os2, 74)?, u16_at(os2, 76)?);
            if u16_at(os2, 0)? >= 2 && os2.len() >= 90 {
                (x_height, cap_height) = (i16_at(os2, 86)?, i16_at(os2, 88)?);
            }
            family = parse_family(table(b"name")?)?;
        }

        Ok(TrueTypeFont {
            cmap: parse_cmap(table(b"cmap")?)?,
            family,
            units_per_em,
            advances,
            loca,
            short_loca,
            win_ascent,
            win_descent,
            cap_height,
            x_height,
            bbox,
            tables,
            checksums,
            data,
        })
    }

    pub fn units_per_em(&self) -> u16 {
        self.units_per_em
    }

    /// The Windows family name (name id 1), e.g. "Key Transport Medium".
    pub fn family(&self) -> &str {
        &self.family
    }

    /// The glyph for a UTF-16 code unit; 0 (the missing glyph) when the font has none.
    pub fn glyph(&self, ch: u16) -> u16 {
        self.cmap.get(&ch).copied().unwrap_or(0)
    }

    /// Advance width of a glyph in font units.
    pub fn advance(&self, glyph: u16) -> u16 {
        *self.advances.get(glyph as usize).unwrap_or(self.advances.last().expect("at least one metric"))
    }

    /// Width of `text` in font units, as PDFsharp measures it: every UTF-16 unit from U+0020 up,
    /// through the cmap, summed.
    pub fn text_width_units(&self, text: &str) -> i64 {
        text.encode_utf16().filter(|&c| c >= 32).map(|c| i64::from(self.advance(self.glyph(c)))).sum()
    }

    /// A value in font units as thousandths of an em, rounded (font descriptor entries). The
    /// Transport faces have no value that lands on a half, so how PDFsharp rounds one is not
    /// known; halves round away from zero here.
    fn milli(&self, v: i32) -> i32 {
        (f64::from(v) * 1000.0 / f64::from(self.units_per_em)).round() as i32
    }

    pub fn ascent(&self) -> i32 {
        self.milli(i32::from(self.win_ascent))
    }
    pub fn descent(&self) -> i32 {
        self.milli(i32::from(self.win_descent))
    }
    pub fn cap_height(&self) -> i32 {
        self.milli(i32::from(self.cap_height))
    }
    pub fn x_height(&self) -> i32 {
        self.milli(i32::from(self.x_height))
    }
    pub fn bbox(&self) -> [i32; 4] {
        self.bbox.map(|v| self.milli(i32::from(v)))
    }

    /// Advance widths for the WinAnsi codes 32..=255 in thousandths of an em, truncated.
    pub fn win_ansi_widths(&self) -> Vec<i32> {
        (32u8..=255).map(|code| i32::from(self.advance(self.glyph(win_ansi_to_unicode(code)))) * 1000 / i32::from(self.units_per_em)).collect()
    }

    fn glyph_data(&self, glyph: u16) -> &[u8] {
        let (start, _) = self.tables[b"glyf"];
        let g = glyph as usize;
        if g + 1 >= self.loca.len() {
            return &[];
        }
        &self.data[start + self.loca[g] as usize..start + self.loca[g + 1] as usize]
    }

    /// Adds the components of composite glyphs, recursively. No character of the Transport
    /// faces is a composite glyph, so this is the one part of the subset that the comparison
    /// with PDFsharp's font programs does not exercise.
    fn close_over_components(&self, glyphs: &mut BTreeSet<u16>) {
        let mut pending: Vec<u16> = glyphs.iter().copied().collect();
        while let Some(g) = pending.pop() {
            let d = self.glyph_data(g);
            if d.len() < 10 || i16::from_be_bytes([d[0], d[1]]) >= 0 {
                continue;
            }
            let mut at = 10;
            while let (Ok(flags), Ok(component)) = (u16_at(d, at), u16_at(d, at + 2)) {
                if glyphs.insert(component) {
                    pending.push(component);
                }
                at += 4 + if flags & 0x0001 != 0 { 4 } else { 2 };
                at += if flags & 0x0008 != 0 {
                    2
                } else if flags & 0x0040 != 0 {
                    4
                } else if flags & 0x0080 != 0 {
                    8
                } else {
                    0
                };
                if flags & 0x0020 == 0 {
                    break;
                }
            }
        }
    }

    /// A font program with only `used` glyphs (plus the missing glyph and the components of
    /// composites). Glyph numbers are unchanged.
    pub fn subset(&self, used: &BTreeSet<u16>) -> Vec<u8> {
        let num_glyphs = self.loca.len() - 1;
        let mut glyphs: BTreeSet<u16> = used.iter().copied().filter(|&g| (g as usize) < num_glyphs).collect();
        glyphs.insert(0);
        self.close_over_components(&mut glyphs);

        let mut glyf = Vec::new();
        let mut offsets = Vec::with_capacity(num_glyphs + 1);
        for g in 0..num_glyphs {
            offsets.push(glyf.len() as u32);
            if glyphs.contains(&(g as u16)) {
                glyf.extend_from_slice(self.glyph_data(g as u16));
            }
        }
        offsets.push(glyf.len() as u32);
        let mut loca = Vec::new();
        for o in offsets {
            if self.short_loca {
                loca.extend_from_slice(&((o / 2) as u16).to_be_bytes());
            } else {
                loca.extend_from_slice(&o.to_be_bytes());
            }
        }

        let copy = |tag: &[u8; 4]| self.tables.get(tag).map(|&(o, l)| self.data[o..o + l].to_vec());
        // A copied table keeps the checksum of the font's own directory (for `head` that is not
        // the plain sum of its bytes); the two rebuilt tables get a fresh one.
        let mut out_tables: Vec<([u8; 4], u32, Vec<u8>)> = Vec::new();
        for tag in [b"cmap", b"cvt ", b"fpgm", b"glyf", b"head", b"hhea", b"hmtx", b"loca", b"maxp", b"prep"] {
            let table = match tag {
                b"glyf" => Some((checksum(&glyf), glyf.clone())),
                b"loca" => Some((checksum(&loca), loca.clone())),
                _ => copy(tag).map(|bytes| (self.checksums[tag], bytes)),
            };
            if let Some((sum, bytes)) = table {
                out_tables.push((*tag, sum, bytes));
            }
        }

        let n = out_tables.len() as u16;
        let entry_selector = 15 - n.leading_zeros() as u16; // floor(log2(n)); n ≥ 2 here
        let search_range = 16 * (1 << entry_selector);
        let mut out = Vec::new();
        out.extend_from_slice(&0x0001_0000u32.to_be_bytes());
        for v in [n, search_range, entry_selector, 16 * n - search_range] {
            out.extend_from_slice(&v.to_be_bytes());
        }
        let mut offset = 12 + 16 * out_tables.len();
        for (tag, sum, bytes) in &out_tables {
            out.extend_from_slice(tag);
            out.extend_from_slice(&sum.to_be_bytes());
            out.extend_from_slice(&(offset as u32).to_be_bytes());
            out.extend_from_slice(&(bytes.len() as u32).to_be_bytes());
            offset += bytes.len().next_multiple_of(4);
        }
        for (_, _, bytes) in &out_tables {
            out.extend_from_slice(bytes);
            out.resize(out.len().next_multiple_of(4), 0);
        }
        out
    }
}

/// The TrueType table checksum: the sum of the table's big-endian 32-bit words, zero padded.
fn checksum(bytes: &[u8]) -> u32 {
    bytes.chunks(4).fold(0u32, |sum, c| {
        let mut word = [0u8; 4];
        word[..c.len()].copy_from_slice(c);
        sum.wrapping_add(u32::from_be_bytes(word))
    })
}

/// The Windows Unicode BMP subtable (platform 3, encoding 1, format 4).
fn parse_cmap(cmap: &[u8]) -> Result<HashMap<u16, u16>, FabricationError> {
    let n = u16_at(cmap, 2)? as usize;
    let mut base = None;
    for i in 0..n {
        let rec = 4 + 8 * i;
        let (platform, encoding, offset) = (u16_at(cmap, rec)?, u16_at(cmap, rec + 2)?, u32_at(cmap, rec + 4)? as usize);
        if platform == 3 && encoding == 1 && u16_at(cmap, offset)? == 4 {
            base = Some(offset);
        }
    }
    let base = base.ok_or_else(|| bad("no Windows Unicode character map (format 4)"))?;
    let seg_count = u16_at(cmap, base + 6)? as usize / 2;
    let ends = base + 14;
    let starts = ends + 2 * seg_count + 2;
    let deltas = starts + 2 * seg_count;
    let range_offsets = deltas + 2 * seg_count;
    let mut map = HashMap::new();
    for i in 0..seg_count {
        let (end, start) = (u16_at(cmap, ends + 2 * i)?, u16_at(cmap, starts + 2 * i)?);
        let (delta, range_offset) = (u16_at(cmap, deltas + 2 * i)?, u16_at(cmap, range_offsets + 2 * i)? as usize);
        if start > end {
            continue;
        }
        for c in start..=end {
            if c == 0xFFFF {
                continue;
            }
            let glyph = if range_offset == 0 {
                c.wrapping_add(delta)
            } else {
                let at = range_offsets + 2 * i + range_offset + 2 * (c - start) as usize;
                match u16_at(cmap, at) {
                    Ok(0) | Err(_) => 0,
                    Ok(g) => g.wrapping_add(delta),
                }
            };
            if glyph != 0 {
                map.insert(c, glyph);
            }
        }
    }
    Ok(map)
}

/// Name id 1: the Windows (US English first) record, else the Macintosh one.
fn parse_family(name: &[u8]) -> Result<String, FabricationError> {
    let (count, strings) = (u16_at(name, 2)? as usize, u16_at(name, 4)? as usize);
    let mut best: Option<(u8, String)> = None;
    for i in 0..count {
        let rec = 6 + 12 * i;
        let (platform, language, id) = (u16_at(name, rec)?, u16_at(name, rec + 4)?, u16_at(name, rec + 6)?);
        let (length, offset) = (u16_at(name, rec + 8)? as usize, u16_at(name, rec + 10)? as usize);
        if id != 1 {
            continue;
        }
        let Some(raw) = name.get(strings + offset..strings + offset + length) else { continue };
        let (rank, text) = match platform {
            3 => {
                let units: Vec<u16> = raw.chunks_exact(2).map(|c| u16::from_be_bytes([c[0], c[1]])).collect();
                (if language == 0x0409 { 3 } else { 2 }, String::from_utf16_lossy(&units))
            }
            1 => (1, raw.iter().map(|&b| b as char).collect()),
            _ => continue,
        };
        if best.as_ref().is_none_or(|(r, _)| rank > *r) {
            best = Some((rank, text));
        }
    }
    best.map(|(_, s)| s).ok_or_else(|| bad("no family name"))
}

/// Windows-1252 to Unicode; the five codes the code page leaves undefined map to themselves.
pub fn win_ansi_to_unicode(code: u8) -> u16 {
    const HIGH: [u16; 32] = [
        0x20AC, 0x0081, 0x201A, 0x0192, 0x201E, 0x2026, 0x2020, 0x2021, 0x02C6, 0x2030, 0x0160, 0x2039, 0x0152, 0x008D, 0x017D, 0x008F,
        0x0090, 0x2018, 0x2019, 0x201C, 0x201D, 0x2022, 0x2013, 0x2014, 0x02DC, 0x2122, 0x0161, 0x203A, 0x0153, 0x009D, 0x017E, 0x0178,
    ];
    match code {
        0x80..=0x9F => HIGH[(code - 0x80) as usize],
        _ => u16::from(code),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn checksum_pads_the_last_word() {
        assert_eq!(checksum(&[0, 0, 0, 1, 0, 0, 0, 2]), 3);
        assert_eq!(checksum(&[0, 0, 0, 1, 0x80]), 0x8000_0001);
        assert_eq!(checksum(&[0xFF, 0xFF, 0xFF, 0xFF, 0, 0, 0, 2]), 1);
    }

    #[test]
    fn win_ansi_maps_the_c1_range_through_the_code_page() {
        assert_eq!(win_ansi_to_unicode(b'A'), 0x41);
        assert_eq!(win_ansi_to_unicode(0x80), 0x20AC);
        assert_eq!(win_ansi_to_unicode(0x81), 0x81);
        assert_eq!(win_ansi_to_unicode(0x96), 0x2013);
        assert_eq!(win_ansi_to_unicode(0xE9), 0xE9);
    }

    fn transport(file: &str) -> TrueTypeFont {
        let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../../fonts").join(file);
        TrueTypeFont::parse(std::fs::read(path).unwrap()).unwrap()
    }

    /// (tag, checksum, offset, length) of each table of a font file.
    fn directory(font: &[u8]) -> Vec<(String, u32, usize, usize)> {
        let n = u16_at(font, 4).unwrap() as usize;
        (0..n)
            .map(|i| {
                let rec = 12 + 16 * i;
                let word = |at| u32_at(font, at).unwrap();
                (String::from_utf8_lossy(&font[rec..rec + 4]).into_owned(), word(rec + 4), word(rec + 8) as usize, word(rec + 12) as usize)
            })
            .collect()
    }

    #[test]
    fn descriptor_metrics_are_the_ones_pdfsharp_quotes() {
        // From the font descriptors PDFsharp 6.2.4 wrote for these two files.
        let medium = transport("tranm___.TTF");
        assert_eq!(medium.family(), "Key Transport Medium");
        assert_eq!((medium.ascent(), medium.cap_height(), medium.descent(), medium.x_height()), (1074, 867, 212, 646));
        assert_eq!(medium.bbox(), [-188, -236, 2254, 1074]);
        let heavy = transport("tranh___.TTF");
        assert_eq!(heavy.family(), "Key Transport Heavy");
        assert_eq!((heavy.ascent(), heavy.cap_height(), heavy.descent(), heavy.x_height()), (1073, 853, 211, 638));
        assert_eq!(heavy.bbox(), [-188, -236, 2262, 1074]);
    }

    #[test]
    fn widths_are_truncated_thousandths_and_missing_characters_take_glyph_zero() {
        let medium = transport("tranm___.TTF");
        let w = medium.win_ansi_widths();
        assert_eq!(w.len(), 224);
        // space ! " # $ % & ' as PDFsharp lists them; 850/1000 would round up to 850.
        assert_eq!(w[..8], [310, 449, 486, 769, 775, 849, 667, 207]);
        // ']' has an empty, zero-width glyph in this font; 0x7F has none and takes glyph 0.
        assert_eq!((w[(b']' - 32) as usize], w[0x7F - 32]), (0, 500));
        assert_eq!(medium.glyph(0x7F), 0);
        // "750" is 3896 units wide: the reference centres it with exactly that width.
        assert_eq!(medium.text_width_units("750"), 3896);
        assert_eq!(medium.text_width_units("7\t50"), 3896, "characters below U+0020 are not measured");
    }

    #[test]
    fn subset_keeps_glyph_numbers_and_copies_the_other_tables() {
        let medium = transport("tranm___.TTF");
        let used: BTreeSet<u16> = [medium.glyph(u16::from(b'A'))].into();
        let subset = medium.subset(&used);
        // The font program PDFsharp embedded for the single letter "A" was 5152 bytes with
        // this header and a 676-byte glyph table.
        assert_eq!(subset.len(), 5152);
        assert_eq!(subset[..12], [0, 1, 0, 0, 0, 10, 0, 0x80, 0, 3, 0, 0x20]);
        let dir = directory(&subset);
        let tags: Vec<&str> = dir.iter().map(|t| t.0.as_str()).collect();
        assert_eq!(tags, ["cmap", "cvt ", "fpgm", "glyf", "head", "hhea", "hmtx", "loca", "maxp", "prep"]);
        let glyf = dir.iter().find(|t| t.0 == "glyf").unwrap();
        assert_eq!((glyf.1, glyf.2, glyf.3), (0xdeed_ad0c, 2528, 676));
        let loca = dir.iter().find(|t| t.0 == "loca").unwrap();
        assert_eq!((loca.1, loca.3), (0x8f1a_906c, 504));

        // It is a font again: same glyph count, the letter's outline where the index says.
        let again = TrueTypeFont::parse_program(subset.clone()).unwrap();
        assert_eq!(again.loca.len(), medium.loca.len());
        let a = medium.glyph(u16::from(b'A'));
        assert_eq!(again.glyph_data(a), medium.glyph_data(a));
        assert_eq!(again.glyph_data(0), medium.glyph_data(0));
        let b = medium.glyph(u16::from(b'B'));
        assert!(again.glyph_data(b).is_empty() && !medium.glyph_data(b).is_empty());
        // Copied tables are the original bytes with the original checksums.
        for (tag, sum, offset, length) in &dir {
            if tag == "glyf" || tag == "loca" {
                continue;
            }
            let key: [u8; 4] = tag.as_bytes().try_into().unwrap();
            let (o, l) = medium.tables[&key];
            assert_eq!(&subset[*offset..offset + length], &medium.data[o..o + l], "{tag}");
            assert_eq!(*sum, medium.checksums[&key], "{tag}");
        }
    }

    #[test]
    fn subset_takes_the_components_of_a_composite_glyph() {
        // Transport Heavy has one composite glyph (253, not mapped to a character).
        let heavy = transport("tranh___.TTF");
        let composite = 253u16;
        assert!(i16_at(heavy.glyph_data(composite), 0).unwrap() < 0, "glyph 253 is expected to be composite");
        let mut closure: BTreeSet<u16> = [composite].into();
        heavy.close_over_components(&mut closure);
        assert!(closure.len() > 1);
        let again = TrueTypeFont::parse_program(heavy.subset(&[composite].into())).unwrap();
        for g in closure {
            assert_eq!(again.glyph_data(g), heavy.glyph_data(g), "glyph {g}");
        }
    }

    #[test]
    fn garbage_is_rejected_not_panicked_on() {
        assert!(TrueTypeFont::parse(vec![]).is_err());
        assert!(TrueTypeFont::parse(vec![0; 64]).is_err());
        let mut junk = vec![0, 1, 0, 0, 0, 1, 0, 0, 0, 0, 0, 0];
        junk.extend_from_slice(b"head");
        junk.extend_from_slice(&[0; 4]);
        junk.extend_from_slice(&1_000_000u32.to_be_bytes());
        junk.extend_from_slice(&54u32.to_be_bytes());
        assert!(TrueTypeFont::parse(junk).is_err());
    }
}
