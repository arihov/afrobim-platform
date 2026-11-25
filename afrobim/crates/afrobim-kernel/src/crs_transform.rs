// SPDX-License-Identifier: LicenseRef-AfroBIM-Proprietary
// Copyright (c) 2026 AfroBIM. All rights reserved.
//! Datum transformations: geodesy, methods and the registry records they are used with.
//!
//! Nothing in this module changes a document. It is the maths and the data; [`crate::crs_plan`]
//! builds the explicit, logged operation from it.
//!
//! # Sources, and what was not taken from memory
//!
//! Every formula is from IOGP Publication 373-7-2, Geomatics Guidance Note number 7, part 2,
//! "Coordinate Conversions and Transformations including Formulas", revised September 2019,
//! <https://www.iogp.org/wp-content/uploads/2019/09/373-07-02.pdf>, fetched 2026-11-16 (162
//! pages; cited below as GN7-2 with its section numbers). The epsg.org page of guidance notes
//! lists a newer version (74, July 2026) but its download sits behind a form that asks for a name
//! and an e-mail address; that was not filled in, so this is the 2019 revision, which the
//! publisher serves openly. The method sections used here (4.1.1, 4.2, 4.3.1) concern formulas that
//! the registry's method descriptions repeat; no change to them was seen, but the 2026 text was
//! not read.
//!
//! Every ellipsoid, transformation parameter, area of use and accuracy below was read from the
//! EPSG registry through its mirror epsg.io on 2026-11-16: the page
//! `https://epsg.io/<transformation code>` for each transformation (parameters, accuracy, area of
//! use text and bounding box, revision date, remarks) and `https://epsg.io/<crs code>` /
//! `.wkt` / `.json` for each coordinate reference system. The fetch tool returned a summary of
//! each page rather than the raw page, so a digit mistaken in summarising is possible; the codes
//! and values are meant to be re-checked against the registry (see the README table).
//! The `accuracy_m` figure of a transformation is the registry's "operation accuracy" as shown;
//! the pages did not always show the unit, and the registry's own remarks state the per-axis
//! accuracies in metres, so the figure is read as metres.
//!
//! No independent geodetic software was used to check any result. The check values are the
//! worked examples of GN7-2 (reproduced in the tests) and round trips.

use crate::crs::{AreaOfUse, Ellipsoid, CLARKE_1880_ARC, CLARKE_1880_RGS, WGS84};
use serde::{Deserialize, Serialize};

/// When the registry pages were read.
pub const REGISTRY_RETRIEVED: &str = "2026-11-16";

/// Arc-seconds to radians.
const ARCSEC: f64 = std::f64::consts::PI / (180.0 * 3600.0);

// ---------------------------------------------------------------------------------------------
// Geographic coordinate reference systems

/// A geographic 2D coordinate reference system the kernel knows: the ends of a datum
/// transformation. Axis order is latitude then longitude, in degrees (EPSG:4326 page: "Axes:
/// latitude, longitude. Orientations: north, east"; the base CRS of EPSG:20136 is described the
/// same way).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct GeographicCrs {
    pub epsg: u32,
    pub name: &'static str,
    pub datum: &'static str,
    pub ellipsoid: Ellipsoid,
    pub area: AreaOfUse,
    pub area_text: &'static str,
    /// Where it was read, retrieved 2026-11-16.
    pub source: &'static str,
}

/// The geographic systems known, in code order.
pub fn geographic_crs_codes() -> [u32; 6] {
    [4148, 4201, 4209, 4210, 4222, 4326]
}

/// A geographic system by EPSG code.
///
/// * 4326 WGS 84: `https://epsg.io/4326.json` and `.wkt`: area "World", box -90 -180 90 180;
///   datum ensemble "World Geodetic System 1984 ensemble", ellipsoid WGS 84, ensemble accuracy 2.0
///   as shown (no unit on the page).
/// * 4210 Arc 1960: `https://epsg.io/4210.json`: Burundi, Kenya, Rwanda, Tanzania and Uganda,
///   box -11.75 28.85 4.63 41.91; ellipsoid Clarke 1880 (RGS).
/// * 4209 Arc 1950: `https://epsg.io/4209.wkt` and the transformation list on `https://epsg.io/4209`:
///   box -26.88 19.99 -8.19 35.93; ellipsoid Clarke 1880 (Arc), 1/f 293.4663077.
/// * 4201 Adindan: `https://epsg.io/4201` and `.wkt`: box 3.4 21.82 22.24 47.99; Clarke 1880 (RGS).
/// * 4222 Cape: `https://epsg.io/4222` and `.wkt`: Botswana; Eswatini (Swaziland); Lesotho; South
///   Africa - mainland, box -34.88 16.45 -17.78 32.95; Clarke 1880 (Arc).
/// * 4148 Hartebeesthoek94: `https://epsg.io/4148`: Eswatini (Swaziland), Lesotho, South Africa,
///   box -50.32 13.33 -22.13 42.85, datum 6148, ellipsoid WGS 84 (a 6378137, 1/f 298.257223563).
///   The page lists no transformation to WGS 84, so none is recorded here.
pub fn geographic_crs(epsg: u32) -> Option<GeographicCrs> {
    let b = |south, west, north, east| AreaOfUse { south, west, north, east };
    Some(match epsg {
        4326 => GeographicCrs {
            epsg,
            name: "WGS 84",
            datum: "World Geodetic System 1984 ensemble",
            ellipsoid: WGS84,
            area: b(-90.0, -180.0, 90.0, 180.0),
            area_text: "World.",
            source: "https://epsg.io/4326.json",
        },
        4210 => GeographicCrs {
            epsg,
            name: "Arc 1960",
            datum: "Arc 1960",
            ellipsoid: CLARKE_1880_RGS,
            area: b(-11.75, 28.85, 4.63, 41.91),
            area_text: "Burundi, Kenya, Rwanda, Tanzania and Uganda",
            source: "https://epsg.io/4210.json",
        },
        4209 => GeographicCrs {
            epsg,
            name: "Arc 1950",
            datum: "Arc 1950",
            ellipsoid: CLARKE_1880_ARC,
            area: b(-26.88, 19.99, -8.19, 35.93),
            area_text: "(text not fetched; box from the page)",
            source: "https://epsg.io/4209",
        },
        4201 => GeographicCrs {
            epsg,
            name: "Adindan",
            datum: "Adindan",
            ellipsoid: CLARKE_1880_RGS,
            area: b(3.4, 21.82, 22.24, 47.99),
            area_text: "(text not fetched; box from the page)",
            source: "https://epsg.io/4201",
        },
        4222 => GeographicCrs {
            epsg,
            name: "Cape",
            datum: "Cape",
            ellipsoid: CLARKE_1880_ARC,
            area: b(-34.88, 16.45, -17.78, 32.95),
            area_text: "Botswana; Eswatini (Swaziland); Lesotho; South Africa - mainland",
            source: "https://epsg.io/4222",
        },
        4148 => GeographicCrs {
            epsg,
            name: "Hartebeesthoek94",
            datum: "Hartebeesthoek94",
            ellipsoid: WGS84,
            area: b(-50.32, 13.33, -22.13, 42.85),
            area_text: "Eswatini (Swaziland), Lesotho, and South Africa, onshore and offshore",
            source: "https://epsg.io/4148",
        },
        _ => return None,
    })
}

