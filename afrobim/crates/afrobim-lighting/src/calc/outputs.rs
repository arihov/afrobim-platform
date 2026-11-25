// SPDX-License-Identifier: LicenseRef-AfroBIM-Proprietary
// Copyright (c) 2026 AfroBIM. All rights reserved.
//! Drawings and CSV of a calculation: iso-illuminance contours, the grid of values in plan, the
//! polar diagram of a luminaire, and the grid as CSV.
//!
//! Drawings are in millimetres like the other drawings of this crate, with plan metres turned into
//! millimetres relative to a recorded origin (`origin_x_m`, `origin_y_m`: the plan position of the
//! drawing's (0, 0)). Lettering is not drawn as glyphs (this crate has no font): texts are
//! returned separately as [`DrawingLabel`]s at positions in the same units, for the front end to
//! draw.

use afrobim_draw::{Colour, Drawing, Extents, Primitive, Pt2};

use crate::{fixed, Photometry};

use super::engine::{AreaResult, CalcResult};
use super::frame::LuminaireInstance;

/// A text to be drawn at a position, in drawing units (millimetres).
#[derive(Debug, Clone, PartialEq)]
pub struct DrawingLabel {
    pub text: String,
    pub x: f64,
    pub y: f64,
    /// Text height, mm.
    pub height: f64,
    pub rotation_deg: f64,
}

/// A drawing and its labels.
#[derive(Debug, Clone, PartialEq)]
pub struct LabelledDrawing {
    pub drawing: Drawing,
    pub labels: Vec<DrawingLabel>,
    /// Plan position (metres) of the drawing's origin; 0 for diagrams that have no plan position.
    pub origin_x_m: f64,
    pub origin_y_m: f64,
}

const PALETTE: [Colour; 5] = [Colour::Blue, Colour::Green, Colour::Brown, Colour::Red, Colour::Black];

fn extents_of(prims: &[Primitive], labels: &[DrawingLabel]) -> Extents {
    let (mut min, mut max) = (Pt2::new(f64::INFINITY, f64::INFINITY), Pt2::new(f64::NEG_INFINITY, f64::NEG_INFINITY));
    let mut add = |p: Pt2| {
        min = Pt2::new(min.x.min(p.x), min.y.min(p.y));
        max = Pt2::new(max.x.max(p.x), max.y.max(p.y));
    };
    for p in prims {
        match p {
            Primitive::Line { from, to, .. } => {
                add(*from);
                add(*to);
            }
            Primitive::Arc { centre, radius, .. } => {
                add(Pt2::new(centre.x - radius, centre.y - radius));
                add(Pt2::new(centre.x + radius, centre.y + radius));
            }
            Primitive::Fill { outline, .. } => outline.iter().for_each(|q| add(*q)),
            _ => {}
        }
    }
    for l in labels {
        add(Pt2::new(l.x, l.y));
    }
    if min.x.is_finite() {
        Extents { min, max }
    } else {
        Extents::default()
    }
}

/// "Nice" contour levels (1, 2, 2.5, 5 times a power of ten) strictly between `min` and `max`,
/// about `target` of them. A presentation aid only; levels can also be entered.
pub fn nice_levels(min: f64, max: f64, target: usize) -> Vec<f64> {
    if max.partial_cmp(&min) != Some(std::cmp::Ordering::Greater) || target == 0 {
        return Vec::new();
    }
    let raw = (max - min) / target as f64;
    let mag = 10f64.powf(raw.log10().floor());
    let step = [1.0, 2.0, 2.5, 5.0, 10.0].iter().map(|m| m * mag).find(|s| *s >= raw).unwrap_or(10.0 * mag);
    let mut out = Vec::new();
    let mut v = (min / step).floor() * step + step;
    while v < max - 1e-12 {
        if v > min + 1e-12 {
            out.push((v / step).round() * step);
        }
        v += step;
    }
    out
}

