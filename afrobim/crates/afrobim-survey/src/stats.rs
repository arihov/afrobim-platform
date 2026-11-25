// SPDX-License-Identifier: LicenseRef-AfroBIM-Proprietary
// Copyright (c) 2026 AfroBIM. All rights reserved.
//! Statistical distributions computed numerically (no typed tables).
//!
//! * `ln_gamma`: Stirling series (DLMF section 5.11, eq. 5.11.1: ln G(z) ~ (z - 1/2) ln z - z +
//!   (1/2) ln 2pi + sum B_2k / (2k (2k-1) z^(2k-1))) after shifting the argument up with
//!   G(z) = G(z + n) / (z (z+1) ... (z+n-1)). The Bernoulli numbers B2 = 1/6, B4 = -1/30, B6 = 1/42,
//!   B8 = -1/30, B10 = 5/66 are mathematical constants.
//! * `gamma_p`: regularised lower incomplete gamma P(a, x). For x < a + 1 the power series DLMF 8.7.1,
//!   gamma*(a, z) = e^-z sum_k z^k / G(a + k + 1), with P(a, x) = x^a gamma*(a, x). Otherwise
//!   Q(a, x) = 1 - P(a, x) from the continued fraction of DLMF 8.9.2 in its even contraction (Jones and
//!   Thron), evaluated with the modified Lentz algorithm (Press et al., *Numerical Recipes*, the
//!   chapter on the incomplete gamma function).
//! * `chi2_cdf(x, k) = P(k/2, x/2)` (the chi-square distribution is the gamma distribution with shape
//!   k/2 and scale 2). `chi2_quantile` inverts it by bracketing and bisection, so the critical value is
//!   computed, never read from a table. Teunissen, *Network Quality Control* (TU Delft OPEN, 2024,
//!   doi 10.59490/tb.100), eq. 3.43 and 3.44, uses this distribution for the overall model test.
//!
//! The tests check the functions against closed forms (chi-square with 2 and 4 degrees of freedom)
//! and against direct numerical integration of the density (1 and 3 degrees of freedom).

const SQRT_2PI_LN: f64 = 0.918_938_533_204_672_7; // ln(sqrt(2 pi)), a mathematical constant

/// Natural log of the gamma function for x > 0.
pub fn ln_gamma(x: f64) -> f64 {
    assert!(x > 0.0, "ln_gamma needs x > 0");
    let mut z = x;
    let mut shift = 0.0;
    while z < 20.0 {
        shift += z.ln();
        z += 1.0;
    }
    let z2 = z * z;
    let series = 1.0 / (12.0 * z) - 1.0 / (360.0 * z * z2) + 1.0 / (1260.0 * z * z2 * z2) - 1.0 / (1680.0 * z * z2 * z2 * z2)
        + 1.0 / (1188.0 * z * z2 * z2 * z2 * z2);
    (z - 0.5) * z.ln() - z + SQRT_2PI_LN + series - shift
}

/// Regularised lower incomplete gamma function P(a, x), a > 0, x >= 0.
pub fn gamma_p(a: f64, x: f64) -> f64 {
    if x <= 0.0 {
        return 0.0;
    }
    if x < a + 1.0 {
        // series: P = exp(a ln x - x - ln G(a+1)) * sum x^k / ((a+1)...(a+k))
        let mut term = 1.0 / a;
        let mut sum = term;
        let mut ap = a;
        for _ in 0..10_000 {
            ap += 1.0;
            term *= x / ap;
            sum += term;
            if term.abs() < sum.abs() * 1e-17 {
                break;
            }
        }
        (sum.ln() + a * x.ln() - x - ln_gamma(a)).exp()
    } else {
        1.0 - gamma_q_cf(a, x)
    }
}

fn gamma_q_cf(a: f64, x: f64) -> f64 {
    let tiny = 1e-300;
    let mut b = x + 1.0 - a;
    let mut c = 1.0 / tiny;
    let mut d = 1.0 / b;
    let mut h = d;
    for i in 1..10_000 {
        let an = -(i as f64) * (i as f64 - a);
        b += 2.0;
        d = an * d + b;
        if d.abs() < tiny {
            d = tiny;
        }
        c = b + an / c;
        if c.abs() < tiny {
            c = tiny;
        }
        d = 1.0 / d;
        let del = d * c;
        h *= del;
        if (del - 1.0).abs() < 1e-16 {
            break;
        }
    }
    (-x + a * x.ln() - ln_gamma(a)).exp() * h
}

