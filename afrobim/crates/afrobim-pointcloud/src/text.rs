//! Delimited text point files: XYZ, XYZ with intensity, XYZRGB, PTS and any other column order.
//!
//! [`detect_text_format`] reads the start of a file and proposes a delimiter, the number of lines
//! to skip, a column mapping and a preview of the first rows. The proposal is only a guess: the
//! caller shows the preview and may change [`TextFormat`] before reading.
//!
//! Limits: numbers use a decimal point (a decimal comma is not read); lines starting with `#`
//! or `//` are comments; one point per line.

use crate::error::{PointCloudError, Result};
use serde::{Deserialize, Serialize};
use std::io::{BufRead, BufReader};
use std::path::Path;

/// What a column holds.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TextRole {
    X,
    Y,
    Z,
    Intensity,
    Red,
    Green,
    Blue,
    Class,
    ReturnNumber,
    NumberOfReturns,
    GpsTime,
    Ignore,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Delimiter {
    /// Choose from the first lines (tab, semicolon, comma, then runs of white space).
    Auto,
    Comma,
    Semicolon,
    Tab,
    /// One or more spaces or tabs.
    Whitespace,
}

/// The common layouts, for the user to pick or for a detected guess.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TextLayout {
    Xyz,
    XyzIntensity,
    XyzRgb,
    /// Leica PTS: a first line with the point count, then `x y z intensity r g b`.
    Pts,
    XyzIntensityRgb,
    /// Columns named by the header line.
    Named,
    Custom,
}

/// How to read a text file.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct TextFormat {
    pub delimiter: Delimiter,
    /// Lines to skip at the start (a header line, the PTS count line).
    pub skip_lines: usize,
    /// One role per column, left to right. Columns beyond the list are ignored.
    pub columns: Vec<TextRole>,
    /// Stored intensity = `round(value * intensity_scale + intensity_offset)` clamped to 0..=65535.
    /// Both are inputs: file conventions differ (0..255, 0..65535, 0..1, PTS -2048..2047).
    pub intensity_scale: f64,
    pub intensity_offset: f64,
    /// Largest colour value in the file (255 or 65535 typically); stored colour is
    /// `value / rgb_max * 65535`. An input: it cannot be known from a few rows.
    pub rgb_max: f64,
}

impl TextFormat {
    /// `x y z` in the first three columns.
    pub fn xyz() -> TextFormat {
        TextFormat::with_columns(vec![TextRole::X, TextRole::Y, TextRole::Z])
    }

    pub fn with_columns(columns: Vec<TextRole>) -> TextFormat {
        TextFormat { delimiter: Delimiter::Auto, skip_lines: 0, columns, intensity_scale: 1.0, intensity_offset: 0.0, rgb_max: 255.0 }
    }

    /// The columns of a layout (`Named` and `Custom` give `x y z`).
    pub fn for_layout(layout: TextLayout) -> TextFormat {
        use TextRole::*;
        match layout {
            TextLayout::Xyz | TextLayout::Named | TextLayout::Custom => TextFormat::xyz(),
            TextLayout::XyzIntensity => TextFormat::with_columns(vec![X, Y, Z, Intensity]),
            TextLayout::XyzRgb => TextFormat::with_columns(vec![X, Y, Z, Red, Green, Blue]),
            TextLayout::XyzIntensityRgb => TextFormat::with_columns(vec![X, Y, Z, Intensity, Red, Green, Blue]),
            TextLayout::Pts => {
                let mut f = TextFormat::with_columns(vec![X, Y, Z, Intensity, Red, Green, Blue]);
                f.skip_lines = 1;
                f
            }
        }
    }

    pub(crate) fn validate(&self) -> Result<()> {
        for r in [TextRole::X, TextRole::Y, TextRole::Z] {
            if !self.columns.contains(&r) {
                return Err(PointCloudError::Invalid(format!("the column mapping has no {r:?} column")));
            }
        }
        if !(self.rgb_max.is_finite() && self.rgb_max > 0.0) || !self.intensity_scale.is_finite() || !self.intensity_offset.is_finite() {
            return Err(PointCloudError::Invalid("the intensity scale, offset and colour maximum must be finite, and the colour maximum positive".into()));
        }
        Ok(())
    }

    pub(crate) fn has_role(&self, r: TextRole) -> bool {
        self.columns.contains(&r)
    }
}

