//! The kernel module for lighting elements and the helpers that add them to a document.

use std::collections::BTreeMap;

use afrobim_alignment::Alignment;
use afrobim_kernel::{Document, Element, ElementId, Generated, Module, Pt, Severity, SitePlacement};

use crate::geometry::lighting_geometry;
use crate::layout::{layout_installation, layout_single_column};
use crate::model::{EndTreatment, Installation, LightingData, LightingDefinition, SingleColumn};
use crate::{error_text, fixed, note, LightingError};

/// The element category [`LightingModule`] owns.
pub const CATEGORY: &str = "lighting";
/// The layer lighting elements are added on.
pub const LAYER: &str = "Street lighting";

/// The kernel module for street lighting.
///
/// `generate` draws the stored column positions in plan (see [`crate::LightingGeometry`]) and
/// returns their schematic solids and these derived values:
///
/// | key | value |
/// |---|---|
/// | `kind` | `installation` or `column` |
/// | `alignment` | the alignment the positions were computed on |
/// | `arrangement` | e.g. `staggered`, or `single column` |
/// | `lighting_class` | the entered label |
/// | `columns`, `luminaires` | counts |
/// | `connected_load_w` | sum of the entered luminaire powers, W |
/// | `mounting_height_m`, `outreach_m`, `tilt_deg` | as entered |
/// | `start_station`, `end_station` | displayed stations of the first and last column |
/// | `spacing_m`, `actual_spacing_m`, `bays`, `last_bay_m`, `remainder_m` | installations only |
/// | `setback_m`, `overhang_m` | installations only; overhang is outreach less set-back |
/// | `plan_origin_x_m`, `plan_origin_y_m` | plan coordinates of the drawing origin, m, 6 decimals |
///
/// It never reads the alignment: the positions are those stored by [`add_installation`],
/// [`add_column`] or [`refresh_layout`].
#[derive(Debug, Clone, Copy, Default)]
pub struct LightingModule;

impl Module for LightingModule {
    fn category(&self) -> &str {
        CATEGORY
    }

    fn display_name(&self) -> &str {
        "Street lighting"
    }