/// Chi-square cumulative distribution, k degrees of freedom (k > 0).
pub fn chi2_cdf(x: f64, k: f64) -> f64 {
    if x <= 0.0 {
        0.0
    } else {
        gamma_p(k / 2.0, x / 2.0)
    }
}

/// Chi-square quantile: the x with cdf(x) = p, 0 < p < 1, by bracketing and bisection.
pub fn chi2_quantile(p: f64, k: f64) -> f64 {
    assert!(p > 0.0 && p < 1.0 && k > 0.0, "chi2_quantile needs 0 < p < 1 and k > 0");
    let mut lo = 0.0;
    let mut hi = k.max(1.0);
    while chi2_cdf(hi, k) < p {
        lo = hi;
        hi *= 2.0;
        if hi > 1e12 {
            break;
        }
    }
    for _ in 0..200 {
        let mid = 0.5 * (lo + hi);
        if chi2_cdf(mid, k) < p {
            lo = mid;
        } else {
            hi = mid;
        }
        if hi - lo <= 1e-14 * hi.max(1.0) {
            break;
        }
    }
    0.5 * (lo + hi)
}

/// Two sided critical value of the standard normal at level alpha: z with P(|Z| > z) = alpha.
/// Since Z^2 is chi-square with one degree of freedom, z = sqrt(chi2_quantile(1 - alpha, 1)).
pub fn normal_two_sided_critical(alpha: f64) -> f64 {
    chi2_quantile(1.0 - alpha, 1.0).sqrt()
}

/// Scale factor k of the confidence ellipse of a bivariate normal with KNOWN covariance Q at
/// probability p: the quadratic form (x - mu)' Q^-1 (x - mu) is chi-square with 2 degrees of freedom,
/// whose distribution function is 1 - exp(-t/2) (closed form, the density is exp(-t/2)/2), so the
/// ellipse (x - mu)' Q^-1 (x - mu) = t_p has t_p = -2 ln(1 - p) and k = sqrt(t_p). Semi-axes are k
/// sqrt(lambda_i) for the eigenvalues lambda_i of Q. MIT 12.215 lecture 14 gives the same factor.
pub fn ellipse_scale_known_variance(p: f64) -> f64 {
    (-2.0 * (1.0 - p).ln()).sqrt()
}

