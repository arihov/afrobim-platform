// SPDX-License-Identifier: LicenseRef-AfroBIM-Proprietary
// Copyright (c) 2026 AfroBIM. All rights reserved.
//! Stack-type advance direction signs: the panelled layout of UK TSM Chapter 7 §4.2–4.3
//! (Figure 4-9). Port of C# `StackSignLayout.PanelledLayout` with `SignPalette` and
//! `ConstraintValidator`.
//!
//! The C# engine's other stack layout (one arrow per row, for a pack without a `stack`
//! section such as the Uganda pack) is in `ug_guidance.rs`.

use afrobim_draw::Pt2;

use crate::arrow;
use crate::column::{DestinationColumn, Edge};
use crate::measurer::TextTileMeasurer;
use crate::model::{
    Colour, ColourScheme, Destination, Diagnostic, Direction, FontFace, SignDefinition,
};
use crate::pack::{ArrowRules, PanelRules, RulePack, StackRules};
use crate::prim::{rect, Prim};
use crate::SignError;

/// What a layout engine hands back before lettering is outlined.
#[derive(Debug, Clone)]
pub(crate) struct RawLayout {
    pub prims: Vec<Prim>,
    pub diagnostics: Vec<Diagnostic>,
    pub width: f64,
    pub height: f64,
    /// The outlines of the guidance arrows, as the layout registered them. The overlap check
    /// tests legend tiles and the border against these.
    pub symbols: Vec<Vec<Pt2>>,
}

/// The concrete colours and lettering face a layout should paint, resolved from a sign's
/// colour scheme.
#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct Palette {
    pub ground: Colour,
    pub lettering: Colour,
    pub arm: Colour,
    pub face: FontFace,
}

/// The scheme name the sign resolves to (explicit, else its catalog entry's), or `None` for a
/// legacy sign that only names a background.
pub(crate) fn scheme_of<'a>(sign: &SignDefinition, pack: &'a RulePack) -> Option<&'a str> {
    let explicit = match sign.scheme {
        ColourScheme::Primary => Some("primary"),
        ColourScheme::Secondary => Some("secondary"),
        ColourScheme::Information => Some("information"),
        ColourScheme::Tourist => Some("tourist"),
        ColourScheme::Temporary => Some("temporary"),
        ColourScheme::Motorway => Some("motorway"),
        ColourScheme::Auto => None, // resolved via catalog or legacy fallback
    };
    explicit.or_else(|| {
        let key = sign.catalog_key.as_deref().filter(|k| !k.is_empty())?;
        pack.catalog.as_ref()?.find(key)?.scheme.as_deref()
    })
}

/// Maps a sign's scheme to the rule-pack palette (App.1 §1.2 lettering + App.3 scheme table).
///
/// Resolution order:
///   1. An explicit `scheme` wins.
///   2. Otherwise a `catalog_key` that matches a registered catalog entry supplies its scheme.
///   3. Otherwise the sign keeps its explicitly supplied `background` and the legacy §1.2 rule
///      (Transport Heavy on white, Medium on dark; arm/lettering take the inverse ground colour).
pub(crate) fn resolve_palette(sign: &SignDefinition, pack: &RulePack) -> Palette {
    if let Some(entry) = scheme_of(sign, pack).and_then(|s| pack.palettes.as_ref()?.find(s)) {
        return Palette {
            ground: entry.ground,
            lettering: entry.lettering,
            arm: entry.arm,
            face: entry.face,
        };
    }
    // Legacy / unspecified: honour the explicit Background (§1.2 fallback).
    let white = sign.background == Colour::White;
    let face = if white {
        FontFace::TransportHeavy
    } else {
        FontFace::TransportMedium
    };
    let ink = if white { Colour::Black } else { Colour::White };
    Palette {
        ground: sign.background,
        lettering: ink,
        arm: ink,
        face,
    }
}

