//! The rule-pack sections the map-type layout reads: route arms, text-to-arm spacing, the
//! Figure 5-7 route symbol, the Figure 5-13 roundabout and the drawn sign border.
//!
//! Port of the C# `SignCore.Rules` records `RouteArmRules`, `MapTypeSpacing`,
//! `RoundaboutRules`, `MapSymbolRules` and `SignBorderRules`, and of the checks
//! `RulePackLoader.ValidateOptionalSections` makes on them. As in `pack.rs`, no value is
//! defaulted: a length the layout needs comes from the pack or the pack fails to load. Only
//! the options the C# records themselves default (`sideArmReachFrom`, `exitMinToPoint`,
//! `stubsCited`, `provisional` and the optional roundabout values) may be left out.

use serde::Deserialize;

use crate::pack::RulePack;
use crate::SignError;

/// App.1 §2.2.1; UK TSM Chapter 7 §5.2.1 arm widths (6 sw primary and motorway, 4 sw numbered
/// or important non-primary, 2.5 sw local), §5.3.3 / Figure 5-7 vertical arm minimum, §5.4.4
/// other arms minimum, Figure 5-3 through arm to the top border. All values in sw.
#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RouteArmRules {
    pub width_primary: f64,
    pub width_secondary: f64,
    pub width_local: f64,
    pub min_length_vertical: f64,
    pub min_length_horizontal: f64,
    pub arm_to_top_border: f64,
    /// Only read by the original map-type engine (a pack without `mapSymbol`), which is not
    /// ported.
    pub arm_to_bottom_border: f64,
}

/// App.1 §2.2.1 text-to-arm spacing (UK TSM Chapter 7 Figure 5-2). All values in sw.
#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MapTypeSpacing {
    pub tile_to_vertical_arm_top: f64,
    pub tile_to_vertical_arm_side: f64,
    pub tile_to_horizontal_arm_bottom: f64,
    pub tile_to_arm_top: f64,
    pub tile_to_diagonal_arm_end: f64,
}

/// The roundabout ring (Uganda App.1 Figure 2; UK TSM Chapter 7 Figure 5-13). All values in
/// sw. The optional values drive the filled map-type roundabout symbol drawn by a pack with a
/// `mapSymbol` section (UK TSM Chapter 7 paras 5.39–5.40, Figures 5-13 and 5-14).
#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RoundaboutRules {
    /// Inside diameter of the ring (Figure 5-13 "7r": 14).
    pub inner_diameter: f64,
    /// Outside diameter of the ring (Figure 5-13 "12r": 24), so the ring is 5 sw wide whatever
    /// the route status (para 5.39).
    pub outer_diameter: f64,
    /// Minimum exit arm, from the outside of the ring on the arm's axis to its point (para
    /// 5.39, Figure 5-13 "12 min (all exit arms)").
    #[serde(default)]
    pub exit_min_length: Option<f64>,
    /// Minimum approach arm, from the outside of the ring on its axis to its base (para 5.39,
    /// Figure 5-13 "8.5 min").
    #[serde(default)]
    pub approach_min_length: Option<f64>,
    /// Bearing of the first (radial) cut-out edge from the approach arm's axis, anticlockwise
    /// towards the right exit (Figure 5-13: 30°).
    #[serde(default)]
    pub cut_out_from_approach_deg: Option<f64>,
    /// Angle of the cut-out between its two radial edges (Figure 5-13: 30°).
    #[serde(default)]
    pub cut_out_angle_deg: Option<f64>,
    /// Recommended minimum cut-out angle (para 5.40: 25°).
    #[serde(default)]
    pub cut_out_min_angle_deg: Option<f64>,
    /// Legend blocks stay outside this circle about the ring's centre (Figure 5-13 "14.5r",
    /// the "shall not cross this line" boundary).
    #[serde(default)]
    pub legend_keep_out_radius: Option<f64>,
    /// Outside of the ring to the inside of the border where no arm leaves that side (Figure
    /// 5-13 sub-figures: 4 to the top border with no ahead arm, 4 to the right border with no
    /// right arm).
    #[serde(default)]
    pub ring_to_border: Option<f64>,
    /// Alternative to `legend_keep_out_radius`: side legend tiles stay this far outside the
    /// vertical tangents of the ring (Uganda App.1 Figure 2: 2.0). A pack gives exactly one of
    /// the two.
    #[serde(default)]
    pub legend_to_ring_side: Option<f64>,
    /// Values measured from a drawing or borrowed rather than stated by the manual; when set,
    /// every roundabout symbol raises an Info diagnostic quoting it.
    #[serde(default)]
    pub provisional: Option<String>,
}

