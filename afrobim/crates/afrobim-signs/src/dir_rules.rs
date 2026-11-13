//! Rule-pack sections the destination-column family reads beyond the stack-type ones: the
//! drawn sign border (`signBorder`) and the route-arm lengths (`routeArms`). Port of C#
//! `SignBorderRules` and `RouteArmRules` with the checks `RulePackLoader` makes on them.
//!
//! They are read through [`DirRules`], which `RulePack` flattens in, so the pack file is
//! parsed once and these sections sit beside the others.

use serde::Deserialize;

use crate::pack::PanelRules;
use crate::SignError;

/// The drawn border of a single-panel sign (UK TSM Chapter 7 §2.15, Figure 2-4). Its width and
/// inside corner radius are `PanelRules::border_width` and `PanelRules::corner_radius`; this
/// section adds the outside corner of the plate and switches the border on. A pack without it
/// keeps the plain square ground. Lengths in sw.
#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SignBorderRules {
    /// Radius of the plate's outside corners, concentric with the inside corner of the border.
    pub outer_corner_radius: f64,
    /// Straight segments each 90° corner arc is drawn with (a drawing-fidelity setting, not a
    /// manual value).
    pub segments_per_corner: i64,
}

/// App.1 §2.2.1 route arms (UK TSM Chapter 7 §5.2.1, §5.3.3, §5.4.4). All values in sw.
#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RouteArmRules {
    pub width_primary: f64,
    pub width_secondary: f64,
    pub width_local: f64,
    pub min_length_vertical: f64,
    pub min_length_horizontal: f64,
    pub arm_to_top_border: f64,
    pub arm_to_bottom_border: f64,
}

/// The sections of a rule pack that direction, confirmatory, place, lane and diversion signs
/// read in addition to those the stack-type layout reads.
#[derive(Debug, Clone, PartialEq, Default, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DirRules {
    #[serde(default)]
    pub sign_border: Option<SignBorderRules>,
    /// Required by the C# loader. Optional here so a pack written for stack-type signs alone
    /// still loads; the lane layout, which reads it, reports its absence ([`Self::route_arms`]).
    #[serde(default)]
    pub route_arms: Option<RouteArmRules>,
}

impl DirRules {
    /// The `routeArms` section, or the C# loader's error for a pack without one.
    pub(crate) fn route_arms(&self, pack_id: &str) -> Result<&RouteArmRules, SignError> {
        self.route_arms.as_ref().ok_or_else(|| {
            SignError::Pack(format!(
                "Rule pack is missing section 'routeArms': {pack_id}"
            ))
        })
    }

    /// The checks of C# `RulePackLoader.ValidateOptionalSections` for `signBorder`.
    pub(crate) fn validate(&self, panel: &PanelRules, path: &str) -> Result<(), SignError> {
        if let Some(sb) = &self.sign_border {
            if sb.outer_corner_radius < panel.corner_radius {
                return Err(SignError::Pack(format!(
                    "Rule pack 'signBorder.outerCornerRadius' must be at least 'panel.cornerRadius': {path}"
                )));
            }
            if sb.segments_per_corner < 1 {
                return Err(SignError::Pack(format!(
                    "Rule pack 'signBorder.segmentsPerCorner' must be at least 1: {path}"
                )));
            }
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use crate::pack::tests::uk_pack;
    use crate::pack::RulePack;
    use crate::SignError;

    const MINIMAL: &str = r#"{"id":"X","name":"X","source":"X",
        "textSizes":[{"speedMaxKph":null,"xHeightMm":100,"swMm":25,"capHeightMm":140}],
        "panel":{"borderWidth":1.5,"cornerRadius":1.5,"padTop":2.5,"padSide":2.5,"padBottom":1.5,"wordSpacing":2.5,"textTileHeight":8},
        "fonts":{"onDark":{"face":"m","file":"m.ttf"},"onLight":{"face":"h","file":"h.ttf"}},
        "constraints":{"maxDestinations":6,"maxDestinationsPerDirection":6,"nearerDestinationAbove":true,"uniformTextSize":true}"#;

    /// UK TSM Chapter 7 §2.15 / Figure 2-4 and §5.3.3 / Figure 5-7, as the UK pack states them.
    #[test]
    fn the_uk_pack_carries_the_border_and_route_arm_sections() {
        let p = uk_pack();
        let sb = p.dir.sign_border.as_ref().unwrap();
        assert_eq!((sb.outer_corner_radius, sb.segments_per_corner), (3.0, 12));
        let arms = p.dir.route_arms("UK").unwrap();
        assert_eq!(arms.min_length_vertical, 12.5);
        assert_eq!(arms.min_length_horizontal, 12.0);
        assert_eq!(arms.width_primary, 6.0);
    }

    #[test]
    fn a_pack_without_the_sections_loads_and_reports_missing_route_arms_on_use() {
        let p = RulePack::from_json(&format!("{MINIMAL}}}"), "x.json").unwrap();
        assert!(p.dir.sign_border.is_none());
        let err = p.dir.route_arms("X").unwrap_err();
        assert!(matches!(err, SignError::Pack(_)));
        assert!(err.to_string().contains("'routeArms'"), "{err}");
    }

    #[test]
    fn a_border_tighter_than_the_inside_corner_or_without_segments_fails_to_load() {
        let load = |border: &str| {
            RulePack::from_json(&format!("{MINIMAL},\"signBorder\":{border}}}"), "x.json")
        };
        assert!(load(r#"{"outerCornerRadius":3.0,"segmentsPerCorner":12}"#).is_ok());
        let err = load(r#"{"outerCornerRadius":1.0,"segmentsPerCorner":12}"#).unwrap_err();
        assert!(err.to_string().contains("outerCornerRadius"), "{err}");
        let err = load(r#"{"outerCornerRadius":3.0,"segmentsPerCorner":0}"#).unwrap_err();
        assert!(err.to_string().contains("segmentsPerCorner"), "{err}");
    }
}
