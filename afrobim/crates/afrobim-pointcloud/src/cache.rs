// SPDX-License-Identifier: LicenseRef-AfroBIM-Proprietary
// Copyright (c) 2026 AfroBIM. All rights reserved.
//! The cache file beside the source, so reopening is fast.
//!
//! `<source file name>.afrocloud` holds the finished index: a JSON block (format version, the
//! SHA-256 of the source, its size, the options that built the index and the header facts) and the
//! raw little-endian columns in index order. On open the source is hashed (SHA-256, streamed);
//! when the hash, the size and the options all match, the columns are read back and no sorting
//! is done. Otherwise the index is rebuilt and the cache rewritten (through a temporary file and a
//! rename, so a crash never leaves half a cache). A cache that cannot be written (read-only
//! folder) is reported, not fatal.
//!
//! Hashing reads the whole source once; its cost is reported by the benchmark in the README.

use crate::buffer::{Bounds3, PointBuffer};
use crate::error::{PointCloudError, Result};
use crate::header::HeaderInfo;
use crate::index::{IndexOptions, Overflow, PointIndex};
use crate::source::{open_source, SourceOptions};
use crate::text::TextFormat;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::fs::File;
use std::io::{BufReader, BufWriter, Read, Write};
use std::path::{Path, PathBuf};

const MAGIC: &[u8; 8] = b"AFROPC01";
const FORMAT_VERSION: u32 = 1;

/// What happened when opening.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CacheStatus {
    /// The cache matched the source and was read.
    Hit,
    /// There was no cache; the index was built and the cache written.
    Built,
    /// A cache existed but did not match (source changed, options changed or file damaged).
    Rebuilt { reason: String },
}

#[derive(Debug)]
pub struct OpenedIndex {
    pub index: PointIndex,
    pub header: HeaderInfo,
    /// Lower-case hex SHA-256 of the source file.
    pub source_hash: String,
    pub cache: CacheStatus,
    pub cache_path: PathBuf,
    /// Set when the cache could not be written.
    pub cache_write_error: Option<String>,
}

#[derive(Serialize, Deserialize, PartialEq)]
struct Key {
    source_hash: String,
    source_size: u64,
    leaf_size: usize,
    max_points: Option<u64>,
    overflow: Overflow,
    text: Option<TextFormat>,
    e57_apply_pose: bool,
}

#[derive(Serialize, Deserialize)]
struct Meta {
    format_version: u32,
    key: Key,
    header: HeaderInfo,
    points: u64,
    has_rgb: bool,
    has_gps: bool,
    bounds: Bounds3,
    extent: f64,
    stride: u32,
    source_points: u64,
}

/// `<source>.afrocloud` in the same folder.
pub fn cache_path_for(source: &Path) -> PathBuf {
    let mut name = source.file_name().map(|n| n.to_os_string()).unwrap_or_default();
    name.push(".afrocloud");
    source.with_file_name(name)
}

/// SHA-256 of a file, lower-case hex, streamed in 1 MiB blocks.
pub fn sha256_file(path: &Path) -> Result<String> {
    let mut f = File::open(path)?;
    let mut h = Sha256::new();
    let mut buf = vec![0u8; 1 << 20];
    loop {
        let n = f.read(&mut buf)?;
        if n == 0 {
            break;
        }
        h.update(&buf[..n]);
    }
    Ok(h.finalize().iter().map(|b| format!("{b:02x}")).collect())
}

/// Opens a point file as an index, using the cache when it matches.
pub fn open_indexed(path: &Path, src: &SourceOptions, idx: &IndexOptions) -> Result<OpenedIndex> {
    let hash = sha256_file(path)?;
    let size = std::fs::metadata(path)?.len();
    let key = Key { source_hash: hash.clone(), source_size: size, leaf_size: idx.leaf_size, max_points: idx.max_points, overflow: idx.overflow, text: src.text.clone(), e57_apply_pose: src.e57_apply_pose };
    let cache_path = cache_path_for(path);
    let mut status = CacheStatus::Built;
    if cache_path.exists() {
        match read_cache(&cache_path, &key) {
            Ok(Some((index, header))) => return Ok(OpenedIndex { index, header, source_hash: hash, cache: CacheStatus::Hit, cache_path, cache_write_error: None }),
            Ok(None) => status = CacheStatus::Rebuilt { reason: "the source or the options changed since the cache was written".into() },
            Err(e) => status = CacheStatus::Rebuilt { reason: format!("the cache could not be read: {e}") },
        }
    }
    let mut source = open_source(path, src)?;
    let header = source.header().clone();
    let index = PointIndex::build_from_source(source.as_mut(), idx)?;
    drop(source);
    let meta = Meta {
        format_version: FORMAT_VERSION,
        key,
        header: header.clone(),
        points: index.len() as u64,
        has_rgb: index.points.rgb.is_some(),
        has_gps: index.points.gps_time.is_some(),
        bounds: index.bounds,
        extent: index.extent,
        stride: index.stride,
        source_points: index.source_points,
    };
    let cache_write_error = write_cache(&cache_path, &meta, &index).err().map(|e| e.to_string());
    Ok(OpenedIndex { index, header, source_hash: hash, cache: status, cache_path, cache_write_error })
}

