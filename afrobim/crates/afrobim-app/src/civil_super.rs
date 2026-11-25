// SPDX-License-Identifier: LicenseRef-AfroBIM-Proprietary
// Copyright (c) 2026 AfroBIM. All rights reserved.
//! The superelevation of an imported alignment: the table as the LandXML file wrote it is kept on
//! the alignment element, and the alignment's Properties show it beside the cross-falls the
//! engine derived from it, which are the ones a corridor lane that follows superelevation takes.

use afrobim_alignment::{critical_station_element, Alignment, LandXmlFile, LandXmlSuperelevationTable};
use afrobim_kernel::{Document, Element, ElementId};

use crate::ui_kit::{self, Col, Level};

/// The property that holds the table as written (hidden from the property set by its prefix).
pub const PROP_TABLE: &str = "civil.landxml.superelevation";

/// Keeps the file's superelevation table of `alignment` on the element, when the file has one.
pub fn keep_table(doc: &mut Document, id: ElementId, file: Option<&LandXmlFile>, alignment: &str) {
    let Some(table) = file.and_then(|f| f.superelevation_tables.iter().find(|t| t.alignment == alignment)) else { return };
    if table.non_empty() == 0 {
        return;
    }
    if let (Some(e), Ok(text)) = (doc.find_mut(id), serde_json::to_string(table)) {
        e.properties.insert(PROP_TABLE.into(), text);
    }
}

pub fn kept_table(e: &Element) -> Option<LandXmlSuperelevationTable> {
    serde_json::from_str(e.properties.get(PROP_TABLE)?).ok()
}

/// The rows of the table as written: record, critical station, station, full rate.
pub fn file_rows(t: &LandXmlSuperelevationTable, alignment: &Alignment) -> Vec<Vec<String>> {
    t.rows()
        .iter()
        .map(|r| vec![(r.record + 1).to_string(), critical_station_element(r.kind).to_string(), alignment.station_label(r.station_m, 3), r.full_rate_pct.map(|p| format!("{p:.2}")).unwrap_or_else(|| "-".into())])
        .collect()
}

/// The cross-falls the alignment holds, one row per stated station, in percent.
pub fn used_rows(alignment: &Alignment) -> Vec<Vec<String>> {
    let Some(se) = &alignment.superelevation else { return Vec::new() };
    let pct = |v: Option<f64>| v.map(|p| format!("{p:+.2}")).unwrap_or_else(|| "-".into());
    se.sections.iter().flat_map(|s| s.points.iter()).map(|p| vec![alignment.station_label(p.station_m, 3), pct(p.left_pct), pct(p.right_pct), p.kind.map(|k| critical_station_element(k).to_string()).unwrap_or_default()]).collect()
}

/// The Definition area of an alignment in the Properties panel: tabs Geometry and Superelevation.
/// Returns true when something was edited (nothing is: both tabs show).
pub fn alignment_tabs(ui: &mut egui::Ui, e: &mut Element, tab: &mut usize) -> bool {
    let Ok(alignment) = serde_json::from_value::<Alignment>(e.data.clone()) else {
        ui_kit::callout(ui, Level::Warning, "The alignment data cannot be read.");
        return false;
    };
    ui_kit::tabs(ui, tab, &["Geometry", "Superelevation", "Design check"]);
    if *tab == 0 {
        ui_kit::grid(ui, "al-geom", |ui| {
            let mut row = |k: &str, v: String| {
                ui.label(k);
                ui.label(v);
                ui.end_row();
            };
            row("Start station", alignment.station_label(alignment.start_station_m, 3));
            row("End station", alignment.station_label(alignment.end_station_m(), 3));
            row("Length", format!("{:.3} m", alignment.length()));
            row("Elements", alignment.elements.len().to_string());
            row("Vertical profile", alignment.profile.as_ref().map(|p| format!("{} PVIs", p.pvis.len())).unwrap_or_else(|| "none".into()));
            row("Station equations", alignment.station_equations.equations.len().to_string());
        });
        return false;
    }
    if *tab == 2 {
        return design_check_tab(ui, e, &alignment);
    }
    match (&alignment.superelevation, kept_table(e)) {
        (None, None) => {
            ui_kit::callout(ui, Level::Info, "This alignment has no superelevation. A corridor lane that follows superelevation takes the crossfall of its own template.");
        }
        (used, table) => {
            if let Some(t) = table {
                ui_kit::section(ui, "As the file states it", |ui| {
                    ui_kit::results_table(ui, "se-file", &["Record", "Station kind", "Station", "Full rate %"], &[Col::Num, Col::Text, Col::Num, Col::Num], &file_rows(&t, &alignment));
                });
            }
            if used.is_some() {
                ui_kit::section(ui, "Cross-fall the corridors use (percent, rising away from the centreline)", |ui| {
                    ui_kit::results_table(ui, "se-used", &["Station", "Left", "Right", "Kind"], &[Col::Num, Col::Num, Col::Num, Col::Text], &used_rows(&alignment));
                });
                ui_kit::note(ui, "A lane or shoulder set to follow superelevation takes these values, interpolated between the stations, at every section of a corridor on this alignment.");
            } else {
                ui_kit::callout(ui, Level::Warning, "The file states critical stations but no usable cross-fall: corridors cannot follow it.");
            }
        }
    }
    false
}

/// The property that holds the design speed chosen for the check of an alignment.
pub const PROP_SPEED: &str = "civil.design_speed_kmh";

/// The Design check tab: a design speed from the speeds the pack holds, and the report of the
/// engine's checks of the alignment against that row of the pack.
fn design_check_tab(ui: &mut egui::Ui, e: &mut Element, alignment: &Alignment) -> bool {
    let mut changed = false;
    let speeds = crate::defaults_glue::speeds_of(crate::civil_check::KEYS[1].1);
    let current: Option<f64> = e.properties.get(PROP_SPEED).and_then(|v| v.parse().ok());
    let mut chosen = current;
    ui_kit::form(ui, "al-check", |f| {
        f.row("Design speed", |ui| {
            egui::ComboBox::from_id_salt("al-speed").selected_text(chosen.map(|s| format!("{s} km/h")).unwrap_or_else(|| "not set".into())).show_ui(ui, |ui| {
                ui.selectable_value(&mut chosen, None, "not set");
                for s in &speeds {
                    ui.selectable_value(&mut chosen, Some(*s), format!("{s} km/h"));
                }
            });
        });
    });
    if chosen != current {
        match chosen {
            Some(s) => e.properties.insert(PROP_SPEED.into(), s.to_string()),
            None => e.properties.remove(PROP_SPEED),
        };
        changed = true;
    }
    let (report, notes) = crate::civil_check::check(alignment, chosen);
    for n in &notes {
        ui_kit::callout(ui, Level::Info, n.as_str());
    }
    if let Some(r) = report {
        ui_kit::note(ui, format!("{}. Limits from the {} defaults pack; a limit it does not hold at this speed is not checked.", crate::civil_check::summary(&r), crate::defaults_glue::basis_name()));
        let rows: Vec<Vec<String>> = r
            .items
            .iter()
            .map(|i| vec![i.location.clone(), i.description.clone(), i.value.map(|v| format!("{v:.2} {}", i.unit)).unwrap_or_else(|| "-".into()), i.limit.map(|v| format!("{v:.2}")).unwrap_or_else(|| "none".into()), format!("{:?}", i.status)])
            .collect();
        ui_kit::results_table(ui, "al-check-rows", &["Where", "Check", "Value", "Limit", "Result"], &[Col::Text, Col::Text, Col::Num, Col::Num, Col::Text], &rows);
    }
    changed
}
