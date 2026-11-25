// SPDX-License-Identifier: LicenseRef-AfroBIM-Proprietary
// Copyright (c) 2026 AfroBIM. All rights reserved.
//! Map-type advance direction signs drawn to UK TSM Chapter 7 §5 (used when the rule pack has
//! a `mapSymbol` section). Port of C# `MapSymbolLayout`, of the `MapSignLayout` dispatch that
//! reaches it, and of the part of `SignLayout.Layout` that follows a map-type layout.
//!
//! The crossroads and T-junction symbols (S12-7-1) are one filled outline built to the Figure
//! 5-7 working drawing: the approach arm rises from 1.5 sw above the bottom border (§5.7.2),
//! arms join with 1 sw fillets (§5.7.3), arm ends are pointed at 45° (Figure 5-7) and each
//! horizontal arm reaches two thirds along its destination block (§5.3.4). A direction with no
//! destinations gets a square-ended stub as long as it is wide (§5.5.1, Figure 5-5).
//!
//! Destinations are laid out by [`DestinationColumn`], the stack-type engine, so route numbers,
//! patches and panels follow the same rules as on stack-type signs (Figure 5-7 draws its blocks
//! with the §3 rules). Symbols and distances are not laid out on map-type signs, as in C#.
//!
//! The roundabout is the filled ring symbol of Figure 5-13 with the same destination blocks.
//!
//! Not ported:
//! - the original map-type engine of C# `MapSignLayout` (stroked arms and a two-circle ring,
//!   Uganda App.1 Figures 2-4), which a pack without a `mapSymbol` section uses. It is
//!   reported as [`SignError::Unsupported`].
//!
//! Embedded warning / regulatory signs are drawn over the vertical arm or inside the ring (C#
//! `EmbedOverArm`; `embedded.rs`).

use afrobim_draw::{Drawing, Extents, Pt2};

use crate::column::{DestinationColumn, Edge};
use crate::map_plate;
use crate::map_rules::{MapSymbolRules, MapTypeSpacing, RoundaboutRules, RouteArmRules};
use crate::measurer::{self, TextTileMeasurer};
use crate::model::{
    Destination, Diagnostic, Direction, RoadClass, RouteStatus, Severity, SignDefinition, SignKind,
};
use crate::pack::{self, RulePack};
use crate::prim::Prim;
use crate::stack::{resolve_palette, scheme_of, validate_constraints, Palette, RawLayout};
use crate::{outliner, overlap, SignEngine, SignError, SignLayout};

/// Segments in each quarter-circle fillet.
const FILLET_SEGMENTS: usize = 8;

/// The ring's arcs are drawn through every whole degree of bearing (a 12r arc deviates from
/// its 1° chords by under 0.0005 sw), so the ring's top, bottom and sides are exact vertices.
const RING_STEP_DEG: f64 = 1.0;

type Pts = Vec<(f64, f64)>;

/// True for the three map-type kinds this module lays out.
pub(crate) fn is_map_type(kind: SignKind) -> bool {
    matches!(
        kind,
        SignKind::MapTypeCrossroads | SignKind::MapTypeTee | SignKind::MapTypeRoundabout
    )
}

/// C# `SignFamily.Label` for the map-type kinds.
fn family_label(kind: SignKind) -> &'static str {
    match kind {
        SignKind::MapTypeRoundabout => "Map · roundabout",
        SignKind::MapTypeTee => "Map · T-intersection",
        _ => "Map · crossroads",
    }
}

fn info(code: &str, message: String, source: &str) -> Diagnostic {
    Diagnostic {
        severity: Severity::Info,
        code: code.to_string(),
        message,
        source: source.to_string(),
    }
}

