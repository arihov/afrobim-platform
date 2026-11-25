// SPDX-License-Identifier: LicenseRef-AfroBIM-Proprietary
// Copyright (c) 2026 AfroBIM. All rights reserved.
//! Geometry checks shared by the public fixture test and the private-file test: how well the
//! alignments the reader built agree with the points the file states.
//!
//! None of it trusts the reader's own clothoid code: a spiral's end is also computed here by
//! integrating the heading `θ(s) = θ0 + k0 s + (k1 - k0) s² / (2 L)` with Simpson's rule, from the
//! start point and the direction `Start → PI` (the PI is the intersection of the tangents at the two
//! ends), and compared with the `End` the file states.

#![allow(dead_code)]

use afrobim_alignment::{Alignment, LandXmlFile, LandXmlGeomElement};

/// A worst value and the index of the alignment it was found on.
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct Worst {
    pub value: f64,
    pub alignment: Option<usize>,
}

impl Worst {
    fn update(&mut self, value: f64, alignment: usize) {
        if self.alignment.is_none() || value > self.value {
            self.value = value;
            self.alignment = Some(alignment);
        }
    }
}

#[derive(Debug, Default)]
pub struct GeometryReport {
    /// End of one element to the start of the next, as the file states them (m).
    pub join: Worst,
    /// |Center to Start − Center to End| and |radius attribute − Center to Start| of each Curve (m).
    pub radius: Worst,
    /// |sum of the engine element lengths − Alignment@length| (m).
    pub length: Worst,
    /// |stated length − length of the engine element| for each Curve and Spiral (m).
    pub element_length: Worst,
    /// The file's `End` of each Spiral against the end computed here by integration (m).
    pub spiral_end_independent: Worst,
    /// The file's `End` of each Spiral against the end of the engine's element (m).
    pub spiral_end_engine: Worst,
    /// The file's `End` of every element against the end of the engine's element (m).
    pub element_end_engine: Worst,
    /// The file's `Start` of every element against the start of the engine's element (m).
    pub element_start_engine: Worst,
    /// The engine's start heading against the direction the file implies: `Start → PI` for a
    /// spiral, the tangent at `Start` of the circle for a curve, `Start → End` for a line (rad).
    pub start_heading: Worst,
    /// Joints that are not tangent: (alignment, index of the element that starts there, heading change in degrees).
    pub kinks: Vec<(usize, usize, f64)>,
    pub spirals: usize,
    pub curves: usize,
    pub lines: usize,
    /// Alignments whose raw and engine element counts differ (polylines, dropped zero-length elements).
    pub not_compared: Vec<usize>,
}

fn dist(a: (f64, f64), b: (f64, f64)) -> f64 {
    (a.0 - b.0).hypot(a.1 - b.1)
}

fn xy(p: &afrobim_alignment::LandXmlPoint) -> (f64, f64) {
    (p.x, p.y)
}

fn wrap(a: f64) -> f64 {
    let t = std::f64::consts::TAU;
    let r = (a + std::f64::consts::PI).rem_euclid(t) - std::f64::consts::PI;
    r
}

/// Simpson's rule for the end of a clothoid element: start `(x, y)`, start heading `th0`, start and
/// end curvature `k0`, `k1` (signed), length `len`.
pub fn clothoid_end(start: (f64, f64), th0: f64, k0: f64, k1: f64, len: f64) -> (f64, f64) {
    let th = |s: f64| th0 + k0 * s + (k1 - k0) * s * s / (2.0 * len);
    let n = 8000;
    let h = len / n as f64;
    let (mut sx, mut sy) = (th(0.0).cos() + th(len).cos(), th(0.0).sin() + th(len).sin());
    for i in 1..n {
        let s = h * i as f64;
        let w = if i % 2 == 1 { 4.0 } else { 2.0 };
        sx += w * th(s).cos();
        sy += w * th(s).sin();
    }
    (start.0 + sx * h / 3.0, start.1 + sy * h / 3.0)
}

