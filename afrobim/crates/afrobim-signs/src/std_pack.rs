// SPDX-License-Identifier: LicenseRef-AfroBIM-Proprietary
// Copyright (c) 2026 AfroBIM. All rights reserved.
//! The rule-pack sections of the standard (catalog) signs: the full sign catalog, the TSRGD
//! 2016 sign bodies and pictograms, the sizes of warning and regulatory signs and their
//! plates, and the plate border.
//!
//! Port of the matching records of C# `SignCore.Rules.RulePack` and of the checks
//! `RulePackLoader.ValidateOptionalSections` / `ValidateTsrgdSections` runs on them. The
//! sections are read from the same pack JSON as [`RulePack`], which keeps the sections the
//! guidance layouts read. All are optional: a pack without them (the Uganda pack) simply has no
//! TSRGD signs. No value is defaulted here.

use std::collections::BTreeMap;
use std::path::Path;

use serde::Deserialize;

use crate::model::Colour;
use crate::pack::RulePack;
use crate::std_catalog::SignRef;
use crate::SignError;

/// The plate outline of UK TSM Chapter 7 §2.15 (Figure 2-4): a pack with this section draws a
/// rounded border on rectangular plates. Lengths in sw.
#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SignBorderRules {
    /// Radius of the plate's outside corners, concentric with the inside corner of the border.
    pub outer_corner_radius: f64,
    /// Straight segments each 90° corner arc is drawn with (a drawing-fidelity setting, not a
    /// manual value).
    pub segments_per_corner: i64,
}

/// The size of a warning or regulatory sign by 85th percentile speed band (the bands are
/// [`SignSizeRules::speed_bands_kph`]). A `None` band means the size table gives no size for
/// it. `dimension` names what the size measures: a triangle's or octagon's `height`, a disc's
/// `diameter`. `permitted_mm` are the sizes TSRGD 2016 prescribes for the diagram;
/// `alternative_by_band_mm` the bracketed alternatives.
#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PackSignSizing {
    pub dimension: String,
    pub by_band_mm: Vec<Option<f64>>,
    #[serde(default)]
    pub alternative_by_band_mm: Option<Vec<Option<f64>>>,
    #[serde(default)]
    pub permitted_mm: Option<Vec<f64>>,
    pub source: String,
}

/// The body of a TSRGD warning or regulatory sign: its outline and border band. Lengths are
/// fractions of the nominal dimension (triangle and octagon height, disc diameter). `shape` is
/// `triangle-up`, `triangle-down`, `circle` or `octagon` (flat top, height across the flats).
/// `corner_radius` rounds the outside corners of a triangle; its inside edge stays sharp.
#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SignBody {
    pub shape: String,
    pub border: Colour,
    pub ground: Colour,
    pub border_width: f64,
    pub corner_radius: f64,
    pub source: String,
}

/// One filled outline of a pictogram, in the body's frame: x from the body's vertical centre
/// line and y up from its foot, both as fractions of the nominal dimension. `colour` is a
/// [`Colour`] name, or `Ground` for an area showing the sign's ground (so a temporary yellow
/// sign recolours it). Outlines are painted in order.
#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PictogramPath {
    pub colour: String,
    pub points: Vec<Vec<f64>>,
}

/// Numerals a diagram lets the designer vary (the speed limit of diagram 670): the numerals
/// TSRGD draws (`text`), their height and the centre of their extent, in the pictogram frame.
/// The engine sets the chosen numerals at this size.
#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PictogramNumeral {
    pub text: String,
    pub colour: String,
    pub height: f64,
    pub centre_x: f64,
    pub centre_y: f64,
}

/// A sign pictogram read from the TSRGD 2016 vector drawing of `id` (the diagram number).
/// `body` keys [`StdRules::sign_bodies`]; `source` cites the schedule, diagram and page.
#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Pictogram {
    pub id: String,
    pub body: String,
    pub source: String,
    #[serde(default)]
    pub paths: Vec<PictogramPath>,
    #[serde(default)]
    pub numeral: Option<PictogramNumeral>,
}

/// The pack's TSRGD pictograms, with the Open Government Licence attribution.
#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PictogramSet {
    #[serde(default)]
    pub attribution: Option<String>,
    pub entries: Vec<Pictogram>,
}

impl PictogramSet {
    pub fn find(&self, id: &str) -> Option<&Pictogram> {
        self.entries.iter().find(|e| e.id.eq_ignore_ascii_case(id))
    }
}