/// What detection found.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct TextDetection {
    /// The delimiter chosen (never `Auto`).
    pub delimiter: Delimiter,
    pub columns_found: usize,
    /// Names from a header line, if there was one.
    pub header_names: Option<Vec<String>>,
    pub layout: TextLayout,
    /// The proposed reading format. Review it against `preview`.
    pub format: TextFormat,
    /// The first data rows as text fields, as split.
    pub preview: Vec<Vec<String>>,
    /// Plain-language notes about guesses (for example "colour values go above 255: rgb_max set to 65535").
    pub notes: Vec<String>,
}

pub(crate) fn split_fields<'a>(line: &'a str, d: Delimiter) -> Vec<&'a str> {
    match d {
        Delimiter::Comma | Delimiter::Auto => line.split(',').map(str::trim).collect(),
        Delimiter::Semicolon => line.split(';').map(str::trim).collect(),
        Delimiter::Tab => line.split('\t').map(str::trim).collect(),
        Delimiter::Whitespace => line.split_whitespace().collect(),
    }
}

fn is_comment(line: &str) -> bool {
    let t = line.trim_start();
    t.is_empty() || t.starts_with('#') || t.starts_with("//")
}

fn role_from_name(name: &str) -> TextRole {
    match name.trim().trim_matches('"').to_ascii_lowercase().as_str() {
        "x" | "e" | "east" | "easting" | "lon" | "longitude" => TextRole::X,
        "y" | "n" | "north" | "northing" | "lat" | "latitude" => TextRole::Y,
        "z" | "h" | "height" | "elevation" | "elev" | "alt" | "altitude" => TextRole::Z,
        "i" | "intensity" | "inten" | "refl" | "reflectance" => TextRole::Intensity,
        "r" | "red" => TextRole::Red,
        "g" | "green" => TextRole::Green,
        "b" | "blue" => TextRole::Blue,
        "c" | "class" | "classification" => TextRole::Class,
        "returnnumber" | "return_number" | "return" => TextRole::ReturnNumber,
        "numberofreturns" | "number_of_returns" => TextRole::NumberOfReturns,
        "gpstime" | "gps_time" | "time" => TextRole::GpsTime,
        _ => TextRole::Ignore,
    }
}

