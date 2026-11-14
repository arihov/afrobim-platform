//! Station equations: internal station ↔ displayed station, gaps, overlaps and labels.
//!
//! The example used throughout, on an alignment whose internal stations run 1000 to 1900:
//!
//! | region | internal      | displayed     | rule                      |
//! |--------|---------------|---------------|---------------------------|
//! | 0      | 1000 to 1300  | 1000 to 1300  | displayed = internal      |
//! | 1      | 1300 to 1600  | 1350 to 1650  | displayed = internal + 50 |
//! | 2      | 1600 to 1900  | 1500 to 1800  | displayed = internal − 100|
//!
//! Equation 0 at internal 1300: back 1300, ahead 1350, so displayed 1300 to 1350 is a **gap**.
//! Equation 1 at internal 1600: back 1650, ahead 1500, so displayed 1500 to 1650 is an **overlap**
//! (it occurs in region 1 and again in region 2).

use afrobim_alignment::{
    Alignment, AlignmentBuilder, DisplayedStation, StationCandidate, StationDirection, StationEquation, StationEquationError, StationEquations,
};

const START: f64 = 1000.0;
const END: f64 = 1900.0;

fn example() -> StationEquations {
    StationEquations::new(vec![
        StationEquation { internal_m: 1300.0, back_m: Some(1300.0), ahead_m: 1350.0, direction: StationDirection::Increasing },
        StationEquation { internal_m: 1600.0, back_m: Some(1650.0), ahead_m: 1500.0, direction: StationDirection::Increasing },
    ])
    .unwrap()
}

#[test]
fn internal_to_displayed() {
    let e = example();
    assert_eq!(e.region_count(), 3);
    let shown = |internal: f64| e.to_displayed(internal);
    assert_eq!(shown(1200.0), DisplayedStation { station_m: 1200.0, region: 0 });
    assert_eq!(shown(1299.5), DisplayedStation { station_m: 1299.5, region: 0 });
    // On an equation: the ahead station, in the region ahead.
    assert_eq!(shown(1300.0), DisplayedStation { station_m: 1350.0, region: 1 });
    assert_eq!(shown(1450.0), DisplayedStation { station_m: 1500.0, region: 1 });
    assert_eq!(shown(1600.0), DisplayedStation { station_m: 1500.0, region: 2 });
    assert_eq!(shown(1800.0), DisplayedStation { station_m: 1700.0, region: 2 });
    // The back station of each equation follows from what is before it.
    assert_eq!(e.back_station(0), Some(1300.0));
    assert_eq!(e.back_station(1), Some(1650.0));
    assert_eq!(e.back_station(2), None);

    // Labels go through the equations. Two places share the label 1+500.000: one in each region.
    assert_eq!(e.label(1200.0, 3), "1+200.000");
    assert_eq!(e.label(1450.0, 3), "1+500.000");
    assert_eq!(e.label(1600.0, 3), "1+500.000");
    assert_eq!(e.label(1800.0, 1), "1+700.0");

    // No equations: displayed is internal.
    let none = StationEquations::default();
    assert!(none.is_empty());
    assert_eq!(none.to_displayed(1234.5), DisplayedStation { station_m: 1234.5, region: 0 });
    assert_eq!(none.to_internal(1234.5, START, END), Ok(1234.5));
    assert_eq!(none.label(1234.5, 3), "1+234.500");
}