/// One row of a plate size table: the plate diagram, the legends it covers (a regular
/// expression over the legend, lines joined by a space) and its x-heights, either by sign size
/// in mm (TSM Chapter 4 Appendix B, keyed by triangle height) or by speed band (TSM Chapter 3
/// Appendix A). `low_speed_x_height_by_sign_size_mm` holds the smaller value of a "50/62.5"
/// cell (Appendix B note 1).
#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PlateLegendRule {
    pub diagram: String,
    #[serde(default)]
    pub description: String,
    pub pattern: String,
    #[serde(default)]
    pub x_height_by_sign_size_mm: Option<BTreeMap<String, f64>>,
    #[serde(default)]
    pub low_speed_x_height_by_sign_size_mm: Option<BTreeMap<String, f64>>,
    #[serde(default)]
    pub x_height_by_band_mm: Option<Vec<Option<f64>>>,
}

impl PlateLegendRule {
    /// True when the row's pattern is found in `legend` (C# `Regex.IsMatch`). The pattern was
    /// compiled once when the pack loaded, so a failure here is a pack error.
    pub fn matches(&self, legend: &str) -> Result<bool, SignError> {
        regex::Regex::new(&self.pattern)
            .map(|re| re.is_match(legend))
            .map_err(|e| {
                SignError::Pack(format!(
                    "Rule pack plate row {} has a bad pattern: {e}",
                    self.diagram
                ))
            })
    }
}

/// The plates of one sign family (warning or regulatory) and the gap between sign and plate
/// (`None` when no source states one). A plate's x-height is read from the first of `legends`
/// whose pattern matches its legend; a legend no row covers has no x-height. Up to
/// `low_speed_max_kph` the smaller x-height of a row applies.
#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PlateRules {
    pub legends: Vec<PlateLegendRule>,
    #[serde(default)]
    pub low_speed_max_kph: Option<i64>,
    #[serde(default)]
    pub gap_mm: Option<f64>,
    pub source: String,
    #[serde(default)]
    pub gap_source: Option<String>,
}

/// The speed bands that size warning and regulatory signs and their plates. Each band is "up
/// to" its value in km/h; the last band (`None`) is open.
#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SignSizeRules {
    pub speed_bands_kph: Vec<Option<i64>>,
    pub source: String,
    #[serde(default)]
    pub warning_plate: Option<PlateRules>,
    #[serde(default)]
    pub regulatory_plate: Option<PlateRules>,
}

impl SignSizeRules {
    /// Index of the band that holds `speed_kph`.
    pub fn band_index(&self, speed_kph: u32) -> usize {
        self.speed_bands_kph
            .iter()
            .position(|band| band.is_none_or(|max| i64::from(speed_kph) <= max))
            .unwrap_or(self.speed_bands_kph.len().saturating_sub(1))
    }
}

/// A registered sign page as the pack writes it (C# `SignCatalogEntry`). `key` is the
/// canonical sign-page reference; `family` is the layout token. A UK warning or regulatory
/// entry also names its TSRGD 2016 `diagram` number, the schedule item it is prescribed by
/// (`citation`) and its `sizes` by speed band.
#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PackCatalogEntry {
    pub key: String,
    #[serde(default)]
    pub name: String,
    #[serde(default)]
    pub family: String,
    #[serde(default)]
    pub series: String,
    #[serde(default)]
    pub scheme: String,
    #[serde(default)]
    pub symbol_id: Option<String>,
    #[serde(default)]
    pub embedded_symbols: Option<Vec<String>>,
    #[serde(default)]
    pub template: Option<String>,
    #[serde(default)]
    pub diagram: Option<String>,
    #[serde(default)]
    pub citation: Option<String>,
    #[serde(default)]
    pub sizes: Option<PackSignSizing>,
}

/// The catalog section in full.
#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PackCatalog {
    pub entries: Vec<PackCatalogEntry>,
}

/// The standard-sign sections of a rule pack.
#[derive(Debug, Clone, PartialEq, Default, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct StdRules {
    #[serde(default)]
    pub sign_border: Option<SignBorderRules>,
    #[serde(default)]
    pub sign_bodies: Option<BTreeMap<String, SignBody>>,
    #[serde(default)]
    pub sign_sizes: Option<SignSizeRules>,
    #[serde(default)]
    pub pictograms: Option<PictogramSet>,
    #[serde(default)]
    pub catalog: Option<PackCatalog>,
}

/// The body shapes `signBodies` may name.
const BODY_SHAPES: [&str; 4] = ["triangle-up", "triangle-down", "circle", "octagon"];

