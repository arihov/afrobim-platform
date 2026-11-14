//! The UK DMRB design basis for open channels and pipe culverts.
//!
//! # Design basis
//!
//! The *Design Manual for Roads and Bridges* (National Highways and the UK devolved
//! administrations), as downloaded from standardsforhighways.co.uk in November 2026. The
//! versions below are the current ones at that date; all four carry "Date of publication of
//! relevant change: September 2025".
//!
//! | Document | Version | Title |
//! |---|---|---|
//! | CG 501 | 2.2.0 | Design of highway drainage systems |
//! | CD 521 | 1.2.0 | Hydraulic design of road edge surface water channels and outlets |
//! | CD 522 | 0.1.0 | Drainage of runoff from natural catchments |
//! | CD 529 | 1.1.0 | Design of outfall and culvert details |
//!
//! What is taken from them:
//!
//! * **Design return periods**: CG 501 clauses 5.1 and 5.3 ([`RoadDrainageCriterion`]) and
//!   CD 522 clause 5.1 ([`NATURAL_CATCHMENT_RETURN_PERIOD_YEARS`]).
//! * **Manning's n**: CD 521 Table 5.18.1 for concrete and asphalt surface water channels
//!   ([`ChannelSurface`]) and Equation 5.19 for grassed ones ([`grassed_manning_n`]).
//! * **Channel flow**: the Manning resistance equation, CD 521 Equation 5.25 (the shared
//!   [`super::hydraulics`], through [`super::check_channel`] with [`super::DesignBasis::Dmrb`]),
//!   and its closed forms for trapezoidal, triangular and rectangular channels, Equations
//!   5.25.1 to 5.25.4, with the hydraulic-radius factors of Equations 5.14 to 5.16, the shape
//!   parameter of Equation 5.10 and the equivalent slope of Equation 5.17.
//! * **Pipe culvert size rules**: CD 529 clauses 3.4, 3.5.1 and 3.10 and NOTE 2 to clause 1.1
//!   ([`culvert_layout_limits`], [`CULVERT_INVERT_BELOW_DRAIN_BED_M`]).
//!
//! # Scope of CD 521, and an interpretation
//!
//! CD 521 is written for *road edge surface water channels*: shallow lined or grassed channels
//! at the edge of the carriageway. Its Manning equation and section geometry are general, and
//! the functions here accept any section of the trapezoid family. Using its roughness values
//! for a deeper side drain is this crate's reading, not something CD 521 states: its Table
//! 5.18.1 values are for concrete and asphalt channels of that kind, and Equation 5.19 for
//! mown grass of 50 to 75 mm. The DMRB document for ditches is CD 532, which is not held.
//!
//! # Inputs, not defaults
//!
//! * **Rainfall and flow.** Every DMRB flow method here is tied to UK rainfall data and must
//!   not be applied elsewhere: CD 521 Equations 5.20 to 5.22 (drainage length and design depth)
//!   use the 2minM5 rainfall depth read from a UK map (CD 521 Figure 5.3); CD 522 clause 5.2
//!   requires IH 124 or the Flood Estimation Handbook, both fitted to UK catchments. None is
//!   implemented. Under this basis the design flow is an input, with its source.
//! * **Climate change uplift.** CG 501 clauses 4.2 and 4.3 ([`CLIMATE_UPLIFT_DEFAULT`],
//!   [`CLIMATE_UPLIFT_SENSITIVITY`]) are UK policy for UK rainfall. They are held as named
//!   constants and applied to nothing.
//! * **Permissible velocity and freeboard of a ditch.** The four documents state neither.
//!
//! # What is not here
//!
//! * Culvert hydraulics: CD 529 clause 3.3 refers it wholly to CIRIA C786 (*Culvert, screen and
//!   outfall manual*), which is not a free document and is not held.
//! * Ditch design to CD 532; outlets, gratings, weirs, surcharged channels, combined channel
//!   and pipe systems and their internal pipes (CD 521 Equations 5.26 onward, Tables 5.71 and
//!   5.74), which belong to kerb-side channels rather than to the elements this crate models.
//! * Anything given only as a figure: CD 521 Figure 5.3 (rainfall), Figures H.3 and H.4
//!   (surcharged flow ratios) and the outlet design charts.
//!
//! The result is a calculation for an engineer to check, not a certified design.

