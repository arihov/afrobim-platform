// SPDX-License-Identifier: LicenseRef-AfroBIM-Proprietary
// Copyright (c) 2026 AfroBIM. All rights reserved.
//! Values taken from the documents held in the repository (`Manuals/`), each with its citation.
//!
//! Every number in this module was read from the text or table named in its [`Citation`]; the
//! README lists what was searched for and not found. Nothing here is from memory. Where a table
//! is in miles per hour it stays in miles per hour: the choice of the band that a km/h limit
//! belongs to is the engineer's decision and is an input ([`SpeedBandMph`]), never made here.
//!
//! Page numbers are the printed page numbers of the held PDF, with the PDF page in brackets
//! where the two differ.

use std::fmt;

use serde::{Deserialize, Serialize};

/// Where a value comes from: document, edition, provision and page.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub struct Citation {
    pub document: &'static str,
    pub edition: &'static str,
    pub provision: &'static str,
    pub page: &'static str,
}

impl fmt::Display for Citation {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}, {}, {}, {}", self.document, self.edition, self.provision, self.page)
    }
}

const UK1: &str = "UK Traffic Signs Manual, Chapter 1 Introduction";
const UK1_ED: &str = "1982 edition (held as Manuals/tsm-chapter-01.pdf)";
const UK3: &str = "UK Traffic Signs Manual, Chapter 3 Regulatory Signs";
const UK3_ED: &str = "2008 edition (held as Manuals/tsm-chapter-03.pdf)";
const UK4: &str = "UK Traffic Signs Manual, Chapter 4 Warning Signs";
const UK4_ED: &str = "fourth edition 2013 (held as Manuals/tsm-chapter-04.pdf)";
const UK7: &str = "UK Traffic Signs Manual, Chapter 7 The Design of Traffic Signs";
const UK7_ED: &str = "2013 edition (held as Manuals/tsm-chapter-07.pdf)";
const UKX: &str = "UK Traffic Signs Manual, Chapter 2 Informatory Signs, 'Determination of x-height' (3-page extract)";
const UKX_ED: &str = "edition not stated in the extract (held as Manuals/tsm-chapter-02-x-height.pdf)";
const UG1: &str = "Uganda Traffic Signs Manual, Volume 1 (Ministry of Works, Housing and Communications)";
const UG1_ED: &str = "July 2004 (held as 'Manuals/Traffic Signs Manual Vol. 1  FINAL.pdf')";
const UG2: &str = "Uganda Traffic Signs Manual, Volume 2, Appendix 3 sign pages";
const UG2_ED: &str = "2004 (held as 'Manuals/Traffic Signs Manual Vol. 2 Complete.pdf')";

