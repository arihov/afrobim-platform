// SPDX-License-Identifier: LicenseRef-AfroBIM-Proprietary
// Copyright (c) 2026 AfroBIM. All rights reserved.
//! Structural design of a sign's supports: the wind force on the sign, a steel circular hollow
//! section post, and a planted foundation.
//!
//! # Design basis
//!
//! The method is the UK one, as set out in the Institute of Highway Engineers' *Sign Structures
//! Guide 2021* ("the Guide"), which applies BS EN 12899-1:2007 and its UK National Annex:
//!
//! * wind force `Fw = cf · wb · Aref`, with `cf` from the force coefficient table of Table NA.2
//!   (Guide Appendix A and Appendix C 1.3);
//! * partial action factor 1.35 at ULS and 1.0 at SLS (class PAF1), partial material factor
//!   1.05 for steel (BS EN 12899-1 Table 7, as cited in Guide C 1.4.5);
//! * post bending and shear at the top of the foundation, the 0.5 kN point load, and the
//!   temporary deflection against class TDB4, 25 mm/m (Guide C 1.4);
//! * planted foundation to PD 6547 or CD 354 revision 1: `Mg = G·D·P³/10` against 1.25 times
//!   the destabilising moment about a fulcrum at `P/√2` below ground (Guide 5.19, 5.20, C 1.5);
//! * spread foundation to BS EN 1997-1 Design Approach 1 with the assumed design bearing
//!   resistances of the Guide's Tables 5.2 and 5.3 (Guide 5.1 to 5.18, C 2.5).
//!
//! The basic wind pressure `wb` is an input. For Uganda [`mowt_velocity_pressure`] gives the
//! velocity pressures of the Ministry of Works and Transport *Road Design Manual Volume 4:
//! Bridge Design* (January 2010), Section 5.3, Table 2. Using that bridge-manual pressure as
//! `wb` for a sign is an interpretation, not something either document states.
//!
//! # What is not here
//!
//! * Numbers this repository does not hold are inputs, never defaults: the ground factor `G`
//!   (PD 6547 Table 2 / CD 354 Table 12.12; only the "poor" value quoted in the Guide's example
//!   is offered, as [`GROUND_FACTOR_POOR_KN_M2_PER_M`]) and the minimum planting depth of
//!   BS EN 40-2 Table 7.
//! * Signs on slopes, wind funnelling and orography, wind on the posts themselves, passively
//!   safe supports, eccentric signs, fixings and the sign plate.
//! * For spread foundations, anything but the geotechnical checks: the concrete, its
//!   reinforcement and the fixing of the posts are not designed.
//! * Member capacities come from first principles (BS EN 1993-1-1 section classification and
//!   plastic or elastic resistance of the nominal tube), so they sit slightly below the
//!   tabulated design data the Guide quotes.
//!
//! The result is a calculation for an engineer to check, not a certified design.

use std::f64::consts::{PI, SQRT_2};

use crate::SupportError;

/// Modulus of elasticity of structural steel used by the Guide's deflection check, N/mm².
pub const STEEL_E_N_MM2: f64 = 210_000.0;
/// Yield strength of grade S355 steel up to 16 mm thick, N/mm². The Guide's examples use S355.
pub const S355_YIELD_N_MM2: f64 = 355.0;
/// The ground factor G the Guide's Example 1 quotes for "poor" ground from PD 6547 Table 2 and
/// CD 354 Table 12.12, kN/m² per m. The other rows of those tables are not held here.
pub const GROUND_FACTOR_POOR_KN_M2_PER_M: f64 = 230.0;

/// Force coefficient table of BS EN 12899-1 Table NA.2: (aspect ratio, cf).
const FORCE_COEFFICIENTS: [(f64, f64); 8] = [(1.0, 1.26), (1.6, 1.30), (3.0, 1.35), (5.5, 1.40), (7.5, 1.50), (13.5, 1.60), (20.0, 1.70), (30.0, 1.80)];

/// The force coefficient `cf` for a sign face `width` by `height` (any one unit), interpolated
/// linearly in the Table NA.2 values (the Guide, 3.20, accepts interpolation). The aspect ratio
/// is the larger of l/b and b/l (Note 6 to the table). Beyond the table's last entry, 30, the
/// last value is returned.
pub fn force_coefficient(width: f64, height: f64) -> f64 {
    let aspect = (width / height).max(height / width);
    let t = &FORCE_COEFFICIENTS;
    if aspect <= t[0].0 {
        return t[0].1;
    }
    for pair in t.windows(2) {
        let ((a0, c0), (a1, c1)) = (pair[0], pair[1]);
        if aspect <= a1 {
            return c0 + (c1 - c0) * (aspect - a0) / (a1 - a0);
        }
    }
    t[t.len() - 1].1
}

/// Terrain grouping of MoWT Road Design Manual Vol. 4 (2010), Table 2.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MowtTerrain {
    /// Categories 1 and 2: exposed open terrain, or open terrain with scattered obstructions.
    Open,
    /// Categories 3 and 4: numerous closely spaced obstructions the size of houses or larger.
    BuiltUp,
}

/// MoWT Road Design Manual Vol. 4 (2010), Section 5.3, Table 2: (height to top of structure in
/// m, velocity pressure in open terrain, velocity pressure in built-up areas), kN/m².
const MOWT_TABLE_2: [(f64, f64, f64); 6] = [(5.0, 0.81, 0.37), (10.0, 0.92, 0.47), (15.0, 1.00, 0.57), (20.0, 1.06, 0.66), (25.0, 1.10, 0.72), (30.0, 1.12, 0.76)];

/// The velocity pressure of MoWT Table 2 for a structure whose top is `height_to_top_m` above
/// ground, kN/m². The row at or next above the height is used, without interpolation, so a
/// sign up to 5 m takes the 5 m row. None above the table's 30 m.
pub fn mowt_velocity_pressure(height_to_top_m: f64, terrain: MowtTerrain) -> Option<f64> {
    MOWT_TABLE_2.iter().find(|row| height_to_top_m <= row.0).map(|row| match terrain {
        MowtTerrain::Open => row.1,
        MowtTerrain::BuiltUp => row.2,
    })
}

/// The sign and its mounting, as the Guide's Figure in Appendix C names them.
#[derive(Debug, Clone, PartialEq)]
pub struct SignGeometry {
    /// Width of the sign face, l, mm.
    pub width_mm: f64,
    /// Height of the sign face, b, mm.
    pub height_mm: f64,
    /// Mounting height: ground to the lower edge of the face, hm, mm.
    pub mounting_height_mm: f64,
    /// Area of the face, m², when it is not the full rectangle (a circle, a triangle). None
    /// takes width × height, which is never less than the true area.
    pub face_area_m2: Option<f64>,
    /// Depth of post buried above the top of the foundation, hb, mm.
    pub buried_above_foundation_mm: f64,
    /// Number of posts, n. The wind force is shared equally between them.
    pub post_count: u32,
}

impl SignGeometry {
    /// Total height H = hm + b, m.
    pub fn total_height_m(&self) -> f64 {
        (self.mounting_height_mm + self.height_mm) / 1000.0
    }

    /// Height to the centroid of the face z = hm + b/2, m.
    pub fn centroid_height_m(&self) -> f64 {
        (self.mounting_height_mm + self.height_mm / 2.0) / 1000.0
    }

    pub fn area_m2(&self) -> f64 {
        self.face_area_m2.unwrap_or(self.width_mm * self.height_mm / 1.0e6)
    }

