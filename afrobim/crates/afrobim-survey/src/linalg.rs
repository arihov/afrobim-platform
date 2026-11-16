//! Small dense linear algebra: a row-major matrix, Cholesky factorisation of a symmetric positive
//! definite matrix, solution of linear systems and inverse. Written here so that the crate keeps no
//! linear algebra dependency. The factorisation is the textbook Cholesky-Banachiewicz recurrence
//! (any numerical linear algebra text; Golub and Van Loan, *Matrix Computations*, algorithm 4.2.1).

use serde::{Deserialize, Serialize};

use crate::error::{Result, SurveyError};

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Matrix {
    pub rows: usize,
    pub cols: usize,
    pub data: Vec<f64>,
}

impl Matrix {
    pub fn zeros(rows: usize, cols: usize) -> Matrix {
        Matrix { rows, cols, data: vec![0.0; rows * cols] }
    }
    pub fn identity(n: usize) -> Matrix {
        let mut m = Matrix::zeros(n, n);
        for i in 0..n {
            m.data[i * n + i] = 1.0;
        }
        m
    }
    pub fn get(&self, i: usize, j: usize) -> f64 {
        self.data[i * self.cols + j]
    }
    pub fn set(&mut self, i: usize, j: usize, v: f64) {
        self.data[i * self.cols + j] = v;
    }
    pub fn add(&mut self, i: usize, j: usize, v: f64) {
        self.data[i * self.cols + j] += v;
    }
    pub fn mul(&self, o: &Matrix) -> Matrix {
        assert_eq!(self.cols, o.rows);
        let mut r = Matrix::zeros(self.rows, o.cols);
        for i in 0..self.rows {
            for k in 0..self.cols {
                let a = self.get(i, k);
                if a == 0.0 {
                    continue;
                }
                for j in 0..o.cols {
                    r.data[i * o.cols + j] += a * o.get(k, j);
                }
            }
        }
        r
    }
    pub fn transpose(&self) -> Matrix {
        let mut r = Matrix::zeros(self.cols, self.rows);
        for i in 0..self.rows {
            for j in 0..self.cols {
                r.set(j, i, self.get(i, j));
            }
        }
        r
    }
}

/// Lower triangular Cholesky factor L with A = L L'. Fails if a pivot is not positive relative to the
/// matrix scale (singular or not positive definite).
pub fn cholesky(a: &Matrix) -> Result<Matrix> {
    let n = a.rows;
    assert_eq!(a.rows, a.cols);
    let scale = (0..n).map(|i| a.get(i, i).abs()).fold(0.0, f64::max).max(f64::MIN_POSITIVE);
    let mut l = Matrix::zeros(n, n);
    for i in 0..n {
        for j in 0..=i {
            let mut s = a.get(i, j);
            for k in 0..j {
                s -= l.get(i, k) * l.get(j, k);
            }
            if i == j {
                if s <= scale * 1e-13 {
                    return Err(SurveyError::Singular(format!(
                        "unknown number {i} is not determined (pivot {s:e}): the network has a datum defect, a disconnected part or too little data"
                    )));
                }
                l.set(i, i, s.sqrt());
            } else {
                l.set(i, j, s / l.get(j, j));
            }
        }
    }
    Ok(l)
}

/// Solve L L' x = b.
pub fn cholesky_solve(l: &Matrix, b: &[f64]) -> Vec<f64> {
    let n = l.rows;
    let mut y = vec![0.0; n];
    for i in 0..n {
        let mut s = b[i];
        for k in 0..i {
            s -= l.get(i, k) * y[k];
        }
        y[i] = s / l.get(i, i);
    }
    let mut x = vec![0.0; n];
    for i in (0..n).rev() {
        let mut s = y[i];
        for k in i + 1..n {
            s -= l.get(k, i) * x[k];
        }
        x[i] = s / l.get(i, i);
    }
    x
}

/// Inverse of a symmetric positive definite matrix from its Cholesky factor.
pub fn spd_inverse(l: &Matrix) -> Matrix {
    let n = l.rows;
    let mut inv = Matrix::zeros(n, n);
    let mut e = vec![0.0; n];
    for j in 0..n {
        e.iter_mut().for_each(|v| *v = 0.0);
        e[j] = 1.0;
        let col = cholesky_solve(l, &e);
        for i in 0..n {
            inv.set(i, j, col[i]);
        }
    }
    // symmetrise against rounding
    for i in 0..n {
        for j in 0..i {
            let m = 0.5 * (inv.get(i, j) + inv.get(j, i));
            inv.set(i, j, m);
            inv.set(j, i, m);
        }
    }
    inv
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn solve_and_inverse() {
        let mut a = Matrix::zeros(3, 3);
        let vals = [4.0, 1.0, 0.5, 1.0, 3.0, 0.2, 0.5, 0.2, 2.0];
        a.data.copy_from_slice(&vals);
        let l = cholesky(&a).unwrap();
        let x = cholesky_solve(&l, &[1.0, 2.0, 3.0]);
        for i in 0..3 {
            let r: f64 = (0..3).map(|j| a.get(i, j) * x[j]).sum();
            assert!((r - [1.0, 2.0, 3.0][i]).abs() < 1e-13);
        }
        let inv = spd_inverse(&l);
        let id = a.mul(&inv);
        for i in 0..3 {
            for j in 0..3 {
                assert!((id.get(i, j) - if i == j { 1.0 } else { 0.0 }).abs() < 1e-13);
            }
        }
    }

    #[test]
    fn singular_is_reported() {
        let mut a = Matrix::zeros(2, 2);
        a.data.copy_from_slice(&[1.0, 1.0, 1.0, 1.0]);
        assert!(matches!(cholesky(&a), Err(SurveyError::Singular(_))));
    }
}