pub const UK_TABLE_1_1: Citation = Citation { document: UK3, edition: UK3_ED, provision: "Table 1-1 with paras 1.15 to 1.18", page: "p. 6 (PDF p. 9)" };
pub const UK_CH3_MEASURED_FROM: Citation = Citation { document: UK3, edition: UK3_ED, provision: "para 1.16", page: "p. 6 (PDF p. 9)" };
pub const UK_CH3_MOUNTING: Citation = Citation { document: UK3, edition: UK3_ED, provision: "para 1.21", page: "p. 7 (PDF p. 10)" };
pub const UK_TABLE_14_3: Citation = Citation { document: UK3, edition: UK3_ED, provision: "Table 14-3 and its notes 1 to 3", page: "p. 116 (PDF p. 119)" };
pub const UK_EYE_HEIGHT: Citation = Citation { document: UK3, edition: UK3_ED, provision: "para 3.10 (driver's eye position 1.05 m, stated for the STOP sign visibility criteria of Table 3-1)", page: "p. 12 (PDF p. 15)" };
pub const UK_CH4_APPENDIX_A: Citation = Citation { document: UK4, edition: UK4_ED, provision: "Appendix A table, columns 3 and 4, notes 1 to 5", page: "p. 73 (PDF p. 75)" };
pub const UK_CH4_SITING: Citation = Citation { document: UK4, edition: UK4_ED, provision: "paras 1.20 and 1.21", page: "pp. 5 to 6 (PDF pp. 7 to 8)" };
pub const UK_CH4_VISIBILITY: Citation = Citation { document: UK4, edition: UK4_ED, provision: "paras 1.22 and 1.23", page: "p. 6 (PDF p. 8)" };
pub const UK_CH4_MOUNTING: Citation = Citation { document: UK4, edition: UK4_ED, provision: "para 1.24", page: "p. 6 (PDF p. 8)" };
pub const UK_CH1_READING: Citation = Citation { document: UK1, edition: UK1_ED, provision: "para 1.31 (10 degree rule and reading time)", page: "pp. 12 to 13 (PDF pp. 15 to 16)" };
pub const UK_CH1_VISIBLE: Citation = Citation { document: UK1, edition: UK1_ED, provision: "paras 1.41 and 1.56", page: "pp. 14 and 15 (PDF pp. 17 and 18)" };
pub const UK_CH1_PLACEMENT: Citation = Citation { document: UK1, edition: UK1_ED, provision: "para 1.48", page: "p. 14 (PDF p. 17)" };
pub const UK_CH1_MOUNTING: Citation = Citation { document: UK1, edition: UK1_ED, provision: "para 1.49", page: "p. 14 (PDF p. 17)" };
pub const UK_CH1_ORIENTATION: Citation = Citation { document: UK1, edition: UK1_ED, provision: "paras 1.51 and 1.52", page: "pp. 14 to 15 (PDF pp. 17 to 18)" };
pub const UK_CH1_TOLERANCE: Citation = Citation { document: UK1, edition: UK1_ED, provision: "paras 1.54 and 1.55", page: "p. 15 (PDF p. 18)" };
pub const UK_CH7_OVERHEAD: Citation = Citation { document: UK7, edition: UK7_ED, provision: "para 9.7 (10 degree sight line to the lower part of an overhead sign)", page: "p. 89 (PDF p. 91)" };
pub const UK_X_HEIGHT: Citation = Citation { document: UKX, edition: UKX_ED, provision: "C = S x cot 10 deg, R = reading time x speed, reading time = 2 + N/3 s, x-height 100 mm at 60 m", page: "PDF p. 3" };
pub const UK_X_HEIGHT_EXAMPLE: Citation = Citation { document: UKX, edition: UKX_ED, provision: "worked example (single carriageway, 50 mph)", page: "PDF p. 3" };
pub const UK_X_HEIGHT_ITERATIVE: Citation = Citation {
    document: "Determination of x-height, variant with iterative worked example (no manual header)",
    edition: "undated (held as Manuals/Chapter2-x-height.pdf and Manuals/tsschapter2-x-height.pdf)",
    provision: "worked example (iterative, 50 mph, 6 destinations)",
    page: "PDF p. 1 to 3",
};
pub const UG_TABLE_D2: Citation = Citation { document: UG1, edition: UG1_ED, provision: "D.4.2 Siting, Table D.2 Visibility distance requirements", page: "p. 9" };
pub const UG_SPACING: Citation = Citation { document: UG1, edition: UG1_ED, provision: "D.4.2 Siting, second sign at least 0.6v beyond the first, minimum 30 m", page: "p. 9" };
pub const UG_MOUNTING: Citation = Citation { document: UG1, edition: UG1_ED, provision: "D.4.3.1 Mounting Height", page: "pp. 9 to 10" };
pub const UG_CLEARANCE: Citation = Citation { document: UG1, edition: UG1_ED, provision: "D.4.3.2 Clearance from the Edge of the Road", page: "p. 10" };
pub const UG_TABLE_D3: Citation = Citation { document: UG1, edition: UG1_ED, provision: "D.5.2 Siting and Sizes, Table D.3", page: "p. 13" };
pub const UG_TABLE_D5: Citation = Citation { document: UG1, edition: UG1_ED, provision: "D.7.2 Siting, Table D.5", page: "p. 22" };
pub const UG_TABLE_D6: Citation = Citation { document: UG1, edition: UG1_ED, provision: "D.7.3 Sizes and Design Principles, Table D.6", page: "p. 23" };
pub const UG_EYE_HEIGHT: Citation = Citation {
    document: UG2,
    edition: UG2_ED,
    provision: "sign page M02 Double continuous dividing line (visibility distance measured from an eye height of 1.05 m)",
    page: "PDF p. 161",
};

