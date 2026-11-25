// SPDX-License-Identifier: LicenseRef-AfroBIM-Proprietary
// Copyright (c) 2026 AfroBIM. All rights reserved.
//! EULUMDAT reader. See the module documentation of [`crate::photometry`] for what each field
//! rests on.

use super::model::{complete_planes, Completion, Photometry, SourceFormat};
use super::PhotometryError;

fn num(line: &str, what: &str) -> Result<f64, PhotometryError> {
    let t = line.trim().replace(',', ".");
    if t.is_empty() {
        return Err(PhotometryError::Parse(format!("EULUMDAT: {what} is empty")));
    }
    t.parse::<f64>().map_err(|_| PhotometryError::Parse(format!("EULUMDAT: '{}' is not a number where {what} was expected", line.trim())))
}

fn whole(line: &str, what: &str) -> Result<usize, PhotometryError> {
    let v = num(line, what)?;
    if v < 0.0 || v.fract() != 0.0 || v > 1.0e6 {
        return Err(PhotometryError::Parse(format!("EULUMDAT: {what} must be a whole number, found {}", line.trim())));
    }
    Ok(v as usize)
}

/// Reads a EULUMDAT file from its text.
pub fn parse_ldt(text: &str) -> Result<Photometry, PhotometryError> {
    let text = text.trim_start_matches('\u{feff}');
    let lines: Vec<&str> = text.lines().map(|l| l.trim_end_matches('\r')).collect();
    let mut warnings: Vec<String> = Vec::new();
    if lines.len() < 27 {
        return Err(PhotometryError::Parse(format!("EULUMDAT: the file has {} lines; the header alone needs 26", lines.len())));
    }
    let line = |n: usize| lines[n - 1];

    let manufacturer = line(1).trim().to_string();
    let ityp = whole(line(2), "the type indicator")?;
    let isym = whole(line(3), "the symmetry indicator")?;
    let mc = whole(line(4), "the number of C-planes Mc")?;
    let ng = whole(line(6), "the number of intensities per C-plane Ng")?;
    let test_report = line(8).trim().to_string();
    let luminaire = line(9).trim().to_string();
    let catalogue = line(10).trim().to_string();
    if ityp > 3 {
        return Err(PhotometryError::Parse(format!("EULUMDAT: the type indicator must be 0 to 3, found {ityp}")));
    }
    if isym > 4 {
        return Err(PhotometryError::Parse(format!("EULUMDAT: the symmetry indicator must be 0 to 4, found {isym}")));
    }
    if mc == 0 || ng == 0 {
        return Err(PhotometryError::Parse("EULUMDAT: Mc or Ng is zero".to_string()));
    }
    let dim_mm = |n: usize, what: &str| num(line(n), what).map(|v| v / 1000.0);
    let luminaire_len = dim_mm(13, "the length of the luminaire")?;
    let luminaire_wid = dim_mm(14, "the width of the luminaire")?;
    let luminaire_hgt = dim_mm(15, "the height of the luminaire")?;
    let area_len = dim_mm(16, "the length of the luminous area")?;
    let area_wid = dim_mm(17, "the width of the luminous area")?;
    let dff = num(line(22), "the downward flux fraction")?;
    let lorl = num(line(23), "the light output ratio of the luminaire")?;
    let mut conversion = num(line(24), "the conversion factor for luminous intensities")?;
    let tilt = num(line(25), "the tilt of the luminaire during measurement")?;
    let sets = whole(line(26), "the number of standard sets of lamps n")?;
    if sets == 0 {
        return Err(PhotometryError::Parse("EULUMDAT: no lamp set is given, so the cd/klm values cannot be converted to candela".to_string()));
    }
    if lines.len() < 26 + 6 * sets + 1 {
        return Err(PhotometryError::Parse("EULUMDAT: the file ends inside the lamp data".to_string()));
    }
    // Each lamp set takes six lines: number of lamps, type, total flux, colour appearance, colour
    // rendering, wattage including ballast. (Files with one set, the usual case, do not tell the
    // field-by-field and set-by-set orders apart; a file with several sets was read set by set.)
    let mut flux_total = 0.0;
    let mut power = 0.0;
    let mut absolute = false;
    for s in 0..sets {
        let base = 27 + 6 * s;
        let lamps = num(line(base), "the number of lamps")?;
        let flux = num(line(base + 2), "the total luminous flux of the lamps")?;
        let watts = num(line(base + 5), "the wattage including ballast")?;
        if lamps < 0.0 {
            absolute = true;
        }
        flux_total += flux;
        power += watts;
    }
    if sets > 1 {
        warnings.push(format!("{sets} lamp sets are given; their fluxes and powers were added and the lines were read set by set."));
    }
    if !(flux_total.is_finite() && flux_total > 0.0) {
        return Err(PhotometryError::Parse(format!("EULUMDAT: the total luminous flux must be greater than zero (it is {flux_total})")));
    }
    if conversion == 0.0 {
        warnings.push("The conversion factor for luminous intensities is 0; 1 was used.".to_string());
        conversion = 1.0;
    }
    if conversion.is_nan() || conversion < 0.0 {
        return Err(PhotometryError::Parse(format!("EULUMDAT: the conversion factor must be greater than zero (it is {conversion})")));
    }

    // From here on, values may be laid out freely: ten direct ratios, Mc C angles, Ng gamma
    // angles, then the intensities.
    let rest: String = lines[26 + 6 * sets..].join("\n");
    let toks: Vec<&str> = rest.split_whitespace().collect();
    let planes_stored = match isym {
        0 => mc,
        1 => 1,
        2 => mc / 2 + 1,
        3 => mc / 2 + 1,
        _ => mc / 4 + 1,
    };
    if (isym == 2 && mc % 2 != 0) || ((isym == 3 || isym == 4) && mc % 4 != 0) {
        return Err(PhotometryError::Parse(format!("EULUMDAT: Mc = {mc} does not fit the symmetry indicator {isym} (Mc/2 or Mc/4 must be whole)")));
    }
    let needed = 10 + mc + ng + planes_stored * ng;
    if toks.len() < needed {
        return Err(PhotometryError::Parse(format!(
            "EULUMDAT: the data after the lamp sets has {} values; {needed} are needed (10 direct ratios, {mc} C angles, {ng} gamma angles, {planes_stored} planes of {ng} intensities for symmetry {isym})",
            toks.len()
        )));
    }
    let parse_at = |i: usize, what: &str| -> Result<f64, PhotometryError> { num(toks[i], what) };
    let c_list: Vec<f64> = (0..mc).map(|i| parse_at(10 + i, "a C angle")).collect::<Result<_, _>>()?;
    let gamma: Vec<f64> = (0..ng).map(|i| parse_at(10 + mc + i, "a gamma angle")).collect::<Result<_, _>>()?;
    if toks.len() > needed {
        warnings.push(format!("{} values after the intensity table were ignored.", toks.len() - needed));
    }
    let values: Vec<f64> = (0..planes_stored * ng).map(|i| parse_at(10 + mc + ng + i, "an intensity")).collect::<Result<_, _>>()?;

    if gamma.windows(2).any(|w| w[1] <= w[0]) || gamma[0] < 0.0 || gamma[ng - 1] > 180.0 {
        return Err(PhotometryError::Parse("EULUMDAT: the gamma angles must increase and lie between 0 and 180 degrees".to_string()));
    }
    if values.iter().any(|v| !v.is_finite() || *v < 0.0) {
        return Err(PhotometryError::Parse("EULUMDAT: an intensity is negative or not finite".to_string()));
    }

    // The stored C-planes: Mc1 to Mc2 of the angle list, from the table of the DIALux evo
    // knowledge base article (isym 3 starts at C270 and runs through C0 to C90).
    let stored_idx: Vec<usize> = match isym {
        0 => (0..mc).collect(),
        1 => vec![0],
        2 => (0..=mc / 2).collect(),
        3 => (0..=mc / 2).map(|k| (3 * mc / 4 + k) % mc).collect(),
        _ => (0..=mc / 4).collect(),
    };
    let stored_angles: Vec<f64> = stored_idx.iter().map(|&i| c_list[i]).collect();
    let (completion, symmetry) = match isym {
        0 => (Completion::None, "no symmetry"),
        1 => (Completion::Axial, "symmetric about the vertical axis"),
        2 => (Completion::About0And180, "symmetric about the C0-C180 plane"),
        3 => (Completion::About90And270, "symmetric about the C90-C270 plane"),
        _ => (Completion::Quadrants, "symmetric about the C0-C180 and C90-C270 planes"),
    };
    let expect = |ok: bool, msg: &str| -> Result<(), PhotometryError> {
        if ok {
            Ok(())
        } else {
            Err(PhotometryError::Parse(format!("EULUMDAT: the C angles do not fit the symmetry indicator {isym}: {msg}")))
        }
    };
    let close = |a: f64, b: f64| (a.rem_euclid(360.0) - b).abs() < 1e-6 || (a.rem_euclid(360.0) - b - 360.0).abs() < 1e-6 || (a.rem_euclid(360.0) + 360.0 - b).abs() < 1e-6;
    match isym {
        0 => {
            expect(stored_angles.windows(2).all(|w| w[1] > w[0]) || mc == 1, "the angles must increase")?;
        }
        2 => {
            expect(close(stored_angles[0], 0.0) && close(*stored_angles.last().unwrap(), 180.0), "the stored planes must run from C0 to C180")?;
        }
        3 => {
            expect(close(stored_angles[0], 270.0) && close(*stored_angles.last().unwrap(), 90.0), "the stored planes must run from C270 through C0 to C90")?;
        }
        4 => {
            expect(close(stored_angles[0], 0.0) && close(*stored_angles.last().unwrap(), 90.0), "the stored planes must run from C0 to C90")?;
        }
        _ => {}
    }
    let planes: Vec<(f64, Vec<f64>)> = stored_angles
        .iter()
        .enumerate()
        .map(|(k, a)| {
            let angle = if isym == 1 { 0.0 } else { *a };
            let row: Vec<f64> = values[k * ng..(k + 1) * ng].iter().map(|v| v * conversion * flux_total / 1000.0).collect();
            (angle, row)
        })
        .collect();
    let (c_deg, rows) = complete_planes(planes, completion);
    let candela: Vec<f64> = rows.into_iter().flatten().collect();

    if tilt != 0.0 {
        warnings.push(format!(
            "The luminaire was tilted {tilt} degrees during measurement. The intensities are used in the C-gamma frame as measured and the installed tilt is applied relative to this measured tilt, assuming the same sign convention."
        ));
    }
    if isym == 0 || isym == 2 || isym == 3 {
        warnings.push("Which of C90 and C270 faces the carriageway is not stated in the file; see the orientation setting.".to_string());
    }

    let p = Photometry {
        format: SourceFormat::Eulumdat,
        manufacturer,
        luminaire,
        catalogue,
        test_report,
        c_deg,
        gamma_deg: gamma,
        candela,
        lamp_flux_lm: Some(flux_total),
        absolute_photometry: absolute,
        lor_percent: Some(lorl),
        dff_percent: Some(dff),
        input_power_w: Some(power),
        ballast_factor: None,
        ballast_lamp_photometric_factor: None,
        measured_tilt_deg: tilt,
        dimensions_m: Some([luminaire_len, luminaire_wid, luminaire_hgt]),
        luminous_area_m: Some([area_len, area_wid]),
        symmetry: symmetry.to_string(),
        warnings,
    };
    p.validate().map_err(PhotometryError::Parse)?;
    Ok(p)
}
