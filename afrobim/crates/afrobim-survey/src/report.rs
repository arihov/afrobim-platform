//! Plain text adjustment reports.

use std::fmt::Write;

use crate::adjust::{Adjustment, ObsKind};
use crate::level::LevelAdjustment;

fn kind_label(k: ObsKind) -> &'static str {
    match k {
        ObsKind::Direction => "direction",
        ObsKind::Distance => "distance",
        ObsKind::Azimuth => "azimuth",
        ObsKind::BaselineE => "baseline dE",
        ObsKind::BaselineN => "baseline dN",
        ObsKind::CoordinateE => "coord E",
        ObsKind::CoordinateN => "coord N",
        ObsKind::Orientation => "orientation",
        ObsKind::HeightDifference => "height diff",
        ObsKind::BaselineU => "baseline dU",
        ObsKind::CoordinateZ => "coord Z",
    }
}

fn global(s: &mut String, t: &Option<crate::adjust::GlobalTest>, s0: Option<f64>) {
    match t {
        Some(g) => {
            let _ = writeln!(s, "Global test of the variance factor (chi-square, critical values computed numerically)");
            let _ = writeln!(s, "  statistic v'Pv          {:.4}", g.statistic);
            let _ = writeln!(s, "  degrees of freedom      {}", g.dof);
            let _ = writeln!(s, "  significance level      {} (two sided: limits at alpha/2 and 1 - alpha/2)", g.alpha);
            let _ = writeln!(s, "  limits                  {:.4} to {:.4}", g.lower_critical, g.upper_critical);
            let _ = writeln!(s, "  result                  {}", if g.passes { "within the limits" } else { "OUTSIDE the limits" });
            let _ = writeln!(s, "  one sided critical (1 - alpha)  {:.4}; statistic {} it", g.one_sided_critical, if g.exceeds_one_sided { "exceeds" } else { "does not exceed" });
            if let Some(v) = s0 {
                let _ = writeln!(s, "  a posteriori variance factor  {:.4} (sigma0 = {:.4})", v, v.sqrt());
            }
        }
        None => {
            let _ = writeln!(s, "No redundancy: no global test is possible.");
        }
    }
}

/// Report of a plan adjustment.
pub fn adjustment_report(a: &Adjustment) -> String {
    let mut s = String::new();
    let _ = writeln!(s, "LEAST SQUARES ADJUSTMENT (plan) of network '{}'", a.network);
    let _ = writeln!(s, "Observations {}, unknowns {}, degrees of freedom {}", a.n_observations, a.n_unknowns, a.dof);
    let _ = writeln!(s, "Iterations {} ({}), last step norm dx'N dx = {:e}", a.iterations, if a.converged { "converged" } else { "NOT converged" }, a.last_step_norm);
    let _ = writeln!(s);
    global(&mut s, &a.global_test, a.sigma0_sq_posteriori);
    let _ = writeln!(s);
    let _ = writeln!(
        s,
        "Covariances are {}; error ellipses at {:.1} percent confidence, scale factor {:.4}",
        if a.scaled_by_posteriori { "scaled by the a posteriori variance factor (F distribution factor)" } else { "unscaled (a priori variance factor 1, chi-square factor)" },
        a.ellipse_confidence * 100.0,
        a.ellipse_scale
    );
    let _ = writeln!(s, "{:<10} {:>16} {:>16} {:>8} {:>8} {:>9} {:>9} {:>8}", "Point", "Easting", "Northing", "sE", "sN", "semi-maj", "semi-min", "az maj");
    for p in &a.points {
        let _ = writeln!(
            s,
            "{:<10} {:>16.4} {:>16.4} {:>8.4} {:>8.4} {:>9.4} {:>9.4} {:>8.2}{}",
            p.name,
            p.e,
            p.n,
            p.sigma_e,
            p.sigma_n,
            p.ellipse.semi_major,
            p.ellipse.semi_minor,
            p.ellipse.azimuth_major_deg,
            if p.fixed { "  fixed" } else { "" }
        );
    }
    if !a.orientations.is_empty() {
        let _ = writeln!(s, "\nOrientation unknowns");
        for o in &a.orientations {
            let _ = writeln!(s, "  setup {} at {:<8} {:>12.6} deg  +- {:.2} arcsec", o.setup_id, o.station, o.orientation_deg, o.sigma_arcsec);
        }
    }
    let _ = writeln!(s, "\nObservations (residual = adjusted minus observed; arc seconds for angles, metres otherwise)");
    let _ = writeln!(s, "{:<26} {:<12} {:>12} {:>9} {:>9} {:>7} {:>8}  {}", "Key", "Type", "Residual", "sigma", "sigma(v)", "r", "w", "Flag");
    for o in &a.observations {
        let w = o.std_residual.map(|w| format!("{w:.2}")).unwrap_or_else(|| "none".into());
        let _ = writeln!(s, "{:<26} {:<12} {:>12.4} {:>9.4} {:>9.4} {:>7.3} {:>8}  {}", o.key, kind_label(o.kind), o.residual, o.sigma, o.sigma_residual, o.redundancy, w, if o.flagged { "BLUNDER?" } else { "" });
    }
    let flagged: Vec<_> = a.observations.iter().filter(|o| o.flagged).collect();
    let _ = writeln!(s, "\nBlunder rule: an observation is flagged when |w| > {} with w = residual / sd(residual) (Baarda data snooping). {} flagged.", a.blunder_threshold, flagged.len());
    for o in flagged {
        let _ = writeln!(s, "  {}  w = {:.2}", o.key, o.std_residual.unwrap_or(0.0));
    }
    s
}

pub fn level_report(a: &LevelAdjustment) -> String {
    let mut s = String::new();
    let _ = writeln!(s, "LEAST SQUARES ADJUSTMENT (heights) of network '{}'", a.network);
    let _ = writeln!(s, "Observations {}, unknowns {}, degrees of freedom {}", a.n_observations, a.n_unknowns, a.dof);
    let _ = writeln!(s, "Iterations {} ({})\n", a.iterations, if a.converged { "converged" } else { "NOT converged" });
    global(&mut s, &a.global_test, a.sigma0_sq_posteriori);
    let _ = writeln!(s, "\n{:<10} {:>12} {:>9}", "Point", "Height", "sigma");
    for h in &a.heights {
        let _ = writeln!(s, "{:<10} {:>12.4} {:>9.4}{}", h.name, h.z, h.sigma, if h.fixed { "  fixed" } else { "" });
    }
    let _ = writeln!(s, "\n{:<26} {:<12} {:>10} {:>9} {:>9} {:>7} {:>8}  Flag", "Key", "Type", "Residual", "sigma", "sigma(v)", "r", "w");
    for o in &a.observations {
        let w = o.std_residual.map(|w| format!("{w:.2}")).unwrap_or_else(|| "none".into());
        let _ = writeln!(s, "{:<26} {:<12} {:>10.4} {:>9.4} {:>9.4} {:>7.3} {:>8}  {}", o.key, kind_label(o.kind), o.residual, o.sigma, o.sigma_residual, o.redundancy, w, if o.flagged { "BLUNDER?" } else { "" });
    }
    s
}