/// App.1 §2.1.2 design rules common to guidance signs, citing the pack's own `constraints`
/// clause.
pub(crate) fn validate_constraints(sign: &SignDefinition, pack: &RulePack) -> Vec<Diagnostic> {
    let rules = &pack.constraints;
    let clause = pack.clause("constraints", "Uganda TSM Vol.2 App.1 §2.1.2");
    let mut diags = Vec::new();

    if sign.destinations.len() > rules.max_destinations {
        diags.push(Diagnostic::error(
            "MAX_DESTINATIONS",
            format!(
                "{} destinations; maximum is {}",
                sign.destinations.len(),
                rules.max_destinations
            ),
            clause,
        ));
    }

    // Directions in order of first appearance.
    let mut directions: Vec<Direction> = Vec::new();
    for d in &sign.destinations {
        if !directions.contains(&d.direction) {
            directions.push(d.direction);
        }
    }
    for dir in directions {
        let orders: Vec<i32> = sign
            .destinations
            .iter()
            .filter(|d| d.direction == dir)
            .map(|d| d.order)
            .collect();
        if orders.len() > rules.max_destinations_per_direction {
            diags.push(Diagnostic::error(
                "MAX_PER_DIRECTION",
                format!(
                    "{} destinations for {}; maximum is {}",
                    orders.len(),
                    dir.name(),
                    rules.max_destinations_per_direction
                ),
                clause,
            ));
        }
        if rules.nearer_destination_above {
            let distinct = orders
                .iter()
                .enumerate()
                .filter(|(i, o)| !orders[..*i].contains(o))
                .count();
            if distinct != orders.len() {
                diags.push(Diagnostic::error(
                    "NEARER_NOT_ABOVE",
                    format!(
                        "Duplicate proximity order for {}; stacking order is ambiguous",
                        dir.name()
                    ),
                    clause,
                ));
            }
        }
    }
    diags
}

/// One direction's section of a panelled stack-type sign.
#[derive(Debug, Clone)]
pub(crate) struct Section {
    pub direction: Direction,
    pub column: DestinationColumn,
    pub arrow_shaft: f64,
    pub arrow_along: f64,
    pub arrow_across: f64,
    pub height: f64,
    pub min_width: f64,
    pub pad_bottom: f64,
}

pub(crate) struct StackSignLayout<'a> {
    pack: &'a RulePack,
    measurer: &'a TextTileMeasurer<'a>,
    stack: &'a StackRules,
    arrows: &'a ArrowRules,
}

impl<'a> StackSignLayout<'a> {
    /// `Unsupported` for a pack without the `stack` and `arrows` sections: its per-row layout
    /// is not ported.
    pub fn new(pack: &'a RulePack, measurer: &'a TextTileMeasurer<'a>) -> Result<Self, SignError> {
        match (&pack.stack, &pack.arrows) {
            (Some(stack), Some(arrows)) => Ok(StackSignLayout { pack, measurer, stack, arrows }),
            _ => Err(SignError::Unsupported(format!(
                "stack-type signs for rule pack '{}': it has no 'stack' and 'arrows' sections, and only the panelled \
                 (UK TSM Chapter 7 §4.2-4.3) stack layout is ported",
                pack.id
            ))),
        }
    }

