// SPDX-License-Identifier: LicenseRef-AfroBIM-Proprietary
// Copyright (c) 2026 AfroBIM. All rights reserved.
//! Column positions from an arrangement, a spacing and an alignment.
//!
//! The rules are this crate's own and are stated here because TD 501 Version 0.1.0 gives none
//! (its only words on geometry are clause 4.4, that the geometry of adjacent installations shall
//! be adopted for consistent maintenance access):
//!
//! - The first column stands at the start station.
//! - Columns follow at the spacing, measured along the alignment (by station).
//! - The end is treated as the installation's [`EndTreatment`] says.
//! - A column stands square to the alignment at its station, at the offset given by
//!   [`Installation::column_offset_m`]; its bracket points square to the alignment towards the
//!   carriageway.
//! - The level is the alignment's profile level at the station (the centreline level, with no
//!   cross-fall or verge slope applied), or `None` when the alignment has no profile there.

use std::f64::consts::{FRAC_PI_2, TAU};

use afrobim_alignment::Alignment;
use serde::{Deserialize, Serialize};

use crate::model::{Arrangement, BracketAim, EndTreatment, Installation, SingleColumn};
use crate::LightingError;

/// The side of the reference line a column stands on, looking up-station.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum Side {
    Left,
    Right,
    Central,
}

impl Side {
    pub fn label(self) -> &'static str {
        match self {
            Side::Left => "left",
            Side::Right => "right",
            Side::Central => "central",
        }
    }
}

/// One column on site.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ColumnPosition {
    pub mark: String,
    /// Internal station, m.
    pub station_m: f64,
    /// The station as displayed, through the alignment's station equations, e.g. `1+250.000`.
    pub station_label: String,
    pub side: Side,
    /// Offset of the column axis from the alignment, m, positive to the right.
    pub offset_m: f64,
    pub easting_m: f64,
    pub northing_m: f64,
    /// Profile level of the alignment at the station, m; `None` without a profile there.
    pub level_m: Option<f64>,
    /// Heading of the alignment at the station, radians counter-clockwise from +X.
    pub heading_rad: f64,
    /// Direction of each bracket, from the column towards its luminaire, radians
    /// counter-clockwise from +X. One entry per luminaire.
    pub bracket_directions_rad: Vec<f64>,
}

impl ColumnPosition {
    /// Whole-circle bearing of the first bracket, degrees clockwise from north: the bearing from
    /// the column towards the carriageway.
    pub fn bearing_deg(&self) -> f64 {
        whole_circle_bearing_deg(self.bracket_directions_rad.first().copied().unwrap_or(self.heading_rad))
    }
}

/// A direction in radians counter-clockwise from +X (east) as a whole-circle bearing in degrees
/// clockwise from north, in [0, 360).
pub fn whole_circle_bearing_deg(direction_rad: f64) -> f64 {
    let b = (90.0 - direction_rad.to_degrees()).rem_euclid(360.0);
    if b >= 360.0 {
        0.0
    } else {
        b
    }
}

/// The positions of an element and how the length was divided.
#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
pub struct Layout {
    /// The alignment the positions were computed on.
    pub alignment: String,
    pub columns: Vec<ColumnPosition>,
    /// Number of bays between consecutive column stations.
    pub bays: usize,
    /// Distance along the alignment between consecutive column stations, m, other than a short
    /// last bay. Equal to the entered spacing except with [`EndTreatment::EqualiseSpacing`].
    pub actual_spacing_m: f64,
    /// Length of the last bay, m. Shorter than the spacing with [`EndTreatment::ColumnAtEnd`]
    /// when the length is not a whole number of spacings.
    pub last_bay_m: f64,
    /// Distance from the last column to the end station, m. Not zero only with
    /// [`EndTreatment::KeepSpacing`].
    pub remainder_m: f64,
}

/// Stations closer than this are the same station, m.
const STATION_TOLERANCE_M: f64 = 1e-6;

