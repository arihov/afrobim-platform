// SPDX-License-Identifier: LicenseRef-AfroBIM-Proprietary
// Copyright (c) 2026 AfroBIM. All rights reserved.
//! Readers: LAS 1.0 to 1.4, LAZ, E57 and delimited text, all as chunked streams.
//!
//! [`read_header`] returns the header facts without loading points (text files need one pass).
//! [`open_source`] returns a [`PointSource`] that yields points in chunks, so a file larger than
//! memory can be scanned (statistics, filters, export) or indexed with a point limit.
//!
//! LAS and LAZ go through the `las` crate (MIT) with its `laz` feature (pure Rust LAZ), E57
//! through the `e57` crate (MIT, pure Rust). Coordinates are `raw * scale + offset` in `f64`,
//! exactly as the file gives them, never recentred.

use crate::buffer::{Bounds3, PointBuffer, PointRecord};
use crate::classes::ClassTable;
use crate::error::{PointCloudError, Result};
use crate::header::{epsg_from_wkt, CrsInfo, GeoKeyEntry, GeoKeyValue, HeaderInfo, SourceKind};
use crate::text::{detect_text_format, parse_line, resolve_delimiter, Delimiter, TextFormat, TextRole};
use std::fs::File;
use std::io::{BufRead, BufReader, Read};
use std::path::Path;
use std::sync::mpsc::{sync_channel, Receiver};

/// Options for opening a file.
#[derive(Debug, Clone, PartialEq)]
pub struct SourceOptions {
    /// Text files: how to read them. `None` uses the detected proposal (see
    /// [`detect_text_format`]); pass your own after showing the preview to the user.
    pub text: Option<TextFormat>,
    /// E57: apply each scan's pose (rotation and translation) so scans share one frame. The pose
    /// is part of what the file states; without it points stay in the scanner's own frame.
    pub e57_apply_pose: bool,
}

impl Default for SourceOptions {
    fn default() -> Self {
        SourceOptions { text: None, e57_apply_pose: true }
    }
}

/// A stream of points.
pub trait PointSource {
    fn header(&self) -> &HeaderInfo;

    /// Appends up to `max` points to `out` and returns how many were added; 0 means the end.
    /// `out` keeps its column layout (see [`PointBuffer::push`]).
    fn read_chunk(&mut self, max: usize, out: &mut PointBuffer) -> Result<usize>;

    /// An empty buffer with the columns this source provides.
    fn new_buffer(&self, capacity: usize) -> PointBuffer {
        let h = self.header();
        PointBuffer::with_capacity(capacity, h.has_rgb, h.has_gps_time)
    }
}

/// Which kind a file is: by its first bytes, then by extension.
pub fn detect_kind(path: &Path) -> Result<SourceKind> {
    let mut magic = [0u8; 8];
    let n = File::open(path)?.read(&mut magic)?;
    if n >= 4 && &magic[..4] == b"LASF" {
        let compressed = path.extension().is_some_and(|e| e.eq_ignore_ascii_case("laz"));
        // The compression flag is in the point format byte; Header::new knows. Extension is a hint only.
        return Ok(if compressed { SourceKind::Laz } else { SourceKind::Las });
    }
    if n == 8 && &magic == b"ASTM-E57" {
        return Ok(SourceKind::E57);
    }
    let ext = path.extension().map(|e| e.to_string_lossy().to_ascii_lowercase()).unwrap_or_default();
    match ext.as_str() {
        "txt" | "xyz" | "pts" | "csv" | "asc" | "xyzrgb" | "xyzi" => Ok(SourceKind::Text),
        _ => Err(PointCloudError::Format(format!("{}: not LAS, LAZ or E57 by its first bytes, and the extension {ext:?} is not a known text point extension", path.display()))),
    }
}

