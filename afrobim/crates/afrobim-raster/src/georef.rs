// SPDX-License-Identifier: LicenseRef-AfroBIM-Proprietary
// Copyright (c) 2026 AfroBIM. All rights reserved.
//! Where an image sits: the affine from pixels to the image's own coordinate system, and that
//! system.
//!
//! Pixel coordinates here are **corner based**: `(0, 0)` is the top-left corner of the top-left
//! pixel and `(width, height)` the bottom-right corner of the bottom-right pixel. The sources do
//! not all agree, and each is converted on reading:
//!
//! * A world file gives the **centre** of the top-left pixel, so its origin is moved back by half
//!   a pixel along both pixel axes.
//! * A GeoTIFF with `PixelIsArea` (the default, GeoKey 1025 = 1) is corner based already. With
//!   `PixelIsPoint` (1025 = 2) the tie point is a pixel centre and is moved back by half a pixel,
//!   as GDAL does.
//!
//! Row 0 is the top of the image, so the row term of a north-up image is negative.

use std::path::{Path, PathBuf};

use afrobim_kernel::{CrsKind, ProjectCrs};
use serde::{Deserialize, Serialize};

/// `x = a * col + b * row + c`, `y = d * col + e * row + f`, with `(col, row)` corner based.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct Affine {
    pub a: f64,
    pub b: f64,
    pub c: f64,
    pub d: f64,
    pub e: f64,
    pub f: f64,
}

impl Affine {
    /// From the six lines of a world file (A, D, B, E, C, F), which give the centre of the
    /// top-left pixel.
    pub fn from_world_file_terms(a: f64, d: f64, b: f64, e: f64, c: f64, f: f64) -> Affine {
        Affine { a, b, c: c - 0.5 * a - 0.5 * b, d, e, f: f - 0.5 * d - 0.5 * e }
    }

    /// A north-up image whose top-left corner is at `(x0, y0)`, with square pixels of `size`.
    pub fn north_up(x0: f64, y0: f64, size_x: f64, size_y: f64) -> Affine {
        Affine { a: size_x, b: 0.0, c: x0, d: 0.0, e: -size_y, f: y0 }
    }

    pub fn apply(&self, col: f64, row: f64) -> (f64, f64) {
        (self.a * col + self.b * row + self.c, self.d * col + self.e * row + self.f)
    }

    /// The pixel at `(x, y)`, or `None` when the affine cannot be inverted.
    pub fn invert(&self, x: f64, y: f64) -> Option<(f64, f64)> {
        let det = self.a * self.e - self.b * self.d;
        if det.abs() < 1e-300 || !det.is_finite() {
            return None;
        }
        let (dx, dy) = (x - self.c, y - self.f);
        Some(((self.e * dx - self.b * dy) / det, (-self.d * dx + self.a * dy) / det))
    }

    /// Top-left, top-right, bottom-right, bottom-left, in the image's coordinates.
    pub fn corners(&self, width: u32, height: u32) -> [(f64, f64); 4] {
        let (w, h) = (width as f64, height as f64);
        [self.apply(0.0, 0.0), self.apply(w, 0.0), self.apply(w, h), self.apply(0.0, h)]
    }

    /// Ground size of one pixel along the columns and along the rows.
    pub fn pixel_size(&self) -> (f64, f64) {
        (self.a.hypot(self.d), self.b.hypot(self.e))
    }

    /// Turn of the column axis from east, degrees anticlockwise.
    pub fn rotation_deg(&self) -> f64 {
        self.d.atan2(self.a).to_degrees()
    }

    pub fn is_valid(&self) -> bool {
        [self.a, self.b, self.c, self.d, self.e, self.f].iter().all(|v| v.is_finite()) && self.invert(self.c, self.f).is_some()
    }

    pub fn to_array(&self) -> [f64; 6] {
        [self.a, self.b, self.c, self.d, self.e, self.f]
    }

    pub fn from_array(v: [f64; 6]) -> Affine {
        Affine { a: v[0], b: v[1], c: v[2], d: v[3], e: v[4], f: v[5] }
    }
}

