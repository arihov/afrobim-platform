// SPDX-License-Identifier: LicenseRef-AfroBIM-Proprietary
// Copyright (c) 2026 AfroBIM. All rights reserved.
//! Lane pre-selection signs. Port of C# `LanePreselectionLayout` (Uganda TSM Appendix 3 page
//! G05).
//!
//! One column per lane: a directional arrow over that lane's destination, with thin
//! lane-divide lines between the columns. Lanes are laid out left-to-right in the order given
//! (proximity `order`), matching the physical approach. Each lane column is as wide as its
//! widest of the standard arrow or its destination name, so the arrows line up on a common
//! band at the top and the names sit beneath them.
//!
//! The arrow band is `routeArms.minLengthVertical` high and each arrow is a stroked shaft with
//! the pack's route-arm head ([`crate::route_head`]): with the UK pack, the full 8/3 sw shaft
//! and the triangle head. The UK pack has no catalog entry for this sign, the plate has no
//! drawn border, and route numbers are not drawn.

use afrobim_draw::Pt2;

use crate::arrow;
use crate::measurer::TextTileMeasurer;
use crate::model::{Colour, Destination, Direction, SignDefinition};
use crate::pack::{ArrowRules, RulePack};
use crate::prim::{rect, Prim, TextRun};
use crate::route_head;
use crate::stack::{resolve_palette, validate_constraints, RawLayout};
use crate::SignError;

pub(crate) fn layout(
    pack: &RulePack,
    measurer: &TextTileMeasurer,
    sign: &SignDefinition,
) -> Result<RawLayout, SignError> {
    let diags = validate_constraints(sign, pack);
    let p = &pack.panel;
    let palette = resolve_palette(sign, pack);
    let face = palette.face;

    let mut lanes: Vec<&Destination> = sign.destinations.iter().collect();
    lanes.sort_by_key(|d| d.order); // stable, as OrderBy

    let arrows = pack.arrows.as_ref().filter(|_| !lanes.is_empty());
    let arrow_col_w = arrows.map_or(0.0, arrow::total_width);
    let lane_pad = p.word_spacing;
    let arrow_band_h = match arrows {
        Some(_) => pack.dir.route_arms(&pack.id)?.min_length_vertical,
        None => 0.0,
    };
    let arrow_name_gap = if arrows.is_some() { lane_pad } else { 0.0 };

    // Per-lane content width and running x-offsets.
    let lane_tiles = lanes
        .iter()
        .map(|d| measurer.width(&d.name, face))
        .collect::<Result<Vec<f64>, SignError>>()?;
    let lane_w: Vec<f64> = lane_tiles
        .iter()
        .map(|tile_width| 2.0 * lane_pad + arrow_col_w.max(*tile_width))
        .collect();
    let interior_w: f64 = lane_w.iter().sum();

    let width = 2.0 * p.border_width + interior_w;
    let height = 2.0 * p.border_width
        + p.pad_top
        + arrow_band_h
        + arrow_name_gap
        + p.text_tile_height
        + p.pad_bottom;

    let mut prims = vec![rect(0.0, 0.0, width, height, palette.ground)];

    let top_interior = height - p.border_width - p.pad_top;
    let bottom_interior = p.border_width + p.pad_bottom;
    let arrow_tail_y = top_interior - arrow_band_h;
    let name_foot = arrow_tail_y - arrow_name_gap - p.text_tile_height;

    let mut x = p.border_width;
    for (i, lane) in lanes.iter().enumerate() {
        let lane_cx = x + lane_w[i] / 2.0;

        // Lane-divide line on the left edge of every column but the first.
        if i > 0 {
            prims.push(Prim::Line {
                from: Pt2::new(x, bottom_interior),
                to: Pt2::new(x, top_interior),
                width: p.border_width,
                colour: Some(palette.arm),
            });
        }

        if let Some(r) = arrows {
            build_lane_arrow(
                &mut prims,
                lane.direction,
                lane_cx,
                arrow_col_w,
                top_interior,
                arrow_tail_y,
                r,
                palette.arm,
            )?;
        }

        let name_x = lane_cx - lane_tiles[i] / 2.0;
        prims.push(Prim::Text(TextRun {
            origin: Pt2::new(name_x, name_foot),
            text: lane.name.clone(),
            face,
            x_height: 4.0,
            ink: Some(palette.lettering),
        }));

        x += lane_w[i];
    }

    Ok(RawLayout {
        prims,
        diagnostics: diags,
        width,
        height,
        symbols: Vec::new(),
    })
}

/// Compact arrow for one lane cell (shaft line plus head) pointing ahead (up), left, or right,
/// centred on `lane_cx` and vertically centred within the arrow band for the side directions.
#[allow(clippy::too_many_arguments)]
fn build_lane_arrow(
    prims: &mut Vec<Prim>,
    dir: Direction,
    lane_cx: f64,
    cell_w: f64,
    tip_y: f64,
    tail_y: f64,
    r: &ArrowRules,
    arm: Colour,
) -> Result<(), SignError> {
    let stroke = r.shaft_width;
    let cell_l = lane_cx - cell_w / 2.0;
    let cell_r = lane_cx + cell_w / 2.0;

    if dir == Direction::Ahead {
        // Per-lane ahead arrow, over the lane's vertical run.
        let a_stroke = route_head::route_arm_stroke(r, stroke, tip_y - tail_y)?;
        prims.push(Prim::Line {
            from: Pt2::new(lane_cx, tail_y),
            to: Pt2::new(lane_cx, tip_y - route_head::route_head_length(r, a_stroke)?),
            width: a_stroke,
            colour: Some(arm),
        });
        prims.push(Prim::Fill {
            outline: route_head::place_route_head(r, Pt2::new(lane_cx, tip_y), 0.0, a_stroke)?,
            colour: arm,
        });
        return Ok(());
    }

    // The lane cell is only the arrow's width across, the run a side arrow has.
    let s_stroke = route_head::route_arm_stroke(r, stroke, cell_r - cell_l)?;
    let head_len = route_head::route_head_length(r, s_stroke)?;

    let right = dir == Direction::Right;
    let mid_y = (tip_y + tail_y) / 2.0;
    let tip_x = if right { cell_r } else { cell_l };
    let base_x = if right {
        cell_r - head_len
    } else {
        cell_l + head_len
    };
    let tail_x = if right { cell_l } else { cell_r };

    prims.push(Prim::Line {
        from: Pt2::new(tail_x, mid_y),
        to: Pt2::new(base_x, mid_y),
        width: s_stroke,
        colour: Some(arm),
    });
    prims.push(Prim::Fill {
        outline: route_head::place_route_head(
            r,
            Pt2::new(tip_x, mid_y),
            if right { -90.0 } else { 90.0 },
            s_stroke,
        )?,
        colour: arm,
    });
    Ok(())
}
