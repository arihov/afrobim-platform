//! Reduction of observations and computation of points by radiation.
//!
//! ORDER OF THE CORRECTION CHAIN (each step is applied only when switched on in the settings):
//!
//! 1. prism constant: SD1 = SD + c (c from the settings, metres; slope distances only);
//! 2. atmospheric correction: SD2 = SD1 (1 + ppm 1e-6) (slope distances only; ppm is an input);
//! 3. face reduction: readings of face 2 are brought to face 1 (horizontal circle minus 180 degrees,
//!    zenith 360 minus z), the rows of a target are averaged and their spread is reported; collimation
//!    and index error are applied from the equipment record only to rows that are not balanced by the
//!    other face;
//! 4. slope reduction: HD0 = SD2 sin z, V0 = SD2 cos z. Rows that give HD and a vertical distance VD
//!    directly use HD0 = HD and V0 = VD;
//! 5. curvature and refraction on the height difference: V1 = V0 + (1 - k) HD0^2 / (2 R);
//! 6. reduction of the horizontal distance to sea level: HD1 = HD0 R / (R + h) with h the mean height
//!    of the line above sea level (station height plus half the height difference);
//! 7. grid scale factor: HD2 = HD1 f.
//!
//! The height difference of the ground points is dZ = hi + V1 - th (instrument height plus the
//! reduced height difference of the axes minus target height).
//!
//! DERIVATION of step 5. In the local frame with origin at the instrument the level surface through the
//! instrument falls away from the tangent plane by D^2/(2R) at ground distance D (a circle of radius R:
//! sagitta D^2/(2R) to first order). A light ray of radius of curvature r = R/k (Baselga et al. 2014,
//! J. Surv. Eng. 140(2), eq. 1: k = R/r) departs from its initial tangent by D^2/(2r) = k D^2/(2R),
//! bending towards the earth. The target lies on the ray, so its height above the tangent plane is
//! SD cos z - k D^2/(2R), and its height above the level surface is that plus D^2/(2R):
//! SD cos z + (1 - k) D^2/(2R). First order in D/R: the second order term (D^3/R^2), and the difference
//! between SD and D inside the correction, are neglected, as is the small refraction effect on the
//! horizontal angle.
//!
//! DERIVATION of step 6. Two points at height h above sea level whose horizontal separation is L are
//! radially projected onto the sphere of radius R: arcs of concentric circles scale with radius, so
//! L_sl = L R / (R + h). Geoid undulation is not applied: h is taken as the height used in the
//! coordinates.
//!
//! Azimuths are taken as grid azimuths; there is no grid convergence correction. A setup whose
//! backsight is given by azimuth carries whatever frame that azimuth is in.

use std::collections::HashMap;

use afrobim_cogo::{angle::normalize_deg, inverse, Xy};
use serde::{Deserialize, Serialize};

use crate::error::{Result, SurveyError};
use crate::model::*;
use crate::units::{diff_deg, HzDirection};

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct Spread {
    pub n: usize,
    /// max - min of the values.
    pub range: f64,
    /// Sample standard deviation (n - 1), `None` for n < 2.
    pub std_dev: Option<f64>,
}

fn spread(v: &[f64]) -> Spread {
    let n = v.len();
    let (mn, mx) = v.iter().fold((f64::INFINITY, f64::NEG_INFINITY), |a, &x| (a.0.min(x), a.1.max(x)));
    let mean = v.iter().sum::<f64>() / n as f64;
    let sd = if n > 1 { Some((v.iter().map(|x| (x - mean).powi(2)).sum::<f64>() / (n as f64 - 1.0)).sqrt()) } else { None };
    Spread { n, range: if n > 0 { mx - mn } else { 0.0 }, std_dev: sd }
}

