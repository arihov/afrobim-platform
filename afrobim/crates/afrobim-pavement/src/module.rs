//! The pavement as a kernel element: category `"pavement"`, `data` = the serialised
//! [`PavementDefinition`], and the helpers that put one in a document.

use std::collections::BTreeMap;

use afrobim_alignment::Alignment;
use afrobim_kernel::{Document, Element, ElementId, Generated, Module, Severity, SitePlacement};

use crate::geometry::{cross_section, solids, SectionOptions};
use crate::model::PavementDefinition;
use crate::quantities::{fixed, measure_definition};
use crate::PavementError;

/// The element category this crate owns.
pub const PAVEMENT_CATEGORY: &str = "pavement";

/// The layer pavements are put on.
pub const PAVEMENT_LAYER: &str = "Pavement";

/// The kernel module for pavements.
///
/// `generate` returns the typical cross-section ([`crate::cross_section`]) as the drawing, the
/// slabs ([`crate::solids`]) as the solids, the definition's notes, and these derived values:
///
/// | key | value |
/// |---|---|
/// | `alignment` | name of the alignment followed |
/// | `start_station_m`, `end_station_m`, `length_m` | m, 3 decimals |
/// | `heading_change_deg` | change of heading over the length, degrees, left positive, 4 decimals |
/// | `overall_width_m` | carriageway plus shoulders, m, 3 decimals |
/// | `carriageway_thickness_mm` | sum of the carriageway layers |
/// | `carriageway_area_m2` | plan area of the carriageway strip, 3 decimals |
/// | `layers_volume_m3` | compacted volume of all layers of all strips, 3 decimals |
/// | `solids_origin_x_m`, `solids_origin_y_m` | plan coordinates of the solids' origin (see [`crate::PavementSolids`]), m, 6 decimals |
/// | `structure_column_scale` | drawing units per mm of thickness in the structure columns |
///
/// A definition with Error notes gives an empty drawing and no solids, with the notes.
#[derive(Debug, Clone, Copy, Default)]
pub struct PavementModule {
    pub section: SectionOptions,
}

impl Module for PavementModule {
    fn category(&self) -> &str {
        PAVEMENT_CATEGORY
    }

    fn display_name(&self) -> &str {
        "Pavements"
    }

    fn generate(&self, element: &Element) -> Result<Generated, String> {
        let def = PavementDefinition::from_value(&element.data).map_err(|e| format!("'{}': {e}", element.name))?;
        let notes = def.notes();
        if notes.iter().any(|n| n.severity == Severity::Error) {
            return Ok(Generated { notes, ..Generated::default() });
        }
        let section = cross_section(&def, &self.section);
        let slabs = solids(&def).ok_or_else(|| format!("'{}': a station is off the alignment.", element.name))?;
        let rows = measure_definition(&element.name, &def).map_err(|e| format!("'{}': {e}", element.name))?;
        let strips = def.strips();

        let mut derived = BTreeMap::new();
        let mut put = |k: &str, v: String| {
            derived.insert(k.to_string(), v);
        };
        put("alignment", def.alignment.name.clone());
        put("start_station_m", format!("{:.3}", def.start_station_m));
        put("end_station_m", format!("{:.3}", def.end_station_m));
        put("length_m", format!("{:.3}", def.length_m()));
        put("heading_change_deg", format!("{:.4}", rows.first().map_or(0.0, |r| r.heading_change_rad).to_degrees()));
        put("overall_width_m", format!("{:.3}", strips[strips.len() - 1].right_offset_m() - strips[0].left_offset_m()));
        put("carriageway_thickness_mm", fixed(def.layers.iter().map(|l| l.thickness_mm).sum(), 0, 1));
        put("carriageway_area_m2", format!("{:.3}", rows.iter().find(|r| r.strip == "Carriageway").map_or(0.0, |r| r.area_m2)));
        put("layers_volume_m3", format!("{:.3}", rows.iter().map(|r| r.volume_m3).sum::<f64>()));
        put("solids_origin_x_m", format!("{:.6}", slabs.origin_x_m));
        put("solids_origin_y_m", format!("{:.6}", slabs.origin_y_m));
        put("structure_column_scale", fixed(section.column_scale, 0, 3));

        Ok(Generated { drawing: section.drawing, notes, derived, solids: slabs.solids })
    }
}