impl SignEngine {
    /// Lays a map-type sign out in stroke widths, lettering as glyph outlines: C#
    /// `SignLayout.Layout` for the map-type kinds, then `TextOutliner.Apply`. The steps after
    /// the layout itself are those `SignEngine::layout` takes for a stack-type sign.
    pub(crate) fn layout_map_type(&self, sign: &SignDefinition) -> Result<SignLayout, SignError> {
        let pack = &self.pack;
        let measurer = TextTileMeasurer::new(&self.medium, &pack.panel, Some(&self.heavy));

        let mut result = MapSymbolLayout::new(pack, &measurer)?.layout(sign)?;
        let mut diags = std::mem::take(&mut result.diagnostics);

        // A character the pack's tile table does not list was measured by the font; say so.
        for prim in &result.prims {
            let Prim::Text(t) = prim else { continue };
            let missing = measurer.font_fallbacks(&t.text, t.face)?;
            let (false, Some(table)) =
                (missing.is_empty(), measurer::table_for(&pack.panel, t.face))
            else {
                continue;
            };
            let listed: Vec<String> = missing.iter().map(|c| format!("'{c}'")).collect();
            diags.push(Diagnostic::warning(
                "TILE_WIDTH_FROM_FONT",
                format!(
                    "'{}' in {}: {} not in the tile table; the font's advance width is used",
                    t.text,
                    t.face.name(),
                    listed.join(" ")
                ),
                &table.source,
            ));
        }

        // Geometry overlap check: legend tiles against the route symbol the layout registered,
        // and both against the border.
        diags.extend(overlap::check(pack, &measurer, &result)?);

        // Route numbers, patches and panels are drawn by the Chapter 7 map-type layout in a
        // pack with block rules. Without them, say so rather than drop them silently.
        let draws_route_numbers = pack.map_symbol.is_some() && pack.blocks.is_some();
        if !draws_route_numbers
            && sign.destinations.iter().any(|d| {
                d.route_number.as_ref().is_some_and(|n| !n.is_empty())
                    || d.route != RouteStatus::Unspecified
            })
        {
            diags.push(Diagnostic::warning(
                "ROUTE_NUMBER_NOT_DRAWN",
                format!(
                    "Route numbers, patches and panels are not drawn on {} signs yet",
                    family_label(sign.kind)
                ),
                pack.clause("routeNumbers", &pack.source),
            ));
        }

        // UK TSM Chapter 7 §10.11: most route numbers on motorway signs use the enlarged
        // Motorway alphabet, which the engine does not have, so they are drawn in Transport
        // Medium. Bracketed numbers and numbers on all-purpose road panels are Transport Medium
        // anyway (§10.2, §10.15-10.17); flag only an unbracketed number standing on the blue
        // motorway ground.
        if let (true, Some(blocks)) = (draws_route_numbers, &pack.blocks) {
            let is_motorway =
                scheme_of(sign, pack).is_some_and(|s| s.eq_ignore_ascii_case("motorway"));
            if let (true, Some(motorway)) = (
                is_motorway,
                pack.palettes.as_ref().and_then(|p| p.find("motorway")),
            ) {
                let mut on_ground: Vec<&str> = Vec::new();
                for d in &sign.destinations {
                    let Some(n) = d.route_number.as_deref().filter(|n| !n.is_empty()) else {
                        continue;
                    };
                    let on_motorway_ground = blocks
                        .colour_for(d.route)
                        .is_none_or(|panel| panel == motorway.ground);
                    if n.trim_start().starts_with('(') || !on_motorway_ground {
                        continue;
                    }
                    let n = n.trim();
                    if !on_ground.iter().any(|o| o.eq_ignore_ascii_case(n)) {
                        on_ground.push(n);
                    }
                }
                if !on_ground.is_empty() {
                    diags.push(Diagnostic::warning(
                        "MOTORWAY_ALPHABET_NOT_MODELLED",
                        format!(
                            "Route number(s) {} drawn in Transport Medium; the Motorway alphabet and its block spacing \
                             (§10.11-10.12) are not modelled",
                            on_ground.join(", ")
                        ),
                        pack.clause("motorwayAlphabet", &pack.source),
                    ));
                }
            }
        }

        // Lettering as Transport glyph outlines.
        let outliner = outliner::TextOutliner {
            measurer: &measurer,
            medium: &self.medium,
            heavy: &self.heavy,
            panel: &pack.panel,
        };
        let primitives = outliner.apply(result.prims, &mut diags)?;

        // The x-height / stroke-width band: the family's own table when the pack has one (UK
        // Appendix E), otherwise the pack-wide speed table.
        let size = pack::text_size_for_family(pack, sign.kind.family_token(), sign.speed_kph)?;

        Ok(SignLayout {
            drawing: Drawing {
                primitives,
                extents: Extents {
                    min: Pt2::new(0.0, 0.0),
                    max: Pt2::new(result.width, result.height),
                },
            },
            sw_mm: size.sw_mm,
            x_height_mm: size.x_height_mm,
            diagnostics: diags,
        })
    }
}

/// One arm of the roundabout symbol: its bearing from the ring centre (anticlockwise from +X),
/// width, and the distance from the centre to its point (exit) or base (approach).
#[derive(Debug, Clone, Copy, PartialEq)]
struct RingArm {
    bearing_deg: f64,
    width: f64,
    reach: f64,
    pointed: bool,
}

/// The roundabout values of [`RoundaboutRules`] the filled symbol needs, all present.
struct RingRules {
    outer_r: f64,
    inner_r: f64,
    exit_min: f64,
    approach_min: f64,
    cut_out_from_approach_deg: f64,
    cut_out_angle_deg: f64,
    cut_out_min_angle_deg: f64,
    keep_out: Option<f64>,
    ring_side: Option<f64>,
    ring_gap: f64,
}

pub(crate) struct MapSymbolLayout<'a> {
    pack: &'a RulePack,
    measurer: &'a TextTileMeasurer<'a>,
    ms: &'a MapSymbolRules,
    arms: &'a RouteArmRules,
    spacing: &'a MapTypeSpacing,
    roundabout: &'a RoundaboutRules,
}

impl<'a> MapSymbolLayout<'a> {
    /// `Unsupported` for a pack without a `mapSymbol` section: the original map-type engine it
    /// would use (C# `MapSignLayout`, Uganda App.1 Figures 2-4) is not ported. A pack error
    /// for one that lacks a section the C# loader requires of every pack.
    pub fn new(pack: &'a RulePack, measurer: &'a TextTileMeasurer<'a>) -> Result<Self, SignError> {
        let Some(ms) = &pack.map_symbol else {
            return Err(SignError::Unsupported(format!(
                "map-type signs for rule pack '{}': it has no 'mapSymbol' section, and only the Chapter 7 \
                 map-type symbol layout (UK TSM Chapter 7 §5, Figures 5-7 and 5-13) is ported",
                pack.id
            )));
        };
        let missing =
            |section: &str| SignError::Pack(format!("Rule pack is missing section '{section}'"));
        let arms = pack
            .route_arms
            .as_ref()
            .ok_or_else(|| missing("routeArms"))?;
        let spacing = pack
            .map_type_spacing
            .as_ref()
            .ok_or_else(|| missing("mapTypeSpacing"))?;
        let roundabout = pack
            .roundabout
            .as_ref()
            .ok_or_else(|| missing("roundabout"))?;
        Ok(MapSymbolLayout {
            pack,
            measurer,
            ms,
            arms,
            spacing,
            roundabout,
        })
    }

    pub fn layout(&self, sign: &SignDefinition) -> Result<RawLayout, SignError> {
        match sign.kind {
            SignKind::MapTypeCrossroads => self.junction(sign, false),
            SignKind::MapTypeTee => self.junction(sign, true),
            SignKind::MapTypeRoundabout => self.roundabout(sign),
            other => Err(SignError::Unsupported(format!(
                "Map-type layout does not support {}",
                other.name()
            ))),
        }
    }

    // ─── shared helpers ───────────────────────────────────────────────────────────

    fn arm_width_for(&self, road_class: RoadClass) -> f64 {
        let a = self.arms;
        match road_class {
            RoadClass::Primary => a.width_primary,
            RoadClass::Secondary => a.width_secondary,
            RoadClass::Local => a.width_local,
        }
    }

