//! Data from **BS 5489-1:2003** (incorporating Corrigendum No. 1), *Code of practice for the
//! design of road lighting, Part 1: Lighting of roads and public amenity areas*.
//!
//! # This edition is superseded
//!
//! BS 5489-1:2003 was replaced in 2013 and again in 2020. It is used here because it is the
//! edition the owner holds. The later editions select lighting classes differently and use the
//! M, C and P class names of BS EN 13201-2:2015 in place of the ME, CE and S names below. Every
//! value in this module must be confirmed against the current edition before it is relied on,
//! and everything that reports one says so ([`SUPERSEDED_NOTICE`]). All the BS 5489-1 data of
//! the crate is in this one module so that a later edition can replace it.
//!
//! # What is here
//!
//! | Item | Source in BS 5489-1:2003 |
//! |---|---|
//! | [`TABLE_2_CLEARANCE`], [`min_clearance_m`] | Table 2 (clause 5.2.1.2) |
//! | [`VERTICAL_CLEARANCE_OVER_CARRIAGEWAY_M`], [`VERTICAL_CLEARANCE_PEDESTRIAN_M`] | clause 5.2.1.2 |
//! | [`BRACKET_PROJECTION_MAX_FRACTION_OF_HEIGHT`] | clause 5.2.3.3 |
//! | [`TABLE_B1_COMPARABLE`] | Table B.1 |
//! | [`RoadType`], [`traffic_route_class`] | Table B.2 |
//! | [`conflict_area_class`] | Table B.3 |
//! | [`subsidiary_road_class`] | Table B.4 |
//! | [`town_centre_class`] | Table B.5 |
//! | [`luminaire_maintenance_factor`] | Table D.1 (Annex D, informative) |
//!
//! Annex B and Annex D are informative: they give guidance, not requirements.
//!
//! # What is not here
//!
//! The performance requirements of the classes (luminance, uniformities, threshold increment,
//! surround ratio, illuminance) are in BS EN 13201-2, which BS 5489-1:2003 refers to and does
//! not reproduce. They are not in this crate. Nor does the 2003 edition tabulate spacing,
//! mounting height or overhang: those come out of the photometric calculation of
//! BS EN 13201-3.
//!
//! # Readings to confirm
//!
//! - Table 2 gives the clearance "from edge of carriageway to face of lighting column" at four
//!   design speeds. It is not interpolated here. The crate's set-back is to the column axis and
//!   no column width is modelled, so the face is nearer the carriageway than the set-back by
//!   half the column's width.
//! - Table B.2 gives strategic routes the flows "<15 000" and ">15 000" (a flow of exactly
//!   15 000 falls in neither) where the other rows use "≤15 000". [`traffic_route_class`]
//!   returns [`ClassSelection::NotCovered`] for exactly 15 000 on a strategic route.
//! - Table B.3 is written in terms of ME1 to ME5 without the a, b, c suffixes of ME3 and ME4;
//!   [`conflict_area_class`] ignores the suffix. ME6 has no entry.

/// The edition every value in this module comes from.
pub const EDITION: &str = "BS 5489-1:2003";

/// The warning that goes with anything taken from this module.
pub const SUPERSEDED_NOTICE: &str = "BS 5489-1:2003 is a superseded edition (replaced in 2013 and again in 2020): confirm every value taken from it against the current edition.";

/// BS 5489-1:2003 Table 2 "Recommended minimum clearances from edge of carriageway to face of
/// lighting column": (design speed km/h, horizontal clearance m).
pub const TABLE_2_CLEARANCE: [(f64, f64); 4] = [(50.0, 0.8), (80.0, 1.0), (100.0, 1.5), (120.0, 1.5)];

/// The Table 2 clearance for a design speed, m. `None` for a speed that is not a row of the
/// table: it is not interpolated.
pub fn min_clearance_m(design_speed_kph: f64) -> Option<f64> {
    TABLE_2_CLEARANCE.iter().find(|r| r.0 == design_speed_kph).map(|r| r.1)
}

/// BS 5489-1:2003 clause 5.2.1.2: the lowest point of luminaires or bracket arms that overhang
/// the carriageway, or are within the Table 2 clearance, should be at least this far above the
/// carriageway surface, m.
pub const VERTICAL_CLEARANCE_OVER_CARRIAGEWAY_M: f64 = 5.7;

