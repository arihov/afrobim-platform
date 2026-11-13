//! PDF output: the fabrication sheet and the sign-schedule report. Port of C# `SheetPdfWriter`,
//! `SchedulePdfWriter` and `PdfFonts`.
//!
//! The C# writers draw with PDFsharp 6.2.4. There is no PDFsharp here: [`gfx`] writes the page
//! content the way PDFsharp writes it for the same drawing calls, [`ttf`] builds the same font
//! subsets and [`document`] the same objects, so a Rust PDF and a C# PDF of the same sheet are
//! the same document object for object. They are not the same file byte for byte: the creation
//! time, the document ids and the font subset tags are different on every run of either writer,
//! the producer names differ, and the streams are deflated by different compressors.

mod document;
mod gfx;
mod schedule_pdf;
mod sheet_pdf;
pub mod ttf;

use std::hash::{BuildHasher, Hasher};
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use afrobim_signs::SignEngine;

use crate::sheet::{SheetBox, SheetText, SheetTextAlign};
use crate::FabricationError;
use gfx::{Align, Face, Gfx, Pen, Rgb};
use ttf::TrueTypeFont;

pub use schedule_pdf::{schedule_totals, write_schedule_pdf, write_schedule_pdf_stamped, ScheduleInfo, ScheduleRow, ScheduleTotals, ROWS_PER_PAGE};
pub use sheet_pdf::{write_sheet_pdf, write_sheet_pdf_stamped};

/// What the PDFs name as their producer.
pub const PRODUCER: &str = concat!("AfroBIM afrobim-fabrication ", env!("CARGO_PKG_VERSION"));

/// Fonts for the PDF writers: the sheets are set in two TrueType files, a regular face and a
/// bold one. With a rule pack's fonts that is Transport Medium for regular text and Transport
/// Heavy for bold. Only the glyphs a document uses are embedded in it.
#[derive(Debug, Clone)]
pub struct PdfFonts {
    pub(crate) regular: TrueTypeFont,
    pub(crate) bold: TrueTypeFont,
}

impl PdfFonts {
    /// Reads the regular and the bold face from two TrueType files.
    pub fn load(regular_ttf: &Path, bold_ttf: &Path) -> Result<PdfFonts, FabricationError> {
        let read = |path: &Path| -> Result<TrueTypeFont, FabricationError> {
            let data = std::fs::read(path).map_err(|e| FabricationError::Font(format!("{}: {e}", path.display())))?;
            TrueTypeFont::parse(data).map_err(|e| FabricationError::Font(format!("{}: {e}", path.display())))
        };
        Ok(PdfFonts { regular: read(regular_ttf)?, bold: read(bold_ttf)? })
    }

    /// The two faces from font files already in memory.
    pub fn from_bytes(regular_ttf: Vec<u8>, bold_ttf: Vec<u8>) -> Result<PdfFonts, FabricationError> {
        Ok(PdfFonts { regular: TrueTypeFont::parse(regular_ttf)?, bold: TrueTypeFont::parse(bold_ttf)? })
    }

    /// Medium for regular text, Heavy for bold, from the folder holding the fonts the engine's
    /// rule pack names (`fonts.onDark.file` and `fonts.onLight.file`).
    pub fn from_folder(fonts_dir: &Path, engine: &SignEngine) -> Result<PdfFonts, FabricationError> {
        let fonts = &engine.pack().fonts;
        PdfFonts::load(&find_font(fonts_dir, &fonts.on_dark.file), &find_font(fonts_dir, &fonts.on_light.file))
    }
}

/// The font file in `dir`: the name as the pack spells it, or the same name in another letter
/// case (packs are written on Windows, where `TRANM___.TTF` and `tranm___.ttf` are one file).
fn find_font(dir: &Path, file: &str) -> PathBuf {
    let exact = dir.join(file);
    if exact.is_file() {
        return exact;
    }
    std::fs::read_dir(dir)
        .ok()
        .and_then(|entries| {
            entries.flatten().map(|e| e.path()).find(|p| p.file_name().and_then(|n| n.to_str()).is_some_and(|n| n.eq_ignore_ascii_case(file)))
        })
        .unwrap_or(exact)
}

