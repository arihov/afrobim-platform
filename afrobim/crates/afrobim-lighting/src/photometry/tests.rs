// SPDX-License-Identifier: LicenseRef-AfroBIM-Proprietary
// Copyright (c) 2026 AfroBIM. All rights reserved.
use super::testdata::{ies, ies_simple, ldt};
use super::*;

fn close(a: f64, b: f64, tol: f64) -> bool {
    (a - b).abs() <= tol * b.abs().max(1.0)
}

fn row(n: usize, v: f64) -> Vec<f64> {
    vec![v; n]
}

#[test]
fn ies_axisymmetric_isotropic_applies_the_multiplier_and_integrates_to_4_pi_i() {
    let vert: Vec<f64> = (0..=12).map(|k| k as f64 * 15.0).collect();
    let text = ies("IESNA:LM-63-2002", "TILT=NONE", 2.0, 1500.0, 2.0, 1, 2, [0.2, 0.4, 0.1], [1.0, 1.0, 50.0], &vert, &[0.0], &[row(13, 100.0)]);
    let p = parse_ies(&text, IesOptions::default()).unwrap();
    assert_eq!(p.c_deg, vec![0.0]);
    assert!(p.candela.iter().all(|v| *v == 200.0), "100 cd times a multiplier of 2");
    assert_eq!(p.lamp_flux_lm, Some(3000.0));
    assert!(!p.absolute_photometry);
    assert_eq!(p.manufacturer, "Test maker");
    assert_eq!(p.catalogue, "CAT-1");
    assert_eq!(p.dimensions_m, Some([0.4, 0.2, 0.1]), "length, width, height in metres");
    assert_eq!(p.input_power_w, Some(50.0));
    // Uniform source: the solid-angle weights make the integral exact whatever the step.
    for step in [1.0, 5.0, 10.0] {
        let flux = p.integrated_flux_lm(step);
        assert!(close(flux, 4.0 * std::f64::consts::PI * 200.0, 1e-12), "step {step}: {flux}");
    }
    let check = p.flux_check(5.0);
    assert!(close(check.downward_fraction, 0.5, 1e-12) && close(check.upward_fraction, 0.5, 1e-12));
    assert!(close(check.computed_lor_percent.unwrap(), 100.0 * 4.0 * std::f64::consts::PI * 200.0 / 3000.0, 1e-12));
    // The difference is reported against the stated flux (a light output ratio of 100 %).
    assert!(close(check.difference_lm.unwrap(), check.integrated_flux_lm - 3000.0, 1e-12));
}

#[test]
fn ies_feet_and_absolute_photometry_and_ballast() {
    let vert = [0.0, 90.0];
    let text = ies("IESNA:LM-63-2002", "TILT=NONE", 1.0, -1.0, 1.0, 1, 1, [1.0, 2.0, 3.0], [0.9, 1.0, 40.0], &vert, &[0.0], &[vec![100.0, 50.0]]);
    let p = parse_ies(&text, IesOptions::default()).unwrap();
    assert!(p.absolute_photometry && p.lamp_flux_lm.is_none());
    assert!(close(p.dimensions_m.unwrap()[0], 2.0 * 0.3048, 1e-12), "feet to metres");
    assert_eq!(p.candela, vec![100.0, 50.0], "ballast factor not applied by default");
    assert!(p.warnings.iter().any(|w| w.contains("ballast factor is 0.9")));
    let q = parse_ies(&text, IesOptions { apply_ballast_factor: true }).unwrap();
    assert!(close(q.candela[0], 90.0, 1e-12) && close(q.candela[1], 45.0, 1e-12));
    assert_eq!(q.ballast_factor, Some(0.9));
}