// ---------------------------------------------------------------------------------------------
// Geographic and geocentric (GN7-2 section 4.1.1, EPSG method 9602)

/// Geographic latitude, longitude (degrees) and ellipsoidal height (metres) to geocentric X, Y, Z
/// (metres). GN7-2 4.1.1: `X = (nu + h) cos(phi) cos(lambda)`, `Y = (nu + h) cos(phi) sin(lambda)`,
/// `Z = [(1 - e^2) nu + h] sin(phi)`, `nu = a / sqrt(1 - e^2 sin^2 phi)`. The longitude is taken
/// from Greenwich, as every system here is.
pub fn geographic_to_geocentric(ell: &Ellipsoid, lat_deg: f64, lon_deg: f64, h: f64) -> [f64; 3] {
    let f = 1.0 / ell.inverse_flattening;
    let e2 = f * (2.0 - f);
    let (phi, lam) = (lat_deg.to_radians(), lon_deg.to_radians());
    let nu = ell.a / (1.0 - e2 * phi.sin().powi(2)).sqrt();
    [(nu + h) * phi.cos() * lam.cos(), (nu + h) * phi.cos() * lam.sin(), ((1.0 - e2) * nu + h) * phi.sin()]
}

/// Geocentric X, Y, Z (metres) to latitude, longitude (degrees) and ellipsoidal height (metres).
///
/// GN7-2 4.1.1: the closed form `phi = atan2(Z + eps b sin^3 q, p - e^2 a cos^3 q)` with
/// `q = atan2(Z a, p b)`, `p = sqrt(X^2 + Y^2)`, `eps = e^2/(1 - e^2)`, as the first estimate; then
/// the note's iteration `phi = atan2(Z + e^2 nu sin phi, p)` to convergence. The height is
/// `h = p / cos(phi) - nu` away from the poles (the note's form) and `h = Z / sin(phi) - (1 - e^2)
/// nu`, which follows from the Z equation of the forward conversion, near them.
pub fn geocentric_to_geographic(ell: &Ellipsoid, v: [f64; 3]) -> (f64, f64, f64) {
    let [x, y, z] = v;
    let f = 1.0 / ell.inverse_flattening;
    let e2 = f * (2.0 - f);
    let a = ell.a;
    let b = a * (1.0 - f);
    let p = x.hypot(y);
    if p < 1e-9 {
        let lat = if z >= 0.0 { 90.0 } else { -90.0 };
        return (lat, 0.0, z.abs() - b);
    }
    let eps = e2 / (1.0 - e2);
    let q = (z * a).atan2(p * b);
    let mut phi = (z + eps * b * q.sin().powi(3)).atan2(p - e2 * a * q.cos().powi(3));
    for _ in 0..8 {
        let nu = a / (1.0 - e2 * phi.sin().powi(2)).sqrt();
        let next = (z + e2 * nu * phi.sin()).atan2(p);
        let done = (next - phi).abs() < 1e-15;
        phi = next;
        if done {
            break;
        }
    }
    let nu = a / (1.0 - e2 * phi.sin().powi(2)).sqrt();
    let h = if phi.abs() < std::f64::consts::FRAC_PI_4 { p / phi.cos() - nu } else { z / phi.sin() - (1.0 - e2) * nu };
    (phi.to_degrees(), y.atan2(x).to_degrees(), h)
}

// ---------------------------------------------------------------------------------------------
// Geocentric methods (GN7-2 section 4.2)

