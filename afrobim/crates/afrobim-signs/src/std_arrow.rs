//! Detached arrow heads: the head a stroked shaft is capped with on supplementary and
//! information plates. Port of the head half of C# `ArrowGeometry` (`PlaceHead`,
//! `PlaceTriangleHead` and the pack-selected route head); the whole-arrow outline is in
//! [`crate::arrow`].
//!
//! `place_head` reproduces the UG-TSM-2004 Figure-5 head at whatever shaft width it caps, its
//! proportions expressed as multiples of the shaft width (at shaft width 3: head flare 3,
//! head length 12, flare 70°, inner fillet r 2.0, tip truncation r 0.2). A pack whose arrows
//! are the UK TSM Chapter 7 chevron style keeps the compact triangle head
//! (`place_triangle_head`, a provisional App.1 §2.2 proportion and not a UK-cited value) on the
//! full shaft width, as the C# engine does.

use afrobim_draw::Pt2;

use crate::pack::ArrowRules;

// Figure-5 head proportions as multiples of the shaft width.
const HEAD_LENGTH_RATIO: f64 = 4.0; // head length 12 / 3
const HEAD_FLARE_RATIO: f64 = 1.0; // head flare 3 / 3 (half-base = 1.5 × shaft width)
const HEAD_FLARE_ANGLE: f64 = 70.0; // back-flare angle
const HEAD_INNER_RAD_RATIO: f64 = 2.0 / 3.0; // inner corner radius 2 / 3
const HEAD_TIP_RAD_RATIO: f64 = 0.2 / 3.0; // tip radius 0.2 / 3

/// Length of the compact triangle head per unit of shaft width.
pub(crate) const TRIANGLE_HEAD_LENGTH_RATIO: f64 = 2.0;

fn deg(degrees: f64) -> f64 {
    degrees * std::f64::consts::PI / 180.0
}

/// Full width of the Figure-5 head across its base: 3 × the shaft width.
pub(crate) fn head_across(shaft_width: f64) -> f64 {
    2.0 * (0.5 + HEAD_FLARE_RATIO) * shaft_width
}

/// Tip to the arm junction of the Figure-5 head: the head length plus the back flare's rise.
pub(crate) fn head_back_length(shaft_width: f64) -> f64 {
    shaft_width * (HEAD_LENGTH_RATIO + HEAD_FLARE_RATIO / deg(HEAD_FLARE_ANGLE).tan())
}

/// The shaft width whose Figure-5 head takes half of `available_along_arm`, capped at the
/// standard shaft width.
pub(crate) fn fitting_stroke(standard_shaft_width: f64, available_along_arm: f64) -> f64 {
    let fitting = 0.5 * available_along_arm / head_back_length(1.0);
    standard_shaft_width.min(fitting)
}

/// True for a pack whose arrows are the Figure-5 (flared) style, or that has no arrows section.
pub(crate) fn uses_figure5_head(rules: Option<&ArrowRules>) -> bool {
    !rules.is_some_and(ArrowRules::is_chevron)
}

pub(crate) fn route_arm_stroke(
    rules: Option<&ArrowRules>,
    standard_shaft_width: f64,
    available_along_arm: f64,
) -> f64 {
    if uses_figure5_head(rules) {
        fitting_stroke(standard_shaft_width, available_along_arm)
    } else {
        standard_shaft_width
    }
}

pub(crate) fn route_head_length(rules: Option<&ArrowRules>, shaft_width: f64) -> f64 {
    if uses_figure5_head(rules) {
        head_back_length(shaft_width)
    } else {
        TRIANGLE_HEAD_LENGTH_RATIO * shaft_width
    }
}

pub(crate) fn place_route_head(
    rules: Option<&ArrowRules>,
    tip: Pt2,
    rotation_deg: f64,
    shaft_width: f64,
) -> Vec<Pt2> {
    if uses_figure5_head(rules) {
        place_head(tip, rotation_deg, shaft_width)
    } else {
        place_triangle_head(tip, rotation_deg, shaft_width)
    }
}

/// The Figure-5 head with its tip on `tip`, turned `rotation_deg` anticlockwise from pointing
/// up (+90° points left, −90° right). The edge between the last and first vertices is the flat
/// arm-junction span the shaft tucks into.
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
    add_fillet(
        &mut pts,
        (-a, y_back),
        (1.0, 0.0),
        normalise(a - b, d_y),
        rf,
    );
    pts.push((-b, y_base));
    let up_left = normalise(b, -y_base); // base-left → tip
    let up_right = normalise(-b, -y_base); // base-right → tip
    pts.push((-up_left.0 * rt, -up_left.1 * rt)); // tip-left (truncated apex)
    pts.push((-up_right.0 * rt, -up_right.1 * rt)); // tip-right (truncated apex)

    // Right side: base → filleted concave back corner.
    pts.push((b, y_base));
    add_fillet(
        &mut pts,
        (a, y_back),
        normalise(b - a, d_y),
        (-1.0, 0.0),
        rf,
    );

    let (sin, cos) = deg(rotation_deg).sin_cos();
    pts.into_iter()
        .map(|(x, y)| Pt2::new(tip.x + x * cos - y * sin, tip.y + x * sin + y * cos))
        .collect()
}