/// Line segments (plan metres) of the iso-lines of `values` on the quadrilateral lattice whose
/// vertices are the points of `area`, by marching squares.
pub(crate) fn contour_segments(area: &AreaResult, values: &[f64], level: f64) -> Vec<([f64; 2], [f64; 2])> {
    let (na, nc) = (area.n_along, area.n_across);
    let mut out = Vec::new();
    if na < 2 || nc < 2 {
        return out;
    }
    let at = |i: usize, j: usize| i * nc + j;
    let pos = |i: usize, j: usize| [area.points[at(i, j)].east_m, area.points[at(i, j)].north_m];
    let cross = |p1: [f64; 2], v1: f64, p2: [f64; 2], v2: f64| {
        let t = if (v2 - v1).abs() < 1e-300 { 0.5 } else { ((level - v1) / (v2 - v1)).clamp(0.0, 1.0) };
        [p1[0] + t * (p2[0] - p1[0]), p1[1] + t * (p2[1] - p1[1])]
    };
    for i in 0..na - 1 {
        for j in 0..nc - 1 {
            // a, b, c, d go round the cell.
            let (pa, pb, pc, pd) = (pos(i, j), pos(i, j + 1), pos(i + 1, j + 1), pos(i + 1, j));
            let (va, vb, vc, vd) = (values[at(i, j)], values[at(i, j + 1)], values[at(i + 1, j + 1)], values[at(i + 1, j)]);
            let idx = (va >= level) as u8 | ((vb >= level) as u8) << 1 | ((vc >= level) as u8) << 2 | ((vd >= level) as u8) << 3;
            let ab = || cross(pa, va, pb, vb);
            let bc = || cross(pb, vb, pc, vc);
            let cd = || cross(pc, vc, pd, vd);
            let da = || cross(pd, vd, pa, va);
            match idx {
                1 | 14 => out.push((da(), ab())),
                2 | 13 => out.push((ab(), bc())),
                3 | 12 => out.push((da(), bc())),
                4 | 11 => out.push((bc(), cd())),
                6 | 9 => out.push((ab(), cd())),
                7 | 8 => out.push((cd(), da())),
                5 | 10 => {
                    let centre_high = 0.25 * (va + vb + vc + vd) >= level;
                    // 5: a and c high; 10: b and d high.
                    let join_a_c_corners_cut = (idx == 5) != centre_high;
                    if join_a_c_corners_cut {
                        out.push((da(), ab()));
                        out.push((bc(), cd()));
                    } else {
                        out.push((ab(), bc()));
                        out.push((cd(), da()));
                    }
                }
                _ => {}
            }
        }
    }
    out
}

fn mm(p: [f64; 2], ox: f64, oy: f64) -> Pt2 {
    Pt2::new((p[0] - ox) * 1000.0, (p[1] - oy) * 1000.0)
}

fn plan_bounds(result: &CalcResult, lums: &[LuminaireInstance]) -> (f64, f64) {
    let mut min = [f64::INFINITY, f64::INFINITY];
    for a in &result.areas {
        for p in &a.points {
            min[0] = min[0].min(p.east_m);
            min[1] = min[1].min(p.north_m);
        }
    }
    for l in lums {
        min[0] = min[0].min(l.x_m);
        min[1] = min[1].min(l.y_m);
    }
    // One metre of margin.
    (min[0] - 1.0, min[1] - 1.0)
}

fn outline(area: &AreaResult, ox: f64, oy: f64, prims: &mut Vec<Primitive>) {
    let (na, nc) = (area.n_along, area.n_across);
    if na == 0 || nc == 0 {
        return;
    }
    let p = |i: usize, j: usize| {
        let q = &area.points[i * nc + j];
        mm([q.east_m, q.north_m], ox, oy)
    };
    let ring: Vec<(usize, usize)> = (0..nc).map(|j| (0, j)).chain((1..na).map(|i| (i, nc - 1))).chain((0..nc - 1).rev().map(|j| (na - 1, j))).chain((1..na - 1).rev().map(|i| (i, 0))).collect();
    for k in 0..ring.len() {
        let (a, b) = (ring[k], ring[(k + 1) % ring.len()]);
        if a != b {
            prims.push(Primitive::Line { from: p(a.0, a.1), to: p(b.0, b.1), width: 40.0, colour: Some(Colour::Grey) });
        }
    }
}

fn luminaire_marks(lums: &[LuminaireInstance], ox: f64, oy: f64, prims: &mut Vec<Primitive>) {
    for l in lums {
        prims.push(Primitive::Arc { centre: mm([l.x_m, l.y_m], ox, oy), radius: 150.0, start_deg: 0.0, end_deg: 360.0, colour: Some(Colour::Black) });
    }
}

