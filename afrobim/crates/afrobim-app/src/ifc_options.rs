//! The IFC export options dialog: how alignments are written (`IfcOptions::alignment`). The choice
//! is kept in the document's project settings (`ifc.alignment`), so the next export starts from it.

use afrobim_ifc::AlignmentExport;
use afrobim_kernel::{project_setting, set_project_setting, Document};
use egui::RichText;

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
        crate::win::new(ctx, "Export IFC").collapsible(false).resizable(false).show(ctx, |ui| {
            ui.label(RichText::new("How alignments are written").strong());
            for m in MODES {
                ui.radio_value(&mut mode, m, name(m));
                ui.label(RichText::new(explain(m)).small().weak());
            }
            ui.add_space(4.0);
            ui.label(RichText::new("The choice is kept in the document. Simple is the default and what earlier versions wrote.").small().weak());
            ui.add_space(6.0);
            ui.label(RichText::new("Coordinate reference").strong());
            ui.add(egui::Label::new(RichText::new(crate::crs_ui::export_statement(afrobim_kernel::project_crs(&self.doc).as_ref())).small().weak()).wrap());
            ui.add_space(6.0);
            ui.horizontal(|ui| {
                if ui.button("Choose the file and export…").clicked() {
                    export = true;
                }
                if ui.button("Cancel").clicked() {
                    keep = false;
                }
            });
        });
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
