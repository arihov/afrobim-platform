//! Cross-section slices: the points within a thickness of a section line, projected to offset
//! along the line and elevation, for drawing over design sections.

use crate::error::{PointCloudError, Result};
use crate::filter::PointFilter;
use crate::index::PointIndex;
use serde::{Deserialize, Serialize};

/// One projected point.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct SectionPoint {
    /// Index into the index's point arrays.
    pub index: u32,
    /// Distance along the section line from its first vertex to the foot of the point, metres.
    pub offset: f64,
    /// Elevation, true.
    pub z: f64,
    /// Signed distance from the line in plan: positive on the left looking along the line.
    pub lateral: f64,
    pub class: u8,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SectionSlice {
    /// Sorted by `offset`.
    pub points: Vec<SectionPoint>,
    /// Length of the section line.
    pub length: f64,
    /// The thickness used (total, centred on the line).
    pub thickness: f64,
}

/// Points within `thickness / 2` (plan distance) of a section line, which may be a polyline of two
/// or more vertices `[x, y]`. A point near a bend is projected onto its nearest segment, and
/// `offset` is measured along the whole polyline. Points within the thickness of the ends are
/// included (the line is not extended). `filter` is optional.
pub fn section_slice(index: &PointIndex, line: &[[f64; 2]], thickness: f64, filter: Option<&PointFilter>) -> Result<SectionSlice> {
    if line.len() < 2 {
        return Err(PointCloudError::Invalid("a section line needs at least two vertices".into()));
    }
    if !(thickness > 0.0 && thickness.is_finite()) {
        return Err(PointCloudError::Invalid("the thickness must be positive".into()));
    }
    let half = thickness / 2.0;
    let mut cum = vec![0.0f64];
    for w in line.windows(2) {
        let l = ((w[1][0] - w[0][0]).powi(2) + (w[1][1] - w[0][1]).powi(2)).sqrt();
        cum.push(cum[cum.len() - 1] + l);
    }
    let length = cum[cum.len() - 1];
    let mut ids = index.query_polyline(line, half);
    if let Some(f) = filter {
        ids = f.apply_to(index.points(), &ids);
    }
    let mut out = Vec::with_capacity(ids.len());
    for i in ids {
        let p = index.points().xyz[i as usize];
        let mut best: Option<(f64, f64, f64)> = None; // distance, offset, lateral
        for (s, w) in line.windows(2).enumerate() {
            let (dx, dy) = (w[1][0] - w[0][0], w[1][1] - w[0][1]);
            let l2 = dx * dx + dy * dy;
            if l2 == 0.0 {
                continue;
            }
            let (px, py) = (p[0] - w[0][0], p[1] - w[0][1]);
            let t = ((px * dx + py * dy) / l2).clamp(0.0, 1.0);
            let (fx, fy) = (px - t * dx, py - t * dy);
            let d = (fx * fx + fy * fy).sqrt();
            if best.is_none_or(|b| d < b.0) {
                let sign = if dx * py - dy * px >= 0.0 { 1.0 } else { -1.0 };
                best = Some((d, cum[s] + t * l2.sqrt(), sign * d));
            }
        }
        if let Some((d, offset, lateral)) = best {
            if d <= half {
                out.push(SectionPoint { index: i, offset, z: p[2], lateral, class: index.points().class[i as usize] });
            }
        }
    }
    out.sort_by(|a, b| a.offset.total_cmp(&b.offset).then(a.index.cmp(&b.index)));
    Ok(SectionSlice { points: out, length, thickness })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::buffer::{PointBuffer, PointRecord};
    use crate::index::IndexOptions;

    #[test]
    fn slice_projects_to_offset_and_elevation() {
        let mut b = PointBuffer::new(false, false);
        // A line of points along y = 9960000 (z = x offset / 10), one row 0.4 m north, one row 3 m north.
        for k in 0..100 {
            let x = 500000.0 + k as f64 * 0.5;
            for (dy, class) in [(0.0, 2u8), (0.4, 2), (3.0, 5)] {
                b.push(&PointRecord { xyz: [x, 9960000.0 + dy, 50.0 + k as f64 * 0.05], class, ..Default::default() });
            }
        }
        let ix = PointIndex::build(b, &IndexOptions::default()).unwrap();
        let s = section_slice(&ix, &[[500010.0, 9960000.0], [500020.0, 9960000.0]], 1.0, None).unwrap();
        assert!((s.length - 10.0).abs() < 1e-9);
        // x from 500010 to 500020 step 0.5: 21 columns in both rows within 0.5 m, plus the two
        // points of the centre row that lie exactly 0.5 m beyond the ends.
        assert_eq!(s.points.len(), 44);
        assert!(s.points.windows(2).all(|w| w[0].offset <= w[1].offset));
        assert!(s.points.iter().all(|p| p.lateral.abs() <= 0.5 + 1e-9 && p.class == 2));
        assert!(s.points[0].offset.abs() < 1e-9);
        let mid = s.points.iter().find(|p| (p.offset - 5.0).abs() < 1e-9 && p.lateral == 0.0).unwrap();
        assert!((mid.z - (50.0 + 30.0 * 0.05)).abs() < 1e-9);
        // Left of a line running east is north: positive lateral for the 0.4 m row.
        assert!(s.points.iter().any(|p| (p.lateral - 0.4).abs() < 1e-9));
        // Class filter.
        let f = PointFilter { classes: Some(vec![5]), ..Default::default() };
        let wide = section_slice(&ix, &[[500010.0, 9960000.0], [500020.0, 9960000.0]], 7.0, Some(&f)).unwrap();
        // 21 columns on the line plus 3 beyond each end (sqrt(3.5^2 - 3^2) = 1.8 m).
        assert_eq!(wide.points.len(), 27);
        assert!(section_slice(&ix, &[[0.0, 0.0]], 1.0, None).is_err());
    }
}
