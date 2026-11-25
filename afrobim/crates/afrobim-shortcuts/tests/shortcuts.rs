// SPDX-License-Identifier: LicenseRef-AfroBIM-Proprietary
// Copyright (c) 2026 AfroBIM. All rights reserved.
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use afrobim_kernel::{set_project_crs, Document, ElementId, ProjectCrs, Pt};
use afrobim_shortcuts::*;
use serde_json::json;

const BY: &str = "A. Surveyor";
const AT: &str = "2026-11-16T09:00";

fn opts() -> ReferenceOptions {
    ReferenceOptions { by: BY.into(), at: AT.into(), ..ReferenceOptions::default() }
}

fn req(path: &str, id: ElementId) -> PublishRequest {
    PublishRequest::new(path, id, BY, AT)
}

struct Fixture {
    dir: tempfile::TempDir,
    surface: ElementId,
}

impl Fixture {
    fn root(&self) -> &Path {
        self.dir.path()
    }
    fn path(&self, name: &str) -> PathBuf {
        self.dir.path().join(name)
    }
    fn load(&self, name: &str) -> Document {
        Document::load(&self.path(name)).unwrap()
    }
    fn save(&self, name: &str, doc: &Document) {
        doc.save(&self.path(name)).unwrap();
    }
}

fn utm(doc: &mut Document, epsg: u32) {
    set_project_crs(doc, &ProjectCrs::from_epsg(epsg).unwrap());
}

/// survey.afrobim: one ground surface at UTM-like coordinates. The project is created and the surface published.
fn fixture() -> (Fixture, ShortcutProject) {
    let dir = tempfile::tempdir().unwrap();
    let mut survey = Document::default();
    utm(&mut survey, 21036);
    let surface = survey.add("civil-surface", "Existing ground", Pt::new(500_000_000.0, 9_800_000_000.0), 200_000.0, 100_000.0, "Civil", json!({"surface": {"points": [[0.0, 0.0, 10.0], [200.0, 0.0, 12.5], [0.0, 100.0, 11.0]]}, "style": {"border": true}}));
    let f = Fixture { dir, surface };
    f.save("survey.afrobim", &survey);
    let mut p = ShortcutProject::create(f.root(), "Test road").unwrap();
    p.publish(&req("survey.afrobim", surface)).unwrap();
    (f, p)
}

/// roads.afrobim: references the surface, holds an alignment and a ground profile sampled from it. Published with its closure.
fn add_roads(f: &Fixture, p: &mut ShortcutProject) -> (ElementId, ElementId) {
    let mut roads = Document::default();
    utm(&mut roads, 21036);
    let rep = create_reference(&mut roads, p, "civil-surface:Existing ground", &opts()).unwrap();
    let surf_ref = rep.root;
    let al = roads.add("alignment", "Main Rd", Pt::new(500_000_000.0, 9_800_000_000.0), 150_000.0, 20_000.0, "Alignments", json!({"stations": [0.0, 150.0], "kind": "line"}));
    let prof = roads.add("civil-profile", "Main ground", Pt::new(500_000_000.0, 9_800_000_000.0), 150_000.0, 20_000.0, "Civil", json!({"alignment": "Main Rd", "surface": surf_ref.0, "interval_m": 10.0, "imported": []}));
    f.save("roads.afrobim", &roads);
    p.publish_closure(&req("roads.afrobim", prof), &DependencyTable::civil()).unwrap();
    (al, prof)
}

