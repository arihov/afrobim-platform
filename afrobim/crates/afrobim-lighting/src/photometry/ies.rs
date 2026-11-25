// SPDX-License-Identifier: LicenseRef-AfroBIM-Proprietary
// Copyright (c) 2026 AfroBIM. All rights reserved.
//! IES LM-63 reader. See the module documentation of [`crate::photometry`] for what each field
//! rests on.

use super::model::{complete_planes, Completion, Photometry, SourceFormat};
use super::PhotometryError;

/// Options for reading an IES file.
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct IesOptions {
    /// Multiply the candela values by the file's ballast factor as well as by the candela
    /// multiplier. Off by default: the intensities are the file's candela values times the
    /// candela multiplier. The ballast factor is read and reported either way.
    pub apply_ballast_factor: bool,
}

const FEET_M: f64 = 0.3048;

fn parse_num(tok: &str, what: &str) -> Result<f64, PhotometryError> {
    tok.parse::<f64>().map_err(|_| PhotometryError::Parse(format!("IES: '{tok}' is not a number where {what} was expected")))
}

fn count(tok: &str, what: &str) -> Result<usize, PhotometryError> {
    let v = parse_num(tok, what)?;
    if v < 0.0 || v.fract() != 0.0 || v > 1.0e7 {
        return Err(PhotometryError::Parse(format!("IES: {what} must be a whole number, found {tok}")));
    }
    Ok(v as usize)
}