/// Reads up to `max_lines` lines from the start of the file and proposes how to read it.
pub fn detect_text_format(path: &Path, preview_rows: usize) -> Result<TextDetection> {
    let file = std::fs::File::open(path)?;
    let mut lines: Vec<String> = Vec::new();
    for l in BufReader::new(file).lines().take(80) {
        let l = l?;
        if !is_comment(&l) {
            lines.push(l);
        }
    }
    if lines.is_empty() {
        return Err(PointCloudError::Format("the text file has no data lines".into()));
    }
    let mut notes = Vec::new();
    // PTS: first line is one integer and the next lines have 4 or 7 fields.
    let first_tokens: Vec<&str> = lines[0].split_whitespace().collect();
    let pts = first_tokens.len() == 1 && first_tokens[0].parse::<u64>().is_ok() && lines.len() > 1 && matches!(lines[1].split_whitespace().count(), 4 | 7 | 6);
    let body_start = if pts { 1 } else { 0 };
    // Delimiter: the first of tab, semicolon, comma, whitespace that gives the same field count >= 3 on every line.
    let probe: Vec<&String> = lines.iter().skip(body_start + 1).take(20).collect();
    let probe: Vec<&String> = if probe.is_empty() { lines.iter().skip(body_start).collect() } else { probe };
    let mut chosen = None;
    for d in [Delimiter::Tab, Delimiter::Semicolon, Delimiter::Comma, Delimiter::Whitespace] {
        let counts: Vec<usize> = probe.iter().map(|l| split_fields(l, d).len()).collect();
        if counts[0] >= 3 && counts.iter().all(|&c| c == counts[0]) {
            chosen = Some((d, counts[0]));
            break;
        }
    }
    let (delimiter, ncols) = chosen.ok_or_else(|| PointCloudError::Format("no delimiter gives three or more columns on every line".into()))?;
    // Header line: the first line has a field that does not parse as a number.
    let first_fields = split_fields(&lines[body_start], delimiter);
    let header_names = if first_fields.iter().any(|f| f.parse::<f64>().is_err()) { Some(first_fields.iter().map(|s| s.trim_matches('"').to_string()).collect::<Vec<_>>()) } else { None };
    let skip_lines = if pts { 1 } else if header_names.is_some() { 1 } else { 0 };
    // Count comment lines before the data as skipped too: recount against the raw file.
    let data_rows: Vec<Vec<String>> = lines.iter().skip(body_start + usize::from(header_names.is_some())).take(preview_rows).map(|l| split_fields(l, delimiter).iter().map(|s| s.to_string()).collect()).collect();

    let (layout, mut format) = if let Some(names) = &header_names {
        let cols: Vec<TextRole> = names.iter().map(|n| role_from_name(n)).collect();
        if cols.contains(&TextRole::X) && cols.contains(&TextRole::Y) && cols.contains(&TextRole::Z) {
            (TextLayout::Named, TextFormat::with_columns(cols))
        } else {
            notes.push("the header names did not include x, y and z: the first three columns are used".into());
            (TextLayout::Xyz, TextFormat::xyz())
        }
    } else if pts {
        (TextLayout::Pts, TextFormat::for_layout(TextLayout::Pts))
    } else {
        match ncols {
            3 => (TextLayout::Xyz, TextFormat::for_layout(TextLayout::Xyz)),
            4 => (TextLayout::XyzIntensity, TextFormat::for_layout(TextLayout::XyzIntensity)),
            6 => {
                // x y z then three values in [-1, 1] with fractions are most likely normals.
                let normals = data_rows.iter().all(|r| r[3..6].iter().all(|f| f.parse::<f64>().is_ok_and(|v| v.abs() <= 1.0 && v.fract() != 0.0)));
                if normals {
                    notes.push("columns 4 to 6 look like normals (fractions within -1 and 1): ignored".into());
                    (TextLayout::Xyz, TextFormat::for_layout(TextLayout::Xyz))
                } else {
                    (TextLayout::XyzRgb, TextFormat::for_layout(TextLayout::XyzRgb))
                }
            }
            7 => (TextLayout::XyzIntensityRgb, TextFormat::for_layout(TextLayout::XyzIntensityRgb)),
            n => {
                notes.push(format!("{n} columns: only x y z were mapped; set the column roles by hand"));
                (TextLayout::Custom, TextFormat::xyz())
            }
        }
    };
    format.delimiter = delimiter;
    format.skip_lines = skip_lines;
    // Colour range guess from the preview rows.
    let mut max_rgb = 0.0f64;
    for r in &data_rows {
        for (i, role) in format.columns.iter().enumerate() {
            if matches!(role, TextRole::Red | TextRole::Green | TextRole::Blue) {
                if let Some(v) = r.get(i).and_then(|s| s.parse::<f64>().ok()) {
                    max_rgb = max_rgb.max(v);
                }
            }
        }
    }
    if max_rgb > 255.0 {
        format.rgb_max = 65535.0;
        notes.push("colour values go above 255 in the preview: rgb_max set to 65535 (a guess from the preview rows)".into());
    } else if max_rgb > 0.0 {
        notes.push("colour values stay within 255 in the preview: rgb_max left at 255 (a guess from the preview rows; confirm)".into());
    }
    if format.has_role(TextRole::Intensity) {
        notes.push("intensity is stored as value * intensity_scale + intensity_offset (scale 1, offset 0 now); set them to match the file".into());
    }
    Ok(TextDetection { delimiter, columns_found: ncols, header_names, layout, format, preview: data_rows, notes })
}

/// Parses one data line with a resolved delimiter into a record. Returns `Ok(None)` for comment
/// and blank lines.
pub(crate) fn parse_line(line: &str, fmt: &TextFormat, delimiter: Delimiter, line_no: usize) -> Result<Option<crate::buffer::PointRecord>> {
    if is_comment(line) {
        return Ok(None);
    }
    let fields = split_fields(line, delimiter);
    let mut rec = crate::buffer::PointRecord::default();
    let mut rgb = [0u16; 3];
    let mut has_rgb = false;
    let mut have = [false; 3];
    for (i, role) in fmt.columns.iter().enumerate() {
        if *role == TextRole::Ignore {
            continue;
        }
        let f = fields.get(i).ok_or_else(|| PointCloudError::Text { line: line_no, message: format!("expected at least {} columns, found {}", fmt.columns.len(), fields.len()) })?;
        let v: f64 = f.parse().map_err(|_| PointCloudError::Text { line: line_no, message: format!("column {} is not a number: {:?}", i + 1, f) })?;
        if !v.is_finite() {
            return Err(PointCloudError::Text { line: line_no, message: format!("column {} is not finite", i + 1) });
        }
        match role {
            TextRole::X => {
                rec.xyz[0] = v;
                have[0] = true;
            }
            TextRole::Y => {
                rec.xyz[1] = v;
                have[1] = true;
            }
            TextRole::Z => {
                rec.xyz[2] = v;
                have[2] = true;
            }
            TextRole::Intensity => rec.intensity = (v * fmt.intensity_scale + fmt.intensity_offset).round().clamp(0.0, 65535.0) as u16,
            TextRole::Red | TextRole::Green | TextRole::Blue => {
                has_rgb = true;
                let k = match role {
                    TextRole::Red => 0,
                    TextRole::Green => 1,
                    _ => 2,
                };
                rgb[k] = (v / fmt.rgb_max * 65535.0).round().clamp(0.0, 65535.0) as u16;
            }
            TextRole::Class => rec.class = v.round().clamp(0.0, 255.0) as u8,
            TextRole::ReturnNumber => rec.return_number = v.round().clamp(0.0, 15.0) as u8,
            TextRole::NumberOfReturns => rec.number_of_returns = v.round().clamp(0.0, 15.0) as u8,
            TextRole::GpsTime => rec.gps_time = Some(v),
            TextRole::Ignore => {}
        }
    }
    if !have.iter().all(|&h| h) {
        return Err(PointCloudError::Text { line: line_no, message: "x, y and z were not all read".into() });
    }
    if has_rgb {
        rec.rgb = Some(rgb);
    }
    Ok(Some(rec))
}