#[test]
fn a_project_is_created_published_listed_and_saved_with_forward_slashes() {
    let (f, mut p) = fixture();
    assert!(f.path(MANIFEST_FILE).exists());
    assert_eq!(p.list().len(), 1);
    let s = p.get("civil-surface:Existing ground").unwrap().clone();
    assert_eq!((s.revision, s.source_path.as_str(), s.status), (1, "survey.afrobim", ShareStatus::WorkInProgress));
    assert_eq!((s.published_by.as_str(), s.published_at.as_str()), (BY, AT));
    assert!(s.source_crs.is_some() && s.file_stamp.is_some());
    // The manifest is the file: reopen gives the same thing.
    let again = ShortcutProject::open(f.root()).unwrap();
    assert_eq!(again.manifest(), p.manifest());
    assert!(ShortcutProject::create(f.root(), "x").is_err(), "no overwriting a project");

    // Publishing again with nothing changed does not bump the revision; with a change it does.
    let o = p.publish(&req("survey.afrobim", f.surface)).unwrap();
    assert_eq!((o.action, o.revision), (PublishAction::Unchanged, 1));
    let mut survey = f.load("survey.afrobim");
    survey.find_mut(f.surface).unwrap().data["style"]["border"] = json!(false);
    f.save("survey.afrobim", &survey);
    let o = p.publish(&req("survey.afrobim", f.surface)).unwrap();
    assert_eq!((o.action.clone(), o.revision), (PublishAction::Updated { old_revision: 1 }, 2));

    p.set_status(&o.key, ShareStatus::Published, "Checker", "later").unwrap();
    assert_eq!(p.get(&o.key).unwrap().status, ShareStatus::Published);
    assert_eq!(p.get(&o.key).unwrap().revision, 2);
    assert!(!std::fs::read_to_string(f.path(MANIFEST_FILE)).unwrap().contains('\\'));
    p.remove(&o.key).unwrap();
    assert!(p.list().is_empty() && p.remove(&o.key).is_err());
}

#[test]
fn resaving_an_unchanged_source_does_not_change_its_hash() {
    let (f, p) = fixture();
    let h = p.get("civil-surface:Existing ground").unwrap().hash.clone();
    // Load and save again (a different JSON writer pass), then rewrite the file by hand with
    // reordered keys, other whitespace and 1.0 written as 1.
    let doc = f.load("survey.afrobim");
    f.save("survey.afrobim", &doc);
    let mut v: serde_json::Value = serde_json::from_str(&std::fs::read_to_string(f.path("survey.afrobim")).unwrap()).unwrap();
    let el = &mut v["elements"][1]; // [0] is the project settings element
    assert_eq!(el["name"], "Existing ground");
    el["width"] = serde_json::from_str("200000").unwrap();
    std::fs::write(f.path("survey.afrobim"), serde_json::to_string(&v).unwrap().replace(",\"data\"", ",   \"data\"")).unwrap();
    let back = f.load("survey.afrobim");
    assert_eq!(element_hash(back.find(f.surface).unwrap()), h);
    assert!(p.validate().is_empty(), "{:?}", p.validate());
}

#[test]
fn a_profile_reference_brings_its_alignment_and_surface_and_rewrites_ids() {
    let (f, mut p) = fixture();
    let (_al, _prof) = add_roads(&f, &mut p);
    assert_eq!(p.list().len(), 3, "surface, alignment and profile");

    let mut host = Document::default();
    utm(&mut host, 21036);
    let rep = create_reference(&mut host, &p, "civil-profile:Main ground", &opts()).unwrap();
    assert_eq!(rep.items.len(), 3);
    assert_eq!(rep.items.iter().filter(|i| i.created).count(), 3);
    let names: Vec<_> = rep.items.iter().map(|i| host.find(i.element).unwrap().name.clone()).collect();
    assert_eq!(names.last().unwrap(), "Main ground", "the root comes last");
    assert!(names.contains(&"Main Rd".to_string()) && names.contains(&"Existing ground".to_string()));

    let prof = host.find(rep.root).unwrap();
    let surf = host.elements().iter().find(|e| e.category == "civil-surface").unwrap();
    assert_eq!(prof.data["surface"], json!(surf.id.0), "the id now names the host's reference to the surface");
    assert_eq!(prof.data["alignment"], "Main Rd");
    assert!(reference_info(prof).unwrap().id_map.values().any(|h| *h == surf.id.0), "the id map records source id to host id");
    // Referencing again adds nothing.
    let again = create_reference(&mut host, &p, "civil-profile:Main ground", &opts()).unwrap();
    assert!(again.items.iter().all(|i| !i.created));
    assert_eq!(host.elements().iter().filter(|e| is_reference(e)).count(), 3);
    // The host opens and holds everything with the sources gone.
    let text = host.to_json();
    std::fs::remove_file(f.path("survey.afrobim")).unwrap();
    std::fs::remove_file(f.path("roads.afrobim")).unwrap();
    let reopened = Document::from_json(&text).unwrap();
    assert_eq!(reopened.elements(), host.elements());
}

