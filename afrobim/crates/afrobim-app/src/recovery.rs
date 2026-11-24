//! Autosave, the recovery file and the panic hook (plan item 7.1).
//!
//! While there are unsaved changes the app writes a copy of the document, atomically, every few
//! minutes (a preference, 5 by default, 0 switches it off) to `<name>.afrobim.recover` beside the
//! document, or to the `recover` folder of the preferences folder for a document with no name
//! yet. A normal save and a clean exit remove that file. If the program dies, the next start (or
//! the next time that document is opened) finds a recovery file newer than the document and
//! offers it: "Recover unsaved changes from <time>" or "Discard". Recovering loads the changes as
//! unsaved changes; the engineer still chooses Save.
//!
//! The panic hook cannot reach the app, so the app keeps a copy of the document in a slot (see
//! [`set_panic_snapshot`]) that the hook writes to the recovery file. The functions here take
//! their folders, times and slots as arguments so they can be tested without a window.

use std::io;
use std::panic::{catch_unwind, AssertUnwindSafe};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Mutex, TryLockError};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use afrobim_kernel::{recover_path, write_atomic, Document};

use crate::app::AfroBimApp;
use crate::ui_kit::{self, Dialog, Footer, Level};

/// Minutes between autosaves unless the preferences say otherwise.
pub const DEFAULT_AUTOSAVE_MINUTES: u32 = 5;
/// The longest interval the preference accepts.
pub const MAX_AUTOSAVE_MINUTES: u32 = 240;
/// How long after an edit the copy kept for the panic hook is refreshed, at the earliest.
const SNAPSHOT_EVERY_SECS: f64 = 1.0;

/// The interval as the preference holds it, kept in range.
pub fn clamp_minutes(minutes: u32) -> u32 {
    minutes.min(MAX_AUTOSAVE_MINUTES)
}

/// The folder for the recovery file of a document that has no name yet: `recover`, beside the
/// preferences file.
pub fn untitled_dir() -> Option<PathBuf> {
    crate::prefs::path().and_then(|p| p.parent().map(|d| d.join("recover")))
}

/// The recovery file of a document with no name yet, in `dir`.
pub fn untitled_recover(dir: &Path) -> PathBuf {
    dir.join("untitled.afrobim.recover")
}

/// Where the recovery file of this document goes: beside the document, or in `untitled_dir`
/// when it has no file yet. `None` when it has neither.
pub fn target_for(doc_path: Option<&Path>, untitled_dir: Option<&Path>) -> Option<PathBuf> {
    match doc_path {
        Some(p) => Some(recover_path(p)),
        None => untitled_dir.map(untitled_recover),
    }
}

/// Whether the autosave interval has run out. Zero minutes means autosave is off.
pub fn autosave_due(minutes: u32, elapsed: Duration) -> bool {
    let minutes = clamp_minutes(minutes);
    minutes > 0 && elapsed >= Duration::from_secs(u64::from(minutes) * 60)
}

/// What an autosave did.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Autosaved {
    /// There were no unsaved changes: nothing was written.
    Clean,
    /// The recovery file already holds this state: nothing was written.
    Unchanged,
    /// The recovery file was written; the number is the fingerprint of what it holds.
    Written(u64),
}

/// Writes `doc` to the recovery file `target`, atomically, but only when it has unsaved changes
/// (`saved` is the fingerprint at the last open or save, as in `dropped::has_unsaved`) and the file
/// does not already hold this state (`last_written`).
pub fn autosave_if_dirty(doc: &Document, target: &Path, saved: Option<u64>, last_written: Option<u64>) -> io::Result<Autosaved> {
    let json = doc.to_json();
    let fp = crate::background::fingerprint_json(&json);
    if !crate::dropped::has_unsaved(saved, fp, doc.elements().len()) {
        return Ok(Autosaved::Clean);
    }
    if last_written == Some(fp) {
        return Ok(Autosaved::Unchanged);
    }
    if let Some(dir) = target.parent().filter(|d| !d.as_os_str().is_empty()) {
        std::fs::create_dir_all(dir)?;
    }
    write_atomic(target, json.as_bytes())?;
    Ok(Autosaved::Written(fp))
}