/// Reads an IES LM-63 file (any of the verified variants) from its text.
pub fn parse_ies(text: &str, options: IesOptions) -> Result<Photometry, PhotometryError> {
    let text = text.trim_start_matches('\u{feff}');
    let lines: Vec<&str> = text.lines().map(|l| l.trim_end_matches('\r')).collect();
    let mut warnings: Vec<String> = Vec::new();

    // The version line. LM-63-1986 has none: its first line is header text.
    let first = lines.first().map(|l| l.trim()).unwrap_or("");
    let upper = first.to_ascii_uppercase();
    let version = if upper == "IESNA91" {
        "IESNA91 (LM-63-1991)".to_string()
    } else if upper == "IESNA:LM-63-1995" {
        "IESNA:LM-63-1995".to_string()
    } else if upper == "IESNA:LM-63-2002" {
        "IESNA:LM-63-2002".to_string()
    } else if upper.starts_with("IESNA") || upper.starts_with("IES:") {
        warnings.push(format!(
            "IES version line '{first}' is not one of the variants checked for this reader (IESNA91, LM-63-1995, LM-63-2002, or no version line for LM-63-1986). The numeric part of LM-63 files is laid out the same way in the versions described by the sources cited, so it was read, but check the result."
        ));
        first.to_string()
    } else {
        "LM-63-1986 (no version line)".to_string()
    };

    // Header lines up to TILT=.
    let tilt_at = lines
        .iter()
        .position(|l| l.trim_start().to_ascii_uppercase().starts_with("TILT"))
        .ok_or_else(|| PhotometryError::Parse("IES: no TILT= line was found".to_string()))?;
    let mut manufacturer = String::new();
    let mut luminaire = String::new();
    let mut catalogue = String::new();
    let mut test_report = String::new();
    for l in &lines[..tilt_at] {
        let l = l.trim();
        if let Some(rest) = l.strip_prefix('[') {
            if let Some(end) = rest.find(']') {
                let key = rest[..end].trim().to_ascii_uppercase();
                let val = rest[end + 1..].trim().to_string();
                match key.as_str() {
                    "MANUFAC" => manufacturer = val,
                    "LUMINAIRE" => luminaire = val,
                    "LUMCAT" => catalogue = val,
                    "TEST" => test_report = val,
                    _ => {}
                }
            }
        }
    }
    let tilt_line = lines[tilt_at].trim();
    let tilt_value = tilt_line.split_once('=').map(|x| x.1.trim().to_string()).unwrap_or_default();
    let tilt_upper = tilt_value.to_ascii_uppercase();
    if tilt_upper != "NONE" && tilt_upper != "INCLUDE" {
        return Err(PhotometryError::Unsupported(format!(
            "IES: TILT={tilt_value} refers to a separate tilt file, which is not read. Use a file with TILT=NONE or TILT=INCLUDE."
        )));
    }

    let body: String = lines[tilt_at + 1..].join("\n");
    let toks: Vec<&str> = body.split(|c: char| c.is_whitespace() || c == ',').filter(|t| !t.is_empty()).collect();
    let mut pos = 0usize;
    let mut take = |what: &str| -> Result<&str, PhotometryError> {
        let t = toks.get(pos).copied().ok_or_else(|| PhotometryError::Parse(format!("IES: the file ends before {what}")))?;
        pos += 1;
        Ok(t)
    };

    if tilt_upper == "INCLUDE" {
        // Four records: lamp-to-luminaire geometry, the number of pairs n, n angles, n factors.
        let _geometry = take("the lamp-to-luminaire geometry of the tilt data")?;
        let n = count(take("the number of tilt angle pairs")?, "the number of tilt angle pairs")?;
        for _ in 0..2 * n {
            take("the tilt angles and multiplying factors")?;
        }
        warnings.push("TILT=INCLUDE: the tilt multiplying factors are read and skipped. They describe how a lamp's output changes with the lamp's own tilt, which this crate does not model.".to_string());
    }

    let n_lamps = parse_num(take("the number of lamps")?, "the number of lamps")?;
    let lumens_per_lamp = parse_num(take("the lumens per lamp")?, "the lumens per lamp")?;
    let multiplier = parse_num(take("the candela multiplier")?, "the candela multiplier")?;
    let nv = count(take("the number of vertical angles")?, "the number of vertical angles")?;
    let nh = count(take("the number of horizontal angles")?, "the number of horizontal angles")?;
    let ptype = parse_num(take("the photometric type")?, "the photometric type")?;
    let units = parse_num(take("the units type")?, "the units type")?;
    let width = parse_num(take("the width")?, "the width")?;
    let length = parse_num(take("the length")?, "the length")?;
    let height = parse_num(take("the height")?, "the height")?;
    let ballast_factor = parse_num(take("the ballast factor")?, "the ballast factor")?;
    let ballast_lamp = parse_num(take("the ballast-lamp photometric factor")?, "the ballast-lamp photometric factor")?;
    let watts = parse_num(take("the input watts")?, "the input watts")?;

    if ptype != 1.0 {
        return Err(PhotometryError::Unsupported(format!(
            "IES: photometric type {ptype} is not handled. Only type C (type value 1) is read; types B and A (floodlight and goniometer photometry) are refused because road lighting files are type C and the angle systems differ."
        )));
    }
    if nv == 0 || nh == 0 {
        return Err(PhotometryError::Parse("IES: the number of vertical or horizontal angles is zero".to_string()));
    }
    if !(multiplier.is_finite() && multiplier > 0.0) {
        return Err(PhotometryError::Parse(format!("IES: the candela multiplier must be greater than zero (it is {multiplier})")));
    }
    let mut vert = Vec::with_capacity(nv);
    for _ in 0..nv {
        vert.push(parse_num(take("the vertical angles")?, "a vertical angle")?);
    }
    let mut horiz = Vec::with_capacity(nh);
    for _ in 0..nh {
        horiz.push(parse_num(take("the horizontal angles")?, "a horizontal angle")?);
    }
    let mut candela = Vec::with_capacity(nv * nh);
    for _ in 0..nv * nh {
        candela.push(parse_num(take("the candela values")?, "a candela value")?);
    }
    if pos < toks.len() {
        warnings.push(format!("{} values after the candela table were ignored.", toks.len() - pos));
    }

    let increasing = |v: &[f64]| v.windows(2).all(|w| w[1] > w[0]);
    if !increasing(&vert) || !increasing(&horiz) {
        return Err(PhotometryError::Parse("IES: the vertical and horizontal angles must be in increasing order".to_string()));
    }
    if vert[0] < 0.0 || vert[nv - 1] > 180.0 {
        return Err(PhotometryError::Parse("IES: vertical angles must be between 0 and 180 degrees".to_string()));
    }
    let standard_vertical = [(0.0, 90.0), (90.0, 180.0), (0.0, 180.0)].iter().any(|(a, b)| vert[0] == *a && vert[nv - 1] == *b);
    if !standard_vertical {
        warnings.push(format!(
            "The vertical angles run from {} to {} degrees, not 0 to 90, 90 to 180 or 0 to 180 as LM-63 describes. Intensity outside the range is taken as zero.",
            vert[0],
            vert[nv - 1]
        ));
    }
    if candela.iter().any(|v| !v.is_finite() || *v < 0.0) {
        return Err(PhotometryError::Parse("IES: a candela value is negative or not finite".to_string()));
    }

    // Absolute photometry: lumens per lamp of -1 (LM-63). Some writers put a negative flux there.
    let absolute = lumens_per_lamp < 0.0;
    let lamp_flux_lm = if !absolute {
        Some(n_lamps.abs().max(1.0) * lumens_per_lamp)
    } else if lumens_per_lamp == -1.0 {
        None
    } else {
        warnings.push(format!(
            "Lumens per lamp is {lumens_per_lamp}: LM-63 uses -1 for absolute photometry. The magnitude is taken as the flux of the luminaire."
        ));
        Some(lumens_per_lamp.abs())
    };

    let mut scale = multiplier;
    if options.apply_ballast_factor {
        scale *= ballast_factor;
    } else if (ballast_factor - 1.0).abs() > 1e-9 {
        warnings.push(format!("The ballast factor is {ballast_factor}; it is not applied to the candela values (IesOptions::apply_ballast_factor is off)."));
    }
    if (ballast_lamp - 1.0).abs() > 1e-9 {
        warnings.push(format!("The ballast-lamp photometric factor / future-use field is {ballast_lamp}; it is not used."));
    }

    // Horizontal angles decide the symmetry (Autodesk 3ds Max help, IES Standard File Format; AGi32
    // Photometric File Type C concepts for the 90 to 270 case).
    let (completion, symmetry, stored): (Completion, &str, Vec<f64>) = if nh == 1 {
        if horiz[0] != 0.0 {
            warnings.push(format!("A single horizontal angle of {} degrees was found; the distribution is taken as rotationally symmetric.", horiz[0]));
        }
        (Completion::Axial, "rotationally symmetric (one horizontal angle)", horiz.clone())
    } else if horiz[0] == 0.0 && horiz[nh - 1] == 90.0 {
        (Completion::Quadrants, "symmetric about the 0-180 and 90-270 planes (horizontal angles 0 to 90)", horiz.clone())
    } else if horiz[0] == 0.0 && horiz[nh - 1] == 180.0 {
        (Completion::About0And180, "symmetric about the 0-180 plane (horizontal angles 0 to 180)", horiz.clone())
    } else if horiz[0] == 90.0 && horiz[nh - 1] == 270.0 {
        (Completion::About90And270, "symmetric about the 90-270 plane (horizontal angles 90 to 270)", horiz.clone())
    } else if horiz[0] == 0.0 && horiz[nh - 1] > 180.0 && horiz[nh - 1] <= 360.0 {
        (Completion::None, "no lateral symmetry (horizontal angles 0 to more than 180)", horiz.clone())
    } else {
        return Err(PhotometryError::Unsupported(format!(
            "IES: horizontal angles from {} to {} degrees are not one of the cases handled (a single angle, 0 to 90, 0 to 180, 90 to 270, or 0 to more than 180 up to 360).",
            horiz[0],
            horiz[nh - 1]
        )));
    };

    let mut planes: Vec<(f64, Vec<f64>)> = Vec::with_capacity(nh);
    for (k, a) in stored.iter().enumerate() {
        let row: Vec<f64> = candela[k * nv..(k + 1) * nv].iter().map(|v| v * scale).collect();
        planes.push((*a, row));
    }
    if completion == Completion::None && stored[nh - 1] == 360.0 {
        let (first, last) = (&planes[0].1, &planes[nh - 1].1);
        let worst = first.iter().zip(last).map(|(a, b)| (a - b).abs()).fold(0.0, f64::max);
        let peak = first.iter().cloned().fold(0.0, f64::max).max(1e-12);
        if worst > 0.01 * peak {
            warnings.push("The 360 degree plane differs from the 0 degree plane by more than 1 % of the peak; the 360 degree plane was dropped.".to_string());
        }
        planes.pop();
    }
    let (c_deg, rows) = complete_planes(planes, completion);
    let candela_full: Vec<f64> = rows.into_iter().flatten().collect();

    let to_m = match units {
        1.0 => Some(FEET_M),
        2.0 => Some(1.0),
        other => {
            warnings.push(format!("Units type {other} is neither 1 (feet) nor 2 (metres); the dimensions are not reported."));
            None
        }
    };
    let dimensions_m = to_m.map(|k| [length.abs() * k, width.abs() * k, height.abs() * k]);
    if width < 0.0 || length < 0.0 || height < 0.0 {
        warnings.push("A negative luminous-opening dimension denotes a shape in LM-63; absolute values are reported, the shape is not interpreted.".to_string());
    }

    let p = Photometry {
        format: SourceFormat::Ies { version },
        manufacturer,
        luminaire,
        catalogue,
        test_report,
        c_deg,
        gamma_deg: vert,
        candela: candela_full,
        lamp_flux_lm,
        absolute_photometry: absolute,
        lor_percent: None,
        dff_percent: None,
        input_power_w: Some(watts),
        ballast_factor: Some(ballast_factor),
        ballast_lamp_photometric_factor: Some(ballast_lamp),
        measured_tilt_deg: 0.0,
        dimensions_m,
        luminous_area_m: None,
        symmetry: symmetry.to_string(),
        warnings,
    };
    p.validate().map_err(PhotometryError::Parse)?;
    Ok(p)
}