fn locate(alignment: &Alignment, station_m: f64, offset_m: f64) -> Result<(f64, f64, f64, Option<f64>), LightingError> {
    // A station a rounding error past the end is the end.
    let (start, end) = (alignment.start_station_m, alignment.end_station_m());
    let s = if (station_m - end).abs() < STATION_TOLERANCE_M {
        end
    } else if (station_m - start).abs() < STATION_TOLERANCE_M {
        start
    } else {
        station_m
    };
    let p = alignment.point_at(s).ok_or_else(|| LightingError::StationOffAlignment {
        alignment: alignment.name.clone(),
        station_m,
        start_m: start,
        end_m: end,
    })?;
    let (e, n) = p.offset(offset_m);
    let level = alignment.profile.as_ref().and_then(|profile| profile.elevation_at(s).ok());
    Ok((e, n, p.heading_rad, level))
}

/// The column stations of an installation and how the length was divided:
/// (stations, actual spacing, last bay, remainder).
fn stations(def: &Installation) -> (Vec<f64>, f64, f64, f64) {
    let length = def.end_station_m - def.start_station_m;
    let s = def.spacing_m;
    match def.end_treatment {
        EndTreatment::KeepSpacing | EndTreatment::ColumnAtEnd => {
            let whole = ((length + STATION_TOLERANCE_M) / s).floor() as usize;
            let mut out: Vec<f64> = (0..=whole).map(|k| def.start_station_m + k as f64 * s).collect();
            let remainder = (length - whole as f64 * s).max(0.0);
            let remainder = if remainder < STATION_TOLERANCE_M { 0.0 } else { remainder };
            if def.end_treatment == EndTreatment::ColumnAtEnd && remainder > 0.0 {
                out.push(def.end_station_m);
                (out, s, remainder, 0.0)
            } else {
                (out, s, if whole > 0 { s } else { 0.0 }, remainder)
            }
        }
        EndTreatment::EqualiseSpacing => {
            let bays = (((length - STATION_TOLERANCE_M) / s).ceil() as usize).max(1);
            let actual = length / bays as f64;
            let mut out: Vec<f64> = (0..bays).map(|k| def.start_station_m + k as f64 * actual).collect();
            out.push(def.end_station_m);
            (out, actual, actual, 0.0)
        }
    }
}

/// Lays an installation out on its alignment. See the module documentation for the rules.
pub fn layout_installation(alignment: &Alignment, def: &Installation) -> Result<Layout, LightingError> {
    def.check()?;
    let (stations, actual_spacing_m, last_bay_m, remainder_m) = stations(def);
    let mut columns = Vec::new();
    let mut number = def.first_number;
    for (k, &station) in stations.iter().enumerate() {
        let sides: &[Side] = match def.arrangement {
            Arrangement::SingleLeft => &[Side::Left],
            Arrangement::SingleRight => &[Side::Right],
            Arrangement::Opposite => &[Side::Left, Side::Right],
            Arrangement::TwinCentral => &[Side::Central],
            Arrangement::Staggered => {
                let first_left = def.first_side == Side::Left;
                if (k % 2 == 0) == first_left {
                    &[Side::Left]
                } else {
                    &[Side::Right]
                }
            }
        };
        for &side in sides {
            let offset_m = def.column_offset_m(side);
            let (easting_m, northing_m, heading_rad, level_m) = locate(alignment, station, offset_m)?;
            // Left of the heading is heading + 90°. A column on the right has the carriageway on
            // its left; one on the left has it on its right.
            let bracket_directions_rad = match side {
                Side::Right => vec![(heading_rad + FRAC_PI_2).rem_euclid(TAU)],
                Side::Left => vec![(heading_rad - FRAC_PI_2).rem_euclid(TAU)],
                Side::Central => vec![(heading_rad - FRAC_PI_2).rem_euclid(TAU), (heading_rad + FRAC_PI_2).rem_euclid(TAU)],
            };
            columns.push(ColumnPosition {
                mark: format!("{}-{:03}", def.mark_prefix, number),
                station_m: station,
                station_label: alignment.station_label(station, 3),
                side,
                offset_m,
                easting_m,
                northing_m,
                level_m,
                heading_rad,
                bracket_directions_rad,
            });
            number += 1;
        }
    }
    Ok(Layout { alignment: alignment.name.clone(), columns, bays: stations.len() - 1, actual_spacing_m, last_bay_m, remainder_m })
}