/// Which way the rotations of a Helmert transformation are signed (GN7-2 4.2.3).
///
/// Position Vector (EPSG method 1033): `[XT YT ZT] = M [[1 -rZ +rY] [+rZ 1 -rX] [-rY +rX 1]]
/// [XS YS ZS] + T`. Coordinate Frame rotation (method 1032): the same with the rotation signs
/// reversed. The same transformation is written with opposite signs of rX, rY, rZ in the two
/// conventions: the GN7-2 example (WGS 72 to WGS 84) has rZ = +0.554 arc-seconds in Position
/// Vector and -0.554 in Coordinate Frame, and both give the same coordinates. A parameter set must
/// be paired with the convention it was published in.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum RotationConvention {
    PositionVector,
    CoordinateFrame,
}

impl RotationConvention {
    fn sign(self) -> f64 {
        match self {
            RotationConvention::PositionVector => 1.0,
            RotationConvention::CoordinateFrame => -1.0,
        }
    }
}

/// A 7-parameter Helmert transformation: translations in metres, rotations in arc-seconds, scale
/// difference in parts per million, `M = 1 + dS`.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct Helmert {
    pub tx: f64,
    pub ty: f64,
    pub tz: f64,
    pub rx_arcsec: f64,
    pub ry_arcsec: f64,
    pub rz_arcsec: f64,
    pub scale_ppm: f64,
    pub convention: RotationConvention,
}

impl Helmert {
    /// The transformation applied to a geocentric position (metres).
    pub fn apply(&self, v: [f64; 3]) -> [f64; 3] {
        molodensky_badekas(v, [0.0; 3], [self.tx, self.ty, self.tz], [self.rx_arcsec, self.ry_arcsec, self.rz_arcsec], self.scale_ppm, self.convention)
    }

    /// The parameter set for the reverse direction as the EPSG dataset documents it: the same
    /// formula with the signs of the translations, rotations and scale difference changed (GN7-2
    /// 4.2.3, "Reversibility"). It is an approximation of the exact inverse; the note puts the
    /// misclosure of a round trip at no more than 2 cm in practical circumstances.
    pub fn reversed(&self) -> Helmert {
        Helmert { tx: -self.tx, ty: -self.ty, tz: -self.tz, rx_arcsec: -self.rx_arcsec, ry_arcsec: -self.ry_arcsec, rz_arcsec: -self.rz_arcsec, scale_ppm: -self.scale_ppm, convention: self.convention }
    }
}

/// A 10-parameter Molodensky-Badekas transformation (GN7-2 4.2.2, variant A, the one the EPSG
/// dataset documents): `VT = M R (VS - VP) + VP + T`.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct MolodenskyBadekasParams {
    pub helmert: Helmert,
    /// The pivot point (the evaluation point of the registry), geocentric metres.
    pub pivot: [f64; 3],
}

impl MolodenskyBadekasParams {
    pub fn apply(&self, v: [f64; 3]) -> [f64; 3] {
        let h = &self.helmert;
        molodensky_badekas(v, self.pivot, [h.tx, h.ty, h.tz], [h.rx_arcsec, h.ry_arcsec, h.rz_arcsec], h.scale_ppm, h.convention)
    }

    /// The reverse as the EPSG dataset documents it: signs of the translations, rotations and
    /// scale difference changed, pivot unchanged (GN7-2 4.2.2, "Reversibility").
    pub fn reversed(&self) -> MolodenskyBadekasParams {
        MolodenskyBadekasParams { helmert: self.helmert.reversed(), pivot: self.pivot }
    }
}

fn molodensky_badekas(v: [f64; 3], p: [f64; 3], t: [f64; 3], r_arcsec: [f64; 3], ds_ppm: f64, conv: RotationConvention) -> [f64; 3] {
    let s = conv.sign();
    let (rx, ry, rz) = (s * r_arcsec[0] * ARCSEC, s * r_arcsec[1] * ARCSEC, s * r_arcsec[2] * ARCSEC);
    let m = 1.0 + ds_ppm * 1e-6;
    let d = [v[0] - p[0], v[1] - p[1], v[2] - p[2]];
    [
        m * (d[0] - rz * d[1] + ry * d[2]) + p[0] + t[0],
        m * (rz * d[0] + d[1] - rx * d[2]) + p[1] + t[1],
        m * (-ry * d[0] + rx * d[1] + d[2]) + p[2] + t[2],
    ]
}

/// The method of a datum transformation, in the geographic 2D domain: geographic to geocentric,
/// this step, geocentric to geographic (GN7-2 4.3.1).
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub enum DatumMethod {
    /// EPSG method 9603 (geocentric domain 1031, GN7-2 4.2.4): `XT = XS + tX`, ... exactly reversible.
    GeocentricTranslations { tx: f64, ty: f64, tz: f64 },
    /// EPSG methods 9606 (Position Vector) and 9607 (Coordinate Frame), geocentric domain 1033 and 1032.
    Helmert(Helmert),
    /// EPSG methods 1063 (Position Vector) and 9636 (Coordinate Frame), geocentric domain 1061 and 1034.
    MolodenskyBadekas(MolodenskyBadekasParams),
}

impl DatumMethod {
    /// The step in the geocentric domain; `reverse` applies the documented reverse.
    pub fn apply_geocentric(&self, v: [f64; 3], reverse: bool) -> [f64; 3] {
        match *self {
            DatumMethod::GeocentricTranslations { tx, ty, tz } => {
                let s = if reverse { -1.0 } else { 1.0 };
                [v[0] + s * tx, v[1] + s * ty, v[2] + s * tz]
            }
            DatumMethod::Helmert(h) => (if reverse { h.reversed() } else { h }).apply(v),
            DatumMethod::MolodenskyBadekas(m) => (if reverse { m.reversed() } else { m }).apply(v),
        }
    }

