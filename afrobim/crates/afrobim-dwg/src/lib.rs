//! Built-in DWG reading and writing, as ASCII DXF text.
//!
//! AfroBIM reads DXF with its own reader (`afrobim-dxf`). This crate is the bridge that lets it
//! read and write DWG without any other program installed: the `acadrust` crate (MPL-2.0, used
//! unmodified as a separate library; its DWG code is ported from ACadSharp, MIT) decodes the DWG,
//! and its DXF writer writes what it read as ASCII DXF, which the existing reader reads like any
//! other DXF. Writing goes the other way: the DXF text AfroBIM's exporter produces is read by
//! acadrust and saved as a DWG of the version asked for. Nothing outside this crate sees
//! acadrust's types.
//!
//! A DWG from outside is untrusted. Its size is checked against [`Limits`] before the decoder
//! runs, the decoder's total decompressed allocation is capped, the DXF it becomes is capped while
//! it is written, and every call into acadrust runs under `catch_unwind`. That is not enough on its
//! own: an allocation the system refuses, or a decoder that never finishes, cannot be caught in
//! process. So the application runs these functions in a child process of its own (see
//! `afrobim-app/src/dwg_bridge.rs`), with a wall-clock limit, and a file that brings the decoder
//! down takes only the child with it.
//!
//! What the built-in reader keeps of other programs' objects: acadrust writes Civil 3D objects
//! (alignments, surfaces, points, labels …) as `ACAD_PROXY_ENTITY` with the proxy graphics the
//! drawing saved, as ODA File Converter does, so `afrobim-dxf` draws them the same way. Measured on
//! two Civil 3D 2018 (AC1032) project drawings: one gave the same entity counts and the same drawn
//! picture (primitive for primitive, same extents) as ODA File Converter; the other gave the same
//! entity counts, but 13 two-dimensional polylines inside blocks came out as LWPOLYLINE where ODA
//! keeps POLYLINE, and the drawn picture had about 14 % more primitives and extents reaching about
//! 195 drawing units further west. Which of the two is right there has not been checked in
//! AutoCAD. ODA File Converter remains the fallback when the built-in reader fails.
//!
//! The design follows BIMCraft's DWG bridge (crates/dwg, MIT OR Apache-2.0,
//! Copyright 2026 IPedrax and the BIMCraft contributors); see NOTICE at the repository root.

#![forbid(unsafe_code)]

use std::io::{Cursor, Write};
use std::panic::{catch_unwind, AssertUnwindSafe};

/// How large a DWG, and the DXF it becomes, may be.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Limits {
    /// The largest DWG read at all, checked before the decoder runs.
    pub dwg_bytes: usize,
    /// The largest DXF a DWG may become, checked while it is written. A DXF is several times
    /// the size of its DWG (a 17 MB Civil 3D drawing gave 142 MB through ODA File Converter).
    pub dxf_bytes: usize,
    /// The most the decoder may decompress in one read, across all sections.
    pub decompressed_bytes: usize,
}

impl Limits {
    /// 128 MB of DWG, 512 MB of DXF and 1 GB decompressed in one read.
    pub const DEFAULT: Limits = Limits { dwg_bytes: 128 << 20, dxf_bytes: 512 << 20, decompressed_bytes: 1 << 30 };
}

impl Default for Limits {
    fn default() -> Self {
        Self::DEFAULT
    }
}

/// The DWG versions written.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum DwgVersion {
    /// AutoCAD 2004 to 2006 (AC1018): opened by every AutoCAD and Civil 3D since 2004.
    R2004,
    /// AutoCAD 2007 to 2009 (AC1021).
    R2007,
    /// AutoCAD 2010 to 2012 (AC1024).
    R2010,
    /// AutoCAD 2013 to 2017 (AC1027).
    R2013,
    /// AutoCAD 2018 and later (AC1032).
    R2018,
}

impl DwgVersion {
    /// Every version written, oldest first.
    pub const ALL: [DwgVersion; 5] = [DwgVersion::R2004, DwgVersion::R2007, DwgVersion::R2010, DwgVersion::R2013, DwgVersion::R2018];

    /// The version code at the start of the file, such as `AC1032`.
    pub fn code(self) -> &'static str {
        match self {
            DwgVersion::R2004 => "AC1018",
            DwgVersion::R2007 => "AC1021",
            DwgVersion::R2010 => "AC1024",
            DwgVersion::R2013 => "AC1027",
            DwgVersion::R2018 => "AC1032",
        }
    }

    /// The release name, such as `AutoCAD 2018`.
    pub fn name(self) -> &'static str {
        match self {
            DwgVersion::R2004 => "AutoCAD 2004",
            DwgVersion::R2007 => "AutoCAD 2007",
            DwgVersion::R2010 => "AutoCAD 2010",
            DwgVersion::R2013 => "AutoCAD 2013",
            DwgVersion::R2018 => "AutoCAD 2018",
        }
    }

    /// The version a code (`AC1018`) or a year (`2004`, `R2004`) names, case ignored.
    pub fn parse(s: &str) -> Option<DwgVersion> {
        let s = s.trim().to_ascii_uppercase();
        let s = s.strip_prefix('R').unwrap_or(&s);
        DwgVersion::ALL.into_iter().find(|v| v.code() == s || v.name().ends_with(s))
    }

    fn acad(self) -> acadrust::DxfVersion {
        match self {
            DwgVersion::R2004 => acadrust::DxfVersion::AC1018,
            DwgVersion::R2007 => acadrust::DxfVersion::AC1021,
            DwgVersion::R2010 => acadrust::DxfVersion::AC1024,
            DwgVersion::R2013 => acadrust::DxfVersion::AC1027,
            DwgVersion::R2018 => acadrust::DxfVersion::AC1032,
        }
    }
}