/// The parts of a PDF that are not its content: who made it, when, and the seed its ids and
/// font subset tags are derived from. [`PdfStamp::now`] is what the writers use; a fixed stamp
/// gives a reproducible file.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PdfStamp {
    /// The application named as the document's creator.
    pub creator: String,
    /// Creation time, UTC, as seconds since 1970-01-01.
    pub created_unix: u64,
    /// Seed for the document id, the XMP ids and the font subset tags.
    pub seed: u64,
}

impl PdfStamp {
    /// The current time and a fresh seed, creator "AfroBIM".
    pub fn now() -> PdfStamp {
        let elapsed = SystemTime::now().duration_since(UNIX_EPOCH).unwrap_or_default();
        let mut hasher = std::collections::hash_map::RandomState::new().build_hasher();
        hasher.write_u128(elapsed.as_nanos());
        PdfStamp { creator: "AfroBIM".to_string(), created_unix: elapsed.as_secs(), seed: hasher.finish() }
    }

    fn random(&self, stream: u64) -> u64 {
        // splitmix64
        let mut z = self.seed.wrapping_add(0x9E37_79B9_7F4A_7C15u64.wrapping_mul(stream + 1));
        z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
        z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
        z ^ (z >> 31)
    }

    /// (year, month, day, hour, minute, second), UTC.
    fn civil(&self) -> (i64, u32, u32, u32, u32, u32) {
        let (days, secs) = ((self.created_unix / 86_400) as i64, (self.created_unix % 86_400) as u32);
        let (y, m, d) = civil_from_days(days);
        (y, m, d, secs / 3600, secs / 60 % 60, secs % 60)
    }

    pub(crate) fn pdf_date(&self) -> String {
        let (y, mo, d, h, mi, s) = self.civil();
        format!("D:{y:04}{mo:02}{d:02}{h:02}{mi:02}{s:02}+00'00'")
    }

    pub(crate) fn xmp_date(&self) -> String {
        let (y, mo, d, h, mi, s) = self.civil();
        format!("{y:04}-{mo:02}-{d:02}T{h:02}:{mi:02}:{s:02}+00:00")
    }

    /// Today's date (UTC) as `yyyy-MM-dd`.
    pub(crate) fn iso_date(&self) -> String {
        let (y, mo, d, ..) = self.civil();
        format!("{y:04}-{mo:02}-{d:02}")
    }

    pub(crate) fn file_id(&self) -> String {
        format!("{:016X}{:016X}", self.random(100), self.random(101))
    }

    pub(crate) fn uuid(&self, n: u64) -> String {
        let (a, b) = (self.random(200 + 2 * n), self.random(201 + 2 * n));
        format!(
            "{:08x}-{:04x}-4{:03x}-{:04x}-{:012x}",
            a >> 32,
            (a >> 16) & 0xFFFF,
            a & 0xFFF,
            0x8000 | (b >> 48) & 0x3FFF,
            b & 0xFFFF_FFFF_FFFF
        )
    }

    /// Six capital letters in front of an embedded subset's font name.
    pub(crate) fn subset_tag(&self, font: usize) -> String {
        let mut v = self.random(300 + font as u64);
        (0..6)
            .map(|_| {
                let c = (b'A' + (v % 26) as u8) as char;
                v /= 26;
                c
            })
            .collect()
    }
}

/// Days since 1970-01-01 to a proleptic Gregorian date.
fn civil_from_days(days: i64) -> (i64, u32, u32) {
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z.rem_euclid(146_097);
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = (doy - (153 * mp + 2) / 5 + 1) as u32;
    let m = if mp < 10 { mp + 3 } else { mp - 9 } as u32;
    (yoe + era * 400 + i64::from(m <= 2), m, d)
}

/// Drawing in sheet coordinates (millimetres, y up) on a page that PDFsharp addresses y down.
/// Port of the C# `SheetPdfWriter.Canvas`: every coordinate takes the same two flips, so the
/// numbers written are the ones PDFsharp writes.
pub(crate) struct Canvas<'g, 'a> {
    g: &'g mut Gfx<'a>,
    page_height: f64,
}

impl<'g, 'a> Canvas<'g, 'a> {
    pub fn new(g: &'g mut Gfx<'a>, page_height: f64) -> Self {
        Canvas { g, page_height }
    }

    fn p(&self, x: f64, y: f64) -> (f64, f64) {
        (x, self.page_height - y)
    }