    fn check(&self) -> Result<(), SupportError> {
        let positive = [("sign width", self.width_mm), ("sign height", self.height_mm), ("face area", self.area_m2())];
        for (name, v) in positive {
            if !(v.is_finite() && v > 0.0) {
                return Err(SupportError::InvalidOption(format!("the {name} must be greater than zero (it is {v})")));
            }
        }
        for (name, v) in [("mounting height", self.mounting_height_mm), ("depth buried above the foundation", self.buried_above_foundation_mm)] {
            if !(v.is_finite() && v >= 0.0) {
                return Err(SupportError::InvalidOption(format!("the {name} must not be negative (it is {v})")));
            }
        }
        if self.post_count == 0 {
            return Err(SupportError::InvalidOption("a sign needs at least one post".into()));
        }
        Ok(())
    }
}

/// Factors and limits of the method. [`DesignFactors::default`] gives the values the Guide's
/// examples use.
#[derive(Debug, Clone, PartialEq)]
pub struct DesignFactors {
    /// Partial action factor at ULS, γF (class PAF1): 1.35.
    pub gamma_f_uls: f64,
    /// Additional factor γf3: 1.0 in the Guide's examples; 1.05 allows for wind on the posts
    /// (Guide 4.4) and 1.1 is the alternative the examples mention.
    pub gamma_f3: f64,
    /// Partial material factor for steel, γm: 1.05.
    pub gamma_m: f64,
    /// Ratio of the 1-year wind force used for temporary deflection to the SLS force. The Guide
    /// uses 0.75² / 0.96²: 75 % of the reference velocity (BS EN 12899-1 5.4.1 note 1), with
    /// 0.96 undoing a 50 to 25 year conversion. Where `wb` was not derived for a 25-year
    /// return period 0.75² is the strict value; the default is the larger, safer one.
    pub one_year_ratio: f64,
    /// Temporary deflection limit in bending, mm per metre of height: class TDB4, 25.
    pub deflection_limit_mm_per_m: f64,
    /// Point load applied at the top of the sign, kN: 0.5 (class PL3).
    pub point_load_kn: f64,
    /// Factor on the destabilising moment of a planted foundation: 1.25.
    pub overturning_factor: f64,
    /// Least ratio of planting depth to foundation diameter for the foundation to act as a
    /// planted one: 2 (Guide 5.20 and C 1.5.2).
    pub min_depth_to_diameter: f64,
}

impl Default for DesignFactors {
    fn default() -> Self {
        DesignFactors {
            gamma_f_uls: 1.35,
            gamma_f3: 1.0,
            gamma_m: 1.05,
            one_year_ratio: (0.75 * 0.75) / (0.96 * 0.96),
            deflection_limit_mm_per_m: 25.0,
            point_load_kn: 0.5,
            overturning_factor: 1.25,
            min_depth_to_diameter: 2.0,
        }
    }
}

/// Wind forces on the whole sign, kN (Guide C 1.3).
#[derive(Debug, Clone, PartialEq)]
pub struct WindForces {
    pub basic_pressure_kn_m2: f64,
    pub aspect_ratio: f64,
    pub cf: f64,
    pub area_m2: f64,
    /// Fw = cf · wb · Aref.
    pub characteristic_kn: f64,
    /// Fw · γF · γf3.
    pub uls_kn: f64,
    /// Fw · γf3.
    pub sls_kn: f64,
    /// The SLS force scaled to a 1-year return period, for temporary deflection.
    pub one_year_kn: f64,
}

pub fn wind_forces(sign: &SignGeometry, basic_pressure_kn_m2: f64, factors: &DesignFactors) -> Result<WindForces, SupportError> {
    sign.check()?;
    if !(basic_pressure_kn_m2.is_finite() && basic_pressure_kn_m2 > 0.0) {
        return Err(SupportError::InvalidOption(format!("the basic wind pressure must be greater than zero (it is {basic_pressure_kn_m2})")));
    }
    let cf = force_coefficient(sign.width_mm, sign.height_mm);
    let area = sign.area_m2();
    let fw = cf * basic_pressure_kn_m2 * area;
    let sls = fw * factors.gamma_f3;
    Ok(WindForces {
        basic_pressure_kn_m2,
        aspect_ratio: (sign.width_mm / sign.height_mm).max(sign.height_mm / sign.width_mm),
        cf,
        area_m2: area,
        characteristic_kn: fw,
        uls_kn: fw * factors.gamma_f_uls * factors.gamma_f3,
        sls_kn: sls,
        one_year_kn: sls * factors.one_year_ratio,
    })
}

/// A steel circular hollow section: nominal outside diameter and wall, and the yield strength.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct SteelChs {
    pub outside_diameter_mm: f64,
    pub wall_mm: f64,
    pub yield_n_mm2: f64,
}

impl SteelChs {
    pub fn s355(outside_diameter_mm: f64, wall_mm: f64) -> Self {
        SteelChs { outside_diameter_mm, wall_mm, yield_n_mm2: S355_YIELD_N_MM2 }
    }

    pub fn label(&self) -> String {
        format!("CHS {} × {}", self.outside_diameter_mm, self.wall_mm)
    }

    fn bore_mm(&self) -> f64 {
        self.outside_diameter_mm - 2.0 * self.wall_mm
    }

    /// Cross-sectional area, mm².
    pub fn area_mm2(&self) -> f64 {
        PI / 4.0 * (self.outside_diameter_mm.powi(2) - self.bore_mm().powi(2))
    }

    /// Second moment of area, mm⁴.
    pub fn inertia_mm4(&self) -> f64 {
        PI / 64.0 * (self.outside_diameter_mm.powi(4) - self.bore_mm().powi(4))
    }

    /// Elastic section modulus, mm³.
    pub fn elastic_modulus_mm3(&self) -> f64 {
        self.inertia_mm4() / (self.outside_diameter_mm / 2.0)
    }

    /// Plastic section modulus, mm³.
    pub fn plastic_modulus_mm3(&self) -> f64 {
        (self.outside_diameter_mm.powi(3) - self.bore_mm().powi(3)) / 6.0
    }

    /// Mass per metre at the given density, kg/m.
    pub fn mass_kg_per_m(&self, density_kg_m3: f64) -> f64 {
        self.area_mm2() / 1.0e6 * density_kg_m3
    }

    /// Section class in bending to BS EN 1993-1-1 Table 5.2 (tubular sections): d/t against
    /// 50ε², 70ε² and 90ε² with ε² = 235/fy. 4 means the limits for class 3 are exceeded.
    pub fn class(&self) -> u8 {
        let ratio = self.outside_diameter_mm / self.wall_mm;
        let e2 = 235.0 / self.yield_n_mm2;
        if ratio <= 50.0 * e2 {
            1
        } else if ratio <= 70.0 * e2 {
            2
        } else if ratio <= 90.0 * e2 {
            3
        } else {
            4
        }
    }

    /// Moment resistance of the cross-section, kNm: plastic for classes 1 and 2, elastic for
    /// class 3 (BS EN 1993-1-1 6.2.5, γM0 = 1.0). None for class 4, which is not covered.
    pub fn moment_resistance_knm(&self) -> Option<f64> {
        let w = match self.class() {
            1 | 2 => self.plastic_modulus_mm3(),
            3 => self.elastic_modulus_mm3(),
            _ => return None,
        };
        Some(w * self.yield_n_mm2 / 1.0e6)
    }

    /// Plastic shear resistance, kN: Av·fy/√3 with Av = 2A/π (BS EN 1993-1-1 6.2.6).
    pub fn shear_resistance_kn(&self) -> f64 {
        2.0 * self.area_mm2() / PI * self.yield_n_mm2 / 3f64.sqrt() / 1000.0
    }

    /// Elastic torsional resistance, kNm: Wt·fy/√3 with Wt = 2·Wel (BS EN 1993-1-1 6.2.7).
    pub fn torsion_resistance_knm(&self) -> f64 {
        2.0 * self.elastic_modulus_mm3() * self.yield_n_mm2 / 3f64.sqrt() / 1.0e6
    }