/// The compact triangle head: tip on `tip`, base 3 × the shaft width wide, 2 × long.
pub(crate) fn place_triangle_head(tip: Pt2, rotation_deg: f64, shaft_width: f64) -> Vec<Pt2> {
    let half_base = 1.5 * shaft_width;
    let length = TRIANGLE_HEAD_LENGTH_RATIO * shaft_width;
    let (sin, cos) = deg(rotation_deg).sin_cos();
    // Canonical frame points +Y, tip at origin, base corners at (±half_base, −length).
    let rot = |x: f64, y: f64| Pt2::new(tip.x + x * cos - y * sin, tip.y + x * sin + y * cos);
    vec![
        rot(0.0, 0.0),
        rot(half_base, -length),
        rot(-half_base, -length),
    ]
}

/// Replaces the sharp vertex `corner` with a tangent-in / quadratic-bulge / tangent-out
/// sequence, so the concave junction reads as a radius rather than a mitre.
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
    const STEPS: u32 = 4;
    for i in 1..STEPS {
        let t = f64::from(i) / f64::from(STEPS);
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

#[cfg(test)]
mod tests {
    use super::*;

    fn bounds(o: &[Pt2]) -> (f64, f64, f64, f64) {
        o.iter()
            .fold((f64::MAX, f64::MAX, f64::MIN, f64::MIN), |b, p| {
                (b.0.min(p.x), b.1.min(p.y), b.2.max(p.x), b.3.max(p.y))
            })
    }

    /// At shaft width 3 the head is the Figure-5 head: 9 across, 12 long plus the back flare
    /// 3 / tan 70°.
    #[test]
    fn the_figure_5_head_scales_with_the_shaft_width() {
        assert!((head_across(3.0) - 9.0).abs() < 1e-12);
        let back = 12.0 + 3.0 / 70f64.to_radians().tan();
        assert!((head_back_length(3.0) - back).abs() < 1e-12);
        assert!((head_back_length(1.0) * 2.0 - head_back_length(2.0)).abs() < 1e-12);

        let head = place_head(Pt2::new(10.0, 20.0), 0.0, 3.0);
        let (x0, y0, x1, y1) = bounds(&head);
        assert!((x0 - 5.5).abs() < 1e-9 && (x1 - 14.5).abs() < 1e-9);
        assert!((y0 - (20.0 - back)).abs() < 1e-9);
        assert!(y1 < 20.0 && y1 > 19.8); // the apex is truncated, not sharp
    }

    #[test]
    fn a_head_turned_90_degrees_points_left() {
        let head = place_head(Pt2::new(0.0, 0.0), 90.0, 2.0);
        let (x0, y0, x1, y1) = bounds(&head);
        assert!(x0 > -0.2 && (x1 - head_back_length(2.0)).abs() < 1e-9);
        assert!((y0 + 3.0).abs() < 1e-9 && (y1 - 3.0).abs() < 1e-9);
    }

    #[test]
    fn the_stroke_shrinks_to_fit_a_short_run() {
        assert_eq!(fitting_stroke(3.0, 1000.0), 3.0);
        let s = fitting_stroke(3.0, 8.0);
        assert!((head_back_length(s) - 4.0).abs() < 1e-12);
    }

    #[test]
    fn the_triangle_head_is_three_wide_and_two_long() {
        let head = place_triangle_head(Pt2::new(5.0, 5.0), -90.0, 2.0);
        assert_eq!(head.len(), 3);
        let (x0, y0, x1, y1) = bounds(&head);
        assert!((x0 - 1.0).abs() < 1e-9 && (x1 - 5.0).abs() < 1e-9);
        assert!((y0 - 2.0).abs() < 1e-9 && (y1 - 8.0).abs() < 1e-9);
    }

    #[test]
    fn a_chevron_pack_keeps_the_triangle_head_on_the_full_shaft() {
        let uk = crate::pack::tests::uk_pack();
        let chevron = uk.arrows.as_ref();
        assert!(!uses_figure5_head(chevron));
        assert!(uses_figure5_head(None));
        assert_eq!(route_arm_stroke(chevron, 4.0, 8.0), 4.0);
        assert_eq!(route_head_length(chevron, 4.0), 8.0);
        assert_eq!(
            place_route_head(chevron, Pt2::new(0.0, 0.0), 0.0, 4.0).len(),
            3
        );
        assert!(route_arm_stroke(None, 4.0, 8.0) < 4.0);
        assert!(place_route_head(None, Pt2::new(0.0, 0.0), 0.0, 4.0).len() > 3);
    }
}
