//! Station fields for the road dialogs.
//!
//! The crates take internal stations (measured along the alignment from its start). The
//! drawings show the displayed station, which runs through the alignment's station equations
//! and can repeat or jump. Every road dialog therefore enters the station shown on the drawings,
//! with the same region picker as "Place by station", and converts it here. On an alignment with
//! no station equations the two are the same and the field is the plain number it always was.

use afrobim_alignment::Alignment;
use afrobim_kernel::{Document, ElementId};
use egui::{Color32, RichText, Ui};

use crate::alignment_glue;

/// The internal station of a displayed station. `region` is the number shown in the picker:
/// 0 when the station occurs once (the alignment refuses an ambiguous one and lists the places),
/// else the stationing region counted from 1. An alignment with no station equations has one
/// region and the two stations are equal.
pub fn displayed_to_internal(alignment: &Alignment, displayed_m: f64, region: usize) -> Result<f64, String> {
    if alignment.station_equations.region_count() <= 1 {
        return Ok(displayed_m);
    }
    alignment.internal_station(displayed_m, region.checked_sub(1)).map_err(|e| e.to_string())
}

/// The displayed station and picker region (0 when there are no equations) of an internal station.
pub fn internal_to_displayed(alignment: &Alignment, internal_m: f64) -> (f64, usize) {
    if alignment.station_equations.region_count() <= 1 {
        return (internal_m, 0);
    }
    match alignment.displayed_station(internal_m) {
        Some(d) => (d.station_m, d.region + 1),
        None => (internal_m, 0),
    }
}

/// What a dialog says about stations, so the engineer knows which one is meant.
pub fn note(doc: &Document, alignment: Option<ElementId>) -> String {
    let regions = alignment.map(|a| alignment_glue::region_count(doc, a)).unwrap_or(1);
    if regions > 1 {
        "Stations are the stations shown on the drawings, read through the alignment's station equations. Where a station occurs in more than one region, pick the region. AfroBIM converts them to internal stations.".to_string()
    } else {
        "Stations are the stations shown on the drawings. This alignment has no station equations, so they equal the internal stations.".to_string()
    }
}

#[derive(Clone, Copy, Default)]
struct FieldState {
    displayed: f64,
    region: usize,
    seen_internal: f64,
    primed: bool,
}

/// One row of a two-column grid: the label and a station field. `internal` is the value the
/// dialog's request holds; it is only changed to a station the alignment accepts.
pub fn station_row(ui: &mut Ui, doc: &Document, alignment: Option<ElementId>, salt: &str, label: &str, internal: &mut f64) {
    ui.label(label);
    let parsed = alignment.and_then(|a| alignment_glue::alignment_of(doc, a));
    match parsed {
        Some(a) if a.station_equations.region_count() > 1 => {
            let regions = a.station_equations.region_count();
            let id = ui.id().with(("station-field", salt));
            let mut st: FieldState = ui.data_mut(|d| d.get_temp(id)).unwrap_or_default();
            if !st.primed || (st.seen_internal - *internal).abs() > 1e-9 {
                let (displayed, region) = internal_to_displayed(&a, *internal);
                st = FieldState { displayed, region, seen_internal: *internal, primed: true };
            }
            let mut problem: Option<String> = None;
            ui.vertical(|ui| {
                ui.horizontal(|ui| {
                    let c1 = ui.add(egui::DragValue::new(&mut st.displayed).speed(1.0).suffix(" m")).changed();
                    ui.label("region");
                    let c2 = ui.add(egui::DragValue::new(&mut st.region).range(0..=regions)).on_hover_text(format!("1 to {regions}; 0 if the station occurs once")).changed();
                    if c1 || c2 {
                        match displayed_to_internal(&a, st.displayed, st.region) {
                            Ok(v) => {
                                *internal = v;
                                st.seen_internal = v;
                            }
                            Err(e) => problem = Some(e),
                        }
                    }
                });
                match &problem {
                    Some(e) => {
                        ui.label(RichText::new(e).small().color(Color32::from_rgb(0xEF, 0x6C, 0x63)));
                    }
                    None => {
                        ui.label(RichText::new(format!("{}, internal station {:.3} m", afrobim_alignment::format_station(st.displayed, 3), *internal)).small().weak());
                    }
                }
            });
            ui.data_mut(|d| d.insert_temp(id, st));
        }
        _ => {
            ui.add(egui::DragValue::new(internal).speed(1.0).suffix(" m"));
        }
    }
    ui.end_row();
}

#[cfg(test)]
mod tests {
    use super::*;
    use afrobim_alignment::StationEquation;

    fn with_equation() -> Alignment {
        let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../afrobim-alignment/tests/landxml/corridor.xml");
        let mut a = afrobim_alignment::read_landxml(&path).expect("test alignment").alignments.remove(0);
        a.station_equations.equations.clear();
        // Test fixture: at internal 500 m the displayed station jumps to 2000 m.
        a.station_equations.equations.push(StationEquation::new(a.start_station_m + 500.0, 2000.0));
        a
    }

    #[test]
    fn without_equations_displayed_is_internal() {
        let mut a = with_equation();
        a.station_equations.equations.clear();
        assert_eq!(displayed_to_internal(&a, 123.0, 0), Ok(123.0));
        assert_eq!(internal_to_displayed(&a, 123.0), (123.0, 0));
    }

    #[test]
    fn displayed_station_converts_to_internal_through_the_equation() {
        let a = with_equation();
        let s0 = a.start_station_m;
        // 2100 m is shown 100 m after the equation, which sits at internal s0 + 500.
        assert!((displayed_to_internal(&a, 2100.0, 0).unwrap() - (s0 + 600.0)).abs() < 1e-6);
        // Region 2 (the one after the equation) says the same; region 1 does not contain it.
        assert!((displayed_to_internal(&a, 2100.0, 2).unwrap() - (s0 + 600.0)).abs() < 1e-6);
        assert!(displayed_to_internal(&a, 2100.0, 1).is_err());
        // Back to displayed: the internal station is read through the equation, region numbered from 1.
        let (shown, region) = internal_to_displayed(&a, s0 + 600.0);
        assert!((shown - 2100.0).abs() < 1e-6);
        assert_eq!(region, 2);
    }
}