    /// §5.2.1: an arm's width follows the status of its route: 6 sw for motorways and primary
    /// routes, 4 sw for non-primary routes (Figure 5-7 f: "4 sw wide because they indicate
    /// numbered non-primary routes"). The highest status among the direction's destinations
    /// sets it; with none given the arm keeps the approach width. The 2.5 sw local width
    /// cannot be told from the schedule, so it is never chosen here.
    fn side_arm_width(&self, dests: &[Destination], approach_width: f64) -> f64 {
        let a = self.arms;
        if dests
            .iter()
            .any(|d| matches!(d.route, RouteStatus::Motorway | RouteStatus::Primary))
        {
            return a.width_primary;
        }
        if dests.iter().any(|d| d.route == RouteStatus::NonPrimary) {
            return a.width_secondary;
        }
        approach_width
    }

    /// One direction's blocks, without symbols or distances; `None` for no destinations.
    fn column(
        &self,
        dests: &[Destination],
        palette: &Palette,
    ) -> Result<Option<DestinationColumn>, SignError> {
        if dests.is_empty() {
            return Ok(None);
        }
        let plain: Vec<Destination> = dests
            .iter()
            .map(|d| Destination {
                symbol_id: None,
                distance: None,
                ..d.clone()
            })
            .collect();
        let refs: Vec<&Destination> = plain.iter().collect();
        DestinationColumn::build(
            &refs,
            palette.ground,
            palette.lettering,
            palette.face,
            self.pack,
            self.measurer,
            &|_| 0.0,
            false,
            false,
        )
        .map(Some)
    }

    /// Bottom border to the foot of a column: 1.5 sw below tiles (§4.2.1), 2.5 sw below a
    /// panel or patch (Figure 5-7, as on a stack-type sign).
    fn bottom_pad(&self, c: &DestinationColumn) -> f64 {
        match (c.bottom, &self.pack.blocks) {
            (Edge::Patch, Some(b)) => b.panel_pad_bottom_patch,
            _ => self.pack.panel.pad_bottom,
        }
    }

    /// Figure 5-2: tiles sit on the point of a vertical arm; a panel, patch, symbol or
    /// descender line is 1.5 sw above it.
    fn gap_above_arm_tip(&self, c: &DestinationColumn) -> f64 {
        if c.bottom != Edge::Tiles || c.bottom_descender {
            self.ms.panel_to_arm_tip
        } else {
            self.spacing.tile_to_vertical_arm_top
        }
    }

    /// Column-to-vertical-arm gap: at least 4 sw (Figure 5-2), widened when needed so the arm,
    /// whose point is two thirds along the block, is at least 12 sw long along its shorter
    /// side (§5.4.4).
    fn side_gap(&self, c: &DestinationColumn, arm_width: f64) -> f64 {
        let ms = self.ms;
        let min_arm = self.arms.min_length_horizontal
            + if ms.exit_min_to_point {
                0.0
            } else {
                ms.tip_length(arm_width)
            };
        if !ms.reach_from_border() {
            return self
                .spacing
                .tile_to_vertical_arm_side
                .max(min_arm - ms.side_arm_reach * c.width);
        }

        // Uganda App.1 Figures 2-3: the point is b/3 from the inside of the border, b being the
        // space from the border to the vertical arm, so the arm is (2/3) b long; b is widened
        // until that reaches the minimum. The block is ranged against the border (pad) as before.
        let pad = self.pack.panel.pad_side;
        self.spacing
            .tile_to_vertical_arm_side
            .max(min_arm / ms.side_arm_reach - pad - c.width)
    }

    /// Info diagnostics for values the pack marks as measured or borrowed.
    fn add_provisional(&self, diags: &mut Vec<Diagnostic>, note: Option<&str>, code: &str) {
        let Some(note) = note.filter(|n| !n.trim().is_empty()) else {
            return;
        };
        diags.push(info(
            code,
            note.to_string(),
            self.pack.clause("mapType", &self.pack.source),
        ));
    }

    // ─── crossroads and T-junction (S12-7-1, Figure 5-7) ──────────────────────────