fn las_crs(h: &las::Header) -> CrsInfo {
    let mut crs = CrsInfo::default();
    if let Some(bytes) = h.get_wkt_crs_bytes() {
        let text = String::from_utf8_lossy(bytes).trim_end_matches('\0').trim().to_string();
        if !text.is_empty() {
            if let Some(e) = epsg_from_wkt(&text) {
                crs.epsg = Some(e);
                crs.epsg_source = Some("EPSG authority in the WKT coordinate system VLR".into());
            }
            crs.wkt = Some(text);
        }
    }
    if let Ok(Some(g)) = h.get_geotiff_crs() {
        for e in &g.entries {
            let value = match &e.data {
                las::crs::GeoTiffData::U16(v) => GeoKeyValue::Short(*v),
                las::crs::GeoTiffData::String(s) => GeoKeyValue::Text(s.clone()),
                las::crs::GeoTiffData::Doubles(d) => GeoKeyValue::Doubles(d.clone()),
            };
            crs.geotiff_keys.push(GeoKeyEntry { id: e.id, value });
        }
        // GeoTIFF: values 1024..=32766 are EPSG codes; 0 and 32767 are not.
        let short = |id: u16| {
            crs.geotiff_keys.iter().find_map(|k| match (&k.value, k.id == id) {
                (GeoKeyValue::Short(v), true) if (1024..=32766).contains(v) => Some(*v as u32),
                _ => None,
            })
        };
        if crs.epsg.is_none() {
            if let Some(p) = short(3072) {
                crs.epsg = Some(p);
                crs.epsg_source = Some("GeoTIFF ProjectedCRSGeoKey (3072)".into());
            } else if let Some(p) = short(2048) {
                crs.epsg = Some(p);
                crs.epsg_source = Some("GeoTIFF GeodeticCRSGeoKey (2048)".into());
            }
        }
        crs.vertical_epsg = short(4096);
    }
    crs
}

fn las_header_info(h: &las::Header, kind: SourceKind) -> HeaderInfo {
    let b = h.bounds();
    let t = h.transforms();
    let f = h.point_format();
    let point_format = f.to_u8().ok();
    let bounds = Bounds3::new([b.min.x, b.min.y, b.min.z], [b.max.x, b.max.y, b.max.z]);
    let usable = bounds.min.iter().chain(bounds.max.iter()).all(|v| v.is_finite()) && !bounds.is_empty();
    HeaderInfo {
        kind: if f.is_compressed { SourceKind::Laz } else if kind == SourceKind::Laz { SourceKind::Las } else { kind },
        version: format!("{}.{}", h.version().major, h.version().minor),
        point_format,
        point_count: h.number_of_points(),
        bounds: usable.then_some(bounds),
        bounds_exact: false,
        scale: Some([t.x.scale, t.y.scale, t.z.scale]),
        offset: Some([t.x.offset, t.y.offset, t.z.offset]),
        has_rgb: f.has_color,
        has_gps_time: f.has_gps_time,
        has_intensity: true,
        compressed: f.is_compressed,
        crs: las_crs(h),
        class_table: point_format.map_or(ClassTable::None, ClassTable::for_las_format),
        generating_software: h.generating_software().to_string(),
        system_identifier: h.system_identifier().to_string(),
        from_scan: false,
    }
}

fn rotate(q: &e57::Quaternion, v: [f64; 3]) -> [f64; 3] {
    let n = (q.w * q.w + q.x * q.x + q.y * q.y + q.z * q.z).sqrt();
    let (w, x, y, z) = if n > 0.0 { (q.w / n, q.x / n, q.y / n, q.z / n) } else { (1.0, 0.0, 0.0, 0.0) };
    [
        (1.0 - 2.0 * (y * y + z * z)) * v[0] + 2.0 * (x * y - w * z) * v[1] + 2.0 * (x * z + w * y) * v[2],
        2.0 * (x * y + w * z) * v[0] + (1.0 - 2.0 * (x * x + z * z)) * v[1] + 2.0 * (y * z - w * x) * v[2],
        2.0 * (x * z - w * y) * v[0] + 2.0 * (y * z + w * x) * v[1] + (1.0 - 2.0 * (x * x + y * y)) * v[2],
    ]
}

