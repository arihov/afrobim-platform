// SPDX-License-Identifier: LicenseRef-AfroBIM-Proprietary
// Copyright (c) 2026 AfroBIM. All rights reserved.
//! The alignment design check of an imported alignment. An imported alignment has no PI design, so
//! the engine's solutions (curves, tangents, grades, vertical curves) are worked out from its
//! elements and its profile, and the limits are one row of the defaults pack at a design speed
//! the engineer chooses. A limit the pack does not hold at that speed is not borrowed from
//! another: the check reads "not checked" and says which key is missing. With no design speed
//! chosen nothing is checked and the message says so.

use afrobim_alignment::{Alignment, Turn, VerticalCurve};
use afrobim_defaults::Conditions;
use afrobim_design::{self as design, CheckReport, CurveData, CurveKind, DesignBasis, GradeData, HorizontalSolution, StandardRow, TangentData, VCurveData, VerticalSolution};

use crate::defaults_glue as dg;

/// The pack keys of the check, with the field of the row each fills.
pub const KEYS: [(&str, &str); 4] = [
    ("min_radius_m", "link.horizontal.desirable_min_radius_superelevation_5"),
    ("stopping_sight_distance_m", "link.ssd.desirable_min"),
    ("min_k_crest", "link.vertical.crest_k_desirable_min"),
    ("min_k_sag", "link.vertical.sag_k_desirable_min"),
];

/// The row of limits at a design speed, from the pack, and what it could not give.
pub fn pack_row(speed_kmh: f64) -> (StandardRow, Vec<String>) {
    let cond = Conditions::none().design_speed(speed_kmh);
    let mut row = StandardRow { label: format!("{speed_kmh} km/h"), design_speed_kmh: Some(speed_kmh), ..Default::default() };
    let mut missing = Vec::new();
    for (field, key) in KEYS {
        match dg::get(key, &cond) {
            Some(v) => {
                let n = v.number();
                match field {
                    "min_radius_m" => row.min_radius_m = n,
                    "stopping_sight_distance_m" => row.stopping_sight_distance_m = n,
                    "min_k_crest" => row.min_k_crest = n,
                    _ => row.min_k_sag = n,
                }
                row.limit_sources.insert(field.to_string(), dg::citation_line(&v));
            }
            None => {
                missing.push(format!("{key} has no value at {speed_kmh} km/h in the {} pack, so that limit is not checked", dg::basis_name()));
                row.limit_sources.insert(field.to_string(), format!("not in the pack at {speed_kmh} km/h"));
            }
        }
    }
    (row, missing)
}

/// The curves and tangents of an alignment's elements, as the engine's horizontal solution.
pub fn horizontal_of(a: &Alignment) -> HorizontalSolution {
    let stations = a.element_stations();
    let els = &a.elements;
    let kind = |i: usize| {
        let e = &els[i];
        let (k0, k1) = (e.curvature_start, e.curvature_end);
        if k0.abs() < 1e-12 && k1.abs() < 1e-12 {
            0 // tangent
        } else if (k0 - k1).abs() < 1e-12 {
            1 // arc
        } else {
            2 // spiral
        }
    };
    let blank = |pi: usize, turn: Turn, radius: f64| CurveData {
        pi_index: pi,
        turn,
        radius_m: radius,
        spiral_in_m: 0.0,
        spiral_out_m: 0.0,
        deflection_rad: 0.0,
        theta_in_rad: 0.0,
        theta_out_rad: 0.0,
        shift_in_m: 0.0,
        shift_out_m: 0.0,
        extension_in_m: 0.0,
        extension_out_m: 0.0,
        spiral_in_xy: (0.0, 0.0),
        spiral_out_xy: (0.0, 0.0),
        tangent_in_m: 0.0,
        tangent_out_m: 0.0,
        arc_length_m: 0.0,
        total_length_m: 0.0,
        external_m: 0.0,
        mid_ordinate_m: 0.0,
        ts_station_m: 0.0,
        sc_station_m: 0.0,
        cs_station_m: 0.0,
        st_station_m: 0.0,
        pi_station_m: 0.0,
        ts: (0.0, 0.0),
        sc: (0.0, 0.0),
        cs: (0.0, 0.0),
        st: (0.0, 0.0),
        centre: (0.0, 0.0),
    };
    let mut curves: Vec<CurveData> = Vec::new();
    let mut i = 0;
    while i < els.len() {
        if kind(i) == 1 {
            let k = els[i].curvature_start;
            let mut c = blank(curves.len(), if k > 0.0 { Turn::Left } else { Turn::Right }, 1.0 / k.abs());
            c.arc_length_m = els[i].length;
            c.ts_station_m = stations[i];
            c.st_station_m = stations[i] + els[i].length;
            c.total_length_m = els[i].length;
            if i > 0 && kind(i - 1) == 2 {
                c.spiral_in_m = els[i - 1].length;
                c.ts_station_m = stations[i - 1];
                c.total_length_m += els[i - 1].length;
            }
            if i + 1 < els.len() && kind(i + 1) == 2 {
                c.spiral_out_m = els[i + 1].length;
                c.st_station_m = stations[i + 1] + els[i + 1].length;
                c.total_length_m += els[i + 1].length;
            }
            c.sc_station_m = stations[i];
            c.cs_station_m = stations[i] + els[i].length;
            curves.push(c);
        }
        i += 1;
    }
    let tangents: Vec<TangentData> = curves
        .windows(2)
        .enumerate()
        .map(|(n, w)| TangentData { from_pi: n, straight_m: (w[1].ts_station_m - w[0].st_station_m).max(0.0), leg_m: (w[1].ts_station_m - w[0].st_station_m).max(0.0), reverse: Some(w[0].turn != w[1].turn), start_station_m: w[0].st_station_m })
        .collect();
    HorizontalSolution { alignment: a.clone(), curves, tangents, warnings: Vec::new() }
}

