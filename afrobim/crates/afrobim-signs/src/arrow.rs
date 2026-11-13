//! The standard guidance arrow as a closed stroke-width outline driven entirely by
//! [`ArrowRules`]. Port of the parts of C# `ArrowGeometry` the stack-type layout uses.
//!
//! Canonical frame: the shaft tail is centred on the origin and the arrow points along +Y
//! ("ahead"). Callers orient it with [`place`], which rotates the outline counter-clockwise
//! about the tail and translates it to a target point.
//!
//! Flared outline (App.1 §2.6, Figure 5). The compressed summary dimensions are applied as
//! follows (documented so the geometry is reproducible rather than hand-plotted):
//!   a   = ShaftWidth / 2                 shaft half-width
//!   b   = a + HeadFlare                  head base half-width (the flare steps out HeadFlare per side)
//!   dY  = HeadFlare / tan(FlareAngle)    axial thickness of the back flare (≈1.09 sw at 70°)
//!   head base sits at y = ShaftLength + dY; tip at y = ShaftLength + dY + HeadLength
//!   InnerCornerRadius rounds the two concave shaft→flare corners
//!   TipRadius truncates the apex (a flat of ~2×TipRadius)
//!
//! FlareAngle/HeadLength/HeadFlare are treated as the authoritative outline drivers;
//! PointHalfAngle is intentionally not used to reshape the flared outline.

use afrobim_draw::Pt2;

use crate::pack::ArrowRules;

fn deg(degrees: f64) -> f64 {
    degrees * std::f64::consts::PI / 180.0
}

/// Axial thickness of the back flare for the given rules.
pub fn flare_rise(r: &ArrowRules) -> f64 {
    r.head_flare / deg(r.flare_angle_deg).tan()
}

/// Half-width of the arrow at its widest point (the head base).
pub fn head_base_half_width(r: &ArrowRules) -> f64 {
    r.shaft_width / 2.0 + r.head_flare
}

/// Total tip-to-tail length. Flared: shaft + flare thickness + head. Chevron: shaft + head,
/// because the chevron's back edges sweep forward inside the head length (UK Figure 4-4:
/// 8 + 8 = 16 sw). `shaft_length` overrides the pack shaft, e.g. with
/// `ArrowRules::short_shaft_length` for the 14 sw arrow.
pub fn total_length(r: &ArrowRules, shaft_length: Option<f64>) -> f64 {
    let shaft = shaft_length.unwrap_or(r.shaft_length);
    if r.is_chevron() {
        shaft + r.head_length
    } else {
        shaft + flare_rise(r) + r.head_length
    }
}

/// Widest across-dimension of the arrow (the head base).
pub fn total_width(r: &ArrowRules) -> f64 {
    2.0 * head_base_half_width(r)
}

/// The closed arrow outline in the canonical frame (tail centre at origin, tip +Y). Vertices
/// run from the shaft tail's left corner up the left side, over the tip and down the right
/// side. `shaft_length` overrides `ArrowRules::shaft_length`.
pub fn outline(r: &ArrowRules, shaft_length: Option<f64>) -> Vec<Pt2> {
    let shaft = shaft_length.unwrap_or(r.shaft_length);
    if r.is_chevron() {
        return chevron_outline(r, shaft);
    }

    let a = r.shaft_width / 2.0;
    let b = a + r.head_flare;
    let d_y = flare_rise(r);
    let y_fb = shaft; // shaft top = fillet corner
    let y_hb = shaft + d_y; // head base (widest)
    let y_tip = y_hb + r.head_length; // apex

    // Cap the rounding so a fillet never exceeds the edges it rounds.
    let edge_len = (r.head_flare * r.head_flare + d_y * d_y).sqrt();
    let rf = r.inner_corner_radius.min(0.4 * y_fb.min(edge_len));
    let rt = r.tip_radius.min(0.4 * r.head_length);

    let mut pts: Vec<(f64, f64)> = Vec::new();

    // ---- left side, tail → shaft → flare → head base ----
    pts.push((-a, 0.0));
    let corner_l = (-a, y_fb);
    let head_base_l = (-b, y_hb);
    {
        // Left fillet: back along the shaft side, then toward the flare.
        let toward_flare = normalise(head_base_l.0 - corner_l.0, head_base_l.1 - corner_l.1);
        let t_prev = (corner_l.0, corner_l.1 - rf);
        let t_next = (
            corner_l.0 + toward_flare.0 * rf,
            corner_l.1 + toward_flare.1 * rf,
        );
        add_quadratic(&mut pts, t_prev, corner_l, t_next);
    }
    pts.push(head_base_l);

    // ---- tip (truncate the apex) ----
    let tip = (0.0, y_tip);
    let up_left = normalise(tip.0 - head_base_l.0, tip.1 - head_base_l.1); // left head edge direction
    let head_base_r = (b, y_hb);
    let up_right = normalise(tip.0 - head_base_r.0, tip.1 - head_base_r.1); // right head edge direction
    pts.push((tip.0 - up_left.0 * rt, tip.1 - up_left.1 * rt)); // tip-left
    pts.push((tip.0 - up_right.0 * rt, tip.1 - up_right.1 * rt)); // tip-right

    // ---- right side, head base → flare → shaft → tail ----
    pts.push(head_base_r);
    let corner_r = (a, y_fb);
    {
        // Right fillet: toward the head base, then down the shaft side.
        let from_flare = normalise(head_base_r.0 - corner_r.0, head_base_r.1 - corner_r.1);
        let t_prev = (
            corner_r.0 + from_flare.0 * rf,
            corner_r.1 + from_flare.1 * rf,
        );
        let t_next = (corner_r.0, corner_r.1 - rf);
        add_quadratic(&mut pts, t_prev, corner_r, t_next);
    }
    pts.push((a, 0.0));

    pts.into_iter().map(|(x, y)| Pt2::new(x, y)).collect()
}