fn e57_header_info(path: &Path, opts: &SourceOptions) -> Result<HeaderInfo> {
    let reader = e57::E57Reader::from_file(path)?;
    let h = reader.header();
    let pcs = reader.pointclouds();
    let mut count = 0u64;
    let mut bounds = Bounds3::EMPTY;
    let mut have_bounds = !pcs.is_empty();
    let (mut rgb, mut intensity) = (false, false);
    for pc in &pcs {
        count += pc.records;
        rgb |= pc.has_color();
        intensity |= pc.has_intensity();
        match pc.cartesian_bounds.as_ref() {
            Some(cb) if cb.x_min.is_some() && cb.x_max.is_some() && cb.y_min.is_some() && cb.y_max.is_some() => {
                let lo = [cb.x_min.unwrap_or(0.0), cb.y_min.unwrap_or(0.0), cb.z_min.unwrap_or(0.0)];
                let hi = [cb.x_max.unwrap_or(0.0), cb.y_max.unwrap_or(0.0), cb.z_max.unwrap_or(0.0)];
                match (&pc.transform, opts.e57_apply_pose) {
                    (Some(t), true) => {
                        for k in 0..8 {
                            let c = [if k & 1 == 0 { lo[0] } else { hi[0] }, if k & 2 == 0 { lo[1] } else { hi[1] }, if k & 4 == 0 { lo[2] } else { hi[2] }];
                            let r = rotate(&t.rotation, c);
                            bounds.include([r[0] + t.translation.x, r[1] + t.translation.y, r[2] + t.translation.z]);
                        }
                    }
                    _ => {
                        bounds.include(lo);
                        bounds.include(hi);
                    }
                }
            }
            _ => have_bounds = false,
        }
    }
    let mut crs = CrsInfo::default();
    if let Some(text) = reader.coordinate_metadata() {
        let text = text.trim().to_string();
        if !text.is_empty() {
            crs.epsg = epsg_from_wkt(&text);
            if crs.epsg.is_some() {
                crs.epsg_source = Some("EPSG authority in the E57 coordinateMetadata text".into());
            }
            crs.wkt = Some(text);
        }
    }
    Ok(HeaderInfo {
        kind: SourceKind::E57,
        version: format!("{}.{}", h.major, h.minor),
        point_format: None,
        point_count: count,
        bounds: have_bounds.then_some(bounds),
        // Cartesian bounds in the file are the stated limits, not checked against the points.
        bounds_exact: false,
        scale: None,
        offset: None,
        has_rgb: rgb,
        has_gps_time: false,
        has_intensity: intensity,
        compressed: false,
        crs,
        class_table: ClassTable::None,
        generating_software: reader.library_version().unwrap_or("").to_string(),
        system_identifier: String::new(),
        from_scan: false,
    })
}

/// Header facts without loading points (text files: one pass over the file).
pub fn read_header(path: &Path, opts: &SourceOptions) -> Result<HeaderInfo> {
    match detect_kind(path)? {
        k @ (SourceKind::Las | SourceKind::Laz) => {
            let h = las::Header::new(BufReader::new(File::open(path)?))?;
            Ok(las_header_info(&h, k))
        }
        SourceKind::E57 => e57_header_info(path, opts),
        SourceKind::Text => Ok(TextSource::open(path, opts)?.header.clone()),
    }
}

/// Opens a file for chunked reading.
pub fn open_source(path: &Path, opts: &SourceOptions) -> Result<Box<dyn PointSource>> {
    Ok(match detect_kind(path)? {
        SourceKind::Las | SourceKind::Laz => Box::new(LasSource::open(path)?),
        SourceKind::E57 => Box::new(E57Source::open(path, opts)?),
        SourceKind::Text => Box::new(TextSource::open(path, opts)?),
    })
}

// ---------------------------------------------------------------- LAS / LAZ

struct LasSource {
    header: HeaderInfo,
    reader: las::Reader,
    data: las::PointData,
}

impl LasSource {
    fn open(path: &Path) -> Result<LasSource> {
        let reader = las::Reader::from_path(path)?;
        let header = las_header_info(reader.header(), SourceKind::Las);
        let data = las::PointDataBuilder::new().for_header(reader.header()).build();
        Ok(LasSource { header, reader, data })
    }
}

impl PointSource for LasSource {
    fn header(&self) -> &HeaderInfo {
        &self.header
    }

