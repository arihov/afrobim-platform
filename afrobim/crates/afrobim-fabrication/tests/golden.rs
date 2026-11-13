//! The Rust port against the C# reference.
//!
//! `tools/GoldenFab` (C#) runs `SignQuantities`, `SheetComposer`, `SheetDxfWriter`,
//! `SheetPdfWriter`, `SchedulePdfWriter` and `FabricationBatch` on a fixed set of UK signs and
//! stores what they write in `tests/golden/`. These tests produce the same files with this crate
//! and compare:
//!
//! - **CSV, DXF and the batch log: byte for byte.**
//! - **PDF: object for object, after the normalisation in [`canonical`].** Two PDFsharp runs on
//!   the same input already differ, so a plain byte comparison is not possible; the function
//!   lists every field that is masked and why. Everything else (every dictionary, every page
//!   content stream, the embedded font programs, the width arrays, the XMP packet) must be
//!   equal byte for byte.

use std::path::{Path, PathBuf};

use afrobim_draw::{Drawing, Extents, Primitive, Pt2};
use afrobim_fabrication::*;
use afrobim_signs::{Colour, Diagnostic, SignDefinition, SignEngine, SignLayout};
use flate2::{Decompress, FlushDecompress, Status};
use serde::Deserialize;
use serde_json::Value;

const PROJECT: &str = "A109 Kasese road – signing (Lot 2)";

#[derive(Deserialize)]
struct Fixture {
    definition: SignDefinition,
    quantity: Option<u32>,
    location: Option<String>,
}

fn golden() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/golden")
}

fn repo_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../..")
}

fn engine() -> SignEngine {
    SignEngine::load(&repo_root().join("packs/UK-TSM-2019.json"), &repo_root().join("fonts")).unwrap()
}

fn fonts(engine: &SignEngine) -> PdfFonts {
    PdfFonts::from_folder(&repo_root().join("fonts"), engine).unwrap()
}

fn fixtures() -> Vec<Fixture> {
    serde_json::from_str(&std::fs::read_to_string(golden().join("signs.json")).unwrap()).unwrap()
}

/// The stamp the comparisons use. Its creator is the one the C# writers name.
fn stamp() -> PdfStamp {
    PdfStamp { creator: "SignCore".to_string(), created_unix: 1_768_435_200, seed: 42 }
}

fn first_difference(a: &str, b: &str) -> String {
    for (n, (x, y)) in a.lines().zip(b.lines()).enumerate() {
        if x != y {
            return format!("line {}: reference `{x}`, ours `{y}`", n + 1);
        }
    }
    format!("lengths differ: reference {} lines, ours {} lines", a.lines().count(), b.lines().count())
}

fn assert_text_eq(name: &str, reference: &str, ours: &str) {
    assert!(ours == reference, "{name}: {}", first_difference(reference, ours));
}

// ─── quantities ───────────────────────────────────────────────────────────

fn measured(options: MeasureOptions) -> Vec<SignQuantity> {
    let e = engine();
    fixtures().iter().map(|f| measure_with(&e, &f.definition, &e.layout(&f.definition).unwrap(), options).unwrap()).collect()
}

#[test]
fn quantities_csv_matches_the_reference() {
    let rows = measured(MeasureOptions::default());
    assert!(rows.len() >= 14);
    assert_text_eq("quantities.csv", &std::fs::read_to_string(golden().join("quantities.csv")).unwrap(), &to_csv(&rows));
    assert_text_eq(
        "quantities.summary.csv",
        &std::fs::read_to_string(golden().join("quantities.summary.csv")).unwrap(),
        &summary_to_csv(&summarise(&rows)),
    );
}

#[test]
fn rounded_quantities_csv_matches_the_reference() {
    let rows = measured(MeasureOptions { sw_mm_override: None, round_up_mm: 50.0 });
    assert_text_eq("quantities-round50.csv", &std::fs::read_to_string(golden().join("quantities-round50.csv")).unwrap(), &to_csv(&rows));
}

// ─── the batch: sheets as DXF and PDF, the schedule, the log ──────────────

fn run_batch(dir_name: &str) -> (PathBuf, FabricationResult) {
    let e = engine();
    let signs: Vec<SheetSign> =
        fixtures().into_iter().map(|f| SheetSign { sign: f.definition, quantity: f.quantity, location: f.location }).collect();
    let out_dir = Path::new(env!("CARGO_TARGET_TMPDIR")).join(dir_name);
    let _ = std::fs::remove_dir_all(&out_dir);
    let options = FabricationOptions {
        sheet: SheetInfo { project: PROJECT.into(), drawn_by: "GoldenFab".into(), revision: "B".into(), date: "2026-01-15".into(), ..SheetInfo::default() },
        stamp: Some(stamp()),
        ..FabricationOptions::new(&out_dir)
    };
    let result = run(&e, &fonts(&e), &signs, &options);
    (out_dir, result)
}

