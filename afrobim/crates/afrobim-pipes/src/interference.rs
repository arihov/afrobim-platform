//! Interference (clearance) check in 3D: pipe to pipe and pipe to structure, within a network
//! and between two networks.
//!
//! # Method
//!
//! * A pipe is its 3D centreline (a polyline; a curved pipe is cut into chords of at most 5
//!   degrees) swept by a circle of the **outer radius**: half the larger inside dimension plus the
//!   wall thickness. For a box, egg or elliptical pipe this circle contains the true outline, so
//!   the check is conservative for them.
//! * A structure is a vertical cylinder from its sump level (if none is set, the lowest invert of
//!   the pipes connected to it, or the rim if it has none) up to its rim level, of radius the
//!   bounding radius of its inner plan size plus wall thickness (zero if no inner size is set).
//! * The **gap** between two bodies is the minimum distance between their axes (closest points of
//!   two 3D segments, Ericson, *Real-Time Collision Detection*, 2005, section 5.1.9) minus the two
//!   radii. A gap below the input clearance is a finding; a negative gap (the bodies overlap) is an
//!   error, a gap between zero and the clearance a warning.
//! * Pipe pairs that share a structure are not compared, and a pipe is not compared with the
//!   structures it connects to: they meet by design.
//!
//! The clearance is an input; no value is built in. Neighbouring pipe segments of a single curved
//! pipe are not compared with each other.

use crate::finding::{Finding, Severity};
use crate::model::*;

type V3 = [f64; 3];

fn sub(a: V3, b: V3) -> V3 {
    [a[0] - b[0], a[1] - b[1], a[2] - b[2]]
}
fn dot(a: V3, b: V3) -> f64 {
    a[0] * b[0] + a[1] * b[1] + a[2] * b[2]
}

/// Closest points between segments p1-q1 and p2-q2 (Ericson 5.1.9). Returns the distance and the
/// two closest points. Computed relative to `p1` so large coordinates lose nothing.
fn segment_segment(p1: V3, q1: V3, p2: V3, q2: V3) -> (f64, V3, V3) {
    let origin = p1;
    let (p1, q1, p2, q2) = ([0.0; 3], sub(q1, origin), sub(p2, origin), sub(q2, origin));
    let d1 = sub(q1, p1);
    let d2 = sub(q2, p2);
    let r = sub(p1, p2);
    let a = dot(d1, d1);
    let e = dot(d2, d2);
    let f = dot(d2, r);
    let eps = 1e-18;
    let (mut s, mut t);
    if a <= eps && e <= eps {
        s = 0.0;
        t = 0.0;
    } else if a <= eps {
        s = 0.0;
        t = (f / e).clamp(0.0, 1.0);
    } else {
        let c = dot(d1, r);
        if e <= eps {
            t = 0.0;
            s = (-c / a).clamp(0.0, 1.0);
        } else {
            let b = dot(d1, d2);
            let denom = a * e - b * b;
            s = if denom > 1e-14 * a * e { ((b * f - c * e) / denom).clamp(0.0, 1.0) } else { 0.0 };
            t = (b * s + f) / e;
            if t < 0.0 {
                t = 0.0;
                s = (-c / a).clamp(0.0, 1.0);
            } else if t > 1.0 {
                t = 1.0;
                s = ((b - c) / a).clamp(0.0, 1.0);
            }
        }
    }
    let c1 = [p1[0] + d1[0] * s, p1[1] + d1[1] * s, p1[2] + d1[2] * s];
    let c2 = [p2[0] + d2[0] * t, p2[1] + d2[1] * t, p2[2] + d2[2] * t];
    let d = sub(c1, c2);
    let dist = dot(d, d).sqrt();
    (dist, [c1[0] + origin[0], c1[1] + origin[1], c1[2] + origin[2]], [c2[0] + origin[0], c2[1] + origin[1], c2[2] + origin[2]])
}

struct PipeBody {
    id: PipeId,
    name: String,
    start: StructureId,
    end: StructureId,
    line: Vec<V3>,
    radius: f64,
    lo: V3,
    hi: V3,
}

struct StructBody {
    id: StructureId,
    name: String,
    axis: [V3; 2],
    radius: f64,
    connected: Vec<PipeId>,
}

fn pipe_bodies(net: &PipeNetwork) -> Vec<PipeBody> {
    net.pipes()
        .filter_map(|p| {
            let line = net.centreline_3d(p.id).ok()?;
            let radius = net.pipe_outer_radius_m(p.id).ok()?;
            let mut lo = [f64::INFINITY; 3];
            let mut hi = [f64::NEG_INFINITY; 3];
            for v in &line {
                for k in 0..3 {
                    lo[k] = lo[k].min(v[k]);
                    hi[k] = hi[k].max(v[k]);
                }
            }
            Some(PipeBody { id: p.id, name: p.name.clone(), start: p.start, end: p.end, line, radius, lo, hi })
        })
        .collect()
}