    fn read_chunk(&mut self, max: usize, out: &mut PointBuffer) -> Result<usize> {
        let n = self.reader.fill_points(max as u64, &mut self.data)? as usize;
        if n == 0 {
            return Ok(0);
        }
        let pd = &self.data;
        let before = out.len();
        out.xyz.extend(pd.x().zip(pd.y()).zip(pd.z()).map(|((x, y), z)| [x, y, z]));
        out.intensity.extend(pd.intensity());
        out.class.extend(pd.classification());
        out.returns.extend(pd.return_number().zip(pd.number_of_returns()).map(|(r, n)| (r & 15) | ((n & 15) << 4)));
        if let Some(col) = &mut out.rgb {
            match pd.rgb() {
                Some(it) => col.extend(it.map(|(r, g, b)| [r, g, b])),
                None => col.resize(before + n, [0; 3]),
            }
        }
        if let Some(col) = &mut out.gps_time {
            match pd.gps_time() {
                Some(it) => col.extend(it),
                None => col.resize(before + n, 0.0),
            }
        }
        debug_assert!(out.is_consistent());
        Ok(out.len() - before)
    }
}

// ---------------------------------------------------------------- E57

/// E57 files are read on a helper thread that owns the reader and sends chunks, because the
/// `e57` point iterator borrows its reader.
struct E57Source {
    header: HeaderInfo,
    rx: Receiver<Result<PointBuffer>>,
    pending: PointBuffer,
    pos: usize,
    finished: bool,
}

const E57_THREAD_CHUNK: usize = 65_536;

impl E57Source {
    fn open(path: &Path, opts: &SourceOptions) -> Result<E57Source> {
        let header = e57_header_info(path, opts)?;
        let (tx, rx) = sync_channel::<Result<PointBuffer>>(2);
        let path = path.to_path_buf();
        let (with_rgb, apply_pose) = (header.has_rgb, opts.e57_apply_pose);
        std::thread::spawn(move || {
            let run = || -> Result<()> {
                let mut reader = e57::E57Reader::from_file(&path)?;
                for pc in reader.pointclouds() {
                    let mut it = reader.pointcloud_simple(&pc)?;
                    it.apply_pose(apply_pose);
                    let mut buf = PointBuffer::with_capacity(E57_THREAD_CHUNK, with_rgb, false);
                    for p in it {
                        let p = p?;
                        let e57::CartesianCoordinate::Valid { x, y, z } = p.cartesian else { continue };
                        let rec = PointRecord {
                            xyz: [x, y, z],
                            // The e57 crate normalises to 0..1 with the file's limits (or the type range).
                            intensity: p.intensity.map_or(0, |v| (v.clamp(0.0, 1.0) * 65535.0).round() as u16),
                            class: 0,
                            return_number: 0,
                            number_of_returns: 0,
                            rgb: p.color.map(|c| [(c.red.clamp(0.0, 1.0) * 65535.0).round() as u16, (c.green.clamp(0.0, 1.0) * 65535.0).round() as u16, (c.blue.clamp(0.0, 1.0) * 65535.0).round() as u16]),
                            gps_time: None,
                        };
                        buf.push(&rec);
                        if buf.len() >= E57_THREAD_CHUNK {
                            let full = std::mem::replace(&mut buf, PointBuffer::with_capacity(E57_THREAD_CHUNK, with_rgb, false));
                            if tx.send(Ok(full)).is_err() {
                                return Ok(());
                            }
                        }
                    }
                    if !buf.is_empty() && tx.send(Ok(buf)).is_err() {
                        return Ok(());
                    }
                }
                Ok(())
            };
            if let Err(e) = run() {
                let _ = tx.send(Err(e));
            }
        });
        let pending = PointBuffer::new(header.has_rgb, false);
        Ok(E57Source { header, rx, pending, pos: 0, finished: false })
    }
}

impl PointSource for E57Source {
    fn header(&self) -> &HeaderInfo {
        &self.header
    }

