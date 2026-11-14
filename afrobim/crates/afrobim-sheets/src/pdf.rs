//! PDF output: one page per sheet, vector strokes and fills, text as real text.
//!
//! A small writer of its own. The fabrication crate's PDF writer is tied to its own sheet type and
//! to TrueType fonts from a rule pack and exposes no general page API, so it is not reused here.
//! This one writes PDF 1.4 with the two standard faces Helvetica and Helvetica-Bold (present in
//! every reader, so nothing is embedded), WinAnsi encoded, with Flate-compressed page streams. It
//! writes no date and no random id, so the same sheets give the same bytes.

use std::fmt::Write as _;
use std::io::Write as _;

use flate2::write::ZlibEncoder;
use flate2::Compression;

use crate::item::{ComposedSheet, Item};
use crate::text::{encode, text_width, Align, CAP_HEIGHT};

/// What the PDF names as its producer.
pub const PRODUCER: &str = concat!("AfroBIM afrobim-sheets ", env!("CARGO_PKG_VERSION"));

const PT_PER_MM: f64 = 72.0 / 25.4;

/// Writes the sheets as one PDF, one page each, in order. `title` is the document title.
pub fn write_pdf(sheets: &[ComposedSheet], title: &str) -> Vec<u8> {
    let mut objects: Vec<Vec<u8>> = Vec::new();
    // Object numbers: 1 catalog, 2 pages, 3 and 4 fonts, 5 info; then a page and its stream per sheet.
    let page_obj = |i: usize| 6 + 2 * i;
    let kids: String = (0..sheets.len()).map(|i| format!("{} 0 R", page_obj(i))).collect::<Vec<_>>().join(" ");
    objects.push(b"<< /Type /Catalog /Pages 2 0 R >>".to_vec());
    objects.push(format!("<< /Type /Pages /Kids [{kids}] /Count {} >>", sheets.len()).into_bytes());
    objects.push(b"<< /Type /Font /Subtype /Type1 /BaseFont /Helvetica /Encoding /WinAnsiEncoding >>".to_vec());
    objects.push(b"<< /Type /Font /Subtype /Type1 /BaseFont /Helvetica-Bold /Encoding /WinAnsiEncoding >>".to_vec());
    let mut info = b"<< /Title ".to_vec();
    info.extend(pdf_string(title));
    info.extend(b" /Producer ");
    info.extend(pdf_string(PRODUCER));
    info.extend(b" >>");
    objects.push(info);

    for (i, sheet) in sheets.iter().enumerate() {
        let (w, h) = (sheet.width_mm * PT_PER_MM, sheet.height_mm * PT_PER_MM);
        objects.push(
            format!(
                "<< /Type /Page /Parent 2 0 R /MediaBox [0 0 {} {}] /Resources << /Font << /F1 3 0 R /F2 4 0 R >> >> /Contents {} 0 R >>",
                n2(w),
                n2(h),
                page_obj(i) + 1
            )
            .into_bytes(),
        );
        let content = page_content(sheet);
        let mut enc = ZlibEncoder::new(Vec::new(), Compression::default());
        enc.write_all(content.as_bytes()).expect("writing to memory");
        let data = enc.finish().expect("compressing in memory");
        let mut stream = format!("<< /Length {} /Filter /FlateDecode >>\nstream\n", data.len()).into_bytes();
        stream.extend(data);
        stream.extend(b"\nendstream");
        objects.push(stream);
    }

    let mut out: Vec<u8> = b"%PDF-1.4\n%\xE2\xE3\xCF\xD3\n".to_vec();
    let mut offsets = Vec::with_capacity(objects.len());
    for (i, body) in objects.iter().enumerate() {
        offsets.push(out.len());
        out.extend(format!("{} 0 obj\n", i + 1).bytes());
        out.extend(body);
        out.extend(b"\nendobj\n");
    }
    let xref = out.len();
    out.extend(format!("xref\n0 {}\n0000000000 65535 f \n", objects.len() + 1).bytes());
    for o in offsets {
        out.extend(format!("{o:010} 00000 n \n").bytes());
    }
    out.extend(format!("trailer\n<< /Size {} /Root 1 0 R /Info 5 0 R >>\nstartxref\n{xref}\n%%EOF\n", objects.len() + 1).bytes());
    out
}