/// Whether a recovery file should be preferred to the document: it is newer, or there is no
/// document file to compare with.
pub fn prefer_recover(doc_modified: Option<SystemTime>, recover_modified: SystemTime) -> bool {
    match doc_modified {
        Some(d) => recover_modified > d,
        None => true,
    }
}

/// A recovery file that is worth offering, already read.
pub struct Offer {
    pub recover: PathBuf,
    pub modified: SystemTime,
    /// The document it belongs to, or `None` for the one with no name yet.
    pub for_doc: Option<PathBuf>,
    pub doc: Document,
}

/// What looking for a recovery file found.
pub enum Found {
    Nothing,
    /// A file that is older than the document, empty or the same as it: of no use.
    Stale(PathBuf),
    /// A file that cannot be read as a document.
    Damaged(PathBuf, String),
    Offer(Box<Offer>),
}

/// Looks for the recovery file of `doc_path` (or of the unnamed document) and decides whether to
/// offer it. Nothing is changed on disk.
pub fn find_offer(doc_path: Option<&Path>, untitled_dir: Option<&Path>) -> Found {
    let Some(recover) = target_for(doc_path, untitled_dir) else { return Found::Nothing };
    let Ok(meta) = std::fs::metadata(&recover) else { return Found::Nothing };
    if !meta.is_file() {
        return Found::Nothing;
    }
    let modified = meta.modified().unwrap_or(UNIX_EPOCH);
    let doc_modified = doc_path.and_then(|p| std::fs::metadata(p).ok()).and_then(|m| m.modified().ok());
    if meta.len() == 0 || !prefer_recover(doc_modified, modified) {
        return Found::Stale(recover);
    }
    let doc = match Document::load(&recover) {
        Ok(d) => d,
        Err(e) => return Found::Damaged(recover, e.to_string()),
    };
    // The same content as the document (a panic copy of a document with no changes): nothing to recover.
    let same = match doc_path {
        Some(p) => Document::load(p).is_ok_and(|saved| saved.to_json() == doc.to_json()),
        None => doc.elements().is_empty(),
    };
    if same {
        return Found::Stale(recover);
    }
    Found::Offer(Box::new(Offer { recover, modified, for_doc: doc_path.map(Path::to_path_buf), doc }))
}

/// Removes recovery files, ignoring any that is not there.
pub fn remove_files<'a>(files: impl IntoIterator<Item = &'a Path>) {
    for f in files {
        let _ = std::fs::remove_file(f);
    }
}

/// A time as `2026-11-25 14:03 UTC` (the program has no time zone data).
pub fn format_utc(t: SystemTime) -> String {
    let secs = t.duration_since(UNIX_EPOCH).map(|d| d.as_secs()).unwrap_or(0);
    let (days, rem) = ((secs / 86_400) as i64, secs % 86_400);
    // Civil date from days since 1970-01-01 (H. Hinnant's algorithm).
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z.rem_euclid(146_097);
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    let y = yoe + era * 400 + i64::from(m <= 2);
    format!("{y:04}-{m:02}-{d:02} {:02}:{:02} UTC", rem / 3600, rem % 3600 / 60)
}

// ─── the panic hook ──────────────────────────────────────────────────────────

/// A copy of the document and where its recovery file goes, for the panic hook.
pub struct PanicSnapshot {
    pub target: PathBuf,
    pub doc: Document,
}

pub type SnapshotSlot = Mutex<Option<PanicSnapshot>>;

static SLOT: SnapshotSlot = Mutex::new(None);
static IN_HOOK: AtomicBool = AtomicBool::new(false);