#[test]
fn references_are_read_only_at_source_coordinates_and_never_touch_the_source() {
    let (f, p) = fixture();
    let before = std::fs::read(f.path("survey.afrobim")).unwrap();
    let manifest_before = std::fs::read(f.path(MANIFEST_FILE)).unwrap();
    let src = f.load("survey.afrobim").find(f.surface).unwrap().clone();
    let mut host = Document::default();
    let rep = create_reference(&mut host, &p, "civil-surface:Existing ground", &opts()).unwrap();
    let r = host.find(rep.root).unwrap();
    assert_eq!((r.origin, r.width, r.height, &r.site, &r.data, &r.category, &r.name), (src.origin, src.width, src.height, &src.site, &src.data, &src.category, &src.name));
    assert_eq!(r.layer, "REF-Civil");
    let info = reference_info(r).unwrap();
    assert!(info.locked && info.source_crs.is_some());
    assert_eq!((info.source_revision, info.source_hash.as_str(), info.source_path.as_str()), (1, p.get(&info.shortcut_key).unwrap().hash.as_str(), "survey.afrobim"));
    assert!(matches!(check_editable(&host, rep.root), Err(ShortcutError::Locked(_))));
    assert_eq!(locked_among(&host, &[rep.root, ElementId(999)]), vec![rep.root]);
    assert!(check_editable(&host, ElementId(999)).is_ok());
    // Unlocking is explicit and visible.
    set_locked(&mut host, rep.root, false).unwrap();
    assert!(check_editable(&host, rep.root).is_ok());
    assert!(!host.find(rep.root).unwrap().properties.contains_key(LOCKED_PROP));
    assert_eq!(std::fs::read(f.path("survey.afrobim")).unwrap(), before);
    assert_eq!(std::fs::read(f.path(MANIFEST_FILE)).unwrap(), manifest_before);
}

#[test]
fn a_corridor_brings_alignments_assemblies_and_the_ground() {
    let (f, mut p) = fixture();
    let (_al, _prof) = add_roads(&f, &mut p);
    // The corridor document references the ground and the alignment, and holds its own assembly and corridor.
    let mut cor = Document::default();
    utm(&mut cor, 21036);
    let g = create_reference(&mut cor, &p, "civil-surface:Existing ground", &opts()).unwrap().root;
    create_reference(&mut cor, &p, "alignment:Main Rd", &opts()).unwrap();
    let asm = cor.add("civil-assembly", "Rural 7.3", Pt::new(0.0, 0.0), 1000.0, 1000.0, "Civil", json!({"name": "Rural 7.3", "left": [], "right": []}));
    let model = json!({"name": "C1", "baselines": [{"name": "B1", "alignment": "Main Rd", "regions": [{"name": "R1", "start_station_m": 0.0, "end_station_m": 150.0, "assembly": {"name": "Rural 7.3"}}, {"name": "R2", "start_station_m": 150.0, "end_station_m": 300.0, "assembly": {"name": "Not an element"}}]}]});
    let c = cor.add("civil-corridor", "Corridor 1", Pt::new(500_000_000.0, 9_800_000_000.0), 150_000.0, 20_000.0, "Civil", json!({"model": model, "ground": g.0}));
    f.save("corridor.afrobim", &cor);
    let _ = (asm, c);
    let outs = p.publish_closure(&req("corridor.afrobim", c), &DependencyTable::civil()).unwrap();
    let keys: Vec<_> = outs.iter().map(|o| o.key.clone()).collect();
    assert_eq!(keys, vec!["civil-assembly:Rural 7.3", "civil-corridor:Corridor 1"], "references are not republished; the unknown assembly name is skipped");

    let mut drain = Document::default();
    let rep = create_reference(&mut drain, &p, "civil-corridor:Corridor 1", &opts()).unwrap();
    let cats: Vec<String> = rep.items.iter().map(|i| drain.find(i.element).unwrap().category.clone()).collect();
    for want in ["civil-surface", "alignment", "civil-assembly", "civil-corridor"] {
        assert!(cats.iter().any(|c| c == want), "{want} in {cats:?}");
    }
    let surf = drain.elements().iter().find(|e| e.category == "civil-surface").unwrap().id;
    assert_eq!(drain.find(rep.root).unwrap().data["ground"], json!(surf.0));
}