    /// Name and EPSG method code of the geographic 2D concatenation (GN7-2 4.3.1).
    pub fn label(&self) -> String {
        match self {
            DatumMethod::GeocentricTranslations { .. } => "Geocentric translations (geog2D domain), EPSG method 9603".to_string(),
            DatumMethod::Helmert(h) => match h.convention {
                RotationConvention::PositionVector => "Position Vector transformation (geog2D domain), EPSG method 9606".to_string(),
                RotationConvention::CoordinateFrame => "Coordinate Frame rotation (geog2D domain), EPSG method 9607".to_string(),
            },
            DatumMethod::MolodenskyBadekas(m) => match m.helmert.convention {
                RotationConvention::PositionVector => "Molodensky-Badekas (PV geog2D domain), EPSG method 1063".to_string(),
                RotationConvention::CoordinateFrame => "Molodensky-Badekas (CF geog2D domain), EPSG method 9636".to_string(),
            },
        }
    }

    /// The parameters as sentences, with units.
    pub fn parameter_text(&self) -> String {
        match self {
            DatumMethod::GeocentricTranslations { tx, ty, tz } => format!("tX {tx} m, tY {ty} m, tZ {tz} m"),
            DatumMethod::Helmert(h) => format!(
                "tX {} m, tY {} m, tZ {} m, rX {} arc-sec, rY {} arc-sec, rZ {} arc-sec, dS {} ppm",
                h.tx, h.ty, h.tz, h.rx_arcsec, h.ry_arcsec, h.rz_arcsec, h.scale_ppm
            ),
            DatumMethod::MolodenskyBadekas(m) => format!(
                "{}; pivot X {} m, Y {} m, Z {} m",
                DatumMethod::Helmert(m.helmert).parameter_text(),
                m.pivot[0],
                m.pivot[1],
                m.pivot[2]
            ),
        }
    }
}

// ---------------------------------------------------------------------------------------------
// The registry's transformations

/// How well a box of data sits inside an area of use.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum Coverage {
    /// Every corner of the data is inside the area.
    Full,
    /// Part of the data is inside.
    Partial,
    /// None of the data is inside.
    Outside,
}

/// One coordinate transformation of the EPSG registry, between two geographic 2D systems, in the
/// direction the registry lists it (source to target).
#[derive(Debug, Clone, Copy, PartialEq, Serialize)]
pub struct Transformation {
    pub epsg: u32,
    /// The name as the transformation's own page shows it.
    pub name: &'static str,
    pub source_crs: u32,
    pub target_crs: u32,
    pub method: DatumMethod,
    pub area: AreaOfUse,
    pub area_text: &'static str,
    /// The registry's operation accuracy, metres.
    pub accuracy_m: f64,
    /// The revision date the page showed.
    pub revision_date: &'static str,
    /// The registry's remarks, as far as they matter here.
    pub remarks: &'static str,
    /// The registry says another transformation replaces this one.
    pub superseded_by: Option<u32>,
    pub source_url: &'static str,
}

impl Transformation {
    /// How a box of latitude and longitude (degrees) sits inside this transformation's area of use.
    pub fn coverage(&self, min_lat: f64, min_lon: f64, max_lat: f64, max_lon: f64) -> Coverage {
        coverage_of(&self.area, min_lat, min_lon, max_lat, max_lon)
    }
}

/// How a box of latitude and longitude (degrees) sits inside an area of use.
pub fn coverage_of(area: &AreaOfUse, min_lat: f64, min_lon: f64, max_lat: f64, max_lon: f64) -> Coverage {
    const TOL: f64 = 1e-6;
    let inside = min_lat >= area.south - TOL && max_lat <= area.north + TOL && min_lon >= area.west - TOL && max_lon <= area.east + TOL;
    if inside {
        return Coverage::Full;
    }
    let disjoint = max_lat < area.south || min_lat > area.north || max_lon < area.west || min_lon > area.east;
    if disjoint {
        Coverage::Outside
    } else {
        Coverage::Partial
    }
}

#[allow(clippy::too_many_arguments)]
fn tr(
    epsg: u32,
    name: &'static str,
    source_crs: u32,
    t: [f64; 3],
    bbox: [f64; 4],
    area_text: &'static str,
    accuracy_m: f64,
    revision_date: &'static str,
    remarks: &'static str,
    superseded_by: Option<u32>,
    source_url: &'static str,
) -> Transformation {
    Transformation {
        epsg,
        name,
        source_crs,
        target_crs: 4326,
        method: DatumMethod::GeocentricTranslations { tx: t[0], ty: t[1], tz: t[2] },
        area: AreaOfUse { south: bbox[0], west: bbox[1], north: bbox[2], east: bbox[3] },
        area_text,
        accuracy_m,
        revision_date,
        remarks,
        superseded_by,
        source_url,
    }
}