    fn check(&self) -> Result<(), SupportError> {
        let ok = [self.outside_diameter_mm, self.wall_mm, self.yield_n_mm2].iter().all(|v| v.is_finite() && *v > 0.0);
        if !ok || self.wall_mm >= self.outside_diameter_mm / 2.0 {
            return Err(SupportError::InvalidOption(format!("{} with fy = {} N/mm² is not a valid tube", self.label(), self.yield_n_mm2)));
        }
        Ok(())
    }
}

/// Nominal sizes of hot-finished circular hollow sections commonly listed to BS EN 10210-2,
/// (outside diameter, wall) in mm, in increasing mass. This is a convenience list for
/// [`select_post`], not the standard's full range, and it says nothing about what a supplier
/// in a given market stocks.
pub const COMMON_CHS_MM: [(f64, f64); 19] = [
    (76.1, 3.2),
    (88.9, 3.2),
    (76.1, 4.0),
    (88.9, 4.0),
    (114.3, 3.6),
    (88.9, 5.0),
    (114.3, 5.0),
    (139.7, 5.0),
    (114.3, 6.3),
    (168.3, 5.0),
    (139.7, 6.3),
    (193.7, 5.0),
    (168.3, 6.3),
    (193.7, 6.3),
    (168.3, 8.0),
    (219.1, 6.3),
    (193.7, 8.0),
    (219.1, 8.0),
    (219.1, 10.0),
];

/// The check of one post (Guide C 1.4). Moments in kNm, forces in kN.
#[derive(Debug, Clone, PartialEq)]
pub struct PostCheck {
    pub section: SteelChs,
    pub section_class: u8,
    /// ULS bending moment per post from wind, at the top of the foundation.
    pub wind_moment_knm: f64,
    /// ULS shear per post from wind.
    pub wind_shear_kn: f64,
    /// Bending moment from the point load at the top of the sign.
    pub point_load_moment_knm: f64,
    /// Torsion from the point load at the edge of the sign. Zero with more than one post.
    pub point_load_torsion_knm: f64,
    /// Mc,Rd / γm.
    pub moment_capacity_knm: f64,
    /// Vc,Rd / γm.
    pub shear_capacity_kn: f64,
    /// T,Rd / γm.
    pub torsion_capacity_knm: f64,
    /// Wind bending moment / capacity.
    pub bending_utilisation: f64,
    /// Wind shear / capacity.
    pub shear_utilisation: f64,
    /// Point load case: M/Mu + T/Tu.
    pub point_load_utilisation: f64,
    /// Temporary deflection at the top of the sign under the 1-year wind, mm.
    pub deflection_mm: f64,
    /// That deflection per metre of height (H + hb), mm/m.
    pub deflection_mm_per_m: f64,
    pub deflection_limit_mm_per_m: f64,
}

impl PostCheck {
    pub fn passes(&self) -> bool {
        self.bending_utilisation <= 1.0 && self.shear_utilisation <= 1.0 && self.point_load_utilisation <= 1.0 && self.deflection_mm_per_m <= self.deflection_limit_mm_per_m
    }
}

/// Checks a post section under the given wind forces.
pub fn check_post(sign: &SignGeometry, wind: &WindForces, section: SteelChs, factors: &DesignFactors) -> Result<PostCheck, SupportError> {
    sign.check()?;
    section.check()?;
    let n = f64::from(sign.post_count);
    let hb = sign.buried_above_foundation_mm / 1000.0;
    let h_total = sign.total_height_m() + hb;
    let class = section.class();
    let Some(mc) = section.moment_resistance_knm() else {
        return Err(SupportError::InvalidOption(format!(
            "{} is a class 4 (slender) section at fy = {} N/mm², which this check does not cover",
            section.label(),
            section.yield_n_mm2
        )));
    };
    let moment_capacity = mc / factors.gamma_m;
    let shear_capacity = section.shear_resistance_kn() / factors.gamma_m;
    let torsion_capacity = section.torsion_resistance_knm() / factors.gamma_m;

    let wind_moment = wind.uls_kn * (sign.centroid_height_m() + hb) / n;
    let wind_shear = wind.uls_kn / n;
    // Guide C 1.4.3: 0.5 kN at the top of the sign, and at its edge for torsion on a single
    // post. C 2.4.2: with two or more posts there is no torsional effect.
    let point_moment = factors.point_load_kn * h_total;
    let point_torsion = if sign.post_count == 1 { factors.point_load_kn * sign.width_mm / 1000.0 / 2.0 } else { 0.0 };

    // Guide C 1.4.8: cantilever carrying a uniform load over the height of the face.
    let w = wind.one_year_kn / (sign.height_mm / 1000.0); // kN/m = N/mm
    let (top, bottom) = (h_total * 1000.0, sign.mounting_height_mm + sign.buried_above_foundation_mm);
    let deflection = w / (24.0 * STEEL_E_N_MM2 * section.inertia_mm4() * n) * (3.0 * top.powi(4) - 4.0 * bottom.powi(3) * top + bottom.powi(4));

    Ok(PostCheck {
        section,
        section_class: class,
        wind_moment_knm: wind_moment,
        wind_shear_kn: wind_shear,
        point_load_moment_knm: point_moment,
        point_load_torsion_knm: point_torsion,
        moment_capacity_knm: moment_capacity,
        shear_capacity_kn: shear_capacity,
        torsion_capacity_knm: torsion_capacity,
        bending_utilisation: wind_moment / moment_capacity,
        shear_utilisation: wind_shear / shear_capacity,
        point_load_utilisation: point_moment / moment_capacity + point_torsion / torsion_capacity,
        deflection_mm: deflection,
        deflection_mm_per_m: deflection / h_total,
        deflection_limit_mm_per_m: factors.deflection_limit_mm_per_m,
    })
}

/// The lightest section of `sizes` (outside diameter, wall, mm) at the given yield strength
/// that passes [`check_post`]. Class 4 sections are passed over. None when nothing passes.
pub fn select_post(sign: &SignGeometry, wind: &WindForces, sizes: &[(f64, f64)], yield_n_mm2: f64, factors: &DesignFactors) -> Result<Option<PostCheck>, SupportError> {
    sign.check()?;
    let mut candidates: Vec<SteelChs> = sizes.iter().map(|&(d, t)| SteelChs { outside_diameter_mm: d, wall_mm: t, yield_n_mm2 }).collect();
    candidates.sort_by(|a, b| a.area_mm2().total_cmp(&b.area_mm2()));
    for section in candidates {
        section.check()?;
        if section.class() == 4 {
            continue;
        }
        let check = check_post(sign, wind, section, factors)?;
        if check.passes() {
            return Ok(Some(check));
        }
    }
    Ok(None)
}

/// Which diameter resists in the ground.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum PlantedMethod {
    /// PD 6547: D is the least diameter of the concrete or compacted backfill around the post.
    /// The Guide (5.19) recommends this where the road's standards permit it.
    Pd6547 { foundation_diameter_mm: f64 },
    /// CD 354 revision 1 (March 2020): D is the post diameter alone; the surround is ignored.
    Cd354,
}

/// The ground and the limits a planted foundation is designed to. Every value is the
/// engineer's: none is taken from a table held here.
#[derive(Debug, Clone, PartialEq)]
pub struct PlantedGround {
    pub method: PlantedMethod,
    /// Ground factor G, kN/m² per m (PD 6547 Table 2 / CD 354 Table 12.12).
    pub ground_factor_kn_m2_per_m: f64,
    /// Minimum planting depth for the sign's height, mm (BS EN 40-2 Table 7; CD 354 12.7).
    pub min_planting_depth_mm: f64,
}

