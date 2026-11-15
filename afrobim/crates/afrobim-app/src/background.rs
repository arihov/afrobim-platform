//! Slow work off the UI thread, with a progress indicator and a Cancel button.
//!
//! A job runs on a worker thread and hands one result back over a channel, as the reference model
//! and underlay loading do. The result is applied only if the document is the same as when the job
//! started: a fingerprint of the document is taken at the start and compared at the end, and a job
//! whose document changed underneath is discarded and says so. Cancel discards the result; the
//! worker itself cannot be interrupted part-way (the engines have no cancellation point), so it
//! finishes in the background and its result is thrown away.
//!
//! A worker cannot share the app's module registry (modules are not `Send`), so a job that needs
//! one builds its own with [`worker_registry`].

use std::collections::hash_map::DefaultHasher;
use std::hash::{Hash, Hasher};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{channel, Receiver, TryRecvError};
use std::sync::Arc;
use std::time::Instant;

use afrobim_kernel::{Document, ModuleRegistry};

/// A fingerprint of the document's content: equal fingerprints mean no edit happened in between.
pub fn fingerprint(doc: &Document) -> u64 {
    let mut h = DefaultHasher::new();
    doc.to_json().hash(&mut h);
    h.finish()
}

/// What polling a job found.
#[derive(Debug, PartialEq)]
pub enum Poll<T> {
    Running,
    /// Finished with the document unchanged: apply it.
    Done(T),
    /// The engineer pressed Cancel; the result was discarded.
    Cancelled,
    /// The document changed while the job ran; the result was discarded.
    Stale,
    /// The worker stopped without a result.
    Lost,
}

/// One running job.
pub struct Job<T> {
    pub label: String,
    fingerprint: u64,
    rx: Receiver<T>,
    cancel: Arc<AtomicBool>,
    started: Instant,
}

impl<T: Send + 'static> Job<T> {
    /// Starts `work` on a worker thread. `fingerprint` is the document's at this moment.
    pub fn spawn(label: &str, fingerprint: u64, work: impl FnOnce() -> T + Send + 'static) -> Job<T> {
        let (tx, rx) = channel();
        std::thread::spawn(move || {
            let _ = tx.send(work());
        });
        Job { label: label.to_string(), fingerprint, rx, cancel: Arc::new(AtomicBool::new(false)), started: Instant::now() }
    }

    pub fn cancel(&self) {
        self.cancel.store(true, Ordering::SeqCst);
    }

    pub fn elapsed_s(&self) -> f64 {
        self.started.elapsed().as_secs_f64()
    }

    /// Looks for the result. The document's fingerprint is only computed when one has arrived.
    pub fn poll(&self, doc: &Document) -> Poll<T> {
        match self.rx.try_recv() {
            Err(TryRecvError::Empty) => {
                if self.cancel.load(Ordering::SeqCst) {
                    Poll::Cancelled
                } else {
                    Poll::Running
                }
            }
            Err(TryRecvError::Disconnected) => Poll::Lost,
            Ok(_) if self.cancel.load(Ordering::SeqCst) => Poll::Cancelled,
            Ok(t) => {
                if fingerprint(doc) == self.fingerprint {
                    Poll::Done(t)
                } else {
                    Poll::Stale
                }
            }
        }
    }
}

/// A corridor's 3D surfaces, built for the element data they were built from.
pub struct CorridorBuilt {
    pub id: afrobim_kernel::ElementId,
    pub key: serde_json::Value,
    pub solids: Result<crate::road_glue::CorridorSolids, String>,
}

/// The corridors whose 3D surfaces are missing or were built from other data than the element has
/// now. `cached` gives the data a corridor's cached surfaces were built from.
pub fn stale_corridors(doc: &Document, cached: &dyn Fn(afrobim_kernel::ElementId) -> Option<serde_json::Value>) -> Vec<afrobim_kernel::ElementId> {
    doc.elements().iter().filter(|e| e.category == afrobim_corridor::CORRIDOR_CATEGORY && cached(e.id).as_ref() != Some(&e.data)).map(|e| e.id).collect()
}