/// What was found about an image's placement.
#[derive(Debug, Clone, PartialEq)]
pub struct Georef {
    pub affine: Affine,
    /// Where the affine came from, in words: "world file ACP.pgw", "GeoTIFF tags".
    pub from: String,
    /// The image's coordinate system when the file says: the GeoTIFF keys, or a `.prj` beside it.
    pub crs: Option<ProjectCrs>,
    /// Where the system came from, in words, empty when there is none.
    pub crs_from: String,
    /// Things the engineer should know (a user-defined GeoTIFF system, a unit that is not the metre).
    pub notes: Vec<String>,
}

/// Reads the six numbers of a world file. Blank lines are skipped; commas are accepted as decimal
/// separators only when the file has no dots at all.
pub fn parse_world_file(text: &str) -> Result<Affine, String> {
    let vals: Vec<f64> = text
        .lines()
        .map(str::trim)
        .filter(|l| !l.is_empty())
        .take(6)
        .map(|l| {
            let l = if l.contains('.') { l.to_string() } else { l.replace(',', ".") };
            l.parse::<f64>().map_err(|_| format!("'{l}' is not a number"))
        })
        .collect::<Result<_, _>>()?;
    if vals.len() < 6 {
        return Err(format!("A world file has six lines of numbers; this one has {}.", vals.len()));
    }
    let a = Affine::from_world_file_terms(vals[0], vals[1], vals[2], vals[3], vals[4], vals[5]);
    if !a.is_valid() {
        return Err("The world file's pixel terms describe no area (the pixel size is zero).".into());
    }
    Ok(a)
}

/// The world files that may belong to `image`, in the order they are tried: the short form
/// (`.pgw`, `.jgw`, `.tfw`, `.gfw`, `.bpw`), the long form (`.pngw`, `.jpgw`, `.tifw`) and `.wld`, in
/// lower and upper case.
pub fn world_file_candidates(image: &Path) -> Vec<PathBuf> {
    let ext = image.extension().map(|e| e.to_string_lossy().to_ascii_lowercase()).unwrap_or_default();
    let mut exts: Vec<String> = Vec::new();
    if ext.len() >= 2 {
        let chars: Vec<char> = ext.chars().collect();
        exts.push(format!("{}{}w", chars[0], chars[chars.len() - 1]));
    }
    exts.push(format!("{ext}w"));
    match ext.as_str() {
        "jpeg" => exts.push("jgw".into()),
        "tiff" => exts.push("tfw".into()),
        _ => {}
    }
    exts.push("wld".into());
    let mut out = Vec::new();
    for e in exts {
        for v in [e.clone(), e.to_ascii_uppercase()] {
            let p = image.with_extension(&v);
            if !out.contains(&p) {
                out.push(p);
            }
        }
    }
    out
}

/// A system from WKT text (a `.prj` file). ESRI's WKT carries no EPSG authority, so when the
/// root has none its name is tried against the kernel's registry (`Arc_1960_UTM_Zone_36S` finds
/// EPSG:21036).
pub fn crs_from_wkt(text: &str) -> Option<ProjectCrs> {
    let crs = ProjectCrs::from_wkt(text)?;
    if crs.kind == CrsKind::NamedOnly && crs.epsg.is_none() {
        if let Some(known) = ProjectCrs::parse(&crs.name) {
            return Some(ProjectCrs { wkt: crs.wkt.clone(), ..known });
        }
        // ESRI writes WGS 84 / UTM as "WGS_1984_UTM_Zone_36S".
        if let Some(known) = ProjectCrs::parse(&crs.name.replace("1984", "84")) {
            return Some(ProjectCrs { wkt: crs.wkt.clone(), ..known });
        }
    }
    Some(crs)
}

/// A system from an EPSG code: projected or geographic when the kernel knows it, named only
/// otherwise.
pub fn crs_from_epsg(code: u32) -> ProjectCrs {
    ProjectCrs::from_epsg(code).or_else(|| ProjectCrs::geographic(code)).unwrap_or_else(|| ProjectCrs::named_only(Some(code), &format!("EPSG:{code}")))
}