/// The check of one planted foundation (Guide C 1.5). Moments in kNm.
#[derive(Debug, Clone, PartialEq)]
pub struct PlantedCheck {
    pub method: PlantedMethod,
    /// The diameter D used in Mg, mm.
    pub effective_diameter_mm: f64,
    pub planting_depth_mm: f64,
    pub ground_factor_kn_m2_per_m: f64,
    /// Mg = G·D·P³/10.
    pub ground_resistance_knm: f64,
    /// MDS = Fw,d(SLS) · (z + hb + P/√2) / n, unfactored.
    pub destabilising_knm: f64,
    /// γs;d · MDS.
    pub factored_destabilising_knm: f64,
    pub utilisation: f64,
    pub meets_min_depth: bool,
    pub meets_depth_to_diameter: bool,
}

impl PlantedCheck {
    pub fn passes(&self) -> bool {
        self.utilisation <= 1.0 && self.meets_min_depth && self.meets_depth_to_diameter
    }
}

/// Checks a planted foundation of the given planting depth on level ground.
pub fn check_planted(
    sign: &SignGeometry,
    wind: &WindForces,
    post_diameter_mm: f64,
    ground: &PlantedGround,
    planting_depth_mm: f64,
    factors: &DesignFactors,
) -> Result<PlantedCheck, SupportError> {
    sign.check()?;
    let d_mm = match ground.method {
        PlantedMethod::Pd6547 { foundation_diameter_mm } => foundation_diameter_mm,
        PlantedMethod::Cd354 => post_diameter_mm,
    };
    for (name, v) in [("foundation diameter", d_mm), ("ground factor G", ground.ground_factor_kn_m2_per_m), ("planting depth", planting_depth_mm)] {
        if !(v.is_finite() && v > 0.0) {
            return Err(SupportError::InvalidOption(format!("the {name} must be greater than zero (it is {v})")));
        }
    }
    if let PlantedMethod::Pd6547 { foundation_diameter_mm } = ground.method {
        if foundation_diameter_mm < post_diameter_mm {
            return Err(SupportError::InvalidOption(format!(
                "a foundation {foundation_diameter_mm} mm in diameter cannot surround a post {post_diameter_mm} mm in diameter"
            )));
        }
    }
    let (d, p) = (d_mm / 1000.0, planting_depth_mm / 1000.0);
    let mg = ground.ground_factor_kn_m2_per_m * d * p.powi(3) / 10.0;
    let lever = sign.centroid_height_m() + sign.buried_above_foundation_mm / 1000.0 + p / SQRT_2;
    let mds = wind.sls_kn * lever / f64::from(sign.post_count);
    let factored = factors.overturning_factor * mds;
    Ok(PlantedCheck {
        method: ground.method,
        effective_diameter_mm: d_mm,
        planting_depth_mm,
        ground_factor_kn_m2_per_m: ground.ground_factor_kn_m2_per_m,
        ground_resistance_knm: mg,
        destabilising_knm: mds,
        factored_destabilising_knm: factored,
        utilisation: factored / mg,
        meets_min_depth: planting_depth_mm >= ground.min_planting_depth_mm,
        meets_depth_to_diameter: planting_depth_mm >= factors.min_depth_to_diameter * d_mm,
    })
}

/// Deepest planting depth [`size_planted`] will try, mm. Beyond this a planted foundation is
/// unlikely to be buildable and a spread foundation should be considered (Guide 5.24).
pub const MAX_PLANTING_DEPTH_MM: f64 = 3000.0;

/// The shallowest planting depth, in steps of 50 mm, that passes [`check_planted`]. None when
/// nothing up to [`MAX_PLANTING_DEPTH_MM`] passes.
pub fn size_planted(sign: &SignGeometry, wind: &WindForces, post_diameter_mm: f64, ground: &PlantedGround, factors: &DesignFactors) -> Result<Option<PlantedCheck>, SupportError> {
    let step = 50.0;
    let mut depth = (ground.min_planting_depth_mm.max(step) / step).ceil() * step;
    while depth <= MAX_PLANTING_DEPTH_MM {
        let check = check_planted(sign, wind, post_diameter_mm, ground, depth, factors)?;
        if check.passes() {
            return Ok(Some(check));
        }
        depth += step;
    }
    Ok(None)
}

/// Assumed ground for a spread foundation: the rows of the Guide's Table 5.2, with the design
/// soil strengths of Table 5.3 (B3). Using them carries the field inspection and testing
/// requirements of Table 5.3 and Figure 5.3 (dynamic cone penetrometer tests in each corner of
/// the formation, no peat above founding level, water table at or below founding level).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AssumedGround {
    Poor,
    Average,
    Good,
}

impl AssumedGround {
    /// (design bearing resistance DA1C2, DA1C1, kPa; design shear strength cu;d DA1C2, DA1C1, kPa).
    fn values(self) -> (f64, f64, f64, f64) {
        match self {
            AssumedGround::Poor => (100.0, 135.0, 18.0, 25.0),
            AssumedGround::Average => (150.0, 205.0, 29.0, 41.0),
            AssumedGround::Good => (200.0, 275.0, 40.0, 56.0),
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            AssumedGround::Poor => "poor",
            AssumedGround::Average => "average",
            AssumedGround::Good => "good",
        }
    }
}

/// Unit weight of reinforced concrete the Guide's Example 2 uses, kN/m³.
pub const CONCRETE_UNIT_WEIGHT_KN_M3: f64 = 24.0;
/// Unit weight of backfill above the foundation the Guide's Example 2 uses, kN/m³.
pub const BACKFILL_UNIT_WEIGHT_KN_M3: f64 = 20.0;
/// Table 5.3 limits: eccentricity e ≤ L/3, bearing width L′ ≥ 0.5 m, cohesive horizontal load
/// ratio ≤ 0.36, granular Hd/Wd ≤ 0.15, founding depth ≥ 0.5 m, sign top no more than 4 m.
const SPREAD_MIN_BEARING_WIDTH_M: f64 = 0.5;
const SPREAD_COHESIVE_LIMIT: f64 = 0.36;
const SPREAD_GRANULAR_LIMIT: f64 = 0.15;
const SPREAD_MIN_FOUNDING_DEPTH_M: f64 = 0.5;
const SPREAD_MAX_SIGN_HEIGHT_M: f64 = 4.0;

/// One combination of the spread foundation check (Guide C 2.5.5, 2.5.6).
#[derive(Debug, Clone, PartialEq)]
pub struct SpreadCase {
    pub name: &'static str,
    pub gamma_q: f64,
    pub gamma_g: f64,
    /// Design vertical load Wd, kN.
    pub vertical_kn: f64,
    /// Design destabilising moment Ed, kNm.
    pub moment_knm: f64,
    /// e = Ed / Wd, m.
    pub eccentricity_m: f64,
    /// L′ = L − 2e, m.
    pub bearing_width_m: f64,
    /// Vd = Wd / (L′ · w), kPa.
    pub bearing_kpa: f64,
    /// Assumed design bearing resistance, kPa.
    pub resistance_kpa: f64,
    /// Hd = Frep · γQ, kN.
    pub horizontal_kn: f64,
    /// Hd / (L′ · w · cu;d).
    pub cohesive_ratio: f64,
    /// Hd / Wd, checked in Combination 2 only (Table 5.3 note 3).
    pub granular_ratio: Option<f64>,
    pub eccentricity_ok: bool,
}

impl SpreadCase {
    pub fn passes(&self) -> bool {
        self.eccentricity_ok
            && self.bearing_width_m >= SPREAD_MIN_BEARING_WIDTH_M
            && self.bearing_kpa <= self.resistance_kpa
            && self.cohesive_ratio <= SPREAD_COHESIVE_LIMIT
            && self.granular_ratio.is_none_or(|r| r <= SPREAD_GRANULAR_LIMIT)
    }
}

