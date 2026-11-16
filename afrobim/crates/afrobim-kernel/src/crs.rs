//! The coordinate system of a project: a document-level setting, as in a Civil 3D drawing's
//! zone setting, plus the transverse Mercator maths to say where on the Earth a coordinate is.
//!
//! Nothing here reprojects model data. Objects keep the coordinates of their source. The system
//! is a record of what those coordinates mean. The projection functions exist so that an import
//! can report the latitude and longitude of a dataset, which shows a wrong zone or swapped
//! northing and easting at once, and so that grid convergence and scale can be read at a point.
//!
//! # Where each constant comes from
//!
//! * WGS 84 ellipsoid: a = 6 378 137 m, 1/f = 298.257 223 563. Fetched from the EPSG registry
//!   WKT of EPSG:32736, <https://epsg.io/32736.wkt> (ellipsoid EPSG:7030).
//! * UTM parameters (scale 0.9996, false easting 500 000 m, false northing 0 north and
//!   10 000 000 m south, central meridian 6 zone - 183 degrees): same WKT for EPSG:32736 and
//!   <https://epsg.io/32636.json>, which also gives the area of use of zone 36 north (30 to 36
//!   degrees east, equator to 84 north) and south (80 south to equator). The other 58 WGS 84 UTM
//!   zones use the same rule (6 degree band, 84 north or 80 south); only zones 36N and 36S were
//!   fetched.
//! * Arc 1960 ellipsoid Clarke 1880 (RGS): a = 6 378 249.145 m, 1/f = 293.465, from
//!   <https://epsg.io/21096.wkt> (geographic CRS EPSG:4210).
//! * Arc 1960 / UTM zones 35S 36S 37S 35N 36N 37N = EPSG:21035, 21036, 21037, 21095, 21096,
//!   21097: central meridian, scale, false easting, false northing from
//!   `https://epsg.io/<code>.wkt`, areas of use from `https://epsg.io/<code>.json`, all fetched.
//!   Transverse Mercator is EPSG method 9807 (same JSON).
//!
//! * Adindan / UTM zones 35N to 38N = EPSG:20135 to 20138 and Arc 1950 / UTM zones 34S to 36S =
//!   EPSG:20934 to 20936: parameters, base CRS and area of use from `https://epsg.io/<code>.json`
//!   (all with method 9807, scale 0.9996, false easting 500 000 m), fetched 2026-11-16; the
//!   Adindan ellipsoid is Clarke 1880 (RGS) (<https://epsg.io/4201.wkt>), the Arc 1950 ellipsoid is
//!   Clarke 1880 (Arc), a = 6 378 249.145 m, 1/f = 293.4663077 (<https://epsg.io/4209.wkt>).
//! * Hartebeesthoek94 / Lo15 to Lo33 (odd) = EPSG:2046 to 2055: Transverse Mercator (South
//!   Orientated), method 9808, scale 1, no false origin, axes westing then southing, base CRS
//!   EPSG:4148 on the WGS 84 ellipsoid; from `https://epsg.io/<code>.json`, fetched 2026-11-16. The
//!   worked example of GN7-2 section 3.2.3.3 (Lo29) is a test, so the axis convention is checked.
//!
//! Projection series: IOGP Publication 373-7-2 (Geomatics Guidance Note 7, part 2, September 2019,
//! <https://www.iogp.org/wp-content/uploads/2019/09/373-07-02.pdf>), section 3.2.3.1, the "JHS"
//! formulas for the Transverse Mercator (EPSG method 9807), which are the Krüger series in the third
//! flattening n (Karney, C. F. F. (2011), J. Geodesy 85(8), 475-485, arXiv:1002.1417) taken to
//! fourth order in n: the coefficients h1 to h4 forward and h1' to h4' in reverse are the note's. The
//! latitude in the reverse direction is found by the note's iteration on Q'', not by a series. The
//! note's British National Grid example is a test, as are forward against inverse, the meridian arc
//! by numerical integration, and scale and convergence by finite differences.
//!
//! Geographic coordinates returned by these functions are on the datum of the system itself
//! (WGS 84 for WGS 84 / UTM, Arc 1960 for Arc 1960 / UTM). Moving a coordinate to another datum is
//! not done here but in [`crate::crs_plan`], explicitly, using the registry transformations of
//! [`crate::crs_transform`].

use serde::{Deserialize, Serialize};

// ---------------------------------------------------------------------------------------------
// Ellipsoids

/// A reference ellipsoid: semi-major axis in metres and inverse flattening.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Ellipsoid {
    pub name: &'static str,
    pub a: f64,
    pub inverse_flattening: f64,
    /// Where the numbers were read.
    pub source: &'static str,
}

/// WGS 84 (EPSG:7030). Source: <https://epsg.io/32736.wkt>.
pub const WGS84: Ellipsoid =
    Ellipsoid { name: "WGS 84", a: 6_378_137.0, inverse_flattening: 298.257_223_563, source: "https://epsg.io/32736.wkt" };

/// Clarke 1880 (RGS), the ellipsoid of Arc 1960 (EPSG:4210). Source: <https://epsg.io/21096.wkt>.
pub const CLARKE_1880_RGS: Ellipsoid =
    Ellipsoid { name: "Clarke 1880 (RGS)", a: 6_378_249.145, inverse_flattening: 293.465, source: "https://epsg.io/21096.wkt" };

/// Clarke 1880 (Arc), the ellipsoid of Arc 1950 (EPSG:4209) and Cape (EPSG:4222): a = 6 378 249.145 m,
/// 1/f = 293.4663077. Source: <https://epsg.io/4209.wkt> and <https://epsg.io/4222.wkt>, retrieved 2026-11-16.
pub const CLARKE_1880_ARC: Ellipsoid =
    Ellipsoid { name: "Clarke 1880 (Arc)", a: 6_378_249.145, inverse_flattening: 293.466_307_7, source: "https://epsg.io/4209.wkt" };

// ---------------------------------------------------------------------------------------------
// Transverse Mercator

/// A transverse Mercator projection on an ellipsoid (EPSG method 9807), by Karney's Krüger series.
#[derive(Debug, Clone, PartialEq)]
pub struct TransverseMercator {
    ellipsoid: Ellipsoid,
    lon0: f64,
    k0: f64,
    false_easting: f64,
    false_northing: f64,
    /// Conformal-latitude angle xi of the latitude of natural origin (zero when it is the equator).
    xi0: f64,
    /// +1 for eastings and northings; -1 for the south-orientated method (westings and southings,
    /// EPSG method 9808, Guidance Note 7-2 section 3.2.3.3).
    sign: f64,
    e: f64,
    /// Rectifying radius A of Karney eq. 14, metres.
    radius: f64,
    /// h1..h4 of GN7-2 3.2.3.1 (forward) and h1'..h4' (inverse).
    alpha: [f64; 4],
    beta: [f64; 4],
}

impl TransverseMercator {
    /// `central_meridian_deg` in degrees east; latitude of origin is the equator.
    pub fn new(ellipsoid: Ellipsoid, central_meridian_deg: f64, k0: f64, false_easting: f64, false_northing: f64) -> Self {
        Self::new_with_origin(ellipsoid, 0.0, central_meridian_deg, k0, false_easting, false_northing, false)
    }

    /// The general case: a latitude of natural origin, and the south-orientated variant.
    ///
    /// With `south_oriented` the first coordinate is the westing and the second the southing, in
    /// that axis order (the order the registry gives for the Hartebeesthoek94 Lo systems). The
    /// northing term is `FN + k0 (B xi - M0)` with `M0 = B xi0` the meridian distance of the
    /// latitude of origin (Guidance Note 7-2 section 3.2.3.1, JHS formulas); the south-orientated
    /// westing and southing are `FE - k0 B eta` and `FN - k0 (B xi - M0)` (section 3.2.3.3).
    pub fn new_with_origin(ellipsoid: Ellipsoid, lat0_deg: f64, central_meridian_deg: f64, k0: f64, false_easting: f64, false_northing: f64, south_oriented: bool) -> Self {
        let f = 1.0 / ellipsoid.inverse_flattening;
        let n = f / (2.0 - f);
        let (n2, n3, n4) = (n * n, n * n * n, n * n * n * n);
        let radius = ellipsoid.a / (1.0 + n) * (1.0 + n2 / 4.0 + n4 / 64.0);
        // The coefficients h1..h4 (forward) and h1'..h4' (inverse) of the JHS formulas, GN7-2 section
        // 3.2.3.1 (the Krueger series in n, which is Karney's alpha and beta to fourth order).
        let alpha = [
            n / 2.0 - 2.0 * n2 / 3.0 + 5.0 * n3 / 16.0 + 41.0 * n4 / 180.0,
            13.0 * n2 / 48.0 - 3.0 * n3 / 5.0 + 557.0 * n4 / 1440.0,
            61.0 * n3 / 240.0 - 103.0 * n4 / 140.0,
            49561.0 * n4 / 161_280.0,
        ];
        let beta = [
            n / 2.0 - 2.0 * n2 / 3.0 + 37.0 * n3 / 96.0 - n4 / 360.0,
            n2 / 48.0 + n3 / 15.0 - 437.0 * n4 / 1440.0,
            17.0 * n3 / 480.0 - 37.0 * n4 / 840.0,
            4397.0 * n4 / 161_280.0,
        ];
        let e = (f * (2.0 - f)).sqrt();
        let mut tm = Self {
            ellipsoid,
            lon0: central_meridian_deg.to_radians(),
            k0,
            false_easting,
            false_northing,
            xi0: 0.0,
            sign: if south_oriented { -1.0 } else { 1.0 },
            e,
            radius,
            alpha,
            beta,
        };
        if lat0_deg != 0.0 {
            let tau_c = tm.conformal_tau(lat0_deg.to_radians().tan());
            let xi_c = tau_c.atan();
            tm.xi0 = xi_c + tm.alpha.iter().enumerate().map(|(j, a)| a * (2.0 * (j + 1) as f64 * xi_c).sin()).sum::<f64>();
        }
        tm
    }