/// A colour by its name, exactly as written (C# `Enum.TryParse<Colour>`).
pub(crate) fn colour_named(name: &str) -> Option<Colour> {
    Some(match name {
        "White" => Colour::White,
        "Black" => Colour::Black,
        "Green" => Colour::Green,
        "Blue" => Colour::Blue,
        "Brown" => Colour::Brown,
        "Yellow" => Colour::Yellow,
        "Red" => Colour::Red,
        "Grey" => Colour::Grey,
        _ => return None,
    })
}

impl StdRules {
    /// Reads the standard-sign sections of the pack file `pack` was loaded from.
    pub fn load(path: &Path, pack: &RulePack) -> Result<StdRules, SignError> {
        let shown = path.display();
        let text = std::fs::read_to_string(path)
            .map_err(|e| SignError::Pack(format!("Rule pack not found: {shown} ({e})")))?;
        Self::from_json(&text, &shown.to_string(), pack)
    }

    /// Parses the sections from pack JSON; `path` only names the pack in error messages.
    pub fn from_json(text: &str, path: &str, pack: &RulePack) -> Result<StdRules, SignError> {
        let rules: StdRules = serde_json::from_str(&crate::pack::strip_json_comments(text))
            .map_err(|e| SignError::Pack(format!("Rule pack is not valid JSON: {path}: {e}")))?;
        rules.validate(pack, path)?;
        Ok(rules)
    }

    /// The sections from an already parsed document (a folder pack gathered by `xml_pack.rs`);
    /// `path` only names the pack in error messages. The same checks as [`StdRules::from_json`].
    pub fn from_value(
        value: &serde_json::Value,
        path: &str,
        pack: &RulePack,
    ) -> Result<StdRules, SignError> {
        let rules: StdRules = serde_json::from_value(value.clone())
            .map_err(|e| SignError::Pack(format!("Rule pack is not valid: {path}: {e}")))?;
        rules.validate(pack, path)?;
        Ok(rules)
    }

    /// The catalog entry with this key (case is not significant).
    pub fn find_entry(&self, key: &str) -> Option<&PackCatalogEntry> {
        self.catalog
            .as_ref()?
            .entries
            .iter()
            .find(|e| e.key.eq_ignore_ascii_case(key))
    }