#[test]
fn an_unpublished_dependency_refuses_the_reference_and_says_which() {
    let (f, mut p) = fixture();
    let mut roads = Document::default();
    let al = roads.add("alignment", "Side Rd", Pt::new(0.0, 0.0), 1.0, 1.0, "A", json!({}));
    let prof = roads.add("civil-profile", "Side ground", Pt::new(0.0, 0.0), 1.0, 1.0, "A", json!({"alignment": "Side Rd", "surface": 0, "interval_m": 5.0}));
    let _ = al;
    f.save("side.afrobim", &roads);
    p.publish(&req("side.afrobim", prof)).unwrap();
    let mut host = Document::default();
    let err = create_reference(&mut host, &p, "civil-profile:Side ground", &opts()).unwrap_err().to_string();
    assert!(err.contains("Side Rd") || err.contains("not published") || err.contains("is not in"), "{err}");
    assert!(host.elements().is_empty(), "nothing is added when the closure fails");
}

#[test]
fn status_reports_up_to_date_out_of_date_missing_removed_and_crs() {
    let (f, mut p) = fixture();
    let (_al, prof) = add_roads(&f, &mut p);
    let mut host = Document::default();
    utm(&mut host, 21036);
    create_reference(&mut host, &p, "civil-profile:Main ground", &opts()).unwrap();
    let st = status(&host, &p, &StatusOptions::default());
    assert_eq!(st.len(), 3);
    assert!(st.iter().all(|s| s.state == RefState::UpToDate && !s.crs_differs && s.crs_messages.is_empty()), "{st:?}");

    // The roads team edits the profile and publishes: out of date, with both revisions.
    let mut roads = f.load("roads.afrobim");
    roads.find_mut(prof).unwrap().data["interval_m"] = json!(25.0);
    f.save("roads.afrobim", &roads);
    // Edited but not published: flagged as unpublished, the reference is still up to date.
    let st = status(&host, &p, &StatusOptions::default());
    let row = st.iter().find(|s| s.key == "civil-profile:Main ground").unwrap();
    assert_eq!(row.state, RefState::UpToDate);
    assert!(row.unpublished_changes);
    p.publish(&req("roads.afrobim", prof)).unwrap();
    let st = status(&host, &p, &StatusOptions::default());
    let row = st.iter().find(|s| s.key == "civil-profile:Main ground").unwrap();
    match &row.state {
        RefState::OutOfDate { old_revision, new_revision, old_hash, new_hash } => {
            assert_eq!((*old_revision, *new_revision), (1, 2));
            assert_ne!(old_hash, new_hash);
        }
        other => panic!("{other:?}"),
    }
    assert!(!row.unpublished_changes);

    // Source missing and shortcut removed.
    std::fs::rename(f.path("survey.afrobim"), f.path("survey-moved.afrobim")).unwrap();
    p.remove("alignment:Main Rd").unwrap();
    let st = status(&host, &p, &StatusOptions::default());
    let get = |k: &str| st.iter().find(|s| s.key == k).unwrap().state.clone();
    assert!(matches!(get("civil-surface:Existing ground"), RefState::SourceMissing { .. }));
    assert_eq!(get("alignment:Main Rd"), RefState::ShortcutRemoved);
    // Without looking at the files, only the manifest is read.
    let st = status(&host, &p, &StatusOptions { check_sources: false });
    assert_eq!(st.iter().find(|s| s.key == "civil-surface:Existing ground").unwrap().state, RefState::UpToDate);

    // A host in another system is told, and nothing is transformed.
    let mut other = Document::default();
    utm(&mut other, 21035);
    let origin_before;
    {
        let (f2, p2) = fixture();
        let rep = create_reference(&mut other, &p2, "civil-surface:Existing ground", &opts()).unwrap();
        origin_before = other.find(rep.root).unwrap().origin;
        assert!(rep.crs_messages.iter().any(|m| m.contains("different systems")), "{:?}", rep.crs_messages);
        let st = status(&other, &p2, &StatusOptions::default());
        assert!(st[0].crs_differs);
        assert_eq!(other.find(rep.root).unwrap().origin, origin_before);
        assert_eq!(origin_before, f2.load("survey.afrobim").find(f2.surface).unwrap().origin);
    }
}