/// All the rows of one target in one setup reduced to means. Angles in degrees, lengths in metres.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ReducedSet {
    pub setup_id: u64,
    pub station: String,
    pub target: String,
    pub hz_kind: HzKind,
    /// Mean horizontal reading (kind Direction, in [0, 360)) or angle (kind Angle), face reduced.
    pub hz_deg: Option<f64>,
    pub hz_spread: Option<Spread>,
    /// Mean of face 2 reduced readings minus mean of face 1 readings (twice the horizontal
    /// collimation plus noise), when both faces are present. Degrees.
    pub hz_face_difference_deg: Option<f64>,
    pub zenith_deg: Option<f64>,
    pub zenith_spread: Option<Spread>,
    /// Index error (z1 + z2 - 360)/2 from both faces, degrees.
    pub index_error_deg: Option<f64>,
    /// Mean slope distance after steps 1 and 2.
    pub slope_distance: Option<f64>,
    pub distance_spread: Option<Spread>,
    /// Horizontal grid distance after the whole chain.
    pub horizontal_distance: Option<f64>,
    /// Height difference ground to ground (target minus station), if it can be formed.
    pub height_difference: Option<f64>,
    pub target_height: f64,
    pub description: String,
    pub n_rows: usize,
}

fn mean_wrapped(vals: &[f64]) -> f64 {
    let r = vals[0];
    let m = vals.iter().map(|&v| diff_deg(v, r)).sum::<f64>() / vals.len() as f64;
    normalize_deg(r + m)
}

