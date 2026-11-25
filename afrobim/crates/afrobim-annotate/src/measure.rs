// SPDX-License-Identifier: LicenseRef-AfroBIM-Proprietary
// Copyright (c) 2026 AfroBIM. All rights reserved.
//! Measuring: answers without creating an element.
//!
//! Every function takes model coordinates (millimetres) and returns a [`MeasureResult`]: the
//! structured [`Measurement`] and a one-line text for the command line, formatted with the
//! document's [`DisplayUnits`].
//!
//! Conventions: the signed area of a polygon is the shoelace sum
//! `A = 1/2 * sum(x_i * y_(i+1) - x_(i+1) * y_i)` over the closed ring with Y up, so it is
//! **positive when the vertices run anticlockwise and negative when clockwise**. The reported
//! area is `|A|`. For a ring that crosses itself the shoelace sum is the net area (regions wound
//! the other way subtract); the result says so in `self_intersecting`. Perimeter always includes
//! the closing edge.

use afrobim_alignment::Alignment;
use serde::{Deserialize, Serialize};

use crate::dim::station_offset_cache;
use crate::geometry::*;
use crate::units::DisplayUnits;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum Winding {
    Anticlockwise,
    Clockwise,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum Measurement {
    Distance {
        distance_mm: f64,
        dx_mm: f64,
        dy_mm: f64,
        /// Degrees anticlockwise from +X, from the first point to the second.
        direction_deg: f64,
    },
    Angle {
        /// The angle between the arms, 0 to 180 degrees.
        degrees: f64,
        /// Direction of each arm from the vertex, degrees anticlockwise from +X.
        direction1_deg: f64,
        direction2_deg: f64,
    },
    Radius { centre: Pt, radius_mm: f64, diameter_mm: f64 },
    Polygon {
        /// `|A|`, mm2.
        area_mm2: f64,
        /// `A` itself, mm2 (see the module docs for the sign).
        signed_area_mm2: f64,
        perimeter_mm: f64,
        winding: Winding,
        vertices: usize,
        self_intersecting: bool,
    },
    Cumulative { legs_mm: Vec<f64>, running_mm: Vec<f64>, total_mm: f64 },
    OnAlignment {
        alignment: String,
        /// The station as displayed (through station equations), m.
        station_m: f64,
        /// The continuous station along the alignment, m.
        internal_station_m: f64,
        region: usize,
        /// Metres, positive to the right looking up-station.
        offset_m: f64,
        /// Centreline level at that station, when the alignment has a profile covering it.
        level_m: Option<f64>,
    },
}

#[derive(Debug, Clone, PartialEq)]
pub struct MeasureResult {
    pub measurement: Measurement,
    /// One line for the command line.
    pub text: String,
}

pub fn distance(a: Pt, b: Pt, u: &DisplayUnits) -> MeasureResult {
    let d = sub(b, a);
    let dir = if len(d) > 0.0 { angle_of(d) } else { 0.0 };
    let m = Measurement::Distance { distance_mm: len(d), dx_mm: d.x, dy_mm: d.y, direction_deg: dir };
    let text = format!("Distance = {}, dX = {}, dY = {}, direction {}", u.length_u(len(d)), u.length_u(d.x), u.length_u(d.y), u.direction(dir));
    MeasureResult { measurement: m, text }
}

/// The angle at `vertex` between the arms through `p1` and `p2`, 0 to 180 degrees.
pub fn angle(vertex: Pt, p1: Pt, p2: Pt, u: &DisplayUnits) -> Result<MeasureResult, String> {
    let (a, b) = (sub(p1, vertex), sub(p2, vertex));
    if len(a) < 1e-9 || len(b) < 1e-9 {
        return Err("An arm of the angle has no length.".into());
    }
    let deg = cross(a, b).abs().atan2(dot(a, b)).to_degrees();
    let (d1, d2) = (angle_of(a), angle_of(b));
    let text = format!("Angle = {}, arm directions {} and {}", u.angle(deg), u.direction(d1), u.direction(d2));
    Ok(MeasureResult { measurement: Measurement::Angle { degrees: deg, direction1_deg: d1, direction2_deg: d2 }, text })
}

/// The circle through three points.
pub fn radius_three_points(a: Pt, b: Pt, c: Pt, u: &DisplayUnits) -> Result<MeasureResult, String> {
    let d = 2.0 * (a.x * (b.y - c.y) + b.x * (c.y - a.y) + c.x * (a.y - b.y));
    let scale = len(sub(b, a)).max(len(sub(c, a))).max(1.0);
    if d.abs() < 1e-12 * scale * scale {
        return Err("The three points are in a line: they do not define a circle.".into());
    }
    let (a2, b2, c2) = (a.x * a.x + a.y * a.y, b.x * b.x + b.y * b.y, c.x * c.x + c.y * c.y);
    let centre = pt((a2 * (b.y - c.y) + b2 * (c.y - a.y) + c2 * (a.y - b.y)) / d, (a2 * (c.x - b.x) + b2 * (a.x - c.x) + c2 * (b.x - a.x)) / d);
    let r = dist(centre, a);
    let text = format!("Radius = {}, diameter {}, centre ({}, {})", u.length_u(r), u.length_u(2.0 * r), u.length(centre.x), u.length(centre.y));
    Ok(MeasureResult { measurement: Measurement::Radius { centre, radius_mm: r, diameter_mm: 2.0 * r }, text })
}

/// Area and perimeter of the closed polygon through `pts`.
pub fn polygon(pts: &[Pt], u: &DisplayUnits) -> Result<MeasureResult, String> {
    if pts.len() < 3 {
        return Err("A polygon needs at least three points.".into());
    }
    let n = pts.len();
    let signed = 0.5 * (0..n).map(|i| pts[i].x * pts[(i + 1) % n].y - pts[(i + 1) % n].x * pts[i].y).sum::<f64>();
    let perimeter: f64 = (0..n).map(|i| dist(pts[i], pts[(i + 1) % n])).sum();
    let winding = if signed >= 0.0 { Winding::Anticlockwise } else { Winding::Clockwise };
    let crossing = self_intersects(pts);
    let mut text = format!("Area = {}, perimeter = {}, {} points, {}", u.area_u(signed.abs()), u.length_u(perimeter), n, if winding == Winding::Anticlockwise { "anticlockwise" } else { "clockwise" });
    if crossing {
        text.push_str(", crosses itself (net area)");
    }
    Ok(MeasureResult {
        measurement: Measurement::Polygon { area_mm2: signed.abs(), signed_area_mm2: signed, perimeter_mm: perimeter, winding, vertices: n, self_intersecting: crossing },
        text,
    })
}

fn self_intersects(pts: &[Pt]) -> bool {
    let n = pts.len();
    let seg = |i: usize| (pts[i], pts[(i + 1) % n]);
    for i in 0..n {
        for j in i + 1..n {
            // Neighbouring edges share a vertex.
            if j == i + 1 || (i == 0 && j == n - 1) {
                continue;
            }
            let ((a, b), (c, d)) = (seg(i), seg(j));
            let (o1, o2, o3, o4) = (cross(sub(b, a), sub(c, a)), cross(sub(b, a), sub(d, a)), cross(sub(d, c), sub(a, c)), cross(sub(d, c), sub(b, c)));
            if o1 * o2 < 0.0 && o3 * o4 < 0.0 {
                return true;
            }
        }
    }
    false
}

/// Distances along a picked path: each leg, the running total, and the total.
pub fn cumulative(pts: &[Pt], u: &DisplayUnits) -> Result<MeasureResult, String> {
    if pts.len() < 2 {
        return Err("Pick at least two points.".into());
    }
    let legs: Vec<f64> = pts.windows(2).map(|w| dist(w[0], w[1])).collect();
    let mut running = Vec::with_capacity(legs.len());
    let mut acc = 0.0;
    for l in &legs {
        acc += l;
        running.push(acc);
    }
    let text = format!("Leg {} = {}, total {}", legs.len(), u.length_u(*legs.last().unwrap()), u.length_u(acc));
    Ok(MeasureResult { measurement: Measurement::Cumulative { legs_mm: legs, running_mm: running, total_mm: acc }, text })
}

/// Station, offset and level of a point (site millimetres) relative to an alignment.
pub fn on_alignment(alignment: &Alignment, site_point_mm: Pt, u: &DisplayUnits) -> Result<MeasureResult, String> {
    let (x, y) = (site_point_mm.x / 1000.0, site_point_mm.y / 1000.0);
    let (internal, offset) = alignment.station_offset(x, y).ok_or("The point has no perpendicular foot on the alignment (it is before the start or beyond the end).")?;
    let cache = station_offset_cache(alignment, site_point_mm);
    let (station, region) = match (cache.station_m, cache.region) {
        (Some(s), Some(r)) => (s, r),
        _ => return Err("The station is off the alignment.".into()),
    };
    let level = alignment.profile.as_ref().and_then(|p| p.elevation_at(internal).ok());
    let mut text = format!("Station {}, offset {} m {}", u.station(station), crate::units::fixed(offset.abs(), u.length_decimals), if offset >= 0.0 { "right" } else { "left" });
    if let Some(l) = level {
        text.push_str(&format!(", level {} m", crate::units::fixed(l, u.length_decimals)));
    }
    Ok(MeasureResult {
        measurement: Measurement::OnAlignment { alignment: alignment.name.clone(), station_m: station, internal_station_m: internal, region, offset_m: offset, level_m: level },
        text,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::units::{AngleFormat, LengthUnit};

    fn u() -> DisplayUnits {
        DisplayUnits::default()
    }

    #[test]
    fn distance_is_hand_worked() {
        // 3-4-5 triangle in metres.
        let r = distance(pt(1000.0, 1000.0), pt(4000.0, 5000.0), &u());
        let Measurement::Distance { distance_mm, dx_mm, dy_mm, direction_deg } = r.measurement else { panic!() };
        assert_eq!((distance_mm, dx_mm, dy_mm), (5000.0, 3000.0, 4000.0));
        assert!((direction_deg - 53.13010235415598).abs() < 1e-9);
        assert_eq!(r.text, "Distance = 5.000 m, dX = 3.000 m, dY = 4.000 m, direction 53.13\u{b0}");
        let mut units = u();
        units.length = LengthUnit::Millimetre;
        units.length_decimals = 0;
        units.angle = AngleFormat::QuadrantBearing;
        units.angle_decimals = 0;
        assert_eq!(distance(pt(0.0, 0.0), pt(0.0, 100.0), &units).text, "Distance = 100 mm, dX = 0 mm, dY = 100 mm, direction N 0\u{b0}00'00\" E");
    }

    #[test]
    fn angle_is_the_interior_angle() {
        let r = angle(pt(0.0, 0.0), pt(100.0, 0.0), pt(0.0, 100.0), &u()).unwrap();
        let Measurement::Angle { degrees, .. } = r.measurement else { panic!() };
        assert!((degrees - 90.0).abs() < 1e-9);
        // 300 degrees of rotation measured as 60 (interior).
        let b = angle(pt(0.0, 0.0), pt(100.0, 0.0), rot(pt(100.0, 0.0), 300.0), &u()).unwrap();
        let Measurement::Angle { degrees, .. } = b.measurement else { panic!() };
        assert!((degrees - 60.0).abs() < 1e-9);
        assert!(angle(pt(0.0, 0.0), pt(0.0, 0.0), pt(1.0, 0.0), &u()).is_err());
    }

    #[test]
    fn radius_of_three_points() {
        // Points on the circle centre (10, -5) radius 25.
        let c = pt(10_000.0, -5_000.0);
        let on = |deg: f64| add(c, mul(dir(deg), 25_000.0));
        let r = radius_three_points(on(10.0), on(100.0), on(200.0), &u()).unwrap();
        let Measurement::Radius { centre, radius_mm, diameter_mm } = r.measurement else { panic!() };
        assert!((radius_mm - 25_000.0).abs() < 1e-6 && (diameter_mm - 50_000.0).abs() < 1e-6);
        assert!(dist(centre, c) < 1e-6);
        assert!(radius_three_points(pt(0.0, 0.0), pt(1.0, 1.0), pt(2.0, 2.0), &u()).is_err());
    }

    #[test]
    fn polygon_area_perimeter_and_sign_convention() {
        // A 4 m by 3 m rectangle, anticlockwise: area 12 m2 = 1.2e7 mm2, perimeter 14 m.
        let ccw = [pt(0.0, 0.0), pt(4000.0, 0.0), pt(4000.0, 3000.0), pt(0.0, 3000.0)];
        let r = polygon(&ccw, &u()).unwrap();
        let Measurement::Polygon { area_mm2, signed_area_mm2, perimeter_mm, winding, .. } = r.measurement else { panic!() };
        assert_eq!((area_mm2, signed_area_mm2, perimeter_mm, winding), (1.2e7, 1.2e7, 14_000.0, Winding::Anticlockwise));
        assert_eq!(r.text, "Area = 12.00 m2, perimeter = 14.000 m, 4 points, anticlockwise");
        let cw: Vec<Pt> = ccw.iter().rev().copied().collect();
        let Measurement::Polygon { area_mm2, signed_area_mm2, winding, .. } = polygon(&cw, &u()).unwrap().measurement else { panic!() };
        assert_eq!((area_mm2, signed_area_mm2, winding), (1.2e7, -1.2e7, Winding::Clockwise));
        // A right triangle with legs 6 and 8: area 24, perimeter 24 (6 + 8 + 10).
        let t = polygon(&[pt(0.0, 0.0), pt(6000.0, 0.0), pt(0.0, 8000.0)], &u()).unwrap();
        let Measurement::Polygon { area_mm2, perimeter_mm, .. } = t.measurement else { panic!() };
        assert_eq!((area_mm2, perimeter_mm), (2.4e7, 24_000.0));
        // Hectares.
        let mut h = u();
        h.area = crate::units::AreaUnit::Hectare;
        h.area_decimals = 4;
        let big = polygon(&[pt(0.0, 0.0), pt(100_000.0, 0.0), pt(100_000.0, 100_000.0), pt(0.0, 100_000.0)], &h).unwrap();
        assert!(big.text.starts_with("Area = 1.0000 ha"));
        // A bow tie crosses itself: net area, flagged.
        let bow = polygon(&[pt(0.0, 0.0), pt(2000.0, 2000.0), pt(2000.0, 0.0), pt(0.0, 2000.0)], &u()).unwrap();
        let Measurement::Polygon { self_intersecting, signed_area_mm2, .. } = bow.measurement else { panic!() };
        assert!(self_intersecting && signed_area_mm2.abs() < 1e-6);
        assert!(polygon(&ccw[..2], &u()).is_err());
    }

    #[test]
    fn cumulative_distance() {
        let r = cumulative(&[pt(0.0, 0.0), pt(3000.0, 4000.0), pt(3000.0, 6000.0), pt(0.0, 6000.0)], &u()).unwrap();
        let Measurement::Cumulative { legs_mm, running_mm, total_mm } = r.measurement else { panic!() };
        assert_eq!(legs_mm, vec![5000.0, 2000.0, 3000.0]);
        assert_eq!(running_mm, vec![5000.0, 7000.0, 10_000.0]);
        assert_eq!(total_mm, 10_000.0);
        assert_eq!(r.text, "Leg 3 = 3.000 m, total 10.000 m");
        assert!(cumulative(&[pt(0.0, 0.0)], &u()).is_err());
    }

    #[test]
    fn station_offset_and_level_on_an_alignment() {
        use afrobim_alignment::{Profile, Pvi};
        let mut a = afrobim_alignment::AlignmentBuilder::new("A", 0.0, 0.0, 0.0, 0.0).line(2000.0).build().unwrap();
        a.profile = Some(Profile::new("P", vec![Pvi::point(0.0, 100.0), Pvi::point(2000.0, 120.0)]).unwrap());
        // 1 % grade: level at 1250.5 is 100 + 12.505 = 112.505.
        let r = on_alignment(&a, pt(1_250_500.0, -4500.0), &u()).unwrap();
        let Measurement::OnAlignment { station_m, offset_m, level_m, .. } = r.measurement else { panic!() };
        assert!((station_m - 1250.5).abs() < 1e-9 && (offset_m - 4.5).abs() < 1e-9 && (level_m.unwrap() - 112.505).abs() < 1e-9);
        assert_eq!(r.text, "Station 1+250.500, offset 4.500 m right, level 112.505 m");
        assert!(on_alignment(&a, pt(-10_000.0, 0.0), &u()).is_err());
    }
}