/// Puts a copy of the document in the slot the panic hook reads, replacing the last one.
pub fn set_snapshot(slot: &SnapshotSlot, target: Option<PathBuf>, doc: &Document) {
    let snapshot = target.map(|target| PanicSnapshot { target, doc: doc.clone() });
    let mut guard = slot.lock().unwrap_or_else(|poisoned| poisoned.into_inner());
    *guard = snapshot;
}

/// The panic hook's work: writes the copy in `slot` to its recovery file, once, atomically.
/// Never panics and never waits: a slot that is locked (the panic happened while it was being
/// refreshed) is skipped, and `busy` stops a second panic inside the hook from starting again.
/// Returns whether a file was written.
pub fn write_panic_recovery(slot: &SnapshotSlot, busy: &AtomicBool) -> bool {
    if busy.swap(true, Ordering::SeqCst) {
        return false;
    }
    let wrote = catch_unwind(AssertUnwindSafe(|| {
        let guard = match slot.try_lock() {
            Ok(g) => g,
            Err(TryLockError::Poisoned(p)) => p.into_inner(),
            Err(TryLockError::WouldBlock) => return false,
        };
        let Some(snapshot) = guard.as_ref() else { return false };
        if let Some(dir) = snapshot.target.parent().filter(|d| !d.as_os_str().is_empty()) {
            let _ = std::fs::create_dir_all(dir);
        }
        write_atomic(&snapshot.target, snapshot.doc.to_json().as_bytes()).is_ok()
    }))
    .unwrap_or(false);
    busy.store(false, Ordering::SeqCst);
    wrote
}

/// Installs the panic hook: a panic on the thread that calls this tries once to write the
/// recovery file, then the previous hook (the message, the backtrace) runs as before. Called from
/// `main` only.
pub fn install_panic_hook() {
    let main_thread = std::thread::current().id();
    let previous = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |info| {
        if std::thread::current().id() == main_thread && write_panic_recovery(&SLOT, &IN_HOOK) {
            eprintln!("AfroBIM: unsaved changes were written to a recovery file.");
        }
        previous(info);
    }));
}

// ─── in the app ──────────────────────────────────────────────────────────────

/// What the app keeps between frames for autosave, the panic copy and the offer.
#[derive(Default)]
pub struct RecoveryState {
    /// The frame time (seconds) of the last autosave check that ran.
    last_check: f64,
    /// The fingerprint the recovery file holds, if this session wrote it.
    pub last_written: Option<u64>,
    /// The document changed since the panic copy was taken.
    snapshot_stale: bool,
    last_snapshot: f64,
    snapshot_target: Option<PathBuf>,
    /// The recovery file being offered.
    pub offer: Option<Box<Offer>>,
    /// The autosave dialog is open.
    pub settings_open: bool,
}

impl RecoveryState {
    /// The document changed: the copy for the panic hook is taken again soon.
    pub fn note_edit(&mut self) {
        self.snapshot_stale = true;
    }
}

impl AfroBimApp {
    /// Autosave and recovery run in a normal session, not while taking a screenshot or quitting.
    fn recovery_enabled(&self) -> bool {
        self.options.screenshot.is_none() && !self.quit
    }

    /// Where this document's recovery file goes now.
    pub(crate) fn recover_target(&self) -> Option<PathBuf> {
        target_for(self.doc_path(), untitled_dir().as_deref())
    }

