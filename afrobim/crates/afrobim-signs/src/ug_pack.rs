// SPDX-License-Identifier: LicenseRef-AfroBIM-Proprietary
// Copyright (c) 2026 AfroBIM. All rights reserved.
//! The rule-pack sections only the Uganda pack carries: the speed-class body-size ladders
//! (`standardSizes`) and the warning / regulatory body outlines with their stated slash forms
//! (`roundelsTriangles`).
//!
//! Port of the C# `SignCore.Rules` records `StandardSizeBand`, `SizeFamily`,
//! `StandardSizeRules`, `SlashForm`, `ProvisionalProportion` and `RoundelTriangleRules`, and of
//! the checks `RulePackLoader.ValidateOptionalSections` makes on them and on `embeddedShapes`.
//! Both sections are optional. No value is defaulted here.

use serde::Deserialize;

use crate::SignError;

/// One speed class in the standard body-size ladder (Uganda Vol.1 Table D.1/D.3/D.4 and the
/// Vol.2 App.1 §3.1 / Table 2 supplementary ladder). Values are the exact millimetres quoted by
/// the manual; the open-ended top band leaves `speed_max_kph` unset.
#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct StandardSizeBand {
    pub class: String,
    #[serde(default)]
    pub speed_max_kph: Option<i64>,
    pub warning_triangle_side_mm: f64,
    pub regulatory_diameter_mm: f64,
    pub supplementary_x_height_mm: f64,
    pub supplementary_sw_mm: f64,
    pub supplementary_cap_mm: f64,
}

/// Which column of the speed-class body-size ladder a sign family reads (Vol.1 Table D.1
/// warning triangles / D.3 regulatory discs / supplementary Table 2).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SizeFamily {
    /// Warning triangles: the equilateral side.
    Warning,
    /// Regulatory discs: the circle diameter.
    Regulatory,
    /// Supplementary plates: the text x-height.
    Supplementary,
}

/// The speed-class body-size ladders as data. `low_speed_regulatory_diameter_mm` is the D.4
/// low-speed / space-limited regulatory circle diameter (400 mm, Vol.1 p.21) that sits
/// alongside the three bands.
#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct StandardSizeRules {
    pub bands: Vec<StandardSizeBand>,
    pub low_speed_regulatory_diameter_mm: f64,
}

impl StandardSizeRules {
    /// The band of a speed class (`low`, `medium`, `high`); case is not significant.
    pub fn for_class(&self, class: &str) -> Option<&StandardSizeBand> {
        self.bands
            .iter()
            .find(|b| b.class.eq_ignore_ascii_case(class))
    }

    /// The standard-size band covering `speed_kph`. Same inclusive-upper-bound convention as
    /// the text sizes: bands are ordered by their speed ceiling (the open-ended top band sorts
    /// last) and the first band whose ceiling is unset or at least the speed wins (≤50 → low,
    /// ≤80 → medium, otherwise the open high band).
    pub fn for_speed(&self, speed_kph: u32) -> Result<&StandardSizeBand, SignError> {
        let mut ordered: Vec<&StandardSizeBand> = self.bands.iter().collect();
        ordered.sort_by_key(|b| b.speed_max_kph.unwrap_or(i64::MAX)); // stable, as OrderBy
        ordered
            .into_iter()
            .find(|b| {
                b.speed_max_kph
                    .is_none_or(|max| i64::from(speed_kph) <= max)
            })
            .ok_or_else(|| {
                SignError::Pack(format!("No standard-size band covers {speed_kph} km/h"))
            })
    }