/// Rebuilds the surfaces of the corridors `ids` on a worker thread. The worker works on a copy of
/// the document taken here, so the surfaces are the ones of the data that was current at this
/// moment; they are applied only if the document is still the same.
pub fn spawn_corridors(doc: &Document, ids: Vec<afrobim_kernel::ElementId>) -> Job<Vec<CorridorBuilt>> {
    let copy = doc.clone();
    Job::spawn("Corridor rebuild", fingerprint(doc), move || {
        ids.into_iter()
            .filter_map(|id| {
                let key = copy.find(id)?.data.clone();
                Some(CorridorBuilt { id, key, solids: crate::road_glue::corridor_solids(&copy, id) })
            })
            .collect()
    })
}

/// The jobs the app may have running, at most one of each kind.
#[derive(Default)]
pub struct Background {
    pub clash: Option<Job<crate::coordination_ui::ClashOut>>,
    pub pack: Option<Job<(crate::report_pack::Pack, usize)>>,
    /// The sign visibility scene build and run, for one sign or the road.
    pub vis: Option<Job<crate::visibility_ui::VisOut>>,
    /// A photometry calculation.
    pub photo: Option<Job<Result<crate::photometry_ui::PhotoOut, String>>>,
    /// The 3D surfaces of corridors after an edit or when the 3D view is first shown.
    pub corridors: Option<Job<Vec<CorridorBuilt>>>,
    /// The built corridor of the sections window.
    pub sections: Option<Job<Result<crate::road_dialogs::SectionsView, String>>>,
    /// The model-wide check (CHECKALL), with its visibility scene.
    pub check_all: Option<Job<crate::check_all::Report>>,
    /// A sheet set composed for the preview or an export.
    pub sheets: Option<Job<crate::sheets_ui::ComposeOut>>,
}

/// The kinds of job, in the order the progress window lists them.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Kind {
    Clash,
    Pack,
    Vis,
    Photo,
    Corridors,
    Sections,
    CheckAll,
    Sheets,
}

impl Background {
    pub fn busy(&self) -> bool {
        self.clash.is_some() || self.pack.is_some() || self.vis.is_some() || self.photo.is_some() || self.corridors.is_some() || self.sections.is_some() || self.check_all.is_some() || self.sheets.is_some()
    }

    /// The running jobs: kind, label and seconds so far.
    pub fn running(&self) -> Vec<(Kind, String, f64)> {
        fn row<T: Send + 'static>(kind: Kind, job: &Option<Job<T>>) -> Option<(Kind, String, f64)> {
            job.as_ref().map(|j| (kind, j.label.clone(), j.elapsed_s()))
        }
        [row(Kind::Clash, &self.clash), row(Kind::Pack, &self.pack), row(Kind::Vis, &self.vis), row(Kind::Photo, &self.photo), row(Kind::Corridors, &self.corridors), row(Kind::Sections, &self.sections), row(Kind::CheckAll, &self.check_all), row(Kind::Sheets, &self.sheets)].into_iter().flatten().collect()
    }

    pub fn cancel(&self, kind: Kind) {
        fn go<T: Send + 'static>(job: &Option<Job<T>>) {
            if let Some(j) = job {
                j.cancel();
            }
        }
        match kind {
            Kind::Clash => go(&self.clash),
            Kind::Pack => go(&self.pack),
            Kind::Vis => go(&self.vis),
            Kind::Photo => go(&self.photo),
            Kind::Corridors => go(&self.corridors),
            Kind::Sections => go(&self.sections),
            Kind::CheckAll => go(&self.check_all),
            Kind::Sheets => go(&self.sheets),
        }
    }
}