/// The grades and vertical curves of an alignment's profile, as the engine's vertical solution.
pub fn vertical_of(a: &Alignment) -> Option<VerticalSolution> {
    let profile = a.profile.clone()?;
    let extents = profile.curve_extents().ok()?;
    let mut grades = Vec::new();
    for i in 0..profile.pvis.len().saturating_sub(1) {
        let g = profile.grade_between(i)?;
        let tangent = profile.pvis[i + 1].station_m - profile.pvis[i].station_m;
        grades.push(GradeData { from_pvi: i, grade: g, start_station_m: profile.pvis[i].station_m, end_station_m: profile.pvis[i + 1].station_m, tangent_length_m: tangent });
    }
    let mut curves = Vec::new();
    for ex in extents {
        let (p, i) = (&profile.pvis[ex.pvi], ex.pvi);
        if i == 0 || i + 1 >= profile.pvis.len() || !matches!(p.curve, VerticalCurve::Parabola { .. }) {
            continue;
        }
        let (g_in, g_out) = (profile.grade_between(i - 1)?, profile.grade_between(i)?);
        let change = g_out - g_in;
        if change.abs() < 1e-12 {
            continue;
        }
        let length = ex.end_station_m - ex.start_station_m;
        let at = |s: f64| profile.elevation_at(s).unwrap_or(p.elevation_m);
        curves.push(VCurveData {
            pvi_index: i,
            kind: if change < 0.0 { CurveKind::Crest } else { CurveKind::Sag },
            grade_in: g_in,
            grade_out: g_out,
            grade_change: change.abs(),
            length_m: length,
            k: length / (100.0 * change.abs()),
            pvi_station_m: p.station_m,
            pvi_elevation_m: p.elevation_m,
            pvc_station_m: ex.start_station_m,
            pvc_elevation_m: at(ex.start_station_m),
            pvt_station_m: ex.end_station_m,
            pvt_elevation_m: at(ex.end_station_m),
            offset_at_pvi_m: change.abs() * length / 8.0,
            turning_point: None,
        });
    }
    Some(VerticalSolution { profile, grades, curves, warnings: Vec::new() })
}

/// The check of an imported alignment at a design speed: the report, and the lines that say what
/// could not be checked and why. No speed gives no report.
pub fn check(a: &Alignment, speed_kmh: Option<f64>) -> (Option<CheckReport>, Vec<String>) {
    let Some(speed) = speed_kmh.filter(|s| s.is_finite() && *s > 0.0) else {
        return (None, vec![format!("{}: no design speed is set, so nothing is checked. Choose one to check it against the {} pack.", a.name, dg::basis_name())]);
    };
    if dg::basis_id() == dg::NO_BASIS {
        return (None, vec![format!("{}: no defaults basis is chosen, so no limit can be applied. Choose the DMRB pack in the defaults window.", a.name)]);
    }
    let (row, mut notes) = pack_row(speed);
    let h = horizontal_of(a);
    let v = vertical_of(a);
    let basis = DesignBasis { name: format!("{} defaults pack", dg::basis_name()), source: format!("the {} defaults pack, one row at {speed} km/h", dg::basis_name()), notes: String::new(), rows: vec![row.clone()] };
    let report = design::run_checks(&basis, &row, Some(&h), v.as_ref(), &design::CheckOptions::default());
    if v.is_none() {
        notes.push(format!("{}: it has no vertical profile, so grades and vertical curves are not checked.", a.name));
    }
    (Some(report), notes)
}

/// A one-line account of a report: how many items passed, failed and could not be checked.
pub fn summary(report: &CheckReport) -> String {
    use design::CheckStatus::*;
    format!("{} item(s): {} pass, {} fail, {} not checked", report.items.len(), report.count(Pass), report.count(Fail), report.count(NotChecked))
}
