// SPDX-License-Identifier: LicenseRef-AfroBIM-Proprietary
// Copyright (c) 2026 AfroBIM. All rights reserved.
//! The arrowhead that caps a route arm drawn as a stroked line (stack, direction, lane and
//! diversion arrows). Port of the pack-selected route-arm head of C# `ArrowGeometry`
//! (`RouteArmStroke`, `RouteHeadLength`, `PlaceRouteHead`, `PlaceTriangleHead`, `PlaceHead`,
//! `FittingStroke`, `HeadBackLength`, `HeadAcross`).
//!
//! The C# engine picks the head by the pack's arrow style. A pack whose arrows are the UK TSM
//! Chapter 7 chevron style (UK-TSM-2019) draws the compact triangle head at the full shaft
//! width. That head is a provisional App.1 §2.2 proportion, not a UK-cited value; it is ported
//! as the C# engine draws it. Every other pack (UG-TSM-2004) draws the Uganda App.1 §2.6
//! Figure 5 barbed head with the stroke fitted to the run.

use afrobim_draw::Pt2;

use crate::pack::ArrowRules;
use crate::SignError;

/// Length of the compact triangle head per unit shaft width ([`place_triangle_head`]).
pub(crate) const TRIANGLE_HEAD_LENGTH_RATIO: f64 = 2.0;

// UG-TSM-2004 Figure 5 head proportions (packs/UG-TSM-2004/arrows.xml) expressed as multiples
// of the shaft width, so a compact route-arm head reproduces the standard silhouette at
// whatever local arm stroke it caps. At shaft width 3: HeadFlare = 3 (1×), HeadLength = 12
// (4×), flare 70°, inner fillet r2.0 (⅔×), tip truncation r0.2 (0.2/3×). These are constants of
// the C# engine, not read from the loaded pack.
const HEAD_LENGTH_RATIO: f64 = 4.0;
const HEAD_FLARE_RATIO: f64 = 1.0;
const HEAD_FLARE_ANGLE: f64 = 70.0;
const HEAD_INNER_RAD_RATIO: f64 = 2.0 / 3.0;
const HEAD_TIP_RAD_RATIO: f64 = 0.2 / 3.0;

fn deg(degrees: f64) -> f64 {
    degrees * std::f64::consts::PI / 180.0
}

/// True when route-arm heads for this pack use the Figure 5 barbed head: every pack except one
/// declaring the chevron arrow style.
pub(crate) fn uses_figure5_head(rules: &ArrowRules) -> bool {
    !rules.is_chevron()
}

/// Widest across-dimension of a Figure 5 route-arm head scaled to the given shaft (arm) width
/// (3 × stroke).
#[cfg_attr(not(test), allow(dead_code))]
pub(crate) fn head_across(shaft_width: f64) -> f64 {
    2.0 * (0.5 + HEAD_FLARE_RATIO) * shaft_width
}

/// Depth of a Figure 5 route-arm head from its tip back to where it meets the arm, scaled to
/// the given shaft (arm) width. Callers reserve this much arm length so the shaft stops exactly
/// where the head's back begins (≈4.36 × stroke for the Figure 5 ratios).
pub(crate) fn head_back_length(shaft_width: f64) -> f64 {
    shaft_width * (HEAD_LENGTH_RATIO + HEAD_FLARE_RATIO / deg(HEAD_FLARE_ANGLE).tan())
}

/// Shaft (arm) width for a route arrow that only has `available_along_arm` sw along its
/// pointing axis. A full-size Figure 5 arrow is about half head and half shaft, so the head is
/// kept near half the span and the whole arrow (arm width, head across and depth) scales down
/// together to stay within a short per-row cell, returning to `standard_shaft_width` unchanged
/// once the arm is long enough for a full head.
pub(crate) fn fitting_stroke(standard_shaft_width: f64, available_along_arm: f64) -> f64 {
    let fitting = 0.5 * available_along_arm / head_back_length(1.0);
    standard_shaft_width.min(fitting)
}