/// Every transformation recorded, each read from its own page `https://epsg.io/<code>` on
/// 2026-11-16. All are "Geocentric translations (geog2D domain)" to WGS 84 (EPSG:4326), scope
/// "Military survey" where the page gave one. Bounding boxes are south, west, north, east.
///
/// Arc 1960 (4210), the four the registry lists (the CRS page shows "4 transformations"):
/// 1122, 1284, 1285, 3998.
/// Arc 1950 (4209): 1113, 1120, 1114, 6906, 1117, 6904, 1118, 1116, 1121. The page also lists
/// 1115 (Burundi, 35 m) and 1119 (Democratic Republic of the Congo, 25 m) as deprecated; those two
/// were not opened and are not recorded.
/// Adindan (4201): 1100, 1106, 1103 (East Africa). The page also lists 1101, 1102, 1104 and 1105
/// (Burkina Faso, Cameroon, Mali, Senegal, 44 m each); West Africa, not recorded.
/// Cape (4222): 1128, 1129.
///
/// Where the registry's only accurate option is a grid, nothing is recorded: for none of the
/// systems above did a page show a grid-based transformation, so none is implemented and none
/// was found to be needed. Hartebeesthoek94 (4148): its page lists no transformation to WGS 84,
/// so there is no recorded path from the Lo systems to another datum.
pub fn transformations() -> Vec<Transformation> {
    vec![
        // Arc 1960
        tr(1122, "Arc 1960 to WGS 84 (1)", 4210, [-160.0, -6.0, -302.0], [-11.75, 29.34, 4.63, 41.91], "Kenya; Tanzania.", 35.0, "2020-03-14", "Derived at 25 stations. Accuracy 20m in each axis.", None, "https://epsg.io/1122"),
        tr(1284, "Arc 1960 to WGS 84 (2)", 4210, [-157.0, -2.0, -299.0], [-4.72, 33.9, 4.63, 41.91], "Kenya - onshore.", 6.0, "2020-03-14", "Derived at 24 stations. Accuracy 4m, 3m and 3m in X, Y and Z axes.", None, "https://epsg.io/1284"),
        tr(1285, "Arc 1960 to WGS 84 (3)", 4210, [-175.0, -23.0, -303.0], [-11.75, 29.34, -0.99, 40.48], "Tanzania - onshore.", 15.0, "2020-03-14", "Derived at 12 stations. Accuracy 6m, 9m and 10m in X, Y and Z axes.", None, "https://epsg.io/1285"),
        tr(
            3998,
            "Arc 1960 to WGS 84 (4)",
            4210,
            [-153.0, -5.0, -292.0],
            [-4.45, 28.98, -2.3, 30.86],
            "Burundi.",
            35.0,
            "2025-03-16",
            "Derived at 3 stations. Accuracy 20m in each axis. The source gives the source CRS as Arc 1950; OGP believes it should be Arc 1960.",
            None,
            "https://epsg.io/3998",
        ),
        // Arc 1950
        tr(
            1113,
            "Arc 1950 to WGS 84 (1)",
            4209,
            [-143.0, -90.0, -294.0],
            [-30.66, 19.99, -8.19, 35.93],
            "Botswana; Eswatini (Swaziland); Lesotho; Malawi; Zambia; Zimbabwe.",
            44.0,
            "2025-01-06",
            "Derived at 41 stations. Accuracy 20m, 33m and 20m in X, Y and Z axes. Eswatini and Lesotho do not use Arc 1950 but do use Cape (4222).",
            None,
            "https://epsg.io/1113",
        ),
        tr(1120, "Arc 1950 to WGS 84 (8)", 4209, [-147.0, -74.0, -283.0], [-18.08, 21.99, -8.19, 33.71], "Zambia.", 41.0, "2020-03-14", "Derived at 5 stations. Accuracy 21m, 21m and 27m in X, Y and Z axes.", None, "https://epsg.io/1120"),
        tr(1114, "Arc 1950 to WGS 84 (2)", 4209, [-138.0, -105.0, -289.0], [-26.88, 19.99, -17.78, 29.38], "Botswana.", 7.0, "2020-03-14", "Derived at 9 stations. Accuracy 3m, 5m and 3m in X, Y and Z axes.", None, "https://epsg.io/1114"),
        tr(
            6906,
            "Arc 1950 to WGS 84 (10)",
            4209,
            [-145.0, -97.0, -292.0],
            [-22.42, 25.23, -15.61, 33.08],
            "Zimbabwe.",
            17.0,
            "2020-03-14",
            "Derived at 38 stations. Accuracy 10m in each of X, Y and Z axes. Replaces Arc 1950 to WGS 84 (9), tfm code 1121.",
            None,
            "https://epsg.io/6906",
        ),
        tr(1117, "Arc 1950 to WGS 84 (5)", 4209, [-161.0, -73.0, -317.0], [-17.14, 32.68, -9.37, 35.93], "Malawi.", 27.0, "2020-03-14", "Derived at 6 stations. Accuracy 9m, 24m and 8m in X, Y and Z axes.", None, "https://epsg.io/1117"),
        tr(
            6904,
            "Arc 1950 to WGS 84 (11)",
            4209,
            [-179.0, -81.0, -314.0],
            [-17.14, 32.68, -9.37, 35.93],
            "Malawi.",
            29.0,
            "2020-03-14",
            "Derived at 7 stations. Accuracy 13m, 25m and 7m in X, Y and Z axes. The source gives the source CRS as Arc 1960; OGP believes it should be Arc 1950.",
            None,
            "https://epsg.io/6904",
        ),
        tr(
            1118,
            "Arc 1950 to WGS 84 (6)",
            4209,
            [-134.0, -105.0, -295.0],
            [-27.32, 30.79, -25.72, 32.14],
            "Eswatini (Swaziland).",
            26.0,
            "2025-01-06",
            "Derived at 4 stations. Accuracy 15m in each axis. Eswatini does not use Arc 1950 but does use Cape (4222).",
            None,
            "https://epsg.io/1118",
        ),
        tr(
            1116,
            "Arc 1950 to WGS 84 (4)",
            4209,
            [-125.0, -108.0, -295.0],
            [-30.66, 27.01, -28.57, 29.46],
            "Lesotho.",
            10.0,
            "2025-01-06",
            "Derived at 5 stations. Accuracy 3m, 3m and 8m in X, Y and Z axes. Lesotho does not use Arc 1950 but does use Cape (4222).",
            None,
            "https://epsg.io/1116",
        ),
        tr(
            1121,
            "Arc 1950 to WGS 84 (9)",
            4209,
            [-142.0, -96.0, -293.0],
            [-22.42, 25.23, -15.61, 33.08],
            "Zimbabwe.",
            15.0,
            "2020-03-14",
            "Derived at 10 stations. Accuracy 5m, 8m and 11m in X, Y and Z axes. Replaced by Arc 1950 to WGS 84 (10), tfm code 6906.",
            Some(6906),
            "https://epsg.io/1121",
        ),
        // Adindan
        tr(
            1100,
            "Adindan to WGS 84 (1)",
            4201,
            [-166.0, -15.0, 204.0],
            [3.4, 21.82, 22.24, 47.99],
            "Eritrea; Ethiopia; South Sudan; Sudan.",
            9.0,
            "2020-03-14",
            "Derived at 22 stations. Accuracy 5m in each axis.",
            None,
            "https://epsg.io/1100",
        ),
        tr(1106, "Adindan to WGS 84 (7)", 4201, [-161.0, -14.0, 205.0], [3.49, 21.82, 22.24, 38.66], "South Sudan. Sudan - onshore.", 7.0, "2020-03-14", "Derived at 14 stations. Accuracy 3m, 5m and 3m in X, Y and Z axes.", None, "https://epsg.io/1106"),
        tr(1103, "Adindan to WGS 84 (4)", 4201, [-165.0, -11.0, 206.0], [3.4, 32.99, 14.89, 47.99], "Ethiopia.", 6.0, "2020-03-14", "Derived at 8 stations. Accuracy 3m in each axis.", None, "https://epsg.io/1103"),
        // Cape
        tr(1128, "Cape to WGS 84 (1)", 4222, [-136.0, -108.0, -292.0], [-34.88, 16.45, -22.13, 32.95], "South Africa - mainland onshore.", 9.0, "2020-03-14", "Derived at 5 stations. Accuracy 3m, 6m and 6m in X, Y and Z axes.", None, "https://epsg.io/1128"),
        tr(
            1129,
            "Cape to WGS 84 (2)",
            4222,
            [-134.73, -110.92, -292.66],
            [-34.88, 16.45, -22.13, 32.95],
            "South Africa - mainland onshore.",
            15.0,
            "2021-09-23",
            "Parameters derived from Cape to Hartebeesthoek94 (1) (code 1504), treating Hartebeesthoek94 and WGS 84 as equivalent within the accuracy. Residuals should not exceed 15 metres.",
            None,
            "https://epsg.io/1129",
        ),
    ]
}