    /// Conformal tangent tau' from tau = tan(phi) (Karney 2011).
    fn conformal_tau(&self, tau: f64) -> f64 {
        let e = self.e;
        let sigma = (e * (e * tau / (1.0 + tau * tau).sqrt()).atanh()).sinh();
        tau * (1.0 + sigma * sigma).sqrt() - sigma * (1.0 + tau * tau).sqrt()
    }

    /// True for the south-orientated variant (westing, southing).
    pub fn is_south_oriented(&self) -> bool {
        self.sign < 0.0
    }

    /// The longitude of natural origin in degrees.
    pub fn central_meridian_deg(&self) -> f64 {
        self.lon0.to_degrees()
    }

    pub fn ellipsoid(&self) -> &Ellipsoid {
        &self.ellipsoid
    }

    /// Geographic degrees (latitude, longitude) to (easting, northing) metres.
    /// `None` when an input is not finite, the latitude is beyond 90 degrees, or the point is
    /// so far from the central meridian that the series is meaningless (beyond 80 degrees).
    pub fn forward(&self, lat_deg: f64, lon_deg: f64) -> Option<(f64, f64)> {
        let (_, _, xi, eta, _) = self.forward_parts(lat_deg, lon_deg)?;
        Some((
            self.false_easting + self.sign * self.k0 * self.radius * eta,
            self.false_northing + self.sign * self.k0 * self.radius * (xi - self.xi0),
        ))
    }

    /// (xi', eta', xi, eta, conformal tau') shared by forward, scale and convergence.
    fn forward_parts(&self, lat_deg: f64, lon_deg: f64) -> Option<(f64, f64, f64, f64, f64)> {
        if !lat_deg.is_finite() || !lon_deg.is_finite() || lat_deg.abs() > 90.0 {
            return None;
        }
        let mut lam = (lon_deg.to_radians() - self.lon0) % std::f64::consts::TAU;
        if lam > std::f64::consts::PI {
            lam -= std::f64::consts::TAU;
        } else if lam < -std::f64::consts::PI {
            lam += std::f64::consts::TAU;
        }
        if lam.abs() > 80f64.to_radians() {
            return None;
        }
        let phi = lat_deg.to_radians();
        let tau = phi.tan();
        let tau_c = self.conformal_tau(tau);
        let (sl, cl) = lam.sin_cos();
        let xi_c = tau_c.atan2(cl);
        let eta_c = (sl / (tau_c * tau_c + cl * cl).sqrt()).asinh();
        let (mut xi, mut eta) = (xi_c, eta_c);
        for (j, a) in self.alpha.iter().enumerate() {
            let k = 2.0 * (j + 1) as f64;
            xi += a * (k * xi_c).sin() * (k * eta_c).cosh();
            eta += a * (k * xi_c).cos() * (k * eta_c).sinh();
        }
        Some((xi_c, eta_c, xi, eta, tau_c))
    }

    /// (easting, northing) metres to geographic degrees (latitude, longitude).
    pub fn inverse(&self, easting_m: f64, northing_m: f64) -> Option<(f64, f64)> {
        if !easting_m.is_finite() || !northing_m.is_finite() {
            return None;
        }
        let xi = self.xi0 + self.sign * (northing_m - self.false_northing) / (self.k0 * self.radius);
        let eta = self.sign * (easting_m - self.false_easting) / (self.k0 * self.radius);
        let (mut xi1, mut eta1) = (xi, eta);
        for (j, b) in self.beta.iter().enumerate() {
            let k = 2.0 * (j + 1) as f64;
            xi1 -= b * (k * xi).sin() * (k * eta).cosh();
            eta1 -= b * (k * xi).cos() * (k * eta).sinh();
        }
        // GN7-2 3.2.3.1, reverse: beta' = asin(sin xi0' / cosh eta0'), Q' = asinh(tan beta'), then Q'' = Q' +
        // e atanh(e tanh Q'') iterated until the change is insignificant, phi = atan(sinh Q'').
        let chi = (xi1.sin() / eta1.cosh()).asin();
        let q1 = chi.tan().asinh();
        let mut q2 = q1;
        for _ in 0..30 {
            let next = q1 + self.e * (self.e * q2.tanh()).atanh();
            let done = (next - q2).abs() < 1e-15;
            q2 = next;
            if done {
                break;
            }
        }
        let latitude = q2.sinh().atan();
        let longitude = self.lon0 + eta1.sinh().atan2(xi1.cos());
        let (lat, lon) = (latitude.to_degrees(), longitude.to_degrees());
        (lat.is_finite() && lon.is_finite()).then_some((lat, lon))
    }

    /// Point scale factor k and grid convergence gamma (degrees) at a geographic position.
    ///
    /// Both are derived from the derivative of the complex series of Karney (2011), with
    /// zeta' = xi' + i eta' and D = d zeta / d zeta' = 1 + sum 2 j alpha_j cos(2 j zeta'):
    /// k = k0 (A / a) |D| sqrt(1 - e^2 sin^2 phi) sqrt(1 + tau^2) / sqrt(tau'^2 + cos^2 lambda), and
    /// gamma = atan(tan xi' tanh eta') - atan2(Im D, Re D), the angle from grid north to true
    /// north, counterclockwise positive (east of the central meridian in the northern hemisphere
    /// true north lies west of grid north, so gamma is positive there). The equation numbers of
    /// the paper were not read, so none is cited: the correctness evidence is the tests, which
    /// check both against finite differences of [`forward`](Self::forward) on both ellipsoids
    /// and both hemispheres.
    pub fn scale_and_convergence(&self, lat_deg: f64, lon_deg: f64) -> Option<(f64, f64)> {
        // A south-orientated grid has its axes turned half a turn; the convergence is not defined here.
        if self.is_south_oriented() {
            return None;
        }
        let (xi_c, eta_c, _, _, tau_c) = self.forward_parts(lat_deg, lon_deg)?;
        // D = 1 + sum 2 j alpha_j cos(2 j (xi' + i eta')), cos(a + ib) = cos a cosh b - i sin a sinh b.
        let (mut dr, mut di) = (1.0, 0.0);
        for (j, a) in self.alpha.iter().enumerate() {
            let k = 2.0 * (j + 1) as f64;
            dr += k * a * (k * xi_c).cos() * (k * eta_c).cosh();
            di -= k * a * (k * xi_c).sin() * (k * eta_c).sinh();
        }
        let phi = lat_deg.to_radians();
        let tau = phi.tan();
        let lam = lon_deg.to_radians() - self.lon0;
        let cl = lam.cos();
        let k = self.k0 * (self.radius / self.ellipsoid.a) * dr.hypot(di) * (1.0 - self.e * self.e * phi.sin().powi(2)).sqrt()
            * (1.0 + tau * tau).sqrt()
            / (tau_c * tau_c + cl * cl).sqrt();
        let gamma = (xi_c.tan() * eta_c.tanh()).atan() - di.atan2(dr);
        Some((k, gamma.to_degrees()))
    }
}

// ---------------------------------------------------------------------------------------------
// The project coordinate system

/// What is known about a coordinate system.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum CrsKind {
    /// A projected system whose maths is implemented here: coordinates can be located on the Earth.
    Projected,
    /// Known by name or code only. Nothing can be computed from it.
    NamedOnly,
    /// A local site grid with no geodetic meaning.
    LocalGrid,
}

/// The linear unit of the coordinates. Metric only.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
pub enum LinearUnit {
    #[default]
    Metre,
}

/// Where the project coordinate system was taken from.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
pub enum CrsSource {
    #[default]
    Typed,
    LandXml,
    Ifc,
    Survey,
}

impl CrsSource {
    pub fn label(self) -> &'static str {
        match self {
            CrsSource::Typed => "typed by the user",
            CrsSource::LandXml => "a LandXML file",
            CrsSource::Ifc => "an IFC file",
            CrsSource::Survey => "a survey file",
        }
    }
}

/// A box of latitude and longitude in degrees: an EPSG area of use.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct AreaOfUse {
    pub south: f64,
    pub west: f64,
    pub north: f64,
    pub east: f64,
}

impl AreaOfUse {
    pub fn contains(&self, lat: f64, lon: f64) -> bool {
        const TOL: f64 = 1e-6;
        lat >= self.south - TOL && lat <= self.north + TOL && lon >= self.west - TOL && lon <= self.east + TOL
    }
}

/// The project coordinate system, stored in the document (see [`crate::project_crs`]).
///
/// Reading is tolerant: the older corridor type stored a surface's system as `{ "epsg": 32736,
/// "name": "WGS 84 / UTM zone 36S" }` with no `kind`. Such a value is read as the system the kernel
/// knows by that code (and keeps the stored name), or as a named-only system when it does not know
/// the code, so documents written before there was one coordinate system type still open.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(from = "ProjectCrsRaw")]
pub struct ProjectCrs {
    #[serde(default)]
    pub epsg: Option<u32>,
    pub name: String,
    pub kind: CrsKind,
    /// Free text, user input. Never interpreted.
    #[serde(default)]
    pub horizontal_datum: String,
    /// Free text, user input (for example the name of a levelling datum). Never interpreted.
    #[serde(default)]
    pub vertical_datum: String,
    #[serde(default)]
    pub linear_unit: LinearUnit,
    /// The WKT text as it came, carried verbatim.
    #[serde(default)]
    pub wkt: Option<String>,
    #[serde(default)]
    pub source: CrsSource,
}

#[derive(Deserialize)]
struct ProjectCrsRaw {
    #[serde(default)]
    epsg: Option<u32>,
    #[serde(default)]
    name: String,
    #[serde(default)]
    kind: Option<CrsKind>,
    #[serde(default)]
    horizontal_datum: String,
    #[serde(default)]
    vertical_datum: String,
    #[serde(default)]
    linear_unit: LinearUnit,
    #[serde(default)]
    wkt: Option<String>,
    #[serde(default)]
    source: CrsSource,
}