/// Route-arm shaft width: fitted to the run for a Figure 5 pack, otherwise the standard width.
pub(crate) fn route_arm_stroke(
    rules: &ArrowRules,
    standard_shaft_width: f64,
    available_along_arm: f64,
) -> Result<f64, SignError> {
    Ok(if uses_figure5_head(rules) {
        fitting_stroke(standard_shaft_width, available_along_arm)
    } else {
        standard_shaft_width
    })
}

/// Arm length the route-arm head occupies behind its tip for this pack.
pub(crate) fn route_head_length(rules: &ArrowRules, shaft_width: f64) -> Result<f64, SignError> {
    Ok(if uses_figure5_head(rules) {
        head_back_length(shaft_width)
    } else {
        TRIANGLE_HEAD_LENGTH_RATIO * shaft_width
    })
}

/// The outline of the route-arm head this pack draws: [`place_head`] or
/// [`place_triangle_head`].
pub(crate) fn place_route_head(
    rules: &ArrowRules,
    tip: Pt2,
    rotation_deg: f64,
    shaft_width: f64,
) -> Result<Vec<Pt2>, SignError> {
    Ok(if uses_figure5_head(rules) {
        place_head(tip, rotation_deg, shaft_width)
    } else {
        place_triangle_head(tip, rotation_deg, shaft_width)
    })
}

/// A standard guidance arrowhead (App.1 §2.6, Figure 5) capping a route / junction arm drawn as
/// a stroked line (App.1 §2.2, Figures 2–4). It reproduces the Figure 5 head proportions (head
/// length 4 × shaft, half-base 1.5 × shaft so 3 × shaft across, 70° back flare, ⅔ × shaft inner
/// fillet, 0.2/3 × shaft truncated tip), so the glyph reads as the slender barbed standard
/// arrow rather than a blunt triangle.
///
/// The tip sits at `tip` and the arrow points along `rotation_deg`, counter-clockwise from +Y
/// (0° = +Y "ahead", −90° = +X "right", +90° = −X "left"). The head's flat back sits on the arm
/// axis and occupies [`head_back_length`] of arm length behind the tip; callers reserve that
/// length on their shaft.
pub(crate) fn place_head(tip: Pt2, rotation_deg: f64, shaft_width: f64) -> Vec<Pt2> {
    let s = shaft_width;
    let a = 0.5 * s; // half-width where the head meets the arm
    let b = (0.5 + HEAD_FLARE_RATIO) * s; // widest half-width at the head base (1.5·s)
    let d_y = HEAD_FLARE_RATIO * s / deg(HEAD_FLARE_ANGLE).tan(); // axial thickness of the back flare
    let y_base = -HEAD_LENGTH_RATIO * s; // head base (widest), behind the tip
    let y_back = y_base - d_y; // arm junction (farthest back)

    let edge_len = ((HEAD_FLARE_RATIO * s).powi(2) + d_y * d_y).sqrt();
    let rf = (HEAD_INNER_RAD_RATIO * s).min(0.4 * a.min(edge_len));
    let rt = (HEAD_TIP_RAD_RATIO * s).min(0.4 * HEAD_LENGTH_RATIO * s);

    let mut pts: Vec<(f64, f64)> = Vec::new();

    // Left side: filleted concave back corner → flared base → truncated tip.
    add_fillet(&mut pts, (-a, y_back), (1.0, 0.0), normalise(a - b, d_y), rf);
    pts.push((-b, y_base));
    let up_left = normalise(b, -y_base); // base-left → tip
    let up_right = normalise(-b, -y_base); // base-right → tip
    pts.push((-up_left.0 * rt, -up_left.1 * rt)); // tip-left (truncated apex)
    pts.push((-up_right.0 * rt, -up_right.1 * rt)); // tip-right (truncated apex)

    // Right side: base → filleted concave back corner. The implicit closing edge between the
    // last and first vertices is the flat arm-junction span the shaft tucks into.
    pts.push((b, y_base));
    add_fillet(&mut pts, (a, y_back), normalise(b - a, d_y), (-1.0, 0.0), rf);

    let rads = deg(rotation_deg);
    let (cos, sin) = (rads.cos(), rads.sin());
    pts.into_iter()
        .map(|(x, y)| Pt2::new(tip.x + x * cos - y * sin, tip.y + x * sin + y * cos))
        .collect()
}

