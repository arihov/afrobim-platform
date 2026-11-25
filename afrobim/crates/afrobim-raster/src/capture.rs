// SPDX-License-Identifier: LicenseRef-AfroBIM-Proprietary
// Copyright (c) 2026 AfroBIM. All rights reserved.
//! Map capture (MAPCAPTURE): a north-up raster in the project's own coordinate system, resampled
//! from slippy-map tiles and written as a PNG with a world file and a `.prj`, so that
//! [`crate::read_georef`] reads it back exactly and the application can attach it as an image
//! underlay.
//!
//! The steps, all pure (no network, no window):
//!
//! 1. [`plan_capture`]: the engineer frames an area in project eastings and northings (metres)
//!    and picks a ground resolution in metres per pixel. The plan gives the raster size, the
//!    affine from pixel corners to project coordinates, the tile zoom and the tiles needed. It
//!    refuses captures that are too large in pixels or that would need more than
//!    [`MAX_CAPTURE_TILES`] tiles (the providers' terms forbid bulk download).
//! 2. [`resample`]: every output pixel centre is moved from the project system to longitude and
//!    latitude, then to fractional tile pixels, and sampled bilinearly from the tile mosaic.
//! 3. [`write_capture`]: the PNG, the `.pgw`, the `.prj` and a short `.attribution.txt` note of the
//!    attribution, each written to a temporary file and renamed into place.
//!
//! Coordinates passed to the `to_lonlat` closures are project easting and northing in metres;
//! the closures return `(longitude, latitude)` in WGS 84 degrees. Note the order: the kernel's
//! [`afrobim_kernel::TransformPlan::apply`] into a geographic system returns latitude as `first`
//! and longitude as `second`, so a closure built on it returns `(p.second, p.first)`.

use std::collections::HashMap;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use afrobim_kernel::{CrsKind, ProjectCrs};

use crate::tiles;
use crate::Affine;

/// Side of a web map tile, pixels.
pub const TILE_PX: u32 = 256;

/// The most tiles one capture may fetch. The providers' terms forbid bulk download, so a capture
/// that needs more is refused with a coarser resolution to try.
pub const MAX_CAPTURE_TILES: usize = 400;

/// Spacing, in output pixels, of the grid on which positions are projected exactly. Between
/// grid nodes the tile pixel position is interpolated bilinearly. For a UTM frame of a few
/// kilometres the interpolation error is far below a hundredth of a tile pixel (the test
/// `the_coarse_grid_is_within_a_tenth_of_a_pixel_of_the_exact_projection` measures it).
pub const GRID_STEP_PX: usize = 16;

/// What the engineer asks for.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct CaptureSpec {
    /// South-west corner, project easting and northing in metres.
    pub min: [f64; 2],
    /// North-east corner, project easting and northing in metres.
    pub max: [f64; 2],
    /// Ground size of one output pixel, metres.
    pub m_per_px: f64,
    /// The most pixels allowed on the longest side (for example 8000).
    pub max_px: u32,
}

/// What will be produced, and what it needs.
#[derive(Debug, Clone, PartialEq)]
pub struct CapturePlan {
    pub width: u32,
    pub height: u32,
    /// From pixel corners to project easting and northing, north-up. The top-left corner is the
    /// frame's north-west corner; the raster may run up to one pixel beyond the frame's east and
    /// south edges, since its size is rounded up to whole pixels.
    pub affine: Affine,
    /// Tile zoom level.
    pub zoom: u8,
    /// The tiles `(x, y)` at `zoom` that the resampling reads, row by row from the north-west.
    pub tiles: Vec<(u32, u32)>,
}

/// A function from project easting and northing (metres) to WGS 84 `(longitude, latitude)`.
pub type ToLonLat<'a> = &'a dyn Fn(f64, f64) -> Result<(f64, f64), String>;

/// Whole pixels needed to cover `extent` at `m` metres per pixel. A hair of tolerance keeps an
/// exact multiple from gaining a pixel through rounding.
fn px_count(extent: f64, m: f64) -> f64 {
    ((extent / m) - 1e-6).ceil().max(1.0)
}

/// `v` rounded up to three significant figures, as text with no trailing noise.
fn round_up_text(v: f64) -> (f64, String) {
    if !(v.is_finite() && v > 0.0) {
        return (v, format!("{v}"));
    }
    let mag = v.log10().floor() as i32;
    let p = 10f64.powi(mag - 2);
    let r = (v / p).ceil() * p;
    let dec = (2 - mag).max(0) as usize;
    let text = format!("{r:.dec$}");
    (text.parse().unwrap_or(r), text)
}

/// The inclusive tile ranges at zoom `z` that hold every texel a bilinear sample at the given
/// points can touch.
fn tile_range(ring: &[(f64, f64)], z: u8) -> (u32, u32, u32, u32) {
    let (mut umin, mut umax, mut vmin, mut vmax) = (f64::MAX, f64::MIN, f64::MAX, f64::MIN);
    for &(lon, lat) in ring {
        let (tx, ty) = tiles::lonlat_to_tile(lon, lat, z);
        let (u, v) = (tx * TILE_PX as f64 - 0.5, ty * TILE_PX as f64 - 0.5);
        umin = umin.min(u);
        umax = umax.max(u);
        vmin = vmin.min(v);
        vmax = vmax.max(v);
    }
    let n = (1u64 << z) as f64;
    let t = |texel: f64| ((texel / TILE_PX as f64).floor().clamp(0.0, n - 1.0)) as u32;
    (t(umin.floor()), t(umax.ceil()), t(vmin.floor()), t(vmax.ceil()))
}