/// Reduce all enabled observations of a setup to one set per target.
pub fn reduce_setup(db: &SurveyDatabase, net: &Network, setup: &Setup) -> Result<Vec<ReducedSet>> {
    let corr = &db.settings.corrections;
    let eq = setup.equipment.as_ref().and_then(|n| db.equipment(n));
    let station_z = net.point(&setup.station).and_then(|p| p.z);
    let mut order: Vec<&str> = Vec::new();
    for o in setup.observations.iter().filter(|o| !o.disabled) {
        if !order.contains(&o.target.as_str()) {
            order.push(&o.target);
        }
    }
    let mut out = Vec::new();
    for target in order {
        let rows: Vec<&Observation> = setup.observations.iter().filter(|o| !o.disabled && o.target == target).collect();
        let kinds: Vec<HzKind> = rows.iter().filter(|o| o.hz_deg.is_some()).map(|o| o.hz_kind).collect();
        if kinds.windows(2).any(|w| w[0] != w[1]) {
            return Err(SurveyError::Invalid(format!("setup {} target {target}: angles and circle readings are mixed", setup.station)));
        }
        let hz_kind = kinds.first().copied().unwrap_or(HzKind::Direction);

        // vertical: face reduction and index error
        let mut z_rows: Vec<(Face, f64)> = rows.iter().filter_map(|o| o.zenith_deg.map(|z| (o.face, z))).collect();
        let (mut z_f1, mut z_f2) = (Vec::new(), Vec::new());
        let mut z_red = Vec::new();
        for (f, z) in z_rows.drain(..) {
            match f {
                Face::Face2 => {
                    z_f2.push(360.0 - z);
                    z_red.push(360.0 - z);
                }
                _ => {
                    z_f1.push(z);
                    z_red.push(z);
                }
            }
        }
        let index_error = if !z_f1.is_empty() && !z_f2.is_empty() {
            let m1 = z_f1.iter().sum::<f64>() / z_f1.len() as f64;
            let m2 = z_f2.iter().sum::<f64>() / z_f2.len() as f64;
            Some(m1 - m2).map(|d| d / 2.0)
        } else {
            None
        };
        if index_error.is_none() {
            if let Some(ie) = eq.and_then(|e| e.vertical_index_error_deg) {
                for z in z_red.iter_mut() {
                    *z -= ie;
                }
            }
        }
        let zenith = if z_red.is_empty() { None } else { Some(z_red.iter().sum::<f64>() / z_red.len() as f64) };
        let sin_z = zenith.map(|z| z.to_radians().sin()).unwrap_or(1.0).abs().max(1e-9);

        // horizontal
        let mut hz_red = Vec::new();
        let (mut h1, mut h2) = (Vec::new(), Vec::new());
        for o in &rows {
            if let Some(h) = o.hz_deg {
                let mut v = h;
                if let (Some(c), Face::Face1 | Face::Face2) = (eq.and_then(|e| e.horizontal_collimation_deg), o.face) {
                    v += if o.face == Face::Face1 { -c / sin_z } else { c / sin_z };
                }
                match o.face {
                    Face::Face2 => {
                        h2.push(v - 180.0);
                        hz_red.push(v - 180.0);
                    }
                    _ => {
                        h1.push(v);
                        hz_red.push(v);
                    }
                }
            }
        }
        let (hz_deg, hz_spread, hz_face_diff) = if hz_red.is_empty() {
            (None, None, None)
        } else {
            let r = hz_red[0];
            let rel: Vec<f64> = hz_red.iter().map(|&v| diff_deg(v, r)).collect();
            let fd = if !h1.is_empty() && !h2.is_empty() { Some(diff_deg(mean_wrapped(&h2), mean_wrapped(&h1))) } else { None };
            (Some(mean_wrapped(&hz_red)), Some(spread(&rel)), fd)
        };

        // distances: steps 1 and 2
        let mut sd_rows = Vec::new();
        for o in &rows {
            if let Some(sd) = o.slope_distance {
                let mut d = sd;
                if let Some(c) = corr.prism_constant_m {
                    d += c;
                }
                if let Some(ppm) = corr.atmospheric_ppm {
                    d *= 1.0 + ppm * 1e-6;
                }
                sd_rows.push(d);
            }
        }
        let hd_rows: Vec<f64> = rows.iter().filter_map(|o| o.horizontal_distance).collect();
        let vd_rows: Vec<f64> = rows.iter().filter_map(|o| o.vertical_distance).collect();
        let th = rows.iter().map(|o| o.target_height).sum::<f64>() / rows.len() as f64;
        let mean = |v: &[f64]| if v.is_empty() { None } else { Some(v.iter().sum::<f64>() / v.len() as f64) };
        let sd_mean = mean(&sd_rows);

        // steps 4 to 7
        let (hd0, v0) = match (sd_mean, zenith, mean(&hd_rows), mean(&vd_rows)) {
            (Some(sd), Some(z), _, _) => (Some(sd * z.to_radians().sin()), Some(sd * z.to_radians().cos())),
            (_, _, Some(hd), vd) => (Some(hd), vd),
            (Some(_), None, None, _) => {
                return Err(SurveyError::Invalid(format!("setup {} target {target}: slope distance without a vertical angle or horizontal distance", setup.station)));
            }
            _ => (None, None),
        };
        let v1 = match (v0, hd0, corr.curvature_refraction) {
            (Some(v), Some(d), Some(cr)) => Some(v + (1.0 - cr.refraction_coefficient) * d * d / (2.0 * cr.earth_radius_m)),
            (v, _, _) => v,
        };
        let dz = v1.map(|v| setup.instrument_height + v - th);
        let hd_final = match hd0 {
            None => None,
            Some(mut d) => {
                if let Some(sl) = corr.sea_level {
                    let zs = station_z.ok_or_else(|| SurveyError::Invalid(format!("setup {}: sea level reduction needs the station elevation", setup.station)))?;
                    let h = zs + 0.5 * dz.unwrap_or(0.0);
                    d *= sl.earth_radius_m / (sl.earth_radius_m + h);
                }
                if let Some(f) = corr.grid_scale_factor {
                    d *= f;
                }
                Some(d)
            }
        };
        let desc = rows.iter().rev().map(|o| o.description.as_str()).find(|d| !d.is_empty()).unwrap_or("").to_string();
        out.push(ReducedSet {
            setup_id: setup.id,
            station: setup.station.clone(),
            target: target.to_string(),
            hz_kind,
            hz_deg,
            hz_spread,
            hz_face_difference_deg: hz_face_diff,
            zenith_deg: zenith,
            zenith_spread: if z_red.is_empty() { None } else { Some(spread(&z_red)) },
            index_error_deg: index_error,
            slope_distance: sd_mean,
            distance_spread: if sd_rows.is_empty() { None } else { Some(spread(&sd_rows)) },
            horizontal_distance: hd_final,
            height_difference: dz,
            target_height: th,
            description: desc,
            n_rows: rows.len(),
        });
    }
    Ok(out)
}