/// Iso-illuminance contours of the horizontal illuminance of one area, in plan, with the
/// luminaire positions and the area outline. `levels` are in lux; each level gets a colour from a
/// small palette and a label near the middle of its first line. Returns `None` for an unknown area.
///
/// The lines are drawn on the lattice of the area's calculation points, so they stop half a cell
/// inside the area's boundary.
pub fn contour_drawing(result: &CalcResult, area_name: &str, levels: &[f64], luminaires: &[LuminaireInstance]) -> Option<LabelledDrawing> {
    let area = result.area(area_name)?;
    let (ox, oy) = plan_bounds(result, luminaires);
    let mut prims = Vec::new();
    let mut labels = Vec::new();
    outline(area, ox, oy, &mut prims);
    luminaire_marks(luminaires, ox, oy, &mut prims);
    for (k, level) in levels.iter().enumerate() {
        let colour = PALETTE[k % PALETTE.len()];
        let segs = contour_segments(area, &area.e_h_lx, *level);
        for (a, b) in &segs {
            prims.push(Primitive::Line { from: mm(*a, ox, oy), to: mm(*b, ox, oy), width: 60.0, colour: Some(colour) });
        }
        if let Some((a, b)) = segs.get(segs.len() / 2) {
            let m = mm([0.5 * (a[0] + b[0]), 0.5 * (a[1] + b[1])], ox, oy);
            labels.push(DrawingLabel { text: format!("{} lx", fixed(*level, 0, 2)), x: m.x, y: m.y, height: 250.0, rotation_deg: 0.0 });
        }
    }
    let extents = extents_of(&prims, &labels);
    Some(LabelledDrawing { drawing: Drawing { primitives: prims, extents }, labels, origin_x_m: ox, origin_y_m: oy })
}

/// The grid of values of one area in plan: a small cross at each calculation point and its
/// horizontal illuminance as a label, with the luminaire positions.
pub fn grid_values_drawing(result: &CalcResult, area_name: &str, luminaires: &[LuminaireInstance]) -> Option<LabelledDrawing> {
    let area = result.area(area_name)?;
    let (ox, oy) = plan_bounds(result, luminaires);
    let mut prims = Vec::new();
    let mut labels = Vec::new();
    outline(area, ox, oy, &mut prims);
    luminaire_marks(luminaires, ox, oy, &mut prims);
    let spacing = if area.points.len() > 1 {
        let d1 = area.points[0].station_m - area.points[area.points.len() - 1].station_m;
        let d2 = area.points[0].offset_m - area.points[area.points.len() - 1].offset_m;
        let along = if area.n_along > 1 { (d1 / (area.n_along as f64 - 1.0)).abs() } else { f64::INFINITY };
        let across = if area.n_across > 1 { (d2 / (area.n_across as f64 - 1.0)).abs() } else { f64::INFINITY };
        along.min(across)
    } else {
        1.0
    };
    let height = if spacing.is_finite() { (spacing * 1000.0 * 0.25).clamp(80.0, 600.0) } else { 250.0 };
    for (p, e) in area.points.iter().zip(&area.e_h_lx) {
        let c = mm([p.east_m, p.north_m], ox, oy);
        let r = height * 0.3;
        prims.push(Primitive::Line { from: Pt2::new(c.x - r, c.y), to: Pt2::new(c.x + r, c.y), width: 15.0, colour: Some(Colour::Grey) });
        prims.push(Primitive::Line { from: Pt2::new(c.x, c.y - r), to: Pt2::new(c.x, c.y + r), width: 15.0, colour: Some(Colour::Grey) });
        labels.push(DrawingLabel { text: fixed(*e, 1, 1), x: c.x + r, y: c.y + r, height, rotation_deg: 0.0 });
    }
    let extents = extents_of(&prims, &labels);
    Some(LabelledDrawing { drawing: Drawing { primitives: prims, extents }, labels, origin_x_m: ox, origin_y_m: oy })
}

fn nice_ceil(v: f64) -> f64 {
    if v.is_nan() || v <= 0.0 {
        return 1.0;
    }
    let mag = 10f64.powf(v.log10().floor());
    [1.0, 2.0, 2.5, 5.0, 10.0].iter().map(|m| m * mag).find(|s| *s >= v * (1.0 - 1e-12)).unwrap_or(10.0 * mag)
}