/// True when the first bytes are a DWG's version code: `AC10` and two digits (`AC1015`, `AC1032`).
pub fn is_dwg(head: &[u8]) -> bool {
    head.len() >= 6 && head.starts_with(b"AC10") && head[4..6].iter().all(u8::is_ascii_digit)
}

/// The version code a DWG starts with (`AC1032`), or `None` when it is not a DWG.
pub fn version_of(head: &[u8]) -> Option<String> {
    is_dwg(head).then(|| String::from_utf8_lossy(&head[..6]).into_owned())
}

/// The release a DWG version code belongs to, for messages.
pub fn release_name(code: &str) -> &'static str {
    match code {
        "AC1009" => "AutoCAD R11/R12",
        "AC1012" => "AutoCAD R13",
        "AC1014" => "AutoCAD R14",
        "AC1015" => "AutoCAD 2000",
        "AC1018" => "AutoCAD 2004",
        "AC1021" => "AutoCAD 2007",
        "AC1024" => "AutoCAD 2010",
        "AC1027" => "AutoCAD 2013",
        "AC1032" => "AutoCAD 2018",
        _ => "an unrecognised release",
    }
}

/// A DWG read: the ASCII DXF it became, and notes on what the read left out.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Converted {
    pub dxf: String,
    pub notes: Vec<String>,
}

/// The ASCII DXF text a DWG holds, within [`Limits::DEFAULT`].
pub fn dwg_to_dxf(bytes: &[u8]) -> Result<String, String> {
    dwg_to_dxf_with(bytes, Limits::DEFAULT).map(|c| c.dxf)
}

/// The ASCII DXF text a DWG holds, within `limits`, with notes on what was left out.
///
/// The read is failsafe: an object record the decoder cannot read is left out and counted rather
/// than failing the whole file. The DXF is written as release 2007 or later so that its text is
/// UTF-8 whatever code page an older drawing used.
pub fn dwg_to_dxf_with(bytes: &[u8], limits: Limits) -> Result<Converted, String> {
    if bytes.len() > limits.dwg_bytes {
        return Err(format!("the file is {}, larger than the {} of DWG AfroBIM reads", mb(bytes.len()), mb(limits.dwg_bytes)));
    }
    let Some(code) = version_of(bytes) else {
        return Err("it is not a DWG file (a DWG begins with its version code, such as AC1032)".into());
    };
    let options = acadrust::DwgReadOptions::failsafe().with_max_decompressed_bytes(limits.decompressed_bytes);
    let read = catch_unwind(|| acadrust::DwgReader::from_stream_with_options(Cursor::new(bytes), options).read_with_stats())
        .map_err(|_| format!("the DWG decoder failed on this {code} file"))?
        .map_err(|e| format!("the DWG could not be read ({code}, {}): {e}", release_name(&code)))?;
    if !read.stats.record_stream_read {
        let why = read.stats.diagnostics.first().map(|d| d.message.trim_start_matches("Failsafe: ").to_string());
        return Err(format!("the DWG could not be read ({code}): {}", why.unwrap_or_else(|| "its objects could not be read".into())));
    }
    let notes = skipped(&read.stats);
    let target = if read.document.version < acadrust::DxfVersion::AC1021 { acadrust::DxfVersion::AC1021 } else { read.document.version };
    let mut out = Capped { buf: Vec::new(), limit: limits.dxf_bytes, over: false };
    let written = catch_unwind(AssertUnwindSafe(|| acadrust::DxfWriter::new(&read.document).with_version(target).write_to_writer(&mut out)))
        .map_err(|_| "the DWG decoder failed writing this drawing as DXF".to_string())?;
    if out.over {
        return Err(format!("this {} DWG became more than {} of DXF, the most AfroBIM reads; the file may be damaged", mb(bytes.len()), mb(limits.dxf_bytes)));
    }
    written.map_err(|e| format!("the DWG could not be written as DXF: {e}"))?;
    let dxf = match String::from_utf8(out.buf) {
        Ok(s) => s,
        // UTF-8 is expected from 2007 on; anything else is kept, with the odd bytes replaced.
        Err(e) => String::from_utf8_lossy(e.as_bytes()).into_owned(),
    };
    Ok(Converted { dxf, notes })
}

