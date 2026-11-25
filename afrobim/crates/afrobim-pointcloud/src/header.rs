// SPDX-License-Identifier: LicenseRef-AfroBIM-Proprietary
// Copyright (c) 2026 AfroBIM. All rights reserved.
//! Header facts, read without loading points, and the coordinate system as plain data.
//!
//! No coordinate reference system type is used: the WKT text, the EPSG code(s) found and the raw
//! GeoTIFF keys are returned as data and the caller decides what they mean.

use crate::buffer::Bounds3;
use crate::classes::ClassTable;
use serde::{Deserialize, Serialize};

/// What kind of file the points came from.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SourceKind {
    Las,
    Laz,
    E57,
    Text,
}

/// One GeoTIFF key from the LAS GeoKeyDirectory, with its value.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct GeoKeyEntry {
    pub id: u16,
    pub value: GeoKeyValue,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum GeoKeyValue {
    Short(u16),
    Text(String),
    Doubles(Vec<f64>),
}

/// The coordinate system as found in the file. Nothing is interpreted beyond the codes below.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct CrsInfo {
    /// OGC WKT text (LAS: the WKT coordinate system VLR; E57: the coordinate metadata text when it
    /// is WKT or any other text, returned as is).
    pub wkt: Option<String>,
    /// An EPSG code found: from the WKT root authority, else from GeoTIFF ProjectedCRSGeoKey (3072)
    /// or GeographicTypeGeoKey (2048). Not checked against any registry.
    pub epsg: Option<u32>,
    /// Where `epsg` came from, in words.
    pub epsg_source: Option<String>,
    /// EPSG code of the vertical CRS (GeoTIFF VerticalGeoKey 4096) if present.
    pub vertical_epsg: Option<u32>,
    /// Every GeoTIFF key found, as the file wrote it.
    pub geotiff_keys: Vec<GeoKeyEntry>,
}

impl CrsInfo {
    pub fn is_empty(&self) -> bool {
        self.wkt.is_none() && self.epsg.is_none() && self.geotiff_keys.is_empty()
    }
}

/// Facts from the file header. For LAS and E57 no point is read. For text files there is no header,
/// so these facts come from one pass over the file ([`HeaderInfo::from_scan`] is `true`).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct HeaderInfo {
    pub kind: SourceKind,
    /// "1.2", "1.4" for LAS and LAZ; "ASTM E57 1.0" style text for E57 (as the file states); empty for text.
    pub version: String,
    /// LAS point data record format number; `None` for other kinds.
    pub point_format: Option<u8>,
    pub point_count: u64,
    /// Bounds as the header states them (true coordinates). `None` if the file has none.
    pub bounds: Option<Bounds3>,
    /// `true` when `bounds` are the exact extent of the points, `false` when they are only what
    /// the header states (LAS writers can be wrong) or a conservative box (E57 with a rotating pose).
    pub bounds_exact: bool,
    /// LAS coordinate scale factors (x, y, z), metres per unit.
    pub scale: Option<[f64; 3]>,
    /// LAS coordinate offsets (x, y, z). Coordinates are `raw * scale + offset`.
    pub offset: Option<[f64; 3]>,
    pub has_rgb: bool,
    pub has_gps_time: bool,
    pub has_intensity: bool,
    pub compressed: bool,
    pub crs: CrsInfo,
    /// Which table of the LAS specification gives the class meanings, if any.
    pub class_table: ClassTable,
    pub generating_software: String,
    pub system_identifier: String,
    /// `true` for text files (facts from a scan, not from a header).
    pub from_scan: bool,
}

impl HeaderInfo {
    /// Bytes of memory per point in an in-memory index of this file (see the README).
    pub fn bytes_per_point(&self) -> usize {
        8 + 24 + 4 + if self.has_rgb { 6 } else { 0 } + if self.has_gps_time { 8 } else { 0 }
    }
}

/// An EPSG code from WKT text: the authority of the root node (WKT 1 `AUTHORITY["EPSG","n"]`,
/// WKT 2 `ID["EPSG",n]`), else the authority of the first node one level below (for a compound CRS
/// without its own authority). A text scan, nothing more.
pub fn epsg_from_wkt(wkt: &str) -> Option<u32> {
    let b = wkt.as_bytes();
    let mut depth = 0i32;
    let mut in_quote = false;
    let mut ident_start = 0usize;
    let mut last_depth1: Option<u32> = None;
    let mut first_depth2: Option<u32> = None;
    let mut i = 0usize;
    while i < b.len() {
        let c = b[i];
        if in_quote {
            if c == b'"' {
                in_quote = false;
            }
            i += 1;
            continue;
        }
        match c {
            b'"' => in_quote = true,
            b'[' | b'(' => {
                let ident = wkt[ident_start..i].trim().to_ascii_uppercase();
                if ident == "AUTHORITY" || ident == "ID" {
                    if let Some(end) = wkt[i..].find(']').or_else(|| wkt[i..].find(')')) {
                        let inner = &wkt[i + 1..i + end];
                        let mut parts = inner.split(',').map(|s| s.trim().trim_matches('"'));
                        if parts.next().is_some_and(|a| a.eq_ignore_ascii_case("EPSG")) {
                            if let Some(code) = parts.next().and_then(|s| s.parse::<u32>().ok()) {
                                if depth == 1 {
                                    last_depth1 = Some(code);
                                } else if depth == 2 && first_depth2.is_none() {
                                    first_depth2 = Some(code);
                                }
                            }
                        }
                    }
                }
                depth += 1;
                ident_start = i + 1;
            }
            b']' | b')' => {
                depth -= 1;
                ident_start = i + 1;
            }
            b',' => ident_start = i + 1,
            _ => {}
        }
        i += 1;
    }
    last_depth1.or(first_depth2)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn epsg_from_wkt1_root_not_from_inner_nodes() {
        let wkt = r#"PROJCS["WGS 84 / UTM zone 36N",GEOGCS["WGS 84",DATUM["WGS_1984",SPHEROID["WGS 84",6378137,298.257223563,AUTHORITY["EPSG","7030"]],AUTHORITY["EPSG","6326"]],AUTHORITY["EPSG","4326"]],PROJECTION["Transverse_Mercator"],AUTHORITY["EPSG","32636"]]"#;
        assert_eq!(epsg_from_wkt(wkt), Some(32636));
    }

    #[test]
    fn epsg_from_wkt2_id_and_absent() {
        let wkt = r#"PROJCRS["WGS 84 / UTM zone 36N",BASEGEOGCRS["WGS 84",ID["EPSG",4326]],ID["EPSG",32636]]"#;
        assert_eq!(epsg_from_wkt(wkt), Some(32636));
        assert_eq!(epsg_from_wkt("LOCAL_CS[\"site grid\"]"), None);
        assert_eq!(epsg_from_wkt(""), None);
    }
}
