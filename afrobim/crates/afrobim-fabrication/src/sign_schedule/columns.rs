// SPDX-License-Identifier: LicenseRef-AfroBIM-Proprietary
// Copyright (c) 2026 AfroBIM. All rights reserved.
//! The schedule's fields and the mapping from a file's headers to them.

use serde::{Deserialize, Serialize};

use super::params::is_known_param;

/// A field of the AfroBIM sign schedule.
#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(into = "String", try_from = "String")]
pub enum Field {
    Mark,
    Code,
    Description,
    Alignment,
    Station,
    Region,
    Side,
    Offset,
    MountingHeight,
    Size,
    Legend,
    Rotation,
    Notes,
    /// `param:<name>`: a parameter of the sign definition (see `params::PARAMS`).
    Param(String),
}

impl Field {
    /// The fields without which a row cannot be planned.
    pub const REQUIRED: [Field; 6] = [Field::Mark, Field::Code, Field::Alignment, Field::Station, Field::Side, Field::Offset];

    /// The fixed fields, in canonical column order.
    pub const FIXED: [Field; 13] = [
        Field::Mark,
        Field::Code,
        Field::Description,
        Field::Alignment,
        Field::Station,
        Field::Region,
        Field::Side,
        Field::Offset,
        Field::MountingHeight,
        Field::Size,
        Field::Legend,
        Field::Rotation,
        Field::Notes,
    ];

    /// The key used in a saved mapping, e.g. `mark` or `param:speed_kph`.
    pub fn key(&self) -> String {
        match self {
            Field::Mark => "mark".into(),
            Field::Code => "code".into(),
            Field::Description => "description".into(),
            Field::Alignment => "alignment".into(),
            Field::Station => "station".into(),
            Field::Region => "region".into(),
            Field::Side => "side".into(),
            Field::Offset => "offset".into(),
            Field::MountingHeight => "mounting_height".into(),
            Field::Size => "size".into(),
            Field::Legend => "legend".into(),
            Field::Rotation => "rotation".into(),
            Field::Notes => "notes".into(),
            Field::Param(p) => format!("param:{p}"),
        }
    }

    pub fn from_key(key: &str) -> Option<Field> {
        let k = key.trim().to_lowercase();
        if let Some(p) = k.strip_prefix("param:") {
            let p = p.trim();
            return (!p.is_empty()).then(|| Field::Param(p.to_string()));
        }
        Field::FIXED.iter().find(|f| f.key() == k).cloned()
    }

    /// The header the template and the export write for the field.
    pub fn canonical_header(&self) -> String {
        match self {
            Field::Offset => "offset_m".into(),
            Field::MountingHeight => "mounting_height_mm".into(),
            Field::Size => "size_mm".into(),
            Field::Rotation => "rotation_deg".into(),
            other => other.key(),
        }
    }

    /// The length unit a bare number in the field is read in.
    pub fn default_unit(&self) -> Option<LengthUnit> {
        match self {
            Field::Offset => Some(LengthUnit::Metres),
            Field::MountingHeight | Field::Size => Some(LengthUnit::Millimetres),
            _ => None,
        }
    }
}

impl From<Field> for String {
    fn from(f: Field) -> String {
        f.key()
    }
}

impl TryFrom<String> for Field {
    type Error = String;
    fn try_from(s: String) -> Result<Field, String> {
        Field::from_key(&s).ok_or_else(|| format!("'{s}' is not a schedule field"))
    }
}

/// A length unit for the columns that hold lengths.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum LengthUnit {
    Metres,
    Millimetres,
}

impl LengthUnit {
    pub fn to_mm(self, v: f64) -> f64 {
        match self {
            LengthUnit::Metres => v * 1000.0,
            LengthUnit::Millimetres => v,
        }
    }
    pub fn to_m(self, v: f64) -> f64 {
        match self {
            LengthUnit::Metres => v,
            LengthUnit::Millimetres => v / 1000.0,
        }
    }
}

/// One header of the file and the field it holds. `unit` overrides the field's default unit for
/// a bare number in the column (a header such as `Offset (m)` or `Mounting height (mm)` says it).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ColumnEntry {
    pub header: String,
    pub field: Field,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub unit: Option<LengthUnit>,
}

/// Which file header holds which field. Saved and loaded as JSON.
#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
pub struct ColumnMapping {
    pub columns: Vec<ColumnEntry>,
}

impl ColumnMapping {
    pub fn to_json(&self) -> String {
        serde_json::to_string_pretty(self).expect("a mapping always serialises")
    }

    pub fn from_json(text: &str) -> Result<ColumnMapping, String> {
        serde_json::from_str(text).map_err(|e| format!("the column mapping cannot be read: {e}"))
    }

