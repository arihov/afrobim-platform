//! Road alignments in the app: LandXML import, and placing a sign by station and offset.
//!
//! An alignment lives in plan (site) coordinates, kilometres away from the design sheet where
//! sign faces are laid out, so its layer is hidden on the sheet. It is used for placement and
//! shown in the 3D site view.

use std::path::Path;

use afrobim_alignment::{Alignment, SignLocation, SignSide, TrafficSide};
use afrobim_kernel::{Document, ElementId, ModuleRegistry, Pt, SitePlacement};

pub const CATEGORY: &str = afrobim_alignment::CATEGORY;
pub const LAYER: &str = "Alignments";

pub fn register(modules: &mut ModuleRegistry) {
    modules.register(Box::new(afrobim_alignment::AlignmentModule));
}

pub fn alignment_of(doc: &Document, id: ElementId) -> Option<Alignment> {
    let e = doc.find(id)?;
    if e.category != CATEGORY {
        return None;
    }
    serde_json::from_value(e.data.clone()).ok()
}

/// The alignments in the document: element id and name.
pub fn alignments(doc: &Document) -> Vec<(ElementId, String)> {
    doc.elements().iter().filter(|e| e.category == CATEGORY).map(|e| (e.id, e.name.clone())).collect()
}

/// Adds one alignment as an element. Its site placement records where its plan drawing sits.
pub fn add(doc: &mut Document, alignment: &Alignment) -> Option<ElementId> {
    let plan = afrobim_alignment::plan_drawing(alignment)?;
    let (w, h) = (plan.drawing.extents.width(), plan.drawing.extents.height());
    let name = doc.unique_name(if alignment.name.trim().is_empty() { "Alignment" } else { alignment.name.trim() });
    let mut stored = alignment.clone();
    stored.name = name.clone();
    let data = serde_json::to_value(&stored).ok()?;
    let id = doc.add(CATEGORY, &name, Pt::new(0.0, 0.0), w.max(1.0), h.max(1.0), LAYER, data);
    if let Some(e) = doc.find_mut(id) {
        e.site = Some(SitePlacement {
            x: plan.origin_x_m * 1000.0,
            y: plan.origin_y_m * 1000.0,
            z: 0.0,
            rotation_deg: 0.0,
            alignment: None,
            station_m: None,
            offset_m: None,
        });
    }
    // Not shown on the design sheet: see the module comment.
    if let Some(layer) = doc.layers_mut().iter_mut().find(|l| l.name == LAYER) {
        layer.visible = false;
    }
    Some(id)
}

/// Imports every horizontal alignment of a LandXML file. Returns how many were added and the
/// importer's notes (unsupported elements, skipped alignments).
pub fn import_landxml(doc: &mut Document, path: &Path) -> Result<(usize, Vec<String>), String> {
    let import = afrobim_alignment::read_landxml(path).map_err(|e| e.to_string())?;
    let mut added = 0;
    for a in &import.alignments {
        if add(doc, a).is_some() {
            added += 1;
        }
    }
    Ok((added, import.notes))
}

/// Stands a sign at a station and offset on an alignment and moves its supports with it.
/// `right` chooses the side looking up-station; the offset is entered as a distance from the
/// centreline. Traffic keeps left (the UK and Uganda). Returns a note for the status line.
pub fn place_sign(doc: &mut Document, sign: ElementId, alignment_id: ElementId, station_m: f64, distance_m: f64, right: bool) -> Result<String, String> {
    let alignment = alignment_of(doc, alignment_id).ok_or("That alignment cannot be read.")?;
    let location = SignLocation {
        alignment: Some(alignment.name.clone()),
        station: Some(station_m),
        offset: Some(distance_m.abs()),
        side: Some(if right { SignSide::Right } else { SignSide::Left }),
    };
    let placement = afrobim_alignment::place(&alignment, &location, TrafficSide::Left).map_err(|e| e.to_string())?;
    let position = placement.positions.first().ok_or("The alignment gave no position for that station.")?;
    let site = position.site_placement(&alignment.name);
    let mark = {
        let e = doc.find_mut(sign).ok_or("The sign no longer exists.")?;
        e.site = Some(site);
        e.name.clone()
    };
    let _ = afrobim_supports::sync_support_sites(doc, sign);
    Ok(format!(
        "{mark} placed on {} at {} , {:.2} m {}.",
        alignment.name,
        afrobim_alignment::format_station(station_m, 3),
        distance_m.abs(),
        if right { "right" } else { "left" }
    ))
}
