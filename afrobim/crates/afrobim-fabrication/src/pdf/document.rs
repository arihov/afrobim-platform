//! The PDF file around the page content: the objects PDFsharp 6.2.4 writes for a document of
//! pages set in two TrueType faces, in the same order and with the same dictionaries, so that
//! the Rust and C# files can be compared object for object.

use std::io::Write as _;

use flate2::write::ZlibEncoder;
use flate2::Compression;

use crate::num::fixed;

use super::gfx::{DocResources, ExtGState, Face, Gfx};
use super::{PdfFonts, PdfStamp, PRODUCER};

struct Page {
    width_mm: f64,
    height_mm: f64,
    page_object: u32,
    content_object: u32,
    content: Vec<u8>,
    ext_gstates: Vec<ExtGState>,
    faces: Vec<Face>,
}

/// A document under construction.
pub(crate) struct Document<'a> {
    fonts: &'a PdfFonts,
    title: String,
    res: DocResources,
    pages: Vec<Page>,
}

impl<'a> Document<'a> {
    pub fn new(fonts: &'a PdfFonts, title: impl Into<String>) -> Self {
        // 1 Info, 2 Catalog, 3 Pages.
        Document { fonts, title: title.into(), res: DocResources { next_object: 3, ..DocResources::default() }, pages: Vec::new() }
    }

    /// Adds a page and draws it.
    pub fn page(&mut self, width_mm: f64, height_mm: f64, draw: impl FnOnce(&mut Gfx)) {
        let page_object = self.res.allocate();
        let content_object = self.res.allocate();
        let mut gfx = Gfx::new(&mut self.res, [&self.fonts.regular, &self.fonts.bold], height_mm);
        draw(&mut gfx);
        let (ext_gstates, faces) = (gfx.ext_gstates.clone(), gfx.faces.clone());
        let content = gfx.finish();
        self.pages.push(Page { width_mm, height_mm, page_object, content_object, content, ext_gstates, faces });
    }