/// BS 5489-1:2003 clause 5.2.1.2: height clearance over a pedestrian-only area of a public
/// highway not accessible to vehicles, m.
pub const VERTICAL_CLEARANCE_PEDESTRIAN_M: f64 = 2.1;

/// BS 5489-1:2003 clause 5.2.3.3: "it is recommended that [bracket projection] does not exceed
/// one quarter of the mounting height".
pub const BRACKET_PROJECTION_MAX_FRACTION_OF_HEIGHT: f64 = 0.25;

/// BS 5489-1:2003 Table B.1 "Lighting classes of comparable level": (ME, CE, S), empty where
/// the table has a dash.
pub const TABLE_B1_COMPARABLE: [(&str, &str, &str); 9] = [
    ("", "CE0", ""),
    ("ME1", "CE1", ""),
    ("ME2", "CE2", ""),
    ("ME3", "CE3", "S1"),
    ("ME4", "CE4", "S2"),
    ("ME5", "CE5", "S3"),
    ("ME6", "", "S4"),
    ("", "", "S5"),
    ("", "", "S6"),
];

/// The rows of BS 5489-1:2003 Table B.2 "Lighting classes for motorways and traffic routes".
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RoadType {
    /// Motorway, main carriageway in complex interchange areas.
    MotorwayComplexInterchange,
    /// Motorway, main carriageway with interchanges less than 3 km apart.
    MotorwayInterchangesUnder3Km,
    /// Motorway, main carriageway with interchanges 3 km or more apart.
    MotorwayInterchanges3KmOrMore,
    /// Motorway emergency lanes.
    MotorwayEmergencyLane,
    /// Strategic route (trunk and some principal "A" roads), single carriageway.
    StrategicSingle,
    /// Strategic route, dual carriageway.
    StrategicDual,
    /// Main distributor (major urban network and inter-primary links), single carriageway.
    MainDistributorSingle,
    /// Main distributor, dual carriageway.
    MainDistributorDual,
    /// Secondary distributor, rural areas (environmental zone E1/2).
    SecondaryDistributorRural,
    /// Secondary distributor, urban areas (zone E3).
    SecondaryDistributorUrban,
    /// Link road, rural areas (zone E1/2).
    LinkRoadRural,
    /// Link road, urban areas (zone E3).
    LinkRoadUrban,
    /// Link road, urban areas, with high pedestrian or cyclist traffic.
    LinkRoadUrbanHighPedestrian,
}

impl RoadType {
    pub fn label(self) -> &'static str {
        match self {
            RoadType::MotorwayComplexInterchange => "motorway, main carriageway in complex interchange areas",
            RoadType::MotorwayInterchangesUnder3Km => "motorway, main carriageway with interchanges < 3 km",
            RoadType::MotorwayInterchanges3KmOrMore => "motorway, main carriageway with interchanges ≥ 3 km",
            RoadType::MotorwayEmergencyLane => "motorway emergency lanes",
            RoadType::StrategicSingle => "strategic route, single carriageway",
            RoadType::StrategicDual => "strategic route, dual carriageway",
            RoadType::MainDistributorSingle => "main distributor, single carriageway",
            RoadType::MainDistributorDual => "main distributor, dual carriageway",
            RoadType::SecondaryDistributorRural => "secondary distributor, rural (zone E1/2)",
            RoadType::SecondaryDistributorUrban => "secondary distributor, urban (zone E3)",
            RoadType::LinkRoadRural => "link road, rural (zone E1/2)",
            RoadType::LinkRoadUrban => "link road, urban (zone E3)",
            RoadType::LinkRoadUrbanHighPedestrian => "link road, urban (zone E3), high pedestrian or cyclist traffic",
        }
    }
}

/// What Table B.2 gives for a road type and flow.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ClassSelection {
    /// The class or classes of the table, e.g. `["ME3a"]` or `["ME4b", "S2"]`.
    Classes(&'static [&'static str]),
    /// The row depends on traffic flow and none was given.
    FlowNeeded,
    /// The table has no entry for this flow (see the module documentation).
    NotCovered,
}

