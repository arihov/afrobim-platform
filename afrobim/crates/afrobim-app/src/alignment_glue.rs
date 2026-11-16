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
    // The element sits at the plan point its drawing starts from, so that the element origin plus a
    // drawing coordinate is the source easting and northing times 1000 and nothing else: no shift,
    // no re-basing. (The drawing itself is local to that point, which keeps its numbers small.)
    let at = plan_origin_mm(&plan);
    let id = doc.add(CATEGORY, &name, at, w.max(1.0), h.max(1.0), LAYER, data);
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

/// The model position (mm) of the plan drawing's origin: the plan origin in metres times 1000.
pub fn plan_origin_mm(plan: &afrobim_alignment::PlanDrawing) -> Pt {
    afrobim_kernel::model_point_from_crs(plan.origin_x_m, plan.origin_y_m)
}

/// Sets the origin of an imported element that has a site placement (an alignment, a surface) to
/// that placement, which is the plan position of its drawing's origin in millimetres. The element
/// then sits at the coordinates of its source file. Does nothing for an element with no placement.
pub fn place_at_source(doc: &mut Document, id: ElementId) {
    if let Some(e) = doc.find_mut(id) {
        if let Some(s) = &e.site {
            e.origin = Pt::new(s.x, s.y);
        }
    }
}

/// Adds a ground surface at the coordinates of its source: the element sits at the plan origin of
/// its drawing (the least easting and northing of its points), so the element origin plus a point
/// of the drawing is the source easting and northing times 1000 in model millimetres.
pub fn add_surface_at_source(doc: &mut Document, name: &str, tin: &afrobim_corridor::Tin, source: Option<&str>) -> Result<ElementId, afrobim_corridor::CorridorError> {
    let id = afrobim_corridor::add_surface(doc, name, tin, source)?;
    place_at_source(doc, id);
    Ok(id)
}

/// The model position (mm) of the alignment's point at `station_m`, as the element carries it:
/// the element origin plus the plan drawing of the stored alignment is not used here, this is the
/// alignment's own geometry times 1000. None when the station is outside the alignment.
#[cfg(test)]
pub fn model_point_at(doc: &Document, id: ElementId, station_m: f64) -> Option<Pt> {
    let a = alignment_of(doc, id)?;
    let p = a.point_at(station_m)?;
    Some(afrobim_kernel::model_point_from_crs(p.x, p.y))
}

/// Imports every alignment of a LandXML file, with its profile, superelevation and station
/// equations where the file has them. Returns how many were added and the
/// importer's notes (unsupported elements, skipped alignments).
pub fn import_landxml(doc: &mut Document, path: &Path) -> Result<(usize, Vec<String>), String> {
    let import = afrobim_alignment::read_landxml(path).map_err(|e| e.to_string())?;
    let mut added = 0;
    for a in &import.alignments {
        if add(doc, a).is_some() {
            added += 1;
        }
    }
    let mut notes = import.notes;
    // Without a dialog to ask in: a project with no coordinate system takes the one the file
    // declares (the dialog offers the same, ticked). A project that has one keeps it; the
    // objects are at source coordinates either way and nothing is reprojected.
    let declared = afrobim_alignment::read_landxml_file(path).ok().and_then(|f| f.coordinate_system).and_then(|c| crate::crs_ui::from_landxml(&c));
    if let Some(crs) = declared {
        match afrobim_kernel::project_crs(doc) {
            None => {
                notes.push(format!("The project had no coordinate system and now uses the file's: {}.", crs.label()));
                afrobim_kernel::set_project_crs(doc, &crs);
            }
            Some(p) => {
                let check = crate::crs_ui::dataset_check(Some(&p), &path.file_name().map(|n| n.to_string_lossy().to_string()).unwrap_or_default(), Some(&crs), None);
                notes.extend(check.into_iter().filter(|f| f.warning).map(|f| f.message));
            }
        }
    }
    Ok((added, notes))
}

/// Stands a sign at a station and offset on an alignment and moves its supports with it.
/// `right` chooses the side looking up-station; the offset is entered as a distance from the
/// centreline. Traffic keeps left (the UK and Uganda). Returns a note for the status line.
/// The number of stationing regions of an alignment: 1 without station equations.
pub fn region_count(doc: &Document, alignment_id: ElementId) -> usize {
    alignment_of(doc, alignment_id).map(|a| a.station_equations.region_count()).unwrap_or(1)
}

/// `station_m` is the station shown on the drawings. `region` says which stationing region it is
/// in (0 before the first station equation); None accepts only a station that occurs once.
pub fn place_sign(doc: &mut Document, sign: ElementId, alignment_id: ElementId, station_m: f64, region: Option<usize>, distance_m: f64, right: bool) -> Result<String, String> {
    let alignment = alignment_of(doc, alignment_id).ok_or("That alignment cannot be read.")?;
    let location = SignLocation {
        alignment: Some(alignment.name.clone()),
        station: Some(station_m),
        offset: Some(distance_m.abs()),
        side: Some(if right { SignSide::Right } else { SignSide::Left }),
    };
    // The station is the one shown on the drawings: read through the station equations, if any.
    let options = afrobim_alignment::PlaceOptions { station: afrobim_alignment::StationReference::Displayed { region }, ..Default::default() };
    let placement = afrobim_alignment::place_with(&alignment, &location, TrafficSide::Left, &options).map_err(|e| e.to_string())?;
    let position = placement.positions.first().ok_or("The alignment gave no position for that station.")?;
    let site = position.site_placement(&alignment.name);
    let mark = {
        let e = doc.find_mut(sign).ok_or("The sign no longer exists.")?;
        e.site = Some(site);
        e.name.clone()
    };
    let _ = afrobim_supports::sync_support_sites(doc, sign);
    let level = if alignment.profile.is_some() { format!(", centreline level {:.3} m", position.z_mm / 1000.0) } else { String::new() };
    Ok(format!(
        "{mark} placed on {} at {} , {:.2} m {}{level}.",
        alignment.name,
        crate::anno_ui::units_of(doc).station(station_m),
        distance_m.abs(),
        if right { "right" } else { "left" }
    ))
}
