//! The document's display units applied to the design tools and the sheet windows.
//!
//! The design tools and the sheet sources hold metres (a station, a level, a radius, an
//! interval). The document's [`DisplayUnits`] (millimetres or metres, with their decimals, and the
//! station decimals) say how they are shown and typed. These helpers are the one place that
//! converts: every number a window shows goes through a formatter here and every number typed goes
//! through its parser, so what is shown can be typed back. Stored values stay metres.
//!
//! * A length is shown as a number in the display unit; a bare number typed is in the display
//!   unit and a trailing `m` or `mm` overrides it (`DisplayUnits::parse_length`).
//! * A station is shown as `k+mmm` with the station decimals and typed as `k+mmm` or as a plain
//!   number of metres (`DisplayUnits::parse_station`).
//! * K (metres of vertical curve per 1 % change of grade) is a length per percent and follows the
//!   length unit. Grades are percentages and angles are degrees: neither follows the length unit.
//! * A scale (1:n), a vertical exaggeration and a count are pure numbers and stay as typed.

use afrobim_annotate::DisplayUnits;

/// Metres as a number in the display unit, with the unit's decimals (no suffix).
pub fn len(u: &DisplayUnits, metres: f64) -> String {
    u.length(metres * 1000.0)
}

/// As [`len`] with the unit: `12.500 m`.
pub fn len_u(u: &DisplayUnits, metres: f64) -> String {
    u.length_u(metres * 1000.0)
}

/// The unit suffix: `m` or `mm`.
pub fn suffix(u: &DisplayUnits) -> &'static str {
    u.length.suffix()
}

/// A column or field label with the unit: `East (m)`.
pub fn head(u: &DisplayUnits, what: &str) -> String {
    format!("{what} ({})", suffix(u))
}

/// A typed length into metres. `None` for text that is not a length.
pub fn parse_len(u: &DisplayUnits, text: &str) -> Option<f64> {
    u.parse_length(&text.trim().replace(',', ".")).ok().map(|mm| mm / 1000.0).filter(|v| v.is_finite())
}

/// A typed length into metres, or the reason it was refused.
pub fn parse_len_or(u: &DisplayUnits, text: &str, what: &str) -> Result<f64, String> {
    let t = text.trim();
    if t.is_empty() {
        return Err(format!("Enter {what}."));
    }
    parse_len(u, t).ok_or_else(|| format!("'{t}' is not a length ({what}). Examples: 12.5, 12.5m, 12500mm."))
}

/// True when the two lengths (metres) read the same in the display unit. An edit whose typed
/// value reads the same as the stored one changes nothing, so retyping a rounded display does not
/// move a point by the rounding.
pub fn same_len(u: &DisplayUnits, a_m: f64, b_m: f64) -> bool {
    len(u, a_m) == len(u, b_m)
}

/// A station (metres) as `k+mmm`.
pub fn station(u: &DisplayUnits, metres: f64) -> String {
    u.station(metres)
}

/// A typed station into metres: `k+mmm` or a plain number of metres.
pub fn parse_station(u: &DisplayUnits, text: &str) -> Option<f64> {
    u.parse_station(text).ok().filter(|v| v.is_finite())
}

/// A typed station into metres, or the reason it was refused.
pub fn parse_station_or(u: &DisplayUnits, text: &str, what: &str) -> Result<f64, String> {
    let t = text.trim();
    if t.is_empty() {
        return Err(format!("Enter {what}."));
    }
    parse_station(u, t).ok_or_else(|| format!("'{t}' is not a station ({what}). Examples: 1+250.500, 1250.5."))
}

/// A station typed in a list or a station field: the unit-aware parse where the text is a station,
/// so the same forms work in every sheet field.
pub fn same_station(u: &DisplayUnits, a_m: f64, b_m: f64) -> bool {
    station(u, a_m) == station(u, b_m)
}

/// A point typed on the command line while a design tool waits for one, in metres (the design
/// tools hold metres): `x,y`, `@dx,dy` relative to `last`, or `@distance<angle`. Each length is
/// parsed as a length in the display unit (`12.5`, `12.5m`, `12500mm`).
pub fn parse_point(u: &DisplayUnits, text: &str, last_m: Option<(f64, f64)>) -> Option<(f64, f64)> {
    let last = last_m.map(|p| afrobim_kernel::Pt::new(p.0 * 1000.0, p.1 * 1000.0));
    crate::anno_glue::parse_point(text, last, u).map(|p| (p.x / 1000.0, p.y / 1000.0))
}