/// A transformation by its EPSG code.
pub fn transformation(epsg: u32) -> Option<Transformation> {
    transformations().into_iter().find(|t| t.epsg == epsg)
}

/// The transformations that connect two geographic systems directly, with `true` when the
/// registry lists it the other way round (the reverse is applied). The caller chooses: nothing is
/// picked here. Ordered by stated accuracy, best first, then by code.
pub fn transformations_between(from_geog: u32, to_geog: u32) -> Vec<(Transformation, bool)> {
    let mut v: Vec<(Transformation, bool)> = transformations()
        .into_iter()
        .filter_map(|t| {
            if t.source_crs == from_geog && t.target_crs == to_geog {
                Some((t, false))
            } else if t.source_crs == to_geog && t.target_crs == from_geog {
                Some((t, true))
            } else {
                None
            }
        })
        .collect();
    v.sort_by(|a, b| a.0.accuracy_m.total_cmp(&b.0.accuracy_m).then(a.0.epsg.cmp(&b.0.epsg)));
    v
}

/// The transformations between two geographic systems whose area of use contains the whole box of
/// data (latitude and longitude in degrees), best accuracy first. A transformation whose area
/// only partly contains the data, or not at all, is left out; use [`transformations_between`] and
/// [`Transformation::coverage`] to see those. For Arc 1960 to WGS 84 in Tanzania this lists 1285
/// and 1122; in Kenya, 1284 and 1122; the caller decides.
pub fn transformations_containing(from_geog: u32, to_geog: u32, min_lat: f64, min_lon: f64, max_lat: f64, max_lon: f64) -> Vec<(Transformation, bool)> {
    transformations_between(from_geog, to_geog).into_iter().filter(|(t, _)| t.coverage(min_lat, min_lon, max_lat, max_lon) == Coverage::Full).collect()
}

// ---------------------------------------------------------------------------------------------
// Tests

#[cfg(test)]
mod tests {
    use super::*;
    use crate::crs::WGS84;

    fn dms(d: f64, m: f64, s: f64) -> f64 {
        d.signum() * (d.abs() + m / 60.0 + s / 3600.0)
    }

    fn close(a: f64, b: f64, tol: f64, what: &str) {
        assert!((a - b).abs() <= tol, "{what}: {a} vs {b} (tol {tol})");
    }