/// Replaces a sharp corner with a tangent-in / quadratic-bulge / tangent-out sequence: the two
/// tangent points are emitted along the incident edges and the interior of the quadratic is
/// sampled between them, so a concave junction reads as a radius rather than a mitre.
fn add_fillet(
    pts: &mut Vec<(f64, f64)>,
    corner: (f64, f64),
    dir_prev: (f64, f64),
    dir_next: (f64, f64),
    r: f64,
) {
    let (p0x, p0y) = (corner.0 + dir_prev.0 * r, corner.1 + dir_prev.1 * r);
    let (p1x, p1y) = (corner.0 + dir_next.0 * r, corner.1 + dir_next.1 * r);
    pts.push((p0x, p0y));
    const STEPS: usize = 4;
    for i in 1..STEPS {
        let t = i as f64 / STEPS as f64;
        let u = 1.0 - t;
        pts.push((
            u * u * p0x + 2.0 * u * t * corner.0 + t * t * p1x,
            u * u * p0y + 2.0 * u * t * corner.1 + t * t * p1y,
        ));
    }
    pts.push((p1x, p1y));
}

fn normalise(x: f64, y: f64) -> (f64, f64) {
    let len = (x * x + y * y).sqrt();
    if len < 1e-9 {
        (0.0, 0.0)
    } else {
        (x / len, y / len)
    }
}