impl From<ProjectCrsRaw> for ProjectCrs {
    fn from(r: ProjectCrsRaw) -> ProjectCrs {
        let kind = r.kind.unwrap_or_else(|| if r.epsg.and_then(definition).is_some() { CrsKind::Projected } else { CrsKind::NamedOnly });
        let mut crs = ProjectCrs {
            epsg: r.epsg,
            name: r.name,
            kind,
            horizontal_datum: r.horizontal_datum,
            vertical_datum: r.vertical_datum,
            linear_unit: r.linear_unit,
            wkt: r.wkt,
            source: r.source,
        };
        if crs.kind == CrsKind::Projected && crs.horizontal_datum.is_empty() {
            crs.horizontal_datum = crs.epsg.and_then(definition).map(|d| d.datum.to_string()).unwrap_or_default();
        }
        crs
    }
}

/// One projected system the kernel can compute with.
struct Definition {
    name: String,
    datum: &'static str,
    /// EPSG code of the base geographic CRS (the datum the projection sits on).
    geog: u32,
    /// A UTM system (6 degree zone numbering applies).
    utm: bool,
    projection: TransverseMercator,
    area: AreaOfUse,
}

/// Other transverse Mercator systems. Every row was fetched from `https://epsg.io/<code>.json`
/// on 2026-11-16 (name, area of use, method code 9807 or 9808, longitude of natural origin, scale,
/// false easting, false northing, base CRS), plus `https://epsg.io/<code>.wkt` for the Adindan
/// and Arc 1950 ellipsoids. Rows: (EPSG, name, datum, base geographic CRS, ellipsoid, central
/// meridian, scale, false easting, false northing, south oriented, UTM, south, west, north, east).
type OtherRow = (u32, &'static str, &'static str, u32, Ellipsoid, f64, f64, f64, f64, bool, bool, f64, f64, f64, f64);

const fn utm_row(epsg: u32, name: &'static str, datum: &'static str, geog: u32, ell: Ellipsoid, lon0: f64, fnorth: f64, a: [f64; 4]) -> OtherRow {
    (epsg, name, datum, geog, ell, lon0, 0.9996, 500_000.0, fnorth, false, true, a[0], a[1], a[2], a[3])
}

/// Hartebeesthoek94 / Lo (south orientated, WGS 84 ellipsoid, scale 1, no false origin).
const fn lo_row(epsg: u32, name: &'static str, lon0: f64, a: [f64; 4]) -> OtherRow {
    (epsg, name, "Hartebeesthoek94", 4148, WGS84, lon0, 1.0, 0.0, 0.0, true, false, a[0], a[1], a[2], a[3])
}

const OTHER_PROJECTED: [OtherRow; 17] = [
    utm_row(20135, "Adindan / UTM zone 35N", "Adindan", 4201, CLARKE_1880_RGS, 27.0, 0.0, [4.21, 23.99, 22.01, 30.01]),
    utm_row(20136, "Adindan / UTM zone 36N", "Adindan", 4201, CLARKE_1880_RGS, 33.0, 0.0, [3.49, 29.99, 22.24, 36.0]),
    utm_row(20137, "Adindan / UTM zone 37N", "Adindan", 4201, CLARKE_1880_RGS, 39.0, 0.0, [3.4, 36.0, 22.01, 42.0]),
    utm_row(20138, "Adindan / UTM zone 38N", "Adindan", 4201, CLARKE_1880_RGS, 45.0, 0.0, [4.11, 42.0, 12.85, 47.99]),
    utm_row(20934, "Arc 1950 / UTM zone 34S", "Arc 1950", 4209, CLARKE_1880_ARC, 21.0, 10_000_000.0, [-26.88, 19.99, -10.86, 24.0]),
    utm_row(20935, "Arc 1950 / UTM zone 35S", "Arc 1950", 4209, CLARKE_1880_ARC, 27.0, 10_000_000.0, [-25.84, 24.0, -8.31, 30.0]),
    utm_row(20936, "Arc 1950 / UTM zone 36S", "Arc 1950", 4209, CLARKE_1880_ARC, 33.0, 10_000_000.0, [-22.42, 30.0, -8.19, 35.93]),
    lo_row(2046, "Hartebeesthoek94 / Lo15", 15.0, [-23.15, 14.35, -22.68, 14.6]),
    lo_row(2047, "Hartebeesthoek94 / Lo17", 17.0, [-33.1, 16.45, -28.03, 18.0]),
    lo_row(2048, "Hartebeesthoek94 / Lo19", 19.0, [-34.88, 17.99, -28.38, 20.0]),
    lo_row(2049, "Hartebeesthoek94 / Lo21", 21.0, [-34.88, 19.99, -24.76, 22.01]),
    lo_row(2050, "Hartebeesthoek94 / Lo23", 23.0, [-34.26, 22.0, -25.26, 24.01]),
    lo_row(2051, "Hartebeesthoek94 / Lo25", 25.0, [-34.26, 24.0, -24.71, 26.01]),
    lo_row(2052, "Hartebeesthoek94 / Lo27", 27.0, [-33.83, 26.0, -22.92, 28.0]),
    lo_row(2053, "Hartebeesthoek94 / Lo29", 29.0, [-33.03, 27.99, -22.13, 30.01]),
    lo_row(2054, "Hartebeesthoek94 / Lo31", 31.0, [-31.38, 29.99, -22.22, 32.02]),
    lo_row(2055, "Hartebeesthoek94 / Lo33", 33.0, [-28.94, 31.95, -26.8, 32.95]),
];

/// Arc 1960 / UTM rows: (EPSG, name, central meridian, false northing, south, west, north, east).
/// Every number fetched from `https://epsg.io/<code>.wkt` and `https://epsg.io/<code>.json`.
#[allow(clippy::type_complexity)]
const ARC_1960_UTM: [(u32, &str, f64, f64, f64, f64, f64, f64); 6] = [
    (21035, "Arc 1960 / UTM zone 35S", 27.0, 10_000_000.0, -6.81, 29.34, 0.0, 30.0),
    (21036, "Arc 1960 / UTM zone 36S", 33.0, 10_000_000.0, -11.61, 29.99, 0.01, 36.0),
    (21037, "Arc 1960 / UTM zone 37S", 39.0, 10_000_000.0, -11.75, 36.0, 0.0, 41.6),
    (21095, "Arc 1960 / UTM zone 35N", 27.0, 0.0, 0.0, 29.71, 0.86, 30.0),
    (21096, "Arc 1960 / UTM zone 36N", 33.0, 0.0, 0.0, 29.99, 4.63, 36.0),
    (21097, "Arc 1960 / UTM zone 37N", 39.0, 0.0, 0.0, 36.0, 4.49, 41.91),
];

fn definition(epsg: u32) -> Option<Definition> {
    match epsg {
        32601..=32660 | 32701..=32760 => {
            let south = epsg >= 32701;
            let zone = epsg - if south { 32700 } else { 32600 };
            let lon0 = f64::from(zone) * 6.0 - 183.0;
            Some(Definition {
                name: format!("WGS 84 / UTM zone {zone}{}", if south { 'S' } else { 'N' }),
                datum: "WGS 84",
                geog: 4326,
                utm: true,
                projection: TransverseMercator::new(WGS84, lon0, 0.9996, 500_000.0, if south { 10_000_000.0 } else { 0.0 }),
                area: AreaOfUse { south: if south { -80.0 } else { 0.0 }, west: lon0 - 3.0, north: if south { 0.0 } else { 84.0 }, east: lon0 + 3.0 },
            })
        }
        _ => ARC_1960_UTM
            .iter()
            .find(|r| r.0 == epsg)
            .map(|&(_, name, lon0, fnorth, s, w, n, e)| Definition {
                name: name.to_string(),
                datum: "Arc 1960",
                geog: 4210,
                utm: true,
                projection: TransverseMercator::new(CLARKE_1880_RGS, lon0, 0.9996, 500_000.0, fnorth),
                area: AreaOfUse { south: s, west: w, north: n, east: e },
            })
            .or_else(|| {
                OTHER_PROJECTED.iter().find(|r| r.0 == epsg).map(|&(_, name, datum, geog, ell, lon0, k0, fe, fnorth, south_oriented, utm, s, w, n, e)| Definition {
                    name: name.to_string(),
                    datum,
                    geog,
                    utm,
                    projection: TransverseMercator::new_with_origin(ell, 0.0, lon0, k0, fe, fnorth, south_oriented),
                    area: AreaOfUse { south: s, west: w, north: n, east: e },
                })
            }),
    }
}

/// Every EPSG code the kernel can project, in order.
pub fn supported_epsg_codes() -> Vec<u32> {
    let mut v: Vec<u32> = (32601..=32660).chain(32701..=32760).collect();
    v.extend(ARC_1960_UTM.iter().map(|r| r.0));
    v.extend(OTHER_PROJECTED.iter().map(|r| r.0));
    v
}

fn compact(text: &str) -> String {
    text.chars().filter(|c| c.is_ascii_alphanumeric()).collect::<String>().to_ascii_uppercase()
}

impl ProjectCrs {
    /// A projected system the kernel knows, by EPSG code.
    pub fn from_epsg(epsg: u32) -> Option<ProjectCrs> {
        let d = definition(epsg)?;
        Some(ProjectCrs {
            epsg: Some(epsg),
            name: d.name,
            kind: CrsKind::Projected,
            horizontal_datum: d.datum.to_string(),
            vertical_datum: String::new(),
            linear_unit: LinearUnit::Metre,
            wkt: None,
            source: CrsSource::Typed,
        })
    }

    /// WGS 84 / UTM zone `zone` (1 to 60), north or south of the equator.
    pub fn utm_wgs84(zone: u8, south: bool) -> Option<ProjectCrs> {
        if !(1..=60).contains(&zone) {
            return None;
        }
        ProjectCrs::from_epsg(if south { 32700 } else { 32600 } + u32::from(zone))
    }