/// Metres in an international mile, exact by definition (not a standard's value). The x-height
/// extract uses 1609.3.
pub const METRES_PER_MILE: f64 = 1609.344;

/// 1 mph in km/h. Both tables are kept in their own units: this is used only to show a
/// conversion and to turn an entered km/h speed into m/s.
pub const KPH_PER_MPH: f64 = METRES_PER_MILE / 1000.0;

/// The driver's eye height above the carriageway, m, as stated in two held sources for other
/// criteria ([`UK_EYE_HEIGHT`], [`UG_EYE_HEIGHT`]). It is the default of the eye height input;
/// neither source states it for sign visibility, so it stays an input.
pub const EYE_HEIGHT_M: f64 = 1.05;

// ---------------------------------------------------------------------------------------------
// UK speed bands (mph)
// ---------------------------------------------------------------------------------------------

/// The 85th percentile speed bands of the UK tables, in mph. Which band a km/h speed belongs to
/// is the engineer's decision.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
pub enum SpeedBandMph {
    UpTo20,
    From21To30,
    From31To40,
    From41To50,
    From51To60,
    Over60,
}

impl SpeedBandMph {
    pub const ALL: [SpeedBandMph; 6] =
        [SpeedBandMph::UpTo20, SpeedBandMph::From21To30, SpeedBandMph::From31To40, SpeedBandMph::From41To50, SpeedBandMph::From51To60, SpeedBandMph::Over60];

    /// The row label as printed in the tables.
    pub fn label(self) -> &'static str {
        match self {
            SpeedBandMph::UpTo20 => "Up to 20 mph",
            SpeedBandMph::From21To30 => "21 to 30 mph",
            SpeedBandMph::From31To40 => "31 to 40 mph",
            SpeedBandMph::From41To50 => "41 to 50 mph",
            SpeedBandMph::From51To60 => "51 to 60 mph",
            SpeedBandMph::Over60 => "Over 60 mph",
        }
    }

    /// A convenience for callers that hold a speed in mph: the band whose printed range contains
    /// it, taking the upper figure of each band as inclusive (20, 30, 40, 50, 60). The tables
    /// print whole miles per hour, so a fractional speed is placed by the same rule. This is NOT
    /// a km/h to mph mapping; that decision is not made by this crate.
    pub fn from_mph(mph: f64) -> SpeedBandMph {
        match mph {
            m if m <= 20.0 => SpeedBandMph::UpTo20,
            m if m <= 30.0 => SpeedBandMph::From21To30,
            m if m <= 40.0 => SpeedBandMph::From31To40,
            m if m <= 50.0 => SpeedBandMph::From41To50,
            m if m <= 60.0 => SpeedBandMph::From51To60,
            _ => SpeedBandMph::Over60,
        }
    }
}