    fn generate(&self, element: &Element) -> Result<Generated, String> {
        let data = LightingData::from_value(&element.data).map_err(|e| format!("'{}': {e}", element.name))?;
        let mut notes = data.definition.notes();
        if let Some(errors) = error_text(&notes) {
            return Err(format!("'{}' cannot be drawn: {errors}", element.name));
        }
        let spec = data.definition.column();
        let geometry =
            lighting_geometry(&data.layout.columns, spec).ok_or_else(|| format!("'{}' has no column positions; lay it out on its alignment.", element.name))?;

        let mut derived = BTreeMap::new();
        let mut put = |k: &str, v: String| {
            derived.insert(k.to_string(), v);
        };
        put("alignment", data.layout.alignment.clone());
        put("arrangement", data.definition.arrangement_label().to_string());
        put("lighting_class", data.definition.lighting_class().to_string());
        put("columns", data.layout.columns.len().to_string());
        put("luminaires", data.luminaire_count().to_string());
        put("connected_load_w", fixed(data.connected_load_w(), 0, 1));
        put("mounting_height_m", fixed(spec.mounting_height_m, 0, 3));
        put("outreach_m", fixed(spec.bracket_outreach_m, 0, 3));
        put("tilt_deg", fixed(spec.tilt_deg, 0, 2));
        if let (Some(first), Some(last)) = (data.layout.columns.first(), data.layout.columns.last()) {
            put("start_station", first.station_label.clone());
            put("end_station", last.station_label.clone());
        }
        put("plan_origin_x_m", format!("{:.6}", geometry.origin_x_m));
        put("plan_origin_y_m", format!("{:.6}", geometry.origin_y_m));
        match &data.definition {
            LightingDefinition::Installation(i) => {
                put("kind", "installation".to_string());
                put("spacing_m", fixed(i.spacing_m, 0, 3));
                put("actual_spacing_m", fixed(data.layout.actual_spacing_m, 0, 3));
                put("bays", data.layout.bays.to_string());
                put("last_bay_m", fixed(data.layout.last_bay_m, 0, 3));
                put("remainder_m", fixed(data.layout.remainder_m, 0, 3));
                put("setback_m", fixed(i.setback_m, 0, 3));
                put("overhang_m", fixed(i.overhang_m(), 0, 3));
                if data.layout.remainder_m > 0.0 {
                    notes.push(note(
                        Severity::Warning,
                        "LIGHTING_REMAINDER",
                        format!(
                            "The last {} m before the end station has no column: the length is not a whole number of spacings. Choose another end treatment if a column is wanted at the end.",
                            fixed(data.layout.remainder_m, 0, 3)
                        ),
                    ));
                }
                if i.end_treatment == EndTreatment::ColumnAtEnd && data.layout.last_bay_m < i.spacing_m {
                    notes.push(note(
                        Severity::Info,
                        "LIGHTING_SHORT_LAST_BAY",
                        format!("The last bay is {} m, shorter than the spacing of {} m.", fixed(data.layout.last_bay_m, 0, 3), fixed(i.spacing_m, 0, 3)),
                    ));
                }
            }
            LightingDefinition::Column(_) => put("kind", "column".to_string()),
        }
        if data.layout.columns.iter().any(|c| c.level_m.is_none()) {
            notes.push(note(
                Severity::Info,
                "LIGHTING_NO_LEVEL",
                "The alignment has no profile at one or more columns: those have no level and stand at zero in the 3D view.".to_string(),
            ));
        }
        notes.push(note(
            Severity::Info,
            "LIGHTING_SCHEMATIC",
            "Schematic: the column, bracket and luminaire are display shapes at the entered mounting height and outreach, not a product model. No photometric, structural or electrical design has been done.".to_string(),
        ));
        notes.push(note(
            Severity::Info,
            "LIGHTING_STORED_LAYOUT",
            format!("The positions are those last computed on alignment '{}'; refresh the layout after the alignment changes.", data.layout.alignment),
        ));
        Ok(Generated { drawing: geometry.drawing, notes, derived, solids: geometry.solids })
    }
}

fn check_alignment(alignment: &Alignment, definition: &LightingDefinition) -> Result<(), LightingError> {
    if definition.alignment() != alignment.name {
        return Err(LightingError::InvalidDefinition(format!(
            "the definition names alignment '{}' but was given '{}'",
            definition.alignment(),
            alignment.name
        )));
    }
    Ok(())
}

fn store(doc: &mut Document, id: Option<ElementId>, name: &str, data: &LightingData) -> Result<ElementId, LightingError> {
    let geometry = lighting_geometry(&data.layout.columns, data.definition.column())
        .ok_or_else(|| LightingError::InvalidDefinition("the layout has no columns".to_string()))?;
    let (w, h) = (geometry.drawing.extents.width().max(1.0), geometry.drawing.extents.height().max(1.0));
    let id = match id {
        Some(id) => id,
        None => {
            let name = doc.unique_name(if name.trim().is_empty() { "Lighting" } else { name.trim() });
            doc.add(CATEGORY, &name, Pt::new(0.0, 0.0), w, h, LAYER, serde_json::Value::Null)
        }
    };
    let first = data.layout.columns.first();
    let e = doc.find_mut(id).ok_or(LightingError::ElementNotFound(id.0))?;
    e.width = w;
    e.height = h;
    e.data = data.to_value();
    // Like an alignment, the element lives in site coordinates: its site placement records
    // where the drawing origin is. Solids carry their own levels, so z is zero.
    e.site = Some(SitePlacement {
        x: geometry.origin_x_m * 1000.0,
        y: geometry.origin_y_m * 1000.0,
        z: 0.0,
        rotation_deg: 0.0,
        alignment: Some(data.layout.alignment.clone()),
        station_m: first.map(|c| c.station_m),
        offset_m: first.map(|c| c.offset_m),
    });
    Ok(id)
}