    /// Each direction is its own section of the sign (§4.3.2), separated by
    /// `StackRules::divider_width` dividers in the border colour and ordered ahead, left, right
    /// (§4.3.3). Every section carries one Figure 4-4 arrow centred vertically (§4.2.1), 2.5 sw
    /// from the nearer side border, on the left for ahead and left turns and on the right for
    /// right turns (§4.3.5 b). The legend is 2.5 sw from the section top and 1.5 sw from its
    /// bottom (2.5 sw below a panel or patch, §4.2.5); where the arrow sets the height, a plain
    /// tiled legend sits `StackRules::legend_raise` above centre (Figure 4-1 "H+1") and a panel
    /// or patch is centred (Figures 4-2, 4-3). A vertical arrow beside a single-line legend
    /// uses the short shaft (§4.2.2: 14 sw rather than 16 sw).
    ///
    /// The destinations themselves are laid out by [`DestinationColumn`]: blocks, route
    /// numbers, patches, panels, symbols (at the end away from the arrow, §4.2.6) and
    /// distances. A symbol the pack gives no size for is drawn at the engine's 10 sw nominal
    /// box and flagged, so an unprescribed size is never passed off as compliant.
    pub fn layout(&self, sign: &SignDefinition) -> Result<RawLayout, SignError> {
        let mut diags = validate_constraints(sign, self.pack);
        let p = &self.pack.panel;
        let st = self.stack;
        let palette = resolve_palette(sign, self.pack);

        let mut sections: Vec<Section> = Vec::new();
        for dir in [Direction::Ahead, Direction::Left, Direction::Right] {
            let mut dests: Vec<&Destination> = sign
                .destinations
                .iter()
                .filter(|d| d.direction == dir)
                .collect();
            dests.sort_by_key(|d| d.order); // stable, as OrderBy
            if !dests.is_empty() {
                sections.push(self.build_section(dir, &dests, &palette)?);
            }
        }

        let inner_width = if sections.is_empty() {
            2.0 * p.pad_side
        } else {
            sections
                .iter()
                .map(|s| s.min_width)
                .fold(f64::NEG_INFINITY, f64::max)
        };
        let width = 2.0 * p.border_width + inner_width;
        let mut height = 2.0 * p.border_width
            + sections.iter().map(|s| s.height).sum::<f64>()
            + sections.len().saturating_sub(1) as f64 * st.divider_width;
        if sections.is_empty() {
            height += p.pad_top + p.pad_bottom;
        }

        // The whole face in the border colour; each section's ground is painted over it, which
        // leaves the border and the dividers showing.
        let mut prims = vec![rect(0.0, 0.0, width, height, palette.lettering)];

        let (x_in, x_out) = (p.border_width, width - p.border_width);
        let mut top = height - p.border_width;
        let mut symbols = Vec::new();
        for s in &sections {
            let bottom = top - s.height;
            prims.push(rect(x_in, bottom, x_out, top, palette.ground));
            symbols.push(
                self.draw_section(&mut prims, s, x_in, x_out, bottom, top, &palette, p, 0.0)?,
            );
            top = bottom - st.divider_width;
        }

        let mut notes: Vec<&String> = Vec::new();
        for note in sections.iter().flat_map(|s| &s.column.unsupported) {
            if !notes.contains(&note) {
                notes.push(note);
            }
        }
        for note in notes {
            diags.push(Diagnostic::warning(
                "ROUTE_NUMBER_NOT_DRAWN",
                format!("Not drawn: {note}"),
                &self.pack.source,
            ));
        }

        for d in &sign.destinations {
            let Some(id) = &d.symbol_id else { continue };
            if self
                .pack
                .symbols
                .as_ref()
                .and_then(|s| s.find(id))
                .is_none()
            {
                diags.push(Diagnostic::warning(
                    "SYMBOL_SIZE_UNSPECIFIED",
                    format!(
                        "Rule pack gives no size for symbol '{id}'; drawn at the 10 sw nominal box"
                    ),
                    self.pack.clause("symbolSize", &self.pack.source),
                ));
            }
        }

        if sign.embedded.as_ref().is_some_and(|e| !e.is_empty()) {
            diags.push(Diagnostic::warning(
                "EMBEDDED_NOT_PLACED",
                "Warning/regulatory symbols on a panelled stack-type sign are not laid out yet; omitted".to_string(),
                self.pack.clause("stackEmbedded", &self.pack.source),
            ));
        }

        for prim in &prims {
            if let Prim::Symbol { id, .. } = prim {
                diags.push(Diagnostic::warning(
                    "SYMBOL_UNRESOLVED",
                    format!("Symbol '{id}' has no artwork; placeholder emitted"),
                    self.pack.clause("symbolPlacement", &self.pack.source),
                ));
            }
        }

        Ok(RawLayout {
            prims,
            diagnostics: diags,
            width,
            height,
            symbols,
        })
    }

    fn symbol_size(&self, symbol_id: &str) -> f64 {
        match self.pack.symbols.as_ref().and_then(|s| s.find(symbol_id)) {
            Some(spec) if spec.size_sw > 0.0 => spec.size_sw,
            _ => 10.0,
        }
    }