    /// Once a frame: refreshes the copy for the panic hook, autosaves when the interval is up,
    /// and shows the offer to recover.
    pub(crate) fn recovery_tick(&mut self, ctx: &egui::Context) {
        if !self.recovery_enabled() {
            return;
        }
        let now = ctx.input(|i| i.time);
        // While an offer waits, the file it offers is not overwritten; an unnamed document with
        // nothing in it has nothing to keep.
        let hold = self.recovery.offer.is_some();
        let target = self.recover_target();
        let panic_target = if hold || (self.doc_path().is_none() && self.doc.elements().is_empty()) { None } else { target.clone() };
        // The panic copy: when the document changed (at most once a second) or the target moved.
        if self.recovery.snapshot_target != panic_target {
            self.recovery.snapshot_target = panic_target.clone();
            self.recovery.snapshot_stale = true;
            self.recovery.last_snapshot = f64::NEG_INFINITY;
        }
        if self.recovery.snapshot_stale {
            if now - self.recovery.last_snapshot >= SNAPSHOT_EVERY_SECS {
                set_snapshot(&SLOT, panic_target, &self.doc);
                self.recovery.snapshot_stale = false;
                self.recovery.last_snapshot = now;
            } else {
                ctx.request_repaint_after(Duration::from_secs_f64(SNAPSHOT_EVERY_SECS));
            }
        }
        // The timed autosave. Nothing is serialised until the interval is up.
        let minutes = clamp_minutes(self.prefs.autosave_minutes);
        if minutes > 0 && !hold {
            let interval = f64::from(minutes) * 60.0;
            let elapsed = (now - self.recovery.last_check).max(0.0);
            if !autosave_due(minutes, Duration::from_secs_f64(elapsed)) {
                ctx.request_repaint_after(Duration::from_secs_f64((interval - elapsed).max(1.0)));
            } else if ctx.input(|i| i.pointer.any_down()) {
                ctx.request_repaint_after(Duration::from_secs(1));
            } else {
                self.recovery.last_check = now;
                if let Some(target) = target {
                    match autosave_if_dirty(&self.doc, &target, self.drops.saved, self.recovery.last_written) {
                        Ok(Autosaved::Written(fp)) => self.recovery.last_written = Some(fp),
                        Ok(_) => {}
                        Err(e) => self.status = format!("Autosave could not write {}: {e}", target.display()),
                    }
                }
            }
        }
        self.recovery_offer_window(ctx);
    }

    /// Looks for a recovery file for the open document (or the unnamed one) and offers it.
    pub(crate) fn recovery_look(&mut self) {
        if !self.recovery_enabled() {
            return;
        }
        self.recovery.offer = None;
        match find_offer(self.doc_path(), untitled_dir().as_deref()) {
            Found::Nothing => {}
            Found::Stale(file) => remove_files([file.as_path()]),
            Found::Damaged(file, why) => self.status = format!("The recovery file {} cannot be read ({why}); it was left as it is.", file.display()),
            Found::Offer(offer) => self.recovery.offer = Some(offer),
        }
    }

    /// After a normal save: the recovery files of the document before and after are not needed.
    pub(crate) fn recovery_saved(&mut self, before: Option<PathBuf>) {
        let mut files = vec![self.recover_target()];
        files.push(target_for(before.as_deref(), untitled_dir().as_deref()));
        remove_files(files.iter().flatten().map(PathBuf::as_path));
        self.recovery.last_written = None;
        self.recovery.offer = None;
        self.recovery.note_edit();
    }

    /// A clean exit: the recovery file of the open document goes.
    pub(crate) fn recovery_exit(&mut self) {
        if self.options.screenshot.is_some() {
            return;
        }
        let Some(t) = self.recover_target() else { return };
        // Quitting with unsaved changes keeps (and refreshes) the copy, so the work is offered
        // again on the next start; only a document with nothing unsaved loses its copy. An offer
        // still waiting is left exactly as it is.
        if self.recovery.offer.is_some() {
            return;
        }
        if crate::dropped::has_unsaved(self.drops.saved, crate::background::fingerprint(&self.doc), self.doc.elements().len()) {
            if let Err(e) = autosave_if_dirty(&self.doc, &t, self.drops.saved, None) {
                eprintln!("AfroBIM: could not keep the unsaved work at {}: {e}", t.display());
            }
        } else {
            remove_files([t.as_path()]);
        }
    }