/// BS 5489-1:2003 Table B.2: the lighting class for a road type and average daily traffic flow
/// (ADT, both directions). Footnotes b and c of the table (slip roads one class lower; ME3b or
/// ME3c in place of ME3a in urban areas) are not applied.
pub fn traffic_route_class(road: RoadType, adt: Option<f64>) -> ClassSelection {
    use ClassSelection::Classes;
    use RoadType::*;
    let by_flow = |f: &dyn Fn(f64) -> ClassSelection| match adt {
        Some(q) if q.is_finite() && q >= 0.0 => f(q),
        _ => ClassSelection::FlowNeeded,
    };
    match road {
        MotorwayComplexInterchange => by_flow(&|_| Classes(&["ME1"])),
        MotorwayInterchangesUnder3Km => by_flow(&|q| if q <= 40_000.0 { Classes(&["ME2"]) } else { Classes(&["ME1"]) }),
        MotorwayInterchanges3KmOrMore => by_flow(&|_| Classes(&["ME2"])),
        MotorwayEmergencyLane => Classes(&["ME4a"]),
        StrategicSingle | StrategicDual => by_flow(&|q| {
            if q < 15_000.0 {
                Classes(&["ME3a"])
            } else if q > 15_000.0 {
                Classes(&["ME2"])
            } else {
                ClassSelection::NotCovered
            }
        }),
        MainDistributorSingle | MainDistributorDual => by_flow(&|q| if q <= 15_000.0 { Classes(&["ME3a"]) } else { Classes(&["ME2"]) }),
        SecondaryDistributorRural => by_flow(&|q| {
            if q <= 7_000.0 {
                Classes(&["ME4a"])
            } else if q <= 15_000.0 {
                Classes(&["ME3b"])
            } else {
                Classes(&["ME3a"])
            }
        }),
        SecondaryDistributorUrban => by_flow(&|q| {
            if q <= 7_000.0 {
                Classes(&["ME3c"])
            } else if q <= 15_000.0 {
                Classes(&["ME3b"])
            } else {
                Classes(&["ME2"])
            }
        }),
        LinkRoadRural => Classes(&["ME5"]),
        LinkRoadUrban => Classes(&["ME4b", "S2"]),
        LinkRoadUrbanHighPedestrian => Classes(&["S1"]),
    }
}

/// BS 5489-1:2003 Table B.3: the conflict area class for the class of the traffic routes
/// approaching it ("the higher class normally determines the class at the conflict area").
/// Accepts ME1 to ME5 with or without a suffix letter; `None` for anything else.
pub fn conflict_area_class(traffic_route_class: &str) -> Option<&'static str> {
    let c = traffic_route_class.trim().to_uppercase();
    let number = c.strip_prefix("ME")?.trim_end_matches(['A', 'B', 'C']);
    match number {
        "1" => Some("CE0"),
        "2" => Some("CE1"),
        "3" => Some("CE2"),
        "4" => Some("CE3"),
        "5" => Some("CE4"),
        _ => None,
    }
}

/// Crime rate of Table B.4, relative to the local area.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CrimeRate {
    Low,
    Moderate,
    High,
}

/// Traffic flow of Tables B.4 and B.5 as their footnotes describe it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TrafficFlow {
    /// Equivalent to a residential road, solely associated with the adjacent properties.
    Low,
    /// Equivalent to a housing estate access road.
    Normal,
    High,
}

/// Environmental zones as grouped in Tables B.4 and B.5 (ILE guidance notes for the reduction
/// of light pollution).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Zone {
    E1,
    E2,
    E3,
    E4,
}

/// BS 5489-1:2003 Table B.4 "Lighting classes for subsidiary roads (pedestrians and cyclists)".
/// `ra_60_or_more` is whether the light source has a colour rendering index Ra ≥ 60. `None`
/// where the table has a dash (moderate or high crime, high flow, zone E1/E2). Note 2 of the
/// table (one class higher near traffic calming) is not applied.
pub fn subsidiary_road_class(crime: CrimeRate, ra_60_or_more: bool, flow: TrafficFlow, zone: Zone) -> Option<&'static str> {
    // Columns: low E1/E2, low E3/E4, normal E1/E2, normal E3/E4, high E1/E2, high E3/E4.
    let row: [&str; 6] = match (crime, ra_60_or_more) {
        (CrimeRate::Low, false) => ["S5", "S4", "S4", "S3", "S3", "S2"],
        (CrimeRate::Low, true) => ["S6", "S5", "S5", "S4", "S4", "S3"],
        (CrimeRate::Moderate, false) => ["S4", "S3", "S3", "S2", "", "S1"],
        (CrimeRate::Moderate, true) => ["S5", "S4", "S4", "S3", "", "S2"],
        (CrimeRate::High, false) => ["S2", "S2", "S2", "S1", "", "S1"],
        (CrimeRate::High, true) => ["S3", "S3", "S3", "S2", "", "S2"],
    };
    let column = 2 * match flow {
        TrafficFlow::Low => 0,
        TrafficFlow::Normal => 1,
        TrafficFlow::High => 2,
    } + usize::from(matches!(zone, Zone::E3 | Zone::E4));
    Some(row[column]).filter(|c| !c.is_empty())
}

