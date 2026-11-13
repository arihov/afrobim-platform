//! Drafting aids: grid snap, ortho, grid display stepping and typed point entry.

use crate::Pt;

/// Rounds a point to the nearest snap-grid node.
pub fn snap(p: Pt, spacing: f64) -> Pt {
    if spacing <= 0.0 {
        return p;
    }
    Pt::new((p.x / spacing).round() * spacing, (p.y / spacing).round() * spacing)
}

/// Ortho: keeps only the larger axis of the displacement from `from`.
pub fn ortho(from: Pt, to: Pt) -> Pt {
    if (to.x - from.x).abs() >= (to.y - from.y).abs() {
        Pt::new(to.x, from.y)
    } else {
        Pt::new(from.x, to.y)
    }
}

/// Grid line spacing for display: the smallest 1-2-5 multiple of `base` that is at least
/// `min_pixels` apart on screen, so the grid stays readable at every zoom.
pub fn display_spacing(base: f64, pixels_per_mm: f64, min_pixels: f64) -> f64 {
    if base <= 0.0 || pixels_per_mm <= 0.0 {
        return base;
    }
    let steps = [2.0, 2.5, 2.0]; // 1 → 2 → 5 → 10
    let mut s = base;
    let mut i = 0;
    while s * pixels_per_mm < min_pixels && s < 1e9 {
        s *= steps[i % 3];
        i += 1;
    }
    s
}

/// Parses typed point input the AutoCAD way: `x,y` is absolute, `@dx,dy` is relative to `last`,
/// and `@d<a` is a polar offset (degrees anticlockwise from east). Millimetres.
pub fn parse_point(text: &str, last: Option<Pt>) -> Option<Pt> {
    let mut t = text.trim();
    if t.is_empty() {
        return None;
    }
    let relative = t.starts_with('@');
    if relative {
        t = &t[1..];
    }
    let origin = if relative { last.unwrap_or_default() } else { Pt::default() };
    let num = |s: &str| s.trim().parse::<f64>().ok().filter(|v| v.is_finite());

    if let Some((d, a)) = t.split_once('<') {
        let (d, a) = (num(d)?, num(a)?);
        let rad = a.to_radians();
        return Some(Pt::new(origin.x + d * rad.cos(), origin.y + d * rad.sin()));
    }
    let mut parts = t.split(',');
    let (x, y) = (num(parts.next()?)?, num(parts.next()?)?);
    if parts.next().is_some() {
        return None;
    }
    Some(Pt::new(origin.x + x, origin.y + y))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn snap_and_ortho() {
        assert_eq!(snap(Pt::new(163.0, -38.0), 50.0), Pt::new(150.0, -50.0));
        assert_eq!(snap(Pt::new(163.0, -38.0), 0.0), Pt::new(163.0, -38.0));
        assert_eq!(ortho(Pt::new(10.0, 10.0), Pt::new(90.0, 40.0)), Pt::new(90.0, 10.0));
        assert_eq!(ortho(Pt::new(10.0, 10.0), Pt::new(30.0, 95.0)), Pt::new(10.0, 95.0));
    }

    #[test]
    fn display_grid_steps_1_2_5() {
        assert_eq!(display_spacing(100.0, 1.0, 12.0), 100.0);
        assert_eq!(display_spacing(100.0, 0.05, 12.0), 500.0);
        assert_eq!(display_spacing(100.0, 0.011, 12.0), 2000.0);
    }

    #[test]
    fn typed_points() {
        assert_eq!(parse_point("1200, -350.5", None), Some(Pt::new(1200.0, -350.5)));
        assert_eq!(parse_point("@100,50", Some(Pt::new(10.0, 10.0))), Some(Pt::new(110.0, 60.0)));
        let p = parse_point("@1000<90", Some(Pt::new(5.0, 5.0))).unwrap();
        assert!((p.x - 5.0).abs() < 1e-9 && (p.y - 1005.0).abs() < 1e-9);
        assert_eq!(parse_point("move", None), None);
        assert_eq!(parse_point("1,2,3", None), None);
        assert_eq!(parse_point("", None), None);
    }
}