    fn recovery_offer_window(&mut self, ctx: &egui::Context) {
        let Some(offer) = self.recovery.offer.as_ref() else { return };
        let when = format_utc(offer.modified);
        let name = offer.for_doc.as_ref().and_then(|p| p.file_name()).map(|n| n.to_string_lossy().into_owned()).unwrap_or_else(|| "the untitled document".into());
        let primary = format!("Recover unsaved changes from {when}");
        let out = Dialog::titled("Recover unsaved changes").subtitle(name.clone()).width(480.0).resizable(false).show(
            ctx,
            |ui| {
                ui_kit::callout(ui, Level::Warning, format!("AfroBIM stopped before {name} was saved. A copy of the work in progress from {when} was kept."));
                ui_kit::note(ui, "Recovering opens the copy as unsaved changes. Nothing is written to your document until you choose Save. Discarding deletes the copy.");
                ui_kit::note(ui, offer.recover.display().to_string());
            },
            // Escape presses Cancel, so Cancel is the harmless one ("Decide later") and Discard is a
            // secondary button that only a click reaches.
            |ui| Footer::new(&primary).secondary(&["Discard"]).cancel(Some("Decide later")).show(ui),
        );
        let footer = out.inner.map(|(_, f)| f).unwrap_or_default();
        if out.closed || footer.cancel {
            // Closing the window, Escape and "Decide later" decide nothing: the file stays.
            self.recovery.offer = None;
            self.status = "The recovery file was left where it is.".into();
            return;
        }
        if footer.primary {
            if let Some(offer) = self.recovery.offer.take() {
                self.recover_accept(*offer);
            }
        } else if footer.secondary == Some(0) {
            if let Some(offer) = self.recovery.offer.take() {
                remove_files([offer.recover.as_path()]);
                self.status = "The recovery file was discarded.".into();
            }
        }
    }

    /// Loads the recovered document as unsaved changes. The path stays the document's own and the
    /// recovery file stays until a save, so another failure still finds it.
    pub(crate) fn recover_accept(&mut self, offer: Offer) {
        let when = format_utc(offer.modified);
        self.install_document(offer.doc);
        self.recovery.last_written = None;
        self.recovery.note_edit();
        self.status = format!("Recovered the unsaved changes from {when}. Save to keep them.");
    }