    fn junction(&self, sign: &SignDefinition, tee: bool) -> Result<RawLayout, SignError> {
        let mut diags = validate_constraints(sign, self.pack);
        let p = &self.pack.panel;
        let a = self.arms;
        let s = self.spacing;
        let ms = self.ms;
        let palette = resolve_palette(sign, self.pack);
        let border = p.border_width;
        let inset = border + p.pad_side;

        let ahead = by_order(sign, Direction::Ahead);
        let left = by_order(sign, Direction::Left);
        let right = by_order(sign, Direction::Right);

        if tee && !ahead.is_empty() {
            diags.push(Diagnostic::warning(
                "TEE_AHEAD_IGNORED",
                format!(
                    "{} forward destination(s) on a T-intersection: no through route exists",
                    ahead.len()
                ),
                self.pack
                    .clause("mapTee", "Uganda TSM Vol.2 App.1 §2.2 (Figure 4)"),
            ));
        }

        let ahead_col = if tee {
            None
        } else {
            self.column(&ahead, &palette)?
        };
        let left_col = self.column(&left, &palette)?;
        let right_col = self.column(&right, &palette)?;

        // Arm widths (§5.2.1). The vertical arm (approach and ahead) follows the approach road.
        let wv = self.arm_width_for(sign.approach_class);
        let wl = if left_col.is_none() {
            wv
        } else {
            self.side_arm_width(&left, wv)
        };
        let wr = if right_col.is_none() {
            wv
        } else {
            self.side_arm_width(&right, wv)
        };
        let reach = ms.side_arm_reach;

        // ── Horizontal. xv0 is the left face of the vertical arm.
        let gap_l = left_col.as_ref().map_or(0.0, |c| self.side_gap(c, wl));
        let gap_r = right_col.as_ref().map_or(0.0, |c| self.side_gap(c, wr));
        let mut xv0 = match &left_col {
            None => border + ms.stub_to_border + wl, // Figure 5-5: stub end 2.5 sw from the border
            Some(c) => inset + c.width + gap_l,
        };
        // From the right face of the vertical arm to the sign's right edge.
        let right_reach = match &right_col {
            None => wr + ms.stub_to_border + border,
            Some(c) => gap_r + c.width + p.pad_side + border,
        };

        let ahead_w = ahead_col.as_ref().map_or(0.0, |c| c.width);
        let (width, ahead_left);
        if ahead_col.is_none() || (left_col.is_none() == right_col.is_none()) {
            // §5.3.5 / §5.7.6 c: with destinations both ways the ahead block (its longest
            // line) is centred over the vertical arm, the rest ranged left. Gaps to the
            // vertical arm are minimums (§5.3.1), so a wide ahead block pushes the arm right.
            let deficit = inset - (xv0 + wv / 2.0 - ahead_w / 2.0);
            if ahead_col.is_some() && deficit > 0.0 {
                xv0 += deficit;
            }
            ahead_left = xv0 + wv / 2.0 - ahead_w / 2.0;
            width = (xv0 + wv + right_reach).max(if ahead_col.is_none() {
                0.0
            } else {
                ahead_left + ahead_w + p.pad_side + border
            });
        } else if left_col.is_none() {
            // Only right turns: the ahead block is ranged against the left border (Figure 5-5).
            ahead_left = inset;
            width = (xv0 + wv + right_reach).max(inset + ahead_w + p.pad_side + border);
        } else {
            // Only left turns: the stub stays 2.5 sw from the right border and the ahead block
            // is ranged against it (Figure 5-5), so a wide ahead block widens the left gap.
            let need = inset + ahead_w + p.pad_side + border;
            let natural = xv0 + wv + right_reach;
            if need > natural {
                xv0 += need - natural;
            }
            width = natural.max(need);
            ahead_left = width - border - p.pad_side - ahead_w;
        }
        let xv1 = xv0 + wv;
        let cx = xv0 + wv / 2.0;

        // ── Vertical. yc is the horizontal arms' centre line.
        let need = |c: &Option<DestinationColumn>, w: f64| match c {
            None => border + ms.stub_to_bottom_border + w / 2.0, // Figure 5-5
            Some(c) => {
                border + self.bottom_pad(c) + c.height + s.tile_to_horizontal_arm_bottom + w / 2.0
            } // Figure 5-2
        };
        let yc = need(&left_col, wl).max(need(&right_col, wr));
        let y_base = border + ms.approach_to_bottom_border; // §5.7.2
        let (l_b, l_t) = (yc - wl / 2.0, yc + wl / 2.0);
        let (r_b, r_t) = (yc - wr / 2.0, yc + wr / 2.0);
        let arm_top = l_t.max(r_t);

        let height;
        let (mut ahead_top, mut y_apex, mut stub_top) = (0.0, 0.0, 0.0);
        if tee {
            height = arm_top + a.arm_to_top_border + border; // Figure 5-3
        } else if let Some(c) = &ahead_col {
            y_apex = arm_top + a.min_length_vertical; // §5.3.3
            ahead_top = y_apex + self.gap_above_arm_tip(c) + c.height;
            height = ahead_top + p.pad_top + border;
        } else {
            stub_top = arm_top + wv; // §5.5.1
            height = stub_top + ms.stub_to_border + border; // Figure 5-5
        }

        // ── The route symbol, counter-clockwise from the base of the approach arm.
        let r = ms.fillet_radius;
        let mut pts: Pts = vec![(xv0, y_base), (xv1, y_base)];
        corner(&mut pts, (xv1, r_b), (0.0, 1.0), (1.0, 0.0), r);
        match &right_col {
            None => {
                pts.push((xv1 + wr, r_b));
                pts.push((xv1 + wr, r_t));
            }
            Some(c) => {
                let tip = if ms.reach_from_border() {
                    xv1 + reach * (width - border - xv1)
                } else {
                    xv1 + gap_r + reach * c.width
                };
                let t = ms.tip_length(wr);
                pts.push((tip - t, r_b));
                pts.push((tip, yc));
                pts.push((tip - t, r_t));
            }
        }
        if tee {
            // The through arm's top edge is straight; arms of different widths step at the centre.
            if (r_t - l_t).abs() > 1e-9 {
                pts.push((cx, r_t));
                pts.push((cx, l_t));
            }
        } else {
            corner(&mut pts, (xv1, r_t), (-1.0, 0.0), (0.0, 1.0), r);
            if ahead_col.is_none() {
                pts.push((xv1, stub_top));
                pts.push((xv0, stub_top));
            } else {
                let t = ms.tip_length(wv);
                pts.push((xv1, y_apex - t));
                pts.push((cx, y_apex));
                pts.push((xv0, y_apex - t));
            }
            corner(&mut pts, (xv0, l_t), (0.0, -1.0), (-1.0, 0.0), r);
        }
        match &left_col {
            None => {
                pts.push((xv0 - wl, l_t));
                pts.push((xv0 - wl, l_b));
            }
            Some(c) => {
                let tip = if ms.reach_from_border() {
                    border + (1.0 - reach) * (xv0 - border)
                } else {
                    inset + (1.0 - reach) * c.width
                };
                let t = ms.tip_length(wl);
                pts.push((tip + t, l_t));
                pts.push((tip, yc));
                pts.push((tip + t, l_b));
            }
        }
        corner(&mut pts, (xv0, l_b), (1.0, 0.0), (0.0, -1.0), r);

        self.add_provisional(
            &mut diags,
            ms.provisional.as_deref(),
            "MAP_SYMBOL_PROVISIONAL",
        );
        let stub = left_col.is_none() || right_col.is_none() || (!tee && ahead_col.is_none());
        if stub && !ms.stubs_cited {
            diags.push(info(
                "MAP_STUB_UNCITED",
                format!(
                    "A direction without destinations is drawn as a stub, which this manual does not specify; \
                     the stub uses {} sw from the border and {} sw above the bottom border",
                    ms.stub_to_border, ms.stub_to_bottom_border
                ),
                self.pack.clause("mapType", &self.pack.source),
            ));
        }

        // The plate (ground, and the §2.15 border when the pack draws one), then the symbol.
        let mut prims = map_plate::draw(self.pack, width, height, &palette);
        let symbol = to_outline(&pts);
        prims.push(Prim::Fill {
            outline: symbol.clone(),
            colour: palette.arm,
        });

        // Destination blocks: ahead above the arm point, side blocks 2.5 sw below their arm.
        if let Some(c) = &ahead_col {
            c.draw(&mut prims, ahead_left, ahead_top, self.measurer)?;
        }
        if let Some(c) = &left_col {
            c.draw(
                &mut prims,
                inset,
                l_b - s.tile_to_horizontal_arm_bottom,
                self.measurer,
            )?;
        }
        if let Some(c) = &right_col {
            c.draw(
                &mut prims,
                xv1 + gap_r,
                r_b - s.tile_to_horizontal_arm_bottom,
                self.measurer,
            )?;
        }

        // Embedded restriction signs sit centred over the vertical arm (§2.1.3 of the original
        // engine): above the junction on a crossroads, on the approach of a tee.
        let (span_top, span_bot) = if tee {
            (l_b.min(r_b), y_base)
        } else if ahead_col.is_none() {
            (stub_top, arm_top)
        } else {
            (y_apex - ms.tip_length(wv), arm_top)
        };
        crate::embedded::draw_stacked(
            &mut prims,
            sign,
            self.pack,
            cx,
            span_top,
            span_bot,
            palette.ground,
        )?;

        Ok(RawLayout {
            prims,
            diagnostics: diags,
            width,
            height,
            symbols: vec![symbol],
        })
    }