/// Type of traffic of Table B.5.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TownCentreTraffic {
    PedestrianOnly,
    MixedSeparateFootways,
    MixedSameSurface,
}

/// BS 5489-1:2003 Table B.5 "Lighting classes for city and town centres". The table covers
/// normal and high traffic flow in zones E3 and E4 only; `None` otherwise.
pub fn town_centre_class(traffic: TownCentreTraffic, flow: TrafficFlow, zone: Zone) -> Option<&'static str> {
    // Columns: normal E3, normal E4, high E3, high E4.
    let row: [&str; 4] = match traffic {
        TownCentreTraffic::PedestrianOnly => ["CE3", "CE2", "CE2", "CE1"],
        TownCentreTraffic::MixedSeparateFootways | TownCentreTraffic::MixedSameSurface => ["CE2", "CE1", "CE1", "CE1"],
    };
    let f = match flow {
        TrafficFlow::Normal => 0,
        TrafficFlow::High => 2,
        TrafficFlow::Low => return None,
    };
    let z = match zone {
        Zone::E3 => 0,
        Zone::E4 => 1,
        Zone::E1 | Zone::E2 => return None,
    };
    Some(row[f + z])
}

/// Minimum ingress protection of the lamp housing, the column groups of Table D.1.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum IpMinimum {
    Ip2x,
    Ip5x,
    Ip6x,
}

/// Pollution category of Table D.1: high in the centre of large urban areas and heavy
/// industrial areas, medium in semi-urban, residential and light industrial areas, low in rural
/// areas.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Pollution {
    High,
    Medium,
    Low,
}

/// BS 5489-1:2003 Table D.1 "Luminaire maintenance factors" (Annex D, informative: "typical
/// luminaire maintenance factors, which may be used in design calculations"). `None` for a
/// cleaning interval that is not a row of the table (12, 18, 24, 36 months): not interpolated.
///
/// This is the luminaire maintenance factor only. By clause 6.4.4 note 1 the maintenance factor
/// is its product with the lamp flux maintenance factor, which comes from the manufacturer.
pub fn luminaire_maintenance_factor(ip: IpMinimum, pollution: Pollution, cleaning_interval_months: u32) -> Option<f64> {
    // Columns: IP2X high, medium, low; IP5X high, medium, low; IP6X high, medium, low.
    let row: [f64; 9] = match cleaning_interval_months {
        12 => [0.53, 0.62, 0.82, 0.89, 0.90, 0.92, 0.91, 0.92, 0.93],
        18 => [0.48, 0.58, 0.80, 0.87, 0.88, 0.91, 0.90, 0.91, 0.92],
        24 => [0.45, 0.56, 0.79, 0.84, 0.86, 0.90, 0.88, 0.89, 0.91],
        36 => [0.42, 0.53, 0.78, 0.76, 0.82, 0.88, 0.83, 0.87, 0.90],
        _ => return None,
    };
    let i = match ip {
        IpMinimum::Ip2x => 0,
        IpMinimum::Ip5x => 3,
        IpMinimum::Ip6x => 6,
    };
    let p = match pollution {
        Pollution::High => 0,
        Pollution::Medium => 1,
        Pollution::Low => 2,
    };
    Some(row[i + p])
}

#[cfg(test)]
mod tests {
    use super::*;
    use ClassSelection::*;

    #[test]
    fn table_2_clearances() {
        // BS 5489-1:2003 Table 2.
        assert_eq!(min_clearance_m(50.0), Some(0.8));
        assert_eq!(min_clearance_m(80.0), Some(1.0));
        assert_eq!(min_clearance_m(100.0), Some(1.5));
        assert_eq!(min_clearance_m(120.0), Some(1.5));
        assert_eq!(min_clearance_m(60.0), None);
        assert_eq!(min_clearance_m(30.0), None);
        assert_eq!((VERTICAL_CLEARANCE_OVER_CARRIAGEWAY_M, VERTICAL_CLEARANCE_PEDESTRIAN_M, BRACKET_PROJECTION_MAX_FRACTION_OF_HEIGHT), (5.7, 2.1, 0.25));
    }