    /// The checks of C# `RulePackLoader` for these sections, so a bad hand-authored pack fails
    /// at load rather than mid-layout.
    fn validate(&self, pack: &RulePack, path: &str) -> Result<(), SignError> {
        let bad = |msg: String| Err(SignError::Pack(format!("{msg}: {path}")));

        if let Some(sb) = &self.sign_border {
            if sb.outer_corner_radius < pack.panel.corner_radius {
                return bad(
                    "Rule pack 'signBorder.outerCornerRadius' must be at least 'panel.cornerRadius'"
                        .into(),
                );
            }
            if sb.segments_per_corner < 1 {
                return bad("Rule pack 'signBorder.segmentsPerCorner' must be at least 1".into());
            }
        }

        if let Some(cat) = &self.catalog {
            for e in &cat.entries {
                if SignRef::parse(&e.key).is_none() {
                    return bad(format!(
                        "Rule pack 'catalog' entry has an invalid key '{}'",
                        e.key
                    ));
                }
                if e.family.trim().is_empty() {
                    return bad(format!(
                        "Rule pack 'catalog' entry '{}' has no family",
                        e.key
                    ));
                }
                if e.series.trim().is_empty() {
                    return bad(format!(
                        "Rule pack 'catalog' entry '{}' has no series",
                        e.key
                    ));
                }
            }
            // Cross-reference: a catalog scheme must exist in the palette when both present.
            if let Some(pal) = &pack.palettes {
                if let Some(e) = cat
                    .entries
                    .iter()
                    .find(|e| !e.scheme.trim().is_empty() && pal.find(&e.scheme).is_none())
                {
                    return bad(format!(
                        "Rule pack 'catalog' entry '{}→{}' references an undefined scheme",
                        e.key, e.scheme
                    ));
                }
            }
        }

        if let Some(bodies) = &self.sign_bodies {
            for (id, b) in bodies {
                if !BODY_SHAPES.contains(&b.shape.as_str()) {
                    return bad(format!(
                        "Rule pack 'signBodies.{id}' shape must be one of {}",
                        BODY_SHAPES.join(", ")
                    ));
                }
                // Written so that a NaN fails too.
                let border_ok = b.border_width > 0.0 && b.border_width < 0.5;
                if !border_ok || b.corner_radius < 0.0 {
                    return bad(format!(
                        "Rule pack 'signBodies.{id}' border must be in (0, 0.5) and corner radius non-negative"
                    ));
                }
                if b.source.trim().is_empty() {
                    return bad(format!("Rule pack 'signBodies.{id}' has no source"));
                }
            }
        }

        if let Some(pics) = &self.pictograms {
            let Some(bodies) = &self.sign_bodies else {
                return bad("Rule pack 'pictograms' needs a 'signBodies' section".into());
            };
            for (i, p) in pics.entries.iter().enumerate() {
                if p.id.trim().is_empty()
                    || pics.entries[..i]
                        .iter()
                        .any(|o| o.id.eq_ignore_ascii_case(&p.id))
                {
                    return bad(format!(
                        "Rule pack 'pictograms' has duplicate or empty id '{}'",
                        p.id
                    ));
                }
            }
            for p in &pics.entries {
                if !bodies.contains_key(&p.body) {
                    return bad(format!(
                        "Rule pack pictogram '{}' names an undefined body '{}'",
                        p.id, p.body
                    ));
                }
                if p.source.trim().is_empty() {
                    return bad(format!("Rule pack pictogram '{}' has no source", p.id));
                }
                for pp in &p.paths {
                    if pp.colour != "Ground" && colour_named(&pp.colour).is_none() {
                        return bad(format!(
                            "Rule pack pictogram '{}' has an unknown colour '{}'",
                            p.id, pp.colour
                        ));
                    }
                    if pp.points.len() < 3 || pp.points.iter().any(|q| q.len() != 2) {
                        return bad(format!(
                            "Rule pack pictogram '{}' has a path that is not a list of three or more [x, y] points",
                            p.id
                        ));
                    }
                }
                if let Some(n) = &p.numeral {
                    let height_ok = n.height > 0.0;
                    if !height_ok || n.text.trim().is_empty() {
                        return bad(format!(
                            "Rule pack pictogram '{}' numeral needs text and a height",
                            p.id
                        ));
                    }
                    // The C# engine parses this colour when it draws the numerals; a bad name
                    // is caught here instead.
                    if n.colour != "Ground" && colour_named(&n.colour).is_none() {
                        return bad(format!(
                            "Rule pack pictogram '{}' has an unknown colour '{}'",
                            p.id, n.colour
                        ));
                    }
                }
            }
        }

        let mut bands = 0;
        if let Some(sizes) = &self.sign_sizes {
            bands = sizes.speed_bands_kph.len();
            if bands == 0 {
                return bad("Rule pack 'signSizes.speedBandsKph' is empty".into());
            }
            for pair in sizes.speed_bands_kph.windows(2) {
                let rises = match (pair[0], pair[1]) {
                    (None, _) => false,
                    (Some(a), Some(b)) => b > a,
                    (Some(_), None) => true,
                };
                if !rises {
                    return bad(
                        "Rule pack 'signSizes.speedBandsKph' must rise, with only the last band open"
                            .into(),
                    );
                }
            }
            for plates in [&sizes.warning_plate, &sizes.regulatory_plate]
                .into_iter()
                .flatten()
            {
                for row in &plates.legends {
                    if row.pattern.trim().is_empty() || row.diagram.trim().is_empty() {
                        return bad(
                            "Rule pack 'signSizes' plate rows need a diagram and a pattern".into(),
                        );
                    }
                    if let Err(e) = regex::Regex::new(&row.pattern) {
                        return bad(format!(
                            "Rule pack plate row {} has a bad pattern: {e}",
                            row.diagram
                        ));
                    }
                    if row
                        .x_height_by_band_mm
                        .as_ref()
                        .is_some_and(|b| b.len() != bands)
                    {
                        return bad(format!(
                            "Rule pack plate row {} needs one x-height per speed band",
                            row.diagram
                        ));
                    }
                    if row.x_height_by_sign_size_mm.is_none() && row.x_height_by_band_mm.is_none() {
                        return bad(format!(
                            "Rule pack plate row {} has no x-heights",
                            row.diagram
                        ));
                    }
                }
            }
        }

        for e in self.catalog.iter().flat_map(|c| &c.entries) {
            if let Some(s) = &e.sizes {
                if bands == 0 {
                    return bad(format!(
                        "Rule pack catalog entry '{}' has sizes but the pack has no 'signSizes'",
                        e.key
                    ));
                }
                if s.by_band_mm.len() != bands
                    || s.alternative_by_band_mm
                        .as_ref()
                        .is_some_and(|alt| alt.len() != bands)
                {
                    return bad(format!(
                        "Rule pack catalog entry '{}' needs one size per speed band",
                        e.key
                    ));
                }
                if s.by_band_mm.iter().flatten().any(|v| *v <= 0.0) || s.source.trim().is_empty() {
                    return bad(format!(
                        "Rule pack catalog entry '{}' sizes must be positive and cite a source",
                        e.key
                    ));
                }
            }
            if let (Some(d), Some(set)) = (
                e.diagram.as_deref().filter(|d| !d.is_empty()),
                &self.pictograms,
            ) {
                if set.find(d).is_none() {
                    return bad(format!(
                        "Rule pack catalog entry '{}' names diagram {d} with no pictogram",
                        e.key
                    ));
                }
            }
        }
        Ok(())
    }
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;

