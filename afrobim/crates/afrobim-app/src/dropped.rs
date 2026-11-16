//! Files dropped on the window.
//!
//! On macOS, double-clicking an `.afrobim` file in Finder (or dropping one on the Dock icon) sends
//! the application an open-document event (`application:openURLs:`), not a command-line argument.
//! The framework versions the app is built on do not pass that event on: winit 0.30.13 registers
//! an application delegate that handles only `applicationDidFinishLaunching:` and
//! `applicationWillTerminate:` (`platform_impl/macos/app_state.rs`), and eframe 0.31.1 has no
//! open-file API. Catching the event would need a custom Objective-C delegate. So the app supports
//! the one route every platform has: a file dropped onto the window. Each dropped file is sorted
//! by its extension and, before anything happens, the engineer is asked to confirm what will be
//! done with it. A document with unsaved changes open is named in the question.

use std::path::{Path, PathBuf};


/// What a dropped file is, by its extension.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum DropKind {
    /// An AfroBIM document: replaces the open one.
    Document,
    /// LandXML: its alignments are added to the open document.
    LandXml,
    /// DXF drawing: attached as a read-only underlay.
    Underlay,
    /// IFC: imported as a reference model.
    Reference,
    /// Anything else.
    Unsupported,
}

/// Sorts a path by its extension, ignoring case.
pub fn classify(path: &Path) -> DropKind {
    match path.extension().and_then(|e| e.to_str()).map(|e| e.to_ascii_lowercase()).as_deref() {
        Some("afrobim") => DropKind::Document,
        Some("xml") => DropKind::LandXml,
        Some("dxf") => DropKind::Underlay,
        Some("ifc") => DropKind::Reference,
        _ => DropKind::Unsupported,
    }
}

/// True when opening a document would lose work: the document has changed since it was opened or
/// saved. `saved` is the fingerprint then (`None` for a document never opened or saved, which has
/// unsaved work when it holds any element).
pub fn has_unsaved(saved: Option<u64>, now: u64, elements: usize) -> bool {
    match saved {
        Some(fp) => fp != now,
        None => elements > 0,
    }
}

/// The question put to the engineer: a title, what will happen, and the confirming button.
pub fn describe(kind: DropKind, path: &Path, unsaved: bool) -> (String, String, &'static str) {
    let name = path.file_name().map(|n| n.to_string_lossy().to_string()).unwrap_or_else(|| path.display().to_string());
    match kind {
        DropKind::Document => {
            let warn = if unsaved { " The open document has unsaved changes, and they will be lost." } else { "" };
            (format!("Open {name}?"), format!("Open this AfroBIM document in place of the one now open.{warn}"), "Open")
        }
        DropKind::LandXml => (format!("Import {name}?"), "Read the horizontal alignments in this LandXML file and add them to the open document.".into(), "Import alignments"),
        DropKind::Underlay => (format!("Attach {name}?"), "Attach this DXF drawing as a read-only underlay. It is read from its file in the background; the document keeps its path and hash, not a copy.".into(), "Attach underlay"),
        DropKind::Reference => (format!("Import {name}?"), "Import this IFC file as a reference model, shown in the 3D view and checked for clashes. The model itself is not changed.".into(), "Import reference model"),
        DropKind::Unsupported => (format!("{name} cannot be used"), "AfroBIM opens .afrobim documents and imports .xml (LandXML), .dxf (underlay) and .ifc (reference model) files.".into(), "OK"),
    }
}

/// The dropped files waiting to be confirmed, one at a time, and the saved state of the document.
#[derive(Default)]
pub struct DropUi {
    pub queue: Vec<(PathBuf, DropKind)>,
    /// The document's fingerprint when last opened, saved or started new.
    pub saved: Option<u64>,
}

impl DropUi {
    pub fn add(&mut self, paths: impl IntoIterator<Item = PathBuf>) {
        self.queue.extend(paths.into_iter().map(|p| {
            let k = classify(&p);
            (p, k)
        }));
    }
}