#[test]
fn synchronise_refreshes_the_copy_and_reports_the_field_level_difference() {
    let (f, mut p) = fixture();
    let (_al, prof) = add_roads(&f, &mut p);
    let mut host = Document::default();
    let rep = create_reference(&mut host, &p, "civil-profile:Main ground", &opts()).unwrap();
    let rev_before = host.find(rep.root).unwrap().properties["civil.rev"].clone();

    let mut roads = f.load("roads.afrobim");
    {
        let e = roads.find_mut(prof).unwrap();
        e.data["interval_m"] = json!(25.0);
        e.data["note"] = json!("regraded");
        e.data.as_object_mut().unwrap().remove("imported");
        e.origin = Pt::new(e.origin.x + 5000.0, e.origin.y);
        e.width = 160_000.0;
    }
    f.save("roads.afrobim", &roads);

    // Not yet published: skipped with a reason.
    let r = synchronise(&mut host, &p, &SyncTarget::All, &opts());
    let item = r.items.iter().find(|i| i.key == "civil-profile:Main ground").unwrap();
    assert_eq!(item.outcome, SyncOutcome::AlreadyUpToDate, "the shortcut itself has not changed");

    p.publish(&req("roads.afrobim", prof)).unwrap();
    let r = synchronise(&mut host, &p, &SyncTarget::Keys(vec!["civil-profile:Main ground".into()]), &opts());
    assert_eq!(r.items.len(), 1);
    let SyncOutcome::Refreshed { old_revision, new_revision, changes } = &r.items[0].outcome else { panic!("{:?}", r.items[0]) };
    assert_eq!((*old_revision, *new_revision), (1, 2));
    let by_path: BTreeMap<_, _> = changes.fields.iter().map(|c| (c.path.as_str(), c.kind)).collect();
    assert_eq!(by_path["interval_m"], ChangeKind::Changed);
    assert_eq!(by_path["note"], ChangeKind::Added);
    assert_eq!(by_path["imported"], ChangeKind::Removed);
    assert!(changes.placement_changed);
    assert_eq!(changes.bbox_before.max_x - changes.bbox_before.min_x, 150_000.0);
    assert_eq!(changes.bbox_after.max_x - changes.bbox_after.min_x, 160_000.0);
    assert_eq!(changes.bbox_after.min_x, changes.bbox_before.min_x + 5000.0);
    assert_eq!(r.changed_elements(), vec![rep.root]);

    let e = host.find(rep.root).unwrap();
    assert_eq!(e.data["interval_m"], json!(25.0));
    assert_eq!(e.data["surface"], json!(host.elements().iter().find(|x| x.category == "civil-surface").unwrap().id.0), "ids stay rewritten");
    assert_ne!(e.properties["civil.rev"], rev_before, "the application's cache key moved");
    let info = reference_info(e).unwrap();
    assert!(info.locked);
    assert_eq!(info.source_revision, 2);
    assert!(status(&host, &p, &StatusOptions::default()).iter().all(|s| s.state == RefState::UpToDate));
    let again = synchronise(&mut host, &p, &SyncTarget::All, &opts());
    assert!(again.items.iter().all(|i| i.outcome == SyncOutcome::AlreadyUpToDate));
}

#[test]
fn synchronise_does_not_follow_unpublished_edits() {
    let (f, mut p) = fixture();
    let mut host = Document::default();
    create_reference(&mut host, &p, "civil-surface:Existing ground", &opts()).unwrap();
    let mut survey = f.load("survey.afrobim");
    survey.find_mut(f.surface).unwrap().data["style"]["border"] = json!(false);
    f.save("survey.afrobim", &survey);
    p.publish(&req("survey.afrobim", f.surface)).unwrap();
    // A second edit after the publish: the file no longer holds what was published.
    survey.find_mut(f.surface).unwrap().data["style"]["extra"] = json!("a longer value so the size differs");
    f.save("survey.afrobim", &survey);
    let r = synchronise(&mut host, &p, &SyncTarget::All, &opts());
    let SyncOutcome::Skipped { reason } = &r.items[0].outcome else { panic!("{:?}", r.items) };
    assert!(reason.contains("Publish it"), "{reason}");
    assert_eq!(host.elements().iter().find(|e| is_reference(e)).unwrap().data["style"]["border"], json!(true), "the copy is untouched");
    // And a new reference refuses too.
    let mut other = Document::default();
    assert!(create_reference(&mut other, &p, "civil-surface:Existing ground", &opts()).is_err());
}