    // ─── roundabout (S12-7-2, Figure 5-13) ────────────────────────────────────────

    /// The values the filled ring needs. The loader checks them for a pack with a `mapSymbol`
    /// section; a pack built in code that lacks one is a pack error here, never a guess.
    fn ring_rules(&self) -> Result<RingRules, SignError> {
        let rb = self.roundabout;
        match (
            rb.has_symbol_rules(),
            rb.exit_min_length,
            rb.approach_min_length,
            rb.cut_out_from_approach_deg,
            rb.cut_out_angle_deg,
            rb.cut_out_min_angle_deg,
            rb.ring_to_border,
        ) {
            (true, Some(exit_min), Some(approach_min), Some(from), Some(angle), Some(min_angle), Some(ring_gap)) => {
                Ok(RingRules {
                    outer_r: rb.outer_diameter / 2.0,
                    inner_r: rb.inner_diameter / 2.0,
                    exit_min,
                    approach_min,
                    cut_out_from_approach_deg: from,
                    cut_out_angle_deg: angle,
                    cut_out_min_angle_deg: min_angle,
                    keep_out: rb.legend_keep_out_radius,
                    ring_side: rb.legend_to_ring_side,
                    ring_gap,
                })
            }
            _ => Err(SignError::Pack(
                "Rule pack 'roundabout' needs exitMinLength, approachMinLength, cutOutFromApproachDeg, \
                 cutOutAngleDeg, cutOutMinAngleDeg, ringToBorder and exactly one of legendKeepOutRadius \
                 or legendToRingSide with 'mapSymbol'"
                    .to_string(),
            )),
        }
    }