#[test]
fn ies_symmetry_cases_are_expanded_to_the_full_turn() {
    // Quadrants: horizontal 0, 45, 90.
    let vert = [0.0, 90.0];
    let p = parse_ies(&ies_simple(&[0.0, 45.0, 90.0], &vert, &[vec![10.0, 1.0], vec![20.0, 2.0], vec![30.0, 3.0]]), IesOptions::default()).unwrap();
    for (c, expect) in [(0.0, 10.0), (45.0, 20.0), (90.0, 30.0), (135.0, 20.0), (180.0, 10.0), (225.0, 20.0), (270.0, 30.0), (315.0, 20.0)] {
        assert!(close(p.intensity(c, 0.0), expect, 1e-12), "C{c}");
    }
    assert!(p.symmetry.contains("both") || p.symmetry.contains("0-180 and 90-270"));
    // Bilateral about 0-180: C270 = C90.
    let p = parse_ies(&ies_simple(&[0.0, 90.0, 180.0], &vert, &[vec![10.0, 0.0], vec![20.0, 0.0], vec![30.0, 0.0]]), IesOptions::default()).unwrap();
    assert!(close(p.intensity(270.0, 0.0), 20.0, 1e-12) && close(p.intensity(300.0, 0.0), p.intensity(60.0, 0.0), 1e-12));
    // Bilateral about 90-270 (horizontal angles 90 to 270): C0 mirrors C180, C45 mirrors C135.
    let p = parse_ies(&ies_simple(&[90.0, 180.0, 270.0], &vert, &[vec![10.0, 0.0], vec![20.0, 0.0], vec![30.0, 0.0]]), IesOptions::default()).unwrap();
    assert!(close(p.intensity(0.0, 0.0), 20.0, 1e-12) && close(p.intensity(180.0, 0.0), 20.0, 1e-12));
    assert!(close(p.intensity(135.0, 0.0), p.intensity(45.0, 0.0), 1e-12));
    assert!(close(p.intensity(90.0, 0.0), 10.0, 1e-12) && close(p.intensity(270.0, 0.0), 30.0, 1e-12));
    // No symmetry, 0 to 345: a 352.5 degree direction is half way between C345 and C0 (wraps).
    let horiz: Vec<f64> = (0..24).map(|k| k as f64 * 15.0).collect();
    let rows: Vec<Vec<f64>> = (0..24).map(|k| vec![k as f64, 0.0]).collect();
    let p = parse_ies(&ies_simple(&horiz, &vert, &rows), IesOptions::default()).unwrap();
    assert!(close(p.intensity(352.5, 0.0), 0.5 * (23.0 + 0.0), 1e-12));
    assert!(close(p.intensity(7.5, 0.0), 0.5 * (0.0 + 1.0), 1e-12));
    // A trailing 360 plane equal to 0 is dropped.
    let p = parse_ies(&ies_simple(&[0.0, 180.0, 360.0], &vert, &[vec![5.0, 0.0], vec![7.0, 0.0], vec![5.0, 0.0]]), IesOptions::default()).unwrap();
    assert_eq!(p.c_deg, vec![0.0, 180.0]);
}