fn struct_bodies(net: &PipeNetwork) -> Vec<StructBody> {
    net.structures()
        .map(|s| {
            let mut connected = net.incoming(s.id);
            connected.extend(net.outgoing(s.id));
            let lowest_invert = connected
                .iter()
                .filter_map(|p| net.pipe(*p).ok().map(|p| if p.start == s.id { p.start_invert_m } else { p.end_invert_m }))
                .fold(f64::INFINITY, f64::min);
            let bottom = s.sump_level_m.unwrap_or(if lowest_invert.is_finite() { lowest_invert } else { s.rim_level_m });
            let radius = s.inner.map(|i| i.bounding_radius_m()).unwrap_or(0.0) + s.wall_thickness_m.unwrap_or(0.0);
            StructBody { id: s.id, name: s.name.clone(), axis: [[s.easting_m, s.northing_m, bottom], [s.easting_m, s.northing_m, s.rim_level_m]], radius, connected }
        })
        .collect()
}

fn min_distance(a: &[V3], b: &[V3]) -> (f64, V3, V3) {
    let mut best = (f64::INFINITY, [0.0; 3], [0.0; 3]);
    for i in 1..a.len().max(1) {
        for j in 1..b.len().max(1) {
            let (pa, qa) = (a[i - 1], a[i]);
            let (pb, qb) = (b[j - 1], b[j]);
            let r = segment_segment(pa, qa, pb, qb);
            if r.0 < best.0 {
                best = r;
            }
        }
    }
    best
}

fn finding(gap: f64, clearance: f64, els: Vec<ElementRef>, what: String, at: V3) -> Finding {
    let sev = if gap < 0.0 { Severity::Error } else { Severity::Warning };
    let verb = if gap < 0.0 { format!("overlap by {:.3} m", -gap) } else { format!("are {gap:.3} m apart, less than the clearance {clearance:.3} m") };
    Finding::new("interference", sev, els, format!("{what} {verb} (near E {:.3}, N {:.3}, level {:.3} m)", at[0], at[1], at[2]))
}

impl PipeNetwork {
    /// Clearance check within this network.
    pub fn check_interference(&self, clearance_m: f64) -> Vec<Finding> {
        interference(self, self, clearance_m, true)
    }

    /// Clearance check between this network and another. Every pipe and structure of one is
    /// compared with every pipe and structure of the other (none are skipped as connected).
    /// Element ids in a finding are those of each element in its own network; the message names the networks.
    pub fn check_interference_with(&self, other: &PipeNetwork, clearance_m: f64) -> Vec<Finding> {
        interference(self, other, clearance_m, false)
    }
}

fn interference(a: &PipeNetwork, b: &PipeNetwork, clearance: f64, same: bool) -> Vec<Finding> {
    let mut out = Vec::new();
    if !(clearance >= 0.0) || !clearance.is_finite() {
        out.push(Finding::new("interference", Severity::Error, vec![], "the clearance must be zero or more".to_string()));
        return out;
    }
    let (pa, pb) = (pipe_bodies(a), pipe_bodies(b));
    let (sa, sb) = (struct_bodies(a), struct_bodies(b));
    let tag = |same: bool, n1: &str, n2: &str| if same { String::new() } else { format!(" [{n1} / {n2}]") };
    // pipe to pipe
    for (i, x) in pa.iter().enumerate() {
        for (j, y) in pb.iter().enumerate() {
            if same && j <= i {
                continue;
            }
            if same && (x.start == y.start || x.start == y.end || x.end == y.start || x.end == y.end) {
                continue;
            }
            let m = x.radius + y.radius + clearance;
            if (0..3).any(|k| x.lo[k] - m > y.hi[k] || y.lo[k] - m > x.hi[k]) {
                continue;
            }
            let (d, c1, c2) = min_distance(&x.line, &y.line);
            let gap = d - x.radius - y.radius;
            if gap < clearance {
                let at = [(c1[0] + c2[0]) / 2.0, (c1[1] + c2[1]) / 2.0, (c1[2] + c2[2]) / 2.0];
                out.push(finding(gap, clearance, vec![ElementRef::Pipe(x.id), ElementRef::Pipe(y.id)], format!("pipes {} and {}{}", x.name, y.name, tag(same, &a.name, &b.name)), at));
            }
        }
    }
    // pipes of one against structures of the other (and within one network)
    let mut pipe_struct = |pipes: &[PipeBody], structs: &[StructBody], pipes_in_a: bool| {
        for p in pipes {
            for s in structs {
                if same && s.connected.contains(&p.id) {
                    continue;
                }
                let m = p.radius + s.radius + clearance;
                let (lo, hi) = ([s.axis[0][0], s.axis[0][1], s.axis[0][2]], s.axis[1]);
                if (0..3).any(|k| p.lo[k] - m > hi[k].max(lo[k]) || lo[k].min(hi[k]) - m > p.hi[k]) {
                    continue;
                }
                let (d, c1, c2) = min_distance(&p.line, &s.axis);
                let gap = d - p.radius - s.radius;
                if gap < clearance {
                    let at = [(c1[0] + c2[0]) / 2.0, (c1[1] + c2[1]) / 2.0, (c1[2] + c2[2]) / 2.0];
                    let t = if same { String::new() } else if pipes_in_a { format!(" [{} / {}]", a.name, b.name) } else { format!(" [{} / {}]", b.name, a.name) };
                    out.push(finding(gap, clearance, vec![ElementRef::Pipe(p.id), ElementRef::Structure(s.id)], format!("pipe {} and structure {}{}", p.name, s.name, t), at));
                }
            }
        }
    };
    pipe_struct(&pa, &sb, true);
    if !same {
        pipe_struct(&pb, &sa, false);
    }
    out
}