#[test]
fn batch_writes_the_reference_files() {
    let (out_dir, result) = run_batch("batch");
    assert!(result.errors.is_empty(), "{:?}", result.errors);

    // The log, line for line, and through it the file names, scales and diagnostics.
    let reference_log = std::fs::read_to_string(golden().join("batch.log")).unwrap();
    assert_text_eq("batch.log", &reference_log, &(result.log.join("\n") + "\n"));
    assert_eq!(result.exit_code, 5, "the fixtures include a sign with layout errors");
    assert_eq!((result.error_count, result.warning_count), (2, 6));
    assert_eq!(result.rows.len(), fixtures().len());
    assert_eq!(result.files_written.len(), 2 * fixtures().len() + 1);

    let mut dxf = 0;
    let mut pdf = 0;
    for path in &result.files_written {
        let name = path.file_name().unwrap().to_str().unwrap();
        let reference = golden().join(name);
        assert!(reference.is_file(), "{name}: the reference run wrote no such file");
        if name.ends_with(".dxf") {
            assert_text_eq(name, &std::fs::read_to_string(&reference).unwrap(), &std::fs::read_to_string(path).unwrap());
            dxf += 1;
        } else {
            assert_pdf_matches(name, &std::fs::read(&reference).unwrap(), &std::fs::read(path).unwrap());
            pdf += 1;
        }
    }
    assert_eq!((dxf, pdf), (fixtures().len(), fixtures().len() + 1));
    assert!(out_dir.join(SCHEDULE_FILE_NAME).is_file());
}

#[test]
fn a_fixed_stamp_makes_a_run_reproducible() {
    let (a_dir, a) = run_batch("repeat-a");
    let (b_dir, b) = run_batch("repeat-b");
    assert_eq!(a.files_written.len(), b.files_written.len());
    for path in &a.files_written {
        let name = path.file_name().unwrap();
        assert!(std::fs::read(a_dir.join(name)).unwrap() == std::fs::read(b_dir.join(name)).unwrap(), "{name:?} differs between two runs");
    }
}

#[test]
fn long_schedule_without_optional_columns_matches_the_reference() {
    let e = engine();
    let rows: Vec<ScheduleRow> = fixtures()
        .iter()
        .map(|f| {
            let layout = e.layout(&f.definition).unwrap();
            ScheduleRow { q: measure(&e, &f.definition, &layout), type_name: composer::type_name(&f.definition), quantity: None, location: None }
        })
        .collect();
    let plain: Vec<ScheduleRow> = (0..60).map(|i| rows[i % rows.len()].clone()).collect();
    let info = ScheduleInfo { project: "Long schedule".into(), date: "2026-01-15".into(), standard: e.pack_name().to_string() };
    let ours = write_schedule_pdf_stamped(&plain, &info, &fonts(&e), &stamp());
    assert_pdf_matches("schedule-plain.pdf", &std::fs::read(golden().join("schedule-plain.pdf")).unwrap(), &ours);
    assert_eq!(count(&ours, b"/Type/Page/"), 3);
}

// ─── a hand-made face: arcs, hairlines, strokes without a colour ──────────

fn pt(v: &Value) -> Pt2 {
    Pt2::new(v[0].as_f64().unwrap(), v[1].as_f64().unwrap())
}

fn colour(v: &Value) -> Option<Colour> {
    serde_json::from_value(v.clone()).unwrap()
}

fn primitive(p: &Value) -> Primitive {
    match p["type"].as_str().unwrap() {
        "Fill" => Primitive::Fill { outline: p["outline"].as_array().unwrap().iter().map(pt).collect(), colour: colour(&p["colour"]).unwrap() },
        "Line" => Primitive::Line { from: pt(&p["from"]), to: pt(&p["to"]), width: p["width"].as_f64().unwrap(), colour: colour(&p["colour"]) },
        "Arc" => Primitive::Arc {
            centre: pt(&p["centre"]),
            radius: p["radius"].as_f64().unwrap(),
            start_deg: p["start_deg"].as_f64().unwrap(),
            end_deg: p["end_deg"].as_f64().unwrap(),
            colour: colour(&p["colour"]),
        },
        "Glyphs" => Primitive::Glyphs {
            contours: p["contours"].as_array().unwrap().iter().map(|c| c.as_array().unwrap().iter().map(pt).collect()).collect(),
            text: p["text"].as_str().unwrap().to_string(),
            ink: colour(&p["ink"]),
        },
        "Symbol" => Primitive::Symbol {
            origin: pt(&p["origin"]),
            id: p["id"].as_str().unwrap().to_string(),
            size: p["size"].as_f64().unwrap(),
            rotation_deg: p["rotation_deg"].as_f64().unwrap(),
        },
        other => panic!("unknown primitive {other}"),
    }
}