/// Table 1-1, minimum clear visibility distance in metres for a regulatory sign, with the
/// bracketed value where there is one (Over 60 mph: 105, bracket 120).
///
/// Paragraph 1.17: the bracketed 120 m is generally for dual carriageway roads and motorways with
/// a 70 mph speed limit, where the bracketed larger size of sign in Appendix A is used.
pub fn uk_table_1_1(band: SpeedBandMph) -> (f64, Option<f64>) {
    match band {
        SpeedBandMph::UpTo20 => (45.0, None),
        SpeedBandMph::From21To30 => (60.0, None),
        SpeedBandMph::From31To40 => (60.0, None),
        SpeedBandMph::From41To50 => (75.0, None),
        SpeedBandMph::From51To60 => (90.0, None),
        SpeedBandMph::Over60 => (105.0, Some(120.0)),
    }
}

/// One row of the Chapter 4 Appendix A table for warning signs.
#[derive(Debug, Clone, Copy, PartialEq, Serialize)]
pub struct UkWarningRow {
    /// Column 2, height of the triangle in mm.
    pub triangle_mm: f64,
    /// Column 2 bracketed figure (see notes 1 and 2 of the table).
    pub triangle_bracketed_mm: Option<f64>,
    /// Column 3, minimum clear visibility distance, m.
    pub min_clear_visibility_m: f64,
    /// Column 3 bracketed figure (note 2).
    pub min_clear_visibility_bracketed_m: Option<f64>,
    /// Column 4, distance of the sign from the hazard, m: lower and upper figure of the range
    /// (equal when a single figure is printed).
    pub siting_m: (f64, f64),
}

/// Chapter 4 Appendix A, the table of sizes of warning signs and siting details.
///
/// Note 2 of the table: for the 70 mph dual three- or four-lane motorways and all-purpose roads
/// with hard shoulders the largest figures are used (1500 mm, 120 m, 305 m); otherwise the 1200
/// mm size with the 105 m visibility distance and the siting distance of column 4. Note 5: where
/// a range is quoted, progressively larger figures are used for speeds towards the higher end of
/// the band.
pub fn uk_warning_row(band: SpeedBandMph) -> UkWarningRow {
    let row = |t: f64, tb: Option<f64>, v: f64, vb: Option<f64>, lo: f64, hi: f64| UkWarningRow {
        triangle_mm: t,
        triangle_bracketed_mm: tb,
        min_clear_visibility_m: v,
        min_clear_visibility_bracketed_m: vb,
        siting_m: (lo, hi),
    };
    match band {
        SpeedBandMph::UpTo20 => row(600.0, None, 45.0, None, 45.0, 45.0),
        SpeedBandMph::From21To30 => row(600.0, None, 60.0, None, 45.0, 45.0),
        SpeedBandMph::From31To40 => row(750.0, Some(600.0), 60.0, None, 45.0, 110.0),
        SpeedBandMph::From41To50 => row(900.0, Some(750.0), 75.0, None, 110.0, 180.0),
        SpeedBandMph::From51To60 => row(1200.0, Some(900.0), 90.0, None, 180.0, 245.0),
        SpeedBandMph::Over60 => row(1200.0, Some(1500.0), 105.0, Some(120.0), 245.0, 305.0),
    }
}

/// Table 14-3, minimum clear visibility distance in metres for terminal speed limit signs (other
/// than entering a side road), by the approach road speed limit and the speed limit on the sign,
/// both in mph. `None` where the table prints a dash (the pair does not occur).
pub fn uk_table_14_3(approach_mph: u32, sign_mph: u32) -> Option<f64> {
    let v = match (approach_mph, sign_mph) {
        (70, 30) => 165.0,
        (70, 40) => 140.0,
        (70, 50) => 110.0,
        (70, 60) => 75.0,
        (60, 30) => 115.0,
        (60, 40) => 95.0,
        (60, 50) => 65.0,
        (50, 30) => 75.0,
        (50, 40) => 50.0,
        (40, 30) => 40.0,
        (30, 20) => 30.0,
        _ => return None,
    };
    Some(v)
}

/// UK tolerance on siting distance: a sign may be sited within about 10 % of the recommended
/// distance (Chapter 4 para 1.20; Chapter 1 (1982) para 1.54, "variations in distance of up to
/// 10% are generally permissible").
pub const UK_SITING_TOLERANCE: f64 = 0.10;