use serde::{Deserialize, Serialize};

use super::hydraulics::ChannelGeometry;
use super::{CulvertLayoutLimits, Limit, LongCulvertRule};

/// The first line of every calculation to this basis.
pub const BASIS: &str = "Design basis: UK Design Manual for Roads and Bridges: CG 501 v2.2.0, CD 521 v1.2.0, CD 522 v0.1.0, CD 529 v1.1.0 (each published September 2025).";

/// The documents and versions this module was written from: (reference, version, title).
pub const DOCUMENTS: [(&str, &str, &str); 4] = [
    ("CG 501", "2.2.0", "Design of highway drainage systems"),
    ("CD 521", "1.2.0", "Hydraulic design of road edge surface water channels and outlets"),
    ("CD 522", "0.1.0", "Drainage of runoff from natural catchments"),
    ("CD 529", "1.1.0", "Design of outfall and culvert details"),
];

// ---------------------------------------------------------------------------------------------
// Design return periods
// ---------------------------------------------------------------------------------------------

/// The storm criteria of CG 501 for road runoff within drainage systems. NOTE to clause 5.1:
/// "This requirement applies to all types of road drainage."
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum RoadDrainageCriterion {
    /// CG 501 clause 5.1 (1): "1 in 1 year – no surcharge of the drainage system". For surface
    /// water channels Table 5.2 reads: "Flows to be contained within the channel".
    NoSurcharge,
    /// CG 501 clause 5.1 (2): "1 in 5 years – no flooding from the drainage system".
    NoFlooding,
    /// CG 501 clause 5.3: flooding not to "extend beyond the highway boundary up to the
    /// 1-in-100 year rainfall event, including an allowance for climate change".
    FloodingWithinHighwayBoundary,
}

impl RoadDrainageCriterion {
    /// The return period of the criterion, years.
    pub fn return_period_years(self) -> u32 {
        match self {
            RoadDrainageCriterion::NoSurcharge => 1,
            RoadDrainageCriterion::NoFlooding => 5,
            RoadDrainageCriterion::FloodingWithinHighwayBoundary => 100,
        }
    }

    pub fn citation(self) -> &'static str {
        match self {
            RoadDrainageCriterion::NoSurcharge => "CG 501 v2.2.0 clause 5.1 (1)",
            RoadDrainageCriterion::NoFlooding => "CG 501 v2.2.0 clause 5.1 (2)",
            RoadDrainageCriterion::FloodingWithinHighwayBoundary => "CG 501 v2.2.0 clause 5.3",
        }
    }
}

/// CD 522 v0.1.0 clause 5.1: "Flow rates from natural catchments shall be determined for a
/// 1 in 100 year storm with the relevant allowance for climate change." This is the return
/// period for a ditch or culvert that carries runoff from land outside the highway; a drain
/// that carries road runoff only falls under [`RoadDrainageCriterion`]. Which a given side
/// drain is, is the engineer's call: the documents do not class "side drains" as such.
pub const NATURAL_CATCHMENT_RETURN_PERIOD_YEARS: u32 = 100;

/// CG 501 v2.2.0 clause 4.2: where the National Application Annex states no allowance, "a 20 %
/// uplift in peak rainfall intensity shall be applied". UK policy for UK rainfall; held for
/// reference and applied to nothing in this crate.
pub const CLIMATE_UPLIFT_DEFAULT: f64 = 0.20;
/// CG 501 v2.2.0 clause 4.3: "a sensitivity test to 40 % uplift in peak rainfall intensity
/// shall be undertaken". UK policy; see [`CLIMATE_UPLIFT_DEFAULT`].
pub const CLIMATE_UPLIFT_SENSITIVITY: f64 = 0.40;

// ---------------------------------------------------------------------------------------------
// Roughness
// ---------------------------------------------------------------------------------------------

/// The rows of CD 521 v1.2.0 Table 5.18.1, "Values of Manning's roughness coefficients for
/// surface water channels".
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ChannelSurface {
    ConcreteAverage,
    ConcretePoor,
    AsphaltAverage,
    AsphaltPoor,
}

impl ChannelSurface {
    pub const ALL: [ChannelSurface; 4] = [ChannelSurface::ConcreteAverage, ChannelSurface::ConcretePoor, ChannelSurface::AsphaltAverage, ChannelSurface::AsphaltPoor];