    fn read_chunk(&mut self, max: usize, out: &mut PointBuffer) -> Result<usize> {
        let mut added = 0;
        while added < max {
            if self.pos >= self.pending.len() {
                if self.finished {
                    break;
                }
                match self.rx.recv() {
                    Ok(Ok(b)) => {
                        self.pending = b;
                        self.pos = 0;
                    }
                    Ok(Err(e)) => {
                        self.finished = true;
                        return Err(e);
                    }
                    Err(_) => {
                        self.finished = true;
                        break;
                    }
                }
            }
            let take = (self.pending.len() - self.pos).min(max - added);
            for i in self.pos..self.pos + take {
                out.push(&self.pending.get(i));
            }
            self.pos += take;
            added += take;
        }
        Ok(added)
    }
}

// ---------------------------------------------------------------- text

struct TextSource {
    header: HeaderInfo,
    fmt: TextFormat,
    delimiter: Delimiter,
    lines: BufReader<File>,
    line_no: usize,
    skipped: usize,
    scratch: String,
}

impl TextSource {
    fn open(path: &Path, opts: &SourceOptions) -> Result<TextSource> {
        let fmt = match &opts.text {
            Some(f) => f.clone(),
            None => detect_text_format(path, 5)?.format,
        };
        fmt.validate()?;
        let delimiter = resolve_delimiter(path, &fmt)?;
        // One pass for the facts a header would give.
        let mut scan = TextSource { header: placeholder_header(&fmt), fmt: fmt.clone(), delimiter, lines: BufReader::with_capacity(1 << 20, File::open(path)?), line_no: 0, skipped: 0, scratch: String::new() };
        let mut count = 0u64;
        let mut b = Bounds3::EMPTY;
        let mut buf = PointBuffer::new(false, false);
        loop {
            buf.clear();
            let n = scan.read_chunk(100_000, &mut buf)?;
            if n == 0 {
                break;
            }
            count += n as u64;
            for p in &buf.xyz {
                b.include(*p);
            }
        }
        let mut header = placeholder_header(&fmt);
        header.point_count = count;
        header.bounds = (count > 0).then_some(b);
        header.bounds_exact = true;
        Ok(TextSource { header, fmt, delimiter, lines: BufReader::with_capacity(1 << 20, File::open(path)?), line_no: 0, skipped: 0, scratch: String::new() })
    }
}

fn placeholder_header(fmt: &TextFormat) -> HeaderInfo {
    HeaderInfo {
        kind: SourceKind::Text,
        version: String::new(),
        point_format: None,
        point_count: 0,
        bounds: None,
        bounds_exact: true,
        scale: None,
        offset: None,
        has_rgb: fmt.has_role(TextRole::Red) && fmt.has_role(TextRole::Green) && fmt.has_role(TextRole::Blue),
        has_gps_time: fmt.has_role(TextRole::GpsTime),
        has_intensity: fmt.has_role(TextRole::Intensity),
        compressed: false,
        crs: CrsInfo::default(),
        class_table: ClassTable::None,
        generating_software: String::new(),
        system_identifier: String::new(),
        from_scan: true,
    }
}

impl PointSource for TextSource {
    fn header(&self) -> &HeaderInfo {
        &self.header
    }

    fn read_chunk(&mut self, max: usize, out: &mut PointBuffer) -> Result<usize> {
        let mut added = 0;
        while added < max {
            self.scratch.clear();
            if self.lines.read_line(&mut self.scratch)? == 0 {
                break;
            }
            self.line_no += 1;
            let line = self.scratch.trim_end_matches(['\n', '\r']);
            let t = line.trim_start();
            if t.is_empty() || t.starts_with('#') || t.starts_with("//") {
                continue;
            }
            if self.skipped < self.fmt.skip_lines {
                self.skipped += 1;
                continue;
            }
            if let Some(rec) = parse_line(line, &self.fmt, self.delimiter, self.line_no)? {
                out.push(&rec);
                added += 1;
            }
        }
        Ok(added)
    }
}

/// Reads a whole file into memory (for small files and tests). Large files: use [`open_source`].
pub fn read_all(path: &Path, opts: &SourceOptions) -> Result<PointBuffer> {
    let mut src = open_source(path, opts)?;
    let mut buf = src.new_buffer(src.header().point_count.min(50_000_000) as usize);
    while src.read_chunk(500_000, &mut buf)? > 0 {}
    Ok(buf)
}
