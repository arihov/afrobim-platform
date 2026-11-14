//! Drawings and meshes of a corridor: the cross-section sheet, the 3D surfaces and the plan lines.

use afrobim_draw::{Colour, Drawing, Extents, Mesh3, Primitive, Pt2};
use serde::{Deserialize, Serialize};

use crate::section::{Corridor, Section, SlopeKind};

/// What a label on a cross-section says.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LabelKind {
    /// The displayed station.
    Title,
    /// Offset and level of a point of the finished surface.
    Design,
    /// Offset and level of a daylight point.
    Daylight,
    /// Ground level on the centreline.
    Ground,
    /// The cut and fill areas.
    Areas,
}

/// A piece of text belonging to a cross-section drawing. The draw crate has no text primitive,
/// so the text is handed over beside the drawing for the front end to set.
#[derive(Debug, Clone, PartialEq)]
pub struct SectionLabel {
    /// Where the text refers to, in drawing units. A `Title` or `Areas` label is anchored at the
    /// top left or top centre of the drawing.
    pub at: Pt2,
    pub text: String,
    pub kind: LabelKind,
}

/// A cross-section as drawing primitives with its labels.
///
/// The drawing is in millimetres at full size: 1 m of offset is 1000 units along X, and 1 m of
/// level is 1000 × `vertical_exaggeration` units along Y. Its lower-left extent is on the
/// origin. A drawing point (u, v) is
///
/// ```text
///   offset = origin_offset_m + u / 1000
///   level  = origin_level_m  + v / (1000 × vertical_exaggeration)
/// ```
#[derive(Debug, Clone, PartialEq)]
pub struct SectionDrawing {
    pub drawing: Drawing,
    pub labels: Vec<SectionLabel>,
    pub origin_offset_m: f64,
    pub origin_level_m: f64,
    pub vertical_exaggeration: f64,
}

/// Draws a cross-section, looking up-station (left on the left).
///
/// Hairlines (`Primitive::Line` of width 0): the ground in green, the earthworks line (side
/// slopes and formation) in red, the finished surface in black, the centreline in grey from the
/// lowest to the highest point drawn. Labels: the station; offset and level of every finished
/// point and daylight point; the ground level on the centreline; the cut and fill areas, or why
/// there are none. `vertical_exaggeration` stretches levels only; 1.0 draws to natural scale.
pub fn section_drawing(section: &Section, vertical_exaggeration: f64) -> SectionDrawing {
    let k = if vertical_exaggeration.is_finite() && vertical_exaggeration > 0.0 { vertical_exaggeration } else { 1.0 };
    let finished: Vec<(f64, f64)> = section.finished.iter().map(|p| (p.offset_m, p.elevation_m)).collect();
    let all = || finished.iter().chain(&section.earthworks).chain(&section.ground);
    let min_o = all().map(|p| p.0).fold(f64::INFINITY, f64::min);
    let max_o = all().map(|p| p.0).fold(f64::NEG_INFINITY, f64::max);
    let min_z = all().map(|p| p.1).fold(f64::INFINITY, f64::min);
    let max_z = all().map(|p| p.1).fold(f64::NEG_INFINITY, f64::max);
    let pt = |p: (f64, f64)| Pt2::new((p.0 - min_o) * 1000.0, (p.1 - min_z) * 1000.0 * k);
    let mut primitives = Vec::new();
    let mut polyline = |line: &[(f64, f64)], colour: Option<Colour>| {
        primitives.extend(line.windows(2).map(|w| Primitive::Line { from: pt(w[0]), to: pt(w[1]), width: 0.0, colour }));
    };
    polyline(&section.ground, Some(Colour::Green));
    polyline(&section.earthworks, Some(Colour::Red));
    polyline(&finished, None);
    polyline(&[(0.0, min_z), (0.0, max_z)], Some(Colour::Grey));

    let mut labels = vec![SectionLabel { at: pt((min_o, max_z)), text: format!("Station {}", section.label), kind: LabelKind::Title }];
    let point = |o: f64, z: f64| format!("{o:+.3} / {z:.3}");
    for p in &section.finished {
        labels.push(SectionLabel { at: pt((p.offset_m, p.elevation_m)), text: format!("{} {}", p.code, point(p.offset_m, p.elevation_m)), kind: LabelKind::Design });
    }
    for d in [section.left_daylight, section.right_daylight].into_iter().flatten() {
        let what = match d.kind {
            SlopeKind::Cut => "cut daylight",
            SlopeKind::Fill => "fill daylight",
            SlopeKind::AtGround => "at ground",
        };
        labels.push(SectionLabel { at: pt((d.offset_m, d.elevation_m)), text: format!("{what} {}", point(d.offset_m, d.elevation_m)), kind: LabelKind::Daylight });
    }
    if let Some(g) = section.ground_at(0.0) {
        labels.push(SectionLabel { at: pt((0.0, g)), text: format!("ground {g:.3}"), kind: LabelKind::Ground });
    }
    let areas = match (section.cut_area_m2, section.fill_area_m2) {
        (Some(c), Some(f)) => format!("Cut {c:.3} m2   Fill {f:.3} m2"),
        _ => section.notes.join(" "),
    };
    labels.push(SectionLabel { at: pt(((min_o + max_o) / 2.0, max_z)), text: areas, kind: LabelKind::Areas });

    SectionDrawing {
        drawing: Drawing { primitives, extents: Extents { min: Pt2::new(0.0, 0.0), max: pt((max_o, max_z)) } },
        labels,
        origin_offset_m: min_o,
        origin_level_m: min_z,
        vertical_exaggeration: k,
    }
}

