// SPDX-License-Identifier: LicenseRef-AfroBIM-Proprietary
// Copyright (c) 2026 AfroBIM. All rights reserved.
//! Affine transforms and small vector helpers.

pub type V3 = [f64; 3];

/// Row-major 3x4 affine matrix: `p' = A * p + t`, with `A = m[.][0..3]` and `t = m[.][3]`.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Mat(pub [[f64; 4]; 3]);

pub fn sub(a: V3, b: V3) -> V3 {
    [a[0] - b[0], a[1] - b[1], a[2] - b[2]]
}
pub fn dot(a: V3, b: V3) -> f64 {
    a[0] * b[0] + a[1] * b[1] + a[2] * b[2]
}
pub fn cross(a: V3, b: V3) -> V3 {
    [a[1] * b[2] - a[2] * b[1], a[2] * b[0] - a[0] * b[2], a[0] * b[1] - a[1] * b[0]]
}
pub fn scale(a: V3, k: f64) -> V3 {
    [a[0] * k, a[1] * k, a[2] * k]
}
pub fn len(a: V3) -> f64 {
    dot(a, a).sqrt()
}
pub fn unit(a: V3) -> Option<V3> {
    let l = len(a);
    (l > 1e-12 && l.is_finite()).then(|| scale(a, 1.0 / l))
}

impl Mat {
    pub const IDENTITY: Mat = Mat([[1.0, 0.0, 0.0, 0.0], [0.0, 1.0, 0.0, 0.0], [0.0, 0.0, 1.0, 0.0]]);

    /// From the images of the unit axes and the origin (columns).
    pub fn from_axes(x: V3, y: V3, z: V3, o: V3) -> Mat {
        Mat([[x[0], y[0], z[0], o[0]], [x[1], y[1], z[1], o[1]], [x[2], y[2], z[2], o[2]]])
    }

    pub fn translation(t: V3) -> Mat {
        Mat::from_axes([1.0, 0.0, 0.0], [0.0, 1.0, 0.0], [0.0, 0.0, 1.0], t)
    }

    pub fn uniform_scale(s: f64) -> Mat {
        Mat::from_axes([s, 0.0, 0.0], [0.0, s, 0.0], [0.0, 0.0, s], [0.0; 3])
    }

    pub fn apply(&self, p: V3) -> V3 {
        let m = &self.0;
        [
            m[0][0] * p[0] + m[0][1] * p[1] + m[0][2] * p[2] + m[0][3],
            m[1][0] * p[0] + m[1][1] * p[1] + m[1][2] * p[2] + m[1][3],
            m[2][0] * p[0] + m[2][1] * p[1] + m[2][2] * p[2] + m[2][3],
        ]
    }

    /// `self * other`: apply `other` first.
    pub fn mul(&self, o: &Mat) -> Mat {
        let a = &self.0;
        let b = &o.0;
        let mut r = [[0.0; 4]; 3];
        for (i, row) in r.iter_mut().enumerate() {
            for (j, cell) in row.iter_mut().enumerate() {
                *cell = a[i][0] * b[0][j] + a[i][1] * b[1][j] + a[i][2] * b[2][j] + if j == 3 { a[i][3] } else { 0.0 };
            }
        }
        Mat(r)
    }

    pub fn det(&self) -> f64 {
        let m = &self.0;
        m[0][0] * (m[1][1] * m[2][2] - m[1][2] * m[2][1]) - m[0][1] * (m[1][0] * m[2][2] - m[1][2] * m[2][0]) + m[0][2] * (m[1][0] * m[2][1] - m[1][1] * m[2][0])
    }