    pub fn label(self) -> &'static str {
        match self {
            ChannelSurface::ConcreteAverage => "concrete, average condition",
            ChannelSurface::ConcretePoor => "concrete, poor condition",
            ChannelSurface::AsphaltAverage => "asphalt, average condition",
            ChannelSurface::AsphaltPoor => "asphalt, poor condition",
        }
    }

    /// Manning's n, CD 521 Table 5.18.1 (clause 5.18.1: "should be used for design of concrete
    /// or asphalt surface water channels").
    pub fn manning_n(self) -> f64 {
        match self {
            ChannelSurface::ConcreteAverage => 0.013,
            ChannelSurface::ConcretePoor => 0.016,
            ChannelSurface::AsphaltAverage => 0.017,
            ChannelSurface::AsphaltPoor => 0.021,
        }
    }

    /// The text to quote as the source of n in a calculation.
    pub fn source(self) -> String {
        format!("CD 521 Table 5.18.1, {}", self.label())
    }
}

/// The two grass mixtures CD 521 v1.2.0 gives values for under Equation 5.19.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum GrassType {
    PerennialRyeGrass,
    FescuesDominated,
}

impl GrassType {
    /// The coefficient mg: "0.0048 for perennial rye grass and 0.0096 for fescues-dominated mix".
    pub fn mg(self) -> f64 {
        match self {
            GrassType::PerennialRyeGrass => 0.0048,
            GrassType::FescuesDominated => 0.0096,
        }
    }

    /// The grass height H to assume, m: "0.05 m for fescues-dominated mixture and 0.075 m for
    /// perennial rye grass-dominated mixture".
    pub fn assumed_height_m(self) -> f64 {
        match self {
            GrassType::PerennialRyeGrass => 0.075,
            GrassType::FescuesDominated => 0.05,
        }
    }
}

/// Manning's n of a grassed surface water channel: CD 521 v1.2.0 Equation 5.19,
/// `n = 0.05 / (1 − mg·H / (R^(5/3)·S^(1/2)))`, with R the hydraulic radius (m) and S the
/// longitudinal gradient (m/m).
///
/// `None` when the bracket is not positive (a flow too shallow or too flat for the relation,
/// which then has no meaning). NOTE 1 to the clause: "Values of n are typically within the
/// range 0.05 to 0.1." n depends on R, so it changes with the depth of flow.
pub fn grassed_manning_n(mg: f64, grass_height_m: f64, hydraulic_radius_m: f64, gradient_m_per_m: f64) -> Option<f64> {
    let bracket = 1.0 - mg * grass_height_m / (hydraulic_radius_m.powf(5.0 / 3.0) * gradient_m_per_m.sqrt());
    if bracket.is_finite() && bracket > 0.0 {
        Some(0.05 / bracket)
    } else {
        None
    }
}

// ---------------------------------------------------------------------------------------------
// Channel flow
// ---------------------------------------------------------------------------------------------

/// The hydraulic-radius factor r of CD 521 v1.2.0:
///
/// ```text
/// r = (Bb + (b1 + b2)·y) / (Bb + [√(1 + b1²) + √(1 + b2²)]·y)      Equation 5.14, trapezoidal
/// ```
///
/// which with `Bb = 0` is Equation 5.15 (triangular) and with `b1 = b2 = 0` is Equation 5.16
/// (rectangular, `Bb / (Bb + 2y)`). `b1`, `b2` are the side slopes, 1 vertical to b horizontal;
/// here they are the geometry's side slopes. r is the surface width over the wetted perimeter.
pub fn hydraulic_radius_factor(geometry: &ChannelGeometry, depth_m: f64) -> f64 {
    geometry.top_width_m(depth_m) / geometry.wetted_perimeter_m(depth_m)
}

/// The shape characteristic parameter of CD 521 v1.2.0 Equation 5.10, `m = B·y / A − 1`, with B
/// the surface width of flow. NOTE 2: "For a triangular profile m = 1, and for a rectangular
/// profile m = 0."
pub fn shape_parameter_m(geometry: &ChannelGeometry, depth_m: f64) -> f64 {
    geometry.top_width_m(depth_m) * depth_m / geometry.area_m2(depth_m) - 1.0
}