/// Lays an installation out on `alignment` and adds it to the document as one element on
/// [`LAYER`], named by its reference (made unique). The element's site placement is the origin
/// of its plan drawing, as for an alignment element.
pub fn add_installation(doc: &mut Document, alignment: &Alignment, def: Installation) -> Result<ElementId, LightingError> {
    let definition = LightingDefinition::Installation(def.clone());
    check_alignment(alignment, &definition)?;
    let layout = layout_installation(alignment, &def)?;
    store(doc, None, &def.reference, &LightingData { definition, layout })
}

/// Positions a single column on `alignment` and adds it to the document, named by its mark.
pub fn add_column(doc: &mut Document, alignment: &Alignment, def: SingleColumn) -> Result<ElementId, LightingError> {
    let definition = LightingDefinition::Column(def.clone());
    check_alignment(alignment, &definition)?;
    let layout = layout_single_column(alignment, &def)?;
    store(doc, None, &def.mark, &LightingData { definition, layout })
}

/// The definition and stored positions of a lighting element.
pub fn lighting_of(doc: &Document, id: ElementId) -> Result<LightingData, LightingError> {
    let e = doc.find(id).ok_or(LightingError::ElementNotFound(id.0))?;
    if e.category != CATEGORY {
        return Err(LightingError::NotLighting(id.0));
    }
    LightingData::from_value(&e.data)
}