/// Chapter 4 Appendix A note 5: braking distance is affected by gradients and siting distances
/// may need to be increased on steep hills, "an increase of 50% might be made for a 10% descent".
pub const UK_DESCENT_EXAMPLE: (f64, f64) = (0.10, 0.50);

// ---------------------------------------------------------------------------------------------
// UK mounting height and placement
// ---------------------------------------------------------------------------------------------

/// Normal mounting height of the lower edge above the carriageway alongside, mm: 900 to 1500
/// (Chapter 3 para 1.21, Chapter 4 para 1.24, Chapter 1 (1982) para 1.49).
pub const UK_MOUNTING_NORMAL_MM: (f64, f64) = (900.0, 1500.0);

/// Headroom over a footway or an area used by pedestrians, mm: 2300 recommended, 2100 absolute
/// minimum (Chapter 3 para 1.21, Chapter 4 para 1.24).
pub const UK_HEADROOM_MM: (f64, f64) = (2300.0, 2100.0);

/// Minimum distance from the edge of the carriageway to the nearest edge of a sign
/// (Chapter 1, 1982, para 1.48: "at least 450 mm", increased to 600 mm where there is a severe
/// camber or crossfall and where signs are mounted on the central reserve of a dual
/// carriageway, and "at least 1200 mm" on high-speed dual carriageway roads; where there is a
/// hardened verge, not less than 600 mm behind the edge of the hardening).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
pub enum UkClearanceCase {
    #[default]
    Normal,
    SevereCamberOrCentralReserve,
    HighSpeedDual,
    /// Behind a hardened verge: measured from the edge of the hardening.
    HardenedVerge,
}

impl UkClearanceCase {
    pub fn min_mm(self) -> f64 {
        match self {
            UkClearanceCase::Normal => 450.0,
            UkClearanceCase::SevereCamberOrCentralReserve => 600.0,
            UkClearanceCase::HighSpeedDual => 1200.0,
            UkClearanceCase::HardenedVerge => 600.0,
        }
    }
}

// ---------------------------------------------------------------------------------------------
// UK x-height and reading
// ---------------------------------------------------------------------------------------------

/// The angle, degrees, beyond which a driver must turn the head to read a sign: reading stops at
/// the point where the observation angle reaches it ([`UK_X_HEIGHT`], para 1.31(b) of Chapter 1).
pub const READING_STOP_ANGLE_DEG: f64 = 10.0;

/// The factor the extract prints for cot 10 degrees ("C = S x 5.7"); the other held copy prints
/// 5.6713 and its worked example uses 5.67.
pub const COT_10_AS_PRINTED_OFFICIAL: f64 = 5.7;

/// cot 10 degrees, exact.
pub fn cot_reading_stop_angle() -> f64 {
    1.0 / READING_STOP_ANGLE_DEG.to_radians().tan()
}

/// Reading time, seconds, for a sign with `words` words or destinations: 2 + N / 3.
pub fn reading_time_s(words: u32) -> f64 {
    2.0 + words as f64 / 3.0
}

/// The extract takes 4 s (N = 6) as the maximum desirable reading time and says the number of
/// destinations should not exceed six.
pub const MAX_WORDS_DESIRABLE: u32 = 6;

/// The x-height of the extract is proportional to the distance at which reading starts: 100 mm
/// at 60 m.
pub const X_HEIGHT_REFERENCE_MM: f64 = 100.0;
pub const X_HEIGHT_REFERENCE_DISTANCE_M: f64 = 60.0;

/// Distance travelled while reading, `R = reading time x speed`, m.
pub fn reading_distance_m(reading_time_s: f64, speed_m_s: f64) -> f64 {
    reading_time_s * speed_m_s
}