/// A tick label of a plot axis, metres in: the display unit with one decimal for metres and none
/// for millimetres.
pub fn axis(u: &DisplayUnits, metres: f64) -> String {
    let decimals = if u.length == afrobim_annotate::LengthUnit::Metre { 1 } else { 0 };
    format!("{:.*}", decimals, metres * 1000.0 / u.length.mm_per_unit())
}

/// K, metres of curve per 1 % change of grade, as a length per percent in the display unit.
pub fn k(u: &DisplayUnits, k_m_per_percent: f64) -> String {
    len(u, k_m_per_percent)
}

/// The label of a K column or field: `K (m per %)`.
pub fn k_head(u: &DisplayUnits) -> String {
    format!("K ({} per %)", suffix(u))
}

/// Metres with the document's decimals but trailing zeros trimmed, for a form field that starts
/// from a stored value (`25` rather than `25.000`): the same number, shorter. Still parses back
/// through [`parse_len`] to the same value at the display precision.
pub fn len_field(u: &DisplayUnits, metres: f64) -> String {
    let s = len(u, metres);
    match s.contains('.') {
        true => s.trim_end_matches('0').trim_end_matches('.').to_string(),
        false => s,
    }
}

/// The editable cells of the PI table and the PVI table. The windows show and read them only
/// through [`cell_text`], [`parse_cell`] and [`cell_changed`], so what a cell shows can be typed
/// back and a rounded display retyped is no edit.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Cell {
    East,
    North,
    Radius,
    SpiralIn,
    SpiralOut,
    PviStation,
    PviLevel,
    CurveLength,
    CurveK,
}

impl Cell {
    fn is_station(self) -> bool {
        self == Cell::PviStation
    }

    /// A cell whose zero is "none" and shows blank: a curve with no radius, no spiral, no curve.
    fn blank_is_none(self) -> bool {
        matches!(self, Cell::Radius | Cell::SpiralIn | Cell::SpiralOut)
    }
}

/// What a cell shows for its stored value (metres; a station for [`Cell::PviStation`]).
pub fn cell_text(u: &DisplayUnits, cell: Cell, value: f64) -> String {
    if cell.blank_is_none() && value <= 0.0 {
        String::new()
    } else if cell.is_station() {
        station(u, value)
    } else if cell == Cell::CurveK {
        k(u, value)
    } else {
        len(u, value)
    }
}

/// What a cell holds after typing: `Ok(Some(value))` in metres (or a station in metres),
/// `Ok(None)` for a blank where blank means none (radius, spirals), `Err` for text that does not
/// read as the cell's quantity or is out of range (a radius or a curve length of zero or less, a
/// negative spiral).
pub fn parse_cell(u: &DisplayUnits, cell: Cell, text: &str) -> Result<Option<f64>, ()> {
    if text.trim().is_empty() {
        return if cell.blank_is_none() { Ok(None) } else { Err(()) };
    }
    let v = if cell.is_station() { parse_station(u, text) } else { parse_len(u, text) }.ok_or(())?;
    match cell {
        Cell::Radius | Cell::CurveLength | Cell::CurveK if v <= 0.0 => Err(()),
        Cell::SpiralIn | Cell::SpiralOut if v < 0.0 => Err(()),
        _ => Ok(Some(v)),
    }
}