    /// The physical body dimension (mm) a sign family takes at `speed_kph`: the
    /// warning-triangle side, the regulatory-disc diameter, or the supplementary-plate
    /// x-height, read straight from the matching band column. Nothing is interpolated. With
    /// `low_speed_regulatory_variant` a regulatory sign takes the Vol.1 p.21 low-speed /
    /// space-limited disc instead of the band diameter, but only where the pack records one.
    pub fn body_size_mm(
        &self,
        family: SizeFamily,
        speed_kph: u32,
        low_speed_regulatory_variant: bool,
    ) -> Result<f64, SignError> {
        if family == SizeFamily::Regulatory && low_speed_regulatory_variant {
            // Written so that a NaN fails too.
            let recorded = self.low_speed_regulatory_diameter_mm > 0.0;
            if !recorded {
                return Err(SignError::Pack(
                    "pack records no low-speed regulatory diameter (Vol.1 p.21 400 mm)".to_string(),
                ));
            }
            return Ok(self.low_speed_regulatory_diameter_mm);
        }
        let band = self.for_speed(speed_kph)?;
        Ok(match family {
            SizeFamily::Warning => band.warning_triangle_side_mm,
            SizeFamily::Regulatory => band.regulatory_diameter_mm,
            SizeFamily::Supplementary => band.supplementary_x_height_mm,
        })
    }
}

/// A "no" / slash / bar form on a regulatory disc. Colour, direction and count are stated per
/// Vol.2 sign page; the stroke width is a documented gap and is not modelled here (see
/// [`ProvisionalProportion`]). `direction` is a form token: `horizontal`, `tl-br`, `tr-bl` or
/// `cross`.
#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SlashForm {
    pub id: String,
    pub applies_to: String,
    pub colour: String,
    pub direction: String,
    pub count: i64,
}

/// A provisional, non-manual proportion for a warning / regulatory geometry value the Uganda
/// manual does not state numerically (border / ring / slash width, numeral height, R01 bar
/// height). `source` is always labelled "UK-derived (GAP)", never a Uganda fact. `fraction` is
/// of the body side or diameter.
#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ProvisionalProportion {
    pub key: String,
    pub fraction: f64,
    pub source: String,
}

/// Warning / regulatory body-outline proportions. The shape tokens are stated forms (warning
/// triangle apex-up, Give Way point-down, Stop regular octagon, regulatory disc a circle).
/// `stop_orientation` is a gap: Vol.1 sizes the octagon on the circle ladder without stating
/// across-flats or across-corners; the pack assumes across-corners and labels it provisional.
#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RoundelTriangleRules {
    pub warning_shape: String,
    pub give_way_shape: String,
    pub stop_shape: String,
    pub regulatory_disc_shape: String,
    pub stop_orientation: String,
    #[serde(default)]
    pub slashes: Vec<SlashForm>,
    #[serde(default)]
    pub provisional_fractions: Vec<ProvisionalProportion>,
}

impl RoundelTriangleRules {
    /// The provisional fraction with this key (matched exactly, as the C# engine matches it).
    pub fn fraction(&self, key: &str) -> Option<f64> {
        self.provisional_fractions
            .iter()
            .find(|p| p.key == key)
            .map(|p| p.fraction)
    }
}

/// `embeddedShapes` as far as the loader checks it (the embedded-sign geometry itself is not
/// read here).
#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(rename_all = "camelCase")]
struct EmbeddedShapeCheck {
    triangle_border_width: f64,
    roundel_ring_width: f64,
    roundel_diameter: f64,
}

/// The Uganda-only sections of a rule pack.
#[derive(Debug, Clone, PartialEq, Default, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ExtraRules {
    #[serde(default)]
    pub standard_sizes: Option<StandardSizeRules>,
    #[serde(default)]
    pub roundels_triangles: Option<RoundelTriangleRules>,
    #[serde(default)]
    embedded_shapes: Option<EmbeddedShapeCheck>,
}

impl ExtraRules {
    /// Reads the sections from a single-file JSON pack (they are optional there too).
    pub fn load_json(path: &std::path::Path) -> Result<ExtraRules, SignError> {
        let shown = path.display().to_string();
        let text = std::fs::read_to_string(path)
            .map_err(|e| SignError::Pack(format!("Rule pack not found: {shown} ({e})")))?;
        let value: serde_json::Value =
            serde_json::from_str(&crate::pack::strip_json_comments(&text)).map_err(|e| {
                SignError::Pack(format!("Rule pack is not valid JSON: {shown}: {e}"))
            })?;
        Self::from_value(&value, &shown)
    }