/// The note on what a failsafe read left out: how many object records, of which kinds; none when
/// it read them all.
fn skipped(stats: &acadrust::ReadStats) -> Vec<String> {
    let n = stats.skipped_source_records;
    if n == 0 {
        return Vec::new();
    }
    let mut kinds: Vec<String> = stats.diagnostics.iter().filter(|d| d.record_handle.is_some()).filter_map(|d| d.record_type.clone()).collect();
    kinds.sort();
    kinds.dedup();
    kinds.truncate(6);
    let of = if stats.source_records > 0 { format!(" of {}", stats.source_records) } else { String::new() };
    let which = if kinds.is_empty() { String::new() } else { format!(" (of types {})", kinds.join(", ")) };
    vec![format!("{n}{of} of the DWG's object records could not be read and were left out{which}.")]
}

/// The DWG of `version` that DXF text becomes, within [`Limits::DEFAULT`].
pub fn dxf_to_dwg(dxf_text: &str, version: DwgVersion) -> Result<Vec<u8>, String> {
    dxf_to_dwg_with(dxf_text, version, Limits::DEFAULT)
}

/// The DWG of `version` that DXF text becomes, within `limits` (the DXF checked before it is read,
/// the DWG after it is written).
pub fn dxf_to_dwg_with(dxf_text: &str, version: DwgVersion, limits: Limits) -> Result<Vec<u8>, String> {
    if dxf_text.len() > limits.dxf_bytes {
        return Err(format!("the drawing is {} of DXF, more than the {} AfroBIM writes as DWG", mb(dxf_text.len()), mb(limits.dxf_bytes)));
    }
    let data = dxf_text.as_bytes().to_vec();
    let mut doc = catch_unwind(move || acadrust::DxfReader::from_reader(Cursor::new(data)).and_then(acadrust::DxfReader::read))
        .map_err(|_| "the DWG writer failed reading the drawing".to_string())?
        .map_err(|e| format!("the drawing could not be read for DWG: {e}"))?;
    doc.version = version.acad();
    catch_unwind(AssertUnwindSafe(|| variables(&mut doc))).map_err(|_| "the DWG writer failed on this drawing".to_string())?;
    let dwg = catch_unwind(AssertUnwindSafe(|| acadrust::DwgWriter::write_to_vec(&doc)))
        .map_err(|_| "the DWG writer failed on this drawing".to_string())?
        .map_err(|e| format!("the DWG could not be written: {e}"))?;
    if dwg.len() > limits.dwg_bytes {
        return Err(format!("the DWG would be {}, more than the {} AfroBIM writes", mb(dwg.len()), mb(limits.dwg_bytes)));
    }
    if !dwg.starts_with(version.code().as_bytes()) {
        return Err(format!("the DWG writer gave back something other than a {} DWG", version.code()));
    }
    Ok(dwg)
}

/// Adds the drawing variables dictionary (`AcDbVariableDictionary`) AutoCAD writes in every
/// drawing's named objects, when the drawing has none. Some readers refuse a 2004 DWG without it
/// (BIMCraft and CADCraft found this with Vectric Aspire).
fn variables(doc: &mut acadrust::CadDocument) {
    use acadrust::objects::{Dictionary, DictionaryVariable, ObjectType};
    const NAME: &str = "AcDbVariableDictionary";
    let root = doc.header.named_objects_dict_handle;
    match doc.objects.get(&root) {
        Some(ObjectType::Dictionary(d)) if d.entries.iter().all(|(key, _)| !key.eq_ignore_ascii_case(NAME)) => {}
        _ => return,
    }
    let handle = doc.allocate_handle();
    let mut dictionary = Dictionary::new();
    dictionary.handle = handle;
    dictionary.owner = root;
    for (name, value) in [("CANNOSCALE", "1:1"), ("CMLEADERSTYLE", "Standard"), ("CTABLESTYLE", "Standard"), ("DIMASSOC", "2"), ("HIDETEXT", "1")] {
        let h = doc.allocate_handle();
        let mut variable = DictionaryVariable::new(name, value);
        variable.handle = h;
        variable.owner_handle = handle;
        dictionary.add_entry(name, h);
        doc.objects.insert(h, ObjectType::DictionaryVariable(variable));
    }
    doc.objects.insert(handle, ObjectType::Dictionary(dictionary));
    if let Some(ObjectType::Dictionary(r)) = doc.objects.get_mut(&root) {
        r.add_entry(NAME, handle);
    }
}

/// The DXF as it is written, refused once it would pass `limit`.
struct Capped {
    buf: Vec<u8>,
    limit: usize,
    over: bool,
}

impl Write for Capped {
    fn write(&mut self, b: &[u8]) -> std::io::Result<usize> {
        if self.over || self.buf.len().saturating_add(b.len()) > self.limit {
            self.over = true;
            return Err(std::io::Error::other("the DXF size limit"));
        }
        self.buf.extend_from_slice(b);
        Ok(b.len())
    }

    fn flush(&mut self) -> std::io::Result<()> {
        Ok(())
    }
}

fn mb(n: usize) -> String {
    format!("{:.1} MB", n as f64 / f64::from(1u32 << 20))
}

#[cfg(test)]
mod tests;