    /// The systems whose code or name contains `query` (case, spaces and punctuation ignored),
    /// at most `limit`, in code order. An empty query matches nothing. A bare number matches the
    /// codes that start with it.
    pub fn search(query: &str, limit: usize) -> Vec<ProjectCrs> {
        let q = compact(query);
        if q.is_empty() {
            return Vec::new();
        }
        let digits = q.strip_prefix("EPSG").unwrap_or(&q);
        let numeric = !digits.is_empty() && digits.chars().all(|c| c.is_ascii_digit());
        let key = |name: &str| compact(name).replace("ZONE", "");
        let q_key = q.replace("ZONE", "");
        supported_epsg_codes()
            .into_iter()
            .filter(|&code| {
                if numeric {
                    code.to_string().starts_with(digits)
                } else {
                    definition(code).is_some_and(|d| key(&d.name).contains(&q_key))
                }
            })
            .take(limit)
            .filter_map(ProjectCrs::from_epsg)
            .collect()
    }

    /// What is odd about plan coordinates said to be in this system, as sentences; empty when
    /// nothing is (and for a system the kernel cannot compute with). `min` and `max` are the
    /// south-west and north-east corners `(easting, northing)` in metres.
    ///
    /// The tests, for a UTM system: eastings between 100 000 m and 900 000 m (a zone is at most
    /// about 668 km wide, centred on 500 000 m), northings between 0 and 10 000 000 m, and the
    /// extent within three degrees of the zone's central meridian and on the system's side of the
    /// equator. See [`CrsCheck`] for the fuller check an import runs.
    pub fn doubts(&self, min: (f64, f64), max: (f64, f64)) -> Vec<String> {
        let mut out = Vec::new();
        let (Some((zone, south)), Some(projection)) = (self.utm_zone(), self.projection()) else { return out };
        if min.0 < 100_000.0 || max.0 > 900_000.0 {
            out.push(format!(
                "Eastings run from {:.3} to {:.3} m. A UTM easting lies between about 166 000 and 834 000 m, so these are not eastings of {}: check the column order.",
                min.0, max.0, self.name
            ));
        }
        if min.1 < 0.0 || max.1 > 10_000_000.0 {
            out.push(format!(
                "Northings run from {:.3} to {:.3} m. A UTM northing lies between 0 and 10 000 000 m, so these are not northings of {}: check the column order.",
                min.1, max.1, self.name
            ));
        }
        if !out.is_empty() {
            return out;
        }
        let central = projection.lon0.to_degrees();
        for (e, n) in [min, max] {
            if let Some((lat, lon)) = self.to_geographic(e, n) {
                if (lon - central).abs() > 3.0 + 1e-9 {
                    out.push(format!(
                        "The extent reaches longitude {lon:.4}, which is outside zone {zone} ({:.0} to {:.0} degrees east). It may belong to a neighbouring zone.",
                        central - 3.0,
                        central + 3.0
                    ));
                    break;
                }
                if south != (lat < 0.0) && lat.abs() > 0.5 {
                    out.push(format!("The extent reaches latitude {lat:.4}, which is not in the {} hemisphere.", if south { "southern" } else { "northern" }));
                    break;
                }
            }
        }
        out
    }

    /// A system known by a code or a name only. Nothing can be computed from it.
    pub fn named_only(epsg: Option<u32>, name: &str) -> ProjectCrs {
        ProjectCrs {
            epsg,
            name: name.to_string(),
            kind: CrsKind::NamedOnly,
            horizontal_datum: String::new(),
            vertical_datum: String::new(),
            linear_unit: LinearUnit::Metre,
            wkt: None,
            source: CrsSource::Typed,
        }
    }

    /// A local site grid: the numbers have no geodetic meaning.
    pub fn local_grid(name: &str) -> ProjectCrs {
        ProjectCrs { kind: CrsKind::LocalGrid, ..ProjectCrs::named_only(None, name) }
    }

    /// From what a person types: `EPSG:32636`, `32636`, `WGS 84 / UTM zone 36N`, `UTM 36N` (read
    /// as WGS 84), `Arc 1960 / UTM zone 36S`, `arc1960 utm36s`. Case, spaces and punctuation do
    /// not matter. A zone with no `N` or `S` is not accepted: the hemisphere changes the northing
    /// by ten thousand kilometres. A well-formed EPSG code the kernel cannot project gives a
    /// named-only system with no name beyond the code; other text gives `None`.
    pub fn parse(text: &str) -> Option<ProjectCrs> {
        let c = compact(text);
        if c.is_empty() {
            return None;
        }
        let digits = c.strip_prefix("EPSG").unwrap_or(&c);
        if !digits.is_empty() && digits.chars().all(|ch| ch.is_ascii_digit()) {
            let code: u32 = digits.parse().ok()?;
            return Some(ProjectCrs::from_epsg(code).unwrap_or_else(|| ProjectCrs::named_only(Some(code), &format!("EPSG:{code}"))));
        }
        let mut c = c.replace("ZONE", "").replace("SOUTH", "S").replace("NORTH", "N");
        if c.starts_with("UTM") {
            c.insert_str(0, "WGS84");
        }
        let key = |name: &str| compact(name).replace("ZONE", "");
        supported_epsg_codes().into_iter().find(|&code| definition(code).is_some_and(|d| key(&d.name) == c)).and_then(ProjectCrs::from_epsg)
    }

    /// A tolerant reader for WKT (WKT 1 or 2): the name of the root node, the EPSG authority code
    /// of the root node (not of its base system), the horizontal datum name, the unit name, and the
    /// text verbatim. When the code is one the kernel can project, the system is [`CrsKind::Projected`].
    /// A `LOCAL_CS` or engineering CRS is a [`CrsKind::LocalGrid`]. `None` when no node can be read.
    pub fn from_wkt(text: &str) -> Option<ProjectCrs> {
        let root = wkt_parse(text)?;
        let name = root.strings.first()?.clone();
        let keyword = root.keyword.to_ascii_uppercase();
        let epsg = root
            .children
            .iter()
            .find(|c| matches!(c.keyword.to_ascii_uppercase().as_str(), "AUTHORITY" | "ID") && c.strings.first().is_some_and(|s| s.eq_ignore_ascii_case("EPSG")))
            .and_then(|c| c.numbers.first().copied().or_else(|| c.strings.get(1).and_then(|s| s.trim().parse::<f64>().ok())))
            .map(|v| v as u32);
        let datum = root.find("DATUM").or_else(|| root.find("ENSEMBLE")).and_then(|d| d.strings.first().cloned()).unwrap_or_default();
        let mut crs = match epsg.and_then(ProjectCrs::from_epsg) {
            Some(known) => known,
            None => {
                let local = keyword.starts_with("LOCAL") || keyword == "ENGCRS" || keyword == "ENGINEERINGCRS";
                ProjectCrs { kind: if local { CrsKind::LocalGrid } else { CrsKind::NamedOnly }, ..ProjectCrs::named_only(epsg, &name) }
            }
        };
        // Keep the file's own name when it differs from the registry name.
        if crs.kind != CrsKind::Projected || crs.name != name {
            crs.name = name;
        }
        if crs.horizontal_datum.is_empty() {
            crs.horizontal_datum = datum;
        }
        crs.wkt = Some(text.to_string());
        Some(crs)
    }

    /// `EPSG:32736`, when there is a code.
    pub fn code(&self) -> Option<String> {
        self.epsg.map(|c| format!("EPSG:{c}"))
    }

    /// `WGS 84 / UTM zone 36S (EPSG:32736)`.
    pub fn label(&self) -> String {
        match self.code() {
            Some(c) => format!("{} ({c})", self.name),
            None => self.name.clone(),
        }
    }

    fn def(&self) -> Option<Definition> {
        if self.kind != CrsKind::Projected {
            return None;
        }
        definition(self.epsg?)
    }

    /// The projection, when the kernel can compute with this system.
    pub fn projection(&self) -> Option<TransverseMercator> {
        self.def().map(|d| d.projection)
    }

    /// The EPSG area of use, when known.
    pub fn area_of_use(&self) -> Option<AreaOfUse> {
        self.def().map(|d| d.area)
    }

    /// EPSG code of the geographic CRS (the datum) this system is on, for a projected system the
    /// kernel can compute with and for a geographic system in [`crate::crs_transform::geographic_crs`].
    /// Local grids and unknown codes give `None`.
    pub fn geographic_epsg(&self) -> Option<u32> {
        if self.kind == CrsKind::LocalGrid {
            return None;
        }
        let code = self.epsg?;
        if let Some(d) = definition(code) {
            return Some(d.geog);
        }
        crate::crs_transform::geographic_crs(code).map(|g| g.epsg)
    }

    /// The UTM zone and whether it is the southern one, for the WGS 84 and Arc 1960 UTM systems.
    pub fn utm_zone(&self) -> Option<(u8, bool)> {
        let d = self.def()?;
        if !d.utm {
            return None;
        }
        let lon0 = d.projection.lon0.to_degrees();
        let zone = ((lon0 + 183.0) / 6.0).round() as u8;
        Some((zone, d.projection.false_northing > 0.0))
    }

    /// Latitude and longitude in degrees, on the datum of this system, of an easting and
    /// northing in metres.
    pub fn to_geographic(&self, easting_m: f64, northing_m: f64) -> Option<(f64, f64)> {
        self.projection()?.inverse(easting_m, northing_m)
    }

    /// Easting and northing in metres of a latitude and longitude in degrees on the datum of this system.
    pub fn from_geographic(&self, lat_deg: f64, lon_deg: f64) -> Option<(f64, f64)> {
        self.projection()?.forward(lat_deg, lon_deg)
    }

    /// Point scale factor and grid convergence (degrees) at an easting and northing in metres.
    pub fn scale_and_convergence_at(&self, easting_m: f64, northing_m: f64) -> Option<(f64, f64)> {
        let p = self.projection()?;
        let (lat, lon) = p.inverse(easting_m, northing_m)?;
        p.scale_and_convergence(lat, lon)
    }
}