    pub(crate) fn uk_std(pack: &RulePack) -> StdRules {
        let path = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../../packs/UK-TSM-2019.json");
        StdRules::load(&path, pack).expect("the UK pack's standard-sign sections load")
    }

    fn uk() -> (RulePack, StdRules) {
        let pack = crate::pack::tests::uk_pack();
        let std = uk_std(&pack);
        (pack, std)
    }

    /// The UK pack with one section replaced by `value` (or removed when `Null`).
    fn with_section(section: &str, value: serde_json::Value) -> Result<StdRules, SignError> {
        let path = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../../packs/UK-TSM-2019.json");
        let text = std::fs::read_to_string(path).unwrap();
        let mut json: serde_json::Value =
            serde_json::from_str(&crate::pack::strip_json_comments(&text)).unwrap();
        if value.is_null() {
            json.as_object_mut().unwrap().remove(section);
        } else {
            json[section] = value;
        }
        StdRules::from_json(&json.to_string(), "x.json", &crate::pack::tests::uk_pack())
    }

    fn message(r: Result<StdRules, SignError>) -> String {
        r.unwrap_err().to_string()
    }

    #[test]
    fn uk_pack_carries_tsrgd_pictograms_bodies_and_sizes() {
        let (_, std) = uk();
        let pics = std.pictograms.as_ref().unwrap();
        assert_eq!(pics.entries.len(), 24);
        assert!(pics
            .attribution
            .as_deref()
            .unwrap()
            .contains("Open Government Licence"));
        let bodies = std.sign_bodies.as_ref().unwrap();
        assert_eq!(bodies["triangle"].shape, "triangle-up");
        assert_eq!(bodies["triangle"].border, Colour::Red);
        assert_eq!(bodies["blue-disc"].ground, Colour::Blue);
        assert_eq!(bodies["octagon"].border_width, 0.0331);
        let limit = pics.find("670").unwrap().numeral.as_ref().unwrap();
        assert_eq!(
            (limit.text.as_str(), limit.colour.as_str()),
            ("40", "Black")
        );
        assert!(pics.find("670").unwrap().paths.is_empty());
        let sizes = std.sign_sizes.as_ref().unwrap();
        assert_eq!(sizes.speed_bands_kph.len(), 6);
        assert_eq!(sizes.warning_plate.as_ref().unwrap().legends.len(), 4);
        assert_eq!(sizes.warning_plate.as_ref().unwrap().gap_mm, None);
        assert_eq!(std.sign_border.as_ref().unwrap().outer_corner_radius, 3.0);
        assert_eq!(std.catalog.as_ref().unwrap().entries.len(), 37);
        let d632 = std.find_entry("d632").unwrap();
        assert_eq!(d632.diagram.as_deref(), Some("632"));
        assert_eq!(d632.sizes.as_ref().unwrap().by_band_mm[0], None);
        assert_eq!(d632.sizes.as_ref().unwrap().by_band_mm[1], Some(600.0));
    }

    /// TSM Chapter 4 Appendix A bands in km/h: up to 32, 48, 64, 80, 96, then open.
    #[test]
    fn a_speed_falls_in_the_first_band_that_holds_it() {
        let (_, std) = uk();
        let sizes = std.sign_sizes.as_ref().unwrap();
        let bands: Vec<usize> = [0, 32, 33, 48, 64, 80, 96, 97, 200]
            .into_iter()
            .map(|kph| sizes.band_index(kph))
            .collect();
        assert_eq!(bands, [0, 0, 1, 1, 2, 3, 4, 5, 5]);
    }

