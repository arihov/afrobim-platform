// SPDX-License-Identifier: LicenseRef-AfroBIM-Proprietary
// Copyright (c) 2026 AfroBIM. All rights reserved.
//! Temporary diversion signs. Port of C# `DiversionSignLayout` (Uganda TSM Appendix 3 §4 pages
//! G31, diversion ahead, and G32, diverted traffic).
//!
//! The destination(s) the driver must follow, each with a directional arrow pointing the way
//! around the closure: a dedicated arrow column plus left-aligned destinations stacked
//! straight-on, right, left (App.1 §2.3.1). Colour, lettering face and arm colour all resolve
//! from the scheme, so the `Temporary` scheme paints the yellow ground with black Transport
//! Heavy; a sign that only names a yellow background takes the legacy §1.2 rule instead (white
//! Transport Medium, as on any non-white ground).
//!
//! Each arrow is a stroked shaft with the pack's route-arm head ([`crate::route_head`]): with
//! the UK pack, the full 8/3 sw shaft and the triangle head. The UK pack has no catalog entry
//! for this sign, the plate has no drawn border, and route numbers are not drawn.

use afrobim_draw::Pt2;

use crate::arrow;
use crate::measurer::TextTileMeasurer;
use crate::model::{Colour, Destination, Direction, SignDefinition};
use crate::pack::{ArrowRules, RulePack};
use crate::prim::{rect, Prim, TextRun};
use crate::route_head;
use crate::stack::{resolve_palette, validate_constraints, RawLayout};
use crate::SignError;

/// Straight-on first, then right, then left (App.1 §2.3.1).
fn direction_rank(d: Direction) -> u8 {
    match d {
        Direction::Ahead => 0,
        Direction::Right => 1,
        Direction::Left => 2,
    }
}

pub(crate) fn layout(
    pack: &RulePack,
    measurer: &TextTileMeasurer,
    sign: &SignDefinition,
) -> Result<RawLayout, SignError> {
    let diags = validate_constraints(sign, pack);
    let p = &pack.panel;
    let palette = resolve_palette(sign, pack);

    // Straight-on first, then right, then left; Order breaks ties.
    let mut ordered: Vec<&Destination> = sign.destinations.iter().collect();
    ordered.sort_by_key(|d| (direction_rank(d.direction), d.order)); // stable, as OrderBy.ThenBy

    let mut widest: f64 = 0.0;
    for d in &ordered {
        widest = widest.max(measurer.width(&d.name, palette.face)?);
    }

    let arrows = pack.arrows.as_ref().filter(|_| !ordered.is_empty());
    let arrow_col_w = arrows.map_or(0.0, arrow::total_width);
    let arrow_gap = if arrows.is_some() {
        p.word_spacing
    } else {
        0.0
    };

    let width = 2.0 * p.border_width + 2.0 * p.pad_side + arrow_col_w + arrow_gap + widest;
    let height =
        2.0 * p.border_width + p.pad_top + ordered.len() as f64 * p.text_tile_height + p.pad_bottom;

    let mut prims = vec![rect(0.0, 0.0, width, height, palette.ground)];

    let x_arrow_left = p.border_width + p.pad_side;
    let x_arrow_right = x_arrow_left + arrow_col_w;
    let x_text = x_arrow_right + arrow_gap;
    let y_top = height - p.border_width - p.pad_top;

    for (i, dest) in ordered.iter().enumerate() {
        let foot = y_top - (i + 1) as f64 * p.text_tile_height;
        let cell_top = y_top - i as f64 * p.text_tile_height;
        let cell_bot = cell_top - p.text_tile_height;
        let row_mid = (cell_top + cell_bot) / 2.0;

        if let Some(r) = arrows {
            build_arrow(
                &mut prims,
                dest.direction,
                x_arrow_left,
                x_arrow_right,
                cell_top,
                cell_bot,
                row_mid,
                r,
                palette.arm,
            )?;
        }

        prims.push(Prim::Text(TextRun {
            origin: Pt2::new(x_text, foot),
            text: dest.name.clone(),
            face: palette.face,
            x_height: 4.0,
            ink: Some(palette.lettering),
        }));
    }

    Ok(RawLayout {
        prims,
        diagnostics: diags,
        width,
        height,
        symbols: Vec::new(),
    })
}

/// A compact per-row directional arrow: a shaft plus the pack's route-arm head, pointing up
/// for straight-on, left or right for a turn.
#[allow(clippy::too_many_arguments)]
fn build_arrow(
    prims: &mut Vec<Prim>,
    dir: Direction,
    x_left: f64,
    x_right: f64,
    cell_top: f64,
    cell_bot: f64,
    row_mid: f64,
    r: &ArrowRules,
    arm: Colour,
) -> Result<(), SignError> {
    let stroke = r.shaft_width;
    let cx = (x_left + x_right) / 2.0;

    if dir == Direction::Ahead {
        // Per-row ahead arrow, over the one-band run.
        let a_stroke = route_head::route_arm_stroke(r, stroke, cell_top - cell_bot)?;
        prims.push(Prim::Line {
            from: Pt2::new(cx, cell_bot),
            to: Pt2::new(cx, cell_top - route_head::route_head_length(r, a_stroke)?),
            width: a_stroke,
            colour: Some(arm),
        });
        prims.push(Prim::Fill {
            outline: route_head::place_route_head(r, Pt2::new(cx, cell_top), 0.0, a_stroke)?,
            colour: arm,
        });
        return Ok(());
    }

    // The side arrow runs across the arrow column.
    let s_stroke = route_head::route_arm_stroke(r, stroke, x_right - x_left)?;
    let head_len = route_head::route_head_length(r, s_stroke)?;

    let right = dir == Direction::Right;
    let tip_x = if right { x_right } else { x_left };
    let base_x = if right {
        x_right - head_len
    } else {
        x_left + head_len
    };
    let tail_x = if right { x_left } else { x_right };

    prims.push(Prim::Line {
        from: Pt2::new(tail_x, row_mid),
        to: Pt2::new(base_x, row_mid),
        width: s_stroke,
        colour: Some(arm),
    });
    prims.push(Prim::Fill {
        outline: route_head::place_route_head(
            r,
            Pt2::new(tip_x, row_mid),
            if right { -90.0 } else { 90.0 },
            s_stroke,
        )?,
        colour: arm,
    });
    Ok(())
}