fn range_count(r: (u32, u32, u32, u32)) -> usize {
    (r.1 - r.0 + 1) as usize * (r.3 - r.2 + 1) as usize
}

/// Plans a capture. Refuses an empty, inverted or non-numeric frame, a resolution that is not
/// above zero, a raster longer than `spec.max_px` on its longest side, a frame the projection
/// cannot place on Earth, and a capture needing more than [`MAX_CAPTURE_TILES`] tiles. Each
/// refusal says what to change.
///
/// The zoom is [`tiles::zoom_for`] at the frame centre's latitude, capped at `provider_max_zoom`.
/// The tiles are found by projecting a dense ring of pixel centres around the frame (not only
/// the four corners), since a straight project edge is curved in Web Mercator.
pub fn plan_capture(spec: &CaptureSpec, to_lonlat: ToLonLat, provider_max_zoom: u8) -> Result<CapturePlan, String> {
    let [x0, y0] = spec.min;
    let [x1, y1] = spec.max;
    let m = spec.m_per_px;
    if ![x0, y0, x1, y1, m].iter().all(|v| v.is_finite()) {
        return Err("The capture frame or the resolution is not a number.".into());
    }
    if m <= 0.0 {
        return Err(format!("The ground resolution must be more than 0 m per pixel (it is {m})."));
    }
    if x1 <= x0 || y1 <= y0 {
        return Err("The capture frame is empty or inverted: pick two opposite corners of an area.".into());
    }
    let (dx, dy) = (x1 - x0, y1 - y0);
    let (wf, hf) = (px_count(dx, m), px_count(dy, m));
    if spec.max_px == 0 || wf.max(hf) > spec.max_px as f64 {
        let need = dx.max(dy) / spec.max_px.max(1) as f64;
        let (_, text) = round_up_text(need);
        return Err(format!(
            "At {m} m per pixel the capture would be {wf} x {hf} pixels, more than the limit of {} on the longest side. Use a coarser resolution of at least {text} m per pixel, or a smaller frame.",
            spec.max_px
        ));
    }
    let (w, h) = (wf as u32, hf as u32);
    let affine = Affine::north_up(x0, y1, m, m);

    let (cx, cy) = affine.apply(w as f64 / 2.0, h as f64 / 2.0);
    let (_, centre_lat) = to_lonlat(cx, cy).map_err(|e| format!("The centre of the frame cannot be placed on Earth: {e}"))?;
    if !centre_lat.is_finite() {
        return Err("The centre of the frame cannot be placed on Earth.".into());
    }
    let zoom = tiles::zoom_for(m, centre_lat, 0, provider_max_zoom);

    // A ring of pixel centres, at most 16 pixels apart and at least 8 per side.
    let side = |n: u32| -> Vec<f64> {
        let k = ((n as usize).div_ceil(GRID_STEP_PX)).clamp(8, 1024);
        (0..=k).map(|i| 0.5 + (n as f64 - 1.0) * i as f64 / k as f64).collect()
    };
    let (cols, rows) = (side(w), side(h));
    let mut ring_px: Vec<(f64, f64)> = Vec::new();
    for &c in &cols {
        ring_px.push((c, 0.5));
        ring_px.push((c, h as f64 - 0.5));
    }
    for &r in &rows {
        ring_px.push((0.5, r));
        ring_px.push((w as f64 - 0.5, r));
    }
    let mut ring = Vec::with_capacity(ring_px.len());
    for (c, r) in ring_px {
        let (e, n) = affine.apply(c, r);
        let ll = to_lonlat(e, n).map_err(|err| format!("Part of the frame (E {e:.1}, N {n:.1}) cannot be placed on Earth: {err}"))?;
        if !(ll.0.is_finite() && ll.1.is_finite()) {
            return Err(format!("Part of the frame (E {e:.1}, N {n:.1}) cannot be placed on Earth."));
        }
        ring.push(ll);
    }

    let range = tile_range(&ring, zoom);
    let count = range_count(range);
    if count > MAX_CAPTURE_TILES {
        let fit = (0..zoom).rev().find(|&z| range_count(tile_range(&ring, z)) <= MAX_CAPTURE_TILES).unwrap_or(0);
        let ground = tiles::M_PER_PX_Z0 * centre_lat.to_radians().cos().max(0.01) / 2f64.powi(fit as i32);
        let (_, text) = round_up_text(ground);
        return Err(format!(
            "This capture needs {count} map tiles at zoom {zoom}, more than the {MAX_CAPTURE_TILES} allowed in one capture (the providers' terms forbid bulk download). Use a coarser resolution of about {text} m per pixel (zoom {fit}), or a smaller frame."
        ));
    }
    let mut tiles_needed = Vec::with_capacity(count);
    for y in range.2..=range.3 {
        for x in range.0..=range.1 {
            tiles_needed.push((x, y));
        }
    }
    Ok(CapturePlan { width: w, height: h, affine, zoom, tiles: tiles_needed })
}

/// Global tile pixel coordinates (tile x times 256 plus the position in the tile) at `zoom` of
/// a project point, exactly.
fn exact_texel(affine: &Affine, zoom: u8, to_lonlat: ToLonLat, col: f64, row: f64) -> Option<(f64, f64)> {
    let (e, n) = affine.apply(col, row);
    let (lon, lat) = to_lonlat(e, n).ok()?;
    if !(lon.is_finite() && lat.is_finite()) {
        return None;
    }
    let (tx, ty) = tiles::lonlat_to_tile(lon, lat, zoom);
    Some((tx * TILE_PX as f64, ty * TILE_PX as f64))
}

