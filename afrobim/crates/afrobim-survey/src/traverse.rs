// SPDX-License-Identifier: LicenseRef-AfroBIM-Proprietary
// Copyright (c) 2026 AfroBIM. All rights reserved.
//! Traverse definition from setups and closure by `afrobim-cogo` (compass and transit rules).
//!
//! A traverse is an ordered list of station names. Each course between consecutive stations takes its
//! distance from the reduced horizontal distances measured in either direction (mean of those
//! available). The bearing of the first course is the grid azimuth observed at the first station from
//! its backsight. Later bearings follow from the angle turned at each intermediate station between its
//! backsight (the previous station) and foresight (the next station), as the difference of the two
//! circle readings of that setup: b(k+1) = b(k) + 180 + angle (angle right; `afrobim_cogo::traverse::
//! bearings_from_angles`, whose derivation is in that crate). If a known final bearing is given, the
//! angular misclosure is shared equally between the angles (`adjust_angles_to_known_bearing`) when
//! `adjust_angles` is set. Then the linear misclosure and the compass (Bowditch) and transit
//! adjustments come from `afrobim_cogo::Traverse`.
//!
//! Least squares: adjust the network itself with `adjust::adjust_network_2d`, which uses every
//! observation, not only the chain of a traverse.

use afrobim_cogo::traverse::{adjust_angles_to_known_bearing, bearings_from_angles, AdjustedTraverse, AngleMode, Closure, Course, Misclosure, Traverse};
use afrobim_cogo::{angle::normalize_deg, Xy};
use serde::{Deserialize, Serialize};

