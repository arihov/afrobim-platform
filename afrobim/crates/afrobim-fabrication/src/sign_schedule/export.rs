// SPDX-License-Identifier: LicenseRef-AfroBIM-Proprietary
// Copyright (c) 2026 AfroBIM. All rights reserved.
//! Writing the blank template and the model's signs in the schedule format.

use afrobim_alignment::{format_station, place_with, Alignment, PlaceOptions, SignLocation as AlignLocation, SignSide, StationReference, TrafficSide};
use afrobim_kernel::Document;
use afrobim_signs::{definition_from_entry, CatalogEntry, SignDefinition};

use super::columns::Field;
use super::csv::join_line;
use super::params::{read_params, takes_legend, write_legend, PARAMS};
use super::plan::fmt_num;
use super::{PROP_DESCRIPTION, PROP_MOUNTING_HEIGHT_MM, PROP_NOTES, PROP_SIZE_MM, SIGN_CATEGORY};

/// How the CSV is written.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CsvStyle {
    pub delimiter: char,
    /// Start the file with a UTF-8 byte order mark (Excel then reads UTF-8 correctly).
    pub bom: bool,
}

impl Default for CsvStyle {
    fn default() -> Self {
        CsvStyle { delimiter: ',', bom: false }
    }
}

fn finish(style: &CsvStyle, lines: Vec<String>) -> String {
    let mut out = String::new();
    if style.bom {
        out.push('\u{feff}');
    }
    for l in lines {
        out.push_str(&l);
        out.push('\n');
    }
    out
}

fn header_cells(params: &[&str]) -> Vec<String> {
    let mut h: Vec<String> = Field::FIXED.iter().map(Field::canonical_header).collect();
    h.extend(params.iter().map(|p| format!("param:{p}")));
    h
}

/// A blank schedule: the header row (every fixed column and every `param:` column) and one
/// commented example line. The example line starts with `#`, so an import skips it.
pub fn template_csv(style: &CsvStyle) -> String {
    let params: Vec<&str> = PARAMS.iter().map(|p| p.name).collect();
    let header = header_cells(&params);
    let example: Vec<String> = header
        .iter()
        .map(|h| match h.as_str() {
            "mark" => "#EXAMPLE-1",
            "code" => "<sign code of the rule pack>",
            "description" => "<free text>",
            "alignment" => "<alignment name>",
            "station" => "<k+mmm.mmm or metres>",
            "region" => "<optional: station region number>",
            "side" => "<left or right>",
            "offset_m" => "<metres from the centreline>",
            "mounting_height_mm" => "<optional, mm>",
            "size_mm" => "<optional, mm>",
            "legend" => "<Name@Direction#distance | Name@Direction>",
            "rotation_deg" => "<optional, degrees>",
            "notes" => "<free text>",
            _ => "<optional>",
        })
        .map(str::to_string)
        .collect();
    finish(style, vec![join_line(&header, style.delimiter), join_line(&example, style.delimiter)])
}

/// The result of [`export_csv`].
#[derive(Debug, Clone, PartialEq)]
pub struct Export {
    pub csv: String,
    /// How many signs were written.
    pub signs: usize,
    /// What the file cannot carry for some sign, or why a sign was not written. Nothing is
    /// dropped without a line here.
    pub warnings: Vec<String>,
}