    /// The finished file.
    pub fn finish(mut self, stamp: &PdfStamp) -> Vec<u8> {
        let fonts = [&self.fonts.regular, &self.fonts.bold];
        // Font programs and width arrays come after every page, in the order the fonts were
        // first used; the metadata stream is last.
        let mut used: Vec<(Face, u32, u32)> = [Face::Regular, Face::Bold]
            .into_iter()
            .filter_map(|f| self.res.fonts[f as usize].map(|(descriptor, font)| (f, descriptor, font)))
            .collect();
        used.sort_by_key(|u| u.1);
        let files: Vec<(u32, u32)> = used.iter().map(|_| (self.res.allocate(), self.res.allocate())).collect();
        let metadata_object = self.res.allocate();

        let mut objects: Vec<(u32, Vec<u8>)> = Vec::new();
        objects.push((
            1,
            latin1_bytes(&format!(
                "<</CreationDate({})/Title{}/Creator{}/Producer{}>>",
                stamp.pdf_date(),
                text_string(&self.title),
                text_string(&stamp.creator),
                text_string(PRODUCER)
            )),
        ));
        objects.push((2, format!("<</Type/Catalog/Pages 3 0 R/Metadata {metadata_object} 0 R>>").into_bytes()));
        let kids: Vec<String> = self.pages.iter().map(|p| format!("{} 0 R", p.page_object)).collect();
        objects.push((3, format!("<</Type/Pages/Count {}/Kids[{}]>>", self.pages.len(), kids.join(" ")).into_bytes()));

        for page in &self.pages {
            let pt = |mm: f64| fixed(mm * 72.0 / 25.4, 0, 3);
            let mut dict = format!(
                "<</Type/Page/MediaBox[0 0 {} {}]/Parent 3 0 R/Contents {} 0 R",
                pt(page.width_mm),
                pt(page.height_mm),
                page.content_object
            );
            if !page.ext_gstates.is_empty() || !page.faces.is_empty() {
                dict.push_str("/Resources<</ProcSet[/PDF/Text/ImageB/ImageC/ImageI]");
                if !page.ext_gstates.is_empty() {
                    dict.push_str("/ExtGState<<");
                    for (i, kind) in page.ext_gstates.iter().enumerate() {
                        let object = match kind {
                            ExtGState::Fill => self.res.ext_fill,
                            ExtGState::Stroke => self.res.ext_stroke,
                        };
                        dict.push_str(&format!("/GS{i} {} 0 R", object.expect("allocated on first use")));
                    }
                    dict.push_str(">>");
                }
                if !page.faces.is_empty() {
                    dict.push_str("/Font<<");
                    for (i, face) in page.faces.iter().enumerate() {
                        dict.push_str(&format!("/F{i} {} 0 R", self.res.fonts[*face as usize].expect("allocated on first use").1));
                    }
                    dict.push_str(">>");
                }
                dict.push_str(">>");
            }
            dict.push_str("/Group<</CS/DeviceRGB/S/Transparency>>>>");
            objects.push((page.page_object, dict.into_bytes()));
            objects.push((page.content_object, stream("", &page.content, true)));
        }

        if let Some(n) = self.res.ext_fill {
            objects.push((n, b"<</Type/ExtGState/ca 1>>".to_vec()));
        }
        if let Some(n) = self.res.ext_stroke {
            objects.push((n, b"<</Type/ExtGState/CA 1>>".to_vec()));
        }

        for (i, ((face, descriptor, font_object), (file, widths))) in used.iter().zip(&files).enumerate() {
            let font = fonts[*face as usize];
            let name = format!(
                "{}+{}{}",
                stamp.subset_tag(i),
                pdf_name(font.family()),
                if *face == Face::Bold { ",Bold" } else { "" }
            );
            let b = font.bbox();
            objects.push((
                *descriptor,
                format!(
                    "<</Type/FontDescriptor/Ascent {}/CapHeight {}/Descent {}/Flags 32/FontBBox[{} {} {} {}]/ItalicAngle 0/StemV 0/XHeight {}/FontName/{name}/FontFile2 {file} 0 R>>",
                    font.ascent(),
                    font.cap_height(),
                    font.descent(),
                    b[0],
                    b[1],
                    b[2],
                    b[3],
                    font.x_height()
                )
                .into_bytes(),
            ));
            objects.push((
                *font_object,
                format!(
                    "<</Type/Font/Subtype/TrueType/BaseFont/{name}/Encoding/WinAnsiEncoding/FontDescriptor {descriptor} 0 R/FirstChar 32/LastChar 255/Widths {widths} 0 R>>"
                )
                .into_bytes(),
            ));
            let program = font.subset(&self.res.glyphs[*face as usize]);
            objects.push((*file, stream(&format!("/Length1 {}", program.len()), &program, true)));
            let w: Vec<String> = font.win_ansi_widths().iter().map(i32::to_string).collect();
            objects.push((*widths, format!("[{}]", w.join(" ")).into_bytes()));
        }

        objects.push((metadata_object, stream("/Type/Metadata/Subtype/XML", xmp(&self.title, stamp).as_bytes(), false)));
        objects.sort_by_key(|o| o.0);

        let mut out: Vec<u8> = b"%PDF-1.7\n%\xD3\xF4\xCC\xE1\n".to_vec();
        let mut offsets = Vec::with_capacity(objects.len());
        for (n, body) in &objects {
            offsets.push(out.len());
            out.extend_from_slice(format!("{n} 0 obj\n").as_bytes());
            out.extend_from_slice(body);
            out.extend_from_slice(b"\nendobj\n");
        }
        let xref = out.len();
        out.extend_from_slice(format!("xref\n0 {}\n0000000000 65535 f \n", objects.len() + 1).as_bytes());
        for o in offsets {
            out.extend_from_slice(format!("{o:010} 00000 n \n").as_bytes());
        }
        let id = stamp.file_id();
        out.extend_from_slice(
            format!(
                "trailer\n<</ID[<{id}><{id}>]/Info 1 0 R/Root 2 0 R/Size {}>>% Created with {PRODUCER}\nstartxref\n{xref}\n%%EOF\n",
                objects.len() + 1
            )
            .as_bytes(),
        );
        out
    }
}

/// A stream object body. `extra` goes at the front of the dictionary.
fn stream(extra: &str, data: &[u8], compress: bool) -> Vec<u8> {
    let (filter, bytes) = if compress {
        let mut z = ZlibEncoder::new(Vec::new(), Compression::best());
        z.write_all(data).expect("writing to memory");
        ("/Filter/FlateDecode", z.finish().expect("writing to memory"))
    } else {
        ("", data.to_vec())
    };
    // PDFsharp puts /Length first on a content stream and last on the others.
    let dict = if extra.is_empty() {
        format!("<</Length {}{filter}>>", bytes.len())
    } else {
        format!("<<{extra}{filter}/Length {}>>", bytes.len())
    };
    let mut out = dict.into_bytes();
    out.extend_from_slice(b"\nstream\n");
    out.extend_from_slice(&bytes);
    out.extend_from_slice(b"\nendstream");
    out
}