#[test]
fn synthetic_sheet_matches_the_reference() {
    let e = engine();
    let fixture: Value = serde_json::from_str(&std::fs::read_to_string(golden().join("synthetic.json")).unwrap()).unwrap();
    let sign: SignDefinition = serde_json::from_value(fixture["definition"].clone()).unwrap();
    let diagnostics: Vec<Diagnostic> = serde_json::from_value(fixture["diagnostics"].clone()).unwrap();
    let layout = SignLayout {
        drawing: Drawing {
            primitives: fixture["primitives"].as_array().unwrap().iter().map(primitive).collect(),
            extents: Extents { min: pt(&fixture["extents"]["min"]), max: pt(&fixture["extents"]["max"]) },
        },
        sw_mm: fixture["sw_mm"].as_f64().unwrap(),
        x_height_mm: fixture["x_height_mm"].as_f64().unwrap(),
        diagnostics,
    };
    let text = |key: &str| fixture["info"][key].as_str().unwrap().to_string();
    let info = SheetInfo { project: text("project"), drawn_by: text("drawn_by"), revision: text("revision"), date: text("date"), drawing_number: text("drawing_number") };

    let sheet = compose(&e, &sign, &layout, &info);
    assert_text_eq("synthetic-sheet.dxf", &std::fs::read_to_string(golden().join("synthetic-sheet.dxf")).unwrap(), &write_sheet_dxf(&sheet));
    let ours = write_sheet_pdf_stamped(&sheet, &fonts(&e), &stamp());
    assert_pdf_matches("synthetic-sheet.pdf", &std::fs::read(golden().join("synthetic-sheet.pdf")).unwrap(), &ours);
}

// ─── PDF comparison ───────────────────────────────────────────────────────

struct PdfObject {
    number: u32,
    /// The dictionary (or array) text.
    head: Vec<u8>,
    /// The stream's bytes as stored, when the object is a stream.
    stream: Option<Vec<u8>>,
}

struct Pdf {
    header: Vec<u8>,
    objects: Vec<PdfObject>,
    /// Offset of each object as the xref table gives it, then as it really is.
    xref: Vec<(usize, usize)>,
    /// From `trailer` to the end of the file.
    trailer: Vec<u8>,
    startxref_ok: bool,
}

fn find(haystack: &[u8], needle: &[u8], from: usize) -> Option<usize> {
    haystack[from..].windows(needle.len()).position(|w| w == needle).map(|i| i + from)
}

fn count(haystack: &[u8], needle: &[u8]) -> usize {
    haystack.windows(needle.len()).filter(|w| *w == needle).count()
}

/// The number after `key` in a dictionary, e.g. `/Length 123`.
fn number_after(dict: &[u8], key: &[u8]) -> Option<usize> {
    let at = find(dict, key, 0)? + key.len();
    let digits: Vec<u8> = dict[at..].iter().copied().take_while(u8::is_ascii_digit).collect();
    std::str::from_utf8(&digits).ok()?.parse().ok()
}