/// Recomputes the positions of a lighting element from `alignment`: call it after the alignment
/// or the element's definition has changed. To change the definition, pass the new one in
/// `definition`; `None` keeps the stored one.
pub fn refresh_layout(doc: &mut Document, id: ElementId, alignment: &Alignment, definition: Option<LightingDefinition>) -> Result<(), LightingError> {
    let definition = match definition {
        Some(d) => d,
        None => lighting_of(doc, id)?.definition,
    };
    if doc.find(id).ok_or(LightingError::ElementNotFound(id.0))?.category != CATEGORY {
        return Err(LightingError::NotLighting(id.0));
    }
    check_alignment(alignment, &definition)?;
    let layout = match &definition {
        LightingDefinition::Installation(i) => layout_installation(alignment, i)?,
        LightingDefinition::Column(c) => layout_single_column(alignment, c)?,
    };
    store(doc, Some(id), "", &LightingData { definition, layout }).map(|_| ())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::layout::tests::{installation, spec, straight};
    use crate::model::{Arrangement, BracketAim};
    use afrobim_alignment::{Profile, Pvi};
    use afrobim_kernel::ModuleRegistry;

    #[test]
    fn an_installation_becomes_an_element_that_draws_and_round_trips() {
        let a = straight();
        let mut doc = Document::default();
        let id = add_installation(&mut doc, &a, installation(Arrangement::Staggered, 0.0, 300.0, 30.0)).unwrap();
        let e = doc.find(id).unwrap().clone();
        assert_eq!((e.category.as_str(), e.name.as_str(), e.layer.as_str()), (CATEGORY, "Z1", LAYER));
        let site = e.site.clone().unwrap();
        assert_eq!(site.alignment.as_deref(), Some("A"));
        assert_eq!((site.station_m, site.offset_m), (Some(0.0), Some(-5.0)));

        let mut modules = ModuleRegistry::default();
        modules.register(Box::new(LightingModule));
        let g = modules.generate(&e).unwrap();
        assert_eq!(g.derived["columns"], "11");
        assert_eq!(g.derived["luminaires"], "11");
        assert_eq!(g.derived["connected_load_w"], "1100");
        assert_eq!(g.derived["arrangement"], "staggered");
        assert_eq!(g.derived["kind"], "installation");
        assert_eq!(g.derived["end_station"], "0+300.000");
        assert_eq!(g.derived["overhang_m"], "0");
        assert_eq!(g.solids.len(), 33);
        // The site placement is the drawing origin.
        assert!((site.x - g.derived["plan_origin_x_m"].parse::<f64>().unwrap() * 1000.0).abs() < 1e-3);
        assert!((e.width - g.drawing.extents.width()).abs() < 1e-9);
        assert!(g.notes.iter().any(|n| n.code == "LIGHTING_SCHEMATIC"));
        assert!(g.notes.iter().any(|n| n.code == "LIGHTING_NO_LEVEL"));
        assert!(!g.notes.iter().any(|n| n.severity == Severity::Error));

        // The document and the definition survive a save and load unchanged.
        let again = Document::from_json(&doc.to_json()).unwrap();
        assert_eq!(lighting_of(&again, id).unwrap(), lighting_of(&doc, id).unwrap());
        let data = lighting_of(&doc, id).unwrap();
        assert_eq!(LightingData::from_value(&serde_json::from_str(&serde_json::to_string(&data).unwrap()).unwrap()).unwrap(), data);
    }

    #[test]
    fn refreshing_follows_the_alignment_and_the_definition() {
        let mut a = straight();
        let mut doc = Document::default();
        let mut def = installation(Arrangement::SingleRight, 0.0, 250.0, 40.0);
        let id = add_installation(&mut doc, &a, def.clone()).unwrap();
        let g = LightingModule.generate(doc.find(id).unwrap()).unwrap();
        assert_eq!(g.derived["remainder_m"], "10");
        assert!(g.notes.iter().any(|n| n.code == "LIGHTING_REMAINDER"));

        a.profile = Some(Profile::new("P", vec![Pvi::point(0.0, 50.0), Pvi::point(300.0, 50.0)]).unwrap());
        def.end_treatment = EndTreatment::ColumnAtEnd;
        refresh_layout(&mut doc, id, &a, Some(LightingDefinition::Installation(def))).unwrap();
        let data = lighting_of(&doc, id).unwrap();
        assert_eq!(data.layout.columns.len(), 8);
        assert_eq!(data.layout.columns[7].level_m, Some(50.0));
        let g = LightingModule.generate(doc.find(id).unwrap()).unwrap();
        assert!(g.notes.iter().any(|n| n.code == "LIGHTING_SHORT_LAST_BAY"));
        assert!(!g.notes.iter().any(|n| n.code == "LIGHTING_NO_LEVEL"));
        // Solids stand at the profile level.
        assert_eq!(g.solids[0].vertices.iter().map(|v| v[2]).fold(f64::INFINITY, f64::min), 50_000.0);

        let other = afrobim_alignment::AlignmentBuilder::new("B", 0.0, 0.0, 0.0, 0.0).line(500.0).build().unwrap();
        assert!(matches!(refresh_layout(&mut doc, id, &other, None), Err(LightingError::InvalidDefinition(_))));
        assert!(matches!(refresh_layout(&mut doc, ElementId(999), &a, None), Err(LightingError::ElementNotFound(999))));
    }

    #[test]
    fn single_columns_and_bad_elements() {
        let a = straight();
        let mut doc = Document::default();
        let def = SingleColumn {
            mark: "J1".to_string(),
            alignment: "A".to_string(),
            station_m: 150.0,
            offset_m: 9.0,
            aim: BracketAim::TowardAlignment,
            column: spec(),
            lighting_class: "C2".to_string(),
        };
        let id = add_column(&mut doc, &a, def.clone()).unwrap();
        // The same mark again is made unique by the document.
        let second = add_column(&mut doc, &a, def).unwrap();
        assert_ne!(doc.find(id).unwrap().name, doc.find(second).unwrap().name);
        let g = LightingModule.generate(doc.find(id).unwrap()).unwrap();
        assert_eq!((g.derived["kind"].as_str(), g.derived["columns"].as_str(), g.derived["arrangement"].as_str()), ("column", "1", "single column"));

        let other = doc.add("sign", "S", Pt::new(0.0, 0.0), 1.0, 1.0, "0", serde_json::Value::Null);
        assert!(matches!(lighting_of(&doc, other), Err(LightingError::NotLighting(_))));
        let mut bad = doc.find(id).unwrap().clone();
        bad.data = serde_json::json!({"nonsense": true});
        assert!(LightingModule.generate(&bad).is_err());
        // A definition edited into an invalid state is refused with the reason.
        let mut data = lighting_of(&doc, id).unwrap();
        if let LightingDefinition::Column(c) = &mut data.definition {
            c.column.mounting_height_m = 0.0;
        }
        bad.data = data.to_value();
        assert!(LightingModule.generate(&bad).unwrap_err().contains("mounting height"));
    }
}