use crate::error::{Result, SurveyError};
use crate::model::*;
use crate::reduce::{orientation, reduce_setup, set_azimuth, ReducedSet};
use crate::units::{diff_deg, HzDirection};

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum TraverseClosure {
    /// No closure check.
    Open,
    /// The last station is the first (list it twice, at start and end).
    Loop,
    /// The last station is a point with known coordinates.
    ToKnownPoint,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct TraverseSpec {
    pub network: String,
    pub stations: Vec<String>,
    pub closure: TraverseClosure,
    /// Known bearing of the last course (for an angular check), degrees.
    pub known_final_bearing_deg: Option<f64>,
    pub adjust_angles: bool,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct TraverseReport {
    pub courses: Vec<Course>,
    /// Angles turned at the intermediate stations as read from the setups, degrees.
    pub angles_deg: Vec<f64>,
    pub angular_misclosure_deg: Option<f64>,
    pub misclosure: Option<Misclosure>,
    pub bowditch: Option<AdjustedTraverse>,
    pub transit: Option<AdjustedTraverse>,
    pub unadjusted_points: Vec<Xy>,
}

fn sets_of(db: &SurveyDatabase, net: &Network, station: &str) -> Result<Vec<(Setup, Vec<ReducedSet>)>> {
    let mut v = Vec::new();
    for s in net.setups.iter().filter(|s| s.station == station) {
        v.push((s.clone(), reduce_setup(db, net, s)?));
    }
    Ok(v)
}

pub fn traverse_from_setups(db: &SurveyDatabase, spec: &TraverseSpec) -> Result<TraverseReport> {
    let net = db.network(&spec.network)?;
    let st = &spec.stations;
    if st.len() < 2 {
        return Err(SurveyError::Invalid("a traverse needs at least two stations".into()));
    }
    let xy = |n: &str| -> Result<Xy> { net.point(n).and_then(|p| p.e.zip(p.n)).map(|(e, n)| Xy::new(e, n)).ok_or_else(|| SurveyError::Invalid(format!("station {n} has no coordinates"))) };
    let start = xy(&st[0])?;
    let mode = if db.settings.hz_direction == HzDirection::Right { AngleMode::Right } else { AngleMode::Left };
    // distances
    let mut dists = Vec::new();
    for w in st.windows(2) {
        let mut vals = Vec::new();
        for (a, b) in [(&w[0], &w[1]), (&w[1], &w[0])] {
            for (_, sets) in sets_of(db, net, a)? {
                for s in sets.iter().filter(|s| &s.target == b) {
                    if let Some(d) = s.horizontal_distance {
                        vals.push(d);
                    }
                }
            }
        }
        if vals.is_empty() {
            return Err(SurveyError::Invalid(format!("no distance between {} and {}", w[0], w[1])));
        }
        dists.push(vals.iter().sum::<f64>() / vals.len() as f64);
    }
    // first bearing
    let first_bearing = {
        let setups = sets_of(db, net, &st[0])?;
        let mut found = None;
        for (s, sets) in &setups {
            if let Some(set) = sets.iter().find(|x| x.target == st[1]) {
                let o = orientation(net, s, sets)?;
                found = set_azimuth(db, set, &o);
                if found.is_some() {
                    break;
                }
            }
        }
        found.ok_or_else(|| SurveyError::Invalid(format!("no setup at {} observes {} with a usable backsight", st[0], st[1])))?
    };
    // angles at intermediate stations
    let mut angles = Vec::new();
    for k in 1..st.len() - 1 {
        let (back, fore) = (&st[k - 1], &st[k + 1]);
        let mut done = None;
        for (s, sets) in sets_of(db, net, &st[k])? {
            let fs = sets.iter().find(|x| &x.target == fore).and_then(|x| x.hz_deg.map(|h| (h, x.hz_kind)));
            let bs_read = sets.iter().find(|x| &x.target == back).and_then(|x| x.hz_deg.map(|h| (h, x.hz_kind)));
            if let Some((hf, kf)) = fs {
                let hb = match bs_read {
                    Some((h, _)) => Some(h),
                    None if kf == HzKind::Angle => Some(0.0),
                    None if s.backsight_point.as_ref() == Some(back) => Some(s.backsight_reading_deg),
                    None => None,
                };
                if let Some(hb) = hb {
                    done = Some(normalize_deg(hf - hb));
                    break;
                }
            }
        }
        angles.push(done.ok_or_else(|| SurveyError::Invalid(format!("no setup at {} observes both {back} and {fore}", st[k])))?);
    }
    let _ = diff_deg;
    let mut angular_mis = None;
    let mut used_angles = angles.clone();
    if let Some(known) = spec.known_final_bearing_deg {
        if !angles.is_empty() {
            let adj = adjust_angles_to_known_bearing(first_bearing, &angles, mode, known)?;
            angular_mis = Some(adj.misclosure_deg);
            if spec.adjust_angles {
                used_angles = adj.adjusted_deg;
            }
        }
    }
    let bearings = bearings_from_angles(first_bearing, &used_angles, mode);
    let courses: Vec<Course> = bearings.iter().zip(&dists).map(|(b, d)| Course { bearing_deg: *b, distance: *d }).collect();
    let closure = match spec.closure {
        TraverseClosure::Open => Closure::Open,
        TraverseClosure::Loop => {
            if st.first() != st.last() {
                return Err(SurveyError::Invalid("a loop traverse must list its first station again at the end".into()));
            }
            Closure::Loop
        }
        TraverseClosure::ToKnownPoint => Closure::ToPoint(xy(st.last().unwrap())?),
    };
    let t = Traverse::new(start, courses.clone(), closure)?;
    let (mis, bow, tr) = if spec.closure == TraverseClosure::Open {
        (None, None, None)
    } else {
        (Some(t.misclosure()?), Some(t.adjust(afrobim_cogo::AdjustMethod::Bowditch)?), Some(t.adjust(afrobim_cogo::AdjustMethod::Transit)?))
    };
    Ok(TraverseReport { courses, angles_deg: angles, angular_misclosure_deg: angular_mis, misclosure: mis, bowditch: bow, transit: tr, unadjusted_points: t.points() })
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A 100 m by 50 m rectangle A(0,0) B(0,100) C(50,100) D(50,0) given as relative (E,N), clockwise
    /// A -> B (north) -> C (east) -> D (south) -> A, measured with right angles and exact distances,
    /// except that the length B-C carries a 0.02 m error. At UTM size.
    fn rectangle(err: f64) -> SurveyDatabase {
        let (e0, n0) = (500_000.0, 9_960_000.0);
        let mut db = SurveyDatabase::new("t");
        db.add_network("N", "t").unwrap();
        let net = db.network_mut("N").unwrap();
        net.points.push(SurveyPoint::control("A", e0, n0, None));
        // each station: backsight = previous, foresight = next; interior angle on the right = 270 for a
        // clockwise rectangle turned to the right: forward bearing = back bearing + 180 + angle
        let names = ["A", "B", "C", "D", "A"];
        let lens = [100.0, 50.0 + err, 100.0, 50.0];
        let bearings = [0.0, 90.0, 180.0, 270.0];
        for k in 0..4 {
            let st = names[k];
            let prev = names[(k + 3) % 4];
            let next = names[k + 1];
            let mut s = Setup::new(st, 1.5);
            s.backsight_point = Some(prev.into());
            // backsight azimuth: from the station back to the previous one
            if k == 0 {
                s.backsight_point = None;
                s.backsight_azimuth_deg = Some(270.0);
            }
            let mut o = Observation::new(next);
            o.hz_kind = HzKind::Angle;
            // angle from backsight (bearing back = bearings[k-1] + 180) to foresight (bearings[k])
            let back_az = if k == 0 { 270.0 } else { (bearings[k - 1] + 180.0) % 360.0 };
            o.hz_deg = Some((bearings[k] - back_az + 360.0) % 360.0);
            o.horizontal_distance = Some(lens[k]);
            s.observations.push(o);
            if k > 0 {
                // also observe the backsight with a zero angle so that the traverse angle can be read
                let mut b = Observation::new(prev);
                b.hz_kind = HzKind::Angle;
                b.hz_deg = Some(0.0);
                s.observations.push(b);
            }
            net.setups.push(s);
        }
        // coordinates of B, C, D only needed as stations for later setups
        let pts = [("B", e0, n0 + 100.0), ("C", e0 + 50.0, n0 + 100.0), ("D", e0 + 50.0, n0)];
        for (n, e, nn) in pts {
            net.points.push(SurveyPoint::given(n, e, nn, None));
        }
        db
    }

    #[test]
    fn rectangle_loop_matches_cogo_closed_form() {
        let db = rectangle(0.02);
        let spec = TraverseSpec { network: "N".into(), stations: ["A", "B", "C", "D", "A"].iter().map(|s| s.to_string()).collect(), closure: TraverseClosure::Loop, known_final_bearing_deg: None, adjust_angles: false };
        let r = traverse_from_setups(&db, &spec).unwrap();
        assert_eq!(r.courses.len(), 4);
        for (c, b) in r.courses.iter().zip([0.0, 90.0, 180.0, 270.0]) {
            assert!((c.bearing_deg - b).abs() < 1e-9, "{c:?}");
        }
        let m = r.misclosure.unwrap();
        assert!((m.error_e - 0.02).abs() < 1e-9 && m.error_n.abs() < 1e-9, "{m:?}");
        // Bowditch: correction to course i is -0.02 L_i / 300.02
        let b = r.bowditch.unwrap();
        let total = 300.02;
        assert!((b.corrections[1].0 - (-0.02 * 50.02 / total)).abs() < 1e-9);
        assert!((b.points[4].e - 500_000.0).abs() < 1e-9 && (b.points[4].n - 9_960_000.0).abs() < 1e-9);
        // transit: east departures are on courses 2 and 4: sum|dE| = 100.02, correction on course 2 is -0.02 * 50.02/100.02
        let t = r.transit.unwrap();
        assert!((t.corrections[1].0 - (-0.02 * 50.02 / 100.02)).abs() < 1e-9);
        assert!(r.angular_misclosure_deg.is_none());
    }

    #[test]
    fn angular_misclosure_shared() {
        let db = rectangle(0.0);
        let spec = TraverseSpec { network: "N".into(), stations: ["A", "B", "C", "D", "A"].iter().map(|s| s.to_string()).collect(), closure: TraverseClosure::Loop, known_final_bearing_deg: Some(270.0), adjust_angles: true };
        let r = traverse_from_setups(&db, &spec).unwrap();
        assert!(r.angular_misclosure_deg.unwrap().abs() < 1e-9);
        assert_eq!(r.angles_deg.len(), 3);
        let bad = TraverseSpec { stations: ["A", "B", "C", "D"].iter().map(|s| s.to_string()).collect(), closure: TraverseClosure::Loop, ..spec.clone() };
        assert!(traverse_from_setups(&db, &bad).is_err());
    }
}
