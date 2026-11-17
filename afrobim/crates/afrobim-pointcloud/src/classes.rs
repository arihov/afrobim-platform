//! Classification code meanings, taken from the published LAS specification.
//!
//! Source: *LAS Specification 1.4*, OGC Community Standard 17-030r1 (ASPRS document version
//! 1.4-R13, published 2018-03-01), fetched from <https://docs.ogc.org/cs/17-030r1/17-030r1.pdf>.
//! Table 9 gives the classes for point data record formats 0 to 5 and Table 17 those for
//! formats 6 to 10. The two tables differ (class 7 is "Low Point (noise)" in both, but 8 is
//! "Model Key-point (mass point)" in Table 9 and "Reserved" in Table 17; 10, 11, 12 and 13 to 18
//! differ), so the meaning depends on the point format of the file.
//!
//! For files that are not LAS (text, E57) the class column is a plain number: no table applies and
//! [`class_label`] returns the number only.

use serde::{Deserialize, Serialize};

/// Which table of the LAS specification gives the meanings.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ClassTable {
    /// Table 9, point data record formats 0 to 5.
    Legacy,
    /// Table 17, point data record formats 6 to 10.
    Extended,
    /// No table applies (not a LAS file or format unknown): numbers only.
    None,
}

impl ClassTable {
    /// The table for a LAS point data record format number.
    pub fn for_las_format(point_format: u8) -> ClassTable {
        match point_format {
            0..=5 => ClassTable::Legacy,
            6..=10 => ClassTable::Extended,
            _ => ClassTable::None,
        }
    }
}

/// The meaning of a class code in the given table, as written in the specification. `None` when
/// the table has no entry (reserved ranges are reported by [`class_label`]).
pub fn class_name(code: u8, table: ClassTable) -> Option<&'static str> {
    match table {
        ClassTable::None => None,
        ClassTable::Legacy => match code {
            0 => Some("Created, never classified"),
            1 => Some("Unclassified"),
            2 => Some("Ground"),
            3 => Some("Low Vegetation"),
            4 => Some("Medium Vegetation"),
            5 => Some("High Vegetation"),
            6 => Some("Building"),
            7 => Some("Low Point (noise)"),
            8 => Some("Model Key-point (mass point)"),
            9 => Some("Water"),
            12 => Some("Overlap Points"),
            _ => None,
        },
        ClassTable::Extended => match code {
            0 => Some("Created, never classified"),
            1 => Some("Unclassified"),
            2 => Some("Ground"),
            3 => Some("Low Vegetation"),
            4 => Some("Medium Vegetation"),
            5 => Some("High Vegetation"),
            6 => Some("Building"),
            7 => Some("Low Point (noise)"),
            9 => Some("Water"),
            10 => Some("Rail"),
            11 => Some("Road Surface"),
            13 => Some("Wire - Guard (Shield)"),
            14 => Some("Wire - Conductor (Phase)"),
            15 => Some("Transmission Tower"),
            16 => Some("Wire-structure Connector (e.g. Insulator)"),
            17 => Some("Bridge Deck"),
            18 => Some("High Noise"),
            _ => None,
        },
    }
}

/// A display label: the specification's name where the table has one, else the range note from the
/// table (reserved, user definable) or the number alone.
pub fn class_label(code: u8, table: ClassTable) -> String {
    if let Some(n) = class_name(code, table) {
        return format!("{code} {n}");
    }
    match table {
        ClassTable::None => code.to_string(),
        ClassTable::Legacy => match code {
            10 | 11 | 13..=31 => format!("{code} Reserved for ASPRS Definition"),
            _ => code.to_string(),
        },
        ClassTable::Extended => match code {
            8 | 12 | 19..=63 => format!("{code} Reserved"),
            64..=255 => format!("{code} User definable"),
            _ => code.to_string(),
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tables_differ_where_the_specification_differs() {
        assert_eq!(class_name(2, ClassTable::Legacy), Some("Ground"));
        assert_eq!(class_name(2, ClassTable::Extended), Some("Ground"));
        assert_eq!(class_name(8, ClassTable::Legacy), Some("Model Key-point (mass point)"));
        assert_eq!(class_name(8, ClassTable::Extended), None);
        assert_eq!(class_name(12, ClassTable::Legacy), Some("Overlap Points"));
        assert_eq!(class_name(17, ClassTable::Extended), Some("Bridge Deck"));
        assert_eq!(class_label(8, ClassTable::Extended), "8 Reserved");
        assert_eq!(class_label(70, ClassTable::Extended), "70 User definable");
        assert_eq!(class_label(2, ClassTable::None), "2");
        assert_eq!(ClassTable::for_las_format(1), ClassTable::Legacy);
        assert_eq!(ClassTable::for_las_format(7), ClassTable::Extended);
    }
}