#[test]
fn displayed_to_internal_with_a_gap_and_an_overlap() {
    let e = example();
    // Unique stations.
    assert_eq!(e.to_internal(1200.0, START, END), Ok(1200.0)); // region 0
    assert_eq!(e.to_internal(1400.0, START, END), Ok(1350.0)); // region 1 only: 1400 − 50
    assert_eq!(e.to_internal(1700.0, START, END), Ok(1800.0)); // region 2 only: 1700 + 100
    assert_eq!(e.to_internal(1800.0, START, END), Ok(1900.0)); // the end of the alignment
    // Both names of the point at equation 0 find it: back 1300 from behind, ahead 1350 from ahead.
    assert_eq!(e.to_internal(1300.0, START, END), Ok(1300.0));
    assert_eq!(e.to_internal(1350.0, START, END), Ok(1300.0));
    assert_eq!(e.candidates(1300.0, START, END), [StationCandidate { region: 0, internal_m: 1300.0 }]);
    assert_eq!(e.candidates(1350.0, START, END), [StationCandidate { region: 1, internal_m: 1300.0 }]);

    // The gap: displayed 1300 to 1350 is nowhere.
    assert_eq!(e.to_internal(1320.0, START, END), Err(StationEquationError::NotOnAlignment { displayed_m: 1320.0 }));
    assert!(e.candidates(1320.0, START, END).is_empty());
    assert_eq!(
        e.to_internal(1320.0, START, END).unwrap_err().to_string(),
        "station 1+320.000 is not on the alignment: it is beyond its ends or in a gap between station equations"
    );
    // Beyond the ends.
    assert_eq!(e.to_internal(999.0, START, END), Err(StationEquationError::NotOnAlignment { displayed_m: 999.0 }));
    assert_eq!(e.to_internal(1801.0, START, END), Err(StationEquationError::NotOnAlignment { displayed_m: 1801.0 }));

    // The overlap: displayed 1550 is internal 1500 in region 1 and internal 1650 in region 2.
    let both = vec![StationCandidate { region: 1, internal_m: 1500.0 }, StationCandidate { region: 2, internal_m: 1650.0 }];
    assert_eq!(e.candidates(1550.0, START, END), both);
    let ambiguous = e.to_internal(1550.0, START, END).unwrap_err();
    assert_eq!(ambiguous, StationEquationError::Ambiguous { displayed_m: 1550.0, candidates: both });
    assert_eq!(ambiguous.to_string(), "station 1+550.000 occurs 2 times on the alignment (regions 1, 2); say which region is meant");
    // With the region there is one answer.
    assert_eq!(e.to_internal_in_region(1550.0, 1, START, END), Ok(1500.0));
    assert_eq!(e.to_internal_in_region(1550.0, 2, START, END), Ok(1650.0));
    // The edges of the overlap are in it: 1500 is internal 1450 or 1600; 1650 is internal 1600 or 1750.
    assert_eq!(e.candidates(1500.0, START, END).iter().map(|c| c.internal_m).collect::<Vec<_>>(), [1450.0, 1600.0]);
    assert_eq!(e.candidates(1650.0, START, END).iter().map(|c| c.internal_m).collect::<Vec<_>>(), [1600.0, 1750.0]);
    // Just outside it there is one again.
    assert_eq!(e.to_internal(1499.0, START, END), Ok(1449.0));
    assert_eq!(e.to_internal(1651.0, START, END), Ok(1751.0));

    // Asking for a region the station is not in, or that does not exist.
    let wrong = e.to_internal_in_region(1550.0, 0, START, END).unwrap_err();
    assert_eq!(wrong, StationEquationError::NotInRegion { displayed_m: 1550.0, region: 0, from_m: 1000.0, to_m: 1300.0 });
    assert_eq!(wrong.to_string(), "station 1+550.000 is not in region 0, which runs from 1+000.000 to 1+300.000");
    assert_eq!(e.to_internal_in_region(1400.0, 2, START, END), Err(StationEquationError::NotInRegion { displayed_m: 1400.0, region: 2, from_m: 1500.0, to_m: 1800.0 }));
    assert_eq!(e.to_internal_in_region(1550.0, 3, START, END), Err(StationEquationError::NoSuchRegion { region: 3, last: 2 }));

    // There and back again, everywhere, when the region is kept.
    let mut internal = START;
    while internal <= END {
        let shown = e.to_displayed(internal);
        assert_eq!(e.to_internal_in_region(shown.station_m, shown.region, START, END), Ok(internal), "{internal}");
        internal += 12.5;
    }
}

#[test]
fn decreasing_and_trivial_equations() {
    // Internal 0 to 800. At internal 500 the displayed station becomes 2000 and then runs down:
    // displayed = 2000 − (internal − 500).
    let e = StationEquations::new(vec![StationEquation { internal_m: 500.0, back_m: None, ahead_m: 2000.0, direction: StationDirection::Decreasing }]).unwrap();
    assert_eq!(e.to_displayed(600.0), DisplayedStation { station_m: 1900.0, region: 1 });
    assert_eq!(e.to_displayed(800.0), DisplayedStation { station_m: 1700.0, region: 1 });
    assert_eq!(e.to_internal(1900.0, 0.0, 800.0), Ok(600.0));
    assert_eq!(e.to_internal(300.0, 0.0, 800.0), Ok(300.0));
    assert_eq!(e.to_internal(1699.0, 0.0, 800.0), Err(StationEquationError::NotOnAlignment { displayed_m: 1699.0 }));
    let wrong = e.to_internal_in_region(1650.0, 1, 0.0, 800.0).unwrap_err();
    assert_eq!(wrong, StationEquationError::NotInRegion { displayed_m: 1650.0, region: 1, from_m: 2000.0, to_m: 1700.0 });

    // An equation that changes nothing (ahead = back) does not make its own station ambiguous.
    let same = StationEquations::new(vec![StationEquation::new(500.0, 500.0)]).unwrap();
    assert_eq!(same.candidates(500.0, 0.0, 800.0), [StationCandidate { region: 1, internal_m: 500.0 }]);
    assert_eq!(same.to_internal(500.0, 0.0, 800.0), Ok(500.0));

    // An equation beyond the end of the alignment has no region on it.
    let beyond = StationEquations::new(vec![StationEquation::new(900.0, 5000.0)]).unwrap();
    assert_eq!(beyond.to_internal(5050.0, 0.0, 800.0), Err(StationEquationError::NotOnAlignment { displayed_m: 5050.0 }));
    assert_eq!(beyond.to_internal_in_region(5050.0, 1, 0.0, 800.0), Err(StationEquationError::NotOnAlignment { displayed_m: 5050.0 }));
    assert_eq!(beyond.to_internal(700.0, 0.0, 800.0), Ok(700.0));
}