/// The UK TSM Chapter 7 Figure 4-4 arrow (TSRGD 2016 S12-5-1). With the UK pack values
/// (shaft 8, shaft width 8/3, head 8, flare 8/3, both angles 45°) it reproduces the figure:
/// 16 sw long, 8 sw across the head, 45° flanks from the tip to square barbs 4 sw long, and
/// back edges at 45° meeting a shaft one third of the head width.
///
///   h   = ShaftWidth / 2 + HeadFlare        head half-width
///   barb outer corner at y = tip − h / tan(PointHalfAngle)
///   barb back corner  at y = shaftLength
///   shaft junction    at y = shaftLength + HeadFlare / tan(FlareAngle)
fn chevron_outline(r: &ArrowRules, shaft_length: f64) -> Vec<Pt2> {
    let a = r.shaft_width / 2.0;
    let h = a + r.head_flare;
    let y_tip = shaft_length + r.head_length;
    let y_barb_outer = y_tip - h / deg(r.point_half_angle_deg).tan();
    let y_join = shaft_length + r.head_flare / deg(r.flare_angle_deg).tan();

    vec![
        Pt2::new(-a, 0.0),
        Pt2::new(-a, y_join),
        Pt2::new(-h, shaft_length),
        Pt2::new(-h, y_barb_outer),
        Pt2::new(0.0, y_tip),
        Pt2::new(h, y_barb_outer),
        Pt2::new(h, shaft_length),
        Pt2::new(a, y_join),
        Pt2::new(a, 0.0),
    ]
}

/// Rotates the canonical outline counter-clockwise by `rotation_deg` about the tail and
/// translates the tail to `tail`: the arrow's outline in the sign's coordinate space
/// (0° = +Y "ahead", −90° = +X "right", +90° = −X "left").
pub fn place(r: &ArrowRules, tail: Pt2, rotation_deg: f64, shaft_length: Option<f64>) -> Vec<Pt2> {
    let rads = deg(rotation_deg);
    let (cos, sin) = (rads.cos(), rads.sin());
    outline(r, shaft_length)
        .into_iter()
        .map(|p| {
            Pt2::new(
                tail.x + p.x * cos - p.y * sin,
                tail.y + p.x * sin + p.y * cos,
            )
        })
        .collect()
}