/// Reads a file of the shape both writers produce: a header, numbered objects one after the
/// other (each a one-line dictionary or array, optionally followed by a stream), a classic
/// cross-reference table and a trailer.
fn parse_pdf(name: &str, d: &[u8]) -> Pdf {
    let mut at = find(d, b"1 0 obj\n", 0).unwrap_or_else(|| panic!("{name}: no first object"));
    let header = d[..at].to_vec();
    let mut objects = Vec::new();
    let mut real_offsets = Vec::new();
    while !d[at..].starts_with(b"xref\n") {
        real_offsets.push(at);
        let line_end = find(d, b"\n", at).unwrap();
        let label = std::str::from_utf8(&d[at..line_end]).unwrap();
        let number: u32 = label.strip_suffix(" 0 obj").unwrap_or_else(|| panic!("{name}: expected an object at {at}, found `{label}`")).parse().unwrap();
        let head_start = line_end + 1;
        let head_end = find(d, b"\n", head_start).unwrap();
        let head = d[head_start..head_end].to_vec();
        let mut next = head_end + 1;
        let mut stream = None;
        if d[next..].starts_with(b"stream\n") {
            let length = number_after(&head, b"/Length ").unwrap_or_else(|| panic!("{name}: object {number} is a stream without /Length"));
            let start = next + 7;
            stream = Some(d[start..start + length].to_vec());
            assert!(d[start + length..].starts_with(b"\nendstream\n"), "{name}: object {number}: /Length does not end at endstream");
            next = start + length + 11;
        }
        assert!(d[next..].starts_with(b"endobj\n"), "{name}: object {number} does not end with endobj");
        at = next + 7;
        objects.push(PdfObject { number, head, stream });
    }
    let xref_at = at;
    let trailer_at = find(d, b"trailer\n", at).unwrap();
    let table = std::str::from_utf8(&d[at..trailer_at]).unwrap();
    let mut lines = table.lines();
    assert_eq!(lines.next(), Some("xref"), "{name}");
    assert_eq!(lines.next().unwrap(), format!("0 {}", objects.len() + 1), "{name}: xref size");
    assert_eq!(lines.next(), Some("0000000000 65535 f "), "{name}");
    let listed: Vec<usize> = lines.map(|l| l.strip_suffix(" 00000 n ").unwrap_or_else(|| panic!("{name}: xref line `{l}`")).parse().unwrap()).collect();
    assert_eq!(listed.len(), objects.len(), "{name}: xref entries");
    let trailer = d[trailer_at..].to_vec();
    let tail = std::str::from_utf8(&trailer).unwrap_or_else(|_| panic!("{name}: trailer is not text"));
    let startxref_ok = tail.ends_with(&format!("\nstartxref\n{xref_at}\n%%EOF\n"));
    Pdf { header, objects, xref: listed.into_iter().zip(real_offsets).collect(), trailer, startxref_ok }
}

/// Inflates a zlib stream, also one that stops without its final block and checksum. Returns
/// the bytes and whether the stream was complete.
fn inflate(data: &[u8]) -> (Vec<u8>, bool) {
    let mut z = Decompress::new(true);
    let mut out = Vec::with_capacity(data.len() * 4);
    loop {
        if out.capacity() - out.len() < 4096 {
            out.reserve(out.len().max(65536));
        }
        let (before_in, before_out) = (z.total_in(), z.total_out());
        let status = z.decompress_vec(&data[z.total_in() as usize..], &mut out, FlushDecompress::Sync).expect("valid deflate data");
        match status {
            Status::StreamEnd => return (out, true),
            _ if z.total_in() == before_in && z.total_out() == before_out => return (out, false),
            _ => {}
        }
    }
}

/// Replaces the text between `before` and the next `after` with `*`, at every occurrence.
fn mask(text: &mut Vec<u8>, before: &[u8], after: &[u8]) {
    let mut from = 0;
    while let Some(start) = find(text, before, from) {
        let value = start + before.len();
        let Some(end) = find(text, after, value) else { break };
        text.splice(value..end, *b"*");
        from = value + 1 + after.len();
    }
}

/// A PDF as the text the comparison is made on: every object in file order, streams inflated.
///
/// Masked, because the C# writer itself gives a different value on every run (two runs of
/// `tools/GoldenFab` were compared to establish the list):
///
/// - `/CreationDate` in the document information, `xmp:CreateDate` and `xmp:ModifyDate`;
/// - the document id: `/ID` in the trailer, `xmpMM:DocumentID` and `xmpMM:InstanceID`;
/// - the six-letter tag in front of each embedded font's name (`HQRSUN+Key#20Transport...`).
///
/// Masked, because the writers are different programs and say so:
///
/// - the producer: `/Producer`, `pdf:Producer` and the comment after the trailer dictionary.
///
/// Masked, because the two writers deflate with different compressors:
///
/// - the stored (compressed) bytes of a `/FlateDecode` stream, replaced by the inflated bytes;
/// - what follows from the compressed sizes: `/Length` of those streams, the offsets in the
///   cross-reference table and `startxref`. For our own files these are checked separately in
///   [`assert_well_formed`].
///
/// PDFsharp 6.2.4 does not finish its deflate streams (no final block, no Adler-32). No data is
/// lost by that, so [`inflate`] reads such a stream to its end and the inflated bytes are
/// compared in full. Ours are complete zlib streams ([`assert_well_formed`]).
fn canonical(pdf: &Pdf) -> Vec<(String, Vec<u8>, Option<Vec<u8>>)> {
    let mut out = Vec::new();
    for o in &pdf.objects {
        let mut head = o.head.clone();
        let mut stream = o.stream.clone();
        if find(&head, b"/Filter/FlateDecode", 0).is_some() {
            stream = Some(inflate(stream.as_ref().unwrap()).0);
            // In a content stream's dictionary `/Length` comes first and this also takes the
            // filter name with it, on both sides alike.
            mask(&mut head, b"/Length ", b">>");
        }
        mask(&mut head, b"/CreationDate(", b")");
        mask(&mut head, b"/Producer(", b")");
        mask(&mut head, b"/FontName/", b"+");
        mask(&mut head, b"/BaseFont/", b"+");
        if let Some(s) = stream.as_mut().filter(|_| find(&head, b"/Type/Metadata", 0).is_some()) {
            mask(s, b"<pdf:Producer>", b"</pdf:Producer>");
            mask(s, b"<xmp:CreateDate>", b"</xmp:CreateDate>");
            mask(s, b"<xmp:ModifyDate>", b"</xmp:ModifyDate>");
            mask(s, b"<xmpMM:DocumentID>", b"</xmpMM:DocumentID>");
            mask(s, b"<xmpMM:InstanceID>", b"</xmpMM:InstanceID>");
            // The masked values have different lengths.
            mask(&mut head, b"/Length ", b">>");
        }
        out.push((format!("object {}", o.number), head, stream));
    }
    let mut trailer = pdf.trailer.clone();
    mask(&mut trailer, b"/ID[", b"]");
    mask(&mut trailer, b">>% Created with ", b"\n");
    mask(&mut trailer, b"\nstartxref\n", b"\n");
    out.push(("trailer".to_string(), trailer, None));
    out
}