/// A PDF text string: a literal in PDFDocEncoding when every character is Latin-1, otherwise
/// UTF-16BE in hex with a byte-order mark. Returned as text for `format!`; the literal form
/// holds Latin-1 characters that [`latin1_bytes`] turns into single bytes.
fn text_string(s: &str) -> String {
    if s.chars().all(|c| (c as u32) < 256) {
        let mut out = String::from("(");
        for c in s.chars() {
            if matches!(c, '(' | ')' | '\\') {
                out.push('\\');
            }
            out.push(c);
        }
        out.push(')');
        out
    } else {
        let mut out = String::from("<FEFF");
        for unit in s.encode_utf16() {
            out.push_str(&format!("{unit:04X}"));
        }
        out.push('>');
        out
    }
}

/// One byte per character; every character must be Latin-1 (as [`text_string`] guarantees).
fn latin1_bytes(s: &str) -> Vec<u8> {
    s.chars().map(|c| c as u32 as u8).collect()
}

/// A name object's text: `#xx` for anything that is not a regular character.
fn pdf_name(s: &str) -> String {
    let mut out = String::new();
    for b in s.bytes() {
        if b.is_ascii_graphic() && !b"#()<>[]{}/%".contains(&b) {
            out.push(b as char);
        } else {
            out.push_str(&format!("#{b:02X}"));
        }
    }
    out
}

/// The XMP packet PDFsharp writes (CR LF line ends, values not XML-escaped).
fn xmp(title: &str, stamp: &PdfStamp) -> String {
    let date = stamp.xmp_date();
    let lines = [
        "<?xpacket begin=\"\u{FEFF}\" id=\"W5M0MpCehiHzreSzNTczkc9d\"?>".to_string(),
        "  <x:xmpmeta xmlns:x=\"adobe:ns:meta/\" x:xmptk=\"3.1-701\">".to_string(),
        "    <rdf:RDF xmlns:rdf=\"http://www.w3.org/1999/02/22-rdf-syntax-ns#\">".to_string(),
        "      <rdf:Description rdf:about=\"\" xmlns:pdf=\"http://ns.adobe.com/pdf/1.3/\">".to_string(),
        format!("        <pdf:Producer>{PRODUCER}</pdf:Producer><pdf:Keywords></pdf:Keywords>"),
        "      </rdf:Description>".to_string(),
        "      <rdf:Description rdf:about=\"\" xmlns:dc=\"http://purl.org/dc/elements/1.1/\">".to_string(),
        format!("        <dc:title><rdf:Alt><rdf:li xml:lang=\"x-default\">{title}</rdf:li></rdf:Alt></dc:title>"),
        "        <dc:creator><rdf:Seq><rdf:li></rdf:li></rdf:Seq></dc:creator>".to_string(),
        "        <dc:description><rdf:Alt><rdf:li xml:lang=\"x-default\"></rdf:li></rdf:Alt></dc:description>".to_string(),
        "      </rdf:Description>".to_string(),
        "      <rdf:Description rdf:about=\"\" xmlns:xmp=\"http://ns.adobe.com/xap/1.0/\">".to_string(),
        format!("        <xmp:CreatorTool>{}</xmp:CreatorTool>", stamp.creator),
        format!("        <xmp:CreateDate>{date}</xmp:CreateDate>"),
        format!("        <xmp:ModifyDate>{date}</xmp:ModifyDate>"),
        "      </rdf:Description>".to_string(),
        "      <rdf:Description rdf:about=\"\" xmlns:xmpMM=\"http://ns.adobe.com/xap/1.0/mm/\">".to_string(),
        format!("        <xmpMM:DocumentID>uuid:{}</xmpMM:DocumentID>", stamp.uuid(1)),
        format!("        <xmpMM:InstanceID>uuid:{}</xmpMM:InstanceID>", stamp.uuid(2)),
        "      </rdf:Description>".to_string(),
        String::new(),
        "    </rdf:RDF>".to_string(),
        "  </x:xmpmeta>".to_string(),
        "<?xpacket end=\"w\"?>".to_string(),
    ];
    lines.join("\r\n")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn text_strings_are_literal_for_latin1_and_hex_otherwise() {
        // Both forms as PDFsharp 6.2.4 wrote them for these titles.
        assert_eq!(text_string("A&B <c> é (x) \\ sheet"), "(A&B <c> é \\(x\\) \\\\ sheet)");
        assert_eq!(text_string("道 € f"), "<FEFF9053002020AC00200066>");
    }

    #[test]
    fn names_escape_spaces() {
        assert_eq!(pdf_name("Key Transport Medium"), "Key#20Transport#20Medium");
    }
}