/// The check of one spread foundation carrying the whole sign (Guide section 5 and C 2.5),
/// using assumed design bearing resistances. Geotechnical checks only: the concrete and its
/// reinforcement (C 2.6) and the fixing of the posts are not designed.
#[derive(Debug, Clone, PartialEq)]
pub struct SpreadCheck {
    pub ground: AssumedGround,
    /// w, parallel to the sign face, mm.
    pub breadth_mm: f64,
    /// L, perpendicular to the sign face, mm.
    pub length_mm: f64,
    /// T, mm.
    pub thickness_mm: f64,
    /// Characteristic weight with and without the backfill above, kN.
    pub weight_with_backfill_kn: f64,
    pub weight_without_backfill_kn: f64,
    /// Ek = Frep · (z + hb + T), kNm.
    pub characteristic_moment_knm: f64,
    /// DA1C1 with the weight stabilising, DA1C1 with it destabilising, DA1C2.
    pub cases: [SpreadCase; 3],
    /// Table 5.3 A1: top of the sign no more than 4 m above ground.
    pub meets_height: bool,
    /// Table 5.3 A2: founding depth at least 0.5 m.
    pub meets_founding_depth: bool,
}

impl SpreadCheck {
    pub fn passes(&self) -> bool {
        self.meets_height && self.meets_founding_depth && self.cases.iter().all(SpreadCase::passes)
    }
}

/// Checks a spread foundation `breadth` (parallel to the face) by `length` by `thickness`, with
/// its top `hb` below ground, on level ground with the water table at or below founding level.
pub fn check_spread(sign: &SignGeometry, wind: &WindForces, breadth_mm: f64, length_mm: f64, thickness_mm: f64, ground: AssumedGround) -> Result<SpreadCheck, SupportError> {
    sign.check()?;
    for (name, v) in [("foundation breadth", breadth_mm), ("foundation length", length_mm), ("foundation thickness", thickness_mm)] {
        if !(v.is_finite() && v > 0.0) {
            return Err(SupportError::InvalidOption(format!("the {name} must be greater than zero (it is {v})")));
        }
    }
    let (w, l, t, hb) = (breadth_mm / 1000.0, length_mm / 1000.0, thickness_mm / 1000.0, sign.buried_above_foundation_mm / 1000.0);
    let without = w * l * t * CONCRETE_UNIT_WEIGHT_KN_M3;
    let with = without + w * l * hb * BACKFILL_UNIT_WEIGHT_KN_M3;
    let frep = wind.sls_kn;
    let ek = frep * (sign.centroid_height_m() + hb + t);
    let (r2, r1, cu2, cu1) = ground.values();
    // Partial factors of the Guide's Table 5.1 (UK National Annexes to BS EN 1990 and 1997).
    let case = |name, gamma_q: f64, gamma_g: f64, wk: f64, resistance: f64, cu: f64, granular: bool| {
        let (wd, ed, hd) = (wk * gamma_g, ek * gamma_q, frep * gamma_q);
        let e = ed / wd;
        let l_eff = l - 2.0 * e;
        let area = l_eff * w;
        SpreadCase {
            name,
            gamma_q,
            gamma_g,
            vertical_kn: wd,
            moment_knm: ed,
            eccentricity_m: e,
            bearing_width_m: l_eff,
            bearing_kpa: if l_eff > 0.0 { wd / area } else { f64::INFINITY },
            resistance_kpa: resistance,
            horizontal_kn: hd,
            cohesive_ratio: if l_eff > 0.0 { hd / (area * cu) } else { f64::INFINITY },
            granular_ratio: granular.then_some(hd / wd),
            eccentricity_ok: e <= l / 3.0,
        }
    };
    Ok(SpreadCheck {
        ground,
        breadth_mm,
        length_mm,
        thickness_mm,
        weight_with_backfill_kn: with,
        weight_without_backfill_kn: without,
        characteristic_moment_knm: ek,
        cases: [
            case("DA1C1, weight stabilising", 1.5, 1.0, without, r1, cu1, false),
            case("DA1C1, weight destabilising", 1.5, 1.35, with, r1, cu1, false),
            case("DA1C2", 1.3, 1.0, with, r2, cu2, true),
        ],
        meets_height: sign.total_height_m() <= SPREAD_MAX_SIGN_HEIGHT_M + 1e-9,
        meets_founding_depth: hb + t >= SPREAD_MIN_FOUNDING_DEPTH_M - 1e-9,
    })
}

/// Longest spread foundation [`size_spread`] will try, mm.
pub const MAX_SPREAD_LENGTH_MM: f64 = 6000.0;

/// The shortest length L, in steps of 50 mm, that passes [`check_spread`] for the given breadth
/// and thickness (the Guide, 5.13: least depth and breadth, then lengthen for stability). None
/// when nothing up to [`MAX_SPREAD_LENGTH_MM`] passes, or the sign is outside Table 5.3.
pub fn size_spread(sign: &SignGeometry, wind: &WindForces, breadth_mm: f64, thickness_mm: f64, ground: AssumedGround) -> Result<Option<SpreadCheck>, SupportError> {
    let mut length = 500.0;
    while length <= MAX_SPREAD_LENGTH_MM {
        let check = check_spread(sign, wind, breadth_mm, length, thickness_mm, ground)?;
        if !(check.meets_height && check.meets_founding_depth) {
            return Ok(None);
        }
        if check.passes() {
            return Ok(Some(check));
        }
        length += 50.0;
    }
    Ok(None)
}

/// The foundation a design asks for.
#[derive(Debug, Clone, PartialEq)]
pub enum FoundationRequest {
    /// One planted foundation under each post.
    Planted(PlantedGround),
    /// One spread foundation under the whole sign, of the given breadth and thickness.
    Spread { breadth_mm: f64, thickness_mm: f64, ground: AssumedGround },
}

/// The foundation a design arrives at.
#[derive(Debug, Clone, PartialEq)]
pub enum FoundationCheck {
    Planted(PlantedCheck),
    Spread(SpreadCheck),
}

impl FoundationCheck {
    pub fn passes(&self) -> bool {
        match self {
            FoundationCheck::Planted(c) => c.passes(),
            FoundationCheck::Spread(c) => c.passes(),
        }
    }
}

/// A complete support design for one sign.
#[derive(Debug, Clone, PartialEq)]
pub struct SupportDesign {
    pub sign: SignGeometry,
    pub wind: WindForces,
    pub post: PostCheck,
    pub foundation: FoundationCheck,
    /// Things the engineer must confirm or that fall outside the method.
    pub warnings: Vec<String>,
}