    pub(crate) fn build_section(
        &self,
        dir: Direction,
        dests: &[&Destination],
        palette: &Palette,
    ) -> Result<Section, SignError> {
        let (p, st, arrows) = (&self.pack.panel, self.stack, self.arrows);
        let column = DestinationColumn::build(
            dests,
            palette.ground,
            palette.lettering,
            palette.face,
            self.pack,
            self.measurer,
            &|id| self.symbol_size(id),
            dir == Direction::Right,
            false,
        )?;

        // §4.2.2: the vertical arrow shortens to 14 sw beside a single-line legend (tiles or
        // panel), unless that line is taller than the short arrow.
        let vertical = dir == Direction::Ahead;
        let mut shaft = arrows.shaft_length;
        if let Some(short_shaft) = arrows.short_shaft_length {
            if vertical
                && column.name_lines == 1
                && column.height <= short_shaft + arrows.head_length
            {
                shaft = short_shaft;
            }
        }
        let along = arrow::total_length(arrows, Some(shaft));
        let across = arrow::total_width(arrows);
        let arrow_h = if vertical { along } else { across };
        let arrow_w = if vertical { across } else { along };

        // §4.2.5: 1.5 sw below tiles, 2.5 sw below a panel, patch or symbol.
        let pad_bottom = match (column.bottom, &self.pack.blocks) {
            (Edge::Patch, Some(blocks)) => blocks.panel_pad_bottom_patch,
            _ => p.pad_bottom,
        };

        let height =
            (p.pad_top + column.height + pad_bottom).max(2.0 * st.arrow_clearance + arrow_h);
        let min_width =
            st.arrow_to_border + arrow_w + st.arrow_to_legend + column.width + p.pad_side;

        Ok(Section {
            direction: dir,
            column,
            arrow_shaft: shaft,
            arrow_along: along,
            arrow_across: across,
            height,
            min_width,
            pad_bottom,
        })
    }

    /// Draws one section between `bottom` and `top` and returns its arrow's outline.
    /// `legend_shift`: extra space between the arrow's legend gap and the blocks (a direction
    /// sign's embedded restriction signs sit there); 0 on a stack-type sign.
    #[allow(clippy::too_many_arguments)]
    pub(crate) fn draw_section(
        &self,
        prims: &mut Vec<Prim>,
        s: &Section,
        x_in: f64,
        x_out: f64,
        bottom: f64,
        top: f64,
        palette: &Palette,
        p: &PanelRules,
        legend_shift: f64,
    ) -> Result<Vec<Pt2>, SignError> {
        let st = self.stack;
        let vertical = s.direction == Direction::Ahead;
        let arrow_on_right = s.direction == Direction::Right;
        let arrow_w = if vertical {
            s.arrow_across
        } else {
            s.arrow_along
        };
        let mid_y = (bottom + top) / 2.0;

        // Arrow 2.5 sw from the nearer border, centred vertically (§4.2.1, §4.3.5 b).
        let arrow_left = if arrow_on_right {
            x_out - st.arrow_to_border - arrow_w
        } else {
            x_in + st.arrow_to_border
        };
        let (tail, rotation) = match s.direction {
            Direction::Ahead => (
                Pt2::new(arrow_left + arrow_w / 2.0, mid_y - s.arrow_along / 2.0),
                0.0,
            ),
            Direction::Left => (Pt2::new(arrow_left + s.arrow_along, mid_y), 90.0),
            Direction::Right => (Pt2::new(arrow_left, mid_y), -90.0),
        };
        let outline = arrow::place(self.arrows, tail, rotation, Some(s.arrow_shaft));
        prims.push(Prim::Fill {
            outline: outline.clone(),
            colour: palette.arm,
        });

        // Blocks are ranged left (§4.2.5): beside the arrow on the left, or ending
        // ArrowToLegend short of a right-hand arrow.
        let block_left = if arrow_on_right {
            arrow_left - st.arrow_to_legend - s.column.width
        } else {
            arrow_left + arrow_w + st.arrow_to_legend + legend_shift
        };

        // Legend top: PadTop below the section top when the legend sets the height, otherwise
        // split the spare space, raising a plain tiled legend by LegendRaise.
        let section_h = top - bottom;
        let legend_governs = p.pad_top + s.column.height + s.pad_bottom;
        let raise = if s.column.all_tiles {
            st.legend_raise
        } else {
            0.0
        };
        let top_gap = if legend_governs >= section_h - 1e-9 {
            p.pad_top
        } else {
            (section_h - s.column.height + raise) / 2.0
        };

        s.column
            .draw(prims, block_left, top - top_gap, self.measurer)?;
        Ok(outline)
    }
}