pub(crate) fn resolve_delimiter(path: &Path, fmt: &TextFormat) -> Result<Delimiter> {
    if fmt.delimiter != Delimiter::Auto {
        return Ok(fmt.delimiter);
    }
    Ok(detect_text_format(path, 5)?.delimiter)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;

    fn file(name: &str, content: &str) -> std::path::PathBuf {
        let dir = std::env::temp_dir().join(format!("afrobim-pc-text-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let p = dir.join(name);
        std::fs::File::create(&p).unwrap().write_all(content.as_bytes()).unwrap();
        p
    }

    #[test]
    fn detects_pts_xyz_header_and_semicolon() {
        let p = file("a.pts", "3\n500000.5 9960000.25 12.5 -100 10 20 30\n500001 9960001 12.6 -90 11 21 31\n500002 9960002 12.7 -80 12 22 32\n");
        let d = detect_text_format(&p, 5).unwrap();
        assert_eq!(d.layout, TextLayout::Pts);
        assert_eq!(d.format.skip_lines, 1);
        assert_eq!(d.preview.len(), 3);
        assert_eq!(d.delimiter, Delimiter::Whitespace);

        let p = file("b.csv", "X,Y,Z,Intensity\n1,2,3,40\n4,5,6,50\n");
        let d = detect_text_format(&p, 5).unwrap();
        assert_eq!(d.layout, TextLayout::Named);
        assert_eq!(d.delimiter, Delimiter::Comma);
        assert_eq!(d.format.columns, vec![TextRole::X, TextRole::Y, TextRole::Z, TextRole::Intensity]);
        assert_eq!(d.format.skip_lines, 1);

        let p = file("c.txt", "1;2;3;300;400;500\n4;5;6;310;410;510\n");
        let d = detect_text_format(&p, 5).unwrap();
        assert_eq!(d.layout, TextLayout::XyzRgb);
        assert_eq!(d.format.rgb_max, 65535.0);
        assert!(!d.notes.is_empty());
    }

    #[test]
    fn six_columns_of_fractions_are_normals() {
        let p = file("n.xyz", "1 2 3 0.5 0.5 0.7\n4 5 6 -0.5 0.25 0.8\n");
        let d = detect_text_format(&p, 5).unwrap();
        assert_eq!(d.layout, TextLayout::Xyz);
    }

    #[test]
    fn parse_line_scales_intensity_and_colour() {
        let mut f = TextFormat::for_layout(TextLayout::XyzIntensityRgb);
        f.delimiter = Delimiter::Whitespace;
        f.intensity_scale = 2.0;
        f.intensity_offset = 10.0;
        let r = parse_line("1.5 2.5 3.5 100 255 0 255", &f, Delimiter::Whitespace, 1).unwrap().unwrap();
        assert_eq!(r.intensity, 210);
        assert_eq!(r.rgb, Some([65535, 0, 65535]));
        assert!(parse_line("1 2", &f, Delimiter::Whitespace, 7).is_err());
        assert!(parse_line("# c", &f, Delimiter::Whitespace, 1).unwrap().is_none());
        assert!(TextFormat::with_columns(vec![TextRole::X]).validate().is_err());
    }
}
