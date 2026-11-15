//! Drawing units from the header.
//!
//! `$INSUNITS` codes and names are those of `ezdxf.units` (1.4.5): 0 unitless, 1 inches, 2 feet,
//! 3 miles, 4 millimetres, 5 centimetres, 6 metres, 7 kilometres, 8 microinches, 9 mils, 10 yards,
//! 11 angstroms, 12 nanometres, 13 microns, 14 decimetres, 15 decametres, 16 hectometres,
//! 17 gigametres, 18 astronomical units, 19 light years, 20 parsecs, 21 to 24 US survey foot,
//! inch, yard and mile. `$MEASUREMENT` is 0 for English and 1 for metric.
//!
//! The millimetres per unit are the international definitions (inch 25.4 mm exactly, foot 12
//! inches, yard 3 feet, mile 5280 feet; the US survey foot is 1200/3937 m). When the header does
//! not say (`$INSUNITS` 0 or absent) a value is assumed and `UnitsInfo::assumed` is set: metres
//! for a metric file (road drawings are commonly in metres) and inches for an English one. The
//! caller overrides it with `Underlay::unit_scale_mm`.

use super::model::UnitsInfo;

const INCH: f64 = 25.4;
const FOOT: f64 = 12.0 * INCH;
const US_FOOT: f64 = 1000.0 * 1200.0 / 3937.0;

/// (name, millimetres per unit) of an `$INSUNITS` code.
pub fn insunits(code: i32) -> Option<(&'static str, f64)> {
    Some(match code {
        1 => ("inches", INCH),
        2 => ("feet", FOOT),
        3 => ("miles", 5280.0 * FOOT),
        4 => ("millimetres", 1.0),
        5 => ("centimetres", 10.0),
        6 => ("metres", 1000.0),
        7 => ("kilometres", 1.0e6),
        8 => ("microinches", INCH * 1.0e-6),
        9 => ("mils", INCH * 1.0e-3),
        10 => ("yards", 3.0 * FOOT),
        11 => ("angstroms", 1.0e-7),
        12 => ("nanometres", 1.0e-6),
        13 => ("microns", 1.0e-3),
        14 => ("decimetres", 100.0),
        15 => ("decametres", 1.0e4),
        16 => ("hectometres", 1.0e5),
        17 => ("gigametres", 1.0e12),
        18 => ("astronomical units", 1.495_978_707e14),
        19 => ("light years", 9.460_730_472_580_8e18),
        20 => ("parsecs", 3.085_677_581_491_367e19),
        21 => ("US survey feet", US_FOOT),
        22 => ("US survey inches", US_FOOT / 12.0),
        23 => ("US survey yards", US_FOOT * 3.0),
        24 => ("US survey miles", US_FOOT * 5280.0),
        _ => return None,
    })
}

/// Fills `name`, `mm_per_unit` and `assumed` from `insunits` and `measurement`.
pub fn units_from_header(u: &mut UnitsInfo) {
    match u.insunits.and_then(insunits) {
        Some((name, mm)) => {
            u.name = name.to_string();
            u.mm_per_unit = mm;
            u.assumed = false;
        }
        None => {
            u.assumed = true;
            if u.measurement == Some(0) {
                u.name = "inches (assumed: the file does not state units and is marked English)".to_string();
                u.mm_per_unit = INCH;
            } else {
                u.name = "metres (assumed: the file does not state units)".to_string();
                u.mm_per_unit = 1000.0;
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn known_codes_and_the_fallback() {
        assert_eq!(insunits(4), Some(("millimetres", 1.0)));
        assert_eq!(insunits(6).unwrap().1, 1000.0);
        assert!((insunits(2).unwrap().1 - 304.8).abs() < 1e-12);
        assert!((insunits(21).unwrap().1 - 304.800_609_6).abs() < 1e-6);
        assert!(insunits(0).is_none() && insunits(99).is_none());
        let mut u = UnitsInfo { insunits: Some(6), measurement: Some(1), ..Default::default() };
        units_from_header(&mut u);
        assert_eq!((u.mm_per_unit, u.assumed), (1000.0, false));
        let mut u = UnitsInfo { insunits: Some(0), measurement: Some(1), ..Default::default() };
        units_from_header(&mut u);
        assert!(u.assumed && u.mm_per_unit == 1000.0 && u.name.contains("assumed"));
        let mut u = UnitsInfo { insunits: None, measurement: Some(0), ..Default::default() };
        units_from_header(&mut u);
        assert!(u.assumed && (u.mm_per_unit - 25.4).abs() < 1e-12);
    }
}
