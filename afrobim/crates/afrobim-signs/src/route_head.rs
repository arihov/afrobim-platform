//! The arrowhead that caps a route arm drawn as a stroked line (lane and diversion arrows).
//! Port of the pack-selected route-arm head of C# `ArrowGeometry` (`RouteArmStroke`,
//! `RouteHeadLength`, `PlaceRouteHead`, `PlaceTriangleHead`).
//!
//! The C# engine picks the head by the pack's arrow style. A pack whose arrows are the UK TSM
//! Chapter 7 chevron style (UK-TSM-2019) draws the compact triangle head at the full shaft
//! width. That head is a provisional App.1 §2.2 proportion, not a UK-cited value; it is ported
//! as the C# engine draws it. Every other pack draws the Uganda Figure 5 barbed head with the
//! stroke fitted to the run (`PlaceHead`, `FittingStroke`), which is not ported: those
//! functions return [`SignError::Unsupported`] for such a pack.

use afrobim_draw::Pt2;

use crate::pack::ArrowRules;
use crate::SignError;

/// Length of the compact triangle head per unit shaft width ([`place_triangle_head`]).
pub(crate) const TRIANGLE_HEAD_LENGTH_RATIO: f64 = 2.0;

/// True when route-arm heads for this pack use the Figure 5 barbed head: every pack except one
/// declaring the chevron arrow style.
pub(crate) fn uses_figure5_head(rules: &ArrowRules) -> bool {
    !rules.is_chevron()
}

fn triangle_head_only(rules: &ArrowRules) -> Result<(), SignError> {
    if uses_figure5_head(rules) {
        return Err(SignError::Unsupported(
            "route-arm arrowheads for a rule pack with Figure 5 (flared) arrows: only the triangle head of a \
             chevron-arrow pack (UK TSM Chapter 7) is ported"
                .to_string(),
        ));
    }
    Ok(())
}

/// Route-arm shaft width: the standard width for a chevron-arrow pack. (A Figure 5 pack fits
/// it to `available_along_arm`; not ported.)
pub(crate) fn route_arm_stroke(
    rules: &ArrowRules,
    standard_shaft_width: f64,
    _available_along_arm: f64,
) -> Result<f64, SignError> {
    triangle_head_only(rules)?;
    Ok(standard_shaft_width)
}

/// Arm length the route-arm head occupies behind its tip for this pack.
pub(crate) fn route_head_length(rules: &ArrowRules, shaft_width: f64) -> Result<f64, SignError> {
    triangle_head_only(rules)?;
    Ok(TRIANGLE_HEAD_LENGTH_RATIO * shaft_width)
}

/// The outline of the route-arm head this pack draws.
pub(crate) fn place_route_head(
    rules: &ArrowRules,
    tip: Pt2,
    rotation_deg: f64,
    shaft_width: f64,
) -> Result<Vec<Pt2>, SignError> {
    triangle_head_only(rules)?;
    Ok(place_triangle_head(tip, rotation_deg, shaft_width))
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

    #[test]
    fn a_figure_5_pack_is_unsupported_not_drawn_with_the_wrong_head() {
        let pack = uk_pack();
        let mut r = pack.arrows.clone().unwrap();
        r.style = Some("flared".to_string());
        assert!(uses_figure5_head(&r));
        for result in [
            route_arm_stroke(&r, 3.0, 8.0).map(|_| ()),
            route_head_length(&r, 3.0).map(|_| ()),
            place_route_head(&r, Pt2::new(0.0, 0.0), 0.0, 3.0).map(|_| ()),
        ] {
            assert!(matches!(result, Err(SignError::Unsupported(_))));
        }
    }
}