    /// The map-type roundabout (UK TSM Chapter 7 paras 5.39–5.40, Figures 5-13 and 5-14), drawn
    /// as one filled outline: a ring 5 sw wide (7r inside, 12r outside) whatever the route
    /// status; arms joined to the ring with 1r fillets and pointed at 45° as on any map-type
    /// sign; every exit arm at least 12 sw and the approach arm at least 8.5 sw from the outside
    /// of the ring; and a radial cut-out (30°, at least 25°) between the approach and the right
    /// exit to show that all traffic goes round clockwise. Because the cut-out breaks the ring,
    /// the ring and its arms form one simple polygon: the outline runs round the outside of the
    /// ring and out along every arm, in along one cut edge, back round the inside of the ring
    /// and out along the other cut edge.
    ///
    /// Destination blocks follow the normal rules: side blocks 2.5 sw below their arm (Figure
    /// 5-2), ranged against the side border and kept outside the 14.5r circle and 4 sw clear of
    /// the approach arm (Figure 5-13); each side arm reaches two thirds along its block (§5.3.4)
    /// and is lengthened if needed to the 12 sw minimum; the ahead block sits on the ahead arm's
    /// point and is centred over it, or ranged as on a junction symbol when only one side has
    /// destinations. Where a direction has no destinations there is no arm, and the ring is 4 sw
    /// from that border (Figure 5-13 sub-figures).
    fn roundabout(&self, sign: &SignDefinition) -> Result<RawLayout, SignError> {
        let mut diags = validate_constraints(sign, self.pack);
        let p = &self.pack.panel;
        let s = self.spacing;
        let ms = self.ms;
        let rb = self.ring_rules()?;
        let palette = resolve_palette(sign, self.pack);
        let border = p.border_width;
        let inset = border + p.pad_side;
        let clause = self.pack.clause("mapRoundabout", &self.pack.source);

        let (outer_r, inner_r) = (rb.outer_r, rb.inner_r);
        let (exit_min, approach_min) = (rb.exit_min, rb.approach_min);
        let ring_gap = rb.ring_gap;
        let reach = ms.side_arm_reach;
        let below_arm = s.tile_to_horizontal_arm_bottom;

        let ahead = by_order(sign, Direction::Ahead);
        let left = by_order(sign, Direction::Left);
        let right = by_order(sign, Direction::Right);
        let ahead_col = self.column(&ahead, &palette)?;
        let left_col = self.column(&left, &palette)?;
        let right_col = self.column(&right, &palette)?;

        // Arm widths (§5.2): the approach follows the approach road, each exit its destinations.
        let wv = self.arm_width_for(sign.approach_class);
        let exit_width = |c: &Option<DestinationColumn>, dests: &[Destination]| {
            if c.is_none() {
                0.0
            } else {
                self.side_arm_width(dests, wv)
            }
        };
        let wa = exit_width(&ahead_col, &ahead);
        let wl = exit_width(&left_col, &left);
        let wr = exit_width(&right_col, &right);

        // Ring centre to the near edge of a side block hanging below an arm w wide: outside the
        // 14.5r circle at the block's top corner, 4 sw clear of the approach arm, and far enough
        // out that the arm, reaching two thirds along the block, is at least the minimum exit.
        let near_gap = |c: &DestinationColumn, w: f64| {
            let dy = w / 2.0 + below_arm;
            let clear = match rb.keep_out {
                // UK Figure 5-13 "14.5r"
                Some(k) => {
                    if dy < k {
                        (k * k - dy * dy).sqrt()
                    } else {
                        0.0
                    }
                }
                // Uganda Figure 2 "2.0 sw"
                None => outer_r + rb.ring_side.unwrap_or_default(),
            };
            let exit = if ms.reach_from_border() {
                exit_min / reach + outer_r - p.pad_side - c.width // Uganda "b/3" from the border
            } else {
                outer_r + exit_min - reach * c.width // UK "2/3 x L" along the block
            };
            clear.max(wv / 2.0 + s.tile_to_vertical_arm_side).max(exit)
        };

        // ── Horizontal.
        let gap_l = left_col.as_ref().map_or(0.0, |c| near_gap(c, wl));
        let gap_r = right_col.as_ref().map_or(0.0, |c| near_gap(c, wr));
        let mut cx = match &left_col {
            None => border + ring_gap + outer_r,
            Some(c) => inset + c.width + gap_l,
        };
        let right_reach = match &right_col {
            None => outer_r + ring_gap + border,
            Some(c) => gap_r + c.width + p.pad_side + border,
        };
        if left_col.is_none() {
            diags.push(info(
                "ROUNDABOUT_LEFT_GAP_UNCITED",
                format!(
                    "No left exit: the ring is placed {ring_gap} sw from the left border. Figure 5-13 gives this gap \
                     only for the top and right borders"
                ),
                clause,
            ));
        }

        let ahead_w = ahead_col.as_ref().map_or(0.0, |c| c.width);
        let (width, ahead_left);
        if ahead_col.is_none() || (left_col.is_none() == right_col.is_none()) {
            // §5.3.5: the ahead block is centred over its arm; the side gaps are minimums.
            let deficit = inset - (cx - ahead_w / 2.0);
            if ahead_col.is_some() && deficit > 0.0 {
                cx += deficit;
            }
            ahead_left = cx - ahead_w / 2.0;
            width = (cx + right_reach).max(if ahead_col.is_none() {
                0.0
            } else {
                ahead_left + ahead_w + p.pad_side + border
            });
        } else if left_col.is_none() {
            // Only right turns: the ahead block is ranged against the left border (Figure 5-5).
            ahead_left = inset;
            width = (cx + right_reach).max(inset + ahead_w + p.pad_side + border);
        } else {
            // Only left turns: the ahead block is ranged against the right border (Figure 5-5).
            let need = inset + ahead_w + p.pad_side + border;
            let natural = cx + right_reach;
            if need > natural {
                cx += need - natural;
            }
            width = natural.max(need);
            ahead_left = width - border - p.pad_side - ahead_w;
        }

        // ── Vertical. The side arms run through the ring centre (Figure 5-13).
        let y_base = border + ms.approach_to_bottom_border;
        let mut cy = y_base + approach_min + outer_r;
        if let Some(c) = &left_col {
            cy = cy.max(border + self.bottom_pad(c) + c.height + below_arm + wl / 2.0);
        }
        if let Some(c) = &right_col {
            cy = cy.max(border + self.bottom_pad(c) + c.height + below_arm + wr / 2.0);
        }

        let height;
        let (mut ahead_top, mut apex) = (0.0, 0.0);
        match &ahead_col {
            None => height = cy + outer_r + ring_gap + border,
            Some(c) => {
                apex = cy + outer_r + exit_min;
                ahead_top = apex + self.gap_above_arm_tip(c) + c.height;
                height = ahead_top + p.pad_top + border;
            }
        }

        // ── The symbol. Each side arm's point: 2/3 along its block (UK) or b/3 from the border
        // (Uganda).
        let mut arms: Vec<RingArm> = Vec::new();
        if let Some(c) = &right_col {
            let to_point = if ms.reach_from_border() {
                outer_r + reach * (width - border - (cx + outer_r))
            } else {
                gap_r + reach * c.width
            };
            arms.push(RingArm {
                bearing_deg: 0.0,
                width: wr,
                reach: to_point,
                pointed: true,
            });
        }
        if ahead_col.is_some() {
            arms.push(RingArm {
                bearing_deg: 90.0,
                width: wa,
                reach: apex - cy,
                pointed: true,
            });
        }
        if let Some(c) = &left_col {
            let to_point = if ms.reach_from_border() {
                outer_r + reach * (cx - outer_r - border)
            } else {
                gap_l + reach * c.width
            };
            arms.push(RingArm {
                bearing_deg: 180.0,
                width: wl,
                reach: to_point,
                pointed: true,
            });
        }
        arms.push(RingArm {
            bearing_deg: 270.0,
            width: wv,
            reach: cy - y_base,
            pointed: false,
        });

        let cut1 = -90.0 + rb.cut_out_from_approach_deg; // edge nearer the approach arm
        let cut2 = cut1 + rb.cut_out_angle_deg; // edge nearer the right exit
        let ring = ring_outline(cx, cy, outer_r, inner_r, ms, &arms, cut1, cut2);

        // Para 5.40: the cut-out must stay clear of the arms either side and be at least 25°.
        let cut_angle = cut2 - cut1;
        if cut_angle < rb.cut_out_min_angle_deg - 1e-9 {
            diags.push(Diagnostic::warning(
                "ROUNDABOUT_CUT_OUT_TOO_SMALL",
                format!(
                    "Roundabout cut-out is {}°; para 5.40 recommends at least {}°",
                    one_decimal(cut_angle),
                    rb.cut_out_min_angle_deg
                ),
                clause,
            ));
        }
        if ring.last_before_cut > cut1 + 360.0 + 1e-9 || ring.first_after_cut < cut2 - 1e-9 {
            diags.push(Diagnostic::warning(
                "ROUNDABOUT_CUT_OUT_BLOCKED",
                "An arm reaches into the roundabout cut-out (the Figure 5-14 adjustment is not modelled)"
                    .to_string(),
                clause,
            ));
        }

        self.add_provisional(
            &mut diags,
            ms.provisional.as_deref(),
            "MAP_SYMBOL_PROVISIONAL",
        );
        self.add_provisional(
            &mut diags,
            self.roundabout.provisional.as_deref(),
            "ROUNDABOUT_PROVISIONAL",
        );

        let mut prims = map_plate::draw(self.pack, width, height, &palette);
        let symbol = to_outline(&ring.points);
        prims.push(Prim::Fill {
            outline: symbol.clone(),
            colour: palette.arm,
        });

        if let Some(c) = &ahead_col {
            c.draw(&mut prims, ahead_left, ahead_top, self.measurer)?;
        }
        if let Some(c) = &left_col {
            c.draw(&mut prims, inset, cy - wl / 2.0 - below_arm, self.measurer)?;
        }
        if let Some(c) = &right_col {
            c.draw(
                &mut prims,
                cx + gap_r,
                cy - wr / 2.0 - below_arm,
                self.measurer,
            )?;
        }

        // Embedded restriction signs inside the ring (C# `EmbedOverArm`).
        crate::embedded::draw_stacked(
            &mut prims,
            sign,
            self.pack,
            cx,
            cy + inner_r,
            cy - inner_r,
            palette.ground,
        )?;

        Ok(RawLayout {
            prims,
            diagnostics: diags,
            width,
            height,
            symbols: vec![symbol],
        })
    }
}