    #[test]
    fn table_b2_traffic_routes() {
        use RoadType::*;
        // Every row of BS 5489-1:2003 Table B.2, at and either side of each flow boundary.
        let rows: [(RoadType, f64, &[&str]); 30] = [
            (MotorwayComplexInterchange, 40_000.0, &["ME1"]),
            (MotorwayComplexInterchange, 40_001.0, &["ME1"]),
            (MotorwayInterchangesUnder3Km, 40_000.0, &["ME2"]),
            (MotorwayInterchangesUnder3Km, 40_001.0, &["ME1"]),
            (MotorwayInterchanges3KmOrMore, 40_000.0, &["ME2"]),
            (MotorwayInterchanges3KmOrMore, 40_001.0, &["ME2"]),
            (StrategicSingle, 14_999.0, &["ME3a"]),
            (StrategicSingle, 15_001.0, &["ME2"]),
            (StrategicDual, 14_999.0, &["ME3a"]),
            (StrategicDual, 15_001.0, &["ME2"]),
            (MainDistributorSingle, 15_000.0, &["ME3a"]),
            (MainDistributorSingle, 15_001.0, &["ME2"]),
            (MainDistributorDual, 15_000.0, &["ME3a"]),
            (MainDistributorDual, 15_001.0, &["ME2"]),
            (SecondaryDistributorRural, 7_000.0, &["ME4a"]),
            (SecondaryDistributorRural, 7_001.0, &["ME3b"]),
            (SecondaryDistributorRural, 15_000.0, &["ME3b"]),
            (SecondaryDistributorRural, 15_001.0, &["ME3a"]),
            (SecondaryDistributorUrban, 7_000.0, &["ME3c"]),
            (SecondaryDistributorUrban, 7_001.0, &["ME3b"]),
            (SecondaryDistributorUrban, 15_000.0, &["ME3b"]),
            (SecondaryDistributorUrban, 15_001.0, &["ME2"]),
            (LinkRoadRural, 500.0, &["ME5"]),
            (LinkRoadUrban, 500.0, &["ME4b", "S2"]),
            (LinkRoadUrbanHighPedestrian, 500.0, &["S1"]),
            (MotorwayEmergencyLane, 500.0, &["ME4a"]),
            (MotorwayInterchangesUnder3Km, 0.0, &["ME2"]),
            (SecondaryDistributorRural, 0.0, &["ME4a"]),
            (StrategicSingle, 0.0, &["ME3a"]),
            (MainDistributorDual, 90_000.0, &["ME2"]),
        ];
        for (road, adt, classes) in rows {
            assert_eq!(traffic_route_class(road, Some(adt)), Classes(classes), "{road:?} {adt}");
        }
        // The table's own gap, and rows that need a flow or do not.
        assert_eq!(traffic_route_class(StrategicSingle, Some(15_000.0)), NotCovered);
        assert_eq!(traffic_route_class(StrategicDual, None), FlowNeeded);
        assert_eq!(traffic_route_class(SecondaryDistributorUrban, Some(f64::NAN)), FlowNeeded);
        assert_eq!(traffic_route_class(LinkRoadRural, None), Classes(&["ME5"]));
        assert_eq!(traffic_route_class(MotorwayEmergencyLane, None), Classes(&["ME4a"]));
    }

    #[test]
    fn tables_b1_and_b3() {
        assert_eq!(conflict_area_class("ME1"), Some("CE0"));
        assert_eq!(conflict_area_class("ME2"), Some("CE1"));
        assert_eq!(conflict_area_class(" me3a "), Some("CE2"));
        assert_eq!(conflict_area_class("ME4b"), Some("CE3"));
        assert_eq!(conflict_area_class("ME5"), Some("CE4"));
        assert_eq!(conflict_area_class("ME6"), None);
        assert_eq!(conflict_area_class("M3"), None);
        assert_eq!(conflict_area_class("S2"), None);
        assert_eq!(TABLE_B1_COMPARABLE[3], ("ME3", "CE3", "S1"));
        assert_eq!(TABLE_B1_COMPARABLE[0], ("", "CE0", ""));
        assert_eq!(TABLE_B1_COMPARABLE[6], ("ME6", "", "S4"));
        assert_eq!(TABLE_B1_COMPARABLE[8], ("", "", "S6"));
    }