    /// Maps `header` to `field`, replacing any entry for that header or that field.
    pub fn set(&mut self, header: &str, field: Field, unit: Option<LengthUnit>) {
        self.columns.retain(|c| !same_header(&c.header, header) && c.field != field);
        self.columns.push(ColumnEntry { header: header.to_string(), field, unit });
    }

    pub fn entry_for(&self, header: &str) -> Option<&ColumnEntry> {
        self.columns.iter().find(|c| same_header(&c.header, header))
    }

    pub fn has(&self, field: &Field) -> bool {
        self.columns.iter().any(|c| &c.field == field)
    }
}

fn same_header(a: &str, b: &str) -> bool {
    a.trim().eq_ignore_ascii_case(b.trim())
}

/// What [`propose_mapping`] found.
#[derive(Debug, Clone, PartialEq)]
pub struct MappingProposal {
    pub mapping: ColumnMapping,
    /// Headers that matched nothing. They are not read.
    pub unmapped_headers: Vec<String>,
    /// Headers that match a field another header already took, with that field. Not read.
    pub duplicate_headers: Vec<(String, Field)>,
    /// Required fields no header matched: the caller must map one before importing.
    pub missing_required: Vec<Field>,
    /// `param:` headers whose parameter name is not one the schedule knows.
    pub unknown_params: Vec<String>,
}

/// The header names accepted for each field, after [`normalise`]. Anything else is not guessed.
pub const SYNONYMS: &[(Field, &[&str])] = &[
    (Field::Mark, &["mark", "sign mark", "sign no", "sign number", "sign ref", "sign reference", "sign id", "ref", "reference", "id", "item no"]),
    (
        Field::Code,
        &["code", "sign code", "diagram", "diagram no", "diagram number", "diagram ref", "sign diagram", "catalog key", "catalogue key", "sign page", "sign type", "type"],
    ),
    (Field::Description, &["description", "desc", "sign description", "name", "sign name", "details"]),
    (Field::Alignment, &["alignment", "alignment name", "road", "road name", "route"]),
    (Field::Station, &["station", "stationing", "chainage", "chain", "ch", "sta", "stn", "displayed station", "station displayed", "station m"]),
    (Field::Region, &["region", "station region", "stationing region", "equation region", "station equation region", "eq region"]),
    (Field::Side, &["side", "side of road", "left right", "l r", "lr", "hand"]),
    (
        Field::Offset,
        &["offset", "offset m", "centreline offset", "centerline offset", "offset from centreline", "offset from cl", "cl offset", "distance from centreline", "dist from cl"],
    ),
    (
        Field::MountingHeight,
        &["mounting height", "mounting height mm", "mounting height m", "mount height", "installation height", "height above ground"],
    ),
    (Field::Size, &["size", "size mm", "sign size", "nominal size", "x height", "xheight", "x height mm", "text height"]),
    (Field::Legend, &["legend", "text", "legend text", "sign text", "message", "wording", "destinations", "destination"]),
    (Field::Rotation, &["rotation", "rotation deg", "facing", "facing deg", "orientation", "angle"]),
    (Field::Notes, &["notes", "note", "remarks", "remark", "comments", "comment"]),
];

/// Lower case, bracketed parts removed, every run of other characters made one space.
pub fn normalise(header: &str) -> String {
    let mut depth = 0u32;
    let mut s = String::new();
    for c in header.chars() {
        match c {
            '(' | '[' => depth += 1,
            ')' | ']' => depth = depth.saturating_sub(1),
            c if depth == 0 => s.push(c),
            _ => {}
        }
    }
    let mut out = String::new();
    let mut gap = true;
    for c in s.chars() {
        if c.is_alphanumeric() {
            out.extend(c.to_lowercase());
            gap = false;
        } else if !gap {
            out.push(' ');
            gap = true;
        }
    }
    out.trim().to_string()
}

/// The length unit a header names: a bracketed `(m)` or `(mm)`, or a trailing `m`/`mm` word.
pub fn unit_hint(header: &str) -> Option<LengthUnit> {
    let h = header.trim().to_lowercase();
    let bracket = h.rfind(['(', '[']).map(|i| h[i + 1..].trim_end_matches([')', ']']).trim().to_string());
    let tail = bracket.or_else(|| h.rsplit(|c: char| !c.is_alphanumeric()).next().map(str::to_string));
    match tail.as_deref() {
        Some("mm") => Some(LengthUnit::Millimetres),
        Some("m") => Some(LengthUnit::Metres),
        _ => None,
    }
}

