// SPDX-License-Identifier: LicenseRef-AfroBIM-Proprietary
// Copyright (c) 2026 AfroBIM. All rights reserved.
//! The IFC export options dialog: how alignments are written (`IfcOptions::alignment`). The choice
//! is kept in the document's project settings (`ifc.alignment`), so the next export starts from it.

use afrobim_ifc::AlignmentExport;
use afrobim_kernel::{project_setting, set_project_setting, Document};

use crate::app::AfroBimApp;

/// The project setting the choice is kept under.
pub const KEY: &str = "ifc.alignment";

pub const MODES: [AlignmentExport; 3] = [AlignmentExport::Simple, AlignmentExport::Geometry, AlignmentExport::Linear];

pub fn name(m: AlignmentExport) -> &'static str {
    match m {
        AlignmentExport::Simple => "Simple",
        AlignmentExport::Geometry => "Geometry",
        AlignmentExport::Linear => "Linear",
    }
}

/// One line on what each mode writes.
pub fn explain(m: AlignmentExport) -> &'static str {
    match m {
        AlignmentExport::Simple => "A plan polyline for each alignment and every element at an absolute position. Opens in every viewer. The default.",
        AlignmentExport::Geometry => "The full horizontal and vertical alignment as curve segments, with stations and superelevation events; elements keep absolute positions.",
        AlignmentExport::Linear => "As Geometry, and elements with a station and offset are placed along the alignment's curve. The viewer must evaluate placement along a curve; an absolute position is written as the fallback.",
    }
}

/// The mode stored in the document; Simple when none was chosen.
pub fn mode_of(doc: &Document) -> AlignmentExport {
    match project_setting(doc, KEY).as_ref().and_then(|v| v.as_str()) {
        Some("Geometry") => AlignmentExport::Geometry,
        Some("Linear") => AlignmentExport::Linear,
        _ => AlignmentExport::Simple,
    }
}

pub fn set_mode(doc: &mut Document, mode: AlignmentExport) {
    set_project_setting(doc, KEY, serde_json::Value::String(name(mode).to_string()));
}

impl AfroBimApp {
    pub(crate) fn ifc_options_window(&mut self, ctx: &egui::Context) {
        if !self.ifc_options_open {
            return;
        }
        let mut mode = mode_of(&self.doc);
        let (mut keep, mut export) = (true, false);
        let crs_line = crate::crs_ui::export_statement(afrobim_kernel::project_crs(&self.doc).as_ref());
        let out = crate::ui_kit::Dialog::titled("Export IFC").subtitle("IFC 4.3 (STEP) of the whole model").width(720.0).resizable(false).show(
            ctx,
            |ui| {
                crate::ui_kit::section(ui, "How alignments are written", |ui| {
                    for m in MODES {
                        ui.radio_value(&mut mode, m, name(m));
                        crate::ui_kit::note(ui, explain(m));
                        ui.add_space(2.0);
                    }
                    crate::ui_kit::note(ui, "The choice is kept in the document. Simple is the default and what earlier versions wrote.");
                });
                let rows = crate::civil_ifc::export_rows(&self.doc);
                if !rows.is_empty() {
                    crate::ui_kit::section(ui, "Civil objects: what is written and what is not", |ui| {
                        let table: Vec<Vec<String>> = rows.iter().map(|r| vec![r.label.clone(), r.count.to_string(), if r.written.is_empty() { "not mapped".to_string() } else { r.written.clone() }, r.not_mapped.clone()]).collect();
                        crate::ui_kit::results_table(ui, "ifc-civil", &["Object", "Count", "Written as", "Not mapped"], &[crate::ui_kit::Col::Text, crate::ui_kit::Col::Num, crate::ui_kit::Col::Text, crate::ui_kit::Col::Text], &table);
                        crate::ui_kit::note(ui, "Every entity and type was checked against the IFC4X3_ADD2 schema. The Geometry or Linear alignment mode writes the vertical and superelevation segments of the alignments.");
                    });
                }
                crate::ui_kit::section(ui, "Coordinate reference", |ui| {
                    crate::ui_kit::note(ui, crs_line.clone());
                });
            },
            |ui| crate::ui_kit::Footer::new("Choose the file and export...").show(ui),
        );
        if let Some((_, f)) = &out.inner {
            export = f.primary;
            if f.cancel {
                keep = false;
            }
        }
        if out.closed {
            keep = false;
        }
        if mode != mode_of(&self.doc) {
            set_mode(&mut self.doc, mode);
            self.schedule_commit_now();
        }
        if export {
            if let Some(mut path) = rfd::FileDialog::new().add_filter("IFC 4.3 (STEP)", &["ifc"]).set_file_name("model.ifc").save_file() {
                if path.extension().is_none() {
                    path.set_extension("ifc");
                }
                self.export_ifc(path);
                keep = false;
            }
        }
        self.ifc_options_open = keep;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_choice_defaults_to_simple_and_is_remembered_through_save_and_open() {
        let mut doc = Document::default();
        assert_eq!(mode_of(&doc), AlignmentExport::Simple);
        for m in MODES {
            set_mode(&mut doc, m);
            let reopened = Document::from_json(&doc.to_json()).unwrap();
            assert_eq!(mode_of(&reopened), m);
        }
    }
}