/// Design flow of a surface water channel at depth `y`, m³/s: CD 521 v1.2.0 Equation 5.25.1,
///
/// ```text
/// Q = (S^(1/2) / n) · [ (2·Bb + b·y)^5 / (32·(Bb + b·y)²) · r² · y^5 ]^(1/3)        b = b1 + b2
/// ```
///
/// With `Bb = 0` this is Equation 5.25.2 (triangular, `Q = (b·S^(1/2)/n)·[r²·y^8/32]^(1/3)`)
/// and with `b = 0` Equation 5.25.4 (rectangular, `Q = (Bb·S^(1/2)/n)·[r²·y^5]^(1/3)`). All are
/// the Manning resistance equation, Equation 5.25, written out for the shape; the tests confirm
/// they return what [`ChannelGeometry::discharge_m3_s`] does.
pub fn design_flow_m3_s(geometry: &ChannelGeometry, depth_m: f64, manning_n: f64, gradient_m_per_m: f64) -> f64 {
    if depth_m <= 0.0 {
        return 0.0;
    }
    let (bb, b, y) = (geometry.bottom_width_m, geometry.left_side_slope + geometry.right_side_slope, depth_m);
    let r = hydraulic_radius_factor(geometry, y);
    gradient_m_per_m.sqrt() / manning_n * ((2.0 * bb + b * y).powi(5) / (32.0 * (bb + b * y).powi(2)) * r * r * y.powi(5)).cbrt()
}

/// Design flow of a symmetrical triangular channel, m³/s: CD 521 v1.2.0 Equation 5.25.3,
/// `Q = 0.315 · (B·y)^(5/3) / (B² + 4y²)^(1/3) · S^(1/2) / n`, with B the surface width of flow.
/// The constant is as printed (it is 2^(−5/3) = 0.3150).
pub fn symmetrical_triangular_flow_m3_s(surface_width_m: f64, depth_m: f64, manning_n: f64, gradient_m_per_m: f64) -> f64 {
    let (b, y) = (surface_width_m, depth_m);
    0.315 * (b * y).powf(5.0 / 3.0) / (b * b + 4.0 * y * y).cbrt() * gradient_m_per_m.sqrt() / manning_n
}

/// The equivalent uniform slope of a channel whose gradient varies along its length: CD 521
/// v1.2.0 Equation 5.17,
///
/// ```text
/// Se = 400 · [ S1^(−1/2) + S11^(−1/2) + 2·Σ(j = 2..10) Sj^(−1/2) ]^(−2)
/// ```
///
/// from the local gradients at eleven equally spaced points, upstream end first. A zero
/// gradient at either end is replaced as clause 5.17.1 says (`S1 = S2/9`, `S11 = S10/9`).
/// `None` when any gradient is negative or not a number, or an intermediate one is zero
/// (clause 5.17.2: place an outlet there and design two lengths).
pub fn equivalent_slope(local_gradients: &[f64; 11]) -> Option<f64> {
    let mut s = *local_gradients;
    if s.iter().any(|v| !v.is_finite() || *v < 0.0) {
        return None;
    }
    if s[0] == 0.0 {
        s[0] = s[1] / 9.0;
    }
    if s[10] == 0.0 {
        s[10] = s[9] / 9.0;
    }
    if s.iter().any(|v| *v <= 0.0) {
        return None;
    }
    let inv = |v: f64| 1.0 / v.sqrt();
    let sum = inv(s[0]) + inv(s[10]) + 2.0 * s[1..10].iter().map(|v| inv(*v)).sum::<f64>();
    Some(400.0 / (sum * sum))
}

// ---------------------------------------------------------------------------------------------
// Pipe culverts
// ---------------------------------------------------------------------------------------------

