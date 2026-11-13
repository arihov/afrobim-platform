//! Writes a `.afrobim` file with `afrobim_kernel::Document` and the sign engine directly,
//! following the desktop app's glue (`afrobim-app/src/signs_glue.rs` and `App::insert_sign`):
//! category `sign`, layer `Signs`, the element's name a unique mark, its data the serialised
//! definition, its size the laid-out drawing in millimetres.
//!
//! `tests/test_document.py` builds the same document through the Python API and compares the
//! two files.
//!
//! Usage: kernel-reference <pack.json> <fonts dir> <out.afrobim>

use std::path::Path;

use afrobim_kernel::{Document, Pt};
use afrobim_signs::{Destination, Direction, SignDefinition, SignEngine};

const CATEGORY: &str = "sign";
const LAYER: &str = "Signs";

/// Adds the sign as the app does once it has generated the element's geometry.
fn add(doc: &mut Document, engine: &SignEngine, mark: &str, def: &SignDefinition, at: Option<Pt>) -> afrobim_kernel::ElementId {
    let name = doc.unique_name(mark);
    let mut named = def.clone();
    named.code = name.clone();
    let layout = engine.layout(&named).expect("the reference signs lay out");
    let drawing = layout.drawing.scaled_to_origin(layout.sw_mm);
    let (w, h) = (drawing.extents.width(), drawing.extents.height());
    let at = at.unwrap_or_else(|| doc.next_free_origin(w, h));
    doc.add(CATEGORY, &name, at, w, h, LAYER, serde_json::to_value(def).unwrap())
}

fn main() {
    let args: Vec<String> = std::env::args().collect();
    let [_, pack, fonts, out] = args.as_slice() else {
        eprintln!("usage: kernel-reference <pack.json> <fonts dir> <out.afrobim>");
        std::process::exit(2);
    };
    let engine = SignEngine::load(Path::new(pack), Path::new(fonts)).expect("the pack and fonts load");

    let mut doc = Document::default();
    doc.standard = engine.pack_id().to_string();

    // The app's seed sign, twice: the second takes the next free mark and the next free place.
    let seed = SignDefinition::stack(
        "",
        60,
        vec![Destination::new("Kasese", Direction::Ahead), Destination::new("Masheruka", Direction::Right)],
    );
    add(&mut doc, &engine, "ADS-001", &seed, None);
    add(&mut doc, &engine, "ADS-001", &seed, None);

    // A catalogued sign, inserted from the library under its key, at a chosen place.
    let entry = engine.catalog_entry("UK06.1").expect("the UK pack catalogues UK06.1");
    let direction = engine.definition_for_speed(entry, 80);
    let id = add(&mut doc, &engine, &entry.key, &direction, Some(Pt::new(5000.0, -3000.0)));
    let props = &mut doc.find_mut(id).unwrap().properties;
    props.insert("Material".to_string(), "Class RA2".to_string());
    props.insert("Drawing".to_string(), "TS-101".to_string());

    doc.save(Path::new(out)).expect("the file is written");
}