#[test]
fn ies_variants_tilt_and_refusals() {
    let vert = [0.0, 90.0];
    let rows = [vec![10.0, 5.0]];
    // 1986: no version line, 1991: IESNA91, 1995.
    for (v, label) in [("", "1986"), ("IESNA91", "IESNA91"), ("IESNA:LM-63-1995", "1995")] {
        let p = parse_ies(&ies(v, "TILT=NONE", 1.0, 1000.0, 1.0, 1, 2, [0.0; 3], [1.0, 1.0, 10.0], &vert, &[0.0], &rows), IesOptions::default()).unwrap();
        assert!(matches!(&p.format, SourceFormat::Ies { version } if version.contains(label)), "{label}: {:?}", p.format);
    }
    let p = parse_ies(&ies("IES:LM-63-2019", "TILT=NONE", 1.0, 1000.0, 1.0, 1, 2, [0.0; 3], [1.0, 1.0, 10.0], &vert, &[0.0], &rows), IesOptions::default()).unwrap();
    assert!(p.warnings.iter().any(|w| w.contains("not one of the variants")));
    // TILT=INCLUDE is read and skipped with a warning.
    let p = parse_ies(&ies("IESNA:LM-63-2002", "TILT=INCLUDE", 1.0, 1000.0, 1.0, 1, 2, [0.0; 3], [1.0, 1.0, 10.0], &vert, &[0.0], &rows), IesOptions::default()).unwrap();
    assert_eq!(p.candela, vec![10.0, 5.0]);
    assert!(p.warnings.iter().any(|w| w.contains("TILT=INCLUDE")));
    // A tilt file is refused.
    let e = parse_ies(&ies("IESNA:LM-63-2002", "TILT=tilt.dat", 1.0, 1000.0, 1.0, 1, 2, [0.0; 3], [1.0, 1.0, 10.0], &vert, &[0.0], &rows), IesOptions::default()).unwrap_err();
    assert!(matches!(e, PhotometryError::Unsupported(m) if m.contains("tilt.dat")));
    // Type B (2) and A (3) are refused with a clear message.
    for t in [2, 3] {
        let e = parse_ies(&ies("IESNA:LM-63-2002", "TILT=NONE", 1.0, 1000.0, 1.0, t, 2, [0.0; 3], [1.0, 1.0, 10.0], &vert, &[0.0], &rows), IesOptions::default()).unwrap_err();
        assert!(matches!(&e, PhotometryError::Unsupported(m) if m.contains("type C")), "{e}");
    }
    // Truncated candela table, bad numbers, decreasing angles, odd horizontal range.
    let mut text = ies_simple(&[0.0], &vert, &rows);
    text.truncate(text.rfind("10 5").unwrap());
    assert!(matches!(parse_ies(&text, IesOptions::default()), Err(PhotometryError::Parse(m)) if m.contains("ends")));
    assert!(matches!(parse_ies(&ies_simple(&[0.0], &[90.0, 0.0], &rows), IesOptions::default()), Err(PhotometryError::Parse(_))));
    let e = parse_ies(&ies_simple(&[10.0, 60.0], &vert, &[vec![1.0, 1.0], vec![1.0, 1.0]]), IesOptions::default()).unwrap_err();
    assert!(matches!(e, PhotometryError::Unsupported(m) if m.contains("horizontal angles")));
    assert!(parse_ies("no tilt here", IesOptions::default()).is_err());
    // Commas as separators (accepted by LM-63 readers).
    let text = ies_simple(&[0.0], &vert, &rows).replace("10 5", "10, 5");
    assert_eq!(parse_ies(&text, IesOptions::default()).unwrap().candela, vec![10.0, 5.0]);
}

#[test]
fn ies_upward_only_and_downward_only_ranges() {
    let down = parse_ies(&ies_simple(&[0.0], &[0.0, 90.0], &[vec![100.0, 100.0]]), IesOptions::default()).unwrap();
    assert_eq!(down.intensity(0.0, 120.0), 0.0, "nothing above the tabulated range");
    assert_eq!(down.flux_check(2.0).upward_flux_lm, 0.0);
    let up = parse_ies(&ies_simple(&[0.0], &[90.0, 180.0], &[vec![100.0, 100.0]]), IesOptions::default()).unwrap();
    assert_eq!(up.intensity(0.0, 30.0), 0.0);
    assert!(up.intensity(0.0, 135.0) > 99.0);
}