impl RoundaboutRules {
    /// True when every value the filled roundabout symbol needs is given, with exactly one
    /// legend clearance rule.
    pub fn has_symbol_rules(&self) -> bool {
        self.exit_min_length.is_some()
            && self.approach_min_length.is_some()
            && self.cut_out_from_approach_deg.is_some()
            && self.cut_out_angle_deg.is_some()
            && self.cut_out_min_angle_deg.is_some()
            && (self.legend_keep_out_radius.is_some() != self.legend_to_ring_side.is_some())
            && self.ring_to_border.is_some()
    }
}

fn yes() -> bool {
    true
}

/// The Chapter 7 map-type route symbol (S12-7-1, Figure 5-7). Lengths in sw.
#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MapSymbolRules {
    /// Fillet joining two arms (§5.7.3 "1r": 1).
    pub fillet_radius: f64,
    /// Half angle of a pointed arm end, between a flank and the arm axis (Figure 5-7: 45°).
    pub tip_half_angle_deg: f64,
    /// How far along its destination block a horizontal arm's point reaches (§5.3.4, Figure
    /// 5-2 "2/3 x L").
    pub side_arm_reach: f64,
    /// Base of the approach arm above the bottom border (§5.7.2, Figure 5-7: 1.5).
    pub approach_to_bottom_border: f64,
    /// A panel, patch or symbol, or a tiled line with a descender, above the point of a
    /// vertical arm (Figure 5-2, §5.7.5, §5.7.6 d: 1.5).
    pub panel_to_arm_tip: f64,
    /// The square end of a stub to the nearer border (Figure 5-5: 2.5).
    pub stub_to_border: f64,
    /// A side stub above the bottom border when no legend sits below it (Figure 5-5 "4 min").
    pub stub_to_bottom_border: f64,
    /// What `side_arm_reach` is a fraction of: `"block"` (default; UK Figure 5-2 "2/3 x L",
    /// the destination block's length, from the vertical arm side) or `"border"` (Uganda App.1
    /// Figures 2-3 "b/3": the point is a third of the space b between the inside of the border
    /// and the vertical arm or ring, from the border).
    #[serde(default)]
    pub side_arm_reach_from: Option<String>,
    /// True when the minimum horizontal arm length runs to the arm's point (Uganda Figure 3
    /// "Min 12 sw"); false (default) when it runs along the arm's shorter side, excluding the
    /// point (UK §5.4.4).
    #[serde(default)]
    pub exit_min_to_point: bool,
    /// False when the manual has no stub rule and the stub values are borrowed; drawing a stub
    /// then raises an Info diagnostic.
    #[serde(default = "yes")]
    pub stubs_cited: bool,
    /// Values measured from a drawing or borrowed rather than stated by the manual; when set,
    /// every map-type symbol raises an Info diagnostic quoting it.
    #[serde(default)]
    pub provisional: Option<String>,
}

impl MapSymbolRules {
    /// True when the side-arm point is placed from the border (Uganda "b/3").
    pub fn reach_from_border(&self) -> bool {
        self.side_arm_reach_from
            .as_deref()
            .is_some_and(|s| s.eq_ignore_ascii_case("border"))
    }

    /// Length of the pointed end of an arm `arm_width` wide.
    pub fn tip_length(&self, arm_width: f64) -> f64 {
        arm_width / 2.0 / (self.tip_half_angle_deg * std::f64::consts::PI / 180.0).tan()
    }
}

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

