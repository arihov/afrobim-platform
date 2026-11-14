//! Calculation of horizontal (and, derived here, vertical and semi-cylindrical) illuminance on
//! user-defined grids from the luminaires of a lighting element, and of luminance when the user
//! supplies an r-table. See [`engine`] for the formulae, their derivation and the limits.
//!
//! What is deliberately **not** here, because the source is not held or retrievable:
//! the EN 13201-3 calculation grid (number and position of points) and observer positions,
//! threshold increment (TI) and its veiling luminance formula, longitudinal luminance uniformity,
//! hemispherical illuminance, and any r-table values.
//!
//! # Example
//!
//! ```
//! use afrobim_alignment::AlignmentBuilder;
//! use afrobim_kernel::Document;
//! use afrobim_lighting::calc::*;
//! use afrobim_lighting::design::{check_lighting, CheckInputs, PhotometricResults};
//! use afrobim_lighting::*;
//!
//! // An installation: 7 m carriageway, one row on the right, 10 m high, 30 m apart.
//! let alignment = AlignmentBuilder::new("A", 0.0, 1000.0, 2000.0, 0.0).line(300.0).build().unwrap();
//! let installation = Installation {
//!     reference: "Z1".into(), alignment: "A".into(), start_station_m: 0.0, end_station_m: 120.0,
//!     arrangement: Arrangement::SingleRight, spacing_m: 30.0, end_treatment: EndTreatment::KeepSpacing,
//!     first_side: Side::Left, carriageway_width_m: 7.0, reference_offset_m: 0.0, setback_m: 1.5,
//!     column: ColumnSpec { mounting_height_m: 10.0, bracket_outreach_m: 1.5, ..Default::default() },
//!     lighting_class: "C2".into(), mark_prefix: "LC".into(), first_number: 1,
//! };
//! let mut doc = Document::default();
//! let id = add_installation(&mut doc, &alignment, installation).unwrap();
//! let data = lighting_of(&doc, id).unwrap();
//!
//! // The luminaire file. Normally: `load_photometry_file(Path::new("lantern.ies"))`.
//! let ies = "IESNA:LM-63-2002\n[TEST] x\nTILT=NONE\n1 1000 1 3 1 1 2 0.2 0.4 0.1\n1 1 50\n0 45 90\n0\n9000 6000 0\n";
//! let loaded = parse_photometry_bytes("lantern.ies", ies.as_bytes()).unwrap();
//!
//! // Everything the engineer enters: maintenance factor, contribution distance, the grid.
//! let settings = CalcSettings {
//!     maintenance_factor: 0.8,
//!     contribution_distance_m: 100.0,
//!     orientation: Orientation::default(),
//!     areas: vec![default_carriageway_area(&data, Division::Spacing(3.0), Division::Spacing(1.0)).unwrap()],
//!     plane_level_m: None,
//!     vertical: None,
//!     observers: vec![],
//!     eir_lane_width_m: None,
//! };
//!
//! // Keep what is needed to repeat the run on the element, then run it.
//! set_photometry(&mut doc, id, "lantern.ies", &loaded, &settings, None).unwrap();
//! let run = run_photometry(&data, &loaded.photometry, &loaded.file_name, &loaded.sha256, &settings, None, Some(&alignment)).unwrap();
//!
//! // Fill the results and run the class check.
//! let area = run.primary_area_for_class(&data.definition.lighting_class()).unwrap().to_string();
//! let mut results = PhotometricResults::default();
//! run.apply_to(&mut results, &area).unwrap();
//! assert!(results.source.starts_with(COMPUTED_SOURCE_PREFIX));
//! let checks = check_lighting(&data, &CheckInputs { photometry: results, ..Default::default() });
//! assert!(checks.lines.iter().any(|l| l.finding.contains("lantern.ies")));
//!
//! // Outputs: contours, grid of values, polar diagram (drawings plus labels), CSV.
//! let lums = luminaire_instances(&data);
//! let levels = nice_levels(run.result.areas[0].e_h.minimum, run.result.areas[0].e_h.maximum, 5);
//! let _contours = contour_drawing(&run.result, &area, &levels, &lums).unwrap();
//! let _values = grid_values_drawing(&run.result, &area, &lums).unwrap();
//! let _polar = polar_diagram(&loaded.photometry, &[(0.0, 180.0), (90.0, 270.0)]);
//! assert!(grid_to_csv(&run.result).starts_with("area,kind,"));
//! ```

mod engine;
mod frame;
mod grid;
mod outputs;
mod rtable;
mod run;

pub use engine::{
    calculate, AreaResult, CalcResult, CalcSettings, DirectionResult, EirResult, Facing, LuminanceResult, ObserverPosition, ObserverSpec, Stats, VerticalSettings,
};
pub use frame::{luminaire_instances, photometric_angles, LuminaireInstance, Orientation};
pub use grid::{car_park_grid_limits, AreaGrid, AreaKind, CalcArea, Division, GridFrame, GridPoint, STRAIGHT_FRAME_HEADING_WARNING_DEG};
pub use outputs::{contour_drawing, grid_to_csv, grid_values_drawing, nice_levels, polar_diagram, DrawingLabel, LabelledDrawing};
pub use rtable::RTable;
pub use run::{
    clear_photometry, photometry_settings_of, resolve_photometry, run_photometry, run_stored_photometry, set_photometry, PhotometryRun, PhotometrySettings,
    COMPUTED_SOURCE_PREFIX, MAX_EMBEDDED_VALUES, PHOTOMETRY_KEY,
};

use crate::{Arrangement, LightingData, LightingDefinition};

/// Why a calculation could not be run.
#[derive(Debug, Clone, PartialEq, thiserror::Error)]
pub enum CalcError {
    #[error("invalid calculation settings: {0}")]
    InvalidSettings(String),
    #[error("r-table: {0}")]
    RTable(String),
    #[error("photometry: {0}")]
    Photometry(String),
}

/// The carriageway of an installation as a calculation area: from the first to the last column
/// station, across the carriageway width about the reference line. A convenient start for the
/// user's grid; it is not a rule of a standard. Errors for a single column and for the twin
/// central arrangement (whose carriageway positions are not given by the installation): enter
/// those areas by hand.
pub fn default_carriageway_area(data: &LightingData, along: Division, across: Division) -> Result<CalcArea, CalcError> {
    match &data.definition {
        LightingDefinition::Installation(i) if i.arrangement != Arrangement::TwinCentral => {
            let (first, last) = match (data.layout.columns.first(), data.layout.columns.last()) {
                (Some(a), Some(b)) => (a.station_m, b.station_m),
                _ => return Err(CalcError::InvalidSettings("the element has no columns".to_string())),
            };
            if last <= first {
                return Err(CalcError::InvalidSettings("the installation has a single column station; enter the area by hand".to_string()));
            }
            Ok(CalcArea {
                name: "Carriageway".to_string(),
                kind: AreaKind::Carriageway,
                station_from_m: first,
                station_to_m: last,
                offset_from_m: i.reference_offset_m - i.carriageway_width_m / 2.0,
                offset_to_m: i.reference_offset_m + i.carriageway_width_m / 2.0,
                along,
                across,
            })
        }
        _ => Err(CalcError::InvalidSettings("a carriageway area cannot be derived for this element; enter the areas by hand".to_string())),
    }
}

#[cfg(test)]
mod crosscheck;
#[cfg(test)]
mod tests;