/// The lines of a corridor in plan, in metres (X east, Y north), each in station order.
#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
pub struct PlanLines {
    pub centreline: Vec<(f64, f64)>,
    /// The outer end of the template on each side: the edge of the formed road before the side slope.
    pub left_edge: Vec<(f64, f64)>,
    pub right_edge: Vec<(f64, f64)>,
    /// Where the side slopes meet the ground. A line is broken wherever a section has no
    /// daylight on that side, so each is a list of pieces.
    pub left_daylight: Vec<Vec<(f64, f64)>>,
    pub right_daylight: Vec<Vec<(f64, f64)>>,
}

/// The centreline, edge and daylight lines of a corridor in plan: one point per section, joined
/// by straights (so on a curve they are chords between sections).
pub fn plan_lines(corridor: &Corridor) -> PlanLines {
    let mut out = PlanLines::default();
    let (mut left_open, mut right_open) = (false, false);
    for s in &corridor.sections {
        out.centreline.push((s.x, s.y));
        out.left_edge.push(s.plan_point(s.finished[0].offset_m));
        out.right_edge.push(s.plan_point(s.finished[s.finished.len() - 1].offset_m));
        for (point, pieces, open) in
            [(s.left_daylight, &mut out.left_daylight, &mut left_open), (s.right_daylight, &mut out.right_daylight, &mut right_open)]
        {
            match point {
                Some(p) => {
                    if !*open {
                        pieces.push(Vec::new());
                    }
                    pieces.last_mut().unwrap().push(s.plan_point(p.offset_m));
                    *open = true;
                }
                None => *open = false,
            }
        }
    }
    out
}

/// The corridor as surfaces for the 3D site view.
///
/// Vertices are millimetres: X east and Y north from the plan point (`origin_x_m`, `origin_y_m`),
/// Z the elevation itself. Placed with a site position of (`origin_x_m` × 1000,
/// `origin_y_m` × 1000, 0) and no rotation, the meshes are in site coordinates.
///
/// These are open surfaces, not solids: [`Mesh3::volume`] means nothing for them, and a viewer
/// that hides faces turned away from the eye shows nothing from below.
#[derive(Debug, Clone, PartialEq)]
pub struct CorridorMeshes {
    pub origin_x_m: f64,
    pub origin_y_m: f64,
    /// The finished surface between the two outer ends of the template.
    pub top: Mesh3,
    pub cut_slopes: Mesh3,
    pub fill_slopes: Mesh3,
}