    /// GN7-2 section 4.1.1, example: a North Sea point, geocentric to geographic.
    #[test]
    fn gn7_2_4_1_1_geocentric_to_geographic_example() {
        // The note gives the WGS 84 inverse flattening as 298.2572236.
        let ell = Ellipsoid { name: "WGS 84 (as the note writes it)", a: 6_378_137.0, inverse_flattening: 298.257_223_6, source: "GN7-2 4.1.1" };
        let (lat, lon, h) = geocentric_to_geographic(&ell, [3_771_793.968, 140_253.342, 5_124_304.349]);
        close(lat, dms(53.0, 48.0, 33.820), 0.001 / 3600.0, "latitude");
        close(lon, dms(2.0, 7.0, 46.380), 0.001 / 3600.0, "longitude");
        close(h, 73.0, 0.01, "height");
        // And the forward conversion gives the note's geocentric coordinates back.
        let v = geographic_to_geocentric(&ell, dms(53.0, 48.0, 33.820), dms(2.0, 7.0, 46.380), 73.0);
        close(v[0], 3_771_793.968, 0.02, "X");
        close(v[1], 140_253.342, 0.02, "Y");
        close(v[2], 5_124_304.349, 0.02, "Z");
    }

    #[test]
    fn geocentric_conversion_round_trips_everywhere_including_the_poles_and_depth() {
        for ell in [WGS84, CLARKE_1880_RGS, CLARKE_1880_ARC] {
            for lat in [-90.0, -89.9, -60.0, -34.0, -11.6, 0.0, 0.001, 4.6, 45.0, 89.99, 90.0] {
                for lon in [-179.0, -66.0, 0.0, 33.3, 179.9] {
                    for h in [-500.0, 0.0, 1500.0, 20_000.0] {
                        let v = geographic_to_geocentric(&ell, lat, lon, h);
                        let (la, lo, hh) = geocentric_to_geographic(&ell, v);
                        let pole = lat.abs() > 89.999_999;
                        close(la, lat, 1e-10, "lat");
                        if !pole {
                            close(lo, lon, 1e-9, "lon");
                        }
                        close(hh, h, 1e-6, "h");
                    }
                }
            }
        }
    }

    /// GN7-2 section 4.2.3, example: WGS 72 to WGS 84 (the note's transformation code 1238), in
    /// both rotation conventions. Parameters are the note's own (dS 0.219 ppm).
    #[test]
    fn gn7_2_4_2_3_helmert_example_in_both_conventions() {
        let pv = Helmert { tx: 0.0, ty: 0.0, tz: 4.5, rx_arcsec: 0.0, ry_arcsec: 0.0, rz_arcsec: 0.554, scale_ppm: 0.219, convention: RotationConvention::PositionVector };
        let cf = Helmert { rz_arcsec: -0.554, convention: RotationConvention::CoordinateFrame, ..pv };
        let src = [3_657_660.66, 255_768.55, 5_201_382.11];
        for (name, h) in [("PV", pv), ("CF", cf)] {
            let t = h.apply(src);
            close(t[0], 3_657_660.78, 0.006, name);
            close(t[1], 255_778.43, 0.006, name);
            close(t[2], 5_201_387.75, 0.006, name);
        }
        // The same parameters read in the wrong convention would not agree: the rotation of
        // 0.554" at 255 km from the axis is 0.68 m.
        let wrong = Helmert { convention: RotationConvention::CoordinateFrame, ..pv }.apply(src);
        assert!((wrong[1] - 255_778.43).abs() > 0.5);
        // Reversal by changing signs: misclosure well under the 2 cm the note allows.
        let back = pv.reversed().apply(pv.apply(src));
        for k in 0..3 {
            assert!((back[k] - src[k]).abs() < 0.02, "{k}: {}", back[k] - src[k]);
        }
    }

    /// GN7-2 section 4.2.4, example: three geocentric translations WGS 84 to ED50.
    #[test]
    fn gn7_2_4_2_4_geocentric_translation_example() {
        let m = DatumMethod::GeocentricTranslations { tx: 84.87, ty: 96.49, tz: 116.95 };
        let t = m.apply_geocentric([3_771_793.97, 140_253.34, 5_124_304.35], false);
        close(t[0], 3_771_878.84, 0.005, "X");
        close(t[1], 140_349.83, 0.005, "Y");
        close(t[2], 5_124_421.30, 0.005, "Z");
        // Exactly reversible.
        let b = m.apply_geocentric(t, true);
        close(b[0], 3_771_793.97, 1e-9, "X back");
    }

    /// GN7-2 section 4.3.1, example: La Canoa to REGVEN (transformation code 1771) by
    /// Molodensky-Badekas (Coordinate Frame), through every step of the note.
    #[test]
    fn gn7_2_4_3_1_molodensky_badekas_example() {
        let intl_1924 = Ellipsoid { name: "International 1924", a: 6_378_388.0, inverse_flattening: 297.0, source: "GN7-2 4.3.1 example" };
        let wgs84_note = Ellipsoid { name: "WGS 84", a: 6_378_137.0, inverse_flattening: 298.257_223_6, source: "GN7-2 4.3.1 example" };
        let mb = MolodenskyBadekasParams {
            helmert: Helmert { tx: -270.933, ty: 115.599, tz: -360.226, rx_arcsec: -5.266, ry_arcsec: -1.238, rz_arcsec: 2.381, scale_ppm: -5.109, convention: RotationConvention::CoordinateFrame },
            pivot: [2_464_351.59, -5_783_466.61, 974_809.81],
        };
        let lat = dms(9.0, 35.0, 0.386);
        let lon = -dms(66.0, 4.0, 48.091);
        let s = geographic_to_geocentric(&intl_1924, lat, lon, 201.46);
        close(s[0], 2_550_408.96, 0.01, "XS");
        close(s[1], -5_749_912.26, 0.01, "YS");
        close(s[2], 1_054_891.11, 0.01, "ZS");
        let t = mb.apply(s);
        close(t[0], 2_550_138.46, 0.01, "XT");
        close(t[1], -5_749_799.87, 0.01, "YT");
        close(t[2], 1_054_530.82, 0.01, "ZT");
        let (la, lo, h) = geocentric_to_geographic(&wgs84_note, t);
        close(la, dms(9.0, 34.0, 49.001), 0.001 / 3600.0, "latitude");
        close(lo, -dms(66.0, 4.0, 54.705), 0.001 / 3600.0, "longitude");
        close(h, 180.51, 0.01, "height");
        // Reverse by sign change: the misclosure is within the note's 2 cm.
        let back = mb.reversed().apply(t);
        for k in 0..3 {
            assert!((back[k] - s[k]).abs() < 0.02, "{k}: {}", back[k] - s[k]);
        }
        // Position Vector and Coordinate Frame differ by the rotation signs only.
        let pv = MolodenskyBadekasParams { helmert: Helmert { convention: RotationConvention::PositionVector, rx_arcsec: 5.266, ry_arcsec: 1.238, rz_arcsec: -2.381, ..mb.helmert }, ..mb };
        let t2 = pv.apply(s);
        for k in 0..3 {
            close(t2[k], t[k], 1e-9, "PV equals CF with signs reversed");
        }
    }