impl crate::app::AfroBimApp {
    /// Once a frame: takes files dropped on the window and asks about the first waiting one.
    pub(crate) fn drop_tick(&mut self, ctx: &egui::Context) {
        let dropped: Vec<PathBuf> = ctx.input(|i| i.raw.dropped_files.iter().filter_map(|f| f.path.clone()).collect());
        self.drops.add(dropped);
        let Some((path, kind)) = self.drops.queue.first().cloned() else { return };
        let unsaved = kind == DropKind::Document && has_unsaved(self.drops.saved, crate::background::fingerprint(&self.doc), self.doc.elements().len());
        let (title, text, button) = describe(kind, &path, unsaved);
        let mut answer = None;
        let waiting = self.drops.queue.len().saturating_sub(1);
        let unsupported = kind == DropKind::Unsupported;
        let out = crate::ui_kit::Dialog::titled("Dropped file").subtitle(title).width(460.0).resizable(false).show(
            ctx,
            |ui| {
                crate::ui_kit::callout(ui, if unsupported { crate::ui_kit::Level::Warning } else { crate::ui_kit::Level::Info }, text);
                crate::ui_kit::note(ui, path.display().to_string());
                if waiting > 0 {
                    crate::ui_kit::note(ui, format!("{waiting} more file(s) waiting."));
                }
            },
            |ui| crate::ui_kit::Footer::new(button).cancel(if unsupported { None } else { Some("Cancel") }).show(ui),
        );
        if let Some((_, f)) = &out.inner {
            if f.primary {
                answer = Some(true);
            } else if f.cancel {
                answer = Some(false);
            }
        }
        if out.closed {
            answer = Some(false);
        }
        let Some(yes) = answer else { return };
        self.drops.queue.remove(0);
        if !yes || kind == DropKind::Unsupported {
            return;
        }
        match kind {
            DropKind::Document => self.open_path(path),
            DropKind::LandXml => self.import_landxml(ctx, path),
            DropKind::Underlay => match self.under.attach(&mut self.doc, &path) {
                Ok(id) => {
                    self.under.selected = Some(id);
                    self.under.window_open = true;
                    self.status = format!("Attached {}: reading in the background.", path.display());
                    self.schedule_commit_now();
                }
                Err(e) => self.status = e,
            },
            DropKind::Reference => {
                self.refs.start_import(&path, &[]);
                self.refs.list_open = true;
            }
            DropKind::Unsupported => {}
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn kind(p: &str) -> DropKind {
        classify(Path::new(p))
    }

    #[test]
    fn a_file_is_sorted_by_its_extension_whatever_the_case() {
        assert_eq!(kind("/a/b/Road.afrobim"), DropKind::Document);
        assert_eq!(kind("ROAD.AFROBIM"), DropKind::Document);
        assert_eq!(kind("corridor.xml"), DropKind::LandXml);
        assert_eq!(kind("C:\\x\\Plan.DXF"), DropKind::Underlay);
        assert_eq!(kind("bridge.ifc"), DropKind::Reference);
        assert_eq!(kind("bridge.IFC"), DropKind::Reference);
    }

    #[test]
    fn other_files_and_files_without_an_extension_are_unsupported() {
        for p in ["notes.txt", "photo.png", "archive.afrobim.zip", "README", "dwg.dwg", ".afrobim", "ifc"] {
            assert_eq!(kind(p), DropKind::Unsupported, "{p}");
        }
    }

    #[test]
    fn unsaved_work_is_a_changed_fingerprint_or_a_never_saved_document_with_elements() {
        assert!(!has_unsaved(Some(7), 7, 30));
        assert!(has_unsaved(Some(7), 8, 30));
        assert!(!has_unsaved(None, 1, 0));
        assert!(has_unsaved(None, 1, 3));
    }

    #[test]
    fn the_question_says_what_will_happen_and_warns_of_unsaved_work_only_for_a_document() {
        let (t, body, b) = describe(DropKind::Document, Path::new("/x/Road.afrobim"), true);
        assert!(t.contains("Road.afrobim") && body.contains("unsaved changes") && b == "Open");
        let (_, body, _) = describe(DropKind::Document, Path::new("/x/Road.afrobim"), false);
        assert!(!body.contains("unsaved"));
        let (_, body, b) = describe(DropKind::LandXml, Path::new("a.xml"), true);
        assert!(body.contains("alignments") && !body.contains("unsaved") && b == "Import alignments");
        assert!(describe(DropKind::Underlay, Path::new("a.dxf"), false).1.contains("underlay"));
        assert!(describe(DropKind::Reference, Path::new("a.ifc"), false).1.contains("reference model"));
        assert!(describe(DropKind::Unsupported, Path::new("a.txt"), false).1.contains(".afrobim"));
    }

    #[test]
    fn dropped_paths_are_queued_in_order_with_their_kind() {
        let mut ui = DropUi::default();
        ui.add([PathBuf::from("a.dxf"), PathBuf::from("b.afrobim"), PathBuf::from("c.bin")]);
        let kinds: Vec<DropKind> = ui.queue.iter().map(|q| q.1).collect();
        assert_eq!(kinds, vec![DropKind::Underlay, DropKind::Document, DropKind::Unsupported]);
    }
}