/// Display colours of the corridor surfaces. For screens only; they mean nothing.
const TOP_RGB: [u8; 3] = [0x5A, 0x5C, 0x60];
const CUT_RGB: [u8; 3] = [0x9A, 0x6B, 0x3C];
const FILL_RGB: [u8; 3] = [0x5E, 0x8C, 0x4A];

/// Builds the surfaces of a corridor: flat strips between each pair of consecutive sections.
///
/// The top joins like points of the two sections, so a strip is made only where both sections
/// use the same template region. A side slope strip joins outer end and daylight point of both
/// sections and is made only where both have a daylight on that side; it goes to the cut mesh or
/// the fill mesh by the slope at the first of the two sections (the second, where the first is
/// at ground). Triangles are counter-clockwise seen from above.
pub fn corridor_meshes(corridor: &Corridor) -> CorridorMeshes {
    let origin_x_m = corridor.sections.iter().map(|s| s.x).fold(f64::INFINITY, f64::min);
    let origin_y_m = corridor.sections.iter().map(|s| s.y).fold(f64::INFINITY, f64::min);
    let mesh = |name: &str, colour| Mesh3 { name: name.to_string(), vertices: Vec::new(), triangles: Vec::new(), colour };
    let mut top = mesh("Corridor top", TOP_RGB);
    let mut cut = mesh("Cut slope", CUT_RGB);
    let mut fill = mesh("Fill slope", FILL_RGB);
    let vertex = |s: &Section, o: f64, z: f64| {
        let (x, y) = s.plan_point(o);
        [(x - origin_x_m) * 1000.0, (y - origin_y_m) * 1000.0, z * 1000.0]
    };
    // a0, a1 on the back section from left to right; b0, b1 on the forward one.
    let quad = |m: &mut Mesh3, a0: [f64; 3], a1: [f64; 3], b0: [f64; 3], b1: [f64; 3]| {
        let n = m.vertices.len() as u32;
        m.vertices.extend([a0, a1, b1, b0]);
        m.triangles.extend([[n, n + 1, n + 2], [n, n + 2, n + 3]]);
    };
    for w in corridor.sections.windows(2) {
        let (a, b) = (&w[0], &w[1]);
        if b.station_m - a.station_m <= 0.0 {
            continue;
        }
        if a.region == b.region && a.finished.len() == b.finished.len() {
            for i in 0..a.finished.len() - 1 {
                let (p, q, r, s) = (&a.finished[i], &a.finished[i + 1], &b.finished[i], &b.finished[i + 1]);
                quad(
                    &mut top,
                    vertex(a, p.offset_m, p.elevation_m),
                    vertex(a, q.offset_m, q.elevation_m),
                    vertex(b, r.offset_m, r.elevation_m),
                    vertex(b, s.offset_m, s.elevation_m),
                );
            }
        }
        let sides = [(a.left_daylight, b.left_daylight, 0usize, 0usize, true), (a.right_daylight, b.right_daylight, a.finished.len() - 1, b.finished.len() - 1, false)];
        for (da, db, ia, ib, left) in sides {
            let (Some(da), Some(db)) = (da, db) else { continue };
            let kind = if da.kind == SlopeKind::AtGround { db.kind } else { da.kind };
            let target = match kind {
                SlopeKind::Cut => &mut cut,
                SlopeKind::Fill => &mut fill,
                SlopeKind::AtGround => continue,
            };
            let (ha, hb) = (&a.finished[ia], &b.finished[ib]);
            let (hinge_a, hinge_b) = (vertex(a, ha.offset_m, ha.elevation_m), vertex(b, hb.offset_m, hb.elevation_m));
            let (day_a, day_b) = (vertex(a, da.offset_m, da.elevation_m), vertex(b, db.offset_m, db.elevation_m));
            if left {
                quad(target, day_a, hinge_a, day_b, hinge_b);
            } else {
                quad(target, hinge_a, day_a, hinge_b, day_b);
            }
        }
    }
    CorridorMeshes { origin_x_m, origin_y_m, top, cut_slopes: cut, fill_slopes: fill }
}