/// CD 529 v1.1.0 clause 3.5.1: "The minimum culvert diameter should be 450 mm as smaller sizes
/// are prone to blockage", m.
pub const MINIMUM_CULVERT_DIAMETER_M: f64 = 0.45;
/// CD 529 v1.1.0 clause 3.4: "Where a circular culvert is longer than 12 metres it shall be at
/// least 1.2 metres in diameter to facilitate access for maintenance": the length, m.
pub const LONG_CULVERT_LENGTH_M: f64 = 12.0;
/// CD 529 v1.1.0 clause 3.4: the diameter a culvert longer than 12 m needs, m.
pub const LONG_CULVERT_MINIMUM_DIAMETER_M: f64 = 1.2;
/// CD 529 v1.1.0 clause 1.1 NOTE 2: "Culverts and outfalls having a diameter or clear span
/// greater than 900 mm are defined as structures", m.
pub const STRUCTURE_ABOVE_DIAMETER_M: f64 = 0.9;
/// CD 529 v1.1.0 clause 3.10: where a culvert conveys a highway drain that is not a public
/// watercourse, "the invert shall be at least 75 mm below the bed of the drain", m. The crate
/// does not hold the bed level of the drain at a culvert, so this is not checked.
pub const CULVERT_INVERT_BELOW_DRAIN_BED_M: f64 = 0.075;