/// Writes the document's signs (elements of category `sign`) in document order.
///
/// `alignments` turns a sign's stored internal station back into the displayed station and
/// region; `catalog` is the rule pack's (`SignEngine::catalog()`), against which a sign's
/// parameters are compared so that only the ones that differ from the catalog's seed are
/// written.
pub fn export_csv(doc: &Document, alignments: &[Alignment], catalog: &[CatalogEntry], style: &CsvStyle) -> Export {
    struct Row {
        fixed: Vec<String>,
        params: Vec<(String, String)>,
    }
    let mut warnings = Vec::new();
    let mut rows: Vec<Row> = Vec::new();
    for e in doc.elements().iter().filter(|e| e.category == SIGN_CATEGORY) {
        let mark = e.name.clone();
        let def: SignDefinition = match serde_json::from_value(e.data.clone()) {
            Ok(d) => d,
            Err(err) => {
                warnings.push(format!("{mark}: its definition cannot be read ({err}); not written"));
                continue;
            }
        };
        if mark.starts_with('#') {
            warnings.push(format!("{mark}: a mark that starts with '#' is read back as a comment line"));
        }
        let entry = def.catalog_key.as_deref().and_then(|k| catalog.iter().find(|c| c.key.eq_ignore_ascii_case(k)));
        let code = match entry {
            Some(c) => c.key.clone(),
            None => {
                warnings.push(format!("{mark}: it has no catalog code the loaded rule pack knows, so its code is blank and it cannot be imported back"));
                String::new()
            }
        };

        // Placement.
        let (mut alignment, mut station, mut region, mut side, mut offset, mut rotation) =
            (String::new(), String::new(), String::new(), String::new(), String::new(), String::new());
        match e.site.as_ref().filter(|s| s.alignment.is_some() && s.station_m.is_some()) {
            None => warnings.push(format!("{mark}: it is not placed on an alignment, so its placement columns are blank")),
            Some(s) => {
                let name = s.alignment.clone().unwrap_or_default();
                let internal = s.station_m.unwrap_or_default();
                let off = s.offset_m.unwrap_or_default();
                alignment = name.clone();
                side = if off < 0.0 { "left" } else { "right" }.to_string();
                offset = fmt_num(off.abs());
                match alignments.iter().find(|a| a.name.trim().eq_ignore_ascii_case(name.trim())) {
                    Some(a) => {
                        match a.displayed_station(internal) {
                            Some(d) => {
                                station = format_station(d.station_m, 3);
                                if !a.station_equations.is_empty() {
                                    region = d.region.to_string();
                                }
                            }
                            None => {
                                station = format_station(internal, 3);
                                warnings.push(format!("{mark}: its station is off alignment '{name}'; the internal station is written"));
                            }
                        }
                        // A rotation is written only when it differs from what the alignment gives.
                        let loc = AlignLocation {
                            alignment: Some(a.name.clone()),
                            station: Some(internal),
                            offset: Some(off.abs()),
                            side: Some(if off < 0.0 { SignSide::Left } else { SignSide::Right }),
                        };
                        let computed = place_with(a, &loc, TrafficSide::Left, &PlaceOptions { station: StationReference::Internal, ..PlaceOptions::default() })
                            .ok()
                            .and_then(|p| p.positions.first().map(|p| p.rotation_deg));
                        if computed.is_none_or(|c| (c - s.rotation_deg).rem_euclid(360.0).min(360.0 - (c - s.rotation_deg).rem_euclid(360.0)) > 0.01) {
                            rotation = fmt_num(s.rotation_deg.rem_euclid(360.0));
                        }
                    }
                    None => {
                        station = format_station(internal, 3);
                        warnings.push(format!("{mark}: alignment '{name}' was not given, so the internal station is written, which may differ from the displayed one"));
                    }
                }
            }
        }

        // Parameters that differ from the catalog's seed.
        let mut params = Vec::new();
        match entry {
            Some(c) => {
                let seed = read_params(&definition_from_entry(c, None));
                for (k, v) in read_params(&def) {
                    if seed.get(&k) != Some(&v) {
                        params.push((k, v));
                    }
                }
            }
            None => warnings.push(format!("{mark}: its parameters cannot be compared with a catalog entry and are not written")),
        }

        // Legend.
        let legend = if takes_legend(def.kind) && !def.destinations.is_empty() {
            let (text, lost) = write_legend(&def.destinations);
            for l in lost {
                warnings.push(format!("{mark}: the schedule cannot carry {l}"));
            }
            text
        } else {
            String::new()
        };

        let prop = |k: &str| e.properties.get(k).cloned().unwrap_or_default();
        let fixed: Vec<String> = Field::FIXED
            .iter()
            .map(|f| match f {
                Field::Mark => mark.clone(),
                Field::Code => code.clone(),
                Field::Description => prop(PROP_DESCRIPTION),
                Field::Alignment => alignment.clone(),
                Field::Station => station.clone(),
                Field::Region => region.clone(),
                Field::Side => side.clone(),
                Field::Offset => offset.clone(),
                Field::MountingHeight => prop(PROP_MOUNTING_HEIGHT_MM),
                Field::Size => prop(PROP_SIZE_MM),
                Field::Legend => legend.clone(),
                Field::Rotation => rotation.clone(),
                Field::Notes => prop(PROP_NOTES),
                Field::Param(_) => String::new(),
            })
            .collect();
        rows.push(Row { fixed, params });
    }

    // Only the parameter columns some sign needs, in the documented order.
    let used: Vec<&str> = PARAMS.iter().map(|p| p.name).filter(|n| rows.iter().any(|r| r.params.iter().any(|(k, _)| k == n))).collect();
    let mut lines = vec![join_line(&header_cells(&used), style.delimiter)];
    for r in &rows {
        let mut cells = r.fixed.clone();
        for p in &used {
            cells.push(r.params.iter().find(|(k, _)| k == p).map(|(_, v)| v.clone()).unwrap_or_default());
        }
        lines.push(join_line(&cells, style.delimiter));
    }
    Export { csv: finish(style, lines), signs: rows.len(), warnings }
}