    #[cfg(test)]
    pub fn inverse(&self) -> Option<Mat> {
        let d = self.det();
        if d.abs() < 1e-18 || !d.is_finite() {
            return None;
        }
        let m = &self.0;
        let inv = 1.0 / d;
        let a = [
            [(m[1][1] * m[2][2] - m[1][2] * m[2][1]) * inv, (m[0][2] * m[2][1] - m[0][1] * m[2][2]) * inv, (m[0][1] * m[1][2] - m[0][2] * m[1][1]) * inv],
            [(m[1][2] * m[2][0] - m[1][0] * m[2][2]) * inv, (m[0][0] * m[2][2] - m[0][2] * m[2][0]) * inv, (m[0][2] * m[1][0] - m[0][0] * m[1][2]) * inv],
            [(m[1][0] * m[2][1] - m[1][1] * m[2][0]) * inv, (m[0][1] * m[2][0] - m[0][0] * m[2][1]) * inv, (m[0][0] * m[1][1] - m[0][1] * m[1][0]) * inv],
        ];
        let t = [m[0][3], m[1][3], m[2][3]];
        let mut r = [[0.0; 4]; 3];
        for i in 0..3 {
            for j in 0..3 {
                r[i][j] = a[i][j];
            }
            r[i][3] = -(a[i][0] * t[0] + a[i][1] * t[1] + a[i][2] * t[2]);
        }
        Some(Mat(r))
    }
}

/// `IfcAxis2Placement3D` from its three attributes (all optional but the location).
/// Z is the axis (default +Z), X the reference direction made perpendicular to Z (default +X;
/// when it is parallel to Z, a perpendicular is chosen), Y = Z x X.
pub fn placement_3d(location: V3, axis: Option<V3>, ref_direction: Option<V3>) -> Mat {
    let z = axis.and_then(unit).unwrap_or([0.0, 0.0, 1.0]);
    let r = ref_direction.and_then(unit).unwrap_or([1.0, 0.0, 0.0]);
    let mut x = sub(r, scale(z, dot(r, z)));
    if len(x) < 1e-9 {
        let alt = if z[0].abs() < 0.9 { [1.0, 0.0, 0.0] } else { [0.0, 1.0, 0.0] };
        x = sub(alt, scale(z, dot(alt, z)));
    }
    let x = unit(x).unwrap_or([1.0, 0.0, 0.0]);
    let y = cross(z, x);
    Mat::from_axes(x, y, z, location)
}

/// `IfcAxis2Placement2D`: X is the reference direction (default +X), Y its left normal.
pub fn placement_2d(location: [f64; 2], ref_direction: Option<[f64; 2]>) -> Mat {
    let r = ref_direction.filter(|d| d[0].hypot(d[1]) > 1e-12).unwrap_or([1.0, 0.0]);
    let l = r[0].hypot(r[1]);
    let x = [r[0] / l, r[1] / l, 0.0];
    let y = [-x[1], x[0], 0.0];
    Mat::from_axes(x, y, [0.0, 0.0, 1.0], [location[0], location[1], 0.0])
}

#[cfg(test)]
mod tests {
    use super::*;

    fn near(a: V3, b: V3) -> bool {
        (0..3).all(|i| (a[i] - b[i]).abs() < 1e-9)
    }

    #[test]
    fn placement_defaults_and_orthogonalisation() {
        let m = placement_3d([1.0, 2.0, 3.0], None, None);
        assert!(near(m.apply([1.0, 1.0, 1.0]), [2.0, 3.0, 4.0]));
        // Axis +X, reference direction +Z: the frame is rotated, X = +Z projected away => fallback.
        let m = placement_3d([0.0; 3], Some([1.0, 0.0, 0.0]), Some([0.0, 1.0, 0.0]));
        assert!(near(m.apply([1.0, 0.0, 0.0]), [0.0, 1.0, 0.0]));
        assert!(near(m.apply([0.0, 0.0, 1.0]), [1.0, 0.0, 0.0]));
        assert!((m.det() - 1.0).abs() < 1e-12);
        // Reference direction parallel to the axis does not produce NaN.
        let m = placement_3d([0.0; 3], Some([0.0, 0.0, 1.0]), Some([0.0, 0.0, 2.0]));
        assert!((m.det() - 1.0).abs() < 1e-12);
    }

    #[test]
    fn inverse_and_product() {
        let a = placement_3d([5.0, -2.0, 1.0], Some([1.0, 1.0, 1.0]), Some([1.0, 0.0, 0.0]));
        let i = a.inverse().unwrap();
        let p = [3.0, 4.0, 5.0];
        assert!(near(i.apply(a.apply(p)), p));
        assert!(near(a.mul(&i).apply(p), p));
        let b = Mat::translation([1.0, 0.0, 0.0]);
        assert!(near(a.mul(&b).apply([0.0; 3]), a.apply([1.0, 0.0, 0.0])));
        assert!(Mat::uniform_scale(0.0).inverse().is_none());
    }
}