/// The pipe culvert layout limits CD 529 states, for
/// [`super::DesignBasis::culvert_layout_limits`]. It states no limit on skew or gradient.
pub fn culvert_layout_limits() -> CulvertLayoutLimits {
    CulvertLayoutLimits {
        minimum_diameter_m: Some(Limit {
            value: MINIMUM_CULVERT_DIAMETER_M,
            citation: "CD 529 v1.1.0 clause 3.5.1: 'The minimum culvert diameter should be 450 mm as smaller sizes are prone to blockage.'",
        }),
        long_culvert: Some(LongCulvertRule {
            longer_than_m: LONG_CULVERT_LENGTH_M,
            minimum_diameter_m: LONG_CULVERT_MINIMUM_DIAMETER_M,
            citation: "CD 529 v1.1.0 clause 3.4: 'Where a circular culvert is longer than 12 metres it shall be at least 1.2 metres in diameter to facilitate access for maintenance.'",
        }),
        maximum_skew_deg: None,
        gradient_not_below: None,
        gradient_avoid_below: None,
        gradient_anchors_above: None,
        gradient_special_anchors_above: None,
        structure_above_diameter_m: Some(Limit {
            value: STRUCTURE_ABOVE_DIAMETER_M,
            citation: "CD 529 v1.1.0 clause 1.1 NOTE 2: culverts 'having a diameter or clear span greater than 900 mm are defined as structures' and their structural design is outside CD 529",
        }),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::design::manning_velocity_m_s;

    fn close(actual: f64, expected: f64, tolerance: f64) {
        assert!((actual - expected).abs() <= tolerance, "expected {expected} ± {tolerance}, got {actual}");
    }

    #[test]
    fn return_periods_and_documents() {
        use RoadDrainageCriterion::*;
        assert_eq!([NoSurcharge, NoFlooding, FloodingWithinHighwayBoundary].map(|c| c.return_period_years()), [1, 5, 100]);
        assert!(NoSurcharge.citation().contains("CG 501 v2.2.0 clause 5.1"));
        assert_eq!(NATURAL_CATCHMENT_RETURN_PERIOD_YEARS, 100);
        assert_eq!((CLIMATE_UPLIFT_DEFAULT, CLIMATE_UPLIFT_SENSITIVITY), (0.20, 0.40));
        assert_eq!(DOCUMENTS.map(|d| (d.0, d.1)), [("CG 501", "2.2.0"), ("CD 521", "1.2.0"), ("CD 522", "0.1.0"), ("CD 529", "1.1.0")]);
        for (reference, version, _) in DOCUMENTS {
            assert!(BASIS.contains(&format!("{reference} v{version}")));
        }
    }

    #[test]
    fn table_5_18_1_roughness() {
        assert_eq!(ChannelSurface::ALL.map(|s| s.manning_n()), [0.013, 0.016, 0.017, 0.021]);
        assert_eq!(ChannelSurface::ConcreteAverage.source(), "CD 521 Table 5.18.1, concrete, average condition");
    }

    /// CD 521 v1.2.0 Appendix B, worked example B1: triangular channel, b1 = b2 = 5,
    /// y1 = 0.120 m.
    #[test]
    fn worked_example_b1_triangular_factors() {
        let g = ChannelGeometry::v(5.0, 5.0);
        // r = (5 + 5) / (2·√26) = 0.981; B = 1.200 m; A = 0.072 m²; m = 1.00.
        close(hydraulic_radius_factor(&g, 0.120), 0.981, 0.0005);
        close(g.top_width_m(0.120), 1.200, 1e-12);
        close(g.area_m2(0.120), 0.072, 1e-12);
        close(shape_parameter_m(&g, 0.120), 1.00, 1e-12);
        // The example then finds the length of road drained from Equation 5.20 and the 2minM5
        // rainfall map for Coventry. That is UK rainfall and is not implemented.
    }

    /// CD 521 v1.2.0 Appendix B, worked example B3: trapezoidal channel, Bb = 0.300 m,
    /// b1 = b2 = 5, y1 = 0.150 m.
    #[test]
    fn worked_example_b3_trapezoidal_factors() {
        let g = ChannelGeometry::trapezoid(0.300, 5.0);
        // r = (0.300 + 10 × 0.150) / (0.300 + 2·√26 × 0.150) = 1.800 / 1.82971 = 0.984.
        close(hydraulic_radius_factor(&g, 0.150), 0.984, 0.0005);
        close(g.top_width_m(0.150), 1.800, 1e-12);
        // A = 0.300 × 0.150 + ½ × 10 × 0.150² = 0.1575 m², printed as 0.158.
        close(g.area_m2(0.150), 0.1575, 1e-12);
        // m = 1.800 × 0.150 / 0.1575 − 1 = 0.714, printed as 0.71.
        close(shape_parameter_m(&g, 0.150), 0.71, 0.005);
        // Rectangle (Equation 5.16): Bb / (Bb + 2y) = 1.0 / 1.3; m = 0.
        let r = ChannelGeometry::rectangle(1.0);
        close(hydraulic_radius_factor(&r, 0.150), 1.0 / 1.3, 1e-12);
        close(shape_parameter_m(&r, 0.150), 0.0, 1e-12);
    }

    /// CD 521 v1.2.0 Appendix B, worked example B5: grassed symmetrical triangular channel,
    /// cross-falls 1:5, y = 0.2 m, B = 2.00 m, S = 1 in 125, perennial ryegrass, H = 0.075 m.
    #[test]
    fn worked_example_b5_grassed_channel() {
        let g = ChannelGeometry::v(5.0, 5.0);
        let (y, s) = (0.2, 0.008);
        // R = 0.2 / (2 × 0.2 × √26) = 0.2 / 2.03961 = 0.098058 m; R^(5/3) = 0.020852;
        // √S = 0.089443; mg·H = 0.0048 × 0.075 = 0.00036; 0.00036 / 0.0018651 = 0.19302;
        // n = 0.05 / 0.80698 = 0.0620. The example prints n = 0.062.
        let grass = GrassType::PerennialRyeGrass;
        let n = grassed_manning_n(grass.mg(), grass.assumed_height_m(), g.hydraulic_radius_m(y), s).unwrap();
        close(n, 0.062, 0.0005);
        // "The maximum flow capacity ... can be determined from Equation 5.25.3. Q = 0.061 m3/s":
        // 0.315 × 0.4^(5/3) / 4.16^(1/3) × 0.089443 / 0.062 = 0.315 × 0.217153 / 1.608286 × 1.44263 = 0.0614.
        let q = symmetrical_triangular_flow_m3_s(2.0, y, 0.062, s);
        close(q, 0.061, 0.0005);
        // The same from the general forms.
        close(design_flow_m3_s(&g, y, 0.062, s), q, 1e-5);
        close(g.discharge_m3_s(y, 0.062, s), q, 1e-5);
        // (The example cites "clause 5.20" for mg and H; they are under Equation 5.19.)
        assert_eq!((GrassType::FescuesDominated.mg(), GrassType::FescuesDominated.assumed_height_m()), (0.0096, 0.05));
        // Too shallow and flat for the relation: no value rather than a negative one.
        assert_eq!(grassed_manning_n(0.0096, 0.05, 0.02, 0.001), None);
    }

    /// CD 521 v1.2.0 Appendix B, worked example B7: triangular channel 1.2 m wide, design flow
    /// depth 0.120 m, S = 0.005, n = 0.013. "R = 0.072 / 1.224 = 0.0588 m", "Qd = 0.0592 m3/s".
    #[test]
    fn worked_example_b7_triangular_channel_full_flow() {
        let g = ChannelGeometry::v(5.0, 5.0);
        close(g.wetted_perimeter_m(0.120), 1.224, 0.0005);
        close(g.hydraulic_radius_m(0.120), 0.0588, 0.00005);
        let n = ChannelSurface::ConcreteAverage.manning_n();
        // 0.072 × 0.0588^(2/3) × 0.005^(1/2) / 0.013 = 0.072 × 0.15127 × 0.070711 / 0.013 = 0.0592.
        close(g.discharge_m3_s(0.120, n, 0.005), 0.0592, 0.00005);
        close(design_flow_m3_s(&g, 0.120, n, 0.005), 0.0592, 0.00005);
        close(symmetrical_triangular_flow_m3_s(1.2, 0.120, n, 0.005), 0.0592, 0.00005);
    }

    /// CD 521 v1.2.0 Appendix B, worked example B8: trapezoidal channel, base 0.300 m, side
    /// slopes 1:5, depth 0.150 m, S = 0.002, n = 0.013. "R = 0.1575 / 1.830 = 0.0861 m",
    /// "Qd = 0.106 m3/s".
    #[test]
    fn worked_example_b8_trapezoidal_channel_full_flow() {
        let g = ChannelGeometry::trapezoid(0.300, 5.0);
        close(g.wetted_perimeter_m(0.150), 1.830, 0.0005);
        close(g.hydraulic_radius_m(0.150), 0.0861, 0.00005);
        // 0.1575 × 0.0861^(2/3) × 0.002^(1/2) / 0.013 = 0.1575 × 0.19500 × 0.044721 / 0.013 = 0.1057.
        close(g.discharge_m3_s(0.150, 0.013, 0.002), 0.106, 0.0005);
        close(design_flow_m3_s(&g, 0.150, 0.013, 0.002), 0.106, 0.0005);
    }

    #[test]
    fn equations_5_25_1_to_5_25_4_are_manning_for_the_shape() {
        for (g, y, n, s) in [
            (ChannelGeometry::trapezoid(0.3, 5.0), 0.15, 0.013, 0.005),
            (ChannelGeometry { bottom_width_m: 0.5, left_side_slope: 1.5, right_side_slope: 3.0 }, 0.6, 0.025, 0.012),
            (ChannelGeometry::v(4.0, 1.0), 0.12, 0.017, 0.02),
            (ChannelGeometry::v(0.0, 5.0), 0.1, 0.016, 0.004),
            (ChannelGeometry::rectangle(1.0), 0.17, 0.013, 0.005),
        ] {
            let manning = g.area_m2(y) * manning_velocity_m_s(n, g.hydraulic_radius_m(y), s);
            close(design_flow_m3_s(&g, y, n, s), manning, 1e-12 * manning.max(1.0));
        }
        assert_eq!(design_flow_m3_s(&ChannelGeometry::rectangle(1.0), 0.0, 0.013, 0.005), 0.0);
    }

    #[test]
    fn equivalent_slope_of_equation_5_17() {
        // A uniform gradient is its own equivalent: 400 × [20 / √S]^(−2) = S.
        close(equivalent_slope(&[0.01; 11]).unwrap(), 0.01, 1e-15);
        // Half at 0.04 and half at 0.01 with the change at the middle point (taken as 0.01):
        // S^(−1/2) is 5 or 10: 5 + 10 + 2 × (4 × 5 + 5 × 10) = 155; Se = 400 / 155² = 0.016649.
        let mut s = [0.01; 11];
        s[..5].fill(0.04);
        close(equivalent_slope(&s).unwrap(), 0.016649, 1e-6);
        // A zero gradient at the upstream end is replaced by S2 / 9 (clause 5.17.1):
        // S1^(−1/2) = 3 × 10 = 30; 30 + 10 + 2 × 9 × 10 = 220; Se = 400 / 220² = 0.0082645.
        let mut s = [0.01; 11];
        s[0] = 0.0;
        close(equivalent_slope(&s).unwrap(), 0.0082645, 1e-7);
        // A zero gradient part-way along has no equivalent slope (clause 5.17.2); nor has an adverse one.
        let mut s = [0.01; 11];
        s[4] = 0.0;
        assert_eq!(equivalent_slope(&s), None);
        s[4] = -0.001;
        assert_eq!(equivalent_slope(&s), None);
    }
}
