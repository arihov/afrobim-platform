//! Tests of the U4 work: what the 3D view receives from the civil objects, and the dependency
//! refresh of the dependent views.

use std::collections::HashMap;

use afrobim_kernel::{Document, ElementId};

use crate::civil::*;
use crate::civil_cache::{build_all, dependency_hash};
use crate::civil_demo::build;

fn demo_doc() -> Document {
    let mut doc = Document::default();
    let notes = build(&mut doc);
    assert!(notes.is_empty(), "{notes:?}");
    doc
}

fn scenes(doc: &Document) -> HashMap<ElementId, crate::civil_build::Built> {
    let elements = doc.elements().iter().filter(|e| is_civil(&e.category) || e.category == afrobim_alignment::CATEGORY).cloned().collect();
    let stale: Vec<ElementId> = doc.elements().iter().filter(|e| is_civil(&e.category)).map(|e| e.id).collect();
    build_all(elements, HashMap::new(), stale, dependency_hash(doc)).into_iter().map(|(id, _, b)| (id, b)).collect()
}

#[test]
fn the_corridor_gives_solids_and_linework_for_the_3d_view() {
    let doc = demo_doc();
    let built = scenes(&doc);
    let cor = doc.elements().iter().find(|e| e.category == CORRIDOR).unwrap();
    let sc = &built[&cor.id].scene;
    let names: Vec<&str> = sc.meshes.iter().map(|m| m.name.as_str()).collect();
    eprintln!("corridor meshes: {:?} lines3 {}", sc.meshes.iter().map(|m| (m.name.clone(), m.triangles.len())).collect::<Vec<_>>(), sc.lines3.len());
    assert!(sc.meshes.iter().any(|m| !m.triangles.is_empty()), "{names:?}");
    assert!(!sc.lines3.is_empty());
}

#[test]
fn parcels_and_surfaces_have_3d_outlines() {
    let doc = demo_doc();
    let built = scenes(&doc);
    for cat in [PARCEL, SURFACE] {
        for e in doc.elements().iter().filter(|e| e.category == cat) {
            assert!(!built[&e.id].scene.lines3.is_empty(), "{} has no 3D linework", e.name);
        }
    }
}

#[test]
fn an_edited_alignment_or_imported_surface_changes_what_the_dependent_views_read() {
    let mut doc = demo_doc();
    let before = dependency_hash(&doc);
    assert_eq!(before, dependency_hash(&doc), "the hash is stable");
    // A new alignment, as an edit of the alignments would make.
    let al = crate::civil_demo::alignment();
    let id = crate::alignment_glue::alignments(&doc).first().map(|a| a.0).unwrap();
    let mut other = al.clone();
    other.profile = None;
    if let Some(e) = doc.find_mut(id) {
        e.data = serde_json::to_value(&other).unwrap();
    }
    assert_ne!(before, dependency_hash(&doc), "an alignment edit is seen");
}

#[test]
fn a_changed_imported_surface_is_seen_by_the_digest() {
    use crate::civil_cache::dependency_hash;
    let mut doc = Document::default();
    let tin = afrobim_corridor::Tin::new(vec![[0.0, 0.0, 1.0], [10.0, 0.0, 2.0], [0.0, 10.0, 3.0]], vec![[0, 1, 2]]).unwrap();
    let id = crate::alignment_glue::add_surface_at_source(&mut doc, "Imported", &tin, Some("test")).unwrap();
    let a = dependency_hash(&doc);
    let tin2 = afrobim_corridor::Tin::new(vec![[0.0, 0.0, 1.0], [10.0, 0.0, 2.0], [0.0, 10.0, 9.0]], vec![[0, 1, 2]]).unwrap();
    let def = afrobim_corridor::SurfaceDefinition { tin: tin2, ..serde_json::from_value(doc.find(id).unwrap().data.clone()).unwrap() };
    doc.find_mut(id).unwrap().data = serde_json::to_value(&def).unwrap();
    assert_ne!(a, dependency_hash(&doc));
}