/// Tile pixel positions computed exactly at every `step`-th pixel centre (and at the last one)
/// and interpolated bilinearly in between.
struct CoarseGrid {
    cols: Vec<f64>,
    rows: Vec<f64>,
    step: usize,
    nodes: Vec<Option<(f64, f64)>>,
}

impl CoarseGrid {
    fn axis(n: usize, step: usize) -> Vec<f64> {
        let mut v = Vec::new();
        let mut k = 0;
        loop {
            let i = (k * step).min(n - 1);
            v.push(i as f64 + 0.5);
            if i == n - 1 {
                break;
            }
            k += 1;
        }
        if v.len() == 1 {
            v.push(v[0]);
        }
        v
    }

    fn new(plan: &CapturePlan, to_lonlat: ToLonLat, step: usize) -> CoarseGrid {
        let cols = Self::axis(plan.width as usize, step);
        let rows = Self::axis(plan.height as usize, step);
        let mut nodes = Vec::with_capacity(cols.len() * rows.len());
        for &r in &rows {
            for &c in &cols {
                nodes.push(exact_texel(&plan.affine, plan.zoom, to_lonlat, c, r));
            }
        }
        CoarseGrid { cols, rows, step, nodes }
    }

    fn cell(axis: &[f64], step: usize, i: usize) -> (usize, f64) {
        let k = (i / step).min(axis.len() - 2);
        let d = axis[k + 1] - axis[k];
        let t = if d > 0.0 { (i as f64 + 0.5 - axis[k]) / d } else { 0.0 };
        (k, t)
    }

    /// The tile pixel position of the centre of pixel `(col, row)`, `None` when a node of its
    /// cell could not be projected.
    fn at(&self, col: usize, row: usize) -> Option<(f64, f64)> {
        let (i, s) = Self::cell(&self.cols, self.step, col);
        let (j, t) = Self::cell(&self.rows, self.step, row);
        let nc = self.cols.len();
        let p00 = self.nodes[j * nc + i]?;
        let p10 = self.nodes[j * nc + i + 1]?;
        let p01 = self.nodes[(j + 1) * nc + i]?;
        let p11 = self.nodes[(j + 1) * nc + i + 1]?;
        let lerp = |a: f64, b: f64, c: f64, d: f64| (1.0 - t) * ((1.0 - s) * a + s * b) + t * ((1.0 - s) * c + s * d);
        Some((lerp(p00.0, p10.0, p01.0, p11.0), lerp(p00.1, p10.1, p01.1, p11.1)))
    }
}

/// The tiles, looked up once each and sampled across their edges.
struct Mosaic<'a> {
    zoom: u8,
    tile: &'a dyn Fn(u32, u32) -> Option<Arc<Vec<u8>>>,
    cache: HashMap<(u32, u32), Option<Arc<Vec<u8>>>>,
}

impl Mosaic<'_> {
    fn get(&mut self, key: (u32, u32)) -> Option<Arc<Vec<u8>>> {
        let tile = self.tile;
        self.cache
            .entry(key)
            .or_insert_with(|| tile(key.0, key.1).filter(|t| t.len() == (TILE_PX * TILE_PX * 4) as usize))
            .clone()
    }

    /// The texel at global tile pixel indices, `None` when its tile is missing or it is beyond
    /// the top or bottom of the world. Columns wrap round the antimeridian.
    fn texel(&mut self, ix: i64, iy: i64) -> Option<[u8; 4]> {
        let n = (TILE_PX as i64) << self.zoom;
        if iy < 0 || iy >= n {
            return None;
        }
        let ix = ix.rem_euclid(n);
        let t = TILE_PX as i64;
        let data = self.get(((ix / t) as u32, (iy / t) as u32))?;
        let o = (((iy % t) * t + (ix % t)) * 4) as usize;
        Some([data[o], data[o + 1], data[o + 2], data[o + 3]])
    }

    /// Bilinear sample at global tile pixel coordinates (texel centres at whole numbers plus a
    /// half). Colours are blended weighted by alpha, so a missing neighbour fades the alpha
    /// without darkening the colour.
    fn sample(&mut self, x: f64, y: f64) -> [u8; 4] {
        let (u, v) = (x - 0.5, y - 0.5);
        let (fu, fv) = (u.floor(), v.floor());
        let (s, t) = (u - fu, v - fv);
        let (ix, iy) = (fu as i64, fv as i64);
        let mut acc = [0.0f64; 3];
        let mut alpha = 0.0f64;
        for (dx, dy, w) in [(0, 0, (1.0 - s) * (1.0 - t)), (1, 0, s * (1.0 - t)), (0, 1, (1.0 - s) * t), (1, 1, s * t)] {
            if w <= 0.0 {
                continue;
            }
            if let Some(p) = self.texel(ix + dx, iy + dy) {
                let a = w * p[3] as f64 / 255.0;
                for k in 0..3 {
                    acc[k] += a * p[k] as f64;
                }
                alpha += a;
            }
        }
        if alpha <= 1e-9 {
            return [0, 0, 0, 0];
        }
        let c = |v: f64| (v / alpha).round().clamp(0.0, 255.0) as u8;
        [c(acc[0]), c(acc[1]), c(acc[2]), (alpha * 255.0).round().clamp(0.0, 255.0) as u8]
    }
}