#[test]
fn eulumdat_conversion_and_each_symmetry() {
    // Mc = 4 planes at 0, 90, 180, 270; Ng = 3 gamma angles 0, 45, 90. Flux 2000 lm, factor 1.5:
    // cd = value * 1.5 * 2000 / 1000 = 3 * value.
    let c = [0.0, 90.0, 180.0, 270.0];
    let g = [0.0, 45.0, 90.0];
    // Isym 0: all four planes.
    let vals = vec![vec![1.0, 2.0, 3.0], vec![4.0, 5.0, 6.0], vec![7.0, 8.0, 9.0], vec![10.0, 11.0, 12.0]];
    let p = parse_ldt(&ldt(0, 4, &c, &g, &vals, 1, 2000.0, 1.5, 80.0, 0.0)).unwrap();
    assert!(close(p.intensity(90.0, 45.0), 15.0, 1e-12) && close(p.intensity(270.0, 90.0), 36.0, 1e-12));
    assert_eq!(p.lamp_flux_lm, Some(2000.0));
    assert_eq!(p.lor_percent, Some(80.0));
    assert_eq!(p.dff_percent, Some(95.0));
    assert_eq!(p.dimensions_m, Some([0.6, 0.3, 0.1]));
    // Isym 1: one plane for all.
    let p = parse_ldt(&ldt(1, 4, &c, &g, &[vec![1.0, 2.0, 3.0]], 1, 2000.0, 1.0, 100.0, 0.0)).unwrap();
    assert!(close(p.intensity(123.0, 45.0), 4.0, 1e-12));
    // Isym 2: planes C0, C90, C180 stored; C270 mirrors C90.
    let p = parse_ldt(&ldt(2, 4, &c, &g, &[vec![1.0, 0.0, 0.0], vec![2.0, 0.0, 0.0], vec![3.0, 0.0, 0.0]], 1, 1000.0, 1.0, 100.0, 0.0)).unwrap();
    assert!(close(p.intensity(270.0, 0.0), 2.0, 1e-12) && close(p.intensity(180.0, 0.0), 3.0, 1e-12));
    // Isym 3: stored planes start at C270 and run through C0 to C90; C180 mirrors C0.
    let p = parse_ldt(&ldt(3, 4, &c, &g, &[vec![7.0, 0.0, 0.0], vec![1.0, 0.0, 0.0], vec![5.0, 0.0, 0.0]], 1, 1000.0, 1.0, 100.0, 0.0)).unwrap();
    assert!(close(p.intensity(270.0, 0.0), 7.0, 1e-12) && close(p.intensity(0.0, 0.0), 1.0, 1e-12) && close(p.intensity(90.0, 0.0), 5.0, 1e-12));
    assert!(close(p.intensity(180.0, 0.0), 1.0, 1e-12), "C180 mirrors C0 about the C90-C270 plane");
    // Isym 4: C0 and C90 stored (Mc/4 + 1 = 2 planes), the rest by mirroring.
    let p = parse_ldt(&ldt(4, 4, &c, &g, &[vec![1.0, 0.0, 0.0], vec![2.0, 0.0, 0.0]], 1, 1000.0, 1.0, 100.0, 0.0)).unwrap();
    assert!(close(p.intensity(180.0, 0.0), 1.0, 1e-12) && close(p.intensity(270.0, 0.0), 2.0, 1e-12));
    // Interpolation in C is linear between planes: C45 between C0 (1) and C90 (2).
    assert!(close(p.intensity(45.0, 0.0), 1.5, 1e-12));
}

#[test]
fn eulumdat_absolute_tilt_and_errors() {
    let c = [0.0, 90.0, 180.0, 270.0];
    let g = [0.0, 90.0];
    let p = parse_ldt(&ldt(1, 4, &c, &g, &[vec![5.0, 5.0]], -1, 3000.0, 1.0, 100.0, 10.0)).unwrap();
    assert!(p.absolute_photometry);
    assert_eq!(p.measured_tilt_deg, 10.0);
    assert!(p.warnings.iter().any(|w| w.contains("tilted 10 degrees")));
    // The C angles must fit the symmetry indicator.
    let three = vec![vec![1.0, 1.0]; 3];
    let ok = parse_ldt(&ldt(3, 4, &c, &g, &three, 1, 1000.0, 1.0, 100.0, 0.0)).unwrap();
    assert_eq!(ok.c_deg.len(), 4);
    let bad = ldt(2, 4, &[0.0, 45.0, 90.0, 135.0], &g, &three, 1, 1000.0, 1.0, 100.0, 0.0);
    assert!(matches!(parse_ldt(&bad), Err(PhotometryError::Parse(m)) if m.contains("C0 to C180")));
    // Mc that does not divide.
    let bad = ldt(4, 6, &[0.0; 6], &g, &vec![vec![1.0, 1.0]; 2], 1, 1000.0, 1.0, 100.0, 0.0);
    assert!(parse_ldt(&bad).is_err());
    // Too little data.
    let mut short = ldt(0, 4, &c, &g, &vec![vec![1.0, 1.0]; 4], 1, 1000.0, 1.0, 100.0, 0.0);
    short.truncate(short.len() - 6);
    assert!(matches!(parse_ldt(&short), Err(PhotometryError::Parse(m)) if m.contains("needed")));
    assert!(parse_ldt("a\nb").is_err());
    // Zero flux cannot be converted.
    assert!(parse_ldt(&ldt(1, 4, &c, &g, &[vec![1.0, 1.0]], 1, 0.0, 1.0, 100.0, 0.0)).is_err());
}