    /// The Autosave dialog: the interval, and what is kept and where.
    pub(crate) fn autosave_window(&mut self, ctx: &egui::Context) {
        if !self.recovery.settings_open {
            return;
        }
        let mut minutes = clamp_minutes(self.prefs.autosave_minutes);
        let out = Dialog::titled("Autosave").subtitle("Recovery copies of unsaved work").width(440.0).resizable(false).show(
            ctx,
            |ui| {
                ui_kit::form(ui, "autosave", |f| {
                    f.row("Minutes between copies", |ui| {
                        let mut value = minutes as f64;
                        if ui.add(egui::DragValue::new(&mut value).range(0..=MAX_AUTOSAVE_MINUTES).speed(0.2).suffix(" min")).changed() {
                            minutes = value.round().max(0.0) as u32;
                        }
                    });
                });
                ui.add_space(4.0);
                ui_kit::callout(
                    ui,
                    Level::Info,
                    if minutes == 0 { "Autosave is off. Nothing is copied while you work.".to_string() } else { format!("While there are unsaved changes, a copy is written every {minutes} min to <name>.afrobim.recover beside the document, or to the recover folder of the AfroBIM settings for a document with no name. Saving or quitting removes it. Your document is never overwritten by autosave.") },
                );
            },
            |ui| Footer::new("Done").cancel(None).note("0 switches autosave off").show(ui),
        );
        if minutes != self.prefs.autosave_minutes {
            self.prefs.autosave_minutes = minutes;
            self.save_prefs();
        }
        if out.closed || out.inner.is_some_and(|(_, f)| f.primary || f.cancel) {
            self.recovery.settings_open = false;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::AtomicU64;

    fn folder(label: &str) -> PathBuf {
        static N: AtomicU64 = AtomicU64::new(0);
        let dir = std::env::temp_dir().join(format!("afrobim-recovery-{label}-{}-{}", std::process::id(), N.fetch_add(1, Ordering::Relaxed)));
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    fn doc_with(n: usize) -> Document {
        let mut d = Document::default();
        for i in 0..n {
            d.add("sign", &format!("S-{i}"), afrobim_kernel::Pt::new(i as f64 * 1000.0, 0.0), 500.0, 500.0, "0", serde_json::json!({}));
        }
        d
    }

    fn secs(s: u64) -> SystemTime {
        UNIX_EPOCH + Duration::from_secs(s)
    }

    #[test]
    fn the_recovery_file_sits_beside_the_document_or_in_the_data_folder() {
        let dir = Path::new("/data/recover");
        assert_eq!(target_for(Some(Path::new("/p/road.afrobim")), Some(dir)), Some(PathBuf::from("/p/road.afrobim.recover")));
        assert_eq!(target_for(None, Some(dir)), Some(PathBuf::from("/data/recover/untitled.afrobim.recover")));
        assert_eq!(target_for(None, None), None);
    }

    #[test]
    fn autosave_is_due_after_the_interval_and_never_at_zero_minutes() {
        assert!(!autosave_due(5, Duration::from_secs(299)));
        assert!(autosave_due(5, Duration::from_secs(300)));
        assert!(!autosave_due(0, Duration::from_secs(100_000)));
        assert!(autosave_due(u32::MAX, Duration::from_secs(u64::from(MAX_AUTOSAVE_MINUTES) * 60)));
    }

    #[test]
    fn autosave_writes_only_when_the_document_has_unsaved_changes() {
        let dir = folder("autosave");
        let target = dir.join("road.afrobim.recover");
        let doc = doc_with(3);
        let fp = crate::background::fingerprint(&doc);
        // Saved and unchanged: nothing is written.
        assert_eq!(autosave_if_dirty(&doc, &target, Some(fp), None).unwrap(), Autosaved::Clean);
        assert!(!target.exists());
        // Changed since the save: written.
        let r = autosave_if_dirty(&doc, &target, Some(fp ^ 1), None).unwrap();
        assert_eq!(r, Autosaved::Written(fp));
        assert_eq!(Document::load(&target).unwrap().elements().len(), 3);
        // The same state again: left alone.
        assert_eq!(autosave_if_dirty(&doc, &target, Some(fp ^ 1), Some(fp)).unwrap(), Autosaved::Unchanged);
        // A new document with elements counts as unsaved; an empty one does not.
        std::fs::remove_file(&target).unwrap();
        assert!(matches!(autosave_if_dirty(&doc, &target, None, None).unwrap(), Autosaved::Written(_)));
        std::fs::remove_file(&target).unwrap();
        assert_eq!(autosave_if_dirty(&Document::default(), &target, None, None).unwrap(), Autosaved::Clean);
        assert!(!target.exists());
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn autosave_makes_the_data_folder_for_a_document_with_no_name() {
        let dir = folder("untitled");
        let target = untitled_recover(&dir.join("recover"));
        let r = autosave_if_dirty(&doc_with(1), &target, None, None).unwrap();
        assert!(matches!(r, Autosaved::Written(_)) && target.is_file());
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn the_recovery_file_is_preferred_only_when_it_is_newer() {
        assert!(prefer_recover(Some(secs(100)), secs(101)));
        assert!(!prefer_recover(Some(secs(100)), secs(100)));
        assert!(!prefer_recover(Some(secs(100)), secs(99)));
        assert!(prefer_recover(None, secs(1)));
    }

    fn set_mtime(path: &Path, t: SystemTime) {
        std::fs::File::options().write(true).open(path).unwrap().set_modified(t).unwrap();
    }

    #[test]
    fn a_newer_recovery_file_is_offered_and_an_older_or_identical_one_is_not() {
        let dir = folder("find");
        let doc_path = dir.join("road.afrobim");
        doc_with(2).save(&doc_path).unwrap();
        let rec = recover_path(&doc_path);
        doc_with(3).save(&rec).unwrap();
        set_mtime(&doc_path, secs(1_000));
        set_mtime(&rec, secs(2_000));
        match find_offer(Some(&doc_path), None) {
            Found::Offer(o) => assert!(o.doc.elements().len() == 3 && o.modified == secs(2_000) && o.for_doc.as_deref() == Some(doc_path.as_path())),
            _ => panic!("a newer recovery file is offered"),
        }
        // Older than the document: stale.
        set_mtime(&rec, secs(500));
        assert!(matches!(find_offer(Some(&doc_path), None), Found::Stale(_)));
        // Newer, but the same content as the document: stale.
        doc_with(2).save(&rec).unwrap();
        set_mtime(&rec, secs(2_000));
        assert!(matches!(find_offer(Some(&doc_path), None), Found::Stale(_)));
        // Not a document: damaged, and left alone.
        std::fs::write(&rec, "{ half a file").unwrap();
        set_mtime(&rec, secs(2_000));
        assert!(matches!(find_offer(Some(&doc_path), None), Found::Damaged(..)));
        assert!(rec.exists());
        // No recovery file at all.
        std::fs::remove_file(&rec).unwrap();
        assert!(matches!(find_offer(Some(&doc_path), None), Found::Nothing));
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn the_unnamed_documents_recovery_file_is_offered_when_it_holds_something() {
        let dir = folder("find-untitled");
        assert!(matches!(find_offer(None, Some(&dir)), Found::Nothing));
        let rec = untitled_recover(&dir);
        doc_with(2).save(&rec).unwrap();
        assert!(matches!(find_offer(None, Some(&dir)), Found::Offer(_)));
        Document::default().save(&rec).unwrap();
        assert!(matches!(find_offer(None, Some(&dir)), Found::Stale(_)));
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn a_time_reads_as_utc() {
        assert_eq!(format_utc(secs(0)), "1970-01-01 00:00 UTC");
        assert_eq!(format_utc(secs(1_700_000_000)), "2023-11-14 22:13 UTC");
        assert_eq!(format_utc(secs(1_709_164_800 + 3600 * 5 + 60 * 7)), "2024-02-29 05:07 UTC");
    }

    #[test]
    fn the_panic_path_writes_the_copy_to_the_recovery_file_and_is_safe_to_call_again() {
        let dir = folder("panic");
        let target = dir.join("road.afrobim.recover");
        let slot: SnapshotSlot = Mutex::new(None);
        let busy = AtomicBool::new(false);
        // Nothing to write yet.
        assert!(!write_panic_recovery(&slot, &busy));
        assert!(!target.exists());
        set_snapshot(&slot, Some(target.clone()), &doc_with(4));
        assert!(write_panic_recovery(&slot, &busy));
        assert_eq!(Document::load(&target).unwrap().elements().len(), 4);
        assert!(!busy.load(Ordering::SeqCst), "the guard is released for the next panic");
        // A panic inside the hook is not allowed to start the work again.
        busy.store(true, Ordering::SeqCst);
        std::fs::remove_file(&target).unwrap();
        assert!(!write_panic_recovery(&slot, &busy) && !target.exists());
        busy.store(false, Ordering::SeqCst);
        // A slot that is locked is skipped, not waited for.
        let held = slot.lock().unwrap();
        assert!(!write_panic_recovery(&slot, &busy));
        drop(held);
        // A slot poisoned by a panic is still read.
        let _ = std::thread::scope(|s| s.spawn(|| {
            let _g = slot.lock().unwrap();
            panic!("poison the slot");
        }).join());
        assert!(slot.is_poisoned());
        assert!(write_panic_recovery(&slot, &busy));
        assert!(target.is_file());
        // A target that cannot be written gives false and no panic.
        set_snapshot(&slot, Some(target.join("below-a-file.recover")), &doc_with(1));
        assert!(!write_panic_recovery(&slot, &busy));
        std::fs::remove_dir_all(&dir).unwrap();
    }

    /// An application (as in a normal session, but with the preferences file in a folder of its own).
    fn app_in(dir: &Path) -> AfroBimApp {
        std::env::set_var("AFROBIM_PREFS", dir.join("prefs.json"));
        let ctx = egui::Context::default();
        let cc = eframe::CreationContext::_new_kittest(ctx);
        AfroBimApp::new(&cc, crate::Options::default())
    }

    #[test]
    fn opening_offers_the_newer_copy_recovering_leaves_unsaved_changes_and_saving_removes_it() {
        let dir = folder("app");
        let doc_path = dir.join("road.afrobim");
        let rec = recover_path(&doc_path);
        doc_with(2).save(&doc_path).unwrap();
        doc_with(5).save(&rec).unwrap();
        set_mtime(&doc_path, secs(1_000));
        set_mtime(&rec, secs(2_000));
        let mut app = app_in(&dir);
        app.open_path(doc_path.clone());
        let offer = app.recovery.offer.take().expect("a newer recovery file is offered on open");
        assert_eq!(app.doc.elements().len(), 2, "nothing is loaded until the engineer chooses");
        app.recover_accept(*offer);
        assert_eq!(app.doc.elements().len(), 5);
        assert_eq!(app.doc_path(), Some(doc_path.as_path()), "the document keeps its own name");
        assert!(crate::dropped::has_unsaved(app.drops.saved, crate::background::fingerprint(&app.doc), app.doc.elements().len()), "the recovered work counts as unsaved");
        assert!(rec.is_file(), "the copy stays until a save");
        assert_eq!(Document::load(&doc_path).unwrap().elements().len(), 2, "the document on disk is not touched");
        app.save_to(doc_path.clone());
        assert!(!rec.exists(), "a normal save removes the recovery file");
        assert_eq!(Document::load(&doc_path).unwrap().elements().len(), 5);
        // A clean exit removes the copy autosave left.
        doc_with(6).save(&rec).unwrap();
        app.recovery_exit();
        assert!(!rec.exists());
        // An older copy is not offered, and is cleared away.
        doc_with(7).save(&rec).unwrap();
        set_mtime(&rec, secs(10));
        app.open_path(doc_path.clone());
        assert!(app.recovery.offer.is_none() && !rec.exists());
        std::env::remove_var("AFROBIM_PREFS");
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn the_interval_preference_is_kept_in_range() {
        assert_eq!(clamp_minutes(5), 5);
        assert_eq!(clamp_minutes(0), 0);
        assert_eq!(clamp_minutes(u32::MAX), MAX_AUTOSAVE_MINUTES);
    }

    #[test]
    fn quitting_with_unsaved_changes_keeps_the_copy_and_quitting_clean_removes_it() {
        let d = std::env::temp_dir().join(format!("afrobim-quit-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&d);
        std::fs::create_dir_all(&d).unwrap();
        let ctx = egui::Context::default();
        let cc = eframe::CreationContext::_new_kittest(ctx);
        let mut app = crate::app::AfroBimApp::new(&cc, crate::Options::default());
        let doc_path = d.join("site.afrobim");
        app.doc.save(&doc_path).unwrap();
        app.open_path(doc_path.clone());
        app.doc.add("wall", "W1", afrobim_kernel::Pt::new(0.0, 0.0), 1.0, 1.0, "0", serde_json::json!({}));
        let rec = app.recover_target().expect("a named document has a recovery file");
        app.recovery_exit();
        assert!(rec.exists(), "unsaved work is kept on quit");
        app.save_to(doc_path.clone());
        app.recovery_exit();
        assert!(!rec.exists(), "a clean quit removes the copy");
    }
}