fn show(bytes: &[u8], at: usize) -> String {
    let from = at.saturating_sub(120);
    let to = (at + 120).min(bytes.len());
    String::from_utf8_lossy(&bytes[from..to]).into_owned()
}

fn assert_bytes_eq(name: &str, what: &str, reference: &[u8], ours: &[u8]) {
    if reference == ours {
        return;
    }
    let at = reference.iter().zip(ours).position(|(a, b)| a != b).unwrap_or(reference.len().min(ours.len()));
    panic!(
        "{name}: {what} differs at byte {at} (reference {} bytes, ours {} bytes)\n--- reference\n{}\n--- ours\n{}",
        reference.len(),
        ours.len(),
        show(reference, at),
        show(ours, at)
    );
}

fn assert_pdf_matches(name: &str, reference: &[u8], ours: &[u8]) {
    let (reference, ours) = (parse_pdf(name, reference), parse_pdf(name, ours));
    assert_well_formed(name, &ours);
    assert_bytes_eq(name, "header", &reference.header, &ours.header);
    let (r, o) = (canonical(&reference), canonical(&ours));
    assert_eq!(r.len(), o.len(), "{name}: number of objects");
    for ((label, r_head, r_stream), (_, o_head, o_stream)) in r.iter().zip(&o) {
        assert_bytes_eq(name, label, r_head, o_head);
        match (r_stream, o_stream) {
            (None, None) => {}
            (Some(r), Some(o)) => assert_bytes_eq(name, &format!("{label} stream"), r, o),
            _ => panic!("{name}: {label}: one side is a stream and the other is not"),
        }
    }
}

/// What the comparison masks, checked on our own file: the cross-reference table points at the
/// objects, `startxref` at the table, every deflated stream is complete, and `/Length1` is the
/// size of the font program.
fn assert_well_formed(name: &str, pdf: &Pdf) {
    assert!(pdf.startxref_ok, "{name}: startxref does not point at the xref table");
    for (listed, real) in &pdf.xref {
        assert_eq!(listed, real, "{name}: xref offset");
    }
    for o in &pdf.objects {
        let Some(stream) = &o.stream else { continue };
        if find(&o.head, b"/Filter/FlateDecode", 0).is_some() {
            let (data, complete) = inflate(stream);
            assert!(complete, "{name}: object {}: unfinished deflate stream", o.number);
            if let Some(length1) = number_after(&o.head, b"/Length1 ") {
                assert_eq!(length1, data.len(), "{name}: object {}: /Length1", o.number);
            }
        }
    }
}

#[test]
fn the_reference_pdfs_are_what_the_comparison_assumes() {
    // The reference's deflate streams stop without a final block, which is why `inflate` must
    // read unfinished streams. Should a regenerated reference have complete ones, this fails
    // and the special case can go.
    let d = std::fs::read(golden().join("schedule.pdf")).unwrap();
    let pdf = parse_pdf("schedule.pdf", &d);
    let content = pdf.objects.iter().find(|o| o.number == 5).unwrap();
    assert!(!inflate(content.stream.as_ref().unwrap()).1, "the reference content stream is a complete zlib stream");
}