#[test]
fn bilinear_interpolation_is_exact_on_a_bilinear_function() {
    // I(C, g) = 10 + 0.1 C + 0.2 g is bilinear in the cells, so interpolation reproduces it.
    let horiz: Vec<f64> = (0..8).map(|k| k as f64 * 45.0).collect();
    let vert: Vec<f64> = (0..=4).map(|k| k as f64 * 22.5).collect();
    let rows: Vec<Vec<f64>> = horiz.iter().map(|c| vert.iter().map(|g| 10.0 + 0.1 * c + 0.2 * g).collect()).collect();
    let p = parse_ies(&ies_simple(&horiz, &vert, &rows), IesOptions::default()).unwrap();
    for (c, g) in [(10.0, 5.0), (100.0, 40.0), (200.5, 77.7), (314.0, 3.0)] {
        assert!(close(p.intensity(c, g), 10.0 + 0.1 * c + 0.2 * g, 1e-12), "({c}, {g})");
    }
    assert_eq!(p.intensity(0.0, 91.0), 0.0);
    assert_eq!(p.intensity(f64::NAN, 10.0), 0.0);
    let peak = p.peak_intensity();
    assert!(close(peak.candela, 10.0 + 0.1 * 315.0 + 0.2 * 90.0, 1e-12) && peak.c_deg == 315.0 && peak.gamma_deg == 90.0);
}

#[test]
fn lambertian_downward_source_has_flux_pi_i0() {
    // I = I0 cos(gamma) on the lower hemisphere: flux = pi * I0 (exact integral). The midpoint
    // rule on a linear-interpolated table is within 1e-4 at a 1 degree step.
    let vert: Vec<f64> = (0..=90).map(|k| k as f64).collect();
    let i0 = 1000.0;
    let row: Vec<f64> = vert.iter().map(|g| i0 * g.to_radians().cos()).collect();
    let p = parse_ies(&ies_simple(&[0.0], &vert, &[row]), IesOptions::default()).unwrap();
    let exact = std::f64::consts::PI * i0;
    let flux = p.integrated_flux_lm(1.0);
    assert!((flux - exact).abs() / exact < 1e-4, "{flux} against {exact}");
    let check = p.flux_check(1.0);
    assert!(close(check.downward_fraction, 1.0, 1e-12) && check.upward_flux_lm == 0.0);
    // A coarser step is less accurate, as stated.
    assert!((p.integrated_flux_lm(10.0) - exact).abs() / exact < 5e-3);
}

#[test]
fn flux_towards_a_side_of_a_symmetric_and_a_one_sided_distribution() {
    let vert = [0.0, 90.0];
    let sym = parse_ies(&ies_simple(&[0.0], &vert, &[vec![10.0, 10.0]]), IesOptions::default()).unwrap();
    assert!(close(sym.flux_fraction_towards_c(90.0, 2.0), 0.5, 1e-9));
    // Light only in C 0 to 180 (the C90 side).
    let rows = [vec![0.0, 0.0], vec![10.0, 10.0], vec![0.0, 0.0], vec![0.0, 0.0]];
    let one = parse_ies(&ies_simple(&[0.0, 90.0, 180.0, 270.0], &vert, &rows), IesOptions::default()).unwrap();
    assert!(one.flux_fraction_towards_c(90.0, 1.0) > 0.99);
    assert!(one.flux_fraction_towards_c(270.0, 1.0) < 0.01);
}