/// Azimuth of the backsight line and the circle reading on it.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Orientation {
    pub backsight_azimuth_deg: f64,
    pub backsight_reading_deg: f64,
}

/// Azimuth from the station to the backsight: from coordinates when the backsight point has them,
/// else a known direction (either way round) between the two names, else the azimuth stored with the
/// setup.
pub fn backsight_azimuth(net: &Network, setup: &Setup) -> Result<f64> {
    let st = net.point(&setup.station).and_then(|p| p.e.zip(p.n)).ok_or_else(|| SurveyError::Invalid(format!("station {} has no coordinates", setup.station)));
    if let Some(bs) = &setup.backsight_point {
        if let (Ok((se, sn)), Some((be, bn))) = (st.as_ref().map(|x| *x), net.point(bs).and_then(|p| p.e.zip(p.n))) {
            let inv = inverse(Xy::new(se, sn), None, Xy::new(be, bn), None);
            return inv.bearing_deg.ok_or_else(|| SurveyError::Invalid(format!("backsight {bs} coincides with station {}", setup.station)));
        }
        for d in &net.directions {
            if d.from == setup.station && &d.to == bs {
                return Ok(normalize_deg(d.azimuth_deg));
            }
            if &d.from == bs && d.to == setup.station {
                return Ok(normalize_deg(d.azimuth_deg + 180.0));
            }
        }
    }
    setup.backsight_azimuth_deg.map(normalize_deg).ok_or_else(|| SurveyError::Invalid(format!("setup {}: no backsight point with coordinates, known direction or azimuth", setup.station)))
}

pub fn orientation(net: &Network, setup: &Setup, sets: &[ReducedSet]) -> Result<Orientation> {
    let az = backsight_azimuth(net, setup)?;
    let observed = setup
        .backsight_point
        .as_ref()
        .and_then(|b| sets.iter().find(|s| &s.target == b && s.hz_kind == HzKind::Direction))
        .and_then(|s| s.hz_deg);
    Ok(Orientation { backsight_azimuth_deg: az, backsight_reading_deg: observed.unwrap_or(setup.backsight_reading_deg) })
}

/// Grid azimuth of a reduced set from its station, given the orientation.
pub fn set_azimuth(db: &SurveyDatabase, set: &ReducedSet, o: &Orientation) -> Option<f64> {
    let h = set.hz_deg?;
    let sgn = if db.settings.hz_direction == HzDirection::Right { 1.0 } else { -1.0 };
    let turn = match set.hz_kind {
        HzKind::Angle => h,
        HzKind::Direction => diff_deg(h, o.backsight_reading_deg),
    };
    Some(normalize_deg(o.backsight_azimuth_deg + sgn * turn))
}

#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
pub struct ComputeReport {
    pub setups_processed: Vec<u64>,
    /// Setups that could not be processed and why.
    pub unprocessed: Vec<(u64, String)>,
    pub points_computed: Vec<String>,
    /// For targets observed more than once: the largest distance of a result from the mean.
    pub max_difference_from_mean: Vec<(String, f64)>,
    /// Computed minus given coordinates for control points that were observed (plan distance).
    pub control_checks: Vec<(String, f64)>,
    pub warnings: Vec<String>,
}