    /// Reads the sections from a parsed pack; `path` only names the pack in error messages.
    pub fn from_value(value: &serde_json::Value, path: &str) -> Result<ExtraRules, SignError> {
        let rules: ExtraRules = serde_json::from_value(value.clone())
            .map_err(|e| SignError::Pack(format!("Rule pack is not valid JSON: {path}: {e}")))?;
        rules.validate(path)?;
        Ok(rules)
    }

    /// The checks of C# `RulePackLoader.ValidateOptionalSections` for these sections.
    fn validate(&self, path: &str) -> Result<(), SignError> {
        let bad = |msg: String| Err(SignError::Pack(format!("{msg}: {path}")));
        // Comparisons are written so that a NaN fails too.
        let positive = |v: f64| v > 0.0;

        if let Some(es) = &self.embedded_shapes {
            if !(positive(es.triangle_border_width)
                && positive(es.roundel_ring_width)
                && positive(es.roundel_diameter))
            {
                return bad("Rule pack 'embeddedShapes' values must be positive".into());
            }
        }

        if let Some(sizes) = &self.standard_sizes {
            if sizes.bands.is_empty() {
                return bad("Rule pack 'standardSizes' has no sizeBand".into());
            }
            if !positive(sizes.low_speed_regulatory_diameter_mm) {
                return bad(
                    "Rule pack 'standardSizes.lowSpeedRegulatoryDiameterMm' must be positive"
                        .into(),
                );
            }
            for band in &sizes.bands {
                if band.class.trim().is_empty() {
                    return bad("Rule pack 'standardSizes' band has no class".into());
                }
                if !(positive(band.warning_triangle_side_mm)
                    && positive(band.regulatory_diameter_mm)
                    && positive(band.supplementary_x_height_mm)
                    && positive(band.supplementary_sw_mm)
                    && positive(band.supplementary_cap_mm))
                {
                    return bad(format!(
                        "Rule pack 'standardSizes' band '{}' must have positive dimensions",
                        band.class
                    ));
                }
            }
        }

        if let Some(rt) = &self.roundels_triangles {
            for s in &rt.slashes {
                if s.count <= 0 {
                    return bad(format!(
                        "Rule pack 'roundels' slash '{}' must have a positive count",
                        s.id
                    ));
                }
            }
            for p in &rt.provisional_fractions {
                if !(p.fraction > 0.0 && p.fraction <= 1.0) {
                    return bad(format!(
                        "Rule pack 'roundels' provisional '{}' fraction must be in (0,1]",
                        p.key
                    ));
                }
            }
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn sizes() -> serde_json::Value {
        json!({"standardSizes": {"lowSpeedRegulatoryDiameterMm": 400.0, "bands": [
            {"class": "high", "warningTriangleSideMm": 1100.0, "regulatoryDiameterMm": 900.0,
             "supplementaryXHeightMm": 90.0, "supplementarySwMm": 22.5, "supplementaryCapMm": 126.0},
            {"class": "low", "speedMaxKph": 50, "warningTriangleSideMm": 720.0, "regulatoryDiameterMm": 600.0,
             "supplementaryXHeightMm": 50.0, "supplementarySwMm": 12.5, "supplementaryCapMm": 70.0},
            {"class": "medium", "speedMaxKph": 80, "warningTriangleSideMm": 900.0, "regulatoryDiameterMm": 750.0,
             "supplementaryXHeightMm": 75.0, "supplementarySwMm": 18.75, "supplementaryCapMm": 105.0}]}})
    }

    /// Uganda Vol.1 Table D.1 / D.3 / D.4 as `standard-sizes.xml` records them; the bands are
    /// found by ceiling whatever order the pack lists them in.
    #[test]
    fn the_ladder_is_read_by_speed_with_inclusive_ceilings() {
        let rules = ExtraRules::from_value(&sizes(), "x").unwrap();
        let s = rules.standard_sizes.unwrap();
        assert_eq!(s.for_speed(50).unwrap().class, "low");
        assert_eq!(s.for_speed(51).unwrap().class, "medium");
        assert_eq!(s.for_speed(80).unwrap().class, "medium");
        assert_eq!(s.for_speed(81).unwrap().class, "high");
        assert_eq!(s.for_class("MEDIUM").unwrap().regulatory_diameter_mm, 750.0);
        assert!(s.for_class("none").is_none());
        assert_eq!(
            s.body_size_mm(SizeFamily::Warning, 60, false).unwrap(),
            900.0
        );
        assert_eq!(
            s.body_size_mm(SizeFamily::Regulatory, 100, false).unwrap(),
            900.0
        );
        assert_eq!(
            s.body_size_mm(SizeFamily::Regulatory, 100, true).unwrap(),
            400.0
        );
        assert_eq!(
            s.body_size_mm(SizeFamily::Supplementary, 30, false)
                .unwrap(),
            50.0
        );
        // The low-speed variant only changes regulatory discs.
        assert_eq!(
            s.body_size_mm(SizeFamily::Warning, 30, true).unwrap(),
            720.0
        );
    }

    #[test]
    fn a_ladder_without_an_open_band_does_not_cover_every_speed() {
        let mut v = sizes();
        v["standardSizes"]["bands"]
            .as_array_mut()
            .unwrap()
            .remove(0);
        let s = ExtraRules::from_value(&v, "x")
            .unwrap()
            .standard_sizes
            .unwrap();
        let err = s.for_speed(120).unwrap_err();
        assert_eq!(err.to_string(), "No standard-size band covers 120 km/h");
    }

    #[test]
    fn bad_sections_fail_to_load_with_the_loader_messages() {
        let err =
            |v: serde_json::Value| ExtraRules::from_value(&v, "p.xml").unwrap_err().to_string();
        let mut v = sizes();
        v["standardSizes"]["bands"] = json!([]);
        assert_eq!(err(v), "Rule pack 'standardSizes' has no sizeBand: p.xml");
        let mut v = sizes();
        v["standardSizes"]["lowSpeedRegulatoryDiameterMm"] = json!(0.0);
        assert!(err(v).contains("lowSpeedRegulatoryDiameterMm' must be positive"));
        let mut v = sizes();
        v["standardSizes"]["bands"][1]["supplementarySwMm"] = json!(-1.0);
        assert!(err(v).contains("band 'low' must have positive dimensions"));
        let rt = |slash_count: i64, fraction: f64| {
            json!({"roundelsTriangles": {"warningShape": "a", "giveWayShape": "b", "stopShape": "c",
                "regulatoryDiscShape": "d", "stopOrientation": "e",
                "slashes": [{"id": "R01-bar", "appliesTo": "R01", "colour": "White", "direction": "horizontal", "count": slash_count}],
                "provisionalFractions": [{"key": "k", "fraction": fraction, "source": "UK-derived (GAP)"}]}})
        };
        assert!(ExtraRules::from_value(&rt(1, 1.0), "p.xml").is_ok());
        assert!(err(rt(0, 0.5)).contains("slash 'R01-bar' must have a positive count"));
        assert!(err(rt(1, 0.0)).contains("provisional 'k' fraction must be in (0,1]"));
        assert!(err(rt(1, 1.5)).contains("provisional 'k' fraction must be in (0,1]"));
        let shapes = json!({"embeddedShapes": {"triangleBorderWidth": 2.5, "roundelRingWidth": 0.0, "roundelDiameter": 20.0}});
        assert_eq!(
            err(shapes),
            "Rule pack 'embeddedShapes' values must be positive: p.xml"
        );
    }

    #[test]
    fn a_pack_without_the_sections_has_none() {
        let rules = ExtraRules::from_value(&json!({"id": "X"}), "x").unwrap();
        assert!(rules.standard_sizes.is_none() && rules.roundels_triangles.is_none());
    }
}
