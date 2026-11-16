//! Traverses: computation, misclosure and adjustment.
//!
//! Sources: the compass (Bowditch) rule and the transit rule are the standard traverse adjustments
//! of any surveying text, for example Ghilani and Wolf, *Elementary Surveying: An Introduction to
//! Geomatics* (the traverse computation and adjustment chapters), and Schofield and Breach,
//! *Engineering Surveying*. The formulae are stated and derived below. **No published numerical
//! example is reproduced in the tests** (the open textbooks tried could not be fetched as readable
//! text); the tests use closed-form cases whose answers follow from the formulae by hand.
//!
//! Terms. A course has a whole circle bearing b and a horizontal length L. Its departure (change of
//! easting) is dE = L sin b and its latitude (change of northing) is dN = L cos b. The traverse starts at
//! a known point and is
//! - **open**: it ends where it ends, no check exists;
//! - a **loop**: it should end at its start;
//! - **to a known point**: it should end at given coordinates.
//!
//! Misclosure: with the computed end minus the known end giving (eE, eN), the linear misclosure is
//! sqrt(eE^2 + eN^2) and the precision ratio is expressed as perimeter / linear misclosure (1 in n).
//! Whether a ratio is good enough is the caller's decision; this module has no required value.
//!
//! Compass (Bowditch) rule: correction to the departure of course i is -eE L_i / sum(L), and to its
//! latitude -eN L_i / sum(L). Derivation: assuming errors accumulate in proportion to length, the
//! share of the total error belonging to course i is L_i / sum(L); the corrections cancel the error,
//! since sum(-eE L_i / sum L) = -eE.
//!
//! Transit rule: correction to dE_i is -eE |dE_i| / sum|dE|, to dN_i is -eN |dN_i| / sum|dN|, i.e. the
//! error is shared in proportion to the size of the component itself. Same cancellation proof. It
//! is not defined when sum|dE| or sum|dN| is zero while the matching error is not.

use serde::{Deserialize, Serialize};

