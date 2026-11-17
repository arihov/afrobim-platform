//! Clip, crop and export: LAS, LAZ and delimited text for a filtered subset, coordinates unchanged.
//!
//! LAS stores coordinates as integers times a scale plus an offset. When the scale and offset
//! are those of the source LAS file ([`ExportOptions::like_source`]) every coordinate is written
//! back exactly. Otherwise the scale is an input (there is no default) and each coordinate is
//! rounded to a multiple of the scale; the report gives the largest rounding error. The storage
//! offset, when not given, is the minimum of the header bounds floored to whole metres: it is a
//! file storage detail, and readers add it back, so true coordinates are unchanged.

use crate::buffer::{Bounds3, PointBuffer};
use crate::error::{PointCloudError, Result};
use crate::filter::PointFilter;
use crate::header::HeaderInfo;
use crate::index::point_in_ring;
use crate::source::PointSource;
use crate::text::TextRole;
use las::point::{Classification, Format};
use las::{Builder, Color, Point, Transform, Vector, Version, Writer};
use serde::{Deserialize, Serialize};
use std::io::Write;
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum LasVersion {
    /// LAS 1.2, point formats 0 to 3 (return numbers 1 to 7, classes 0 to 31).
    V1_2,
    /// LAS 1.4, point formats 6 and 7 (return numbers to 15, classes 0 to 255, WKT coordinate system).
    V1_4,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ExportOptions {
    pub version: LasVersion,
    /// Write LAZ (compressed) instead of LAS.
    pub laz: bool,
    /// Coordinate scale (x, y, z), metres per unit. Required unless the source header gives it.
    pub scale: Option<[f64; 3]>,
    /// Storage offset; see the module documentation.
    pub offset: Option<[f64; 3]>,
    /// Copy the coordinate system VLRs of this LAS or LAZ file (GeoTIFF keys; WKT only for 1.4).
    pub copy_crs_from: Option<PathBuf>,
    /// Write this WKT text as the coordinate system (LAS 1.4 only; ignored for 1.2).
    pub wkt: Option<String>,
    /// Text for the "generating software" header field.
    pub generating_software: String,
}

impl ExportOptions {
    /// Same scale and offset as a LAS source, so its coordinates are written back exactly. For a
    /// non-LAS header the scale stays unset.
    pub fn like_source(h: &HeaderInfo, version: LasVersion, laz: bool) -> ExportOptions {
        ExportOptions { version, laz, scale: h.scale, offset: h.offset, copy_crs_from: None, wkt: h.crs.wkt.clone(), generating_software: "afrobim-pointcloud".into() }
    }

    /// An explicit scale; the offset is chosen from the data.
    pub fn with_scale(scale: [f64; 3], version: LasVersion, laz: bool) -> ExportOptions {
        ExportOptions { version, laz, scale: Some(scale), offset: None, copy_crs_from: None, wkt: None, generating_software: "afrobim-pointcloud".into() }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ExportReport {
    pub points_written: u64,
    pub points_read: u64,
    pub scale: [f64; 3],
    pub offset: [f64; 3],
    /// Largest difference between a coordinate and the value stored (metres); 0 when the scale
    /// divides the coordinates exactly.
    pub max_rounding_error: f64,
    pub point_format: u8,
    /// Things not written as they were (a return number above 7 in LAS 1.2, a class above 31, CRS text that does not fit).
    pub notes: Vec<String>,
}

/// A region to keep when exporting a stream.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum ClipRegion {
    /// A box (use infinite z limits for plan only; infinities are written as null in JSON, so use large finite numbers for stored settings).
    Box(Bounds3),
    /// A plan polygon with an optional elevation range.
    Polygon { ring: Vec<[f64; 2]>, z_range: Option<(f64, f64)> },
}

impl ClipRegion {
    pub fn contains(&self, p: [f64; 3]) -> bool {
        match self {
            ClipRegion::Box(b) => b.contains(p),
            ClipRegion::Polygon { ring, z_range } => {
                if ring.len() < 3 {
                    return false;
                }
                if let Some((lo, hi)) = z_range {
                    if p[2] < *lo || p[2] > *hi {
                        return false;
                    }
                }
                let o = ring[0];
                // Quick reject by the ring's box, then the exact test relative to a ring vertex.
                point_in_ring([p[0] - o[0], p[1] - o[1]], &ring.iter().map(|r| [r[0] - o[0], r[1] - o[1]]).collect::<Vec<_>>())
            }
        }
    }
}

struct Plan {
    scale: [f64; 3],
    offset: [f64; 3],
    format_number: u8,
    has_rgb: bool,
    has_gps: bool,
}

fn build_header(opts: &ExportOptions, plan: &Plan, notes: &mut Vec<String>) -> Result<las::Header> {
    let version = match opts.version {
        LasVersion::V1_2 => Version::new(1, 2),
        LasVersion::V1_4 => Version::new(1, 4),
    };
    let mut b = Builder::from(version);
    b.point_format = Format::new(plan.format_number)?;
    b.point_format.is_compressed = opts.laz;
    b.generating_software = opts.generating_software.clone();
    b.transforms = Vector { x: Transform { scale: plan.scale[0], offset: plan.offset[0] }, y: Transform { scale: plan.scale[1], offset: plan.offset[1] }, z: Transform { scale: plan.scale[2], offset: plan.offset[2] } };
    if let Some(src) = &opts.copy_crs_from {
        let h = las::Header::new(std::io::BufReader::new(std::fs::File::open(src)?))?;
        let mut copied = false;
        for v in h.all_vlrs().filter(|v| v.is_crs()) {
            if v.is_wkt_crs() && opts.version != LasVersion::V1_4 {
                notes.push("the WKT coordinate system of the source was not copied: LAS 1.2 has no WKT record".into());
                continue;
            }
            b.vlrs.push(v.clone());
            if v.is_wkt_crs() {
                b.has_wkt_crs = true;
            }
            copied = true;
        }
        if !copied {
            notes.push("the source file has no coordinate system records to copy".into());
        }
    }
    let mut header = b.into_header()?;
    if let Some(w) = &opts.wkt {
        if opts.copy_crs_from.is_none() {
            if opts.version == LasVersion::V1_4 {
                header.set_wkt_crs(w.clone().into_bytes())?;
            } else {
                notes.push("the WKT coordinate system was not written: LAS 1.2 has no WKT record".into());
            }
        }
    }
    Ok(header)
}

fn plan_for(opts: &ExportOptions, has_rgb: bool, has_gps: bool, scale: [f64; 3], offset: [f64; 3]) -> Plan {
    let format_number = match (opts.version, has_gps, has_rgb) {
        (LasVersion::V1_2, false, false) => 0,
        (LasVersion::V1_2, true, false) => 1,
        (LasVersion::V1_2, false, true) => 2,
        (LasVersion::V1_2, true, true) => 3,
        (LasVersion::V1_4, _, false) => 6,
        (LasVersion::V1_4, _, true) => 7,
    };
    Plan { scale, offset, format_number, has_rgb, has_gps: has_gps || opts.version == LasVersion::V1_4 }
}

fn to_las_point(buf: &PointBuffer, i: usize, plan: &Plan, opts: &ExportOptions, notes: &mut Vec<String>, max_err: &mut f64) -> Point {
    let p = buf.xyz[i];
    let mut q = [0.0; 3];
    for k in 0..3 {
        // The value LAS will hold: round((p - offset) / scale) * scale + offset.
        let raw = ((p[k] - plan.offset[k]) / plan.scale[k]).round();
        q[k] = raw * plan.scale[k] + plan.offset[k];
        *max_err = max_err.max((q[k] - p[k]).abs());
    }
    let legacy = opts.version == LasVersion::V1_2;
    let (mut rn, mut nr, mut class) = (buf.return_number(i), buf.number_of_returns(i), buf.class[i]);
    if legacy {
        if rn > 7 || nr > 7 {
            if !notes.iter().any(|n| n.starts_with("return number")) {
                notes.push("return number or number of returns above 7 was clamped to 7 (LAS 1.2 has 3 bits)".into());
            }
            rn = rn.min(7);
            nr = nr.min(7);
        }
        if class > 31 {
            if !notes.iter().any(|n| n.starts_with("class")) {
                notes.push("class above 31 was written as 0 (LAS 1.2 formats 0 to 3 have 5 bits)".into());
            }
            class = 0;
        }
    }
    Point {
        x: p[0],
        y: p[1],
        z: p[2],
        intensity: buf.intensity[i],
        return_number: rn,
        number_of_returns: nr,
        // Class 12 is rejected by the typed constructor ("overlap" in the 1.2 table); keep the number.
        classification: Classification::new(class).unwrap_or(Classification::Reserved(class)),
        gps_time: plan.has_gps.then(|| buf.gps_time.as_ref().map_or(0.0, |g| g[i])),
        color: plan.has_rgb.then(|| buf.rgb.as_ref().map_or(Color::new(0, 0, 0), |c| Color::new(c[i][0], c[i][1], c[i][2]))),
        ..Default::default()
    }
}

fn floor_offset(min: [f64; 3]) -> [f64; 3] {
    [min[0].floor(), min[1].floor(), min[2].floor()]
}

fn check_scale(scale: [f64; 3]) -> Result<()> {
    if scale.iter().any(|s| !(*s > 0.0 && s.is_finite())) {
        return Err(PointCloudError::Invalid("the coordinate scale must be positive".into()));
    }
    Ok(())
}

/// Writes the points at `indices` (all when `None`) to a LAS or LAZ file.
pub fn export_las(path: &Path, buf: &PointBuffer, indices: Option<&[u32]>, opts: &ExportOptions) -> Result<ExportReport> {
    let scale = opts.scale.ok_or_else(|| PointCloudError::Invalid("the export needs a coordinate scale (an input; use ExportOptions::like_source for a LAS source)".into()))?;
    check_scale(scale)?;
    let all: Vec<u32>;
    let idx: &[u32] = match indices {
        Some(i) => i,
        None => {
            all = (0..buf.len() as u32).collect();
            &all
        }
    };
    let mut b = Bounds3::EMPTY;
    for &i in idx {
        b.include(buf.xyz[i as usize]);
    }
    let offset = opts.offset.unwrap_or_else(|| if b.is_empty() { [0.0; 3] } else { floor_offset(b.min) });
    let plan = plan_for(opts, buf.rgb.is_some(), buf.gps_time.is_some(), scale, offset);
    let mut notes = Vec::new();
    let header = build_header(opts, &plan, &mut notes)?;
    let mut w = Writer::from_path(path, header)?;
    let mut max_err = 0.0f64;
    for &i in idx {
        let p = to_las_point(buf, i as usize, &plan, opts, &mut notes, &mut max_err);
        w.write_point(p)?;
    }
    w.close()?;
    Ok(ExportReport { points_written: idx.len() as u64, points_read: idx.len() as u64, scale, offset, max_rounding_error: max_err, point_format: plan.format_number, notes })
}

/// Streams a source through a filter and an optional clip region into a LAS or LAZ file, chunk by
/// chunk, so the source may be larger than memory.
pub fn export_stream(source: &mut dyn PointSource, path: &Path, filter: &PointFilter, clip: Option<&ClipRegion>, opts: &ExportOptions) -> Result<ExportReport> {
    let h = source.header().clone();
    let scale = opts.scale.or(h.scale).ok_or_else(|| PointCloudError::Invalid("the export needs a coordinate scale: the source has none (an input; there is no default)".into()))?;
    check_scale(scale)?;
    let offset = match opts.offset {
        Some(o) => o,
        None if opts.scale.is_none() && h.offset.is_some() => h.offset.unwrap_or([0.0; 3]),
        None => match h.bounds {
            Some(b) => floor_offset(b.min),
            None => return Err(PointCloudError::Invalid("the source has no bounds to choose a storage offset from; give one".into())),
        },
    };
    // Storage must hold every coordinate: check the header bounds against the i32 range.
    if let Some(b) = h.bounds {
        for k in 0..3 {
            for v in [b.min[k], b.max[k]] {
                if ((v - offset[k]) / scale[k]).abs() >= i32::MAX as f64 {
                    return Err(PointCloudError::Invalid("coordinates do not fit the LAS integer range with this scale and offset; use a larger scale or another offset".into()));
                }
            }
        }
    }
    let plan = plan_for(opts, h.has_rgb, h.has_gps_time, scale, offset);
    let mut notes = Vec::new();
    let header = build_header(opts, &plan, &mut notes)?;
    let mut w = Writer::from_path(path, header)?;
    let f = filter.compile();
    let mut chunk = source.new_buffer(250_000);
    let (mut read, mut written, mut max_err) = (0u64, 0u64, 0.0f64);
    loop {
        chunk.clear();
        let n = source.read_chunk(250_000, &mut chunk)?;
        if n == 0 {
            break;
        }
        read += n as u64;
        for i in 0..n {
            if f.matches(&chunk, i) && clip.is_none_or(|c| c.contains(chunk.xyz[i])) {
                let p = to_las_point(&chunk, i, &plan, opts, &mut notes, &mut max_err);
                w.write_point(p)?;
                written += 1;
            }
        }
    }
    w.close()?;
    Ok(ExportReport { points_written: written, points_read: read, scale, offset, max_rounding_error: max_err, point_format: plan.format_number, notes })
}

/// How to write text.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct TextExportOptions {
    pub delimiter: String,
    /// Decimals for coordinates (an input: 3 is millimetres).
    pub decimals: usize,
    /// Columns in order. `X`, `Y`, `Z`, `Intensity`, `Red`, `Green`, `Blue`, `Class`, `ReturnNumber`,
    /// `NumberOfReturns` and `GpsTime` are written; `Ignore` is skipped.
    pub columns: Vec<TextRole>,
    /// Write a header line with the column names.
    pub header_line: bool,
    /// Largest colour value to write: stored 16-bit colour is scaled to this (255 or 65535).
    pub rgb_max: f64,
}

/// Writes points as delimited text; coordinates are written with the given decimals, in true
/// coordinates. Returns the number of points written.
pub fn export_text(path: &Path, buf: &PointBuffer, indices: Option<&[u32]>, opts: &TextExportOptions) -> Result<u64> {
    if opts.columns.iter().all(|c| *c == TextRole::Ignore) || opts.delimiter.is_empty() || !(opts.rgb_max > 0.0) {
        return Err(PointCloudError::Invalid("the text export needs a delimiter, a colour maximum and at least one column".into()));
    }
    let cols: Vec<TextRole> = opts.columns.iter().copied().filter(|c| *c != TextRole::Ignore).collect();
    let mut w = std::io::BufWriter::new(std::fs::File::create(path)?);
    if opts.header_line {
        let names: Vec<&str> = cols.iter().map(|c| match c {
            TextRole::X => "X",
            TextRole::Y => "Y",
            TextRole::Z => "Z",
            TextRole::Intensity => "Intensity",
            TextRole::Red => "R",
            TextRole::Green => "G",
            TextRole::Blue => "B",
            TextRole::Class => "Class",
            TextRole::ReturnNumber => "ReturnNumber",
            TextRole::NumberOfReturns => "NumberOfReturns",
            TextRole::GpsTime => "GpsTime",
            TextRole::Ignore => "",
        }).collect();
        writeln!(w, "{}", names.join(&opts.delimiter))?;
    }
    let all: Vec<u32>;
    let idx: &[u32] = match indices {
        Some(i) => i,
        None => {
            all = (0..buf.len() as u32).collect();
            &all
        }
    };
    let d = opts.decimals;
    let mut line = String::new();
    for &i in idx {
        let i = i as usize;
        line.clear();
        for (k, c) in cols.iter().enumerate() {
            if k > 0 {
                line.push_str(&opts.delimiter);
            }
            use std::fmt::Write as _;
            let rgb = |ch: usize| buf.rgb.as_ref().map_or(0.0, |v| (v[i][ch] as f64 / 65535.0 * opts.rgb_max).round());
            let _ = match c {
                TextRole::X => write!(line, "{:.d$}", buf.xyz[i][0]),
                TextRole::Y => write!(line, "{:.d$}", buf.xyz[i][1]),
                TextRole::Z => write!(line, "{:.d$}", buf.xyz[i][2]),
                TextRole::Intensity => write!(line, "{}", buf.intensity[i]),
                TextRole::Red => write!(line, "{}", rgb(0)),
                TextRole::Green => write!(line, "{}", rgb(1)),
                TextRole::Blue => write!(line, "{}", rgb(2)),
                TextRole::Class => write!(line, "{}", buf.class[i]),
                TextRole::ReturnNumber => write!(line, "{}", buf.return_number(i)),
                TextRole::NumberOfReturns => write!(line, "{}", buf.number_of_returns(i)),
                TextRole::GpsTime => write!(line, "{:.6}", buf.gps_time.as_ref().map_or(0.0, |g| g[i])),
                TextRole::Ignore => Ok(()),
            };
        }
        writeln!(w, "{line}")?;
    }
    w.flush()?;
    Ok(idx.len() as u64)
}