/// Proposes which field each header holds, by exact match of the normalised header against
/// [`SYNONYMS`] (and `param:<name>` headers). Nothing is guessed beyond that: a header that
/// matches nothing is listed and not read, and a field that two headers match goes to the first.
pub fn propose_mapping(headers: &[String]) -> MappingProposal {
    let mut mapping = ColumnMapping::default();
    let mut unmapped = Vec::new();
    let mut duplicates = Vec::new();
    let mut unknown_params = Vec::new();
    for header in headers {
        let field = match header.trim().to_lowercase().strip_prefix("param:") {
            Some(p) if !p.trim().is_empty() => Some(Field::Param(p.trim().to_string())),
            _ => {
                let n = normalise(header);
                SYNONYMS.iter().find(|(_, names)| names.contains(&n.as_str())).map(|(f, _)| f.clone())
            }
        };
        let Some(field) = field else {
            if !header.trim().is_empty() {
                unmapped.push(header.clone());
            }
            continue;
        };
        if let Field::Param(p) = &field {
            if !is_known_param(p) {
                unknown_params.push(header.clone());
            }
        }
        if mapping.has(&field) {
            duplicates.push((header.clone(), field));
            continue;
        }
        let unit = field.default_unit().and_then(|_| unit_hint(header));
        mapping.columns.push(ColumnEntry { header: header.trim().to_string(), field, unit });
    }
    let missing_required = Field::REQUIRED.iter().filter(|f| !mapping.has(f)).cloned().collect();
    MappingProposal { mapping, unmapped_headers: unmapped, duplicate_headers: duplicates, missing_required, unknown_params }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn h(v: &[&str]) -> Vec<String> {
        v.iter().map(|s| s.to_string()).collect()
    }

    #[test]
    fn canonical_headers_map_to_their_own_fields() {
        let headers: Vec<String> = Field::FIXED.iter().map(Field::canonical_header).collect();
        let p = propose_mapping(&headers);
        assert!(p.unmapped_headers.is_empty() && p.duplicate_headers.is_empty() && p.missing_required.is_empty(), "{p:?}");
        for f in Field::FIXED {
            assert!(p.mapping.has(&f), "{f:?}");
        }
        assert_eq!(p.mapping.entry_for("offset_m").unwrap().unit, Some(LengthUnit::Metres));
        assert_eq!(p.mapping.entry_for("mounting_height_mm").unwrap().unit, Some(LengthUnit::Millimetres));
    }

    #[test]
    fn foreign_headers_map_by_synonym_and_units_come_from_the_header() {
        let p = propose_mapping(&h(&["Sign No.", "TSRGD Diagram", "Road Name", "Chainage", "L/R", "Offset (m)", "Mounting Height (m)", "Remarks", "Colour", "Param:Speed_kph"]));
        let f = |name: &str| p.mapping.entry_for(name).map(|c| c.field.clone());
        assert_eq!(f("Sign No."), Some(Field::Mark));
        assert_eq!(f("Road Name"), Some(Field::Alignment));
        assert_eq!(f("Chainage"), Some(Field::Station));
        assert_eq!(f("L/R"), Some(Field::Side));
        assert_eq!(p.mapping.entry_for("Offset (m)").unwrap().unit, Some(LengthUnit::Metres));
        assert_eq!(p.mapping.entry_for("Mounting Height (m)").unwrap().unit, Some(LengthUnit::Metres));
        assert_eq!(f("Remarks"), Some(Field::Notes));
        assert_eq!(f("Param:Speed_kph"), Some(Field::Param("speed_kph".into())));
        // "TSRGD Diagram" is not a listed synonym: it is not guessed.
        assert_eq!(f("TSRGD Diagram"), None);
        assert_eq!(p.unmapped_headers, ["TSRGD Diagram", "Colour"]);
        assert_eq!(p.missing_required, [Field::Code]);
    }

    #[test]
    fn a_second_header_for_a_field_is_reported_not_read() {
        let p = propose_mapping(&h(&["mark", "ref", "code"]));
        assert_eq!(p.duplicate_headers, [("ref".to_string(), Field::Mark)]);
        assert_eq!(p.mapping.entry_for("mark").unwrap().field, Field::Mark);
    }

    #[test]
    fn unknown_param_names_are_listed() {
        let p = propose_mapping(&h(&["param:colour"]));
        assert_eq!(p.unknown_params, ["param:colour"]);
    }

    #[test]
    fn a_mapping_saves_and_loads_as_json() {
        let p = propose_mapping(&h(&["Sign No.", "Offset (m)", "param:speed_kph"]));
        let json = p.mapping.to_json();
        assert!(json.contains("\"mark\"") && json.contains("param:speed_kph"));
        assert_eq!(ColumnMapping::from_json(&json).unwrap(), p.mapping);
        assert!(ColumnMapping::from_json(r#"{"columns":[{"header":"x","field":"nonsense"}]}"#).is_err());
    }

    #[test]
    fn normalise_strips_brackets_and_punctuation() {
        assert_eq!(normalise("  Sign   No. "), "sign no");
        assert_eq!(normalise("Offset (m)"), "offset");
        assert_eq!(normalise("mounting_height_mm"), "mounting height mm");
        assert_eq!(normalise("L/R"), "l r");
    }
}