/// Sizes the posts from `sizes` and the foundation for the sign.
pub fn design_supports(
    sign: &SignGeometry,
    basic_pressure_kn_m2: f64,
    sizes: &[(f64, f64)],
    yield_n_mm2: f64,
    foundation: &FoundationRequest,
    factors: &DesignFactors,
) -> Result<SupportDesign, SupportError> {
    let wind = wind_forces(sign, basic_pressure_kn_m2, factors)?;
    let post = select_post(sign, &wind, sizes, yield_n_mm2, factors)?
        .ok_or_else(|| SupportError::InvalidOption(format!("no section in the list carries this sign on {} post(s): add posts or offer larger sections", sign.post_count)))?;
    let mut warnings = Vec::new();
    let foundation = match foundation {
        FoundationRequest::Planted(ground) => {
            let check = size_planted(sign, &wind, post.section.outside_diameter_mm, ground, factors)?.ok_or_else(|| {
                SupportError::InvalidOption(format!(
                    "no planted foundation up to {} m deep is stable in this ground: use a larger diameter or more posts, or design a spread foundation",
                    MAX_PLANTING_DEPTH_MM / 1000.0
                ))
            })?;
            warnings.push(format!(
                "Confirm the minimum planting depth of {} mm against BS EN 40-2 Table 7 for a height of {:.2} m; that table is not held in AfroBIM.",
                ground.min_planting_depth_mm,
                sign.total_height_m()
            ));
            warnings.push(format!("Confirm the ground factor G = {} kN/m² per m against the ground on site.", ground.ground_factor_kn_m2_per_m));
            FoundationCheck::Planted(check)
        }
        FoundationRequest::Spread { breadth_mm, thickness_mm, ground } => {
            let check = size_spread(sign, &wind, *breadth_mm, *thickness_mm, *ground)?.ok_or_else(|| {
                SupportError::InvalidOption(format!(
                    "no spread foundation of this breadth and thickness up to {} m long meets the Guide's Table 5.3 (sign top no more than 4 m, founding depth at least 0.5 m, eccentricity, bearing and horizontal load): change the breadth or thickness, or take specialist advice",
                    MAX_SPREAD_LENGTH_MM / 1000.0
                ))
            })?;
            warnings.push(format!(
                "The {} ground bearing resistance is assumed (Guide Table 5.2). Confirm it on site by the inspection and DCP testing of Table 5.3 and Figure 5.3, with the water table at or below founding level.",
                ground.label()
            ));
            warnings.push("Geotechnical checks only: the concrete, its reinforcement (Guide C 2.6) and the fixing of the posts into the foundation are not designed.".to_string());
            warnings.push("The weight of the posts and the sign is neglected, as in the Guide's Example 2.".to_string());
            FoundationCheck::Spread(check)
        }
    };
    warnings.push("Level ground is assumed, with no wind funnelling or significant topography (Guide sections 5.21 and 6).".to_string());
    warnings.push("Member capacities are calculated from the nominal tube, not taken from a manufacturer's or published table.".to_string());
    if sign.total_height_m() > 7.0 {
        warnings.push("The sign is more than 7 m high: the Guide (3.21) calls for the rigorous wind method and structural certification.".to_string());
    }
    if sign.post_count == 1 {
        warnings.push("Wind torsion from a sign fixed eccentrically on its post is not checked.".to_string());
    }
    Ok(SupportDesign { sign: sign.clone(), wind, post, foundation, warnings })
}

