// SPDX-License-Identifier: LicenseRef-AfroBIM-Proprietary
// Copyright (c) 2026 AfroBIM. All rights reserved.
//! Roundabouts: circles, entry and exit kerbs, splitter islands and the figures an engineer checks.
//!
//! **Every dimension is entered; no limit is applied.** No geometric design document held in this
//! repository defines a roundabout geometry, so the terms below are this crate's, defined here.
//!
//! # Definitions (this crate's)
//!
//! With centre `O`, inscribed circle diameter `D`, circulatory carriageway width `W` and apron
//! width `A`: the **inscribed circle** has radius `D/2` (the outer kerb of the circulatory
//! carriageway); the carriageway lies between it and the circle of radius `D/2 - W`; the apron
//! between that and the circle of radius `D/2 - W - A`, the edge of the raised central island.
//! The **circulatory centreline** has radius `D/2 - W/2`. Circles are alignments of four 90 degree
//! arcs turning the way traffic circulates (clockwise: right-hand curves).
//!
//! # An arm
//!
//! The arm's alignment meets the inscribed circle at the **give-way point** `G`. In the frame at `G`,
//! `a` runs along the arm towards the centre, `w` across it towards the **entry side** (the left of
//! the way in for clockwise circulation, the right for anticlockwise: a vehicle enters on the side
//! towards which the circulating flow leaves `G`). The splitter's half-width at distance `d`
//! upstream is `h(d)`, interpolated linearly from the entered table (zero beyond its end).
//!
//! * **Entry kerb.** The flare starts at `A = (a = -l, w = h(l) + v)` where `l` is the flare length and
//!   `v` the approach half-width, and the flare line passes through `P = (a = 0, w = h(0) + e)`, `e`
//!   the entry width, so the entry width is the lateral distance from the splitter kerb to the
//!   entry kerb on the give-way line through `G`. An arc of the entered **entry radius** `r` is tangent
//!   to that line and externally tangent to the inscribed circle. Its centre `c = P + mu t + r n`
//!   (`t` the unit direction of the line, `n` its unit normal on the entry side) satisfies
//!   `|c - O| = D/2 + r`, a quadratic in `mu`; the smaller root is used. If `mu < 0` the arc begins
//!   upstream of the give-way line and the entry width is not what was entered (reported).
//! * **Exit kerb.** The mirror construction on the other side with the exit width, the departure
//!   half-width and the exit radius, travelled from the circle outwards.
//! * **Splitter island.** Sides through the entered `(distance, half-width)` points, mirrored about
//!   the arm axis, closed at the give-way end by a straight cap; the last point has half-width zero
//!   (the nose), rounded by an arc of the **nose radius** `r_n` tangent to both sides: with `alpha` the
//!   angle between a side and the axis at the nose, the tangent length is `r_n / tan(alpha)` and the arc
//!   turns `pi - 2 alpha`.
//! * **Entry angle** (this crate's definition): the angle between the arm axis at `G`, towards the
//!   centre, and the direction of circulating flow at `G`. A radial arm gives 90 degrees, a tangential
//!   one 0.
//! * **Entry path deflection radius** (this crate's construction, only when the three offsets are
//!   entered): the circumradius of three points of a path through the entry: `P1` at the flare start,
//!   `o1` in from the entry kerb; `P2` on the give-way line, `o2` out from the splitter kerb; `P3` on the
//!   ray from `O` that bisects the angle to the next arm in the direction of circulation, at radius
//!   `D/2 - W - A + o3` (`o3` from the central island). The circumradius is `abc / (4 area)`;
//!   collinear points give no radius. This is **not** a published method's fastest path.

use afrobim_alignment::{Alignment, AlignmentBuilder, AlignmentElement, Turn};
use serde::{Deserialize, Serialize};
use std::f64::consts::{PI, TAU};

