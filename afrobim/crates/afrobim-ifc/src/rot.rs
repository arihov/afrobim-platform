// SPDX-License-Identifier: LicenseRef-AfroBIM-Proprietary
// Copyright (c) 2026 AfroBIM. All rights reserved.
//! 3x3 rotations, row-major: `m[row][column]`, the columns being the images of the unit axes.
//!
//! Used by the linear placement: an element placed on an alignment is given the frame of the
//! curve (tangent, left, normal), and what the element owns (its geometry, and the placements of
//! its parts) is turned by the rotation that carries the curve frame onto the frame the element
//! had when it was placed absolutely.

pub(crate) type Rot = [[f64; 3]; 3];
pub(crate) type V3 = [f64; 3];

pub(crate) const IDENTITY: Rot = [[1.0, 0.0, 0.0], [0.0, 1.0, 0.0], [0.0, 0.0, 1.0]];

/// Turn about +Z by `degrees`, counter-clockwise.
pub(crate) fn about_z(degrees: f64) -> Rot {
    let (s, c) = degrees.to_radians().sin_cos();
    [[c, -s, 0.0], [s, c, 0.0], [0.0, 0.0, 1.0]]
}

/// The matrix whose columns are `x`, `y`, `z`.
pub(crate) fn from_columns(x: V3, y: V3, z: V3) -> Rot {
    [[x[0], y[0], z[0]], [x[1], y[1], z[1]], [x[2], y[2], z[2]]]
}

pub(crate) fn column(m: &Rot, j: usize) -> V3 {
    [m[0][j], m[1][j], m[2][j]]
}

/// `a * b`.
pub(crate) fn mul(a: &Rot, b: &Rot) -> Rot {
    let mut r = [[0.0; 3]; 3];
    for (i, row) in r.iter_mut().enumerate() {
        for (j, cell) in row.iter_mut().enumerate() {
            *cell = (0..3).map(|k| a[i][k] * b[k][j]).sum();
        }
    }
    r
}

/// `aᵀ * b`: the rotation from the frame `a` to the frame `b` when both are given in the world.
pub(crate) fn mul_t(a: &Rot, b: &Rot) -> Rot {
    let mut r = [[0.0; 3]; 3];
    for (i, row) in r.iter_mut().enumerate() {
        for (j, cell) in row.iter_mut().enumerate() {
            *cell = (0..3).map(|k| a[k][i] * b[k][j]).sum();
        }
    }
    r
}

pub(crate) fn apply(m: &Rot, v: V3) -> V3 {
    [
        m[0][0] * v[0] + m[0][1] * v[1] + m[0][2] * v[2],
        m[1][0] * v[0] + m[1][1] * v[1] + m[1][2] * v[2],
        m[2][0] * v[0] + m[2][1] * v[1] + m[2][2] * v[2],
    ]
}

/// True when the rotation is the identity to within `tolerance` in every entry.
pub(crate) fn is_identity(m: &Rot, tolerance: f64) -> bool {
    (0..3).all(|i| (0..3).all(|j| (m[i][j] - IDENTITY[i][j]).abs() <= tolerance))
}

pub(crate) fn dot(a: V3, b: V3) -> f64 {
    a[0] * b[0] + a[1] * b[1] + a[2] * b[2]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rotations_compose_and_transpose() {
        let a = about_z(30.0);
        let b = about_z(75.0);
        let ab = mul(&a, &b);
        let c = about_z(105.0);
        for i in 0..3 {
            for j in 0..3 {
                assert!((ab[i][j] - c[i][j]).abs() < 1e-12);
            }
        }
        // aᵀ b = rotation by the difference.
        let d = mul_t(&a, &b);
        let e = about_z(45.0);
        for i in 0..3 {
            for j in 0..3 {
                assert!((d[i][j] - e[i][j]).abs() < 1e-12);
            }
        }
        assert!(is_identity(&mul_t(&a, &a), 1e-12));
        let v = apply(&a, [1.0, 0.0, 0.0]);
        assert!((v[0] - 30f64.to_radians().cos()).abs() < 1e-12 && (v[1] - 0.5).abs() < 1e-12);
    }
}