/// Positions a single column on its alignment.
pub fn layout_single_column(alignment: &Alignment, def: &SingleColumn) -> Result<Layout, LightingError> {
    def.check()?;
    let (easting_m, northing_m, heading_rad, level_m) = locate(alignment, def.station_m, def.offset_m)?;
    let bracket_directions_rad = match def.aim {
        BracketAim::Bearing(b) => vec![(90.0 - b).to_radians().rem_euclid(TAU)],
        BracketAim::TowardAlignment if def.offset_m > 0.0 => vec![(heading_rad + FRAC_PI_2).rem_euclid(TAU)],
        BracketAim::TowardAlignment if def.offset_m < 0.0 => vec![(heading_rad - FRAC_PI_2).rem_euclid(TAU)],
        BracketAim::TowardAlignment => vec![(heading_rad - FRAC_PI_2).rem_euclid(TAU), (heading_rad + FRAC_PI_2).rem_euclid(TAU)],
    };
    let side = if def.offset_m > 0.0 {
        Side::Right
    } else if def.offset_m < 0.0 {
        Side::Left
    } else {
        Side::Central
    };
    let column = ColumnPosition {
        mark: def.mark.clone(),
        station_m: def.station_m,
        station_label: alignment.station_label(def.station_m, 3),
        side,
        offset_m: def.offset_m,
        easting_m,
        northing_m,
        level_m,
        heading_rad,
        bracket_directions_rad,
    };
    Ok(Layout { alignment: alignment.name.clone(), columns: vec![column], bays: 0, actual_spacing_m: 0.0, last_bay_m: 0.0, remainder_m: 0.0 })
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;
    use crate::model::{ColumnSpec, Luminaire};
    use afrobim_alignment::{AlignmentBuilder, Profile, Pvi, StationEquation, StationEquations, Turn};

    pub(crate) fn spec() -> ColumnSpec {
        ColumnSpec {
            column_type: "test column".to_string(),
            mounting_height_m: 10.0,
            bracket_outreach_m: 1.5,
            tilt_deg: 0.0,
            luminaire: Luminaire { product: "Test LED 100".to_string(), lumen_output_lm: 12000.0, power_w: 100.0, ..Default::default() },
        }
    }

    pub(crate) fn installation(arrangement: Arrangement, start: f64, end: f64, spacing: f64) -> Installation {
        Installation {
            reference: "Z1".to_string(),
            alignment: "A".to_string(),
            start_station_m: start,
            end_station_m: end,
            arrangement,
            spacing_m: spacing,
            end_treatment: EndTreatment::KeepSpacing,
            first_side: Side::Left,
            carriageway_width_m: 7.0,
            reference_offset_m: 0.0,
            setback_m: 1.5,
            column: spec(),
            lighting_class: "M3".to_string(),
            mark_prefix: "LC".to_string(),
            first_number: 1,
        }
    }

    /// 300 m due east from (1000, 2000), stations from 0.
    pub(crate) fn straight() -> Alignment {
        AlignmentBuilder::new("A", 0.0, 1000.0, 2000.0, 0.0).line(300.0).build().unwrap()
    }

    fn close(a: f64, b: f64) -> bool {
        (a - b).abs() < 1e-9
    }

    #[test]
    fn single_sided_on_a_straight() {
        let a = straight();
        // 300 m at 40 m: columns at 0, 40, ..., 280 (8), 20 m left over.
        let l = layout_installation(&a, &installation(Arrangement::SingleRight, 0.0, 300.0, 40.0)).unwrap();
        assert_eq!(l.columns.len(), 8);
        assert_eq!((l.bays, l.actual_spacing_m, l.last_bay_m), (7, 40.0, 40.0));
        assert!(close(l.remainder_m, 20.0));
        for (k, c) in l.columns.iter().enumerate() {
            // Heading east: the right is south. Offset 3.5 + 1.5 = 5 m.
            assert!(close(c.station_m, 40.0 * k as f64));
            assert!(close(c.easting_m, 1000.0 + 40.0 * k as f64) && close(c.northing_m, 1995.0));
            assert_eq!((c.side, c.offset_m), (Side::Right, 5.0));
            // The carriageway is to the north of a column on the south side.
            assert!(close(c.bearing_deg(), 0.0));
            assert_eq!(c.level_m, None);
        }
        assert_eq!(l.columns[0].mark, "LC-001");
        assert_eq!(l.columns[7].mark, "LC-008");
        assert_eq!(l.columns[1].station_label, "0+040.000");

        let l = layout_installation(&a, &installation(Arrangement::SingleLeft, 0.0, 300.0, 40.0)).unwrap();
        assert_eq!(l.columns.len(), 8);
        assert!(l.columns.iter().all(|c| close(c.northing_m, 2005.0) && close(c.bearing_deg(), 180.0) && c.offset_m == -5.0));
    }

    #[test]
    fn staggered_opposite_and_twin_central_on_a_straight() {
        let a = straight();
        let l = layout_installation(&a, &installation(Arrangement::Staggered, 0.0, 300.0, 30.0)).unwrap();
        // 0, 30, ..., 300: 11 columns, alternately left and right starting left.
        assert_eq!(l.columns.len(), 11);
        assert_eq!(l.remainder_m, 0.0);
        for (k, c) in l.columns.iter().enumerate() {
            let left = k % 2 == 0;
            assert_eq!(c.side, if left { Side::Left } else { Side::Right });
            assert!(close(c.easting_m, 1000.0 + 30.0 * k as f64));
            assert!(close(c.northing_m, if left { 2005.0 } else { 1995.0 }));
        }
        // Same-side columns are two spacings apart.
        assert!(close(l.columns[2].easting_m - l.columns[0].easting_m, 60.0));
        let mut right_first = installation(Arrangement::Staggered, 0.0, 300.0, 30.0);
        right_first.first_side = Side::Right;
        assert_eq!(layout_installation(&a, &right_first).unwrap().columns[0].side, Side::Right);

        let l = layout_installation(&a, &installation(Arrangement::Opposite, 0.0, 300.0, 50.0)).unwrap();
        // 7 stations, two columns each, 10 m apart across the road.
        assert_eq!(l.columns.len(), 14);
        for pair in l.columns.chunks(2) {
            assert_eq!((pair[0].side, pair[1].side), (Side::Left, Side::Right));
            assert!(close(pair[0].easting_m, pair[1].easting_m) && close(pair[0].northing_m - pair[1].northing_m, 10.0));
        }
        assert_eq!(l.columns[13].mark, "LC-014");

        let l = layout_installation(&a, &installation(Arrangement::TwinCentral, 0.0, 300.0, 50.0)).unwrap();
        assert_eq!(l.columns.len(), 7);
        for c in &l.columns {
            assert_eq!((c.side, c.offset_m), (Side::Central, 0.0));
            assert!(close(c.northing_m, 2000.0));
            // One luminaire to the right-hand carriageway (south) and one to the left (north).
            assert_eq!(c.bracket_directions_rad.len(), 2);
            assert!(close(whole_circle_bearing_deg(c.bracket_directions_rad[0]), 180.0));
            assert!(close(whole_circle_bearing_deg(c.bracket_directions_rad[1]), 0.0));
        }
    }

    #[test]
    fn end_treatments() {
        let a = straight();
        let mut def = installation(Arrangement::SingleRight, 10.0, 290.0, 40.0);
        // 280 m: 7 whole bays, no remainder, whatever the treatment.
        for t in [EndTreatment::KeepSpacing, EndTreatment::ColumnAtEnd, EndTreatment::EqualiseSpacing] {
            def.end_treatment = t;
            let l = layout_installation(&a, &def).unwrap();
            assert_eq!((l.columns.len(), l.bays), (8, 7));
            assert!(close(l.actual_spacing_m, 40.0) && close(l.last_bay_m, 40.0) && l.remainder_m == 0.0);
            assert!(close(l.columns[7].station_m, 290.0));
        }
        // 250 m at 40 m: 6 whole bays and 10 m over.
        def.end_station_m = 260.0;
        def.end_treatment = EndTreatment::KeepSpacing;
        let l = layout_installation(&a, &def).unwrap();
        assert_eq!(l.columns.len(), 7);
        assert!(close(l.columns[6].station_m, 250.0) && close(l.remainder_m, 10.0) && close(l.last_bay_m, 40.0));
        def.end_treatment = EndTreatment::ColumnAtEnd;
        let l = layout_installation(&a, &def).unwrap();
        assert_eq!(l.columns.len(), 8);
        assert!(close(l.columns[7].station_m, 260.0) && close(l.last_bay_m, 10.0) && l.remainder_m == 0.0);
        def.end_treatment = EndTreatment::EqualiseSpacing;
        let l = layout_installation(&a, &def).unwrap();
        // ceil(250 / 40) = 7 bays of 250/7 m.
        assert_eq!((l.columns.len(), l.bays), (8, 7));
        assert!(close(l.actual_spacing_m, 250.0 / 7.0) && close(l.columns[7].station_m, 260.0));
        assert!(close(l.columns[3].station_m, 10.0 + 3.0 * 250.0 / 7.0));
        // Shorter than one spacing.
        def.end_station_m = 30.0;
        def.end_treatment = EndTreatment::KeepSpacing;
        let l = layout_installation(&a, &def).unwrap();
        assert_eq!((l.columns.len(), l.bays), (1, 0));
        assert!(close(l.remainder_m, 20.0));
    }

    #[test]
    fn columns_inside_a_curve_are_closer_together() {
        // A left-hand curve of radius 200 m starting at the origin heading east: centre (0, 200).
        let r = 200.0;
        let a = AlignmentBuilder::new("A", 0.0, 0.0, 0.0, 0.0).arc(r, 200.0, Turn::Left).build().unwrap();
        let s = 40.0;
        let l = layout_installation(&a, &installation(Arrangement::Opposite, 0.0, 200.0, s)).unwrap();
        assert_eq!(l.columns.len(), 12);
        let o = 5.0;
        for (k, pair) in l.columns.chunks(2).enumerate() {
            let phi = k as f64 * s / r;
            let (left, right) = (&pair[0], &pair[1]);
            // The left is the inside: radius R - o; the right the outside: R + o.
            assert!(close(left.easting_m, (r - o) * phi.sin()) && close(left.northing_m, r - (r - o) * phi.cos()));
            assert!(close(right.easting_m, (r + o) * phi.sin()) && close(right.northing_m, r - (r + o) * phi.cos()));
            assert!(close(left.heading_rad, phi));
            // Brackets point at each other across the road.
            let across = (left.bracket_directions_rad[0] - right.bracket_directions_rad[0]).rem_euclid(TAU);
            assert!(close(across, std::f64::consts::PI));
        }
        let chord = |a: &ColumnPosition, b: &ColumnPosition| ((a.easting_m - b.easting_m).powi(2) + (a.northing_m - b.northing_m).powi(2)).sqrt();
        let inside = chord(&l.columns[0], &l.columns[2]);
        let outside = chord(&l.columns[1], &l.columns[3]);
        assert!(close(inside, 2.0 * (r - o) * (s / (2.0 * r)).sin()));
        assert!(close(outside, 2.0 * (r + o) * (s / (2.0 * r)).sin()));
        // Along the column lines the bays are S(1 - o/R) = 39 m and S(1 + o/R) = 41 m.
        assert!(inside < 39.0 && inside > 38.9 && outside < 41.0 && outside > 40.9);
    }

    #[test]
    fn levels_come_from_the_profile_and_labels_from_the_station_equations() {
        let mut a = straight();
        // 100.000 m at station 0 rising at 2 %.
        a.profile = Some(Profile::new("P", vec![Pvi::point(0.0, 100.0), Pvi::point(300.0, 106.0)]).unwrap());
        // From internal station 100 the displayed stations restart at 5000.
        a.station_equations = StationEquations::new(vec![StationEquation::new(100.0, 5000.0)]).unwrap();
        let l = layout_installation(&a, &installation(Arrangement::SingleRight, 0.0, 300.0, 50.0)).unwrap();
        assert!(close(l.columns[3].level_m.unwrap(), 103.0));
        assert_eq!(l.columns[1].station_label, a.station_label(50.0, 3));
        assert_eq!(l.columns[3].station_label, "5+050.000");
        assert_eq!(l.columns[3].station_m, 150.0);
    }

    #[test]
    fn single_columns() {
        let a = straight();
        let mut def = SingleColumn {
            mark: "J1".to_string(),
            alignment: "A".to_string(),
            station_m: 120.0,
            offset_m: -8.0,
            aim: BracketAim::TowardAlignment,
            column: spec(),
            lighting_class: "C2".to_string(),
        };
        let c = &layout_single_column(&a, &def).unwrap().columns[0];
        assert!(close(c.easting_m, 1120.0) && close(c.northing_m, 2008.0) && close(c.bearing_deg(), 180.0));
        assert_eq!((c.side, c.mark.as_str(), c.station_label.as_str()), (Side::Left, "J1", "0+120.000"));
        def.aim = BracketAim::Bearing(45.0);
        let c = &layout_single_column(&a, &def).unwrap().columns[0];
        assert!(close(c.bearing_deg(), 45.0) && close(c.bracket_directions_rad[0], std::f64::consts::FRAC_PI_4));
        def.aim = BracketAim::TowardAlignment;
        def.offset_m = 0.0;
        assert_eq!(layout_single_column(&a, &def).unwrap().columns[0].bracket_directions_rad.len(), 2);
        def.station_m = 400.0;
        assert!(matches!(layout_single_column(&a, &def), Err(LightingError::StationOffAlignment { .. })));
    }

    #[test]
    fn invalid_definitions_and_stations_off_the_alignment_are_refused() {
        let a = straight();
        let mut def = installation(Arrangement::SingleRight, 0.0, 300.0, 0.0);
        assert!(matches!(layout_installation(&a, &def), Err(LightingError::InvalidDefinition(_))));
        def.spacing_m = 40.0;
        def.end_station_m = 400.0;
        assert!(matches!(layout_installation(&a, &def), Err(LightingError::StationOffAlignment { .. })));
        def.end_station_m = 0.0;
        assert!(matches!(layout_installation(&a, &def), Err(LightingError::InvalidDefinition(_))));
    }

    #[test]
    fn bearings() {
        assert!(close(whole_circle_bearing_deg(0.0), 90.0));
        assert!(close(whole_circle_bearing_deg(FRAC_PI_2), 0.0));
        assert!(close(whole_circle_bearing_deg(std::f64::consts::PI), 270.0));
        assert!(close(whole_circle_bearing_deg(-FRAC_PI_2), 180.0));
    }
}