#[test]
fn promote_makes_an_editable_copy_and_records_where_it_came_from() {
    let (f, p) = fixture();
    let mut host = Document::default();
    let id = create_reference(&mut host, &p, "civil-surface:Existing ground", &opts()).unwrap().root;
    let data = host.find(id).unwrap().data.clone();
    let from = promote(&mut host, id, "Designer", "now").unwrap();
    let e = host.find(id).unwrap();
    assert!(!is_reference(e) && check_editable(&host, id).is_ok());
    assert_eq!(e.layer, "Civil");
    assert_eq!(e.data, data);
    assert_eq!(promoted_from(e).unwrap(), from);
    assert_eq!((from.shortcut_key.as_str(), from.source_revision, from.promoted_by.as_str()), ("civil-surface:Existing ground", 1, "Designer"));
    assert!(status(&host, &p, &StatusOptions::default()).is_empty(), "no longer followed");
    assert!(promote(&mut host, id, "x", "y").is_err());
    // Now it may be referenced again only after renaming: the name is taken by the promoted copy.
    assert!(create_reference(&mut host, &p, "civil-surface:Existing ground", &opts()).is_err());
    let _ = f;
}

#[test]
fn a_moved_source_is_repaired_without_changing_hashes() {
    let (f, mut p) = fixture();
    std::fs::create_dir_all(f.path("survey")).unwrap();
    std::fs::rename(f.path("survey.afrobim"), f.path("survey/ground.afrobim")).unwrap();
    let issues = p.validate();
    assert_eq!(issues.len(), 1);
    assert_eq!(issues[0].kind, IssueKind::MissingFile);
    let rep = p.repair_source_path("survey.afrobim", Path::new("survey/ground.afrobim")).unwrap();
    assert_eq!(rep.repaired, vec!["civil-surface:Existing ground".to_string()]);
    assert!(rep.content_differs.is_empty());
    assert_eq!(p.get("civil-surface:Existing ground").unwrap().source_path, "survey/ground.afrobim");
    assert!(p.validate().is_empty());
    // A file without the element is refused and nothing changes.
    let empty = Document::default();
    f.save("empty.afrobim", &empty);
    assert!(p.repair_source_path("survey/ground.afrobim", Path::new("empty.afrobim")).is_err());
    assert_eq!(p.get("civil-surface:Existing ground").unwrap().source_path, "survey/ground.afrobim");
    // References still resolve through the key.
    let mut host = Document::default();
    create_reference(&mut host, &p, "civil-surface:Existing ground", &opts()).unwrap();
}

#[test]
fn validate_finds_missing_elements_duplicates_and_hash_mismatches() {
    let (f, mut p) = fixture();
    let (al, _prof) = add_roads(&f, &mut p);
    assert!(p.validate().is_empty());
    // Edit the surface without publishing, delete the alignment from its file.
    let mut survey = f.load("survey.afrobim");
    survey.find_mut(f.surface).unwrap().width += 1.0;
    f.save("survey.afrobim", &survey);
    let mut roads = f.load("roads.afrobim");
    roads.remove(al);
    f.save("roads.afrobim", &roads);
    let issues = p.validate();
    let kinds: Vec<_> = issues.iter().map(|i| (i.kind, i.key.as_str())).collect();
    assert!(kinds.contains(&(IssueKind::HashMismatch, "civil-surface:Existing ground")), "{kinds:?}");
    assert!(kinds.contains(&(IssueKind::MissingElement, "alignment:Main Rd")), "{kinds:?}");
    // Two keys for the same name in a category (a hand-merged manifest) are reported.
    let mut m = p.manifest().clone();
    let mut dup = m.shortcuts[0].clone();
    dup.key = "other-key".into();
    m.shortcuts.push(dup.clone());
    m.shortcuts.push(dup);
    std::fs::write(f.path(MANIFEST_FILE), m.to_json()).unwrap();
    let issues = ShortcutProject::open(f.root()).unwrap().validate();
    assert!(issues.iter().any(|i| i.kind == IssueKind::DuplicateName));
    assert!(issues.iter().any(|i| i.kind == IssueKind::DuplicateKey));
}