use crate::angle::{normalize_deg, signed_deg};
use crate::error::{CogoError, Result};
use crate::geom::{bearing_of, Xy};

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct Course {
    pub bearing_deg: f64,
    pub distance: f64,
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub enum Closure {
    Open,
    Loop,
    ToPoint(Xy),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum AdjustMethod {
    /// Compass rule.
    Bowditch,
    Transit,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Traverse {
    pub start: Xy,
    pub courses: Vec<Course>,
    pub closure: Closure,
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct Misclosure {
    /// Computed end minus known end, easting.
    pub error_e: f64,
    pub error_n: f64,
    pub linear: f64,
    pub perimeter: f64,
    /// perimeter / linear: the `n` of "1 in n". `None` when the misclosure is zero.
    pub precision_denominator: Option<f64>,
    /// Bearing from the computed end towards the known end (the direction of the needed correction).
    pub direction_to_known_deg: Option<f64>,
}

impl Misclosure {
    /// True when the precision ratio is at least 1 in `required_n`, a value the caller chooses.
    pub fn meets_ratio(&self, required_n: f64) -> bool {
        match self.precision_denominator {
            None => true,
            Some(n) => n >= required_n,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct AdjustedTraverse {
    pub method: AdjustMethod,
    pub misclosure: Misclosure,
    /// n + 1 adjusted points; the last equals the known end.
    pub points: Vec<Xy>,
    /// Correction applied to each course as (dE, dN).
    pub corrections: Vec<(f64, f64)>,
    /// Bearings and lengths recomputed from the adjusted points.
    pub courses: Vec<Course>,
}

impl Traverse {
    pub fn new(start: Xy, courses: Vec<Course>, closure: Closure) -> Result<Traverse> {
        if courses.is_empty() {
            return Err(CogoError::Invalid("a traverse needs at least one course".into()));
        }
        if courses.iter().any(|c| !(c.distance >= 0.0) || !c.distance.is_finite() || !c.bearing_deg.is_finite()) {
            return Err(CogoError::Invalid("course distances must be finite and not negative".into()));
        }
        Ok(Traverse { start, courses, closure })
    }

    fn components(&self) -> Vec<(f64, f64)> {
        self.courses
            .iter()
            .map(|c| {
                let r = c.bearing_deg.to_radians();
                (c.distance * r.sin(), c.distance * r.cos())
            })
            .collect()
    }

    /// The unadjusted points, n + 1 of them, starting at `start`.
    pub fn points(&self) -> Vec<Xy> {
        let mut p = vec![self.start];
        let (mut se, mut sn) = (0.0, 0.0);
        for (de, dn) in self.components() {
            se += de;
            sn += dn;
            p.push(self.start.add(se, sn));
        }
        p
    }

    pub fn known_end(&self) -> Option<Xy> {
        match self.closure {
            Closure::Open => None,
            Closure::Loop => Some(self.start),
            Closure::ToPoint(p) => Some(p),
        }
    }

    pub fn misclosure(&self) -> Result<Misclosure> {
        let known = self.known_end().ok_or(CogoError::OpenTraverse)?;
        let comps = self.components();
        let (se, sn) = comps.iter().fold((0.0, 0.0), |a, c| (a.0 + c.0, a.1 + c.1));
        // computed end minus known end, from differences (no large coordinate is added)
        let (ke, kn) = known.sub(self.start);
        let (ee, en) = (se - ke, sn - kn);
        let linear = ee.hypot(en);
        let perimeter: f64 = self.courses.iter().map(|c| c.distance).sum();
        Ok(Misclosure {
            error_e: ee,
            error_n: en,
            linear,
            perimeter,
            precision_denominator: if linear > 0.0 { Some(perimeter / linear) } else { None },
            direction_to_known_deg: if linear > 0.0 { Some(bearing_of(-ee, -en)) } else { None },
        })
    }

    pub fn adjust(&self, method: AdjustMethod) -> Result<AdjustedTraverse> {
        let m = self.misclosure()?;
        let comps = self.components();
        let total_l: f64 = self.courses.iter().map(|c| c.distance).sum();
        let sum_e: f64 = comps.iter().map(|c| c.0.abs()).sum();
        let sum_n: f64 = comps.iter().map(|c| c.1.abs()).sum();
        let mut corr = Vec::new();
        for (c, (de, dn)) in self.courses.iter().zip(&comps) {
            let (ce, cn) = match method {
                AdjustMethod::Bowditch => {
                    if total_l == 0.0 {
                        return Err(CogoError::Invalid("traverse has zero length".into()));
                    }
                    (-m.error_e * c.distance / total_l, -m.error_n * c.distance / total_l)
                }
                AdjustMethod::Transit => {
                    let ce = if m.error_e == 0.0 {
                        0.0
                    } else if sum_e == 0.0 {
                        return Err(CogoError::Invalid("transit rule: no departures to share the easting error over".into()));
                    } else {
                        -m.error_e * de.abs() / sum_e
                    };
                    let cn = if m.error_n == 0.0 {
                        0.0
                    } else if sum_n == 0.0 {
                        return Err(CogoError::Invalid("transit rule: no latitudes to share the northing error over".into()));
                    } else {
                        -m.error_n * dn.abs() / sum_n
                    };
                    (ce, cn)
                }
            };
            corr.push((ce, cn));
        }
        let mut pts = vec![self.start];
        let (mut se, mut sn) = (0.0, 0.0);
        for ((de, dn), (ce, cn)) in comps.iter().zip(&corr) {
            se += de + ce;
            sn += dn + cn;
            pts.push(self.start.add(se, sn));
        }
        // The sums above reach the known end up to rounding; make that exact.
        let known = self.known_end().unwrap();
        *pts.last_mut().unwrap() = known;
        let courses = pts
            .windows(2)
            .map(|w| {
                let (de, dn) = w[1].sub(w[0]);
                Course { bearing_deg: bearing_of(de, dn), distance: de.hypot(dn) }
            })
            .collect();
        Ok(AdjustedTraverse { method, misclosure: m, points: pts, corrections: corr, courses })
    }
}

// ---------------------------------------------------------------------------------------------
// Angles

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum AngleMode {
    /// Angles measured clockwise from the backsight (angle right).
    Right,
    /// Angles measured anticlockwise from the backsight (angle left).
    Left,
}

/// Bearings of the courses of a traverse from the first bearing and the angle at each following station.
/// With angle right a, the forward bearing = back bearing + a, and back bearing = previous forward
/// bearing + 180, so b_{k+1} = b_k + 180 + a_k. With angle left: b_{k+1} = b_k + 180 - a_k.
/// Returns angles.len() + 1 bearings.
pub fn bearings_from_angles(first_bearing_deg: f64, angles_deg: &[f64], mode: AngleMode) -> Vec<f64> {
    let mut out = vec![normalize_deg(first_bearing_deg)];
    for a in angles_deg {
        let prev = *out.last().unwrap();
        let next = match mode {
            AngleMode::Right => prev + 180.0 + a,
            AngleMode::Left => prev + 180.0 - a,
        };
        out.push(normalize_deg(next));
    }
    out
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct AngleAdjustment {
    /// Misclosure in degrees (sum minus theory for a loop; computed end bearing minus known end
    /// bearing, in (-180, 180], for a traverse between known bearings).
    pub misclosure_deg: f64,
    /// Correction added to each angle (equal shares).
    pub correction_each_deg: f64,
    pub adjusted_deg: Vec<f64>,
}

impl AngleAdjustment {
    /// True when |misclosure| <= `tolerance_deg`, a value the caller chooses.
    pub fn within(&self, tolerance_deg: f64) -> bool {
        self.misclosure_deg.abs() <= tolerance_deg
    }
}

/// Interior angles of a closed polygon of n sides: the sum should be (n - 2) 180 degrees (plane
/// geometry: split the polygon into n - 2 triangles from one vertex). The misclosure is shared
/// equally between the n angles. Needs n >= 3 angles.
pub fn adjust_interior_angles(angles_deg: &[f64]) -> Result<AngleAdjustment> {
    let n = angles_deg.len();
    if n < 3 {
        return Err(CogoError::Invalid("a polygon needs at least three angles".into()));
    }
    let sum: f64 = angles_deg.iter().sum();
    let mis = sum - (n as f64 - 2.0) * 180.0;
    let c = -mis / n as f64;
    Ok(AngleAdjustment { misclosure_deg: mis, correction_each_deg: c, adjusted_deg: angles_deg.iter().map(|a| a + c).collect() })
}

/// Angles between a first bearing and a known last bearing: the end bearing is computed through the
/// angles and compared with the known one. Adding d to every one of the m angles shifts the end
/// bearing by m d (angle right) or -m d (angle left), so d = -misclosure / m (right) and
/// +misclosure / m (left).
pub fn adjust_angles_to_known_bearing(first_bearing_deg: f64, angles_deg: &[f64], mode: AngleMode, known_last_bearing_deg: f64) -> Result<AngleAdjustment> {
    if angles_deg.is_empty() {
        return Err(CogoError::Invalid("no angles".into()));
    }
    let end = *bearings_from_angles(first_bearing_deg, angles_deg, mode).last().unwrap();
    let mis = signed_deg(end - known_last_bearing_deg);
    let m = angles_deg.len() as f64;
    let c = match mode {
        AngleMode::Right => -mis / m,
        AngleMode::Left => mis / m,
    };
    Ok(AngleAdjustment { misclosure_deg: mis, correction_each_deg: c, adjusted_deg: angles_deg.iter().map(|a| a + c).collect() })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn rect(n0: f64, d1: f64) -> Traverse {
        // 100 by 50 rectangle clockwise from (e0, n0), first course lengthened to d1
        Traverse::new(
            Xy::new(500000.0, n0),
            vec![
                Course { bearing_deg: 0.0, distance: d1 },
                Course { bearing_deg: 90.0, distance: 100.0 },
                Course { bearing_deg: 180.0, distance: 50.0 },
                Course { bearing_deg: 270.0, distance: 100.0 },
            ],
            Closure::Loop,
        )
        .unwrap()
    }

    #[test]
    fn misclosure_and_ratio() {
        let t = rect(1000.0, 50.02);
        let m = t.misclosure().unwrap();
        assert!((m.error_n - 0.02).abs() < 1e-9 && m.error_e.abs() < 1e-9);
        assert!((m.linear - 0.02).abs() < 1e-9);
        assert!((m.perimeter - 300.02).abs() < 1e-12);
        assert!((m.precision_denominator.unwrap() - 300.02 / 0.02).abs() < 1e-3);
        assert!((m.direction_to_known_deg.unwrap() - 180.0).abs() < 1e-6);
        assert!(m.meets_ratio(10000.0) && !m.meets_ratio(20000.0));
        assert!(rect(1000.0, 50.0).misclosure().unwrap().precision_denominator.map_or(true, |n| n > 1e8));
    }

    #[test]
    fn bowditch_closed_form_at_utm_magnitude() {
        for n0 in [1000.0, 9999950.0] {
            let t = rect(n0, 50.02);
            let a = t.adjust(AdjustMethod::Bowditch).unwrap();
            // Hand result: latitude correction of course i is -0.02 L_i / 300.02.
            let want = [-0.02 * 50.02 / 300.02, -0.02 * 100.0 / 300.02, -0.02 * 50.0 / 300.02, -0.02 * 100.0 / 300.02];
            for (c, w) in a.corrections.iter().zip(want) {
                assert!((c.1 - w).abs() < 1e-9, "{c:?} {w}");
            }
            // Easting error is zero so departures need no correction
            assert!(a.corrections.iter().all(|c| c.0.abs() < 1e-9));
            let last = *a.points.last().unwrap();
            assert_eq!((last.e, last.n), (500000.0, n0));
            // adjusted courses form a closed figure: the sums of components vanish
            let (se, sn) = a.courses.iter().fold((0.0, 0.0), |s, c| (s.0 + c.distance * c.bearing_deg.to_radians().sin(), s.1 + c.distance * c.bearing_deg.to_radians().cos()));
            assert!(se.abs() < 1e-8 && sn.abs() < 1e-8);
        }
    }

    #[test]
    fn transit_closed_form() {
        let t = rect(1000.0, 50.02);
        let a = t.adjust(AdjustMethod::Transit).unwrap();
        // sum |dN| = 50.02 + 50 = 100.02; correction -0.02 |dN_i| / 100.02
        assert!((a.corrections[0].1 + 0.02 * 50.02 / 100.02).abs() < 1e-9);
        assert!((a.corrections[2].1 + 0.02 * 50.0 / 100.02).abs() < 1e-9);
        assert!(a.corrections[1].1.abs() < 1e-12 && a.corrections[3].1.abs() < 1e-12);
        let total: f64 = a.corrections.iter().map(|c| c.1).sum();
        assert!((total + 0.02).abs() < 1e-9);
    }

    #[test]
    fn bowditch_general_properties() {
        // A skewed loop with errors in both axes: corrections sum to minus the error and are
        // proportional to length.
        let t = Traverse::new(
            Xy::new(100.0, 200.0),
            vec![
                Course { bearing_deg: 30.0, distance: 80.0 },
                Course { bearing_deg: 120.0, distance: 60.0 },
                Course { bearing_deg: 215.0, distance: 85.0 },
                Course { bearing_deg: 300.0, distance: 55.3 },
            ],
            Closure::Loop,
        )
        .unwrap();
        let m = t.misclosure().unwrap();
        let a = t.adjust(AdjustMethod::Bowditch).unwrap();
        let (ce, cn) = a.corrections.iter().fold((0.0, 0.0), |s, c| (s.0 + c.0, s.1 + c.1));
        assert!((ce + m.error_e).abs() < 1e-12 && (cn + m.error_n).abs() < 1e-12);
        assert!((a.corrections[0].0 / a.corrections[3].0 - 80.0 / 55.3).abs() < 1e-9);
    }

    #[test]
    fn to_known_point_and_open() {
        let t = Traverse::new(Xy::new(0.0, 0.0), vec![Course { bearing_deg: 90.0, distance: 100.0 }, Course { bearing_deg: 90.0, distance: 100.0 }], Closure::ToPoint(Xy::new(200.0, 0.4))).unwrap();
        let m = t.misclosure().unwrap();
        assert!((m.error_n + 0.4).abs() < 1e-12);
        let a = t.adjust(AdjustMethod::Bowditch).unwrap();
        assert!((a.points[1].n - 0.2).abs() < 1e-12);
        assert_eq!(*a.points.last().unwrap(), Xy::new(200.0, 0.4));
        let o = Traverse::new(Xy::new(0.0, 0.0), vec![Course { bearing_deg: 0.0, distance: 1.0 }], Closure::Open).unwrap();
        assert_eq!(o.misclosure(), Err(CogoError::OpenTraverse));
        assert_eq!(o.points().len(), 2);
        assert!(Traverse::new(Xy::new(0.0, 0.0), vec![], Closure::Open).is_err());
    }

    #[test]
    fn interior_angles() {
        // Square with +40" total error, shared 10" each.
        let e = 40.0 / 3600.0;
        let a = adjust_interior_angles(&[90.0 + e, 90.0, 90.0, 90.0]).unwrap();
        assert!((a.misclosure_deg - e).abs() < 1e-12);
        assert!((a.adjusted_deg.iter().sum::<f64>() - 360.0).abs() < 1e-12);
        assert!(a.within(1.0 / 60.0) && !a.within(1.0 / 3600.0));
        assert!(adjust_interior_angles(&[90.0, 90.0]).is_err());
    }

    #[test]
    fn bearings_through_angles() {
        // Walking anticlockwise round a square, the interior angle is the angle to the right.
        let b = bearings_from_angles(0.0, &[90.0, 90.0, 90.0, 90.0], AngleMode::Right);
        assert_eq!(b.iter().map(|x| x.round() as i32).collect::<Vec<_>>(), vec![0, 270, 180, 90, 0]);
        // Walking clockwise, the interior angle is the angle to the left.
        let b = bearings_from_angles(0.0, &[90.0, 90.0, 90.0], AngleMode::Left);
        assert_eq!(b.iter().map(|x| x.round() as i32).collect::<Vec<_>>(), vec![0, 90, 180, 270]);
    }

    #[test]
    fn angles_to_known_bearing() {
        // Three angles right of 90 from bearing 0 end at 90 (0, 270, 180, 90); first one measured 0.003 high.
        let a = adjust_angles_to_known_bearing(0.0, &[90.003, 90.0, 90.0], AngleMode::Right, 90.0).unwrap();
        assert!((a.misclosure_deg - 0.003).abs() < 1e-12);
        let end = *bearings_from_angles(0.0, &a.adjusted_deg, AngleMode::Right).last().unwrap();
        assert!((end - 90.0).abs() < 1e-9);
        // Angles left end at 270; a high angle turns the end bearing the other way.
        let a = adjust_angles_to_known_bearing(0.0, &[90.003, 90.0, 90.0], AngleMode::Left, 270.0).unwrap();
        assert!((a.misclosure_deg + 0.003).abs() < 1e-12);
        let end = *bearings_from_angles(0.0, &a.adjusted_deg, AngleMode::Left).last().unwrap();
        assert!((end - 270.0).abs() < 1e-9, "{end}");
    }
}