/// Resamples the tiles into the planned raster: `width * height * 4` bytes of RGBA, row by row
/// from the north-west. Each output pixel centre is moved to longitude and latitude, then to
/// fractional tile pixels, and sampled bilinearly across tile edges. Positions are projected
/// exactly every [`GRID_STEP_PX`] pixels and interpolated between; where a grid node cannot be
/// projected each pixel is projected on its own. Missing tiles, and pixels that cannot be
/// projected, are transparent (alpha 0). `tile(x, y)` gives a tile at `plan.zoom` as
/// 256 x 256 x 4 RGBA bytes, `None` when missing.
pub fn resample(plan: &CapturePlan, to_lonlat: ToLonLat, tile: &dyn Fn(u32, u32) -> Option<Arc<Vec<u8>>>) -> Vec<u8> {
    let (w, h) = (plan.width as usize, plan.height as usize);
    let mut out = vec![0u8; w * h * 4];
    if w == 0 || h == 0 {
        return out;
    }
    let grid = CoarseGrid::new(plan, to_lonlat, GRID_STEP_PX);
    let mut mosaic = Mosaic { zoom: plan.zoom, tile, cache: HashMap::new() };
    for row in 0..h {
        for col in 0..w {
            let p = grid.at(col, row).or_else(|| exact_texel(&plan.affine, plan.zoom, to_lonlat, col as f64 + 0.5, row as f64 + 0.5));
            if let Some((x, y)) = p {
                let o = (row * w + col) * 4;
                out[o..o + 4].copy_from_slice(&mosaic.sample(x, y));
            }
        }
    }
    out
}

/// The six lines of a world file (A, D, B, E, C, F) for a corner-based affine. A world file
/// gives the centre of the top-left pixel, so the origin moves by half a pixel along both pixel
/// axes; [`crate::georef::parse_world_file`] moves it back. Numbers are written in Rust's
/// shortest exact form, so reading gives the same values to the last bit or two.
pub fn world_file_text(affine: &Affine) -> String {
    let a = affine;
    let c = a.c + 0.5 * a.a + 0.5 * a.b;
    let f = a.f + 0.5 * a.d + 0.5 * a.e;
    format!("{}\n{}\n{}\n{}\n{}\n{}\n", a.a, a.d, a.b, a.e, c, f)
}

/// An ESRI-style name: `Arc 1960 / UTM zone 36S` becomes `Arc_1960_UTM_Zone_36S`, `WGS 84`
/// becomes `WGS_1984`.
fn esri_name(s: &str) -> String {
    let s = s.replace("WGS 84", "WGS 1984").replace(" / ", " ").replace("zone", "Zone");
    let mut out = String::new();
    for ch in s.chars() {
        if ch.is_ascii_alphanumeric() || ch == '.' {
            out.push(ch);
        } else if !out.ends_with('_') {
            out.push('_');
        }
    }
    out.trim_matches('_').to_string()
}

fn geogcs(epsg: u32) -> Option<String> {
    let g = afrobim_kernel::geographic_crs(epsg)?;
    let n = esri_name(g.name);
    Some(format!(
        "GEOGCS[\"GCS_{n}\",DATUM[\"D_{n}\",SPHEROID[\"{}\",{},{}]],PRIMEM[\"Greenwich\",0.0],UNIT[\"Degree\",0.0174532925199433]",
        esri_name(g.ellipsoid.name),
        g.ellipsoid.a,
        g.ellipsoid.inverse_flattening
    ))
}

/// The text of a `.prj` for the project system: its WKT as it came when there is one; otherwise
/// a minimal ESRI-style `PROJCS` or `GEOGCS` with ESRI names (`Arc_1960_UTM_Zone_36S`,
/// `GCS_WGS_1984`), Transverse Mercator parameters for the UTM systems, and an EPSG authority on
/// the root. [`crate::georef::crs_from_wkt`] reads the code from the authority; for the
/// projected systems the name alone also resolves to the same code. An error for a local site
/// grid, and for a system known by name only without a code, which have nothing to write.
pub fn prj_text(crs: &ProjectCrs) -> Result<String, String> {
    if let Some(w) = crs.wkt.as_ref().filter(|w| !w.trim().is_empty()) {
        return Ok(w.clone());
    }
    let label = crs.label();
    match crs.kind {
        CrsKind::LocalGrid => Err(format!("The project is on a local site grid ({label}), which has no position on Earth: no .prj can be written.")),
        CrsKind::NamedOnly => match crs.epsg {
            Some(code) => Ok(format!("PROJCS[\"{}\",AUTHORITY[\"EPSG\",\"{code}\"]]", crs.name.replace('"', "'"))),
            None => Err(format!("The project's system ({label}) is known by name only, with no EPSG code: no .prj can be written.")),
        },
        CrsKind::Geographic => {
            let code = crs.epsg.ok_or_else(|| format!("The geographic system {label} has no EPSG code."))?;
            let g = geogcs(code).ok_or_else(|| format!("The kernel does not know the geographic system {label}."))?;
            Ok(format!("{g},AUTHORITY[\"EPSG\",\"{code}\"]]"))
        }
        CrsKind::Projected => {
            let code = crs.epsg.ok_or_else(|| format!("The projected system {label} has no EPSG code."))?;
            let geog = crs.geographic_epsg().and_then(geogcs).ok_or_else(|| format!("The kernel does not know the datum of {label}."))?;
            let mut s = format!("PROJCS[\"{}\",{geog}]", esri_name(&crs.name));
            if let (Some((_, south)), Some(p)) = (crs.utm_zone(), crs.projection()) {
                s.push_str(&format!(
                    ",PROJECTION[\"Transverse_Mercator\"],PARAMETER[\"False_Easting\",500000.0],PARAMETER[\"False_Northing\",{}],PARAMETER[\"Central_Meridian\",{:.1}],PARAMETER[\"Scale_Factor\",0.9996],PARAMETER[\"Latitude_Of_Origin\",0.0]",
                    if south { "10000000.0" } else { "0.0" },
                    p.central_meridian_deg()
                ));
            }
            s.push_str(&format!(",UNIT[\"Meter\",1.0],AUTHORITY[\"EPSG\",\"{code}\"]]"));
            Ok(s)
        }
    }
}