#[test]
fn publish_refuses_references_duplicate_names_and_unreadable_manifests() {
    let (f, mut p) = fixture();
    let mut host = Document::default();
    let id = create_reference(&mut host, &p, "civil-surface:Existing ground", &opts()).unwrap().root;
    f.save("host.afrobim", &host);
    let e = p.publish(&req("host.afrobim", id)).unwrap_err().to_string();
    assert!(e.contains("reference"), "{e}");
    // Same name and category under another key.
    let mut other = Document::default();
    let sid = other.add("civil-surface", "existing GROUND", Pt::new(0.0, 0.0), 1.0, 1.0, "C", json!({}));
    f.save("other.afrobim", &other);
    assert!(p.publish(&req("other.afrobim", sid)).is_err());
    // Manifest versions.
    std::fs::write(f.path(MANIFEST_FILE), r#"{"format":"afrobim-shortcuts","version":99,"shortcuts":[]}"#).unwrap();
    assert!(ShortcutProject::open(f.root()).is_err());
    std::fs::write(f.path(MANIFEST_FILE), r#"{"format":"nope","version":1,"shortcuts":[]}"#).unwrap();
    assert!(ShortcutProject::open(f.root()).is_err());
}

#[test]
fn stale_detection_opens_no_files() {
    let (f, mut p) = fixture();
    let (_al, prof) = add_roads(&f, &mut p);
    let mut host = Document::default();
    create_reference(&mut host, &p, "civil-profile:Main ground", &opts()).unwrap();
    let rec = recorded_references(&host);
    assert_eq!(rec.len(), 3);
    assert!(stale(p.manifest(), &rec).is_empty());
    let mut roads = f.load("roads.afrobim");
    roads.find_mut(prof).unwrap().data["interval_m"] = json!(40.0);
    f.save("roads.afrobim", &roads);
    p.publish(&req("roads.afrobim", prof)).unwrap();
    // The files could now be gone; the check still works from the manifest alone.
    std::fs::remove_file(f.path("roads.afrobim")).unwrap();
    let s = stale(p.manifest(), &rec);
    assert_eq!(s.len(), 1);
    assert_eq!((s[0].recorded_revision, s[0].current_revision), (1, Some(2)));
    p.remove("alignment:Main Rd").unwrap();
    let s = stale(p.manifest(), &rec);
    assert!(s.iter().any(|x| x.key == "alignment:Main Rd" && x.current_revision.is_none()));

    // File-level pre-check: only a file whose stamp differs from the published one is listed.
    let (f, p) = fixture();
    let mut stamps = BTreeMap::new();
    stamps.insert("survey.afrobim".to_string(), FileStamp::of(&f.path("survey.afrobim")).unwrap());
    assert!(changed_sources(p.manifest(), f.root(), &stamps).is_empty());
    let mut s2 = f.load("survey.afrobim");
    s2.find_mut(f.surface).unwrap().data["pad"] = json!("x".repeat(50));
    f.save("survey.afrobim", &s2);
    stamps.insert("survey.afrobim".to_string(), FileStamp::of(&f.path("survey.afrobim")).unwrap());
    assert_eq!(changed_sources(p.manifest(), f.root(), &stamps), vec!["civil-surface:Existing ground".to_string()]);
    assert!(changed_sources(p.manifest(), f.root(), &BTreeMap::new()).len() == 1, "no stamp, no assurance");
    assert_eq!(file_hash(&f.path("survey.afrobim")).unwrap().len(), 64);
}

#[test]
fn a_whole_document_is_attached_as_an_underlay_and_followed() {
    let (f, p) = fixture();
    let _ = p;
    let mut survey = f.load("survey.afrobim");
    survey.add("civil-points", "Control", Pt::new(1.0, 1.0), 1.0, 1.0, "Control", json!({}));
    f.save("survey.afrobim", &survey);
    let before = std::fs::read(f.path("survey.afrobim")).unwrap();

    let mut host = Document::default();
    utm(&mut host, 21036);
    let r = attach_document(&mut host, f.root(), "Survey", Path::new("survey.afrobim"), &["Civil".to_string()], true, BY, AT).unwrap();
    assert_eq!(r.path, "survey.afrobim");
    assert_eq!(r.element_count, 2);
    assert!(attach_document(&mut host, f.root(), "survey", Path::new("survey.afrobim"), &[], true, BY, AT).is_err());
    // Saved with the host.
    let reopened = Document::from_json(&host.to_json()).unwrap();
    assert_eq!(document_references(&reopened), document_references(&host));

    let st = document_status(&host, f.root(), true);
    assert_eq!(st[0].state, DocState::UpToDate);
    assert!(!st[0].crs_differs);

    let under = load_underlay(f.root(), &document_references(&host)[0]).unwrap();
    assert_eq!(under.elements().len(), 1, "only the Civil layer");
    assert_eq!(under.elements()[0].origin, Pt::new(500_000_000.0, 9_800_000_000.0), "at its own coordinates");

    // Source changes: out of date; refreshing reports it; resaving unchanged does not flag.
    let doc = f.load("survey.afrobim");
    f.save("survey.afrobim", &doc);
    assert_eq!(document_status(&host, f.root(), false)[0].state, DocState::UpToDate);
    let mut changed = doc.clone();
    changed.add("civil-points", "More", Pt::new(2.0, 2.0), 1.0, 1.0, "Control", json!({"a": "longer"}));
    f.save("survey.afrobim", &changed);
    assert!(matches!(document_status(&host, f.root(), true)[0].state, DocState::OutOfDate { .. }));
    let rf = refresh_document(&mut host, f.root(), "Survey").unwrap();
    assert!(rf.hash_changed);
    assert_eq!((rf.elements_before, rf.elements_after), (2, 3));
    assert_eq!(document_status(&host, f.root(), true)[0].state, DocState::UpToDate);

    // Missing, and a different system.
    let mut other = Document::default();
    utm(&mut other, 21035);
    other.add("x", "x", Pt::default(), 1.0, 1.0, "0", json!({}));
    set_document_view(&mut host, "Survey", &[], false).unwrap();
    std::fs::rename(f.path("survey.afrobim"), f.path("gone.afrobim")).unwrap();
    assert!(matches!(document_status(&host, f.root(), true)[0].state, DocState::SourceMissing { .. }));
    std::fs::rename(f.path("gone.afrobim"), f.path("survey.afrobim")).unwrap();
    let mut host2 = other;
    attach_document(&mut host2, f.root(), "Survey", Path::new("survey.afrobim"), &[], true, BY, AT).unwrap();
    assert!(document_status(&host2, f.root(), true)[0].crs_differs);
    assert!(detach_document(&mut host2, "Survey") && document_references(&host2).is_empty());
    assert_ne!(std::fs::read(f.path("survey.afrobim")).unwrap(), before, "it is the test that changed the file, not the library");
}

#[test]
fn the_dependency_table_can_be_extended_by_the_application() {
    let (f, mut p) = fixture();
    let mut d = Document::default();
    let culvert = d.add("culvert", "C1", Pt::new(0.0, 0.0), 1.0, 1.0, "D", json!({}));
    let drain = d.add("drain", "D1", Pt::new(0.0, 0.0), 1.0, 1.0, "D", json!({"outfall": "C1", "links": [culvert.0]}));
    f.save("drainage.afrobim", &d);
    // With the built-in table a drain has no dependencies.
    assert_eq!(local_closure(&f.load("drainage.afrobim"), drain, &DependencyTable::civil()), vec![drain]);
    let mut table = DependencyTable::civil();
    table.add(DependencyRule::new("drain", "outfall", TargetBy::Name, &["culvert"], true));
    table.add(DependencyRule::new("drain", "links[]", TargetBy::Id, &["culvert"], true));
    let outs = p.publish_closure(&req("drainage.afrobim", drain), &table).unwrap();
    assert_eq!(outs.len(), 2);
    let mut host = Document::default();
    let o = ReferenceOptions { table, ..opts() };
    let rep = create_reference(&mut host, &p, "drain:D1", &o).unwrap();
    assert_eq!(rep.items.len(), 2);
    let cul = host.elements().iter().find(|e| e.category == "culvert").unwrap().id;
    assert_eq!(host.find(rep.root).unwrap().data["links"], json!([cul.0]));
}