/// The checks of C# `RulePackLoader.ValidateOptionalSections` on the sections above, so a bad
/// hand-authored pack fails at load rather than mid-layout.
pub(crate) fn validate(pack: &RulePack, path: &str) -> Result<(), SignError> {
    let bad = |msg: &str| Err(SignError::Pack(format!("{msg}: {path}")));

    // The Chapter 7 map-type engine draws the filled roundabout symbol, which needs every
    // Figure 5-13 value; a missing one must fail loudly rather than be guessed.
    if let (Some(_), Some(rb)) = (&pack.map_symbol, &pack.roundabout) {
        if !rb.has_symbol_rules() {
            return bad(
                "Rule pack 'roundabout' needs exitMinLength, approachMinLength, cutOutFromApproachDeg, \
                 cutOutAngleDeg, cutOutMinAngleDeg, ringToBorder and exactly one of legendKeepOutRadius \
                 or legendToRingSide with 'mapSymbol'",
            );
        }
        if rb.inner_diameter <= 0.0 || rb.outer_diameter <= rb.inner_diameter {
            return bad("Rule pack 'roundabout' needs 0 < innerDiameter < outerDiameter");
        }
        let (from, angle) = (
            rb.cut_out_from_approach_deg.unwrap_or_default(),
            rb.cut_out_angle_deg.unwrap_or_default(),
        );
        if angle <= 0.0 || from < 0.0 || from + angle > 90.0 {
            return bad(
                "Rule pack 'roundabout' cut-out must lie between the approach and the right exit",
            );
        }
    }

    if let Some(ms) = &pack.map_symbol {
        if ms.fillet_radius < 0.0
            || ms.approach_to_bottom_border < 0.0
            || ms.panel_to_arm_tip < 0.0
            || ms.stub_to_border < 0.0
            || ms.stub_to_bottom_border < 0.0
        {
            return bad("Rule pack 'mapSymbol' lengths must be non-negative");
        }
        if ms.tip_half_angle_deg <= 0.0 || ms.tip_half_angle_deg >= 90.0 {
            return bad("Rule pack 'mapSymbol.tipHalfAngleDeg' must be in (0,90)");
        }
        if ms.side_arm_reach <= 0.0 || ms.side_arm_reach > 1.0 {
            return bad("Rule pack 'mapSymbol.sideArmReach' must be in (0,1]");
        }
        if let Some(from) = &ms.side_arm_reach_from {
            if !from.eq_ignore_ascii_case("block") && !from.eq_ignore_ascii_case("border") {
                return bad("Rule pack 'mapSymbol.sideArmReachFrom' must be 'block' or 'border'");
            }
        }
    }

    if let Some(sb) = &pack.sign_border {
        if sb.outer_corner_radius < pack.panel.corner_radius {
            return bad(
                "Rule pack 'signBorder.outerCornerRadius' must be at least 'panel.cornerRadius'",
            );
        }
        if sb.segments_per_corner < 1 {
            return bad("Rule pack 'signBorder.segmentsPerCorner' must be at least 1");
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::pack::tests::uk_pack;

    #[test]
    fn uk_pack_loads_the_map_sections() {
        let p = uk_pack();
        let a = p.route_arms.as_ref().unwrap();
        assert_eq!(
            (a.width_primary, a.width_secondary, a.width_local),
            (6.0, 4.0, 2.5)
        );
        assert_eq!(
            (a.min_length_vertical, a.min_length_horizontal),
            (12.5, 12.0)
        );
        assert_eq!(a.arm_to_top_border, 4.0);

        let s = p.map_type_spacing.as_ref().unwrap();
        assert_eq!(
            (
                s.tile_to_vertical_arm_top,
                s.tile_to_vertical_arm_side,
                s.tile_to_horizontal_arm_bottom
            ),
            (0.0, 4.0, 2.5)
        );

        let ms = p.map_symbol.as_ref().unwrap();
        assert_eq!((ms.fillet_radius, ms.tip_half_angle_deg), (1.0, 45.0));
        assert!(!ms.reach_from_border() && !ms.exit_min_to_point && ms.stubs_cited);
        assert_eq!(ms.provisional, None);
        // Figure 5-7: 45° flanks, so the point is half the arm width long.
        assert!((ms.tip_length(6.0) - 3.0).abs() < 1e-12);

        let rb = p.roundabout.as_ref().unwrap();
        assert!(rb.has_symbol_rules());
        assert_eq!((rb.inner_diameter, rb.outer_diameter), (14.0, 24.0));
        assert_eq!(rb.legend_keep_out_radius, Some(14.5));
        assert_eq!(rb.legend_to_ring_side, None);

        let sb = p.sign_border.as_ref().unwrap();
        assert_eq!((sb.outer_corner_radius, sb.segments_per_corner), (3.0, 12));
    }

    #[test]
    fn a_roundabout_needs_exactly_one_legend_clearance_rule() {
        let mut rb = uk_pack().roundabout.unwrap();
        assert!(rb.has_symbol_rules());
        rb.legend_to_ring_side = Some(2.0); // both given
        assert!(!rb.has_symbol_rules());
        rb.legend_keep_out_radius = None; // only the Uganda rule
        assert!(rb.has_symbol_rules());
        rb.legend_to_ring_side = None; // neither
        assert!(!rb.has_symbol_rules());
        rb.legend_keep_out_radius = Some(14.5);
        rb.ring_to_border = None;
        assert!(!rb.has_symbol_rules());
    }

    fn message(p: &RulePack) -> String {
        validate(p, "x.json").unwrap_err().to_string()
    }

    #[test]
    fn bad_map_sections_fail_validation() {
        let good = uk_pack();
        assert!(validate(&good, "x.json").is_ok());

        let mut p = good.clone();
        p.roundabout.as_mut().unwrap().exit_min_length = None;
        assert!(
            message(&p).contains("needs exitMinLength"),
            "{}",
            message(&p)
        );
        // Without a mapSymbol section the original ring needs none of them.
        p.map_symbol = None;
        assert!(validate(&p, "x.json").is_ok());

        let mut p = good.clone();
        p.roundabout.as_mut().unwrap().outer_diameter = 14.0;
        assert!(message(&p).contains("0 < innerDiameter < outerDiameter"));

        let mut p = good.clone();
        p.roundabout.as_mut().unwrap().cut_out_angle_deg = Some(70.0);
        assert!(message(&p).contains("between the approach and the right exit"));

        let mut p = good.clone();
        p.map_symbol.as_mut().unwrap().stub_to_border = -1.0;
        assert!(message(&p).contains("'mapSymbol' lengths must be non-negative"));

        let mut p = good.clone();
        p.map_symbol.as_mut().unwrap().tip_half_angle_deg = 90.0;
        assert!(message(&p).contains("tipHalfAngleDeg"));

        let mut p = good.clone();
        p.map_symbol.as_mut().unwrap().side_arm_reach = 1.5;
        assert!(message(&p).contains("sideArmReach' must be in (0,1]"));

        let mut p = good.clone();
        p.map_symbol.as_mut().unwrap().side_arm_reach_from = Some("kerb".into());
        assert!(message(&p).contains("sideArmReachFrom"));
        p.map_symbol.as_mut().unwrap().side_arm_reach_from = Some("Border".into());
        assert!(validate(&p, "x.json").is_ok());
        assert!(p.map_symbol.as_ref().unwrap().reach_from_border());

        let mut p = good.clone();
        p.sign_border.as_mut().unwrap().outer_corner_radius = 1.0;
        assert!(message(&p).contains("outerCornerRadius"));

        let mut p = good;
        p.sign_border.as_mut().unwrap().segments_per_corner = 0;
        assert!(
            message(&p).ends_with("must be at least 1: x.json"),
            "{}",
            message(&p)
        );
    }
}