/// A number in PDF points: at most two decimals, no exponent.
fn n2(v: f64) -> String {
    let s = format!("{v:.2}");
    let s = s.trim_end_matches('0').trim_end_matches('.');
    if s.is_empty() || s == "-0" {
        "0".to_string()
    } else {
        s.to_string()
    }
}

/// A number in points from millimetres.
fn pt(mm: f64) -> String {
    n2(mm * PT_PER_MM)
}

fn rgb(c: [u8; 3]) -> String {
    let f = |v: u8| {
        let s = format!("{:.3}", v as f64 / 255.0);
        s.trim_end_matches('0').trim_end_matches('.').to_string()
    };
    format!("{} {} {}", f(c[0]), f(c[1]), f(c[2]))
}

/// A PDF literal string for WinAnsi text.
fn pdf_string(text: &str) -> Vec<u8> {
    string_bytes(&encode(text))
}

fn string_bytes(bytes: &[u8]) -> Vec<u8> {
    let mut out = vec![b'('];
    for &b in bytes {
        match b {
            b'(' | b')' | b'\\' => {
                out.push(b'\\');
                out.push(b);
            }
            0x20..=0x7E => out.push(b),
            _ => out.extend(format!("\\{b:03o}").bytes()),
        }
    }
    out.push(b')');
    out
}

fn page_content(sheet: &ComposedSheet) -> String {
    let mut c = String::new();
    c.push_str("1 J 1 j\n"); // round caps and joins: thin lines meet cleanly
    let mut stroke_colour = String::new();
    let mut fill_colour = String::new();
    let mut width = f64::NAN;
    for item in &sheet.items {
        match item {
            Item::Stroke { points, closed, weight_mm, rgb: col, .. } => {
                if points.len() < 2 {
                    continue;
                }
                let sc = rgb(*col);
                if sc != stroke_colour {
                    let _ = writeln!(c, "{sc} RG");
                    stroke_colour = sc;
                }
                if *weight_mm != width {
                    let _ = writeln!(c, "{} w", n2((*weight_mm * PT_PER_MM).max(0.0)));
                    width = *weight_mm;
                }
                for (i, p) in points.iter().enumerate() {
                    let _ = writeln!(c, "{} {} {}", pt(p.x), pt(p.y), if i == 0 { "m" } else { "l" });
                }
                c.push_str(if *closed { "h S\n" } else { "S\n" });
            }
            Item::Fill { rings, rgb: col, .. } => {
                let fc = rgb(*col);
                if fc != fill_colour {
                    let _ = writeln!(c, "{fc} rg");
                    fill_colour = fc;
                }
                for ring in rings.iter().filter(|r| r.len() >= 3) {
                    for (i, p) in ring.iter().enumerate() {
                        let _ = writeln!(c, "{} {} {}", pt(p.x), pt(p.y), if i == 0 { "m" } else { "l" });
                    }
                    c.push_str("h\n");
                }
                c.push_str("f*\n");
            }
            Item::Text(t) => {
                let bytes = encode(&t.text);
                if bytes.is_empty() {
                    continue;
                }
                let size = t.height_mm / CAP_HEIGHT;
                // Move back along the baseline by the alignment share of the width.
                let w = text_width(&t.text, t.height_mm, t.bold);
                let back = match t.align {
                    Align::Left => 0.0,
                    Align::Centre => w / 2.0,
                    Align::Right => w,
                };
                let (s, co) = t.rotation_deg.to_radians().sin_cos();
                let (x, y) = (t.at.x - back * co, t.at.y - back * s);
                let fc = rgb(t.rgb);
                let _ = writeln!(c, "BT {fc} rg");
                fill_colour = fc;
                let _ = writeln!(c, "/{} {} Tf", if t.bold { "F2" } else { "F1" }, n2(size * PT_PER_MM));
                let _ = writeln!(c, "{} {} {} {} {} {} Tm", n4(co), n4(s), n4(-s), n4(co), pt(x), pt(y));
                c.push_str(&String::from_utf8(string_bytes(&bytes)).expect("escaped PDF strings are ASCII"));
                c.push_str(" Tj ET\n");
            }
        }
    }
    c
}

fn n4(v: f64) -> String {
    let s = format!("{v:.4}");
    let s = s.trim_end_matches('0').trim_end_matches('.');
    if s.is_empty() || s == "-0" {
        "0".to_string()
    } else {
        s.to_string()
    }
}