/// Takes a finished job out of its slot: `None` while it is absent or still running, otherwise
/// what polling found (and the slot is empty again).
pub fn take<T: Send + 'static>(slot: &mut Option<Job<T>>, doc: &Document) -> Option<Poll<T>> {
    let polled = slot.as_ref()?.poll(doc);
    if matches!(polled, Poll::Running) {
        return None;
    }
    *slot = None;
    Some(polled)
}

impl crate::app::AfroBimApp {
    /// Once a frame: collects finished jobs and shows a progress window with Cancel for each one
    /// still running. The UI stays responsive because nothing here waits.
    pub(crate) fn background_tick(&mut self, ctx: &egui::Context) {
        if let Some(polled) = take(&mut self.bg.clash, &self.doc) {
            match polled {
                Poll::Done(out) => self.apply_clash_out(out),
                other => self.status = discard_message("Clash run", &other),
            }
        }
        if let Some(polled) = take(&mut self.bg.pack, &self.doc) {
            match polled {
                Poll::Done((pack, pages)) => {
                    self.status = format!("Report pack composed: {}", crate::report_pack::summary_line(&pack));
                    self.pack.pages = pages;
                    self.pack.pack = Some(pack);
                }
                other => self.status = discard_message("Report pack", &other),
            }
        }
        if let Some(polled) = take(&mut self.bg.vis, &self.doc) {
            match polled {
                Poll::Done(out) => self.apply_vis_out(out),
                other => self.status = discard_message("Sign visibility run", &other),
            }
        }
        if let Some(polled) = take(&mut self.bg.photo, &self.doc) {
            match polled {
                Poll::Done(out) => {
                    if let Err(e) = self.apply_photo_out(out) {
                        self.photo.form.error = e.clone();
                        self.status = format!("Photometry not calculated: {e}");
                    } else {
                        self.photo.form.error.clear();
                    }
                }
                other => self.status = discard_message("Photometry", &other),
            }
        }
        if let Some(polled) = take(&mut self.bg.corridors, &self.doc) {
            match polled {
                Poll::Done(built) => self.apply_corridors(built),
                other => {
                    // A cancelled rebuild is not started again until the model is edited.
                    self.corridors_held = matches!(other, Poll::Cancelled);
                    self.status = discard_message("Corridor rebuild", &other);
                }
            }
        }
        if let Some(polled) = take(&mut self.bg.sections, &self.doc) {
            match polled {
                Poll::Done(Ok(view)) => self.road.show_sections(view),
                Poll::Done(Err(e)) => self.status = format!("No sections to show: {e}"),
                other => self.status = discard_message("Cross-section build", &other),
            }
        }
        if let Some(polled) = take(&mut self.bg.check_all, &self.doc) {
            match polled {
                Poll::Done(report) => self.apply_check_all(report),
                other => self.status = discard_message("Model check", &other),
            }
        }
        if let Some(polled) = take(&mut self.bg.sheets, &self.doc) {
            match polled {
                Poll::Done(out) => self.apply_composed(out),
                other => self.status = discard_message("Sheet set composition", &other),
            }
        }
        // A corridor edited since its surfaces were last built, or cancelled or discarded
        // above, is built again once nothing else of the kind is running.
        if self.view_3d {
            self.sync_corridor_surfaces_bg();
        }
        let running = self.bg.running();
        if running.is_empty() {
            return;
        }
        ctx.request_repaint_after(std::time::Duration::from_millis(100));
        let mut cancel = Vec::new();
        crate::win::new(ctx, "Working").collapsible(false).resizable(false).title_bar(true).show(ctx, |ui| {
            for (kind, label, elapsed) in &running {
                ui.horizontal(|ui| {
                    ui.add(egui::ProgressBar::new(0.0).animate(true).desired_width(180.0).text(format!("{label}: {elapsed:.0} s")));
                    if ui.button("Cancel").clicked() {
                        cancel.push(*kind);
                    }
                });
            }
            ui.label(egui::RichText::new("The model stays editable. An edit while this runs discards the result.").small().weak());
        });
        for kind in cancel {
            self.bg.cancel(kind);
        }
    }
}