/// App.1 §2.1.2 / Chapter 7 §3.7: nearer destination (lower `order`) first.
fn by_order(sign: &SignDefinition, dir: Direction) -> Vec<Destination> {
    let mut dests: Vec<Destination> = sign
        .destinations
        .iter()
        .filter(|d| d.direction == dir)
        .cloned()
        .collect();
    dests.sort_by_key(|d| d.order); // stable, as OrderBy
    dests
}

fn to_outline(pts: &[(f64, f64)]) -> Vec<Pt2> {
    pts.iter().map(|&(x, y)| Pt2::new(x, y)).collect()
}

/// C# `{value:0.#}`: at most one decimal, none when it is zero.
fn one_decimal(value: f64) -> String {
    let rounded = (value * 10.0).round() / 10.0;
    if rounded.fract() == 0.0 {
        format!("{rounded:.0}")
    } else {
        format!("{rounded:.1}")
    }
}

/// The angle swept from `a1` to `a2` the short way round, in (−π, π].
fn short_sweep(a1: f64, a2: f64) -> f64 {
    use std::f64::consts::PI;
    let mut sweep = a2 - a1;
    while sweep > PI {
        sweep -= 2.0 * PI;
    }
    while sweep <= -PI {
        sweep += 2.0 * PI;
    }
    sweep
}

/// Adds a concave corner at `corner`, where an edge running along `dir_in` turns to run along
/// `dir_out`, rounded with a circular fillet of radius `r` (§5.7.3).
fn corner(pts: &mut Pts, corner: (f64, f64), dir_in: (f64, f64), dir_out: (f64, f64), r: f64) {
    if r <= 0.0 {
        pts.push(corner);
        return;
    }
    let p1 = (corner.0 - r * dir_in.0, corner.1 - r * dir_in.1);
    let p2 = (corner.0 + r * dir_out.0, corner.1 + r * dir_out.1);
    let c = (p1.0 + r * dir_out.0, p1.1 + r * dir_out.1);
    pts.push(p1);
    fillet(pts, c, r, p1, p2);
    pts.push(p2);
}

/// Interior points of a fillet of radius `r` centred on `c`, from tangent point `from` to `to`
/// the short way round (the endpoints are added by the caller).
fn fillet(pts: &mut Pts, c: (f64, f64), r: f64, from: (f64, f64), to: (f64, f64)) {
    if r <= 0.0 {
        return;
    }
    let a1 = (from.1 - c.1).atan2(from.0 - c.0);
    let a2 = (to.1 - c.1).atan2(to.0 - c.0);
    let sweep = short_sweep(a1, a2);
    for k in 1..FILLET_SEGMENTS {
        let a = a1 + sweep * k as f64 / FILLET_SEGMENTS as f64;
        pts.push((c.0 + r * a.cos(), c.1 + r * a.sin()));
    }
}

/// The ring and its arms as one closed outline, with the bearings where the outline rejoins
/// the ring after the last arm (`last_before_cut`) and leaves it for the first arm
/// (`first_after_cut`), so the caller can check that no arm runs into the cut-out.
#[derive(Debug, Clone, PartialEq)]
struct RingOutline {
    points: Pts,
    last_before_cut: f64,
    first_after_cut: f64,
}