/// `C = S x cot 10 deg`: the distance from the sign at which reading must stop, m, for the
/// lateral distance `s_m` from the centre of the driving lane to the centre of the sign.
pub fn reading_stop_distance_m(s_m: f64, cot: f64) -> f64 {
    s_m * cot
}

/// The x-height (mm) the extract requires for reading to start at `distance_m` (R + C):
/// 100 mm per 60 m.
pub fn required_x_height_mm(distance_m: f64) -> f64 {
    X_HEIGHT_REFERENCE_MM / X_HEIGHT_REFERENCE_DISTANCE_M * distance_m
}

/// The distance (m) from which a legend of the given x-height becomes readable by the same
/// proportion (60 m per 100 mm).
pub fn legibility_distance_m(x_height_mm: f64) -> f64 {
    x_height_mm * X_HEIGHT_REFERENCE_DISTANCE_M / X_HEIGHT_REFERENCE_MM
}

/// The sight line below the horizontal, degrees, that fixes the last point at which the lower
/// part of an overhead sign can be read ([`UK_CH7_OVERHEAD`]).
pub const OVERHEAD_SIGHT_LINE_DEG: f64 = 10.0;

/// The angle of the sign's horizontal axis to the general alignment of the left side edge of
/// carriageway on a straight length of road, degrees, "to eliminate or minimise" specular
/// reflection in rural areas ([`UK_CH1_ORIENTATION`], para 1.51). The sense in which the 5
/// degrees are turned is shown in the manual's figure 1.1, which was not read; only the angle is
/// reported.
pub const UK_ORIENTATION_DEG: f64 = 95.0;

// ---------------------------------------------------------------------------------------------
// Uganda (km/h)
// ---------------------------------------------------------------------------------------------

/// The traffic speed classes of Uganda Vol. 1 (Tables D.1, D.2, D.3, D.5, D.6). "Traffic speed"
/// is the 85th percentile speed of motor vehicle traffic (Vol. 1 B.3.1).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum UgSpeedClass {
    UpTo50,
    Over50To80,
    Over80,
}

impl UgSpeedClass {
    pub fn from_kph(kph: f64) -> UgSpeedClass {
        if kph <= 50.0 {
            UgSpeedClass::UpTo50
        } else if kph <= 80.0 {
            UgSpeedClass::Over50To80
        } else {
            UgSpeedClass::Over80
        }
    }
    pub fn label(self) -> &'static str {
        match self {
            UgSpeedClass::UpTo50 => "up to and including 50 km/h",
            UgSpeedClass::Over50To80 => "over 50 km/h and up to and including 80 km/h",
            UgSpeedClass::Over80 => "over 80 km/h",
        }
    }
    /// Table D.2, visibility distance to the sign, m.
    pub fn visibility_m(self) -> f64 {
        match self {
            UgSpeedClass::UpTo50 => 50.0,
            UgSpeedClass::Over50To80 => 150.0,
            UgSpeedClass::Over80 => 200.0,
        }
    }
    /// Table D.3, normal distance of a standard warning sign from the hazard, m.
    pub fn warning_siting_m(self) -> f64 {
        match self {
            UgSpeedClass::UpTo50 => 50.0,
            UgSpeedClass::Over50To80 => 100.0,
            UgSpeedClass::Over80 => 150.0,
        }
    }
    /// Table D.5, distance of an advance direction sign from the intersection, m.
    pub fn advance_direction_siting_m(self) -> f64 {
        match self {
            UgSpeedClass::UpTo50 => 50.0,
            UgSpeedClass::Over50To80 => 100.0,
            UgSpeedClass::Over80 => 150.0,
        }
    }
    /// Table D.6, x-height of lettering on guidance signs, mm.
    pub fn guidance_x_height_mm(self) -> f64 {
        match self {
            UgSpeedClass::UpTo50 => 100.0,
            UgSpeedClass::Over50To80 => 150.0,
            UgSpeedClass::Over80 => 200.0,
        }
    }
}