/// Appends a quadratic Bézier from `p0` through control `c` to `p1`, sampling the interior
/// only (the endpoints are supplied by the caller's surrounding vertices).
fn add_quadratic(pts: &mut Vec<(f64, f64)>, p0: (f64, f64), c: (f64, f64), p1: (f64, f64)) {
    const STEPS: usize = 4;
    for i in 1..STEPS {
        let t = i as f64 / STEPS as f64;
        let u = 1.0 - t;
        pts.push((
            u * u * p0.0 + 2.0 * u * t * c.0 + t * t * p1.0,
            u * u * p0.1 + 2.0 * u * t * c.1 + t * t * p1.1,
        ));
    }
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
    use crate::pack::tests::uk_pack;

    fn close(a: f64, b: f64) -> bool {
        (a - b).abs() < 1e-6
    }

    fn bounds(pts: &[Pt2]) -> (f64, f64, f64, f64) {
        pts.iter().fold(
            (f64::MAX, f64::MAX, f64::MIN, f64::MIN),
            |(x0, y0, x1, y1), p| (x0.min(p.x), y0.min(p.y), x1.max(p.x), y1.max(p.y)),
        )
    }

    /// UK TSM Chapter 7 Figure 4-4: 16 sw long, 8 sw across the head, barbs 4 sw long.
    #[test]
    fn uk_chevron_reproduces_figure_4_4() {
        let pack = uk_pack();
        let r = pack.arrows.as_ref().unwrap();
        assert!(close(total_length(r, None), 16.0));
        assert!(close(total_width(r), 8.0));

        let o = outline(r, None);
        assert_eq!(o.len(), 9);
        let (x0, y0, x1, y1) = bounds(&o);
        assert!(close(x0, -4.0) && close(x1, 4.0) && close(y0, 0.0) && close(y1, 16.0));
        // Tip on the axis; 45° flanks reach the full half-width 4 sw behind it.
        assert!(close(o[4].x, 0.0) && close(o[4].y, 16.0));
        assert!(close(o[3].x, -4.0) && close(o[3].y, 12.0));
        // Square barb: back corner at the shaft length, 4 sw below the outer corner.
        assert!(close(o[2].x, -4.0) && close(o[2].y, 8.0));
        // The 45° back edge meets a shaft one third of the head width.
        assert!(close(o[1].x, -4.0 / 3.0) && close(o[1].y, 8.0 + 8.0 / 3.0));
        // Mirror-symmetric about the axis.
        for i in 0..9 {
            assert!(close(o[i].x, -o[8 - i].x) && close(o[i].y, o[8 - i].y));
        }
    }

    /// §4.2.2: the vertical arrow shortens from 16 sw to 14 sw beside a single-line legend.
    #[test]
    fn the_short_shaft_gives_the_14_sw_arrow() {
        let pack = uk_pack();
        let r = pack.arrows.as_ref().unwrap();
        let short = r.short_shaft_length.unwrap();
        assert!(close(total_length(r, Some(short)), 14.0));
        let (_, _, _, y1) = bounds(&outline(r, Some(short)));
        assert!(close(y1, 14.0));
        // The head is unchanged: only the shaft is shorter.
        let (full, cut) = (outline(r, None), outline(r, Some(short)));
        for i in 1..8 {
            assert!(close(full[i].y - cut[i].y, 2.0) && close(full[i].x, cut[i].x));
        }
    }

    #[test]
    fn place_rotates_about_the_tail() {
        let pack = uk_pack();
        let r = pack.arrows.as_ref().unwrap();
        let tail = Pt2::new(10.0, 20.0);
        // +90° points −X (left); −90° points +X (right).
        let (x0, y0, x1, y1) = bounds(&place(r, tail, 90.0, None));
        assert!(close(x0, -6.0) && close(x1, 10.0) && close(y0, 16.0) && close(y1, 24.0));
        let (x0, y0, x1, y1) = bounds(&place(r, tail, -90.0, None));
        assert!(close(x0, 10.0) && close(x1, 26.0) && close(y0, 16.0) && close(y1, 24.0));
        let (x0, y0, x1, y1) = bounds(&place(r, tail, 0.0, None));
        assert!(close(x0, 6.0) && close(x1, 14.0) && close(y0, 20.0) && close(y1, 36.0));
    }

    /// Uganda App.1 Figure 5 values (packs/UG-TSM-2004/arrows.xml): shaft 12 × 3, head 12,
    /// flare 3 at 70°, fillet r2, tip r0.2.
    #[test]
    fn flared_outline_follows_the_documented_construction() {
        let r = ArrowRules {
            shaft_length: 12.0,
            shaft_width: 3.0,
            head_length: 12.0,
            head_flare: 3.0,
            flare_angle_deg: 70.0,
            point_half_angle_deg: 20.0,
            inner_corner_radius: 2.0,
            tip_radius: 0.2,
            style: None,
            short_shaft_length: None,
        };
        let rise = 3.0 / 70f64.to_radians().tan();
        assert!(close(flare_rise(&r), rise));
        assert!(close(total_length(&r, None), 24.0 + rise));
        assert!(close(total_width(&r), 9.0));

        let o = outline(&r, None);
        // tail, 3 fillet samples, head base, 2 tip points, head base, 3 fillet samples, tail
        assert_eq!(o.len(), 12);
        assert!(close(o[0].x, -1.5) && close(o[0].y, 0.0));
        assert!(close(o[4].x, -4.5) && close(o[4].y, 12.0 + rise));
        assert!(close(o[7].x, 4.5) && close(o[11].x, 1.5));
        let (_, _, _, y1) = bounds(&o);
        assert!(y1 < 24.0 + rise && y1 > 24.0 + rise - 0.2); // the apex is truncated
        for i in 0..12 {
            assert!(close(o[i].x, -o[11 - i].x) && close(o[i].y, o[11 - i].y));
        }
    }
}