/// The ring and its arms as one closed outline, anticlockwise: round the outside of the ring
/// from the far cut edge (`cut2`) past every arm to the near cut edge (`cut1`), in along that
/// radial edge, clockwise round the inside of the ring and out along the far edge. Each arm
/// leaves the ring through two fillets of radius `MapSymbolRules::fillet_radius`, tangent to
/// the ring and to the arm's side.
#[allow(clippy::too_many_arguments)]
fn ring_outline(
    cx: f64,
    cy: f64,
    outer_r: f64,
    inner_r: f64,
    ms: &MapSymbolRules,
    arms: &[RingArm],
    cut1: f64,
    cut2: f64,
) -> RingOutline {
    let rf = ms.fillet_radius;
    let mut pts: Pts = Vec::new();
    let on = |r: f64, deg: f64| {
        let rad = deg * std::f64::consts::PI / 180.0;
        (cx + r * rad.cos(), cy + r * rad.sin())
    };
    // Interior grid points only.
    let arc_points = |pts: &mut Pts, r: f64, from_deg: f64, to_deg: f64| {
        const EPS: f64 = 1e-6;
        if to_deg >= from_deg {
            let mut d = (from_deg / RING_STEP_DEG + 1.0).floor() * RING_STEP_DEG;
            while d < to_deg - EPS {
                if d > from_deg + EPS {
                    pts.push(on(r, d));
                }
                d += RING_STEP_DEG;
            }
        } else {
            let mut d = (from_deg / RING_STEP_DEG - 1.0).ceil() * RING_STEP_DEG;
            while d > to_deg + EPS {
                if d < from_deg - EPS {
                    pts.push(on(r, d));
                }
                d -= RING_STEP_DEG;
            }
        }
    };

    // An arm's bearing counted on from the far cut edge, so the arms sort the way the outline
    // meets them.
    let from_cut = |arm: &RingArm| {
        if arm.bearing_deg < cut2 {
            arm.bearing_deg + 360.0
        } else {
            arm.bearing_deg
        }
    };
    let mut ordered: Vec<&RingArm> = arms.iter().collect();
    ordered.sort_by(|x, y| from_cut(x).total_cmp(&from_cut(y))); // stable, as OrderBy

    let mut at = cut2; // current bearing on the outside of the ring
    let mut first_after_cut = f64::INFINITY;
    let mut last_before_cut = cut2;
    pts.push(on(outer_r, cut2));
    for arm in ordered {
        let b = from_cut(arm);
        let th = b * std::f64::consts::PI / 180.0;
        let u = (th.cos(), th.sin());
        let n = (-u.1, u.0);
        let h = arm.width / 2.0;
        let t = ((outer_r + rf) * (outer_r + rf) - (h + rf) * (h + rf)).sqrt();
        let delta = (h + rf).atan2(t) * 180.0 / std::f64::consts::PI; // fillet centre off the arm axis

        // Clockwise side (−n) first: ring → fillet → arm side.
        let in_deg = b - delta;
        if first_after_cut.is_infinite() {
            first_after_cut = in_deg;
        }
        arc_points(&mut pts, outer_r, at, in_deg);
        pts.push(on(outer_r, in_deg));
        let side_cw = (cx + t * u.0 - h * n.0, cy + t * u.1 - h * n.1);
        fillet(
            &mut pts,
            (cx + t * u.0 - (h + rf) * n.0, cy + t * u.1 - (h + rf) * n.1),
            rf,
            on(outer_r, in_deg),
            side_cw,
        );
        pts.push(side_cw);

        if arm.pointed {
            let back = arm.reach - ms.tip_length(arm.width);
            pts.push((cx + back * u.0 - h * n.0, cy + back * u.1 - h * n.1));
            pts.push((cx + arm.reach * u.0, cy + arm.reach * u.1));
            pts.push((cx + back * u.0 + h * n.0, cy + back * u.1 + h * n.1));
        } else {
            pts.push((
                cx + arm.reach * u.0 - h * n.0,
                cy + arm.reach * u.1 - h * n.1,
            ));
            pts.push((
                cx + arm.reach * u.0 + h * n.0,
                cy + arm.reach * u.1 + h * n.1,
            ));
        }

        // Anticlockwise side (+n): arm side → fillet → ring.
        let out_deg = b + delta;
        let side_acw = (cx + t * u.0 + h * n.0, cy + t * u.1 + h * n.1);
        pts.push(side_acw);
        fillet(
            &mut pts,
            (cx + t * u.0 + (h + rf) * n.0, cy + t * u.1 + (h + rf) * n.1),
            rf,
            side_acw,
            on(outer_r, out_deg),
        );
        pts.push(on(outer_r, out_deg));
        at = out_deg;
        last_before_cut = out_deg;
    }

    // Round to the near cut edge, in along it, back round the inside, out along the far edge.
    let end = cut1 + 360.0;
    arc_points(&mut pts, outer_r, at, end);
    pts.push(on(outer_r, end));
    pts.push(on(inner_r, end));
    arc_points(&mut pts, inner_r, end, cut2);
    pts.push(on(inner_r, cut2));

    if rf <= 0.0 {
        // With no fillet the ring tangent point and the arm-side point coincide: keep one.
        let mut unique: Pts = Vec::with_capacity(pts.len());
        for q in pts {
            match unique.last() {
                Some(l) if (l.0 - q.0).abs() <= 1e-9 && (l.1 - q.1).abs() <= 1e-9 => {}
                _ => unique.push(q),
            }
        }
        pts = unique;
    }
    RingOutline {
        points: pts,
        last_before_cut,
        first_after_cut,
    }
}

impl SignDefinition {
    /// A new map-type crossroads sign with the defaults the desktop app gives one (C#
    /// `AddMapCrossroads`): primary approach, green background, everything else unset. The app
    /// seeds it with one destination each way (`Left`, `Ahead`, `Right`, all of order 0). Set
    /// `kind` to [`SignKind::MapTypeTee`] or [`SignKind::MapTypeRoundabout`] for the other two
    /// map-type signs, which read the same fields.
    pub fn map_crossroads(
        code: &str,
        speed_kph: u32,
        destinations: Vec<Destination>,
    ) -> SignDefinition {
        SignDefinition {
            kind: SignKind::MapTypeCrossroads,
            ..SignDefinition::stack(code, speed_kph, destinations)
        }
    }
}

#[cfg(test)]
#[path = "map_symbol_tests.rs"]
mod tests;