// ---------------------------------------------------------------------------------------------
// WKT reading

#[derive(Debug, Default)]
struct WktNode {
    keyword: String,
    strings: Vec<String>,
    numbers: Vec<f64>,
    children: Vec<WktNode>,
}

impl WktNode {
    /// The first descendant with this keyword (depth first).
    fn find(&self, keyword: &str) -> Option<&WktNode> {
        self.children.iter().find_map(|c| if c.keyword.eq_ignore_ascii_case(keyword) { Some(c) } else { c.find(keyword) })
    }
}

fn wkt_parse(text: &str) -> Option<WktNode> {
    let chars: Vec<char> = text.chars().collect();
    let mut i = 0;
    let node = wkt_node(&chars, &mut i)?;
    Some(node)
}

fn wkt_skip_ws(c: &[char], i: &mut usize) {
    while *i < c.len() && (c[*i].is_whitespace() || c[*i] == ',') {
        *i += 1;
    }
}

fn wkt_node(c: &[char], i: &mut usize) -> Option<WktNode> {
    wkt_skip_ws(c, i);
    let start = *i;
    while *i < c.len() && (c[*i].is_ascii_alphanumeric() || c[*i] == '_') {
        *i += 1;
    }
    if *i == start {
        return None;
    }
    let mut node = WktNode { keyword: c[start..*i].iter().collect(), ..WktNode::default() };
    wkt_skip_ws(c, i);
    if *i >= c.len() || (c[*i] != '[' && c[*i] != '(') {
        return Some(node);
    }
    *i += 1;
    loop {
        wkt_skip_ws(c, i);
        let ch = *c.get(*i)?;
        match ch {
            ']' | ')' => {
                *i += 1;
                return Some(node);
            }
            '"' => {
                *i += 1;
                let s = *i;
                while *i < c.len() && c[*i] != '"' {
                    *i += 1;
                }
                let end = (*i).min(c.len());
                node.strings.push(c[s..end].iter().collect());
                *i += 1;
            }
            _ => {
                let s = *i;
                while *i < c.len() && !matches!(c[*i], ',' | ']' | ')' | '[' | '(') && !c[*i].is_whitespace() {
                    *i += 1;
                }
                let word: String = c[s..*i].iter().collect();
                wkt_skip_ws(c, i);
                if *i < c.len() && (c[*i] == '[' || c[*i] == '(') {
                    *i = s;
                    node.children.push(wkt_node(c, i)?);
                } else if let Ok(v) = word.parse::<f64>() {
                    node.numbers.push(v);
                } else if word.is_empty() {
                    return None;
                }
            }
        }
    }
}

// ---------------------------------------------------------------------------------------------
// Units of position: model millimetres and CRS metres

/// Model space is millimetres, as the kernel has always had it. CRS coordinates are metres.
pub const MM_PER_M: f64 = 1000.0;

/// Metres to model millimetres. Use this instead of writing `* 1000.0`.
pub fn m_to_mm(metres: f64) -> f64 {
    metres * MM_PER_M
}

/// Model millimetres to metres.
pub fn mm_to_m(millimetres: f64) -> f64 {
    millimetres / MM_PER_M
}

/// The model point (millimetres) of an easting and northing in CRS metres. Nothing is shifted:
/// model space carries the source coordinates, so a UTM northing of 9 885 072.123 m is
/// 9 885 072 123 mm. f64 holds that to better than a micrometre (the spacing of f64 at 1e10 is
/// 1.9e-6 mm).
pub fn model_point_from_crs(easting_m: f64, northing_m: f64) -> crate::Pt {
    crate::Pt::new(m_to_mm(easting_m), m_to_mm(northing_m))
}

/// The easting and northing in CRS metres of a model point (millimetres).
pub fn crs_from_model_point(p: crate::Pt) -> (f64, f64) {
    (mm_to_m(p.x), mm_to_m(p.y))
}

/// A height in CRS metres to model millimetres (elevations are kept as given, no datum shift).
pub fn elevation_m_to_mm(metres: f64) -> f64 {
    m_to_mm(metres)
}

/// A model height in millimetres to metres.
pub fn elevation_mm_to_m(millimetres: f64) -> f64 {
    mm_to_m(millimetres)
}

// ---------------------------------------------------------------------------------------------
// Checking an incoming dataset

/// The plan extent of a dataset as its file states it: the numbers in the file, not converted.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct PlanExtent {
    pub min_easting: f64,
    pub min_northing: f64,
    pub max_easting: f64,
    pub max_northing: f64,
}

/// What the importer knows about an incoming dataset.
#[derive(Debug, Clone, PartialEq)]
pub struct IncomingDataset {
    /// A name for sentences, for example the file name.
    pub label: String,
    /// The coordinate system the file declares, when it does.
    pub crs: Option<ProjectCrs>,
    pub extent: Option<PlanExtent>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FindingKind {
    SameSystem,
    DifferentSystem,
    Undeclared,
    OutsideValidArea,
    LikelySwapped,
    LikelyMillimetres,
    LikelyFeet,
    CannotCheck,
}

/// One result of a check. `message` is a plain sentence for the user.
#[derive(Debug, Clone, PartialEq)]
pub struct CrsFinding {
    pub kind: FindingKind,
    /// True when the user should look before importing.
    pub warning: bool,
    pub message: String,
}

/// The result of checking a dataset against the project coordinate system. No reprojection is
/// done and none is offered: objects keep their source coordinates.
#[derive(Debug, Clone, PartialEq)]
pub struct CrsCheck {
    pub findings: Vec<CrsFinding>,
}

/// Factor for the international foot: 0.3048 m exactly, by definition (1959 international yard
/// and pound agreement). A US survey foot differs by 2 parts per million, far below what this
/// heuristic can tell apart.
const FOOT_M: f64 = 0.3048;

fn same_system(a: &ProjectCrs, b: &ProjectCrs) -> bool {
    match (a.epsg, b.epsg) {
        (Some(x), Some(y)) => x == y,
        _ => compact(&a.name) == compact(&b.name) && a.kind == b.kind,
    }
}

/// Does the extent, read as metres of `crs` after multiplying by `factor` (and swapping if
/// asked), lie inside the area of use?
fn extent_fits(crs: &ProjectCrs, ext: &PlanExtent, factor: f64, swap: bool) -> Option<bool> {
    let proj = crs.projection()?;
    let area = crs.area_of_use()?;
    let (e0, e1, n0, n1) = if swap {
        (ext.min_northing, ext.max_northing, ext.min_easting, ext.max_easting)
    } else {
        (ext.min_easting, ext.max_easting, ext.min_northing, ext.max_northing)
    };
    let corners = [(e0, n0), (e0, n1), (e1, n0), (e1, n1), ((e0 + e1) / 2.0, (n0 + n1) / 2.0)];
    Some(corners.iter().all(|&(e, n)| proj.inverse(e * factor, n * factor).is_some_and(|(lat, lon)| area.contains(lat, lon))))
}

impl CrsCheck {
    /// Checks `incoming` against the `project` coordinate system. Unit, swap and area checks use
    /// the dataset's own declared system when it declares one the kernel can project, else the
    /// project's.
    pub fn run(project: &ProjectCrs, incoming: &IncomingDataset) -> CrsCheck {
        let mut findings = Vec::new();
        let mut push = |kind, warning, message: String| findings.push(CrsFinding { kind, warning, message });
        let label = &incoming.label;

        match &incoming.crs {
            None => push(
                FindingKind::Undeclared,
                true,
                format!(
                    "{label} does not say which coordinate system it uses. Its coordinates will be taken as they are, as if they were in {}. Check this against the survey before relying on the position.",
                    project.label()
                ),
            ),
            Some(c) if same_system(project, c) => push(FindingKind::SameSystem, false, format!("{label} is in {}, the same system as the project.", c.label())),
            Some(c) => push(
                FindingKind::DifferentSystem,
                true,
                format!(
                    "{label} is in {}, but the project is in {}. These are different systems. Nothing is reprojected: objects keep the coordinates of their source, so {label} will not line up with the rest of the project until it is converted in a survey tool.",
                    c.label(),
                    project.label()
                ),
            ),
        }

        let basis = incoming.crs.as_ref().filter(|c| c.projection().is_some()).unwrap_or(project);
        if let Some(ext) = &incoming.extent {
            if basis.kind == CrsKind::LocalGrid {
                push(FindingKind::CannotCheck, false, format!("{} is a local site grid. Its coordinates have no geodetic meaning, so the extent of {label} cannot be tested against the Earth.", basis.label()));
            } else if let Some(ok) = extent_fits(basis, ext, 1.0, false) {
                if !ok {
                    let area = basis.area_of_use().expect("a projectable system has an area");
                    let sentence = |what: &str| format!("{what} If that is right, say so and import; otherwise check the file.");
                    if extent_fits(basis, ext, 1.0, true) == Some(true) {
                        push(
                            FindingKind::LikelySwapped,
                            true,
                            sentence(&format!("The extent of {label} falls inside {} only if easting and northing are swapped: the first column looks like the northing.", basis.label())),
                        );
                    } else if extent_fits(basis, ext, 0.001, false) == Some(true) {
                        push(
                            FindingKind::LikelyMillimetres,
                            true,
                            sentence(&format!("The values of {label} look like millimetres: divided by 1000 they fall inside {}.", basis.label())),
                        );
                    } else if extent_fits(basis, ext, FOOT_M, false) == Some(true) {
                        push(
                            FindingKind::LikelyFeet,
                            true,
                            sentence(&format!("The values of {label} look like feet: multiplied by 0.3048 they fall inside {}.", basis.label())),
                        );
                    } else {
                        push(
                            FindingKind::OutsideValidArea,
                            true,
                            format!(
                                "The extent of {label} (easting {:.3} to {:.3}, northing {:.3} to {:.3}, as given) is outside the area where {} is defined ({:.2} to {:.2} degrees north, {:.2} to {:.2} degrees east, the EPSG area of use). It may belong to another zone or system.",
                                ext.min_easting, ext.max_easting, ext.min_northing, ext.max_northing, basis.label(), area.south, area.north, area.west, area.east
                            ),
                        );
                    }
                }
            } else {
                push(
                    FindingKind::CannotCheck,
                    false,
                    format!("The extent of {label} cannot be tested: the kernel has no projection for {}. It is known by name only.", basis.label()),
                );
            }
        }
        CrsCheck { findings }
    }