#[test]
fn validation() {
    let at = |internal_m: f64, back_m: Option<f64>, ahead_m: f64| StationEquation { internal_m, back_m, ahead_m, direction: StationDirection::Increasing };
    assert_eq!(StationEquations::new(vec![at(1300.0, None, 1350.0), at(1300.0, None, 1400.0)]), Err(StationEquationError::NotIncreasing { index: 1 }));
    assert_eq!(StationEquations::new(vec![at(1600.0, None, 1350.0), at(1300.0, None, 1400.0)]), Err(StationEquationError::NotIncreasing { index: 1 }));
    assert_eq!(StationEquations::new(vec![at(f64::NAN, None, 1350.0)]), Err(StationEquationError::NotFinite { index: 0 }));
    assert_eq!(StationEquations::new(vec![at(1300.0, Some(f64::INFINITY), 1350.0)]), Err(StationEquationError::NotFinite { index: 0 }));
    // A back station that the stationing before it does not arrive at: before the first equation
    // displayed = internal, so the back station at internal 1300 is 1300, not 1310.
    let mismatch = StationEquations::new(vec![at(1300.0, Some(1310.0), 1350.0)]).unwrap_err();
    assert_eq!(mismatch, StationEquationError::BackMismatch { index: 0, back_m: 1310.0, expected_m: 1300.0 });
    assert_eq!(mismatch.to_string(), "station equation 0: back station 1+310.000 does not agree with the stationing before it, which arrives at 1+300.000");
    // The second equation's back station follows from the first: 1350 + (1600 − 1300) = 1650.
    assert_eq!(
        StationEquations::new(vec![at(1300.0, None, 1350.0), at(1600.0, Some(1600.0), 1500.0)]),
        Err(StationEquationError::BackMismatch { index: 1, back_m: 1600.0, expected_m: 1650.0 })
    );
    // Within a millimetre is agreement.
    assert!(StationEquations::new(vec![at(1300.0, Some(1300.0004), 1350.0)]).is_ok());
}

#[test]
fn on_an_alignment_and_in_its_json() {
    let mut a: Alignment = AlignmentBuilder::new("MC10", START, 0.0, 0.0, 0.0).line(END - START).build().unwrap();
    // Without equations.
    assert_eq!(a.displayed_station(1450.0), Some(DisplayedStation { station_m: 1450.0, region: 0 }));
    assert_eq!(a.internal_station(1450.0, None), Ok(1450.0));
    assert_eq!(a.station_label(1450.0, 3), "1+450.000");

    a.station_equations = example();
    assert_eq!(a.displayed_station(1450.0), Some(DisplayedStation { station_m: 1500.0, region: 1 }));
    assert_eq!(a.displayed_station(999.0), None);
    assert_eq!(a.displayed_station(1900.5), None);
    assert_eq!(a.station_label(1800.0, 3), "1+700.000");
    assert_eq!(a.internal_station(1700.0, None), Ok(1800.0));
    assert_eq!(a.internal_station(1550.0, Some(2)), Ok(1650.0));
    assert!(matches!(a.internal_station(1550.0, None), Err(StationEquationError::Ambiguous { .. })));
    assert_eq!(a.internal_station_candidates(1550.0).len(), 2);
    assert!(a.internal_station_candidates(1320.0).is_empty());
    // The geometry is untouched: it is asked by internal station.
    assert_eq!(a.point_at(1650.0).unwrap().x, 650.0);

    let json = serde_json::to_string(&a).unwrap();
    assert!(json.contains(r#""station_equations":[{"internal_m":1300.0,"back_m":1300.0,"ahead_m":1350.0,"direction":"increasing"},"#), "{json}");
    let back: Alignment = serde_json::from_str(&json).unwrap();
    assert_eq!(back, a);
    // Minimal form: the back station and direction may be left out.
    let e: StationEquations = serde_json::from_str(r#"[{"internal_m":1300,"ahead_m":1350}]"#).unwrap();
    assert_eq!(e.equations, [StationEquation::new(1300.0, 1350.0)]);
}
