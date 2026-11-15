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

/// The jobs the app may have running, at most one of each kind.
#[derive(Default)]
pub struct Background {
    pub clash: Option<Job<crate::coordination_ui::ClashOut>>,
    pub pack: Option<Job<(crate::report_pack::Pack, usize)>>,
}

impl Background {
    pub fn busy(&self) -> bool {
        self.clash.is_some() || self.pack.is_some()
    }
}

impl crate::app::AfroBimApp {
    /// Once a frame: collects finished jobs and shows a progress window with Cancel for each one
    /// still running. The UI stays responsive because nothing here waits.
    pub(crate) fn background_tick(&mut self, ctx: &egui::Context) {
        let mut cancel_clash = false;
        let mut cancel_pack = false;
        if let Some(job) = &self.bg.clash {
            match job.poll(&self.doc) {
                Poll::Running => {}
                Poll::Done(out) => {
                    self.bg.clash = None;
                    self.apply_clash_out(out);
                }
                other => {
                    self.bg.clash = None;
                    self.status = discard_message("Clash run", &other);
                }
            }
        }
        if let Some(job) = &self.bg.pack {
            match job.poll(&self.doc) {
                Poll::Running => {}
                Poll::Done((pack, pages)) => {
                    self.bg.pack = None;
                    self.status = format!("Report pack composed: {}", crate::report_pack::summary_line(&pack));
                    self.pack.pages = pages;
                    self.pack.pack = Some(pack);
                }
                other => {
                    self.bg.pack = None;
                    self.status = discard_message("Report pack", &other);
                }
            }
        }
        let running: Vec<(&'static str, String, f64)> = [self.bg.clash.as_ref().map(|j| ("clash", j.label.clone(), j.elapsed_s())), self.bg.pack.as_ref().map(|j| ("pack", j.label.clone(), j.elapsed_s()))].into_iter().flatten().collect();
        if running.is_empty() {
            return;
        }
        ctx.request_repaint_after(std::time::Duration::from_millis(100));
        crate::win::new(ctx, "Working").collapsible(false).resizable(false).title_bar(true).show(ctx, |ui| {
            for (kind, label, elapsed) in &running {
                ui.horizontal(|ui| {
                    ui.add(egui::ProgressBar::new(0.0).animate(true).desired_width(180.0).text(format!("{label}: {elapsed:.0} s")));
                    if ui.button("Cancel").clicked() {
                        if *kind == "clash" {
                            cancel_clash = true;
                        } else {
                            cancel_pack = true;
                        }
                    }
                });
            }
            ui.label(egui::RichText::new("The model stays editable. An edit while this runs discards the result.").small().weak());
        });
        if cancel_clash {
            if let Some(j) = &self.bg.clash {
                j.cancel();
            }
        }
        if cancel_pack {
            if let Some(j) = &self.bg.pack {
                j.cancel();
            }
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
        for _ in 0..500 {
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

    #[test]
    fn a_worker_builds_its_own_registry() {
        let r = worker_registry();
        assert!(r.get(afrobim_markings::CATEGORY_EXT).is_some() && r.get(afrobim_drainage::DRAIN_CATEGORY).is_some());
    }
}
