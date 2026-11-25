// SPDX-License-Identifier: LicenseRef-AfroBIM-Proprietary
// Copyright (c) 2026 AfroBIM. All rights reserved.
//! Rectangular direction signs. Port of C# `DirectionSignLayout` for a pack with stack-type
//! section rules (UK TSM Chapter 7 §7.11, Figure 7-4).
//!
//! UK TSM Chapter 7 §7.11: a rectangular direction sign "is basically the same as for advance
//! direction stack type signs". It is drawn as one stack-type section (§4.2–4.3) with the
//! Figure 4-4 arrow (TSRGD 2016 S12-5-1) as a single filled outline: 2.5 sw from the left
//! border and 2.5 sw from the legend, centred between the borders with at least 2.5 sw above
//! and below, 14 sw long beside a single line (§4.2.2), and, when the arrow sets the height,
//! 1 sw more above a tiled legend than below it (Figure 7-4 "H + 1"). The arrow points ahead
//! whatever the destinations' directions; §7.11's arrow on the right and §7.12's 22.5°
//! slip-road arrow need schedule fields that do not exist yet. The C# engine implements no
//! flag-type direction sign.
//!
//! The destinations are laid out by [`crate::column::DestinationColumn`], the stack-type
//! engine, so route numbers, patches and panels follow the same rules as on a stack-type sign.
//! Chapter 7 gives direction signs no separate block rules; this follows the stack-type
//! behaviour (design spec §13). Symbols and distances are not laid out on direction signs.
//!
//! Not ported:
//! - the C# engine's original direction layout (a stroked shaft-and-head arrow beside a text
//!   stack), which it uses for a pack without `stack` and `arrows` sections (the Uganda pack).
//!   Only its no-destination case, which every pack takes, is ported: the plate alone, and
//!   any embedded restriction signs.
//!
//! Embedded restriction signs keep their place between the arrow and the legend (`embedded.rs`).

use crate::measurer::TextTileMeasurer;
use crate::model::{Destination, Direction, SignDefinition};
use crate::pack::RulePack;
use crate::sign_plate;
use crate::stack::{resolve_palette, validate_constraints, RawLayout, StackSignLayout};
use crate::SignError;

/// Ahead, then left, then right: the tie-break within equal proximity.
pub(crate) fn direction_rank(d: Direction) -> u8 {
    match d {
        Direction::Ahead => 0,
        Direction::Left => 1,
        Direction::Right => 2,
    }
}

pub(crate) fn layout(
    pack: &RulePack,
    measurer: &TextTileMeasurer,
    sign: &SignDefinition,
) -> Result<RawLayout, SignError> {
    // Embedded restriction signs sit between the arrow and the destinations (App.1 §2.1.3).
    let embedded_col_w = crate::embedded::column_width(sign, pack)?;

    let diags = validate_constraints(sign, pack);
    let p = &pack.panel;
    let palette = resolve_palette(sign, pack);
    let border = p.border_width;

    if sign.destinations.is_empty() {
        // The original engine with nothing to draw: no arrow, no legend, the plate alone.
        let width =
            2.0 * border + 2.0 * p.pad_side + embedded_col_w.map_or(0.0, |w| w + p.word_spacing);
        let height = 2.0 * border + p.pad_top + p.pad_bottom;
        // The plate: the ground, and the §2.15 border when the pack draws one.
        let mut prims = sign_plate::draw(pack, width, height, &palette);
        if let Some(col_w) = embedded_col_w {
            crate::embedded::draw_stacked(
                &mut prims,
                sign,
                pack,
                border + p.pad_side + col_w / 2.0,
                height - border - p.pad_top,
                border + p.pad_bottom,
                palette.ground,
            )?;
        }
        return Ok(RawLayout {
            prims,
            diagnostics: diags,
            width,
            height,
            symbols: Vec::new(),
        });
    }

    // A pack with stack-type section rules (UK TSM Chapter 7) lays the sign out as one
    // stack-type section.
    if pack.stack.is_none() || pack.arrows.is_none() {
        return Err(SignError::Unsupported(format!(
            "direction signs for rule pack '{}': it has no 'stack' and 'arrows' sections, and only the stack-type \
             section (UK TSM Chapter 7 §7.11) direction layout is ported",
            pack.id
        )));
    }
    let engine = StackSignLayout::new(pack, measurer)?;

    // §2.1.2: nearer destination (lower Order) first. Symbols and distances are not laid out
    // on direction signs.
    let mut ordered: Vec<&Destination> = sign.destinations.iter().collect();
    ordered.sort_by_key(|d| (d.order, direction_rank(d.direction))); // stable, as OrderBy.ThenBy
    let dests: Vec<Destination> = ordered
        .into_iter()
        .map(|d| Destination {
            symbol_id: None,
            distance: None,
            ..d.clone()
        })
        .collect();
    let dest_refs: Vec<&Destination> = dests.iter().collect();

    let section = engine.build_section(Direction::Ahead, &dest_refs, &palette)?;

    let embedded_shift = embedded_col_w.map_or(0.0, |w| w + p.word_spacing);
    let width = 2.0 * border + section.min_width + embedded_shift;
    let height = 2.0 * border + section.height;

    let mut prims = sign_plate::draw(pack, width, height, &palette);
    let arrow = engine.draw_section(
        &mut prims,
        &section,
        border,
        width - border,
        border,
        height - border,
        &palette,
        p,
        embedded_shift,
    )?;

    if let (Some(col_w), Some(st)) = (embedded_col_w, &pack.stack) {
        let emb_cx =
            border + st.arrow_to_border + section.arrow_across + st.arrow_to_legend + col_w / 2.0;
        crate::embedded::draw_stacked(
            &mut prims,
            sign,
            pack,
            emb_cx,
            height - border - p.pad_top,
            border + p.pad_bottom,
            palette.ground,
        )?;
    }

    Ok(RawLayout {
        prims,
        diagnostics: diags,
        width,
        height,
        symbols: vec![arrow],
    })
}