/// Minimum separation, m, of the second of two signs beyond the first so that the first does not
/// obscure it: at least 0.6 v, v in km/h, and never less than 30 m ([`UG_SPACING`]).
pub fn ug_min_spacing_m(speed_kph: f64) -> f64 {
    (0.6 * speed_kph).max(30.0)
}

/// Normal height of the lower edge of the primary sign plate above the highest point of the
/// carriageway, m ([`UG_MOUNTING`]); signs are never mounted less than 1 m above carriageway
/// level; 2.1 m above footway level where over or alongside a footway; overhead signs on
/// gantries and mast arms 5.2 m, with 5.0 m clearance between overhead structures and the
/// carriageway.
pub const UG_MOUNTING_NORMAL_M: f64 = 1.8;
pub const UG_MOUNTING_MIN_M: f64 = 1.0;
pub const UG_MOUNTING_FOOTWAY_M: f64 = 2.1;
pub const UG_MOUNTING_OVERHEAD_M: f64 = 5.2;
pub const UG_OVERHEAD_CLEARANCE_M: f64 = 5.0;

/// Clearance from the edge of the road ([`UG_CLEARANCE`]): on unkerbed roads the nearest edge of
/// a sign at least 0.6 m clear of the road shoulder and not less than 1 m from the edge of the
/// traffic lane; on kerbed roads not less than 0.3 m from the kerb face.
pub const UG_CLEAR_OF_SHOULDER_M: f64 = 0.6;
pub const UG_FROM_LANE_EDGE_M: f64 = 1.0;
pub const UG_FROM_KERB_M: f64 = 0.3;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tables_read_back_as_printed() {
        // Table 1-1: 45, 60, 60, 75, 90, 105 (120).
        let got: Vec<f64> = SpeedBandMph::ALL.iter().map(|b| uk_table_1_1(*b).0).collect();
        assert_eq!(got, vec![45.0, 60.0, 60.0, 75.0, 90.0, 105.0]);
        assert_eq!(uk_table_1_1(SpeedBandMph::Over60).1, Some(120.0));
        // Chapter 4 Appendix A column 3 equals Table 1-1 row by row, as printed.
        for b in SpeedBandMph::ALL {
            assert_eq!(uk_warning_row(b).min_clear_visibility_m, uk_table_1_1(b).0);
        }
        assert_eq!(uk_warning_row(SpeedBandMph::From41To50).siting_m, (110.0, 180.0));
        assert_eq!(uk_warning_row(SpeedBandMph::Over60).siting_m, (245.0, 305.0));
        // Table 14-3: the 70 mph row reads 165, 140, 110, 75 for 30, 40, 50, 60 mph signs.
        assert_eq!(uk_table_14_3(70, 30), Some(165.0));
        assert_eq!(uk_table_14_3(70, 60), Some(75.0));
        assert_eq!(uk_table_14_3(30, 20), Some(30.0));
        assert_eq!(uk_table_14_3(40, 40), None);
        // Uganda tables.
        assert_eq!(UgSpeedClass::from_kph(50.0).visibility_m(), 50.0);
        assert_eq!(UgSpeedClass::from_kph(50.1).visibility_m(), 150.0);
        assert_eq!(UgSpeedClass::from_kph(80.0).warning_siting_m(), 100.0);
        assert_eq!(UgSpeedClass::from_kph(100.0).advance_direction_siting_m(), 150.0);
        assert_eq!(ug_min_spacing_m(100.0), 60.0);
        assert_eq!(ug_min_spacing_m(40.0), 30.0);
    }

    #[test]
    fn mph_conversion_note() {
        assert!((50.0 * KPH_PER_MPH - 80.4672).abs() < 1e-9);
        // The cot 10 deg factor printed in the two copies is a rounding of the exact value.
        assert!((cot_reading_stop_angle() - 5.6713).abs() < 1e-4);
    }
}