use crate::crossing::find_crossings;
use crate::error::{Finding, JunctionError, Severity};
use crate::geom::{add, cross, dir, dist, dot, heading_of, polyline_between, scale, sub, Pt};
use crate::intersection::{fnv1a_hex, Dependency, DependencyKind, DependencyRecord};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Circulation {
    Clockwise,
    Anticlockwise,
}

/// A point of a splitter's side: distance upstream of the give-way line and half-width there, m.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct SplitterPoint {
    pub distance_m: f64,
    pub half_width_m: f64,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SplitterInput {
    /// From the give-way end to the nose, distances increasing; the last half-width is zero.
    pub points: Vec<SplitterPoint>,
    pub nose_radius_m: f64,
}

/// Offsets of the entry path construction, m.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct PathOffsets {
    pub from_entry_kerb_m: f64,
    pub from_splitter_m: f64,
    pub from_central_island_m: f64,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ArmInput {
    pub name: String,
    /// Name of the arm's alignment; it must cross the inscribed circle.
    pub alignment: String,
    pub approach_half_width_m: f64,
    pub flare_length_m: f64,
    pub entry_width_m: f64,
    pub entry_radius_m: f64,
    pub exit_width_m: f64,
    pub exit_radius_m: f64,
    pub departure_half_width_m: f64,
    pub splitter: SplitterInput,
    #[serde(default)]
    pub path_offsets: Option<PathOffsets>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct RoundaboutInputs {
    pub name: String,
    pub centre: (f64, f64),
    pub inscribed_diameter_m: f64,
    pub circulatory_width_m: f64,
    pub apron_width_m: f64,
    pub circulation: Circulation,
    pub arms: Vec<ArmInput>,
}

/// The figures of an arm, as reported. None is compared with a limit.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ArmMeasures {
    pub entry_angle_rad: f64,
    pub entry_width_m: f64,
    pub exit_width_m: f64,
    pub entry_radius_m: f64,
    pub exit_radius_m: f64,
    pub flare_length_m: f64,
    pub approach_half_width_m: f64,
    /// `(entry width - approach half-width) / flare length`.
    pub flare_ratio: f64,
    /// Where the entry arc starts relative to the give-way line, m (negative: upstream of it).
    pub entry_arc_start_m: f64,
    pub exit_arc_start_m: f64,
    /// Angle from this arm to the next in the direction of circulation, radians.
    pub angle_to_next_arm_rad: f64,
    /// Circumradius of the entry path construction, m (`None`: not asked, or the points are collinear).
    pub entry_path_radius_m: Option<f64>,
    pub entry_path_points: Option<[Pt; 3]>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SplitterResult {
    pub alignment: Alignment,
    /// Closed outline sampled for drawing (chords turning at most 0.05 rad).
    pub outline: Vec<Pt>,
    pub nose: Pt,
    pub area_m2: f64,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ArmResult {
    pub name: String,
    /// Station of the arm alignment at the give-way point, and the point.
    pub give_way_station_m: f64,
    pub give_way_point: Pt,
    /// Azimuth of the give-way point round the centre, radians counter-clockwise from +X.
    pub azimuth_rad: f64,
    pub entry_kerb: Alignment,
    pub exit_kerb: Alignment,
    pub splitter: SplitterResult,
    pub measures: ArmMeasures,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct RoundaboutResult {
    pub name: String,
    pub centre: (f64, f64),
    pub inscribed_circle: Alignment,
    pub carriageway_inner_edge: Alignment,
    pub central_island_edge: Alignment,
    pub circulatory_centreline: Alignment,
    pub arms: Vec<ArmResult>,
    pub findings: Vec<Finding>,
    pub dependencies: DependencyRecord,
}

/// A circle as an alignment of four quarter arcs starting at the east point.
pub fn circle_alignment(name: &str, centre: (f64, f64), radius: f64, circulation: Circulation) -> Result<Alignment, JunctionError> {
    if !(radius.is_finite() && radius > 0.0) {
        return Err(JunctionError::input(format!("{name}: the radius must be greater than zero (got {radius})")));
    }
    let (h, turn) = match circulation {
        Circulation::Anticlockwise => (PI / 2.0, Turn::Left),
        Circulation::Clockwise => (-PI / 2.0, Turn::Right),
    };
    let mut b = AlignmentBuilder::new(name, 0.0, centre.0 + radius, centre.1, h);
    for _ in 0..4 {
        b = b.arc(radius, radius * PI / 2.0, turn);
    }
    b.build().map_err(|e| JunctionError::geometry(e.to_string()))
}

fn pos(v: f64, what: &str) -> Result<(), JunctionError> {
    if v.is_finite() && v > 0.0 {
        Ok(())
    } else {
        Err(JunctionError::input(format!("{what} must be greater than zero (got {v})")))
    }
}

fn half_width(points: &[SplitterPoint], d: f64) -> f64 {
    let (first, last) = (points[0], points[points.len() - 1]);
    if d <= first.distance_m {
        return first.half_width_m;
    }
    if d >= last.distance_m {
        return last.half_width_m;
    }
    for w in points.windows(2) {
        if d <= w[1].distance_m {
            let t = (d - w[0].distance_m) / (w[1].distance_m - w[0].distance_m);
            return w[0].half_width_m + t * (w[1].half_width_m - w[0].half_width_m);
        }
    }
    last.half_width_m
}

/// Centre of the circle of radius `r` tangent to the line through `p` with unit direction `t`
/// (on the side of unit normal `n`) and externally tangent to the circle (`o`, `ro`). Returns the
/// parameter `mu` of the tangent point on the line, `p + mu t`, and the centre.
fn fillet_line_circle(p: Pt, t: Pt, n: Pt, r: f64, o: Pt, ro: f64) -> Option<(f64, Pt)> {
    let q = sub(add(p, scale(n, r)), o);
    let rho = ro + r;
    let b = dot(q, t);
    let disc = b * b - dot(q, q) + rho * rho;
    if disc < 0.0 {
        return None;
    }
    let mu = -b - disc.sqrt();
    Some((mu, add(add(p, scale(n, r)), scale(t, mu))))
}

/// A line then an arc (entry) or an arc then a line (exit), as an alignment.
fn kerb_alignment(name: &str, line: (Pt, Pt), arc_start: Pt, arc_end: Pt, centre: Pt, r: f64, line_first: bool) -> Result<Alignment, JunctionError> {
    let (la, lb) = line;
    let lh = heading_of(sub(lb, la));
    let (v1, v2) = (sub(arc_start, centre), sub(arc_end, centre));
    let phi = cross(v1, v2).abs().atan2(dot(v1, v2));
    // Heading at the start of the arc.
    let ccw = cross(v1, v2) > 0.0;
    let h_arc = heading_of(v1) + if ccw { PI / 2.0 } else { -PI / 2.0 };
    let arc = AlignmentElement::arc(arc_start.0, arc_start.1, h_arc, r, r * phi, if ccw { Turn::Left } else { Turn::Right });
    let l = AlignmentElement::line(la.0, la.1, lh, dist(la, lb));
    let elems = if line_first { vec![l, arc] } else { vec![arc, l] };
    Alignment::from_elements(name, 0.0, elems).map_err(|e| JunctionError::geometry(e.to_string()))
}

fn circumradius(a: Pt, b: Pt, c: Pt) -> Option<f64> {
    let area2 = cross(sub(b, a), sub(c, a)).abs();
    if area2 < 1e-12 {
        return None;
    }
    Some(dist(a, b) * dist(b, c) * dist(c, a) / (2.0 * area2))
}

/// Builds a roundabout. See the module documentation.
pub fn build_roundabout(inputs: &RoundaboutInputs, alignments: &[Alignment]) -> Result<RoundaboutResult, JunctionError> {
    pos(inputs.inscribed_diameter_m, "the inscribed circle diameter")?;
    pos(inputs.circulatory_width_m, "the circulatory carriageway width")?;
    if !(inputs.apron_width_m.is_finite() && inputs.apron_width_m >= 0.0) {
        return Err(JunctionError::input("the apron width must be zero or more"));
    }
    if inputs.arms.is_empty() {
        return Err(JunctionError::input("a roundabout needs at least one arm"));
    }
    let ro = inputs.inscribed_diameter_m / 2.0;
    let r_in = ro - inputs.circulatory_width_m;
    let r_island = r_in - inputs.apron_width_m;
    if !(r_island > 0.0) {
        return Err(JunctionError::input(format!("the circulatory width and apron leave a central island of radius {r_island:.3} m: it must be greater than zero")));
    }
    let o = inputs.centre;
    let circ = inputs.circulation;
    let name = &inputs.name;
    let inscribed = circle_alignment(&format!("{name} inscribed circle"), o, ro, circ)?;
    let inner = circle_alignment(&format!("{name} carriageway inner edge"), o, r_in, circ)?;
    let island = circle_alignment(&format!("{name} central island edge"), o, r_island, circ)?;
    let centreline = circle_alignment(&format!("{name} circulatory centreline"), o, ro - inputs.circulatory_width_m / 2.0, circ)?;
    let mut findings = Vec::new();

    // Give-way points and azimuths.
    struct Gw {
        station: f64,
        p: Pt,
        x_in: Pt,
        az: f64,
    }
    let mut gws: Vec<Gw> = Vec::new();
    for arm in &inputs.arms {
        pos(arm.approach_half_width_m, &format!("arm '{}': the approach half-width", arm.name))?;
        pos(arm.flare_length_m, &format!("arm '{}': the flare length", arm.name))?;
        pos(arm.entry_width_m, &format!("arm '{}': the entry width", arm.name))?;
        pos(arm.entry_radius_m, &format!("arm '{}': the entry radius", arm.name))?;
        pos(arm.exit_width_m, &format!("arm '{}': the exit width", arm.name))?;
        pos(arm.exit_radius_m, &format!("arm '{}': the exit radius", arm.name))?;
        pos(arm.departure_half_width_m, &format!("arm '{}': the departure half-width", arm.name))?;
        pos(arm.splitter.nose_radius_m, &format!("arm '{}': the splitter nose radius", arm.name))?;
        let pts = &arm.splitter.points;
        if pts.len() < 2 || pts.windows(2).any(|w| !(w[1].distance_m > w[0].distance_m)) || pts[0].distance_m < 0.0 || pts[pts.len() - 1].half_width_m != 0.0 || pts.iter().any(|p| p.half_width_m < 0.0) {
            return Err(JunctionError::input(format!("arm '{}': the splitter needs two or more points with distances increasing from zero or more and the last half-width zero", arm.name)));
        }
        let al = alignments.iter().find(|a| a.name == arm.alignment).ok_or_else(|| JunctionError::NoAlignment(arm.alignment.clone()))?;
        let xs = find_crossings(al, &inscribed, 0.0);
        let end_near = |a: &Alignment| {
            let (s, e) = (a.point_at(a.start_station_m), a.point_at(a.end_station_m()));
            match (s, e) {
                (Some(s), Some(e)) => {
                    if dist((s.x, s.y), o) < dist((e.x, e.y), o) {
                        (s.x, s.y)
                    } else {
                        (e.x, e.y)
                    }
                }
                _ => o,
            }
        };
        let near = end_near(al);
        let c = xs
            .into_iter()
            .min_by(|a, b| dist((a.x, a.y), near).total_cmp(&dist((b.x, b.y), near)))
            .ok_or_else(|| JunctionError::geometry(format!("arm '{}': alignment '{}' does not cross the inscribed circle", arm.name, al.name)))?;
        let p = (c.x, c.y);
        let h = dir(c.heading_a_rad);
        let x_in = if dot(h, sub(o, p)) > 0.0 { h } else { scale(h, -1.0) };
        gws.push(Gw { station: c.station_a_m, p, x_in, az: (p.1 - o.1).atan2(p.0 - o.0) });
    }

    let mut arms_out = Vec::new();
    for (i, arm) in inputs.arms.iter().enumerate() {
        let g = &gws[i];
        let al = alignments.iter().find(|a| a.name == arm.alignment).expect("found above");
        let left = (-g.x_in.1, g.x_in.0);
        // Entry side: clockwise circulation leaves G towards the left of the way in.
        let ne = if circ == Circulation::Clockwise { left } else { scale(left, -1.0) };
        let nx = scale(ne, -1.0);
        let at = |a: f64, w: f64| add(add(g.p, scale(g.x_in, a)), scale(ne, w));
        let pts = &arm.splitter.points;
        let h0 = half_width(pts, 0.0);
        let l = arm.flare_length_m;

        // Straightness of the arm axis over the constructed length.
        let reach = l.max(pts[pts.len() - 1].distance_m);
        let mut dev = 0.0f64;
        for k in 1..=20 {
            let d = reach * k as f64 / 20.0;
            let s = g.station + if dot(dir(al.point_at(g.station).map(|p| p.heading_rad).unwrap_or(0.0)), g.x_in) > 0.0 { -d } else { d };
            if let Some(p) = al.point_at(s) {
                let q = sub((p.x, p.y), at(-d, 0.0));
                dev = dev.max(cross(g.x_in, q).abs());
            }
        }
        if dev > 1e-3 {
            findings.push(Finding::new(
                "roundabout.arm_not_straight",
                Severity::Warning,
                &arm.name,
                format!("the construction takes the arm axis as the straight line at the give-way point; the alignment is up to {dev:.3} m off it within {reach:.1} m"),
                &[("deviation_m", dev)],
            ));
        }

        // Entry.
        let a_pt = at(-l, half_width(pts, l) + arm.approach_half_width_m);
        let p_pt = at(0.0, h0 + arm.entry_width_m);
        let t = {
            let v = sub(p_pt, a_pt);
            scale(v, 1.0 / dist(a_pt, p_pt))
        };
        let n_e = {
            let n = (-t.1, t.0);
            if dot(n, ne) > 0.0 {
                n
            } else {
                scale(n, -1.0)
            }
        };
        let (mu_e, c_e) = fillet_line_circle(p_pt, t, n_e, arm.entry_radius_m, o, ro)
            .ok_or_else(|| JunctionError::geometry(format!("arm '{}': an entry arc of radius {} m cannot be tangent to the flare line and the inscribed circle: the flare line passes farther from the centre than the inscribed radius, so it never comes near the circle", arm.name, arm.entry_radius_m)))?;
        let te_line = add(p_pt, scale(t, mu_e));
        let tc_e = add(o, scale(sub(c_e, o), ro / dist(c_e, o)));
        let entry_kerb = kerb_alignment(&format!("{} {} entry kerb", name, arm.name), (a_pt, te_line), te_line, tc_e, c_e, arm.entry_radius_m, true)?;
        if mu_e < 0.0 {
            findings.push(Finding::new(
                "roundabout.entry_arc_upstream",
                Severity::Warning,
                &arm.name,
                format!("the entry arc starts {:.3} m upstream of the give-way line, so the kerb is not {:.3} m from the splitter there", -mu_e, arm.entry_width_m),
                &[("entry_arc_start_m", mu_e)],
            ));
        }

        // Exit: mirrored; the flare line is travelled outwards, from the arc.
        let a_x = at(-l, -(half_width(pts, l) + arm.departure_half_width_m));
        let p_x = at(0.0, -(h0 + arm.exit_width_m));
        let t_x = {
            let v = sub(p_x, a_x);
            scale(v, 1.0 / dist(a_x, p_x))
        };
        let n_x = {
            let n = (-t_x.1, t_x.0);
            if dot(n, nx) > 0.0 {
                n
            } else {
                scale(n, -1.0)
            }
        };
        let (mu_x, c_x) = fillet_line_circle(p_x, t_x, n_x, arm.exit_radius_m, o, ro)
            .ok_or_else(|| JunctionError::geometry(format!("arm '{}': an exit arc of radius {} m cannot be tangent to the flare line and the inscribed circle: the flare line passes farther from the centre than the inscribed radius, so it never comes near the circle", arm.name, arm.exit_radius_m)))?;
        let tx_line = add(p_x, scale(t_x, mu_x));
        let tc_x = add(o, scale(sub(c_x, o), ro / dist(c_x, o)));
        let exit_kerb = kerb_alignment(&format!("{} {} exit kerb", name, arm.name), (tx_line, a_x), tc_x, tx_line, c_x, arm.exit_radius_m, false)?;
        if mu_x < 0.0 {
            findings.push(Finding::new(
                "roundabout.exit_arc_upstream",
                Severity::Warning,
                &arm.name,
                format!("the exit arc ends {:.3} m upstream of the give-way line, so the kerb is not {:.3} m from the splitter there", -mu_x, arm.exit_width_m),
                &[("exit_arc_start_m", mu_x)],
            ));
        }

        // Splitter.
        let splitter = splitter_outline(&format!("{} {} splitter", name, arm.name), arm, &at, &mut findings)?;

        // Entry angle.
        let radial = scale(sub(g.p, o), 1.0 / ro);
        let circ_t = if circ == Circulation::Clockwise { (radial.1, -radial.0) } else { (-radial.1, radial.0) };
        let entry_angle = dot(g.x_in, circ_t).clamp(-1.0, 1.0).acos();

        // Angle to the next arm in the direction of circulation.
        let sign = if circ == Circulation::Clockwise { -1.0 } else { 1.0 };
        let step = |j: usize| (sign * (gws[j].az - g.az)).rem_euclid(TAU);
        let next = (0..gws.len()).filter(|&j| j != i).map(|j| step(j)).fold(TAU, f64::min);

        // Entry path.
        let (mut path_r, mut path_pts) = (None, None);
        if let Some(po) = arm.path_offsets {
            for (w, v) in [("entry kerb offset", po.from_entry_kerb_m), ("splitter offset", po.from_splitter_m), ("central island offset", po.from_central_island_m)] {
                if !(v.is_finite() && v >= 0.0) {
                    return Err(JunctionError::input(format!("arm '{}': the {w} must be zero or more", arm.name)));
                }
            }
            let p1 = at(-l, half_width(pts, l) + arm.approach_half_width_m - po.from_entry_kerb_m);
            let p2 = at(0.0, h0 + po.from_splitter_m);
            let az3 = g.az + sign * next / 2.0;
            let p3 = add(o, scale(dir(az3), r_island + po.from_central_island_m));
            path_r = circumradius(p1, p2, p3);
            path_pts = Some([p1, p2, p3]);
        }
        let flare_ratio = (arm.entry_width_m - arm.approach_half_width_m) / l;
        arms_out.push(ArmResult {
            name: arm.name.clone(),
            give_way_station_m: g.station,
            give_way_point: g.p,
            azimuth_rad: g.az,
            entry_kerb,
            exit_kerb,
            splitter,
            measures: ArmMeasures {
                entry_angle_rad: entry_angle,
                entry_width_m: arm.entry_width_m,
                exit_width_m: arm.exit_width_m,
                entry_radius_m: arm.entry_radius_m,
                exit_radius_m: arm.exit_radius_m,
                flare_length_m: l,
                approach_half_width_m: arm.approach_half_width_m,
                flare_ratio,
                entry_arc_start_m: mu_e,
                exit_arc_start_m: mu_x,
                angle_to_next_arm_rad: next,
                entry_path_radius_m: path_r,
                entry_path_points: path_pts,
            },
        });
    }
    let mut items = Vec::new();
    for arm in &inputs.arms {
        if let Some(a) = alignments.iter().find(|a| a.name == arm.alignment) {
            items.push(Dependency { kind: DependencyKind::Alignment, name: a.name.clone(), fingerprint: fnv1a_hex(&(&a.start_station_m, &a.elements, &a.station_equations)) });
        }
    }
    let dependencies = DependencyRecord { intersection: inputs.name.clone(), items, inputs_fingerprint: fnv1a_hex(inputs) };
    Ok(RoundaboutResult { name: inputs.name.clone(), centre: o, inscribed_circle: inscribed, carriageway_inner_edge: inner, central_island_edge: island, circulatory_centreline: centreline, arms: arms_out, findings, dependencies })
}

fn splitter_outline(name: &str, arm: &ArmInput, at: &dyn Fn(f64, f64) -> Pt, findings: &mut Vec<Finding>) -> Result<SplitterResult, JunctionError> {
    let pts = &arm.splitter.points;
    let n = pts.len();
    let side = |sgn: f64| -> Vec<Pt> { pts.iter().map(|p| at(-p.distance_m, sgn * p.half_width_m)).collect() };
    let (es, xs) = (side(1.0), side(-1.0));
    let nose = es[n - 1];
    // Nose fillet.
    let seg = dist(es[n - 2], nose);
    let alpha = (pts[n - 2].half_width_m).atan2(pts[n - 1].distance_m - pts[n - 2].distance_m);
    let r = arm.splitter.nose_radius_m;
    let tl = r / alpha.tan();
    let mut elems: Vec<AlignmentElement> = Vec::new();
    let line = |a: Pt, b: Pt| AlignmentElement::line(a.0, a.1, heading_of(sub(b, a)), dist(a, b));
    for w in es[..n - 1].windows(2) {
        elems.push(line(w[0], w[1]));
    }
    let rounded = alpha > 1e-9 && tl < seg;
    if rounded {
        let u = scale(sub(nose, es[n - 2]), 1.0 / seg);
        let ta = sub(nose, scale(u, tl));
        elems.push(line(es[n - 2], ta));
        let h1 = heading_of(u);
        let u2 = scale(sub(xs[n - 2], nose), 1.0 / seg);
        let turn_signed = afrobim_alignment::wrap_pi(heading_of(u2) - h1);
        let t = if turn_signed > 0.0 { Turn::Left } else { Turn::Right };
        elems.push(AlignmentElement::arc(ta.0, ta.1, h1, r, r * turn_signed.abs(), t));
        let tb = add(nose, scale(u2, tl));
        elems.push(line(tb, xs[n - 2]));
    } else {
        findings.push(Finding::new(
            "roundabout.nose_radius",
            Severity::Warning,
            &arm.name,
            format!("the nose radius {r:.3} m needs a tangent length of {tl:.3} m but the last side is {seg:.3} m long: the nose is left sharp"),
            &[("tangent_length_m", tl), ("last_side_m", seg)],
        ));
        elems.push(line(es[n - 2], nose));
        elems.push(line(nose, xs[n - 2]));
    }
    for k in (0..n - 2).rev() {
        elems.push(line(xs[k + 1], xs[k]));
    }
    elems.push(line(xs[0], es[0]));
    let alignment = Alignment::from_elements(name, 0.0, elems).map_err(|e| JunctionError::geometry(e.to_string()))?;
    // Arcs sampled at 0.05 rad: the chord is r * 0.05.
    let outline = polyline_between(&alignment, 0.0, alignment.length(), (r * 0.05).max(1e-3));
    let first = outline[0];
    let mut area = 0.0;
    for w in outline.windows(2) {
        area += cross(sub(w[0], first), sub(w[1], first)) / 2.0;
    }
    Ok(SplitterResult { alignment, outline, nose, area_m2: area.abs() })
}