#[test]
fn files_are_read_by_extension_and_hashed() {
    let text = ies_simple(&[0.0], &[0.0, 90.0], &[vec![10.0, 5.0]]);
    let l = parse_photometry_bytes("A.IES", text.as_bytes()).unwrap();
    assert_eq!(l.file_name, "A.IES");
    assert_eq!(l.sha256, sha256_hex(text.as_bytes()));
    assert!(matches!(l.photometry.format, SourceFormat::Ies { .. }));
    // Unknown extension: a TILT= line means IES.
    assert!(matches!(parse_photometry_bytes("x.dat", text.as_bytes()).unwrap().photometry.format, SourceFormat::Ies { .. }));
    let c = [0.0, 90.0, 180.0, 270.0];
    let ldt_text = ldt(1, 4, &c, &[0.0, 90.0], &[vec![5.0, 5.0]], 1, 1000.0, 1.0, 100.0, 0.0);
    assert!(matches!(parse_photometry_bytes("b.ldt", ldt_text.as_bytes()).unwrap().photometry.format, SourceFormat::Eulumdat));
    // Latin-1 bytes in a text field are accepted.
    let mut bytes = ldt_text.into_bytes();
    bytes.splice(0..10, b"M\xfcller GmbH".iter().copied());
    assert!(parse_photometry_bytes("c.ldt", &bytes).is_ok());
    assert!(matches!(load_photometry_file(std::path::Path::new("/nonexistent/none.ies")), Err(PhotometryError::Io(_))));
}

/// Real photometric files, when a folder of them is named by `AFROBIM_PHOTOMETRY_SAMPLES`. The
/// files are not in the repository (their licences are not known to allow redistribution). Skips
/// when the variable is not set. Prints one line per file.
#[test]
fn real_files_when_present() {
    let Ok(dir) = std::env::var("AFROBIM_PHOTOMETRY_SAMPLES") else {
        eprintln!("skipped: AFROBIM_PHOTOMETRY_SAMPLES is not set");
        return;
    };
    let mut names: Vec<_> = std::fs::read_dir(&dir).unwrap().filter_map(|e| e.ok()).map(|e| e.path()).filter(|p| matches!(p.extension().and_then(|e| e.to_str()).map(|e| e.to_ascii_lowercase()).as_deref(), Some("ies") | Some("ldt"))).collect();
    names.sort();
    let mut read = 0;
    for path in &names {
        let l = match load_photometry_file(path) {
            Ok(l) => l,
            Err(e) => {
                eprintln!("{}: REFUSED: {e}", path.file_name().unwrap().to_string_lossy());
                continue;
            }
        };
        read += 1;
        let p = &l.photometry;
        p.validate().unwrap();
        let f = p.flux_check(1.0);
        let peak = p.peak_intensity();
        eprintln!(
            "{:28} {:28} {:3} planes x {:3} gamma  stated {:>9} lm  integrated {:>9.1} lm  LOR computed {:>6} stated {:>6}  diff {:>7}  down {:.3}  toward C0 {:.3} C90 {:.3} C270 {:.3}  peak {:.0} cd at C{} g{}",
            path.file_name().unwrap().to_string_lossy(),
            p.format.label().chars().take(28).collect::<String>(),
            p.c_deg.len(),
            p.gamma_deg.len(),
            p.lamp_flux_lm.map_or("-".to_string(), |v| format!("{v:.0}")),
            f.integrated_flux_lm,
            f.computed_lor_percent.map_or("-".to_string(), |v| format!("{v:.1}%")),
            f.stated_lor_percent.map_or("-".to_string(), |v| format!("{v:.1}%")),
            f.difference_percent.map_or("-".to_string(), |v| format!("{v:.2}%")),
            f.downward_fraction,
            p.flux_fraction_towards_c(0.0, 2.0),
            p.flux_fraction_towards_c(90.0, 2.0),
            p.flux_fraction_towards_c(270.0, 2.0),
            peak.candela,
            peak.c_deg,
            peak.gamma_deg
        );
        assert!(f.integrated_flux_lm > 0.0);
        // Evidence for the IES orientation default: a house-side-shielded roadway luminaire sends
        // most of its flux to the C0 side (the street side of an IES file).
        if path.file_name().is_some_and(|n| n == "sdl_t2u.ies") {
            assert!(p.flux_fraction_towards_c(0.0, 2.0) > 0.7);
        }
    }
    assert!(read >= 1 || names.is_empty());
}