/// Reads the placement of `image`: the GeoTIFF tags when it is a TIFF that has them, otherwise a
/// world file beside it. The system comes from the GeoTIFF keys, otherwise from a `.prj` beside
/// the image. `Ok(None)` when the file carries no placement at all (the caller places it by hand).
pub fn read_georef(image: &Path) -> Result<Option<Georef>, String> {
    let ext = image.extension().map(|e| e.to_string_lossy().to_ascii_lowercase()).unwrap_or_default();
    let mut found: Option<Georef> = None;
    if ext == "tif" || ext == "tiff" {
        found = geotiff_georef(image)?;
    }
    if found.is_none() {
        for wf in world_file_candidates(image) {
            if wf.is_file() {
                let text = std::fs::read_to_string(&wf).map_err(|e| format!("{}: {e}", wf.display()))?;
                let affine = parse_world_file(&text).map_err(|e| format!("{}: {e}", wf.display()))?;
                let name = wf.file_name().map(|n| n.to_string_lossy().to_string()).unwrap_or_default();
                found = Some(Georef { affine, from: format!("world file {name}"), crs: None, crs_from: String::new(), notes: Vec::new() });
                break;
            }
        }
    }
    let Some(mut g) = found else { return Ok(None) };
    if g.crs.is_none() {
        for prj in [image.with_extension("prj"), image.with_extension("PRJ")] {
            if let Ok(text) = std::fs::read_to_string(&prj) {
                if let Some(crs) = crs_from_wkt(&text) {
                    g.crs = Some(crs);
                    g.crs_from = prj.file_name().map(|n| n.to_string_lossy().to_string()).unwrap_or_default();
                }
                break;
            }
        }
    }
    Ok(Some(g))
}

// ─── GeoTIFF ────────────────────────────────────────────────────────────────

const TAG_PIXEL_SCALE: u16 = 33550;
const TAG_TIEPOINT: u16 = 33922;
const TAG_TRANSFORMATION: u16 = 34264;
const TAG_GEOKEYS: u16 = 34735;

const KEY_MODEL_TYPE: u16 = 1024;
const KEY_RASTER_TYPE: u16 = 1025;
const KEY_GEOGRAPHIC_TYPE: u16 = 2048;
const KEY_PROJECTED_TYPE: u16 = 3072;
const KEY_LINEAR_UNITS: u16 = 3076;
const USER_DEFINED: u16 = 32767;

/// The GeoKeys whose value sits in the directory itself (location 0), by key.
pub fn geokeys(dir: &[u16]) -> Vec<(u16, u16)> {
    if dir.len() < 4 {
        return Vec::new();
    }
    let n = dir[3] as usize;
    (0..n)
        .filter_map(|i| {
            let k = dir.get(4 + i * 4..8 + i * 4)?;
            (k[1] == 0).then_some((k[0], k[3]))
        })
        .collect()
}

/// The affine and system from a GeoTIFF's tags, `Ok(None)` when it has no placement tags.
pub fn geotiff_georef(path: &Path) -> Result<Option<Georef>, String> {
    use tiff::decoder::Decoder;
    use tiff::tags::Tag;
    let f = std::fs::File::open(path).map_err(|e| format!("{}: {e}", path.display()))?;
    let mut dec = Decoder::new(std::io::BufReader::new(f)).map_err(|e| format!("{} is not a TIFF that can be read: {e}", path.display()))?;
    let f64s = |dec: &mut Decoder<_>, tag: u16| -> Option<Vec<f64>> { dec.find_tag(Tag::Unknown(tag)).ok().flatten().and_then(|v| v.into_f64_vec().ok()) };
    let scale = f64s(&mut dec, TAG_PIXEL_SCALE);
    let tie = f64s(&mut dec, TAG_TIEPOINT);
    let matrix = f64s(&mut dec, TAG_TRANSFORMATION);
    let keys: Vec<(u16, u16)> = dec.find_tag(Tag::Unknown(TAG_GEOKEYS)).ok().flatten().and_then(|v| v.into_u16_vec().ok()).map(|d| geokeys(&d)).unwrap_or_default();
    let key = |k: u16| keys.iter().find(|(id, _)| *id == k).map(|(_, v)| *v);
    let mut affine = match (matrix, scale, tie) {
        (Some(m), _, _) if m.len() >= 8 => Affine { a: m[0], b: m[1], c: m[3], d: m[4], e: m[5], f: m[7] },
        (_, Some(s), Some(t)) if s.len() >= 2 && t.len() >= 6 => {
            let (i, j, x, y) = (t[0], t[1], t[3], t[4]);
            Affine { a: s[0], b: 0.0, c: x - i * s[0], d: 0.0, e: -s[1], f: y + j * s[1] }
        }
        _ => return Ok(None),
    };
    let mut notes = Vec::new();
    if key(KEY_RASTER_TYPE) == Some(2) {
        // PixelIsPoint: the tie point is a pixel centre.
        affine.c -= 0.5 * affine.a + 0.5 * affine.b;
        affine.f -= 0.5 * affine.d + 0.5 * affine.e;
        notes.push("The GeoTIFF ties pixel centres (PixelIsPoint); moved by half a pixel to the corner.".into());
    }
    if !affine.is_valid() {
        return Err("The GeoTIFF placement tags describe no area (the pixel size is zero).".into());
    }
    let projected = key(KEY_PROJECTED_TYPE).filter(|&c| c != 0 && c != USER_DEFINED);
    let geographic = key(KEY_GEOGRAPHIC_TYPE).filter(|&c| c != 0 && c != USER_DEFINED);
    let crs = match (key(KEY_MODEL_TYPE), projected, geographic) {
        (_, Some(p), _) => Some(crs_from_epsg(p as u32)),
        (Some(2), _, Some(g)) | (None, None, Some(g)) => Some(crs_from_epsg(g as u32)),
        _ => None,
    };
    if crs.is_none() && (key(KEY_PROJECTED_TYPE) == Some(USER_DEFINED) || key(KEY_GEOGRAPHIC_TYPE) == Some(USER_DEFINED)) {
        notes.push("The GeoTIFF defines its own coordinate system (user defined), which AfroBIM does not read. Choose the system by hand.".into());
    }
    if let Some(u) = key(KEY_LINEAR_UNITS).filter(|&u| u != 9001) {
        notes.push(format!("The GeoTIFF's linear unit is EPSG:{u}, not the metre. Check the placement."));
    }
    let crs_from = if crs.is_some() { "GeoTIFF keys".to_string() } else { String::new() };
    Ok(Some(Georef { affine, from: "GeoTIFF tags".into(), crs, crs_from, notes }))
}