    /// True when nothing needs the user's attention.
    pub fn is_clean(&self) -> bool {
        !self.findings.iter().any(|f| f.warning)
    }

    pub fn has(&self, kind: FindingKind) -> bool {
        self.findings.iter().any(|f| f.kind == kind)
    }

    pub fn messages(&self) -> Vec<&str> {
        self.findings.iter().map(|f| f.message.as_str()).collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn utm36s() -> ProjectCrs {
        ProjectCrs::from_epsg(32736).unwrap()
    }

    #[test]
    fn systems_have_their_codes_and_names() {
        let c = utm36s();
        assert_eq!((c.epsg, c.name.as_str(), c.kind), (Some(32736), "WGS 84 / UTM zone 36S", CrsKind::Projected));
        assert_eq!(c.label(), "WGS 84 / UTM zone 36S (EPSG:32736)");
        assert_eq!(c.utm_zone(), Some((36, true)));
        assert_eq!(ProjectCrs::from_epsg(32601).unwrap().name, "WGS 84 / UTM zone 1N");
        assert_eq!(ProjectCrs::from_epsg(32760).unwrap().name, "WGS 84 / UTM zone 60S");
        assert_eq!(ProjectCrs::from_epsg(21096).unwrap().name, "Arc 1960 / UTM zone 36N");
        assert_eq!(ProjectCrs::from_epsg(21037).unwrap().utm_zone(), Some((37, true)));
        assert!(ProjectCrs::from_epsg(4326).is_none());
        assert_eq!(supported_epsg_codes().len(), 120 + 6 + 17);
    }

    #[test]
    fn typed_names_are_understood() {
        for text in ["EPSG:32736", "epsg 32736", "32736", "WGS 84 / UTM zone 36S", "WGS 84/ UTM36S", "wgs84 utm 36 s", "UTM36S", "UTM zone 36 South"] {
            assert_eq!(ProjectCrs::parse(text), Some(utm36s()), "{text}");
        }
        assert_eq!(ProjectCrs::parse("UTM 36N").unwrap().epsg, Some(32636));
        assert_eq!(ProjectCrs::parse("Arc 1960 / UTM zone 36S").unwrap().epsg, Some(21036));
        assert_eq!(ProjectCrs::parse("arc1960 utm 37n").unwrap().epsg, Some(21097));
        assert_eq!(ProjectCrs::parse("EPSG:21096").unwrap().kind, CrsKind::Projected);
        // A code the kernel cannot project is kept by name only.
        let wgs = ProjectCrs::parse("EPSG:4326").unwrap();
        assert_eq!((wgs.kind, wgs.epsg), (CrsKind::NamedOnly, Some(4326)));
        for text in ["UTM 36", "Arc 1960 / UTM zone 34S", "UTM 61S", "", "metres", "Clarke UTM 36S"] {
            assert_eq!(ProjectCrs::parse(text), None, "{text}");
        }
    }

    // The WKT as fetched from https://epsg.io/21096.wkt.
    const WKT_21096: &str = r#"PROJCS["Arc 1960 / UTM zone 36N",GEOGCS["Arc 1960",DATUM["Arc_1960",SPHEROID["Clarke 1880 (RGS)",6378249.145,293.465],TOWGS84[-160,-6,-302,0,0,0,0]],PRIMEM["Greenwich",0,AUTHORITY["EPSG","8901"]],UNIT["degree",0.0174532925199433,AUTHORITY["EPSG","9122"]],AUTHORITY["EPSG","4210"]],PROJECTION["Transverse_Mercator"],PARAMETER["latitude_of_origin",0],PARAMETER["central_meridian",33],PARAMETER["scale_factor",0.9996],PARAMETER["false_easting",500000],PARAMETER["false_northing",0],UNIT["metre",1,AUTHORITY["EPSG","9001"]],AXIS["Easting",EAST],AXIS["Northing",NORTH],AUTHORITY["EPSG","21096"]]"#;

    #[test]
    fn wkt_gives_the_root_name_and_code_not_the_base_systems() {
        let c = ProjectCrs::from_wkt(WKT_21096).unwrap();
        assert_eq!((c.name.as_str(), c.epsg, c.kind), ("Arc 1960 / UTM zone 36N", Some(21096), CrsKind::Projected));
        assert_eq!(c.horizontal_datum, "Arc 1960");
        assert_eq!(c.wkt.as_deref(), Some(WKT_21096));
        // WKT 2 with ID, an unknown system, a local system, and junk.
        let w2 = r#"PROJCRS["Some grid",BASEGEOGCRS["X",DATUM["Some datum",ELLIPSOID["E",6378137,298.25,LENGTHUNIT["metre",1]]],ID["EPSG",4326]],CONVERSION["c",METHOD["Transverse Mercator"]],ID["EPSG",3857]]"#;
        let c = ProjectCrs::from_wkt(w2).unwrap();
        assert_eq!((c.name.as_str(), c.epsg, c.kind), ("Some grid", Some(3857), CrsKind::NamedOnly));
        assert_eq!(c.horizontal_datum, "Some datum");
        let local = ProjectCrs::from_wkt(r#"LOCAL_CS["Site grid",UNIT["metre",1]]"#).unwrap();
        assert_eq!((local.kind, local.epsg), (CrsKind::LocalGrid, None));
        assert!(ProjectCrs::from_wkt("not wkt at all").is_none() || ProjectCrs::from_wkt("   ").is_none());
        assert!(ProjectCrs::from_wkt("").is_none());
        // Truncated text does not panic.
        let _ = ProjectCrs::from_wkt(r#"PROJCS["Broken",GEOGCS["#);
    }

    fn assert_close(a: f64, b: f64, tol: f64, what: &str) {
        assert!((a - b).abs() <= tol, "{what}: {a} vs {b} (tol {tol})");
    }

    #[test]
    fn the_inverse_projection_matches_worked_values() {
        let c = utm36s();
        let (lat, lon) = c.to_geographic(500_000.0, 10_000_000.0).unwrap();
        assert!(lat.abs() < 1e-9 && (lon - 33.0).abs() < 1e-9, "{lat} {lon}");
        let (lat, lon) = c.to_geographic(500_000.0, 10_000_000.0 - 110_530.17).unwrap();
        assert!((lat + 1.0).abs() < 1e-6 && (lon - 33.0).abs() < 1e-9, "{lat} {lon}");
        let (lat, lon) = c.to_geographic(611_280.650, 10_000_000.0).unwrap();
        assert!(lat.abs() < 1e-9 && (lon - 34.0).abs() < 1e-6, "{lat} {lon}");
    }

    /// Meridian arc length by Simpson integration of a (1 - e^2) / (1 - e^2 sin^2 phi)^1.5, the
    /// definition of the radius of curvature in the meridian, independent of the series.
    fn meridian_arc(ell: &Ellipsoid, lat_deg: f64) -> f64 {
        let f = 1.0 / ell.inverse_flattening;
        let e2 = f * (2.0 - f);
        let steps = 2000;
        let top = lat_deg.to_radians();
        let h = top / steps as f64;
        let g = |p: f64| ell.a * (1.0 - e2) / (1.0 - e2 * p.sin().powi(2)).powf(1.5);
        let mut s = g(0.0) + g(top);
        for i in 1..steps {
            s += g(i as f64 * h) * if i % 2 == 1 { 4.0 } else { 2.0 };
        }
        s * h / 3.0
    }

    #[test]
    fn the_central_meridian_is_k0_times_the_meridian_arc() {
        for (ell, lon0) in [(WGS84, 33.0), (CLARKE_1880_RGS, 33.0)] {
            let tm = TransverseMercator::new(ell, lon0, 0.9996, 500_000.0, 0.0);
            for lat in [-11.0, -4.0, -0.5, 0.25, 1.0, 4.6, 10.0] {
                let (e, n) = tm.forward(lat, lon0).unwrap();
                assert_close(e, 500_000.0, 1e-6, "easting on the central meridian");
                assert_close(n, 0.9996 * meridian_arc(&ell, lat), 5e-4, &format!("{} northing at {lat}", ell.name));
            }
        }
    }

    #[test]
    fn one_degree_east_on_the_equator_matches_the_series_in_longitude() {
        // 0.9996 a (L + L^3 (1 + e'^2) / 6) with L one degree in radians, e'^2 = 0.006 739 497,
        // gives 111 280.650 m (the corridor crate's worked value, same derivation).
        let c = ProjectCrs::from_epsg(32636).unwrap();
        let (e, n) = c.from_geographic(0.0, 34.0).unwrap();
        assert_close(e, 611_280.650, 1e-3, "easting");
        assert_close(n, 0.0, 1e-6, "northing");
    }

    #[test]
    fn forward_and_inverse_round_trip_to_sub_millimetre_in_every_known_east_african_system() {
        let mut worst = 0.0f64;
        for code in [32636, 32736, 32635, 32735, 32637, 32737, 21035, 21036, 21037, 21095, 21096, 21097] {
            let c = ProjectCrs::from_epsg(code).unwrap();
            let area = c.area_of_use().unwrap();
            let lon0 = c.projection().unwrap().lon0.to_degrees();
            for i in 0..=10 {
                for j in 0..=10 {
                    let lat = area.south + (area.north.min(area.south + 15.0) - area.south) * f64::from(i) / 10.0;
                    let lon = (lon0 - 3.0) + 6.0 * f64::from(j) / 10.0;
                    let (e, n) = c.from_geographic(lat, lon).unwrap();
                    let (lat2, lon2) = c.to_geographic(e, n).unwrap();
                    // Compare in metres on the ground: 1 degree of latitude is about 110 km.
                    let dn = (lat2 - lat) * 110_574.0;
                    let de = (lon2 - lon) * 111_320.0 * lat.to_radians().cos();
                    worst = worst.max(dn.abs()).max(de.abs());
                    // And forward again: the grid position must come back.
                    let (e2, n2) = c.from_geographic(lat2, lon2).unwrap();
                    assert!((e2 - e).abs() < 1e-3 && (n2 - n).abs() < 1e-3, "{code} {lat} {lon}: {} {}", e2 - e, n2 - n);
                }
            }
        }
        assert!(worst < 1e-3, "worst round trip {worst} m");
    }

    #[test]
    fn scale_and_convergence_match_finite_differences() {
        for (code, lat, lon) in [(32736, -3.3, 35.1), (32636, 2.5, 34.2), (21096, 2.5, 34.2), (21036, -1.3, 31.0), (32736, -10.0, 30.5)] {
            let c = ProjectCrs::from_epsg(code).unwrap();
            let p = c.projection().unwrap();
            let (k, gamma) = p.scale_and_convergence(lat, lon).unwrap();
            let d = 1e-6; // degrees, about 0.1 m
            // Scale: grid distance over ellipsoidal distance along the meridian and the parallel.
            let (e0, n0) = p.forward(lat, lon).unwrap();
            let (e1, n1) = p.forward(lat + d, lon).unwrap();
            let (e2, n2) = p.forward(lat, lon + d).unwrap();
            let ell = p.ellipsoid();
            let f = 1.0 / ell.inverse_flattening;
            let e2s = f * (2.0 - f);
            let w = (1.0 - e2s * lat.to_radians().sin().powi(2)).sqrt();
            let m_radius = ell.a * (1.0 - e2s) / w.powi(3);
            let n_radius = ell.a / w;
            let ground_n = m_radius * d.to_radians();
            let ground_e = n_radius * lat.to_radians().cos() * d.to_radians();
            let k_meridian = ((e1 - e0).hypot(n1 - n0)) / ground_n;
            let k_parallel = ((e2 - e0).hypot(n2 - n0)) / ground_e;
            assert_close(k, k_meridian, 2e-8, &format!("{code} k vs meridian"));
            assert_close(k, k_parallel, 2e-8, &format!("{code} k vs parallel"));
            // Convergence: angle from grid north counterclockwise to true north.
            let fd = (-(e1 - e0)).atan2(n1 - n0).to_degrees();
            assert_close(gamma, fd, 2e-5, &format!("{code} gamma"));
        }
        // On the central meridian the convergence is zero and the scale is the minimum there.
        let c = utm36s();
        let (k, g) = c.scale_and_convergence_at(500_000.0, 9_000_000.0).unwrap();
        assert!(g.abs() < 1e-9 && (k - 0.9996).abs() < 1e-3);
        // East of the central meridian in the southern hemisphere the convergence is negative.
        let (_, g) = c.scale_and_convergence_at(650_000.0, 9_000_000.0).unwrap();
        assert!(g < 0.0, "{g}");
        let (_, g) = ProjectCrs::from_epsg(32636).unwrap().scale_and_convergence_at(650_000.0, 300_000.0).unwrap();
        assert!(g > 0.0, "{g}");
    }

    /// GN7-2 section 3.2.3.1, worked example: OSGB 1936 / British National Grid, JHS formulas, forward and reverse.
    /// The projection parameters and the Airy 1830 ellipsoid are the note's own; they test the general case
    /// (a latitude of natural origin, a false northing, a scale below one) that no East African system uses.
    #[test]
    fn gn7_2_transverse_mercator_example_british_national_grid() {
        let airy = Ellipsoid { name: "Airy 1830", a: 6_377_563.396, inverse_flattening: 299.324_96, source: "GN7-2 3.2.3.1 example" };
        let bng = TransverseMercator::new_with_origin(airy, 49.0, -2.0, 0.999_601_271_7, 400_000.0, -100_000.0, false);
        let (e, n) = bng.forward(50.5, 0.5).unwrap();
        // The note prints E 577 274.99 and N 69 740.50. Recomputing them from the note's own printed
        // intermediates (B, eta, xi, M0, k0) gives 577 274.984 and 69 740.492, so its last digit is rounded
        // from a computation with more digits than it prints; agreement is asked to 0.01 m.
        assert_close(e, 577_274.99, 0.01, "easting");
        assert_close(n, 69_740.50, 0.01, "northing");
        assert_close(e, 577_274.984, 0.002, "easting from the note's intermediates");
        assert_close(n, 69_740.492, 0.002, "northing from the note's intermediates");
        let (lat, lon) = bng.inverse(577_274.99, 69_740.50).unwrap();
        assert_close(lat, 50.5, 0.0005 / 3600.0, "latitude");
        assert_close(lon, 0.5, 0.0005 / 3600.0, "longitude");
        // The note's intermediate constants: B and the meridian distance of the origin.
        assert_close(bng.radius, 6_366_914.609, 0.001, "B");
        assert_close(bng.radius * bng.xi0, 5_429_228.602, 0.001, "M0");
    }

    /// GN7-2 section 3.2.3.3, worked example: Hartebeesthoek94 / Lo29 (EPSG:2053), Transverse Mercator
    /// (South Orientated): westing first, southing second, the axis order the registry gives for the system.
    #[test]
    fn gn7_2_transverse_mercator_south_orientated_example_lo29() {
        let c = ProjectCrs::from_epsg(2053).unwrap();
        assert_eq!(c.name, "Hartebeesthoek94 / Lo29");
        assert!(c.utm_zone().is_none(), "Lo systems are not UTM zones");
        let lat = -(25.0 + 43.0 / 60.0 + 55.302 / 3600.0);
        let lon = 28.0 + 16.0 / 60.0 + 57.479 / 3600.0;
        let (w, s) = c.from_geographic(lat, lon).unwrap();
        assert_close(w, 71_984.49, 0.02, "westing");
        assert_close(s, 2_847_342.74, 0.02, "southing");
        let (la, lo) = c.to_geographic(71_984.49, 2_847_342.74).unwrap();
        assert_close(la, lat, 0.0005 / 3600.0, "latitude");
        assert_close(lo, lon, 0.0005 / 3600.0, "longitude");
        // West of the central meridian is a positive westing; the area of use is inside Lo29's.
        assert!(w > 0.0 && c.area_of_use().unwrap().contains(lat, lon));
        // Convergence is not defined for the turned axes.
        assert!(c.scale_and_convergence_at(w, s).is_none());
        // Round trip across the whole area of use.
        let a = c.area_of_use().unwrap();
        for i in 0..=8 {
            for j in 0..=4 {
                let (la, lo) = (a.south + (a.north - a.south) * f64::from(i) / 8.0, a.west + (a.east - a.west) * f64::from(j) / 4.0);
                let (w, s) = c.from_geographic(la, lo).unwrap();
                let (la2, lo2) = c.to_geographic(w, s).unwrap();
                assert!((la2 - la).abs() < 1e-9 && (lo2 - lo).abs() < 1e-9, "{la} {lo}");
            }
        }
    }

    #[test]
    fn the_new_systems_are_what_their_pages_said_and_round_trip() {
        for (code, name, geog, lon0) in [
            (20136, "Adindan / UTM zone 36N", 4201, 33.0),
            (20935, "Arc 1950 / UTM zone 35S", 4209, 27.0),
            (20934, "Arc 1950 / UTM zone 34S", 4209, 21.0),
            (2046, "Hartebeesthoek94 / Lo15", 4148, 15.0),
        ] {
            let c = ProjectCrs::from_epsg(code).unwrap();
            assert_eq!((c.name.as_str(), c.geographic_epsg()), (name, Some(geog)));
            assert_close(c.projection().unwrap().central_meridian_deg(), lon0, 1e-12, "central meridian");
        }
        assert_eq!(ProjectCrs::from_epsg(20138).unwrap().utm_zone(), Some((38, false)));
        assert_eq!(ProjectCrs::from_epsg(20934).unwrap().utm_zone(), Some((34, true)));
        assert_eq!(ProjectCrs::from_epsg(32736).unwrap().geographic_epsg(), Some(4326));
        assert_eq!(ProjectCrs::from_epsg(21036).unwrap().geographic_epsg(), Some(4210));
        assert_eq!(ProjectCrs::parse("EPSG:4326").unwrap().geographic_epsg(), Some(4326));
        assert_eq!(ProjectCrs::local_grid("x").geographic_epsg(), None);
        assert_eq!(ProjectCrs::named_only(Some(2039), "Israeli Grid").geographic_epsg(), None);
        // Round trip in every new UTM system, over its area of use.
        for code in [20135, 20136, 20137, 20138, 20934, 20935, 20936] {
            let c = ProjectCrs::from_epsg(code).unwrap();
            let a = c.area_of_use().unwrap();
            for i in 0..=6 {
                for j in 0..=4 {
                    let lat = a.south + (a.north - a.south) * f64::from(i) / 6.0;
                    let lon = a.west + (a.east - a.west) * f64::from(j) / 4.0;
                    let (e, n) = c.from_geographic(lat, lon).unwrap();
                    let (la, lo) = c.to_geographic(e, n).unwrap();
                    assert!((la - lat).abs() < 1e-9 && (lo - lon).abs() < 1e-9, "{code} {lat} {lon}");
                    assert!((500_000.0 - e).abs() < 400_000.0, "{code}: easting {e} for {lat} {lon} is not a UTM easting");
                }
            }
        }
    }

    #[test]
    fn named_only_and_local_systems_compute_nothing() {
        let n = ProjectCrs::named_only(Some(4326), "WGS 84");
        assert!(n.to_geographic(1.0, 2.0).is_none() && n.projection().is_none() && n.area_of_use().is_none());
        let l = ProjectCrs::local_grid("Site grid");
        assert!(l.from_geographic(0.0, 0.0).is_none());
        assert_eq!(l.kind, CrsKind::LocalGrid);
    }

    #[test]
    fn units_of_position_round_trip_near_a_southern_northing_of_ten_million_metres() {
        // A UTM southern-hemisphere northing near 10 000 000 m is 1e10 mm; the spacing of f64
        // there is 1.9e-6 mm, so a millimetre-level survey value is kept to 0.001 mm.
        let (e, n) = (498_765.432_1, 9_999_876.543_2);
        let p = model_point_from_crs(e, n);
        let (e2, n2) = crs_from_model_point(p);
        assert!((m_to_mm(e2) - m_to_mm(e)).abs() < 0.001 && (m_to_mm(n2) - m_to_mm(n)).abs() < 0.001);
        // A 0.001 mm step is still a step at that magnitude.
        let q = crate::Pt::new(p.x, p.y + 0.001);
        assert!(q.y > p.y && (q.y - p.y - 0.001).abs() < 2e-6);
        assert_eq!(elevation_mm_to_m(elevation_m_to_mm(1234.567)), 1234.567);
        assert_close(m_to_mm(0.001), 1.0, 1e-12, "1 mm");
    }

    fn extent(e0: f64, n0: f64, e1: f64, n1: f64) -> Option<PlanExtent> {
        Some(PlanExtent { min_easting: e0, min_northing: n0, max_easting: e1, max_northing: n1 })
    }

    fn dataset(crs: Option<ProjectCrs>, ext: Option<PlanExtent>) -> IncomingDataset {
        IncomingDataset { label: "road.xml".into(), crs, extent: ext }
    }

    #[test]
    fn a_matching_dataset_is_clean() {
        // Near Dar es Salaam: zone 37S, easting 530 000 m, northing 9 240 000 m.
        let p = ProjectCrs::from_epsg(32737).unwrap();
        let r = CrsCheck::run(&p, &dataset(Some(p.clone()), extent(530_000.0, 9_240_000.0, 531_000.0, 9_241_000.0)));
        assert!(r.is_clean() && r.has(FindingKind::SameSystem), "{r:?}");
    }

    #[test]
    fn a_different_system_is_stated_with_both_names_and_not_reprojected() {
        let p = ProjectCrs::from_epsg(32636).unwrap();
        let arc = ProjectCrs::from_epsg(21096).unwrap();
        let r = CrsCheck::run(&p, &dataset(Some(arc), extent(500_000.0, 100_000.0, 501_000.0, 101_000.0)));
        let m = &r.findings[0];
        assert_eq!(m.kind, FindingKind::DifferentSystem);
        assert!(m.warning && m.message.contains("Arc 1960 / UTM zone 36N (EPSG:21096)") && m.message.contains("WGS 84 / UTM zone 36N (EPSG:32636)") && m.message.contains("Nothing is reprojected"), "{}", m.message);
    }

    #[test]
    fn an_undeclared_dataset_is_flagged() {
        let p = utm36s();
        let r = CrsCheck::run(&p, &dataset(None, extent(213_629.0, 9_885_072.0, 237_955.0, 9_892_378.0)));
        assert!(r.has(FindingKind::Undeclared) && !r.is_clean());
        assert!(r.messages()[0].contains("does not say which coordinate system"));
    }

    #[test]
    fn swapped_millimetre_and_foot_extents_are_recognised() {
        let p = utm36s();
        // Northern Uganda sits in 36N; use a southern-zone block near Kampala-ish longitudes:
        // easting 450 000 m, northing 9 960 000 m (about 0.36 N is north, so take south 0.5).
        let good = extent(450_000.0, 9_950_000.0, 452_000.0, 9_952_000.0).unwrap();
        assert!(CrsCheck::run(&p, &dataset(Some(p.clone()), Some(good))).is_clean());
        let swapped = extent(9_950_000.0, 450_000.0, 9_952_000.0, 452_000.0);
        assert!(CrsCheck::run(&p, &dataset(Some(p.clone()), swapped)).has(FindingKind::LikelySwapped));
        let mm = extent(450_000_000.0, 9_950_000_000.0, 452_000_000.0, 9_952_000_000.0);
        let r = CrsCheck::run(&p, &dataset(Some(p.clone()), mm));
        assert!(r.has(FindingKind::LikelyMillimetres), "{r:?}");
        let feet = extent(450_000.0 / FOOT_M, 9_950_000.0 / FOOT_M, 452_000.0 / FOOT_M, 9_952_000.0 / FOOT_M);
        let r = CrsCheck::run(&p, &dataset(Some(p.clone()), feet));
        assert!(r.has(FindingKind::LikelyFeet), "{r:?}");
        // About 81 degrees south: outside the area of use (80 south to the equator).
        let far = extent(450_000.0, 1_000_000.0, 452_000.0, 1_002_000.0);
        let r = CrsCheck::run(&p, &dataset(Some(p.clone()), far));
        assert!(r.has(FindingKind::OutsideValidArea), "{r:?}");
        assert!(r.messages().iter().any(|m| m.contains("area of use")));
    }

    #[test]
    fn arc_1960_extent_outside_its_area_of_use_is_flagged_and_local_grids_are_not_tested() {
        let arc = ProjectCrs::from_epsg(21096).unwrap();
        // 36N Arc 1960 area of use is 0 to 4.63 N; northing 1 000 000 m is about 9 degrees north.
        let r = CrsCheck::run(&arc, &dataset(Some(arc.clone()), extent(500_000.0, 1_000_000.0, 501_000.0, 1_001_000.0)));
        assert!(r.has(FindingKind::OutsideValidArea));
        let local = ProjectCrs::local_grid("Site grid");
        let r = CrsCheck::run(&local, &dataset(Some(local.clone()), extent(0.0, 0.0, 100.0, 100.0)));
        assert!(r.has(FindingKind::CannotCheck) && r.is_clean());
        let named = ProjectCrs::named_only(Some(3857), "Pseudo-Mercator");
        let r = CrsCheck::run(&named, &dataset(None, extent(0.0, 0.0, 100.0, 100.0)));
        assert!(r.has(FindingKind::CannotCheck) && r.has(FindingKind::Undeclared));
    }

    #[test]
    fn the_project_crs_serialises_and_old_json_without_optional_fields_reads() {
        let c = ProjectCrs::from_wkt(WKT_21096).unwrap();
        let back: ProjectCrs = serde_json::from_str(&serde_json::to_string(&c).unwrap()).unwrap();
        assert_eq!(back, c);
        let minimal: ProjectCrs = serde_json::from_str(r#"{"name":"X","kind":"NamedOnly"}"#).unwrap();
        assert_eq!((minimal.epsg, minimal.source, minimal.linear_unit), (None, CrsSource::Typed, LinearUnit::Metre));
    }

    #[test]
    fn the_older_corridor_value_still_reads() {
        // How a surface's system was stored before there was one type: epsg and name, nothing else.
        let old: ProjectCrs = serde_json::from_str(r#"{"epsg":32736,"name":"WGS 84 / UTM zone 36S"}"#).unwrap();
        assert_eq!(old.kind, CrsKind::Projected);
        assert_eq!(old.epsg, Some(32736));
        assert_eq!(old.horizontal_datum, "WGS 84");
        assert!(old.to_geographic(500_000.0, 10_000_000.0).is_some());
        let other: ProjectCrs = serde_json::from_str(r#"{"epsg":2039,"name":"Israeli Grid"}"#).unwrap();
        assert_eq!(other.kind, CrsKind::NamedOnly);
        assert_eq!(other.name, "Israeli Grid");
        // A value written now round-trips unchanged.
        let now = ProjectCrs::from_epsg(21036).unwrap();
        assert_eq!(serde_json::from_str::<ProjectCrs>(&serde_json::to_string(&now).unwrap()).unwrap(), now);
    }

    #[test]
    fn search_finds_systems_by_code_and_by_name() {
        let by_code: Vec<_> = ProjectCrs::search("2103", 10).iter().map(|c| c.epsg.unwrap()).collect();
        assert_eq!(by_code, vec![21035, 21036, 21037]);
        assert_eq!(ProjectCrs::search("EPSG:32736", 5)[0].name, "WGS 84 / UTM zone 36S");
        let arc = ProjectCrs::search("arc 1960", 20);
        assert_eq!(arc.len(), 6);
        assert!(arc.iter().all(|c| c.horizontal_datum == "Arc 1960"));
        assert_eq!(ProjectCrs::search("utm zone 36s", 10).len(), 3); // WGS 84, Arc 1960 and Arc 1950
        assert!(ProjectCrs::search("", 10).is_empty());
        assert!(ProjectCrs::search("atlantis", 10).is_empty());
        assert_eq!(ProjectCrs::utm_wgs84(36, true).unwrap().epsg, Some(32736));
        assert!(ProjectCrs::utm_wgs84(61, true).is_none());
    }

    #[test]
    fn doubts_catch_swapped_columns_and_a_wrong_zone() {
        let c = ProjectCrs::utm_wgs84(36, true).unwrap();
        assert!(c.doubts((213_629.0, 9_885_072.0), (237_955.0, 9_892_378.0)).is_empty());
        let swapped = c.doubts((9_885_072.0, 213_629.0), (9_892_378.0, 237_955.0));
        assert_eq!(swapped.len(), 1);
        assert!(swapped[0].contains("check the column order"), "{swapped:?}");
        let far = c.doubts((880_000.0, 9_885_072.0), (890_000.0, 9_892_378.0));
        assert!(far.len() == 1 && far[0].contains("neighbouring zone"), "{far:?}");
        // A system the kernel cannot compute with has nothing to doubt.
        assert!(ProjectCrs::named_only(Some(2039), "Israeli Grid").doubts((1.0, 1.0), (2.0, 2.0)).is_empty());
    }
}