/// What the status line says when a result was thrown away.
fn discard_message<T>(what: &str, poll: &Poll<T>) -> String {
    match poll {
        Poll::Cancelled => format!("{what} cancelled: the result was discarded."),
        Poll::Stale => format!("{what} discarded: the document changed while it ran. Run it again."),
        Poll::Lost => format!("{what} stopped without a result."),
        _ => String::new(),
    }
}

/// The modules of a session except the sign module, for a worker thread to build its own copy of.
pub fn worker_registry() -> ModuleRegistry {
    let mut modules = ModuleRegistry::default();
    crate::supports_glue::register(&mut modules);
    crate::alignment_glue::register(&mut modules);
    crate::markings_glue::register(&mut modules);
    crate::modules_glue::register(&mut modules);
    crate::coordination_glue::register(&mut modules);
    modules.register(Box::new(crate::reference_models::RefModule));
    modules.register(Box::new(afrobim_schedule::ScheduleModule::default()));
    modules.register(Box::new(afrobim_cost::EstimateModule));
    modules.register(Box::new(afrobim_progress::ProgressModule));
    crate::sheets_glue::register(&mut modules);
    afrobim_design::register(&mut modules);
    afrobim_annotate::register(&mut modules);
    modules.register(Box::new(crate::underlay::UnderlayModule));
    modules.register(Box::new(afrobim_kernel::ProjectSettingsModule));
    modules
}

#[cfg(test)]
mod tests {
    use super::*;
    use afrobim_kernel::Pt;