/// A compact route-arm arrowhead (App.1 §2.2, Figures 2–4). The figures annotate the shaft as
/// `b/3`, so the head base width is `b = 3 × shaft_width` (half-base = 1.5 × shaft_width); the
/// head length is taken as `2 × shaft_width`, a documented proportion of the C# engine.
///
/// The tip sits at `tip` and the arrow points along `rotation_deg`, counter-clockwise from +Y
/// (0° = +Y "ahead", −90° = +X "right", +90° = −X "left"). The base is placed behind the tip
/// along the pointing direction. Vertices: tip, base right, base left.
pub(crate) fn place_triangle_head(tip: Pt2, rotation_deg: f64, shaft_width: f64) -> Vec<Pt2> {
    let half_base = 1.5 * shaft_width;
    let length = 2.0 * shaft_width;

    let rads = rotation_deg * std::f64::consts::PI / 180.0;
    let (cos, sin) = (rads.cos(), rads.sin());
    // Rotate a canonical offset then translate to the tip. Canonical frame points +Y, tip at
    // origin, base corners at (±half_base, −length).
    let rot = |x: f64, y: f64| Pt2::new(tip.x + x * cos - y * sin, tip.y + x * sin + y * cos);

    vec![
        rot(0.0, 0.0),
        rot(half_base, -length),
        rot(-half_base, -length),
    ]
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::pack::tests::uk_pack;

    fn close(a: f64, b: f64) -> bool {
        (a - b).abs() < 1e-9
    }

    #[test]
    fn the_triangle_head_is_three_shafts_across_and_two_long_behind_its_tip() {
        let h = place_triangle_head(Pt2::new(10.0, 20.0), 0.0, 2.0);
        assert_eq!(h.len(), 3);
        assert!(close(h[0].x, 10.0) && close(h[0].y, 20.0));
        assert!(close(h[1].x, 13.0) && close(h[1].y, 16.0));
        assert!(close(h[2].x, 7.0) && close(h[2].y, 16.0));
    }

    #[test]
    fn rotation_is_counter_clockwise_from_ahead() {
        // −90°: pointing right, so the base lies to the left of the tip.
        let right = place_triangle_head(Pt2::new(10.0, 20.0), -90.0, 2.0);
        assert!(close(right[0].x, 10.0) && close(right[0].y, 20.0));
        assert!(close(right[1].x, 6.0) && close(right[1].y, 17.0));
        assert!(close(right[2].x, 6.0) && close(right[2].y, 23.0));
        // +90°: pointing left.
        let left = place_triangle_head(Pt2::new(10.0, 20.0), 90.0, 2.0);
        assert!(close(left[1].x, 14.0) && close(left[1].y, 23.0));
        assert!(close(left[2].x, 14.0) && close(left[2].y, 17.0));
    }

    /// The UK pack's arrows are the chevron style: the full 8/3 sw shaft whatever the run, and
    /// a head two shaft widths long.
    #[test]
    fn a_chevron_pack_keeps_the_standard_stroke_and_the_triangle_head() {
        let pack = uk_pack();
        let r = pack.arrows.as_ref().unwrap();
        assert!(!uses_figure5_head(r));
        assert_eq!(
            route_arm_stroke(r, r.shaft_width, 1.0).unwrap(),
            r.shaft_width
        );
        assert!(close(route_head_length(r, 3.0).unwrap(), 6.0));
        assert_eq!(
            place_route_head(r, Pt2::new(1.0, 2.0), 0.0, 3.0).unwrap(),
            place_triangle_head(Pt2::new(1.0, 2.0), 0.0, 3.0)
        );
    }

    /// Uganda App.1 Figure 5 at the standard 3 sw shaft: 12 sw from tip to base, 9 sw across,
    /// and 3 / tan 70° more to the arm junction.
    #[test]
    fn the_figure_5_head_keeps_the_standard_proportions() {
        let rise = 3.0 / 70f64.to_radians().tan();
        assert!(close(head_back_length(3.0), 12.0 + rise));
        assert!(close(head_across(3.0), 9.0));

        let h = place_head(Pt2::new(10.0, 20.0), 0.0, 3.0);
        // 5 fillet points, base, 2 tip points, base, 5 fillet points.
        assert_eq!(h.len(), 14);
        // The back corners sit on the arm junction, one fillet radius in from the arm edges.
        let rf = 0.4 * 1.5;
        assert!(close(h[0].x, 8.5 + rf) && close(h[0].y, 20.0 - 12.0 - rise));
        assert!(close(h[13].x, 11.5 - rf) && close(h[13].y, 20.0 - 12.0 - rise));
        assert!(close(h[5].x, 5.5) && close(h[5].y, 8.0));
        assert!(close(h[8].x, 14.5) && close(h[8].y, 8.0));
        // The apex is truncated by the 0.2 sw tip radius.
        assert!(h[6].y < 20.0 && h[6].y > 19.8 && close(h[6].y, h[7].y));
        for i in 0..14 {
            assert!(close(h[i].x - 10.0, 10.0 - h[13 - i].x) && close(h[i].y, h[13 - i].y));
        }
        // −90° points right: the base lies to the left of the tip.
        let right = place_head(Pt2::new(10.0, 20.0), -90.0, 3.0);
        assert!(close(right[5].x, -2.0) && close(right[5].y, 24.5));
    }

    /// A short run scales the whole arrow so the head takes half of it; a long run keeps the
    /// standard shaft.
    #[test]
    fn a_figure_5_pack_fits_the_stroke_to_the_run() {
        let pack = uk_pack();
        let mut r = pack.arrows.clone().unwrap();
        r.style = Some("flared".to_string());
        assert!(uses_figure5_head(&r));
        let per_unit = head_back_length(1.0);
        let stroke = route_arm_stroke(&r, 3.0, 8.0).unwrap();
        assert!(close(stroke, 4.0 / per_unit) && stroke < 3.0);
        assert!(close(route_head_length(&r, stroke).unwrap(), 4.0));
        assert_eq!(route_arm_stroke(&r, 3.0, 100.0).unwrap(), 3.0);
        assert_eq!(fitting_stroke(3.0, 6.0 * per_unit), 3.0);
        assert_eq!(
            place_route_head(&r, Pt2::new(1.0, 2.0), 90.0, stroke).unwrap(),
            place_head(Pt2::new(1.0, 2.0), 90.0, stroke)
        );
    }
}