/// Compute coordinates of observed points by radiation from the setups, in order of dependency.
/// Stations come from control or given points or from points computed earlier in the run (their value
/// at that moment: this is radiation, not an adjustment). Writes `e`, `n`, `z` of non control points
/// that are absent or already marked computed; control and given points are never overwritten.
pub fn compute_points(db: &mut SurveyDatabase, network: &str) -> Result<ComputeReport> {
    let mut report = ComputeReport::default();
    let net_idx = db.networks.iter().position(|n| n.name == network).ok_or_else(|| SurveyError::NoNetwork(network.into()))?;
    let mut cands: HashMap<String, Vec<(f64, f64, Option<f64>)>> = HashMap::new();
    let mut descr: HashMap<String, String> = HashMap::new();
    let mut done: Vec<bool> = vec![false; db.networks[net_idx].setups.len()];
    let mut order_new: Vec<String> = Vec::new();
    let mut again = true;
    while again {
        again = false;
        for si in 0..done.len() {
            if done[si] {
                continue;
            }
            let (sets, orient, st, stz, hi) = {
                let net = &db.networks[net_idx];
                let setup = &net.setups[si];
                let st = match net.point(&setup.station).and_then(|p| p.e.zip(p.n)) {
                    Some(x) => x,
                    None => continue,
                };
                let sets = reduce_setup(db, net, setup)?;
                let orient = match orientation(net, setup, &sets) {
                    Ok(o) => o,
                    Err(_) => continue,
                };
                (sets, orient, st, net.point(&setup.station).and_then(|p| p.z), setup.instrument_height)
            };
            let _ = hi;
            done[si] = true;
            again = true;
            let sid = db.networks[net_idx].setups[si].id;
            report.setups_processed.push(sid);
            for set in &sets {
                let (Some(az), Some(hd)) = (set_azimuth(db, set, &orient), set.horizontal_distance) else { continue };
                let (s, c) = az.to_radians().sin_cos();
                let (e, n) = (st.0 + hd * s, st.1 + hd * c);
                let z = match (stz, set.height_difference) {
                    (Some(z0), Some(dz)) => Some(z0 + dz),
                    _ => None,
                };
                if !set.description.is_empty() {
                    descr.insert(set.target.clone(), set.description.clone());
                }
                let net = &mut db.networks[net_idx];
                match net.point(&set.target) {
                    Some(p) if p.control || (!p.computed && p.e.is_some()) => {
                        if let (Some(pe), Some(pn)) = (p.e, p.n) {
                            report.control_checks.push((set.target.clone(), (e - pe).hypot(n - pn)));
                        }
                        continue;
                    }
                    _ => {}
                }
                let list = cands.entry(set.target.clone()).or_default();
                list.push((e, n, z));
                // running mean written back so later setups can start from it
                let k = list.len() as f64;
                let me = list.iter().map(|c| c.0 - e).sum::<f64>() / k + e;
                let mn = list.iter().map(|c| c.1 - n).sum::<f64>() / k + n;
                let zs: Vec<f64> = list.iter().filter_map(|c| c.2).collect();
                let mz = if zs.is_empty() { None } else { Some(zs.iter().sum::<f64>() / zs.len() as f64) };
                match net.point_mut(&set.target) {
                    Some(p) => {
                        p.e = Some(me);
                        p.n = Some(mn);
                        p.z = mz;
                        p.computed = true;
                    }
                    None => {
                        let mut p = SurveyPoint::given(&set.target, me, mn, mz);
                        p.computed = true;
                        net.points.push(p);
                        order_new.push(set.target.clone());
                    }
                }
            }
        }
    }
    for (i, d) in done.iter().enumerate() {
        if !d {
            let net = &db.networks[net_idx];
            let s = &net.setups[i];
            let why = if net.point(&s.station).and_then(|p| p.e.zip(p.n)).is_none() { format!("station {} has no coordinates", s.station) } else { "no usable backsight".to_string() };
            report.unprocessed.push((s.id, why));
        }
    }
    let net = &mut db.networks[net_idx];
    for (t, list) in &cands {
        report.points_computed.push(t.clone());
        if list.len() > 1 {
            let k = list.len() as f64;
            let (e0, n0) = (list[0].0, list[0].1);
            let me = list.iter().map(|c| c.0 - e0).sum::<f64>() / k;
            let mn = list.iter().map(|c| c.1 - n0).sum::<f64>() / k;
            let m = list.iter().map(|c| (c.0 - e0 - me).hypot(c.1 - n0 - mn)).fold(0.0, f64::max);
            report.max_difference_from_mean.push((t.clone(), m));
        }
        if let (Some(d), Some(p)) = (descr.get(t), net.point_mut(t)) {
            if p.description.is_empty() {
                p.description = d.clone();
            }
        }
    }
    report.points_computed.sort();
    Ok(report)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn base_db() -> SurveyDatabase {
        let mut db = SurveyDatabase::new("t");
        db.add_network("N", "t").unwrap();
        db
    }

    fn obs(target: &str, face: Face, hz: f64, z: f64, sd: f64) -> Observation {
        let mut o = Observation::new(target);
        o.face = face;
        o.hz_deg = Some(hz);
        o.zenith_deg = Some(z);
        o.slope_distance = Some(sd);
        o
    }

    #[test]
    fn face_means_and_spreads() {
        let mut db = base_db();
        let net = db.network_mut("N").unwrap();
        net.points.push(SurveyPoint::control("S", 500000.0, 9_960_000.0, Some(100.0)));
        let mut su = Setup::new("S", 1.5);
        // true direction 100 deg, collimation 0.01 deg, index error 0.005 deg
        su.observations.push(obs("T", Face::Face1, 100.01, 85.005, 100.0));
        su.observations.push(obs("T", Face::Face2, 280.0 - 0.01 + 0.0, 360.0 - 85.0 + 0.005 - 0.0 * 0.0, 100.0));
        net.setups.push(su.clone());
        let sets = reduce_setup(&db, db.network("N").unwrap(), &su).unwrap();
        let s = &sets[0];
        assert!((s.hz_deg.unwrap() - 100.0).abs() < 1e-12, "{:?}", s.hz_deg);
        assert!((s.hz_face_difference_deg.unwrap() + 0.02).abs() < 1e-12);
        assert!((s.zenith_deg.unwrap() - 85.0).abs() < 1e-12);
        assert!((s.index_error_deg.unwrap() - 0.005).abs() < 1e-12);
        assert!((s.hz_spread.unwrap().range - 0.02).abs() < 1e-12);
        assert_eq!(s.hz_spread.unwrap().n, 2);
    }

    #[test]
    fn chain_order_prism_ppm_slope_cr_scale() {
        let mut db = base_db();
        db.settings.corrections.prism_constant_m = Some(-0.030);
        db.settings.corrections.atmospheric_ppm = Some(10.0);
        db.settings.corrections.curvature_refraction = Some(CurvatureRefraction { earth_radius_m: 6_371_000.0, refraction_coefficient: 0.13 });
        db.settings.corrections.grid_scale_factor = Some(0.9996);
        let net = db.network_mut("N").unwrap();
        net.points.push(SurveyPoint::control("S", 500000.0, 9_960_000.0, Some(100.0)));
        let mut su = Setup::new("S", 1.5);
        let mut o = obs("T", Face::Single, 10.0, 80.0, 500.0);
        o.target_height = 1.8;
        su.observations.push(o);
        let sets = reduce_setup(&db, db.network("N").unwrap(), &su).unwrap();
        let sd = (500.0 - 0.030) * (1.0 + 10e-6);
        let hd0 = sd * 80f64.to_radians().sin();
        let v0 = sd * 80f64.to_radians().cos();
        let v1 = v0 + 0.87 * hd0 * hd0 / (2.0 * 6_371_000.0);
        assert!((sets[0].horizontal_distance.unwrap() - hd0 * 0.9996).abs() < 1e-9);
        assert!((sets[0].height_difference.unwrap() - (1.5 + v1 - 1.8)).abs() < 1e-9);
    }

    #[test]
    fn radiation_at_utm_magnitude_and_traverse_ordering() {
        let mut db = base_db();
        let net = db.network_mut("N").unwrap();
        net.points.push(SurveyPoint::control("A", 500_000.0, 9_960_000.0, Some(1000.0)));
        net.points.push(SurveyPoint::control("B", 500_100.0, 9_960_000.0, Some(1000.0)));
        // setup at A back on B (azimuth 90), observes P1 at hz 90 right -> azimuth 180
        let mut s1 = Setup::new("A", 1.0);
        s1.backsight_point = Some("B".into());
        let mut o = Observation::new("P1");
        o.hz_kind = HzKind::Angle;
        o.hz_deg = Some(90.0);
        o.horizontal_distance = Some(50.0);
        o.vertical_distance = Some(2.0);
        o.target_height = 1.0;
        s1.observations.push(o);
        // second setup at P1, listed first to test ordering by dependency: backsight A, target P2 at 90 right of A
        let mut s2 = Setup::new("P1", 1.0);
        s2.backsight_point = Some("A".into());
        let mut o2 = Observation::new("P2");
        o2.hz_kind = HzKind::Angle;
        o2.hz_deg = Some(90.0);
        o2.horizontal_distance = Some(30.0);
        s2.observations.push(o2);
        net.setups.push(s2);
        net.setups.push(s1);
        let r = compute_points(&mut db, "N").unwrap();
        assert!(r.unprocessed.is_empty(), "{:?}", r);
        let net = db.network("N").unwrap();
        let p1 = net.point("P1").unwrap();
        assert!((p1.e.unwrap() - 500_000.0).abs() < 1e-9);
        assert!((p1.n.unwrap() - (9_960_000.0 - 50.0)).abs() < 1e-9);
        assert!((p1.z.unwrap() - 1002.0).abs() < 1e-12);
        // at P1 the backsight A lies at azimuth 0; 90 right gives azimuth 90, so P2 is 30 m east
        let p2 = net.point("P2").unwrap();
        assert!((p2.e.unwrap() - 500_030.0).abs() < 1e-9);
        assert!((p2.n.unwrap() - 9_959_950.0).abs() < 1e-9);
    }

    #[test]
    fn published_civil3d_example_point_computation() {
        // Autodesk Civil 3D help, "To Create a Survey Observation Using a Face1 or Face2 Angle":
        // station N 200 E 200 (units: gon, distances as given). The printed results
        //   point 101: AZIMUTH 101.6126  DISTANCE 30.4800  NORTH 199.2280  EAST 230.4702  (SD 30.480, V 100 gon)
        //   point 102: AZIMUTH 191.1682  DISTANCE 15.3013  NORTH 184.8457  EAST 202.1159  (SD 15.304, V 101.2052 gon)
        // are reproduced here from the printed azimuths (the step from the field angles to the azimuth
        // depends on collimation and backsight context that the page does not give, so only the
        // radiation step is reproduced).
        let mut db = base_db();
        let net = db.network_mut("N").unwrap();
        net.points.push(SurveyPoint::control("2", 200.0, 200.0, None));
        for (name, az_gon, v_gon, sd, n_exp, e_exp, d_exp) in [("101", 101.6126, 100.0, 30.480, 199.2280, 230.4702, 30.4800), ("102", 191.1682, 101.2052, 15.304, 184.8457, 202.1159, 15.3013)] {
            let mut su = Setup::new("2", 1.585);
            su.backsight_azimuth_deg = Some(0.0);
            let mut o = Observation::new(name);
            o.hz_kind = HzKind::Angle;
            o.hz_deg = Some(az_gon * 0.9);
            o.zenith_deg = Some(v_gon * 0.9);
            o.slope_distance = Some(sd);
            su.observations.push(o);
            net.setups.push(su);
            let _ = (n_exp, e_exp, d_exp);
        }
        compute_points(&mut db, "N").unwrap();
        let net = db.network("N").unwrap();
        for (name, n_exp, e_exp) in [("101", 199.2280, 230.4702), ("102", 184.8457, 202.1159)] {
            let p = net.point(name).unwrap();
            assert!((p.n.unwrap() - n_exp).abs() < 6e-5, "{name} N {} vs {n_exp}", p.n.unwrap());
            assert!((p.e.unwrap() - e_exp).abs() < 6e-5, "{name} E {} vs {e_exp}", p.e.unwrap());
        }
    }
}