    pub fn line(&mut self, x1: f64, y1: f64, x2: f64, y2: f64, pen: &Pen) {
        let (a, b) = (self.p(x1, y1), self.p(x2, y2));
        self.g.draw_line(pen, a.0, a.1, b.0, b.1);
    }

    pub fn fill_rect(&mut self, b: &SheetBox, rgb: Rgb) {
        self.g.fill_rectangle(rgb, b.min_x, self.page_height - b.max_y, b.width(), b.height());
    }

    pub fn stroke_rect(&mut self, b: &SheetBox, pen: &Pen) {
        self.g.stroke_rectangle(pen, b.min_x, self.page_height - b.max_y, b.width(), b.height());
    }

    /// One path for all the polygons, filled even-odd.
    pub fn fill_polygons(&mut self, polygons: &[Vec<(f64, f64)>], rgb: Rgb) {
        if polygons.is_empty() {
            return;
        }
        let flipped: Vec<Vec<(f64, f64)>> = polygons.iter().map(|poly| poly.iter().map(|&(x, y)| self.p(x, y)).collect()).collect();
        self.g.fill_polygons(rgb, &flipped);
    }

    /// An anticlockwise arc in y-up degrees. Flipping y turns anticlockwise into clockwise, so
    /// the start angle and the sweep change sign.
    pub fn arc(&mut self, centre: (f64, f64), r: f64, start_deg: f64, end_deg: f64, pen: &Pen) {
        let mut sweep = end_deg - start_deg;
        while sweep <= 0.0 {
            sweep += 360.0;
        }
        let c = self.p(centre.0, centre.1);
        self.g.draw_arc(pen, c.0 - r, c.1 - r, 2.0 * r, 2.0 * r, -start_deg, -sweep);
    }

    /// Text with its capital height as given. The font's em size is scaled from the cap height
    /// (about 0.7 em in Transport and most sans faces). The Transport faces lack typographic
    /// dashes, × and ², so the text is printed in its ASCII form, as in the DXF.
    pub fn text(&mut self, t: &SheetText, rgb: Rgb) {
        if t.text.is_empty() {
            return;
        }
        let size = t.height / 0.7;
        let align = match t.align {
            SheetTextAlign::Centre => Align::Centre,
            SheetTextAlign::Right => Align::Right,
            SheetTextAlign::Left => Align::Left,
        };
        let at = self.p(t.x, t.y);
        self.g.save();
        if t.rotation_deg != 0.0 {
            self.g.rotate_at(-t.rotation_deg, at.0, at.1);
        }
        let face = if t.bold { Face::Bold } else { Face::Regular };
        self.g.draw_string(&crate::sheet_dxf::ascii(&t.text), face, size, rgb, at.0, at.1, align);
        self.g.restore();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn dates_come_from_unix_seconds() {
        assert_eq!(civil_from_days(0), (1970, 1, 1));
        assert_eq!(civil_from_days(19_782), (2024, 2, 29));
        let stamp = PdfStamp { creator: "x".into(), created_unix: 1_794_587_236, seed: 1 };
        assert_eq!(stamp.pdf_date(), "D:20261113162716+00'00'");
        assert_eq!(stamp.xmp_date(), "2026-11-13T16:27:16+00:00");
        assert_eq!(stamp.iso_date(), "2026-11-13");
    }

    #[test]
    fn ids_have_their_shapes_and_follow_the_seed() {
        let a = PdfStamp { creator: "x".into(), created_unix: 0, seed: 7 };
        let b = PdfStamp { seed: 8, ..a.clone() };
        assert_eq!(a.file_id().len(), 32);
        assert!(a.file_id().bytes().all(|c| c.is_ascii_hexdigit() && !c.is_ascii_lowercase()));
        let uuid = a.uuid(1);
        assert_eq!(uuid.len(), 36);
        assert_eq!(uuid.as_bytes()[14], b'4');
        assert_ne!(a.uuid(1), a.uuid(2));
        let tag = a.subset_tag(0);
        assert!(tag.len() == 6 && tag.bytes().all(|c| c.is_ascii_uppercase()));
        assert_ne!(a.subset_tag(0), a.subset_tag(1));
        assert_ne!(a.file_id(), b.file_id());
        assert_eq!(a.file_id(), a.clone().file_id());
    }
}