fn put<T>(w: &mut impl Write, items: &[T], size: usize, enc: impl Fn(&T, &mut Vec<u8>)) -> Result<()> {
    let mut buf: Vec<u8> = Vec::with_capacity((1 << 20) * size.min(8));
    for chunk in items.chunks(1 << 18) {
        buf.clear();
        for it in chunk {
            enc(it, &mut buf);
        }
        w.write_all(&buf)?;
    }
    Ok(())
}

fn write_cache(path: &Path, meta: &Meta, ix: &PointIndex) -> Result<()> {
    let tmp = path.with_extension("afrocloud.tmp");
    {
        let mut w = BufWriter::with_capacity(1 << 20, File::create(&tmp)?);
        let json = serde_json::to_vec(meta)?;
        w.write_all(MAGIC)?;
        w.write_all(&(json.len() as u64).to_le_bytes())?;
        w.write_all(&json)?;
        put(&mut w, &ix.keys, 8, |k, b| b.extend_from_slice(&k.to_le_bytes()))?;
        put(&mut w, &ix.points.xyz, 24, |p, b| {
            for v in p {
                b.extend_from_slice(&v.to_le_bytes());
            }
        })?;
        put(&mut w, &ix.points.intensity, 2, |v, b| b.extend_from_slice(&v.to_le_bytes()))?;
        put(&mut w, &ix.points.class, 1, |v, b| b.push(*v))?;
        put(&mut w, &ix.points.returns, 1, |v, b| b.push(*v))?;
        if let Some(c) = &ix.points.rgb {
            put(&mut w, c, 6, |p, b| {
                for v in p {
                    b.extend_from_slice(&v.to_le_bytes());
                }
            })?;
        }
        if let Some(g) = &ix.points.gps_time {
            put(&mut w, g, 8, |v, b| b.extend_from_slice(&v.to_le_bytes()))?;
        }
        w.flush()?;
    }
    std::fs::rename(&tmp, path)?;
    Ok(())
}

fn get<T>(r: &mut impl Read, n: usize, size: usize, dec: impl Fn(&[u8]) -> T) -> Result<Vec<T>> {
    let mut out = Vec::with_capacity(n);
    let per = 1usize << 18;
    let mut buf = vec![0u8; per * size];
    let mut left = n;
    while left > 0 {
        let take = left.min(per);
        r.read_exact(&mut buf[..take * size])?;
        out.extend(buf[..take * size].chunks_exact(size).map(&dec));
        left -= take;
    }
    Ok(out)
}

/// `Ok(None)` when the cache is valid but belongs to another source or other options.
fn read_cache(path: &Path, key: &Key) -> Result<Option<(PointIndex, HeaderInfo)>> {
    let mut r = BufReader::with_capacity(1 << 20, File::open(path)?);
    let mut magic = [0u8; 8];
    r.read_exact(&mut magic)?;
    if &magic != MAGIC {
        return Err(PointCloudError::Cache("not an afrocloud cache file".into()));
    }
    let mut len = [0u8; 8];
    r.read_exact(&mut len)?;
    let len = u64::from_le_bytes(len);
    if len > 1 << 28 {
        return Err(PointCloudError::Cache("the metadata block is implausibly large".into()));
    }
    let mut json = vec![0u8; len as usize];
    r.read_exact(&mut json)?;
    let meta: Meta = serde_json::from_slice(&json)?;
    if meta.format_version != FORMAT_VERSION {
        return Ok(None);
    }
    if meta.key != *key {
        return Ok(None);
    }
    let n = meta.points as usize;
    let keys = get(&mut r, n, 8, |b| u64::from_le_bytes(b.try_into().unwrap_or([0; 8])))?;
    let f64at = |b: &[u8]| f64::from_le_bytes(b.try_into().unwrap_or([0; 8]));
    let xyz = get(&mut r, n, 24, |b| [f64at(&b[0..8]), f64at(&b[8..16]), f64at(&b[16..24])])?;
    let intensity = get(&mut r, n, 2, |b| u16::from_le_bytes([b[0], b[1]]))?;
    let class = get(&mut r, n, 1, |b| b[0])?;
    let returns = get(&mut r, n, 1, |b| b[0])?;
    let u16at = |b: &[u8]| u16::from_le_bytes([b[0], b[1]]);
    let rgb = if meta.has_rgb { Some(get(&mut r, n, 6, |b| [u16at(&b[0..2]), u16at(&b[2..4]), u16at(&b[4..6])])?) } else { None };
    let gps_time = if meta.has_gps { Some(get(&mut r, n, 8, f64at)?) } else { None };
    // The file must end exactly here.
    let mut extra = [0u8; 1];
    if r.read(&mut extra)? != 0 {
        return Err(PointCloudError::Cache("trailing bytes after the columns".into()));
    }
    let points = PointBuffer { xyz, intensity, class, returns, rgb, gps_time };
    let index = PointIndex { points, keys, bounds: meta.bounds, extent: meta.extent, leaf_size: key.leaf_size, stride: meta.stride, source_points: meta.source_points };
    Ok(Some((index, meta.header)))
}
