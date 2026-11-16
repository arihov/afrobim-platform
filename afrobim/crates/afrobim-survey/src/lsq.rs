//! Weighted nonlinear least squares by Gauss-Newton iteration, with the quality measures of the
//! adjustment.
//!
//! Method and source: Teunissen, *Network Quality Control*, TU Delft OPEN Publishing 2024, doi
//! 10.59490/tb.100.
//!
//! * Linearisation (section 2.4.2, eq. 2.45): y = f(x0) + A dx, with A the matrix of partial
//!   derivatives at the approximation x0.
//! * Iteration (section 2.4.3, Figure 2.7): N = A' W A, r = A' W (y - f(x)), N dx = r, x := x + dx,
//!   stop when ||dx||^2_N = dx' N dx <= delta (delta is an input). The weight matrix here is diagonal,
//!   W = diag(1/sigma_i^2), the observations being uncorrelated.
//! * Precision (section 2.7, eq. 2.94): Q_x = (A' W A)^-1.
//! * Residuals and tests (sections 3.3.2 and 3.4): the residual vector has covariance
//!   Q_e = Q_y - A Q_x A'. For uncorrelated observations the w-test statistic of observation i
//!   (eq. 3.49) reduces to w_i = e_i / sigma_{e_i} with sigma_{e_i}^2 = sigma_i^2 - a_i Q_x a_i'
//!   (a_i the i-th row of A). The overall model test statistic is T = e' W e (eq. 3.43), chi-square
//!   with m - n degrees of freedom under the null hypothesis (eq. 3.44).
//! * Redundancy number r_i = sigma_{e_i}^2 / sigma_i^2 = 1 - w_i a_i Q_x a_i'; the sum of the r_i equals
//!   m - n.
//!
//! The normal matrix is scaled by its diagonal before the Cholesky factorisation (Jacobi scaling) so
//! that unknowns of very different units (metres and radians) do not upset the singularity test.

use crate::error::{Result, SurveyError};
use crate::linalg::{cholesky, cholesky_solve, spd_inverse, Matrix};

/// One linearised observation equation: `l = y - f(x)` and the nonzero entries of its row of A.
#[derive(Debug, Clone)]
pub struct Row {
    pub l: f64,
    pub cols: Vec<(usize, f64)>,
}

#[derive(Debug, Clone)]
pub struct Solution {
    pub x: Vec<f64>,
    pub iterations: usize,
    pub converged: bool,
    /// dx' N dx of the last step.
    pub last_step_norm: f64,
    /// Q_x = N^-1 (a priori, unit variance factor).
    pub qx: Matrix,
    /// Residuals v = f(x_hat) - y.
    pub v: Vec<f64>,
    pub redundancy: Vec<f64>,
    /// Standard deviation of each residual.
    pub sigma_v: Vec<f64>,
    /// w-test statistics; `None` where the redundancy number is below `MIN_REDUNDANCY` (the
    /// observation is not controlled by the others).
    pub w: Vec<Option<f64>>,
    pub vtpv: f64,
    pub dof: isize,
}

/// Below this redundancy number an observation is treated as uncontrolled and gets no w statistic.
pub const MIN_REDUNDANCY: f64 = 1e-6;

fn normal_system(rows: &[Row], sigmas: &[f64], n: usize) -> (Matrix, Vec<f64>) {
    let mut nm = Matrix::zeros(n, n);
    let mut r = vec![0.0; n];
    for (row, &s) in rows.iter().zip(sigmas) {
        let w = 1.0 / (s * s);
        for &(i, a) in &row.cols {
            r[i] += a * w * row.l;
            for &(j, b) in &row.cols {
                nm.add(i, j, a * w * b);
            }
        }
    }
    (nm, r)
}

fn solve_scaled(nm: &Matrix, rhs: &[f64]) -> Result<(Vec<f64>, Matrix)> {
    let n = nm.rows;
    let d: Vec<f64> = (0..n).map(|i| 1.0 / nm.get(i, i).max(f64::MIN_POSITIVE).sqrt()).collect();
    let mut sc = Matrix::zeros(n, n);
    for i in 0..n {
        for j in 0..n {
            sc.set(i, j, nm.get(i, j) * d[i] * d[j]);
        }
    }
    let l = cholesky(&sc)?;
    let b: Vec<f64> = (0..n).map(|i| rhs[i] * d[i]).collect();
    let y = cholesky_solve(&l, &b);
    let x: Vec<f64> = (0..n).map(|i| y[i] * d[i]).collect();
    let inv = spd_inverse(&l);
    let mut q = Matrix::zeros(n, n);
    for i in 0..n {
        for j in 0..n {
            q.set(i, j, inv.get(i, j) * d[i] * d[j]);
        }
    }
    Ok((x, q))
}