/// True when typing `new` into a cell that holds `old` changes what it shows. A blank new value
/// for a cell whose zero is none counts as zero.
pub fn cell_changed(u: &DisplayUnits, cell: Cell, old: f64, new: Option<f64>) -> bool {
    let new = new.unwrap_or(0.0);
    if cell.is_station() {
        !same_station(u, old, new)
    } else {
        !same_len(u, old, new)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use afrobim_annotate::LengthUnit;

    fn mm() -> DisplayUnits {
        DisplayUnits { length: LengthUnit::Millimetre, length_decimals: 1, station_decimals: 2, ..DisplayUnits::default() }
    }

    #[test]
    fn a_length_is_shown_in_the_display_unit_and_typed_back() {
        let metres = DisplayUnits::default();
        assert_eq!(len(&metres, 469720.2774), "469720.277");
        assert_eq!(len_u(&metres, 300.0), "300.000 m");
        assert_eq!(parse_len(&metres, "300.000"), Some(300.0));
        assert_eq!(parse_len(&metres, "300,5"), Some(300.5));
        assert_eq!(parse_len(&metres, "300500mm"), Some(300.5));
        // In millimetres a bare number is millimetres; the suffix overrides it; storage stays metres.
        let u = mm();
        assert_eq!(len(&u, 300.0), "300000.0");
        assert_eq!(len_u(&u, 0.0125), "12.5 mm");
        assert_eq!(parse_len(&u, "300000.0"), Some(300.0));
        assert_eq!(parse_len(&u, "300m"), Some(300.0));
        assert_eq!(parse_len(&u, "12.5"), Some(0.0125));
        assert_eq!(parse_len(&u, "abc"), None);
        assert_eq!(parse_len(&u, ""), None);
        assert_eq!(head(&u, "East"), "East (mm)");
        assert_eq!(head(&metres, "Radius"), "Radius (m)");
    }

    #[test]
    fn every_shown_value_reads_back_to_the_same_shown_value() {
        for u in [DisplayUnits::default(), mm(), DisplayUnits { length_decimals: 0, station_decimals: 0, ..DisplayUnits::default() }] {
            for v in [0.0, 0.001, 12.5, 300.0, 469720.277, 5.0e6 + 0.123, 1000.0, 46.0] {
                assert!(same_len(&u, parse_len(&u, &len(&u, v)).unwrap(), v), "{v} {:?}", u.length);
                assert!(same_len(&u, parse_len(&u, &len_field(&u, v)).unwrap(), v), "field {v} {:?}", u.length);
                assert!(same_len(&u, parse_len(&u, &len_u(&u, v)).unwrap(), v), "with unit {v}");
                assert!(same_station(&u, parse_station(&u, &station(&u, v)).unwrap(), v), "station {v}");
            }
        }
    }

    #[test]
    fn a_station_is_shown_as_k_plus_mmm_and_typed_either_way() {
        let u = DisplayUnits::default();
        assert_eq!(station(&u, 1250.5), "1+250.500");
        assert_eq!(parse_station(&u, "1+250.500"), Some(1250.5));
        assert_eq!(parse_station(&u, "1250.5"), Some(1250.5));
        assert_eq!(station(&u, 0.0), "0+000.000");
        assert_eq!(parse_station(&u, "0+000.000"), Some(0.0));
        assert_eq!(parse_station(&u, "1+5"), None);
        assert!(parse_station_or(&u, "x", "the first station").unwrap_err().contains("not a station"));
        assert!(parse_station_or(&u, " ", "the first station").unwrap_err().starts_with("Enter"));
        assert_eq!(station(&mm(), 1250.5), "1+250.50");
        assert_eq!(parse_station(&mm(), "1+250.50"), Some(1250.5));
    }

    #[test]
    fn k_follows_the_length_unit_and_a_retyped_rounding_is_no_edit() {
        assert_eq!(k(&DisplayUnits::default(), 40.0), "40.000");
        assert_eq!(k(&mm(), 40.0), "40000.0");
        assert_eq!(k_head(&mm()), "K (mm per %)");
        // 0.0004 m shows as 0.000 m at 3 decimals; typing the shown 0.000 back is no change.
        let u = DisplayUnits::default();
        assert!(same_len(&u, 0.0004, parse_len(&u, "0.000").unwrap()));
        assert!(!same_len(&u, 0.0014, 0.0));
    }

    #[test]
    fn a_refused_length_names_what_it_was_for() {
        let u = DisplayUnits::default();
        assert!(parse_len_or(&u, "", "the overlap").unwrap_err().starts_with("Enter the overlap"));
        assert!(parse_len_or(&u, "2x", "the overlap").unwrap_err().contains("not a length"));
        assert_eq!(parse_len_or(&u, "20", "the overlap"), Ok(20.0));
    }

    #[test]
    fn every_editable_cell_of_the_pi_and_pvi_tables_shows_and_reads_back() {
        for u in [DisplayUnits::default(), mm(), DisplayUnits { length_decimals: 2, station_decimals: 1, ..DisplayUnits::default() }] {
            for (cell, value) in [
                (Cell::East, 469720.277),
                (Cell::North, 94256.001),
                (Cell::Radius, 300.0),
                (Cell::SpiralIn, 80.0),
                (Cell::SpiralOut, 62.5),
                (Cell::PviStation, 1300.0),
                (Cell::PviLevel, 104.25),
                (Cell::CurveLength, 100.0),
                (Cell::CurveK, 40.0),
            ] {
                let shown = cell_text(&u, cell, value);
                let read = parse_cell(&u, cell, &shown).unwrap_or_else(|_| panic!("{cell:?} '{shown}' does not read back")).unwrap_or_else(|| panic!("{cell:?} '{shown}' read as none"));
                assert!(!cell_changed(&u, cell, value, Some(read)), "{cell:?} {shown}: retyping the shown value is an edit");
                assert!(cell_changed(&u, cell, value, Some(value + 5.0)), "{cell:?}: a real change is not seen");
                // The value as typed with the other unit's suffix is the same value.
                if !matches!(cell, Cell::PviStation) {
                    let with_m = parse_cell(&u, cell, &format!("{value}m")).unwrap().unwrap();
                    assert!((with_m - value).abs() < 1e-9, "{cell:?}");
                }
            }
        }
        // Blank is none for a radius and a spiral, and is refused elsewhere.
        let u = DisplayUnits::default();
        assert_eq!(parse_cell(&u, Cell::Radius, " "), Ok(None));
        assert_eq!(parse_cell(&u, Cell::SpiralIn, ""), Ok(None));
        assert_eq!(parse_cell(&u, Cell::East, ""), Err(()));
        assert_eq!(cell_text(&u, Cell::Radius, 0.0), "");
        assert_eq!(cell_text(&u, Cell::SpiralOut, 0.0), "");
        // Out of range or not a quantity.
        assert_eq!(parse_cell(&u, Cell::Radius, "0"), Err(()));
        assert_eq!(parse_cell(&u, Cell::Radius, "-5"), Err(()));
        assert_eq!(parse_cell(&u, Cell::SpiralIn, "-1"), Err(()));
        assert_eq!(parse_cell(&u, Cell::CurveLength, "0"), Err(()));
        assert_eq!(parse_cell(&u, Cell::CurveK, "abc"), Err(()));
        assert_eq!(parse_cell(&u, Cell::PviStation, "1+5"), Err(()));
        // A station is typed as k+mmm or as metres, in either display unit.
        assert_eq!(parse_cell(&mm(), Cell::PviStation, "1+300.00"), Ok(Some(1300.0)));
        assert_eq!(parse_cell(&mm(), Cell::PviStation, "1300"), Ok(Some(1300.0)));
        // Clearing a radius is a change; blank over blank is not.
        assert!(cell_changed(&u, Cell::Radius, 300.0, None));
        assert!(!cell_changed(&u, Cell::Radius, 0.0, None));
        assert_eq!(axis(&u, 103.0), "103.0");
        assert_eq!(axis(&mm(), 103.0), "103000");
    }

    #[test]
    fn the_plan_editor_writes_its_curve_labels_in_the_display_units() {
        let mut doc = afrobim_kernel::Document::default();
        let (d, _) = crate::design_glue::create(&mut doc, "Road", 1000.0, &[(0.0, 0.0), (400.0, 0.0), (400.0, 300.0)]).unwrap();
        crate::design_glue::edit_horizontal(&mut doc, d, crate::design_glue::HEdit::SetRadius(1, 100.0)).unwrap();
        let design = crate::design_glue::load(&doc, d).unwrap();
        // Metres: the curve data carries the unit and the document's decimals; stations are k+mmm.
        let metres = crate::design_ui::labels_in(&design, &DisplayUnits::default());
        let data = metres.iter().find(|l| l.kind == afrobim_design::PlanLabelKind::CurveData).unwrap();
        assert!(data.text.contains("R=100.000 m") && data.text.contains("T=") && data.text.contains(" m  L="), "{}", data.text);
        assert!(metres.iter().any(|l| l.kind == afrobim_design::PlanLabelKind::Pi && l.text.starts_with("PI 1  1+")), "{:?}", metres.iter().map(|l| &l.text).collect::<Vec<_>>());
        // Millimetres with one decimal and stations to two.
        let millimetres = crate::design_ui::labels_in(&design, &mm());
        let data = millimetres.iter().find(|l| l.kind == afrobim_design::PlanLabelKind::CurveData).unwrap();
        assert!(data.text.contains("R=100000.0 mm"), "{}", data.text);
        let point = millimetres.iter().find(|l| l.kind == afrobim_design::PlanLabelKind::CurvePoint).unwrap();
        let station = point.text.split_whitespace().last().unwrap();
        assert!(station.contains('+') && station.split('.').nth(1).is_some_and(|d| d.len() == 2), "{}", point.text);
        // The shown station reads back as the station.
        assert!(parse_station(&mm(), station).is_some());
    }
}