    #[test]
    fn table_b4_subsidiary_roads() {
        use CrimeRate::*;
        // Each row of Table B.4 in the order of its six columns; "" is a dash.
        let table: [(CrimeRate, bool, [&str; 6]); 6] = [
            (Low, false, ["S5", "S4", "S4", "S3", "S3", "S2"]),
            (Low, true, ["S6", "S5", "S5", "S4", "S4", "S3"]),
            (Moderate, false, ["S4", "S3", "S3", "S2", "", "S1"]),
            (Moderate, true, ["S5", "S4", "S4", "S3", "", "S2"]),
            (High, false, ["S2", "S2", "S2", "S1", "", "S1"]),
            (High, true, ["S3", "S3", "S3", "S2", "", "S2"]),
        ];
        let columns = [
            (TrafficFlow::Low, Zone::E1),
            (TrafficFlow::Low, Zone::E3),
            (TrafficFlow::Normal, Zone::E2),
            (TrafficFlow::Normal, Zone::E4),
            (TrafficFlow::High, Zone::E1),
            (TrafficFlow::High, Zone::E4),
        ];
        for (crime, ra, row) in table {
            for ((flow, zone), want) in columns.iter().zip(row) {
                assert_eq!(subsidiary_road_class(crime, ra, *flow, *zone), Some(want).filter(|c| !c.is_empty()), "{crime:?} {ra} {flow:?} {zone:?}");
            }
        }
        // E1 and E2 share a column, as do E3 and E4.
        assert_eq!(subsidiary_road_class(Low, false, TrafficFlow::Low, Zone::E2), Some("S5"));
        assert_eq!(subsidiary_road_class(Low, false, TrafficFlow::Low, Zone::E4), Some("S4"));
    }

    #[test]
    fn table_b5_town_centres() {
        use TownCentreTraffic::*;
        let columns = [(TrafficFlow::Normal, Zone::E3), (TrafficFlow::Normal, Zone::E4), (TrafficFlow::High, Zone::E3), (TrafficFlow::High, Zone::E4)];
        for (traffic, row) in [
            (PedestrianOnly, ["CE3", "CE2", "CE2", "CE1"]),
            (MixedSeparateFootways, ["CE2", "CE1", "CE1", "CE1"]),
            (MixedSameSurface, ["CE2", "CE1", "CE1", "CE1"]),
        ] {
            for ((flow, zone), want) in columns.iter().zip(row) {
                assert_eq!(town_centre_class(traffic, *flow, *zone), Some(want));
            }
        }
        assert_eq!(town_centre_class(PedestrianOnly, TrafficFlow::Low, Zone::E3), None);
        assert_eq!(town_centre_class(PedestrianOnly, TrafficFlow::High, Zone::E2), None);
    }

    #[test]
    fn table_d1_luminaire_maintenance_factors() {
        use IpMinimum::*;
        use Pollution::*;
        // Table D.1 row by row: IP2X high, medium, low; IP5X; IP6X.
        let table: [(u32, [f64; 9]); 4] = [
            (12, [0.53, 0.62, 0.82, 0.89, 0.90, 0.92, 0.91, 0.92, 0.93]),
            (18, [0.48, 0.58, 0.80, 0.87, 0.88, 0.91, 0.90, 0.91, 0.92]),
            (24, [0.45, 0.56, 0.79, 0.84, 0.86, 0.90, 0.88, 0.89, 0.91]),
            (36, [0.42, 0.53, 0.78, 0.76, 0.82, 0.88, 0.83, 0.87, 0.90]),
        ];
        for (months, row) in table {
            let mut k = 0;
            for ip in [Ip2x, Ip5x, Ip6x] {
                for pollution in [High, Medium, Low] {
                    assert_eq!(luminaire_maintenance_factor(ip, pollution, months), Some(row[k]));
                    k += 1;
                }
            }
            // Cleaner air and better sealing never give a lower factor.
            assert!(row[0] <= row[1] && row[1] <= row[2] && row[0] <= row[3] && row[3] <= row[6]);
        }
        assert_eq!(luminaire_maintenance_factor(Ip6x, Low, 48), None);
        assert_eq!(luminaire_maintenance_factor(Ip6x, Low, 30), None);
    }
}