    fn wait<T: Send + 'static>(job: &Job<T>, doc: &Document) -> Poll<T> {
        for _ in 0..3000 {
            match job.poll(doc) {
                Poll::Running => std::thread::sleep(std::time::Duration::from_millis(10)),
                other => return other,
            }
        }
        Poll::Running
    }

    #[test]
    fn a_result_is_applied_only_if_the_document_has_not_changed_underneath() {
        let mut doc = Document::default();
        doc.add("drain", "D1", Pt::new(0.0, 0.0), 1.0, 1.0, "0", serde_json::Value::Null);
        // Unchanged: the result comes back.
        let job = Job::spawn("test", fingerprint(&doc), || 41 + 1);
        assert_eq!(wait(&job, &doc), Poll::Done(42));
        // The document changes while the job runs: discarded.
        let job = Job::spawn("test", fingerprint(&doc), || {
            std::thread::sleep(std::time::Duration::from_millis(50));
            7
        });
        doc.add("drain", "D2", Pt::new(5.0, 0.0), 1.0, 1.0, "0", serde_json::Value::Null);
        assert_eq!(wait(&job, &doc), Poll::Stale);
        // Cancelled: discarded, even though the worker finishes.
        let job = Job::spawn("test", fingerprint(&doc), || {
            std::thread::sleep(std::time::Duration::from_millis(30));
            1
        });
        job.cancel();
        assert_eq!(wait(&job, &doc), Poll::Cancelled);
        // The UI thread is not blocked: polling a running job returns at once.
        let job = Job::spawn("slow", fingerprint(&doc), || {
            std::thread::sleep(std::time::Duration::from_millis(300));
            1
        });
        assert_eq!(job.poll(&doc), Poll::Running);
    }

    // ─── the jobs of the app: each applies its result on an unchanged document and discards it
    // on a changed one. The app's own glue (`apply_*`, the progress window) needs a window and is
    // not run here; these tests run the worker functions it spawns and poll them as it does.

    use crate::sheets_glue::test_support::demo_doc;

    fn name<T>(p: &Poll<T>) -> &'static str {
        match p {
            Poll::Running => "running",
            Poll::Done(_) => "done",
            Poll::Cancelled => "cancelled",
            Poll::Stale => "stale",
            Poll::Lost => "lost",
        }
    }

    fn changed(doc: &mut Document) {
        doc.add("drain", "DX", Pt::new(900.0, 900.0), 1.0, 1.0, "0", serde_json::Value::Null);
    }

    fn corridor_id(doc: &Document) -> afrobim_kernel::ElementId {
        doc.elements().iter().find(|e| e.category == afrobim_corridor::CORRIDOR_CATEGORY).expect("the demo has a corridor").id
    }

    #[test]
    fn the_corridor_rebuild_applies_on_an_unchanged_document_and_is_discarded_on_a_changed_one() {
        let mut doc = demo_doc();
        let id = corridor_id(&doc);
        // Nothing cached: the corridor is stale; with its data cached it is not.
        assert_eq!(stale_corridors(&doc, &|_| None), vec![id]);
        let data = doc.find(id).unwrap().data.clone();
        assert!(stale_corridors(&doc, &|_| Some(data.clone())).is_empty());
        assert_eq!(stale_corridors(&doc, &|_| Some(serde_json::Value::Null)), vec![id]);
        // Unchanged: the surfaces come back, built from the data of the moment the job started.
        let job = spawn_corridors(&doc, vec![id]);
        match wait(&job, &doc) {
            Poll::Done(built) => {
                assert_eq!(built.len(), 1);
                assert_eq!((built[0].id, &built[0].key), (id, &data));
                let solids = built[0].solids.as_ref().expect("the corridor builds");
                // The same surfaces as the synchronous build.
                let sync = crate::road_glue::corridor_solids(&doc, id).unwrap();
                assert_eq!(solids.meshes.len(), sync.meshes.len());
                assert_eq!(solids.meshes[0].vertices.len(), sync.meshes[0].vertices.len());
            }
            other => panic!("{}", name(&other)),
        }
        // Changed while it ran: discarded.
        let job = spawn_corridors(&doc, vec![id]);
        changed(&mut doc);
        assert!(matches!(wait(&job, &doc), Poll::Stale));
    }

    #[test]
    fn the_sections_build_applies_on_an_unchanged_document_and_is_discarded_on_a_changed_one() {
        let mut doc = demo_doc();
        let id = corridor_id(&doc);
        let job = crate::road_dialogs::spawn_sections(&doc, id);
        match wait(&job, &doc) {
            Poll::Done(Ok(view)) => assert_eq!(view.mark, doc.find(id).unwrap().name),
            Poll::Done(Err(e)) => panic!("{e}"),
            other => panic!("{}", name(&other)),
        }
        let job = crate::road_dialogs::spawn_sections(&doc, id);
        changed(&mut doc);
        assert!(matches!(wait(&job, &doc), Poll::Stale));
    }

    /// The demo document with one sign beside the alignment and its visibility inputs stored.
    fn doc_with_sign_inputs() -> (Document, afrobim_kernel::ElementId) {
        let mut doc = demo_doc();
        let (aid, _) = crate::alignment_glue::alignments(&doc).remove(0);
        let a = crate::alignment_glue::alignment_of(&doc, aid).unwrap();
        let name = doc.elements().iter().find(|e| e.id == aid).unwrap().name.clone();
        let station = a.start_station_m + 300.0;
        let (x, y) = a.point_at_offset(station, 6.0).unwrap();
        let id = doc.add("sign", "VS-1", Pt::new(0.0, 0.0), 1000.0, 1000.0, "Signs", serde_json::Value::Null);
        doc.find_mut(id).unwrap().site = Some(afrobim_kernel::SitePlacement { x: x * 1000.0, y: y * 1000.0, z: 0.0, rotation_deg: 0.0, alignment: Some(name), station_m: Some(station), offset_m: Some(6.0) });
        let mut f = crate::visibility_ui::VisForm::new(Some(id));
        f.eye_height_accepted = true;
        f.traffic = 1;
        f.eye_mode = 2;
        f.eye_text = "1.75".into();
        crate::visibility_ui::store_form(&mut doc, &f).unwrap();
        (doc, id)
    }

    fn vis_input(doc: &Document, only: Option<Vec<afrobim_kernel::ElementId>>) -> crate::visibility_ui::VisInput {
        crate::visibility_ui::VisInput { doc: doc.clone(), solids: Default::default(), x_heights: Default::default(), only }
    }

    #[test]
    fn the_visibility_run_applies_on_an_unchanged_document_and_is_discarded_on_a_changed_one() {
        let (mut doc, id) = doc_with_sign_inputs();
        // One sign.
        let job = crate::visibility_ui::spawn_run(&doc, vis_input(&doc, Some(vec![id])));
        match wait(&job, &doc) {
            Poll::Done(out) => {
                assert_eq!(out.run.analyses.len(), 1);
                // The same analysis as the synchronous path.
                let src = crate::visibility_ui::source_for(&doc, &|_| Vec::new(), Default::default());
                let sync = crate::visibility_ui::run(&doc, &src, Some(vec![id]));
                assert_eq!(out.run.summary_csv(), sync.summary_csv());
            }
            other => panic!("{}", name(&other)),
        }
        // The road (every placed sign).
        let job = crate::visibility_ui::spawn_run(&doc, vis_input(&doc, None));
        assert!(matches!(wait(&job, &doc), Poll::Done(out) if !out.run.analyses.is_empty()));
        // Changed while it ran: discarded.
        let job = crate::visibility_ui::spawn_run(&doc, vis_input(&doc, None));
        changed(&mut doc);
        assert!(matches!(wait(&job, &doc), Poll::Stale));
    }

    #[test]
    fn the_photometry_run_applies_on_an_unchanged_document_and_is_discarded_on_a_changed_one() {
        use afrobim_lighting::calc::{default_carriageway_area, Division};
        let mut doc = demo_doc();
        let id = crate::photometry_ui::lighting_elements(&doc).first().expect("the demo has street lighting").0;
        let path = crate::sheets_glue::test_support::scratch("job.ies");
        std::fs::write(&path, crate::photometry_ui::tests::synthetic_ies()).unwrap();
        let loaded = afrobim_lighting::load_photometry_file(&path).unwrap();
        let data = afrobim_lighting::lighting_of(&doc, id).unwrap();
        let mut f = crate::photometry_ui::PhotoForm::default();
        f.orient_c = "0".into();
        f.orient_confirmed = true;
        f.maintenance = "0.8".into();
        f.contribution = "60".into();
        f.areas.push(crate::photometry_ui::AreaForm::of(&default_carriageway_area(&data, Division::Spacing(3.0), Division::Spacing(1.0)).unwrap()));
        let (settings, rtable) = f.settings().unwrap();
        let input = |doc: &Document| {
            let data = afrobim_lighting::lighting_of(doc, id).unwrap();
            let alignment = crate::alignment_glue::alignments(doc).into_iter().next().and_then(|a| crate::alignment_glue::alignment_of(doc, a.0));
            crate::photometry_ui::PhotoInput { id, loaded: loaded.clone(), settings: settings.clone(), rtable: rtable.clone(), data, alignment, path: path.to_string_lossy().to_string(), rt_info: None }
        };
        let job = crate::photometry_ui::spawn_photometry(&doc, input(&doc));
        match wait(&job, &doc) {
            Poll::Done(Ok(out)) => {
                assert_eq!(out.id, id);
                assert!(!out.run.result.areas.is_empty());
                // Applying stores the settings on the element, as the synchronous run does.
                afrobim_lighting::calc::set_photometry(&mut doc, id, &out.path, &out.loaded, &out.settings, None).unwrap();
                assert!(afrobim_lighting::calc::photometry_settings_of(&doc, id).unwrap().is_some());
            }
            other => panic!("{}", name(&other)),
        }
        let job = crate::photometry_ui::spawn_photometry(&doc, input(&doc));
        changed(&mut doc);
        assert!(matches!(wait(&job, &doc), Poll::Stale));
    }

    fn clash_input(doc: &Document, prebuilt: bool) -> crate::coordination_ui::ClashInput {
        use afrobim_coordination::{ClashTest, Selection};
        let mut co = afrobim_coordination::Coordination::default();
        co.tests.push(ClashTest::hard("t", Selection::all(), Selection::all()));
        let id = corridor_id(doc);
        let mut corridors = std::collections::HashMap::new();
        let mut stale = vec![id];
        if prebuilt {
            corridors.insert(id, crate::road_glue::corridor_solids(doc, id));
            stale.clear();
        }
        crate::coordination_ui::ClashInput { doc: doc.clone(), co, solids: Default::default(), corridors, stale, site_mode: crate::view3d::site_mode_of(doc), refs: Vec::new() }
    }

    #[test]
    fn the_clash_run_builds_its_corridors_in_the_worker_and_is_discarded_on_a_changed_document() {
        let mut doc = demo_doc();
        let id = corridor_id(&doc);
        // The corridor is stale: the worker builds it, and the scene has its body.
        let job = crate::coordination_ui::spawn_clash(clash_input(&doc, false));
        let built = match wait(&job, &doc) {
            Poll::Done(out) => {
                assert_eq!(out.corridors.len(), 1, "the worker built the corridor");
                assert_eq!((out.corridors[0].id, &out.corridors[0].key), (id, &doc.find(id).unwrap().data));
                assert!(out.corridors[0].solids.is_ok());
                assert!(out.bodies >= 1, "the corridor is a body of the scene");
                out
            }
            other => panic!("{}", name(&other)),
        };
        // With the surfaces already cached nothing is built, and the run is the same.
        let job = crate::coordination_ui::spawn_clash(clash_input(&doc, true));
        match wait(&job, &doc) {
            Poll::Done(out) => {
                assert!(out.corridors.is_empty());
                assert_eq!(out.bodies, built.bodies);
                assert_eq!(out.stats.iter().map(|s| s.clashes).collect::<Vec<_>>(), built.stats.iter().map(|s| s.clashes).collect::<Vec<_>>());
            }
            other => panic!("{}", name(&other)),
        }
        // Changed while it ran: discarded.
        let job = crate::coordination_ui::spawn_clash(clash_input(&doc, false));
        changed(&mut doc);
        assert!(matches!(wait(&job, &doc), Poll::Stale));
        // Cancelled: discarded.
        let job = crate::coordination_ui::spawn_clash(clash_input(&doc, false));
        job.cancel();
        assert!(matches!(wait(&job, &doc), Poll::Cancelled));
    }

    fn check_input(doc: &Document, vis: bool) -> crate::check_ui::CheckAllInput {
        let registry = crate::sheets_glue::test_support::registry();
        let notes = doc.elements().iter().filter(|e| registry.get(&e.category).is_some()).map(|e| (e.id, registry.generate(e).map(|g| g.notes))).collect();
        let labels = registry.categories().map(|(c, n)| (c.to_string(), n.to_string())).collect();
        crate::check_ui::CheckAllInput { doc: doc.clone(), notes, labels, vis: vis.then(|| vis_input(doc, None)) }
    }

    #[test]
    fn the_model_check_applies_on_an_unchanged_document_and_is_discarded_on_a_changed_one() {
        let mut doc = demo_doc();
        // The report is the one the synchronous check gives.
        let sync = crate::sheets_glue::check_report(&doc, &crate::sheets_glue::test_support::registry());
        let job = crate::check_ui::spawn_check_all(check_input(&doc, false));
        match wait(&job, &doc) {
            Poll::Done(report) => {
                assert!(!report.rows.is_empty());
                assert_eq!(report, sync);
            }
            other => panic!("{}", name(&other)),
        }
        // With a sign that has visibility inputs the obstruction scene is built in the worker.
        let (mut vdoc, _) = doc_with_sign_inputs();
        let job = crate::check_ui::spawn_check_all(check_input(&vdoc, true));
        match wait(&job, &vdoc) {
            Poll::Done(report) => {
                assert!(report.rows.iter().any(|r| r.category.to_lowercase().contains("sign") || r.check.to_lowercase().contains("visib")), "{:?}", report.rows.iter().map(|r| (&r.mark, &r.check)).collect::<Vec<_>>());
            }
            other => panic!("{}", name(&other)),
        }
        let job = crate::check_ui::spawn_check_all(check_input(&vdoc, true));
        changed(&mut vdoc);
        assert!(matches!(wait(&job, &vdoc), Poll::Stale));
        let job = crate::check_ui::spawn_check_all(check_input(&doc, false));
        changed(&mut doc);
        assert!(matches!(wait(&job, &doc), Poll::Stale));
    }

    #[test]
    fn a_sheet_set_with_cross_sections_composes_in_the_worker_and_is_discarded_on_a_changed_document() {
        use crate::sheets_ui::{spawn_compose, ComposeThen};
        let mut doc = demo_doc();
        let (set, _) = crate::sheets_glue::sets(&doc).remove(0);
        let job = spawn_compose(&doc, set, None, ComposeThen::Pdf);
        match wait(&job, &doc) {
            Poll::Done(out) => {
                assert_eq!(out.then, ComposeThen::Pdf);
                let c = out.result.expect("the demo set composes");
                // The demo set has six sheets, the last of them cross-sections (DEMO-SC-001).
                let numbers: Vec<&str> = c.composed.sheets.iter().map(|s| s.number.as_str()).collect();
                assert_eq!(numbers.len(), 6, "{numbers:?}");
                assert!(numbers.contains(&"DEMO-SC-001"), "{numbers:?}");
                assert!(c.composed.warnings.is_empty(), "{:?}", c.composed.warnings);
                // The same sheets as the synchronous compose.
                let registry = crate::sheets_glue::test_support::registry();
                let sync = crate::sheets_glue::compose_stored(&doc, &registry, set, &crate::sheets_glue::Live::from_doc(&doc, &registry, None)).unwrap();
                let sync_numbers: Vec<&str> = sync.composed.sheets.iter().map(|s| s.number.as_str()).collect();
                assert_eq!(numbers, sync_numbers);
            }
            other => panic!("{}", name(&other)),
        }
        let job = spawn_compose(&doc, set, None, ComposeThen::Preview);
        changed(&mut doc);
        assert!(matches!(wait(&job, &doc), Poll::Stale));
        let job = spawn_compose(&doc, set, None, ComposeThen::Dxf { one_file: true });
        job.cancel();
        assert!(matches!(wait(&job, &doc), Poll::Cancelled));
    }

    #[test]
    fn the_progress_window_lists_every_running_kind_and_cancel_reaches_it() {
        let doc = Document::default();
        let mut bg = Background::default();
        assert!(!bg.busy() && bg.running().is_empty());
        bg.corridors = Some(Job::spawn("Corridor rebuild", fingerprint(&doc), || {
            std::thread::sleep(std::time::Duration::from_millis(100));
            Vec::new()
        }));
        bg.sections = Some(Job::spawn("Cross-section build", fingerprint(&doc), || Err("x".to_string())));
        assert!(bg.busy());
        let kinds: Vec<Kind> = bg.running().into_iter().map(|r| r.0).collect();
        assert_eq!(kinds, vec![Kind::Corridors, Kind::Sections]);
        bg.cancel(Kind::Corridors);
        // `take` empties the slot of a finished or cancelled job and leaves a running one alone.
        assert!(matches!(take(&mut bg.corridors, &doc), Some(Poll::Cancelled)) && bg.corridors.is_none());
        assert!(take(&mut bg.corridors, &doc).is_none());
    }

    #[test]
    fn a_worker_builds_its_own_registry() {
        let r = worker_registry();
        assert!(r.get(afrobim_markings::CATEGORY_EXT).is_some() && r.get(afrobim_drainage::DRAIN_CATEGORY).is_some());
    }
}