/// Adds a pavement to a document and returns its id.
///
/// The element goes on [`PAVEMENT_LAYER`] at the next free place on the sheet, sized to its
/// cross-section drawing. Its site placement is the origin of its solids (see
/// [`crate::PavementSolids`]): x, y in millimetres, z = 0, no rotation (the slabs carry the
/// profile level themselves), with the alignment's name and the start station recorded. Fails when the definition has Error notes.
pub fn add_pavement(doc: &mut Document, name: &str, def: &PavementDefinition) -> Result<ElementId, PavementError> {
    def.check()?;
    let section = cross_section(def, &SectionOptions::default());
    let (w, h) = (section.drawing.extents.width().max(1.0), section.drawing.extents.height().max(1.0));
    let name = doc.unique_name(if name.trim().is_empty() { "Pavement" } else { name.trim() });
    let origin = doc.next_free_origin(w, h);
    let id = doc.add(PAVEMENT_CATEGORY, &name, origin, w, h, PAVEMENT_LAYER, def.to_value());
    place_on_site(doc, id, def);
    Ok(id)
}

fn place_on_site(doc: &mut Document, id: ElementId, def: &PavementDefinition) {
    let Some(slabs) = solids(def) else { return };
    if let Some(e) = doc.find_mut(id) {
        e.site = Some(SitePlacement {
            x: slabs.origin_x_m * 1000.0,
            y: slabs.origin_y_m * 1000.0,
            z: 0.0,
            rotation_deg: 0.0,
            alignment: Some(def.alignment.name.clone()),
            station_m: Some(def.start_station_m),
            offset_m: Some(def.centre_offset_m),
        });
    }
}

/// Replaces the definition of an existing pavement, resizing the element to its new
/// cross-section and moving its site placement. Fails when the element is not a pavement or the
/// definition has Error notes.
pub fn update_pavement(doc: &mut Document, id: ElementId, def: &PavementDefinition) -> Result<(), PavementError> {
    def.check()?;
    let section = cross_section(def, &SectionOptions::default());
    let e = doc.find_mut(id).filter(|e| e.category == PAVEMENT_CATEGORY).ok_or(PavementError::ElementNotFound(id.0))?;
    e.data = def.to_value();
    e.width = section.drawing.extents.width().max(1.0);
    e.height = section.drawing.extents.height().max(1.0);
    place_on_site(doc, id, def);
    Ok(())
}

/// The pavements in the document: id, name and definition. Unreadable ones are left out.
pub fn pavements(doc: &Document) -> Vec<(ElementId, String, PavementDefinition)> {
    doc.elements()
        .iter()
        .filter(|e| e.category == PAVEMENT_CATEGORY)
        .filter_map(|e| PavementDefinition::from_value(&e.data).ok().map(|d| (e.id, e.name.clone(), d)))
        .collect()
}

/// Refreshes the alignment copy held by each pavement from the alignment element of the same
/// name in the document (category `afrobim_alignment::CATEGORY`). Returns the pavements that
/// changed. A pavement whose stations no longer fit the new alignment is still updated; its
/// module then reports the Error note.
pub fn sync_alignments(doc: &mut Document) -> Vec<ElementId> {
    let alignments: Vec<Alignment> = doc
        .elements()
        .iter()
        .filter(|e| e.category == afrobim_alignment::CATEGORY)
        .filter_map(|e| serde_json::from_value::<Alignment>(e.data.clone()).ok())
        .collect();
    let mut changed = Vec::new();
    for (id, _, mut def) in pavements(doc) {
        let Some(current) = alignments.iter().find(|a| a.name == def.alignment.name) else { continue };
        if *current == def.alignment {
            continue;
        }
        def.alignment = current.clone();
        if let Some(e) = doc.find_mut(id) {
            e.data = def.to_value();
        }
        place_on_site(doc, id, &def);
        changed.push(id);
    }
    changed
}