/// A temporary file beside `path`, written and flushed, ready to be renamed into place.
fn write_temp(path: &Path, write: impl FnOnce(&mut dyn Write) -> Result<(), String>) -> Result<PathBuf, String> {
    let name = path.file_name().map(|n| n.to_string_lossy().to_string()).unwrap_or_default();
    let tmp = path.with_file_name(format!(".{name}.capture-tmp"));
    let result = (|| {
        let f = std::fs::File::create(&tmp).map_err(|e| format!("{}: {e}", tmp.display()))?;
        let mut bw = std::io::BufWriter::new(f);
        write(&mut bw)?;
        let f = bw.into_inner().map_err(|e| format!("{}: {e}", tmp.display()))?;
        f.sync_all().map_err(|e| format!("{}: {e}", tmp.display()))
    })();
    match result {
        Ok(()) => Ok(tmp),
        Err(e) => {
            let _ = std::fs::remove_file(&tmp);
            Err(e)
        }
    }
}

/// Writes the capture: the PNG at `png_path`, the world file (`.pgw`) and the `.prj` beside it,
/// and an `.attribution.txt` note of the attribution, the tile zoom and the size (no date, so the output is
/// the same from run to run). Every file goes to a temporary file first and is renamed into
/// place, the PNG last, so a failure never leaves a PNG without its placement.
pub fn write_capture(png_path: &Path, plan: &CapturePlan, rgba: &[u8], crs: &ProjectCrs, attribution: &str) -> Result<(), String> {
    use image::ImageEncoder;
    let ext = png_path.extension().map(|e| e.to_string_lossy().to_ascii_lowercase()).unwrap_or_default();
    if ext != "png" {
        return Err("The capture is written as a PNG: give the file name a .png extension.".into());
    }
    let (w, h) = (plan.width, plan.height);
    if w == 0 || h == 0 {
        return Err("The capture has no pixels.".into());
    }
    let need = w as usize * h as usize * 4;
    if rgba.len() != need {
        return Err(format!("The capture's pixels are {} bytes; {w} x {h} RGBA needs {need}.", rgba.len()));
    }
    let prj = prj_text(crs)?;
    let pgw = world_file_text(&plan.affine);
    let name = png_path.file_name().map(|n| n.to_string_lossy().to_string()).unwrap_or_default();
    let (sx, _) = plan.affine.pixel_size();
    let note = format!(
        "AfroBIM map capture\nImage: {name} ({w} x {h} pixels, {sx} m per pixel)\nCoordinate system: {}\nSource: web map tiles at zoom {}, {} tiles\nAttribution: {attribution}\nThe imagery remains subject to the provider's terms of use. Keep this attribution with the image and with any drawing that shows it.\n",
        crs.label(),
        plan.zoom,
        plan.tiles.len()
    );

    let targets = [png_path.with_extension("prj"), png_path.with_extension("pgw"), png_path.with_extension("attribution.txt"), png_path.to_path_buf()];
    let mut temps: Vec<PathBuf> = Vec::new();
    let cleanup = |temps: &[PathBuf]| {
        for t in temps {
            let _ = std::fs::remove_file(t);
        }
    };
    let texts = [prj.as_bytes(), pgw.as_bytes(), note.as_bytes()];
    for (target, bytes) in targets.iter().zip(texts) {
        match write_temp(target, |out| out.write_all(bytes).map_err(|e| format!("{}: {e}", target.display()))) {
            Ok(t) => temps.push(t),
            Err(e) => {
                cleanup(&temps);
                return Err(e);
            }
        }
    }
    let png = write_temp(png_path, |out| {
        image::codecs::png::PngEncoder::new(out)
            .write_image(rgba, w, h, image::ExtendedColorType::Rgba8)
            .map_err(|e| format!("{}: {e}", png_path.display()))
    });
    match png {
        Ok(t) => temps.push(t),
        Err(e) => {
            cleanup(&temps);
            return Err(e);
        }
    }
    for (i, (tmp, target)) in temps.iter().zip(targets.iter()).enumerate() {
        if let Err(e) = std::fs::rename(tmp, target) {
            cleanup(&temps[i..]);
            return Err(format!("{}: {e}", target.display()));
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use afrobim_kernel::plan_transform;

    const R: f64 = 6_378_137.0;

    /// A project in spherical Web Mercator metres, so that tiles are exact rectangles.
    fn mercator(e: f64, n: f64) -> Result<(f64, f64), String> {
        Ok(((e / R).to_degrees(), (n / R).sinh().atan().to_degrees()))
    }

    /// Mercator metres of a fractional tile corner at zoom `z`.
    fn merc_of_tile(tx: f64, ty: f64, z: u8) -> (f64, f64) {
        let n = 2f64.powi(z as i32);
        let tau = 2.0 * std::f64::consts::PI;
        ((tx / n * tau - std::f64::consts::PI) * R, (std::f64::consts::PI - ty / n * tau) * R)
    }

    /// Project to WGS 84, as basemap.rs does it: the plan returns latitude first.
    fn to_lonlat_for(epsg: u32) -> impl Fn(f64, f64) -> Result<(f64, f64), String> {
        let proj = ProjectCrs::from_epsg(epsg).unwrap();
        let wgs = ProjectCrs::geographic(4326).unwrap();
        let plan = plan_transform(&proj, &wgs, None).unwrap().into_iter().next().unwrap();
        move |e, n| plan.apply(e, n, 0.0).map(|p| (p.second, p.first)).map_err(|e| e.to_string())
    }

    fn project_of(epsg: u32, lon: f64, lat: f64) -> (f64, f64) {
        let proj = ProjectCrs::from_epsg(epsg).unwrap();
        let wgs = ProjectCrs::geographic(4326).unwrap();
        let plan = plan_transform(&wgs, &proj, None).unwrap().into_iter().next().unwrap();
        let p = plan.apply(lat, lon, 0.0).unwrap();
        (p.first, p.second)
    }

    fn temp_dir(tag: &str) -> PathBuf {
        let d = std::env::temp_dir().join(format!("afrobim-capture-{tag}-{}", std::process::id()));
        std::fs::create_dir_all(&d).unwrap();
        d
    }

    /// The number between `after` and " m per pixel" in a message.
    fn suggested(msg: &str, after: &str) -> f64 {
        let s = &msg[msg.find(after).unwrap() + after.len()..];
        s[..s.find(" m per pixel").unwrap()].parse().unwrap()
    }

    #[test]
    fn a_capture_near_ntungamo_reads_back_exactly() {
        let (e, n) = project_of(21036, 30.26, -0.88);
        assert!((150_000.0..250_000.0).contains(&e) && (9_850_000.0..9_950_000.0).contains(&n), "{e} {n}");
        let f = to_lonlat_for(21036);
        let spec = CaptureSpec { min: [e - 600.0, n - 400.0], max: [e + 600.0, n + 400.0], m_per_px: 2.5, max_px: 8000 };
        let plan = plan_capture(&spec, &f, 19).unwrap();
        assert_eq!((plan.width, plan.height), (480, 320));
        assert!(plan.tiles.len() <= MAX_CAPTURE_TILES);
        let rgba = resample(&plan, &f, &|_, _| Some(Arc::new(vec![90u8; 256 * 256 * 4])));
        let crs = ProjectCrs::from_epsg(21036).unwrap();
        let dir = temp_dir("ntungamo");
        let png = dir.join("capture.png");
        write_capture(&png, &plan, &rgba, &crs, "Test tiles").unwrap();
        let g = crate::read_georef(&png).unwrap().unwrap();
        for (a, b) in g.affine.to_array().iter().zip(plan.affine.to_array()) {
            assert!((a - b).abs() < 1e-6, "{:?} vs {:?}", g.affine, plan.affine);
        }
        let c = g.crs.unwrap();
        assert_eq!((c.epsg, c.kind), (Some(21036), CrsKind::Projected));
        let note = std::fs::read_to_string(dir.join("capture.attribution.txt")).unwrap();
        assert!(note.contains("Test tiles") && note.contains(&format!("zoom {}", plan.zoom)));
        let img = image::open(&png).unwrap().to_rgba8();
        assert_eq!(img.dimensions(), (480, 320));
        // No temporary file is left behind.
        assert!(std::fs::read_dir(&dir).unwrap().all(|e| !e.unwrap().file_name().to_string_lossy().contains("capture-tmp")));
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn the_world_file_text_is_the_inverse_of_the_reader() {
        let a = Affine::north_up(195_123.25, 9_902_700.75, 0.3, 0.3);
        let back = crate::georef::parse_world_file(&world_file_text(&a)).unwrap();
        for (x, y) in back.to_array().iter().zip(a.to_array()) {
            assert!((x - y).abs() < 1e-8);
        }
        let text = world_file_text(&a);
        let first: Vec<&str> = text.lines().collect();
        assert_eq!(first[4].parse::<f64>().unwrap(), 195_123.4);
    }

    #[test]
    fn prj_text_round_trips_to_the_same_code() {
        for code in [21036, 21096, 32636, 32635, 32736, 32735] {
            let crs = ProjectCrs::from_epsg(code).unwrap();
            let text = prj_text(&crs).unwrap();
            assert!(!text.contains('\u{2014}'));
            assert_eq!(crate::georef::crs_from_wkt(&text).unwrap().epsg, Some(code), "{text}");
            // The ESRI name alone resolves too.
            let bare = text.replace(&format!(",AUTHORITY[\"EPSG\",\"{code}\"]"), "");
            assert!(!bare.contains("AUTHORITY"));
            let c = crate::georef::crs_from_wkt(&bare).unwrap();
            assert_eq!((c.epsg, c.kind), (Some(code), CrsKind::Projected), "{bare}");
        }
        let geo = ProjectCrs::geographic(4326).unwrap();
        let text = prj_text(&geo).unwrap();
        assert!(text.starts_with("GEOGCS[\"GCS_WGS_1984\""));
        let c = crate::georef::crs_from_wkt(&text).unwrap();
        assert_eq!((c.epsg, c.kind), (Some(4326), CrsKind::Geographic));
        assert!(prj_text(&ProjectCrs::from_epsg(21036).unwrap()).unwrap().starts_with("PROJCS[\"Arc_1960_UTM_Zone_36S\""));
        // WKT that came with the system is written as it came.
        let mut w = ProjectCrs::from_epsg(32636).unwrap();
        w.wkt = Some("PROJCS[\"Mine\"]".into());
        assert_eq!(prj_text(&w).unwrap(), "PROJCS[\"Mine\"]");
        assert!(prj_text(&ProjectCrs::local_grid("Site")).is_err());
        assert!(prj_text(&ProjectCrs::named_only(None, "Mystery")).is_err());
        let named = prj_text(&ProjectCrs::named_only(Some(99999), "EPSG:99999")).unwrap();
        assert_eq!(crate::georef::crs_from_wkt(&named).unwrap().epsg, Some(99999));
    }

    /// R = pixel column in the tile, G = pixel row, B = tile id from x and y, opaque.
    fn coded_tile(x: u32, y: u32) -> Option<Arc<Vec<u8>>> {
        let mut v = vec![0u8; 256 * 256 * 4];
        for py in 0..256 {
            for px in 0..256 {
                let o = (py * 256 + px) * 4;
                v[o..o + 4].copy_from_slice(&[px as u8, py as u8, ((x % 16) * 16 + (y % 16)) as u8, 255]);
            }
        }
        Some(Arc::new(v))
    }

    #[test]
    fn synthetic_tiles_are_sampled_at_the_right_pixel_across_tile_edges() {
        // Zoom 10 just south of the equator, a frame one tile across shifted by half a tile, so
        // it spans four tiles and output pixel centres fall on texel centres.
        let (z, x0, y0) = (10u8, 600u32, 512u32);
        let (e0, n0) = merc_of_tile(x0 as f64 + 0.5, y0 as f64 + 0.5, z);
        let (e1, n1) = merc_of_tile(x0 as f64 + 1.5, y0 as f64 + 1.5, z);
        let m = (e1 - e0) / 256.0;
        let spec = CaptureSpec { min: [e0, n1], max: [e1, n0], m_per_px: m, max_px: 8000 };
        let plan = plan_capture(&spec, &mercator, 19).unwrap();
        assert_eq!((plan.width, plan.height, plan.zoom), (256, 256, z));
        assert_eq!(plan.tiles, vec![(x0, y0), (x0 + 1, y0), (x0, y0 + 1), (x0 + 1, y0 + 1)]);
        let img = resample(&plan, &mercator, &coded_tile);
        let px = |c: usize, r: usize| -> [u8; 4] { img[(r * 256 + c) * 4..(r * 256 + c) * 4 + 4].try_into().unwrap() };
        let id = |x: u32, y: u32| ((x % 16) * 16 + (y % 16)) as u8;
        let near = |got: [u8; 4], want: [u8; 4]| got.iter().zip(want).all(|(a, b)| (*a as i32 - b as i32).abs() <= 1);
        for (c, r, want) in [
            (0, 0, [128, 128, id(x0, y0), 255]),
            (127, 127, [255, 255, id(x0, y0), 255]),
            (128, 127, [0, 255, id(x0 + 1, y0), 255]),
            (127, 128, [255, 0, id(x0, y0 + 1), 255]),
            (128, 128, [0, 0, id(x0 + 1, y0 + 1), 255]),
            (200, 40, [72, 168, id(x0 + 1, y0), 255]),
            (255, 255, [127, 127, id(x0 + 1, y0 + 1), 255]),
        ] {
            assert!(near(px(c, r), want), "pixel {c},{r}: {:?} vs {want:?}", px(c, r));
        }
    }

    #[test]
    fn bilinear_sampling_reads_the_neighbour_tile() {
        // Left tile red 100, right tile red 200; a sample half way between the last texel of
        // the left and the first of the right is 150 only if the neighbour is looked up.
        let (z, x0, y0) = (10u8, 600u32, 512u32);
        let flat = |x: u32, _y: u32| Some(Arc::new([if x == x0 { 100u8 } else { 200 }, 0, 0, 255].repeat(256 * 256)));
        let (e0, n0) = merc_of_tile(x0 as f64 + 0.5, y0 as f64 + 0.25, z);
        let (e1, n1) = merc_of_tile(x0 as f64 + 1.5, y0 as f64 + 0.75, z);
        let m = (e1 - e0) / 256.0;
        let spec = CaptureSpec { min: [e0 + 0.5 * m, n1], max: [e1 + 0.5 * m, n0], m_per_px: m, max_px: 8000 };
        let plan = plan_capture(&spec, &mercator, 19).unwrap();
        let img = resample(&plan, &mercator, &flat);
        let w = plan.width as usize;
        let at = |c: usize| img[(10 * w + c) * 4];
        assert!((at(127) as i32 - 150).abs() <= 1, "{}", at(127));
        assert_eq!(at(50), 100);
        assert_eq!(at(200), 200);
    }

    #[test]
    fn missing_tiles_are_transparent() {
        let (z, x0, y0) = (10u8, 600u32, 512u32);
        let (e0, n0) = merc_of_tile(x0 as f64, y0 as f64, z);
        let (e1, n1) = merc_of_tile(x0 as f64 + 2.0, y0 as f64 + 1.0, z);
        let spec = CaptureSpec { min: [e0, n1], max: [e1, n0], m_per_px: (e1 - e0) / 512.0, max_px: 8000 };
        let plan = plan_capture(&spec, &mercator, 19).unwrap();
        let img = resample(&plan, &mercator, &|x, y| if x == x0 { coded_tile(x, y) } else { None });
        let w = plan.width as usize;
        let alpha = |c: usize, r: usize| img[(r * w + c) * 4 + 3];
        assert_eq!(alpha(100, 100), 255);
        assert_eq!(alpha(400, 100), 0);
        assert_eq!(&img[(100 * w + 400) * 4..(100 * w + 400) * 4 + 4], &[0, 0, 0, 0]);
        // Nothing at all gives a clear image.
        assert!(resample(&plan, &mercator, &|_, _| None).iter().all(|&b| b == 0));
    }

    #[test]
    fn too_many_tiles_is_refused_with_a_coarser_resolution_that_fits() {
        let f = to_lonlat_for(32636);
        let (e, n) = project_of(32636, 32.5825, 0.3476);
        let spec = CaptureSpec { min: [e - 25_000.0, n - 25_000.0], max: [e + 25_000.0, n + 25_000.0], m_per_px: 0.5, max_px: 1_000_000 };
        let err = plan_capture(&spec, &f, 19).unwrap_err();
        assert!(err.contains("coarser resolution") && err.contains("tiles"), "{err}");
        let m = suggested(&err, "about ");
        let plan = plan_capture(&CaptureSpec { m_per_px: m, ..spec }, &f, 19).unwrap();
        assert!(plan.tiles.len() <= MAX_CAPTURE_TILES);
    }

    #[test]
    fn too_many_pixels_is_refused_with_the_resolution_that_fits() {
        let spec = CaptureSpec { min: [0.0, 0.0], max: [10_000.0, 3_000.0], m_per_px: 0.25, max_px: 8000 };
        let err = plan_capture(&spec, &mercator, 19).unwrap_err();
        assert!(err.contains("40000 x 12000") && err.contains("coarser resolution") && err.contains("8000"), "{err}");
        let m = suggested(&err, "at least ");
        assert_eq!(m, 1.25);
        let plan = plan_capture(&CaptureSpec { m_per_px: m, max_px: 100_000, ..spec }, &mercator, 14);
        assert_eq!(plan.map(|p| (p.width, p.height)), Ok((8000, 2400)));
    }

    #[test]
    fn bad_frames_and_resolutions_are_refused() {
        let ok = CaptureSpec { min: [0.0, 0.0], max: [100.0, 100.0], m_per_px: 1.0, max_px: 8000 };
        assert!(plan_capture(&ok, &mercator, 19).is_ok());
        for bad in [
            CaptureSpec { m_per_px: 0.0, ..ok },
            CaptureSpec { m_per_px: -1.0, ..ok },
            CaptureSpec { m_per_px: f64::NAN, ..ok },
            CaptureSpec { min: [100.0, 0.0], max: [0.0, 100.0], ..ok },
            CaptureSpec { min: [0.0, 0.0], max: [100.0, 0.0], ..ok },
            CaptureSpec { min: [f64::NAN, 0.0], ..ok },
        ] {
            assert!(plan_capture(&bad, &mercator, 19).is_err(), "{bad:?}");
        }
        let fails = |_: f64, _: f64| -> Result<(f64, f64), String> { Err("outside".into()) };
        assert!(plan_capture(&ok, &fails, 19).unwrap_err().contains("outside"));
    }

    #[test]
    fn the_capture_centre_is_where_the_kernel_puts_it_with_no_swap() {
        // Kampala, WGS 84 / UTM 36N.
        let (lon, lat) = (32.5825, 0.3476);
        let (e, n) = project_of(32636, lon, lat);
        let f = to_lonlat_for(32636);
        let spec = CaptureSpec { min: [e - 500.0, n - 500.0], max: [e + 500.0, n + 500.0], m_per_px: 2.0, max_px: 8000 };
        let plan = plan_capture(&spec, &f, 19).unwrap();
        let (ce, cn) = plan.affine.apply(plan.width as f64 / 2.0, plan.height as f64 / 2.0);
        let (clon, clat) = f(ce, cn).unwrap();
        assert!((clon - lon).abs() < 1e-7 && (clat - lat).abs() < 1e-7, "{clon} {clat}");
        // The tile under Kampala is in the plan, and the centre pixel is sampled from it.
        let (tx, ty) = tiles::lonlat_to_tile(lon, lat, plan.zoom);
        let key = (tx.floor() as u32, ty.floor() as u32);
        assert!(plan.tiles.contains(&key));
        let img = resample(&plan, &f, &coded_tile);
        let o = ((plan.height as usize / 2) * plan.width as usize + plan.width as usize / 2) * 4;
        assert_eq!(img[o + 2], ((key.0 % 16) * 16 + (key.1 % 16)) as u8);
        assert_eq!(plan.zoom, tiles::zoom_for(2.0, lat, 0, 19));
    }

    #[test]
    fn the_coarse_grid_is_within_a_tenth_of_a_pixel_of_the_exact_projection() {
        // A 4 km by 3 km frame at 0.5 m per pixel in Arc 1960 / UTM 36S, through the datum step.
        let (e, n) = project_of(21036, 30.26, -0.88);
        let f = to_lonlat_for(21036);
        let spec = CaptureSpec { min: [e - 2000.0, n - 1500.0], max: [e + 2000.0, n + 1500.0], m_per_px: 0.5, max_px: 8000 };
        let affine = Affine::north_up(spec.min[0], spec.max[1], 0.5, 0.5);
        let plan = CapturePlan { width: 8000, height: 6000, affine, zoom: 18, tiles: Vec::new() };
        let grid = CoarseGrid::new(&plan, &f, GRID_STEP_PX);
        let mut worst = 0.0f64;
        for r in (0..6000).step_by(37) {
            for c in (0..8000).step_by(41) {
                let a = grid.at(c, r).unwrap();
                let b = exact_texel(&affine, 18, &f, c as f64 + 0.5, r as f64 + 0.5).unwrap();
                worst = worst.max((a.0 - b.0).hypot(a.1 - b.1));
            }
        }
        assert!(worst < 0.1, "worst {worst} tile pixels");
        assert!(worst < 0.01, "worst {worst} tile pixels");
    }
}