pub fn solve(x0: Vec<f64>, sigmas: &[f64], linearize: &mut dyn FnMut(&[f64]) -> Result<Vec<Row>>, max_iterations: usize, delta: f64) -> Result<Solution> {
    let n = x0.len();
    let m = sigmas.len();
    if n == 0 {
        return Err(SurveyError::Invalid("the adjustment has no unknowns".into()));
    }
    if sigmas.iter().any(|s| !(*s > 0.0) || !s.is_finite()) {
        return Err(SurveyError::Invalid("every observation needs a positive standard error".into()));
    }
    if m < n {
        return Err(SurveyError::Singular(format!("{m} observations for {n} unknowns")));
    }
    let mut x = x0;
    let mut iterations = 0;
    let mut converged = false;
    let mut last = f64::INFINITY;
    for _ in 0..max_iterations.max(1) {
        let rows = linearize(&x)?;
        if rows.len() != m {
            return Err(SurveyError::Invalid("internal: observation count changed during iteration".into()));
        }
        let (nm, r) = normal_system(&rows, sigmas, n);
        let (dx, _q) = solve_scaled(&nm, &r)?;
        for i in 0..n {
            x[i] += dx[i];
        }
        iterations += 1;
        last = (0..n).map(|i| dx[i] * r[i]).sum::<f64>();
        if last <= delta {
            converged = true;
            break;
        }
    }
    // final linearisation at the solution
    let rows = linearize(&x)?;
    let (nm, _) = normal_system(&rows, sigmas, n);
    let (_, qx) = solve_scaled(&nm, &vec![0.0; n])?;
    let mut v = Vec::with_capacity(m);
    let mut red = Vec::with_capacity(m);
    let mut sv = Vec::with_capacity(m);
    let mut w = Vec::with_capacity(m);
    let mut vtpv = 0.0;
    for (row, &s) in rows.iter().zip(sigmas) {
        let vi = -row.l;
        let mut aqa = 0.0;
        for &(i, a) in &row.cols {
            for &(j, b) in &row.cols {
                aqa += a * qx.get(i, j) * b;
            }
        }
        let ri = 1.0 - aqa / (s * s);
        let sig_v = (s * s * ri).max(0.0).sqrt();
        vtpv += vi * vi / (s * s);
        v.push(vi);
        red.push(ri);
        sv.push(sig_v);
        w.push(if ri > MIN_REDUNDANCY { Some(vi / sig_v) } else { None });
    }
    Ok(Solution { x, iterations, converged, last_step_norm: last, qx, v, redundancy: red, sigma_v: sv, w, vtpv, dof: m as isize - n as isize })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn linear_line_fit_matches_normal_equations() {
        // y = a + b t, t = 0..4, y = 1, 3.1, 4.9, 7.2, 8.8, equal weights sigma = 0.1
        let t = [0.0, 1.0, 2.0, 3.0, 4.0];
        let y = [1.0, 3.1, 4.9, 7.2, 8.8];
        let sig = vec![0.1; 5];
        let mut lin = |x: &[f64]| -> Result<Vec<Row>> { Ok((0..5).map(|i| Row { l: y[i] - (x[0] + x[1] * t[i]), cols: vec![(0, 1.0), (1, t[i])] }).collect()) };
        let s = solve(vec![0.0, 0.0], &sig, &mut lin, 5, 1e-20).unwrap();
        // closed form: b = Sxy/Sxx, a = ybar - b tbar
        let tb = 2.0;
        let yb: f64 = y.iter().sum::<f64>() / 5.0;
        let sxx: f64 = t.iter().map(|x| (x - tb).powi(2)).sum();
        let sxy: f64 = t.iter().zip(&y).map(|(x, yy)| (x - tb) * (yy - yb)).sum();
        let b = sxy / sxx;
        let a = yb - b * tb;
        assert!((s.x[0] - a).abs() < 1e-10 && (s.x[1] - b).abs() < 1e-10);
        assert_eq!(s.dof, 3);
        assert!((s.redundancy.iter().sum::<f64>() - 3.0).abs() < 1e-9, "sum of redundancy numbers equals the degrees of freedom");
        // var(b) = sigma^2 / Sxx
        assert!((s.qx.get(1, 1) - 0.01 / sxx).abs() < 1e-12);
        assert!(s.converged);
    }
}