pub fn geometry_report(file: &LandXmlFile) -> GeometryReport {
    let mut r = GeometryReport::default();
    for (ai, a) in file.alignments.iter().enumerate() {
        let info = file.alignment_info(&a.name).expect("every imported alignment has its info");
        let raw: Vec<&LandXmlGeomElement> = info.coord_geoms.iter().flat_map(|g| g.elements.iter()).collect();
        r.length.update((a.length() - info.declared_length_m.expect("a stated length")).abs(), ai);

        // Joins as the file states them.
        for w in raw.windows(2) {
            let end = match w[0] {
                LandXmlGeomElement::Line { end, .. } | LandXmlGeomElement::Curve { end, .. } | LandXmlGeomElement::Spiral { end, .. } => xy(end),
                _ => continue,
            };
            let start = match w[1] {
                LandXmlGeomElement::Line { start, .. } | LandXmlGeomElement::Curve { start, .. } | LandXmlGeomElement::Spiral { start, .. } => xy(start),
                _ => continue,
            };
            r.join.update(dist(end, start), ai);
        }
        for j in a.discontinuities(1e-3, 1e-6) {
            r.kinks.push((ai, j.index, j.heading_change_rad.to_degrees()));
        }

        if raw.iter().any(|e| matches!(e, LandXmlGeomElement::IrregularLine { .. } | LandXmlGeomElement::Chain { .. })) || raw.len() != a.elements.len() {
            r.not_compared.push(ai);
            continue;
        }
        for (e, g) in raw.iter().zip(&a.elements) {
            let (ex, ey, _) = g.end();
            match e {
                LandXmlGeomElement::Line { start, end } => {
                    r.lines += 1;
                    r.element_start_engine.update(dist(xy(start), (g.x, g.y)), ai);
                    r.element_end_engine.update(dist(xy(end), (ex, ey)), ai);
                    r.start_heading.update(wrap((end.y - start.y).atan2(end.x - start.x) - g.heading_rad).abs(), ai);
                }
                LandXmlGeomElement::Curve { start, center, end, clockwise, radius_m, length_m, .. } => {
                    r.curves += 1;
                    let rs = dist(xy(start), xy(center));
                    r.radius.update((rs - dist(xy(end), xy(center))).abs(), ai);
                    if let Some(rad) = radius_m {
                        r.radius.update((rs - rad).abs(), ai);
                    }
                    if let Some(l) = length_m {
                        r.element_length.update((l - g.length).abs(), ai);
                    }
                    r.element_start_engine.update(dist(xy(start), (g.x, g.y)), ai);
                    r.element_end_engine.update(dist(xy(end), (ex, ey)), ai);
                    let turn = if *clockwise { -1.0 } else { 1.0 };
                    let h0 = (start.y - center.y).atan2(start.x - center.x) + turn * std::f64::consts::FRAC_PI_2;
                    r.start_heading.update(wrap(h0 - g.heading_rad).abs(), ai);
                }
                LandXmlGeomElement::Spiral { start, pi, end, clockwise, length_m, radius_start_m, radius_end_m, .. } => {
                    r.spirals += 1;
                    let turn = if *clockwise { -1.0 } else { 1.0 };
                    let k = |rad: &Option<f64>| rad.map_or(0.0, |v| turn / v);
                    r.element_length.update((length_m - g.length).abs(), ai);
                    r.element_start_engine.update(dist(xy(start), (g.x, g.y)), ai);
                    r.element_end_engine.update(dist(xy(end), (ex, ey)), ai);
                    r.spiral_end_engine.update(dist(xy(end), (ex, ey)), ai);
                    let pi = pi.expect("a Civil 3D spiral has a PI");
                    let th0 = (pi.y - start.y).atan2(pi.x - start.x);
                    r.start_heading.update(wrap(th0 - g.heading_rad).abs(), ai);
                    let calc = clothoid_end(xy(start), th0, k(radius_start_m), k(radius_end_m), *length_m);
                    r.spiral_end_independent.update(dist(xy(end), calc), ai);
                }
                _ => {}
            }
        }
    }
    r
}

/// Station to point and point to station along an alignment, every `step` metres and at both ends:
/// the worst station difference and the worst |offset| found when the point is put back.
pub fn station_round_trip(a: &Alignment, step: f64) -> (f64, f64, usize) {
    let (mut worst_station, mut worst_offset, mut n) = (0.0f64, 0.0f64, 0usize);
    let start = a.start_station_m;
    let end = a.end_station_m();
    let mut s = start;
    loop {
        let p = a.point_at(s.min(end)).expect("on the alignment");
        let (st, off) = a.station_offset(p.x, p.y).expect("the point is on the alignment");
        worst_station = worst_station.max((st - s.min(end)).abs());
        worst_offset = worst_offset.max(off.abs());
        n += 1;
        if s >= end {
            break;
        }
        s += step;
    }
    (worst_station, worst_offset, n)
}