/// How the image's system relates to the project's, as far as placement goes.
#[derive(Debug, Clone, PartialEq)]
pub enum CrsRelation {
    /// The numbers are used as they are (metres to millimetres): the same system, or one side
    /// does not say. The text says which, for the window.
    AsIs(String),
    /// The image is in another system and must be moved through a transformation.
    Transform { from: ProjectCrs, to: ProjectCrs },
    /// The image cannot be placed, with the reason.
    Cannot(String),
}

/// Decides how an image in `image` is placed in a project in `project`.
pub fn resolve_crs(image: Option<&ProjectCrs>, project: Option<&ProjectCrs>) -> CrsRelation {
    match (image, project) {
        (None, None) => CrsRelation::AsIs("Neither the image nor the project states a coordinate system: the image's coordinates are used as they are.".into()),
        (None, Some(p)) => CrsRelation::AsIs(format!("The image states no coordinate system: it is taken to be the project's ({}).", p.label())),
        (Some(i), None) => {
            if i.kind == CrsKind::Geographic {
                CrsRelation::Cannot(format!("The image is in latitude and longitude ({}). Set the project coordinate system (command CRS) so that it can be projected.", i.label()))
            } else {
                CrsRelation::AsIs(format!("The project has no coordinate system: the image is placed at its own coordinates ({}).", i.label()))
            }
        }
        (Some(i), Some(p)) => {
            let same = match (i.epsg, p.epsg) {
                (Some(a), Some(b)) => a == b,
                _ => i.name == p.name,
            };
            if same {
                CrsRelation::AsIs(format!("The image and the project are both in {}.", p.label()))
            } else {
                CrsRelation::Transform { from: i.clone(), to: p.clone() }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_world_file_gives_pixel_centres_and_is_moved_to_the_corner() {
        let a = parse_world_file("0.25\n0\n0\n-0.25\n500000.125\n9900000.875\n").unwrap();
        // Corner of the top-left pixel is half a pixel up and to the left of its centre.
        assert_eq!(a.apply(0.0, 0.0), (500000.0, 9900001.0));
        assert_eq!(a.apply(4.0, 4.0), (500001.0, 9900000.0));
        let (col, row) = a.invert(500001.0, 9900000.0).unwrap();
        assert!((col - 4.0).abs() < 1e-9 && (row - 4.0).abs() < 1e-9);
    }

    #[test]
    fn a_rotated_world_file_from_the_acp_orthophotos_keeps_its_rotation() {
        // ACP_Ortho_2026-07_Park.pgw, as delivered.
        let a = parse_world_file("0.2472693611\n0.0001790648\n0.0001756749\n-0.2430400166\n179018.9241729926\n9909443.3415584043\n").unwrap();
        let (sx, sy) = a.pixel_size();
        assert!((sx - 0.24727).abs() < 1e-4 && (sy - 0.24304).abs() < 1e-4);
        assert!(a.rotation_deg().abs() < 0.05 && a.rotation_deg() != 0.0);
        let (x, y) = a.apply(0.5, 0.5);
        assert!((x - 179018.9241729926).abs() < 1e-6 && (y - 9_909_443.341_558_404).abs() < 1e-6);
    }

    #[test]
    fn a_world_file_that_is_short_or_degenerate_is_refused() {
        assert!(parse_world_file("1\n0\n0\n").is_err());
        assert!(parse_world_file("0\n0\n0\n0\n1\n1\n").is_err());
        assert!(parse_world_file("1\n0\n0\nx\n1\n1\n").is_err());
        // Commas as the decimal mark are read when there is no dot.
        let a = parse_world_file("0,5\n0\n0\n-0,5\n10\n20\n").unwrap();
        assert_eq!(a.pixel_size(), (0.5, 0.5));
    }

    #[test]
    fn world_file_candidates_cover_the_short_and_long_forms() {
        let c = world_file_candidates(Path::new("/x/ortho.png"));
        assert_eq!(c[0], PathBuf::from("/x/ortho.pgw"));
        assert!(c.contains(&PathBuf::from("/x/ortho.pngw")) && c.contains(&PathBuf::from("/x/ortho.wld")) && c.contains(&PathBuf::from("/x/ortho.PGW")));
        let t = world_file_candidates(Path::new("/x/o.tiff"));
        assert!(t.contains(&PathBuf::from("/x/o.tfw")));
        let j = world_file_candidates(Path::new("/x/o.jpeg"));
        assert!(j.contains(&PathBuf::from("/x/o.jgw")));
    }

    #[test]
    fn an_esri_prj_with_no_authority_is_found_by_its_name() {
        let wkt = r#"PROJCS["Arc_1960_UTM_Zone_36S",GEOGCS["GCS_Arc_1960",DATUM["D_Arc_1960",SPHEROID["Clarke_1880_RGS",6378249.145,293.465]],PRIMEM["Greenwich",0.0],UNIT["Degree",0.0174532925199433]],PROJECTION["Transverse_Mercator"],PARAMETER["False_Easting",500000.0],PARAMETER["False_Northing",10000000.0],PARAMETER["Central_Meridian",33.0],PARAMETER["Scale_Factor",0.9996],PARAMETER["Latitude_Of_Origin",0.0],UNIT["Meter",1.0]]"#;
        let c = crs_from_wkt(wkt).unwrap();
        assert_eq!(c.epsg, Some(21036));
        assert_eq!(c.kind, CrsKind::Projected);
        let w = r#"PROJCS["WGS_1984_UTM_Zone_36N",GEOGCS["GCS_WGS_1984",DATUM["D_WGS_1984",SPHEROID["WGS_1984",6378137.0,298.257223563]]]]"#;
        assert_eq!(crs_from_wkt(w).unwrap().epsg, Some(32636));
    }

    #[test]
    fn geokeys_read_the_values_held_in_the_directory() {
        let dir = [1, 1, 0, 3, 1024, 0, 1, 1, 1025, 0, 1, 2, 3072, 0, 1, 32636];
        assert_eq!(geokeys(&dir), vec![(1024, 1), (1025, 2), (3072, 32636)]);
        assert!(geokeys(&[1, 1]).is_empty());
    }

    #[test]
    fn placement_relation_follows_what_each_side_states() {
        let utm = ProjectCrs::from_epsg(32636).unwrap();
        let arc = ProjectCrs::from_epsg(21036).unwrap();
        let geo = ProjectCrs::geographic(4326).unwrap();
        assert!(matches!(resolve_crs(None, None), CrsRelation::AsIs(_)));
        assert!(matches!(resolve_crs(None, Some(&utm)), CrsRelation::AsIs(_)));
        assert!(matches!(resolve_crs(Some(&utm), Some(&utm)), CrsRelation::AsIs(_)));
        assert!(matches!(resolve_crs(Some(&arc), Some(&utm)), CrsRelation::Transform { .. }));
        assert!(matches!(resolve_crs(Some(&geo), None), CrsRelation::Cannot(_)));
    }
}