/// Polar diagram of the intensity in plane pairs (C, C + 180): for each pair, the half on the
/// right is plane `C`, the half on the left plane `C + 180`, nadir at the bottom. The default
/// pairs for a road luminaire are C0-C180 and C90-C270 (`[(0.0, 180.0), (90.0, 270.0)]`). Four
/// rings mark a quarter to the whole of the outer value, the smallest of 1, 2, 2.5, 5 times a
/// power of ten that holds the peak. Radius 1000 mm stands for the outer value.
pub fn polar_diagram(phot: &Photometry, planes: &[(f64, f64)]) -> LabelledDrawing {
    const R: f64 = 1000.0;
    let peak = planes
        .iter()
        .flat_map(|(a, b)| (0..=180).flat_map(move |g| [(*a, g as f64), (*b, g as f64)]))
        .map(|(c, g)| phot.intensity(c, g))
        .fold(0.0, f64::max);
    let outer = nice_ceil(peak);
    let mut prims = Vec::new();
    let mut labels = Vec::new();
    for k in 1..=4 {
        let r = R * k as f64 / 4.0;
        prims.push(Primitive::Arc { centre: Pt2::new(0.0, 0.0), radius: r, start_deg: 0.0, end_deg: 360.0, colour: Some(Colour::Grey) });
        labels.push(DrawingLabel { text: format!("{} cd", fixed(outer * k as f64 / 4.0, 0, 1)), x: 20.0, y: -r + 20.0, height: 50.0, rotation_deg: 0.0 });
    }
    prims.push(Primitive::Line { from: Pt2::new(0.0, -R), to: Pt2::new(0.0, R), width: 6.0, colour: Some(Colour::Grey) });
    prims.push(Primitive::Line { from: Pt2::new(-R, 0.0), to: Pt2::new(R, 0.0), width: 6.0, colour: Some(Colour::Grey) });
    for (k, (c_right, c_left)) in planes.iter().enumerate() {
        let colour = PALETTE[(k * 3) % PALETTE.len()];
        for (c, side) in [(*c_right, 1.0), (*c_left, -1.0)] {
            let pts: Vec<Pt2> = (0..=180)
                .map(|g| {
                    let g = g as f64;
                    let r = R * phot.intensity(c, g) / outer;
                    Pt2::new(side * r * g.to_radians().sin(), -r * g.to_radians().cos())
                })
                .collect();
            for w in pts.windows(2) {
                prims.push(Primitive::Line { from: w[0], to: w[1], width: 12.0, colour: Some(colour) });
            }
        }
        labels.push(DrawingLabel { text: format!("C{}-C{}", fixed(*c_right, 0, 1), fixed(*c_left, 0, 1)), x: -R, y: R + 120.0 + 90.0 * k as f64, height: 70.0, rotation_deg: 0.0 });
    }
    let extents = extents_of(&prims, &labels);
    LabelledDrawing { drawing: Drawing { primitives: prims, extents }, labels, origin_x_m: 0.0, origin_y_m: 0.0 }
}

fn csv(s: &str) -> String {
    if s.contains([',', '"', '\n']) {
        format!("\"{}\"", s.replace('"', "\"\""))
    } else {
        s.to_string()
    }
}

/// The grid of values as CSV, one row per calculation point: area, kind, indices, station,
/// offset, plan position, maintained horizontal illuminance, then for the vertical settings
/// `Ev` and `Esc` of each direction and for an r-table the luminance of each observer. Areas
/// that lack a column (the edge strips) leave it empty.
pub fn grid_to_csv(result: &CalcResult) -> String {
    let template = result.areas.iter().find(|a| !a.vertical.is_empty() || !a.luminance.is_empty()).or(result.areas.first());
    let mut header = "area,kind,i_along,i_across,station_m,offset_m,easting_m,northing_m,e_h_lx".to_string();
    let dirs: Vec<String> = template.map(|a| a.vertical.iter().map(|d| d.label.clone()).collect()).unwrap_or_default();
    let obs: Vec<String> = template.map(|a| a.luminance.iter().map(|l| l.observer.clone()).collect()).unwrap_or_default();
    for d in &dirs {
        header.push_str(&format!(",{},{}", csv(&format!("ev_lx {d}")), csv(&format!("esc_lx {d}"))));
    }
    for o in &obs {
        header.push_str(&format!(",{}", csv(&format!("l_cd_m2 {o}"))));
    }
    let mut out = format!("{header}\n");
    for a in &result.areas {
        for (k, p) in a.points.iter().enumerate() {
            out.push_str(&format!(
                "{},{},{},{},{},{},{},{},{}",
                csv(&a.name),
                a.kind.label(),
                p.i_along,
                p.i_across,
                fixed(p.station_m, 3, 3),
                fixed(p.offset_m, 3, 3),
                fixed(p.east_m, 3, 3),
                fixed(p.north_m, 3, 3),
                fixed(a.e_h_lx[k], 3, 3)
            ));
            for d in &dirs {
                match a.vertical.iter().find(|x| &x.label == d) {
                    Some(x) => out.push_str(&format!(",{},{}", fixed(x.ev_lx[k], 3, 3), fixed(x.esc_lx[k], 3, 3))),
                    None => out.push_str(",,"),
                }
            }
            for o in &obs {
                match a.luminance.iter().find(|x| &x.observer == o) {
                    Some(x) => out.push_str(&format!(",{}", fixed(x.l_cd_m2[k], 4, 4))),
                    None => out.push(','),
                }
            }
            out.push('\n');
        }
    }
    out
}