/// The same factor when the covariance is Q_hat = s0_hat^2 Q with the variance factor estimated from r
/// degrees of freedom: (x - mu)' Q_hat^-1 (x - mu) / 2 follows the F distribution with (2, r) degrees
/// of freedom. For 2 numerator degrees of freedom the F distribution function has the closed form
/// 1 - (1 + 2 f / r)^(-r/2) (integrate the density), so f_p = (r/2) ((1 - p)^(-2/r) - 1) and the
/// ellipse is (x - mu)' Q_hat^-1 (x - mu) = 2 f_p, k = sqrt(2 f_p). As r grows k tends to the known
/// variance value.
pub fn ellipse_scale_estimated_variance(p: f64, r: f64) -> f64 {
    (r * ((1.0 - p).powf(-2.0 / r) - 1.0)).sqrt()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn gamma_closed_forms() {
        assert!((ln_gamma(5.0) - 24f64.ln()).abs() < 1e-13);
        assert!((ln_gamma(10.0) - 362_880f64.ln()).abs() < 1e-12);
        assert!((ln_gamma(0.5) - std::f64::consts::PI.sqrt().ln()).abs() < 1e-13);
        assert!((ln_gamma(1.0)).abs() < 1e-13);
    }

    #[test]
    fn chi2_even_dof_closed_forms() {
        // k = 2: 1 - e^(-x/2); k = 4: 1 - e^(-x/2) (1 + x/2); k = 6: 1 - e^(-x/2)(1 + x/2 + x^2/8)
        for &x in &[0.1f64, 0.9, 2.0, 5.99, 7.8, 15.0, 40.0] {
            let c2 = 1.0 - (-x / 2.0).exp();
            let c4 = 1.0 - (-x / 2.0).exp() * (1.0 + x / 2.0);
            let c6 = 1.0 - (-x / 2.0).exp() * (1.0 + x / 2.0 + x * x / 8.0);
            assert!((chi2_cdf(x, 2.0) - c2).abs() < 1e-13, "k=2 x={x}");
            assert!((chi2_cdf(x, 4.0) - c4).abs() < 1e-13, "k=4 x={x}");
            assert!((chi2_cdf(x, 6.0) - c6).abs() < 1e-13, "k=6 x={x}");
        }
    }

    /// Simpson integration of the chi-square density, odd degrees of freedom (no closed form without erf).
    fn simpson_cdf(x: f64, k: f64) -> f64 {
        // substitute t = u^2 to remove the singularity of the density at 0 for k = 1:
        // F(x) = integral_0^sqrt(x) 2u f(u^2) du with f(t) = t^(k/2-1) e^(-t/2) / (2^(k/2) G(k/2))
        let n = 200_000;
        let top = x.sqrt();
        let h = top / n as f64;
        let f = |u: f64| {
            if u == 0.0 {
                return if k == 1.0 { 2.0 / ((2f64).sqrt() * std::f64::consts::PI.sqrt()) } else { 0.0 };
            }
            let t = u * u;
            2.0 * u * (((k / 2.0 - 1.0) * t.ln() - t / 2.0) - (k / 2.0) * 2f64.ln() - ln_gamma(k / 2.0)).exp()
        };
        let mut s = f(0.0) + f(top);
        for i in 1..n {
            s += f(i as f64 * h) * if i % 2 == 1 { 4.0 } else { 2.0 };
        }
        s * h / 3.0
    }

    #[test]
    fn chi2_odd_dof_against_integration() {
        for &(x, k) in &[(0.5, 1.0), (3.84, 1.0), (9.0, 1.0), (1.0, 3.0), (7.81, 3.0), (20.0, 3.0), (6.0, 5.0)] {
            assert!((chi2_cdf(x, k) - simpson_cdf(x, k)).abs() < 1e-8, "x={x} k={k}");
        }
    }

    #[test]
    fn quantile_roundtrip_and_known_closed_form() {
        // k = 2: quantile = -2 ln(1 - p), exact
        for &p in &[0.5, 0.9, 0.95, 0.99] {
            assert!((chi2_quantile(p, 2.0) - (-2.0 * (1.0 - p).ln())).abs() < 1e-10);
        }
        for &k in &[1.0, 3.0, 7.0, 25.0, 120.0] {
            for &p in &[0.025, 0.5, 0.975] {
                let q = chi2_quantile(p, k);
                assert!((chi2_cdf(q, k) - p).abs() < 1e-11, "k={k} p={p}");
            }
        }
        // normal two sided: P(|Z| > z) = alpha, check by integrating the normal density
        let z = normal_two_sided_critical(0.05);
        let n = 100_000;
        let h = z / n as f64;
        let dens = |t: f64| (-t * t / 2.0).exp() / (2.0 * std::f64::consts::PI).sqrt();
        let mut s = dens(0.0) + dens(z);
        for i in 1..n {
            s += dens(i as f64 * h) * if i % 2 == 1 { 4.0 } else { 2.0 };
        }
        let central = 2.0 * s * h / 3.0;
        assert!((central - 0.95).abs() < 1e-9);
    }

    #[test]
    fn ellipse_scales() {
        let k = ellipse_scale_known_variance(0.95);
        assert!((chi2_cdf(k * k, 2.0) - 0.95).abs() < 1e-12);
        // F closed form against numerical definition: cdf_F(2, r)(f) = 1 - (1 + 2f/r)^(-r/2); big r tends to chi2 / 2
        let ke = ellipse_scale_estimated_variance(0.95, 1.0e7);
        assert!((ke - k).abs() < 1e-5);
        assert!(ellipse_scale_estimated_variance(0.95, 5.0) > k);
    }
}