impl SupportDesign {
    /// The calculation set out line by line with its references, for the engineer to check.
    pub fn calculation(&self, wind_source: &str) -> Vec<String> {
        let (s, w, p) = (&self.sign, &self.wind, &self.post);
        let n = s.post_count;
        let hb = s.buried_above_foundation_mm / 1000.0;
        let ok = |pass: bool| if pass { "OK" } else { "FAILS" };
        let mut out = vec![
            "Design basis: IHE Sign Structures Guide 2021, BS EN 12899-1:2007 and its UK National Annex.".to_string(),
            format!("Sign: l = {:.3} m, b = {:.3} m, hm = {:.3} m, H = {:.3} m, z = {:.3} m, hb = {:.3} m, n = {}.", s.width_mm / 1000.0, s.height_mm / 1000.0, s.mounting_height_mm / 1000.0, s.total_height_m(), s.centroid_height_m(), hb, n),
            format!("Basic wind pressure wb = {:.2} kN/m² ({wind_source}).", w.basic_pressure_kn_m2),
            format!("Aspect ratio = {:.2}, cf = {:.3} (BS EN 12899-1 Table NA.2).", w.aspect_ratio, w.cf),
            format!("Fw = cf · wb · Aref = {:.3} × {:.2} × {:.3} = {:.2} kN.", w.cf, w.basic_pressure_kn_m2, w.area_m2, w.characteristic_kn),
            format!("Fw,d (ULS) = {:.2} kN; Fw,d (SLS) = {:.2} kN; Fw,d (1 year) = {:.2} kN.", w.uls_kn, w.sls_kn, w.one_year_kn),
            format!("Post: {} with fy = {} N/mm², class {} (BS EN 1993-1-1 Table 5.2).", p.section.label(), p.section.yield_n_mm2, p.section_class),
            format!("Md = Fw,d (ULS) · (z + hb) / n = {:.2} kNm; Mc,Rd / γm = {:.2} kNm; utilisation {:.2}: {}.", p.wind_moment_knm, p.moment_capacity_knm, p.bending_utilisation, ok(p.bending_utilisation <= 1.0)),
            format!("Vd = Fw,d (ULS) / n = {:.2} kN; Vc,Rd / γm = {:.1} kN; utilisation {:.2}: {}.", p.wind_shear_kn, p.shear_capacity_kn, p.shear_utilisation, ok(p.shear_utilisation <= 1.0)),
            format!("Point load: M = {:.2} kNm, T = {:.3} kNm; M/Mu + T/Tu = {:.2}: {}.", p.point_load_moment_knm, p.point_load_torsion_knm, p.point_load_utilisation, ok(p.point_load_utilisation <= 1.0)),
            format!("Temporary deflection at the top of the sign = {:.1} mm = {:.2} mm/m against {} mm/m (class TDB4): {}.", p.deflection_mm, p.deflection_mm_per_m, p.deflection_limit_mm_per_m, ok(p.deflection_mm_per_m <= p.deflection_limit_mm_per_m)),
        ];
        match &self.foundation {
            FoundationCheck::Planted(f) => {
                let method = match f.method {
                    PlantedMethod::Pd6547 { .. } => "PD 6547 (D = diameter of the concrete surround)",
                    PlantedMethod::Cd354 => "CD 354 rev 1 (D = diameter of the post)",
                };
                out.push(format!("Planted foundation to {method}: D = {:.3} m, P = {:.2} m, G = {} kN/m² per m.", f.effective_diameter_mm / 1000.0, f.planting_depth_mm / 1000.0, f.ground_factor_kn_m2_per_m));
                out.push(format!("Mg = G · D · P³ / 10 = {:.2} kNm; MDS = Fw,d (SLS) · (z + hb + P/√2) / n = {:.2} kNm; 1.25 · MDS = {:.2} kNm: {}.", f.ground_resistance_knm, f.destabilising_knm, f.factored_destabilising_knm, ok(f.utilisation <= 1.0)));
                out.push(format!("Planting depth against the minimum: {}; against twice the diameter: {}.", ok(f.meets_min_depth), ok(f.meets_depth_to_diameter)));
            }
            FoundationCheck::Spread(f) => {
                out.push(format!("Spread foundation, one under the whole sign (Guide section 5): w = {:.2} m, L = {:.2} m, T = {:.2} m, {} ground assumed.", f.breadth_mm / 1000.0, f.length_mm / 1000.0, f.thickness_mm / 1000.0, f.ground.label()));
                out.push(format!("Wk = {:.1} kN with backfill, {:.1} kN without; Ek = Frep · (z + hb + T) = {:.2} × {:.3} = {:.2} kNm.", f.weight_with_backfill_kn, f.weight_without_backfill_kn, w.sls_kn, s.centroid_height_m() + hb + f.thickness_mm / 1000.0, f.characteristic_moment_knm));
                for c in &f.cases {
                    let granular = c.granular_ratio.map(|r| format!("; Hd/Wd = {r:.2} ≤ 0.15")).unwrap_or_default();
                    out.push(format!(
                        "{} (γQ = {}, γG = {}): Ed = {:.2} kNm, Wd = {:.1} kN, e = {:.2} m ≤ L/3 = {:.2} m, L' = {:.2} m; Vd = {:.1} kPa ≤ {} kPa; Hd/(L'·w·cu;d) = {:.2} ≤ 0.36{granular}: {}.",
                        c.name, c.gamma_q, c.gamma_g, c.moment_knm, c.vertical_kn, c.eccentricity_m, f.length_mm / 3000.0, c.bearing_width_m, c.bearing_kpa, c.resistance_kpa, c.cohesive_ratio, ok(c.passes())
                    ));
                }
                out.push(format!("Table 5.3: sign top no more than 4 m: {}; founding depth at least 0.5 m: {}.", ok(f.meets_height), ok(f.meets_founding_depth)));
            }
        }
        out.extend(self.warnings.iter().map(|w| format!("Check: {w}")));
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn close(actual: f64, expected: f64, tolerance: f64) {
        assert!((actual - expected).abs() <= tolerance, "expected {expected} ± {tolerance}, got {actual}");
    }

    /// IHE Sign Structures Guide 2021, Appendix C, Example 1.
    fn example_1() -> SignGeometry {
        SignGeometry {
            width_mm: 900.0,
            height_mm: 900.0,
            mounting_height_mm: 2000.0,
            face_area_m2: Some(PI * 0.45 * 0.45),
            buried_above_foundation_mm: 0.0,
            post_count: 1,
        }
    }

    /// Example 2: 4.0 m × 2.5 m on two posts, hb = 75 mm.
    fn example_2() -> SignGeometry {
        SignGeometry { width_mm: 4000.0, height_mm: 2500.0, mounting_height_mm: 1500.0, face_area_m2: None, buried_above_foundation_mm: 75.0, post_count: 2 }
    }

    #[test]
    fn force_coefficients_match_table_na2() {
        for (aspect, cf) in FORCE_COEFFICIENTS {
            close(force_coefficient(aspect, 1.0), cf, 1e-12);
            close(force_coefficient(1.0, aspect), cf, 1e-12);
        }
        close(force_coefficient(2.3, 1.0), 1.325, 1e-12); // halfway between 1.6 and 3
        close(force_coefficient(50.0, 1.0), 1.8, 1e-12);
    }

    #[test]
    fn mowt_table_2_rows() {
        assert_eq!(mowt_velocity_pressure(2.9, MowtTerrain::Open), Some(0.81));
        assert_eq!(mowt_velocity_pressure(5.0, MowtTerrain::BuiltUp), Some(0.37));
        assert_eq!(mowt_velocity_pressure(5.1, MowtTerrain::Open), Some(0.92));
        assert_eq!(mowt_velocity_pressure(30.0, MowtTerrain::BuiltUp), Some(0.76));
        assert_eq!(mowt_velocity_pressure(30.1, MowtTerrain::Open), None);
    }

    #[test]
    fn example_1_wind_force() {
        let w = wind_forces(&example_1(), 1.0, &DesignFactors::default()).unwrap();
        close(w.cf, 1.26, 1e-12);
        close(w.characteristic_kn, 0.80, 0.005); // 1.3.2
        close(w.uls_kn, 1.08, 0.005); // 1.3.4
        close(w.one_year_kn, 0.488, 0.002); // 1.3.5
    }

    #[test]
    fn section_properties_against_published_data() {
        // The Guide quotes I = 96.3 cm⁴, Mc,Rd = 10.30 kNm, Vc,Rd = 140.0 kN for CHS 88.9 × 4.0
        // and I = 856 cm⁴, Mc,Rd = 47.2 kNm, Vc,Rd = 335.0 kN for CHS 168.3 × 5.0, both S355.
        let a = SteelChs::s355(88.9, 4.0);
        close(a.inertia_mm4() / 1.0e4, 96.3, 0.1);
        close(a.moment_resistance_knm().unwrap(), 10.30, 0.10);
        close(a.shear_resistance_kn(), 140.0, 1.5);
        assert_eq!(a.class(), 1);
        let b = SteelChs::s355(168.3, 5.0);
        close(b.inertia_mm4() / 1.0e4, 856.0, 1.0);
        close(b.moment_resistance_knm().unwrap(), 47.2, 0.3);
        close(b.shear_resistance_kn(), 335.0, 3.0);
        assert_eq!(b.class(), 2);
    }

    #[test]
    fn example_1_post() {
        let f = DesignFactors::default();
        let sign = example_1();
        let w = wind_forces(&sign, 1.0, &f).unwrap();
        let p = check_post(&sign, &w, SteelChs::s355(88.9, 4.0), &f).unwrap();
        close(p.wind_moment_knm, 2.65, 0.01); // 1.4.2
        close(p.wind_shear_kn, 1.08, 0.005);
        close(p.point_load_moment_knm, 1.45, 1e-9); // 1.4.3
        close(p.point_load_torsion_knm, 0.225, 1e-9);
        close(p.moment_capacity_knm, 9.80, 0.10); // 1.4.6, from first principles
        close(p.deflection_mm, 15.06, 0.10); // 1.4.8
        close(p.deflection_mm_per_m, 5.19, 0.04);
        assert!(p.passes());
    }

    #[test]
    fn example_2_post_with_the_guides_forces() {
        // 2.4 uses the BS EN 1991-1-4 forces: 17.6 kN at ULS, 7.94 kN at 1 year.
        let f = DesignFactors::default();
        let sign = example_2();
        let mut w = wind_forces(&sign, 1.2, &f).unwrap();
        close(w.cf, 1.30, 1e-12);
        close(w.characteristic_kn, 15.6, 1e-9); // 2.3.2
        close(w.uls_kn, 21.06, 1e-9); // 2.3.4 (printed as 21.1)
        close(w.one_year_kn, 9.52, 0.005); // 2.3.5
        w.uls_kn = 17.6;
        w.one_year_kn = 7.94;
        let p = check_post(&sign, &w, SteelChs::s355(168.3, 5.0), &f).unwrap();
        close(p.wind_moment_knm, 24.86, 0.005); // 2.4.2
        close(p.wind_shear_kn, 8.8, 1e-9);
        // 2.4.6 prints 28.17 mm and 6.91 mm/m, but its own expression with its own numbers
        // (3.18 kN/m, I = 856 cm⁴) evaluates to 28.37 mm, i.e. 6.96 mm/m.
        close(p.deflection_mm, 28.37, 0.05);
        close(p.deflection_mm_per_m, 6.96, 0.02);
        assert_eq!(p.point_load_torsion_knm, 0.0);
        assert!(p.passes());
    }

    #[test]
    fn example_1_planted_foundation_both_methods() {
        let f = DesignFactors::default();
        let sign = example_1();
        let w = wind_forces(&sign, 1.0, &f).unwrap();
        // 1.5.1 to 1.5.4, PD 6547: D = 0.4 m, P = 0.8 m, G = 230.
        let pd = PlantedGround { method: PlantedMethod::Pd6547 { foundation_diameter_mm: 400.0 }, ground_factor_kn_m2_per_m: 230.0, min_planting_depth_mm: 800.0 };
        let c = check_planted(&sign, &w, 88.9, &pd, 800.0, &f).unwrap();
        close(c.ground_resistance_knm, 4.71, 0.005);
        close(c.destabilising_knm, 2.41, 0.01);
        close(c.factored_destabilising_knm, 3.01, 0.015);
        assert!(c.passes());
        // 1.5.6 to 1.5.9, CD 354: D = 0.089 m, P = 1.2 m.
        let cd = PlantedGround { method: PlantedMethod::Cd354, ground_factor_kn_m2_per_m: 230.0, min_planting_depth_mm: 800.0 };
        let c = check_planted(&sign, &w, 88.9, &cd, 1200.0, &f).unwrap();
        close(c.ground_resistance_knm, 3.53, 0.01);
        close(c.destabilising_knm, 2.64, 0.01);
        close(c.factored_destabilising_knm, 3.30, 0.015);
        assert!(c.passes());
        // The sizing finds the same depths the Guide settles on.
        assert_eq!(size_planted(&sign, &w, 88.9, &pd, &f).unwrap().unwrap().planting_depth_mm, 800.0);
        assert_eq!(size_planted(&sign, &w, 88.9, &cd, &f).unwrap().unwrap().planting_depth_mm, 1200.0);
    }

    #[test]
    fn a_shallow_wide_foundation_is_not_a_planted_one() {
        let f = DesignFactors::default();
        let sign = example_1();
        let w = wind_forces(&sign, 1.0, &f).unwrap();
        let g = PlantedGround { method: PlantedMethod::Pd6547 { foundation_diameter_mm: 600.0 }, ground_factor_kn_m2_per_m: 230.0, min_planting_depth_mm: 800.0 };
        let c = check_planted(&sign, &w, 88.9, &g, 800.0, &f).unwrap();
        assert!(c.utilisation <= 1.0 && !c.meets_depth_to_diameter && !c.passes());
        assert_eq!(size_planted(&sign, &w, 88.9, &g, &f).unwrap().unwrap().planting_depth_mm, 1200.0);
    }

    #[test]
    fn design_picks_the_lightest_section_that_passes() {
        let f = DesignFactors::default();
        let g = PlantedGround { method: PlantedMethod::Pd6547 { foundation_diameter_mm: 400.0 }, ground_factor_kn_m2_per_m: 230.0, min_planting_depth_mm: 800.0 };
        let g = FoundationRequest::Planted(g);
        let d = design_supports(&example_1(), 1.0, &COMMON_CHS_MM, S355_YIELD_N_MM2, &g, &f).unwrap();
        // The Guide notes that a smaller section than 88.9 × 4.0 might also be suitable.
        assert_eq!((d.post.section.outside_diameter_mm, d.post.section.wall_mm), (76.1, 3.2));
        assert!(d.post.passes() && d.foundation.passes());
        assert!(d.calculation("test").iter().all(|line| !line.contains("FAILS")));
        // A heavier sign needs more: every lighter section must fail where the chosen one passes.
        let big = design_supports(&example_2(), 1.2, &COMMON_CHS_MM, S355_YIELD_N_MM2, &g, &f).unwrap();
        let chosen = big.post.section.area_mm2();
        for &(dia, t) in COMMON_CHS_MM.iter() {
            let s = SteelChs::s355(dia, t);
            if s.area_mm2() < chosen && s.class() < 4 {
                assert!(!check_post(&big.sign, &big.wind, s, &f).unwrap().passes(), "{} is lighter and passes", s.label());
            }
        }
    }

    #[test]
    fn example_2_spread_foundation() {
        // C 2.5: Frep = 13.0 kN, w = 3.40 m, L = 2.10 m, T = 1.50 m, poor ground.
        let sign = example_2();
        let mut w = wind_forces(&sign, 1.2, &DesignFactors::default()).unwrap();
        w.sls_kn = 13.0;
        let c = check_spread(&sign, &w, 3400.0, 2100.0, 1500.0, AssumedGround::Poor).unwrap();
        close(c.weight_with_backfill_kn, 268.0, 0.5); // 2.5.3
        close(c.weight_without_backfill_kn, 257.0, 0.1);
        close(c.characteristic_moment_knm, 56.23, 0.01);
        // DA1C1, weight stabilising. The Guide prints Ed = 83.34 kNm for 56.23 × 1.50, which is
        // 84.34; its e = 0.33 m, L′ = 1.44 m, 52.4 kPa and 0.16 agree with the correct product.
        let a = &c.cases[0];
        close(a.moment_knm, 84.34, 0.01);
        close(a.eccentricity_m, 0.33, 0.005);
        close(a.bearing_width_m, 1.44, 0.01);
        close(a.bearing_kpa, 52.4, 0.2);
        close(a.cohesive_ratio, 0.16, 0.005);
        assert_eq!(a.granular_ratio, None);
        // DA1C1, weight destabilising. The Guide's example applies γG = 1.3 (and prints three
        // different values of Wd); its own Table 5.1 gives γG,sup = 1.35, which is used here:
        // Wd = 267.75 × 1.35 = 361.5 kN, e = 84.34 / 361.5 = 0.233 m, Vd = 361.5 / (1.633 × 3.4).
        let b = &c.cases[1];
        close(b.vertical_kn, 361.46, 0.01);
        close(b.eccentricity_m, 0.2333, 0.0005);
        close(b.bearing_kpa, 65.1, 0.1);
        // DA1C2: 2.5.6.
        let d = &c.cases[2];
        close(d.moment_knm, 73.09, 0.01);
        close(d.eccentricity_m, 0.27, 0.005);
        close(d.bearing_width_m, 1.55, 0.01);
        close(d.bearing_kpa, 50.7, 0.1);
        close(d.cohesive_ratio, 0.18, 0.005);
        close(d.granular_ratio.unwrap(), 0.06, 0.005);
        assert!(c.passes());
        // "A smaller foundation ... might also be suitable": the sizing finds a shorter one,
        // and the next step down from it fails.
        let sized = size_spread(&sign, &w, 3400.0, 1500.0, AssumedGround::Poor).unwrap().unwrap();
        assert!(sized.length_mm < 2100.0 && sized.passes());
        assert!(!check_spread(&sign, &w, 3400.0, sized.length_mm - 50.0, 1500.0, AssumedGround::Poor).unwrap().passes());
    }

    #[test]
    fn spread_foundations_outside_table_5_3_are_refused() {
        let f = DesignFactors::default();
        let mut tall = example_2();
        tall.mounting_height_mm = 2000.0; // top at 4.5 m
        let w = wind_forces(&tall, 1.2, &f).unwrap();
        assert!(!check_spread(&tall, &w, 3400.0, 2100.0, 1500.0, AssumedGround::Poor).unwrap().meets_height);
        assert_eq!(size_spread(&tall, &w, 3400.0, 1500.0, AssumedGround::Poor).unwrap(), None);
        let request = FoundationRequest::Spread { breadth_mm: 3400.0, thickness_mm: 1500.0, ground: AssumedGround::Poor };
        assert!(design_supports(&tall, 1.2, &COMMON_CHS_MM, S355_YIELD_N_MM2, &request, &f).is_err());
        // A thin foundation at ground level is shallower than 0.5 m.
        let mut shallow = example_2();
        shallow.buried_above_foundation_mm = 0.0;
        let w = wind_forces(&shallow, 1.2, &f).unwrap();
        assert!(!check_spread(&shallow, &w, 3400.0, 2100.0, 400.0, AssumedGround::Good).unwrap().meets_founding_depth);
        // Within the table, the design goes through and the calculation shows no failure.
        let d = design_supports(&example_2(), 1.2, &COMMON_CHS_MM, S355_YIELD_N_MM2, &request, &f).unwrap();
        assert!(d.foundation.passes());
        assert!(d.calculation("test").iter().all(|line| !line.contains("FAILS")));
    }

    #[test]
    fn the_common_sizes_are_in_increasing_mass() {
        let areas: Vec<f64> = COMMON_CHS_MM.iter().map(|&(d, t)| SteelChs::s355(d, t).area_mm2()).collect();
        assert!(areas.windows(2).all(|w| w[0] < w[1]), "{areas:?}");
    }

    #[test]
    fn invalid_inputs_are_refused() {
        let f = DesignFactors::default();
        assert!(wind_forces(&example_1(), 0.0, &f).is_err());
        let mut none = example_1();
        none.post_count = 0;
        assert!(wind_forces(&none, 1.0, &f).is_err());
        let w = wind_forces(&example_1(), 1.0, &f).unwrap();
        assert!(check_post(&example_1(), &w, SteelChs::s355(88.9, 50.0), &f).is_err());
        assert!(check_post(&example_1(), &w, SteelChs::s355(219.1, 3.0), &f).is_err()); // class 4
        let g = PlantedGround { method: PlantedMethod::Pd6547 { foundation_diameter_mm: 50.0 }, ground_factor_kn_m2_per_m: 230.0, min_planting_depth_mm: 800.0 };
        assert!(check_planted(&example_1(), &w, 88.9, &g, 800.0, &f).is_err());
    }
}