    #[test]
    fn the_registry_records_are_what_the_pages_said() {
        let all = transformations();
        assert_eq!(all.len(), 18);
        // Unique codes.
        let mut codes: Vec<u32> = all.iter().map(|t| t.epsg).collect();
        codes.sort_unstable();
        codes.dedup();
        assert_eq!(codes.len(), all.len());
        // The four Arc 1960 transformations, exactly.
        let arc: Vec<u32> = transformations_between(4210, 4326).iter().map(|(t, _)| t.epsg).collect();
        assert_eq!(arc, vec![1284, 1285, 1122, 3998]);
        let t1122 = transformation(1122).unwrap();
        assert_eq!(t1122.method, DatumMethod::GeocentricTranslations { tx: -160.0, ty: -6.0, tz: -302.0 });
        assert_eq!(t1122.accuracy_m, 35.0);
        // Every record's source and target systems are known, and the box is a box.
        for t in &all {
            assert!(geographic_crs(t.source_crs).is_some() && geographic_crs(t.target_crs).is_some(), "{}", t.epsg);
            assert!(t.area.south < t.area.north && t.area.west < t.area.east, "{}", t.epsg);
            assert!(t.accuracy_m > 0.0 && t.source_url.ends_with(&t.epsg.to_string()), "{}", t.epsg);
        }
        // The superseded record points at one that exists.
        assert_eq!(transformation(1121).unwrap().superseded_by, Some(6906));
        assert!(transformation(6906).is_some());
    }

    #[test]
    fn transformations_whose_area_contains_the_data_are_listed_not_chosen() {
        // Dodoma, Tanzania.
        let tz: Vec<u32> = transformations_containing(4210, 4326, -6.3, 35.6, -6.0, 35.9).iter().map(|(t, _)| t.epsg).collect();
        assert_eq!(tz, vec![1285, 1122]);
        // Nairobi, Kenya, lies at 1.3 degrees south: inside the bounding box of the Tanzania transformation too
        // (its box reaches 0.99 south). Boxes are coarse; the list is a list, and the caller chooses.
        let ke: Vec<u32> = transformations_containing(4210, 4326, -1.4, 36.7, -1.2, 36.9).iter().map(|(t, _)| t.epsg).collect();
        assert_eq!(ke, vec![1284, 1285, 1122]);
        // Kampala, Uganda: no Arc 1960 transformation is for Uganda, and the list says so.
        let ug = transformations_containing(4210, 4326, 0.2, 32.5, 0.4, 32.7);
        assert!(ug.iter().all(|(t, _)| t.epsg != 1284 && t.epsg != 1285), "{ug:?}");
        // The data may straddle two areas: no single one contains it, but partial coverage is reported.
        assert_eq!(transformation(1284).unwrap().coverage(-5.0, 33.0, 0.0, 38.0), Coverage::Partial);
        assert_eq!(transformation(1284).unwrap().coverage(-10.0, 30.0, -8.0, 31.0), Coverage::Outside);
        // The reverse direction is found too.
        assert!(transformations_between(4326, 4210).iter().all(|(_, rev)| *rev));
    }

    #[test]
    fn every_translation_changes_a_point_by_its_own_length_and_back() {
        for t in transformations() {
            let DatumMethod::GeocentricTranslations { tx, ty, tz } = t.method else { panic!() };
            let src = geographic_crs(t.source_crs).unwrap();
            let dst = geographic_crs(t.target_crs).unwrap();
            let lat = (t.area.south + t.area.north) / 2.0;
            let lon = (t.area.west + t.area.east) / 2.0;
            let v = geographic_to_geocentric(&src.ellipsoid, lat, lon, 0.0);
            let w = t.method.apply_geocentric(v, false);
            let d = ((w[0] - v[0]).powi(2) + (w[1] - v[1]).powi(2) + (w[2] - v[2]).powi(2)).sqrt();
            close(d, (tx * tx + ty * ty + tz * tz).sqrt(), 1e-6, "length");
            let b = t.method.apply_geocentric(w, true);
            close(b[0], v[0], 1e-9, "back");
            let (la, lo, _) = geocentric_to_geographic(&dst.ellipsoid, w);
            assert!(la.is_finite() && lo.is_finite());
        }
    }
}