    #[test]
    fn plate_rows_are_chosen_by_legend() {
        let (_, std) = uk();
        let plates = std
            .sign_sizes
            .as_ref()
            .unwrap()
            .warning_plate
            .as_ref()
            .unwrap();
        let row = |legend: &str| {
            plates
                .legends
                .iter()
                .find(|r| r.matches(legend).unwrap())
                .map(|r| r.diagram.as_str())
        };
        assert_eq!(row("100 yds"), Some("572"));
        assert_eq!(row("1 mile"), Some("572"));
        assert_eq!(row("For 2 miles"), Some("570"));
        assert_eq!(row("Single file traffic"), Some("518"));
        assert_eq!(row("Adverse camber"), Some("513.1"));
        assert_eq!(row("Humps for 1 mile"), None);
        assert_eq!(row("adverse camber"), None); // patterns are case-sensitive
    }

    #[test]
    fn colours_are_named_exactly() {
        assert_eq!(colour_named("Red"), Some(Colour::Red));
        assert_eq!(colour_named("red"), None);
        assert_eq!(colour_named("Ground"), None);
    }

    #[test]
    fn a_pack_without_the_sections_has_none() {
        let pack = crate::pack::tests::uk_pack();
        let std = StdRules::from_json("{}", "x.json", &pack).unwrap();
        assert_eq!(std, StdRules::default());
        assert!(std.find_entry("D512").is_none());
    }

    #[test]
    fn bad_sections_fail_to_load() {
        use serde_json::json;
        assert!(message(with_section(
            "signBorder",
            json!({"outerCornerRadius": 1.0, "segmentsPerCorner": 12})
        ))
        .contains("must be at least 'panel.cornerRadius'"));
        assert!(message(with_section(
            "signBorder",
            json!({"outerCornerRadius": 3.0, "segmentsPerCorner": 0})
        ))
        .contains("segmentsPerCorner' must be at least 1"));
        assert!(message(with_section("signBodies", serde_json::Value::Null))
            .contains("'pictograms' needs a 'signBodies' section"));
        assert!(message(with_section("signSizes", serde_json::Value::Null))
            .contains("has sizes but the pack has no 'signSizes'"));
        assert!(message(with_section(
            "signSizes",
            json!({"speedBandsKph": [48, 32, null], "source": "x"})
        ))
        .contains("must rise"));
        assert!(message(with_section(
            "signSizes",
            json!({"speedBandsKph": [32, 48, 64, 80, 96, null], "source": "x", "warningPlate": {
                "source": "x",
                "legends": [{"diagram": "1", "pattern": "(", "xHeightByBandMm": [1, 1, 1, 1, 1, 1]}]}})
        ))
        .contains("plate row 1 has a bad pattern"));
        assert!(message(with_section(
            "signSizes",
            json!({"speedBandsKph": [32, 48, 64, 80, 96, null], "source": "x", "warningPlate": {
                "source": "x", "legends": [{"diagram": "1", "pattern": "x"}]}})
        ))
        .contains("plate row 1 has no x-heights"));
        assert!(message(with_section(
            "catalog",
            json!({"entries": [{"key": "12", "family": "stack", "series": "UK"}]})
        ))
        .contains("invalid key '12'"));
        assert!(message(with_section(
            "catalog",
            json!({"entries": [{"key": "UK1", "family": "stack", "series": "UK", "scheme": "pink"}]})
        ))
        .contains("'UK1→pink' references an undefined scheme"));
        assert!(message(with_section(
            "catalog",
            json!({"entries": [{"key": "D1", "family": "warning", "series": "D", "diagram": "999"}]})
        ))
        .contains("names diagram 999 with no pictogram"));
        assert!(message(with_section(
            "pictograms",
            json!({"entries": [{"id": "1", "body": "triangle", "source": "x",
                "paths": [{"colour": "Pink", "points": [[0, 0], [1, 0], [0, 1]]}]}]})
        ))
        .contains("unknown colour 'Pink'"));
        assert!(message(with_section(
            "pictograms",
            json!({"entries": [{"id": "1", "body": "triangle", "source": "x",
                "paths": [{"colour": "Black", "points": [[0, 0], [1, 0]]}]}]})
        ))
        .contains("three or more [x, y] points"));
        assert!(message(with_section(
            "pictograms",
            json!({"entries": [{"id": "1", "body": "hexagon", "source": "x"}]})
        ))
        .contains("undefined body 'hexagon'"));
    }
}
