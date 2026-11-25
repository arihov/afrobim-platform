// SPDX-License-Identifier: LicenseRef-AfroBIM-Proprietary
// Copyright (c) 2026 AfroBIM. All rights reserved.
//! The guidance layouts a pack without the UK TSM Chapter 7 sections takes (the Uganda pack):
//!
//! - stack-type advance direction signs with one arrow per row (App.1 §2.3, Figure 6): the
//!   layout of C# `StackSignLayout.Layout` for a pack without `stack` and `arrows` sections
//!   together;
//! - direction signs with one stroked arrow beside the stack (App.1 §2.4, Figure 7): the
//!   original engine of C# `DirectionSignLayout.Layout`;
//! - confirmatory signs with names ranged left and distances in a column (App.1 §2.5,
//!   Figure 8): the engine of C# `ConfirmatorySignLayout.Layout` for a pack without `blocks`.
//!
//! Not ported: embedded warning / regulatory signs (C# `EmbeddedSignGeometry`), which the C#
//! stack-type and direction layouts place in a column between the arrows and the lettering.
//! A sign that carries any is [`SignError::Unsupported`] here.

use afrobim_draw::Pt2;

use crate::arrow;
use crate::column::{DestinationColumn, Edge};
use crate::direction::direction_rank;
use crate::measurer::TextTileMeasurer;
use crate::model::{Colour, Destination, Diagnostic, Direction, SignDefinition};
use crate::pack::{ArrowRules, RulePack};
use crate::prim::{rect, Prim, TextRun};
use crate::route_head;
use crate::sign_plate;
use crate::stack::{resolve_palette, validate_constraints, RawLayout};
use crate::symbol;
use crate::SignError;

/// True when a stack-type sign takes the per-row layout: the pack lacks the `stack` or the
/// `arrows` section the panelled layout needs.
pub(crate) fn takes_stack(pack: &RulePack) -> bool {
    pack.stack.is_none() || pack.arrows.is_none()
}

/// True when a direction sign takes the original engine: it has destinations and the pack
/// lacks the `stack` or the `arrows` section of the stack-type section layout.
pub(crate) fn takes_direction(pack: &RulePack, sign: &SignDefinition) -> bool {
    takes_stack(pack) && !sign.destinations.is_empty()
}

/// True when a confirmatory sign takes the plain layout: it has destinations and the pack has
/// no `blocks` section.
pub(crate) fn takes_confirmatory(pack: &RulePack, sign: &SignDefinition) -> bool {
    pack.blocks.is_none() && !sign.destinations.is_empty()
}

fn refuse_embedded(sign: &SignDefinition, family: &str) -> Result<(), SignError> {
    if sign.embedded.as_ref().is_some_and(|e| !e.is_empty()) {
        return Err(SignError::Unsupported(format!(
            "embedded warning/regulatory signs on a {family} sign ('{}'): the embedded-sign geometry is not ported",
            sign.code
        )));
    }
    Ok(())
}

/// A stroked line's true-width rectangle, as the overlap check tests it (C#
/// `LayoutOverlapCheck.LineRect`). Empty for a line without length or width.
pub(crate) fn line_rect(from: Pt2, to: Pt2, width: f64) -> Vec<Pt2> {
    let (x0, y0, x1, y1) = (from.x, from.y, to.x, to.y);
    let len = ((x1 - x0) * (x1 - x0) + (y1 - y0) * (y1 - y0)).sqrt();
    // Written so that a NaN width registers nothing, as `Width > 0` does.
    let has_width = width > 0.0;
    if !has_width || len < 1e-12 {
        return Vec::new();
    }
    let h = width / 2.0;
    let (nx, ny) = (-(y1 - y0) / len * h, (x1 - x0) / len * h);
    vec![
        Pt2::new(x0 - nx, y0 - ny),
        Pt2::new(x1 - nx, y1 - ny),
        Pt2::new(x1 + nx, y1 + ny),
        Pt2::new(x0 + nx, y0 + ny),
    ]
}

/// Pushes a shaft and its head, and registers both for the overlap check.
fn push_arrow(
    prims: &mut Vec<Prim>,
    symbols: &mut Vec<Vec<Pt2>>,
    from: Pt2,
    to: Pt2,
    stroke: f64,
    head: Vec<Pt2>,
    arm: Colour,
) {
    symbols.push(line_rect(from, to, stroke));
    symbols.push(head.clone());
    prims.push(Prim::Line {
        from,
        to,
        width: stroke,
        colour: Some(arm),
    });
    prims.push(Prim::Fill {
        outline: head,
        colour: arm,
    });
}

// ─── stack-type, one arrow per row ───────────────────────────────────────────

// App.1 §2.3.1 stacking order: straight-on, then right-turn, then left-turn.
fn stack_rank(d: Direction) -> u8 {
    match d {
        Direction::Ahead => 0,
        Direction::Right => 1,
        Direction::Left => 2,
    }
}

/// Stack-type advance direction signs. App.1 §2.3 (Figure 6): destinations stacked
/// left-aligned with a directional arrow per row in a dedicated arrow column, and (optionally)
/// an embedded-symbol column between the arrows and the lettering (App.1 §2.1.3). Rows are
/// ordered straight-on, right-turn, left-turn (§2.3.1).
pub(crate) fn stack(
    pack: &RulePack,
    measurer: &TextTileMeasurer,
    sign: &SignDefinition,
) -> Result<RawLayout, SignError> {
    refuse_embedded(sign, "stack-type")?;
    let mut diags = validate_constraints(sign, pack);
    let p = &pack.panel;

    // Colours + lettering face resolved from the scheme (App.1 §1.2 / App.3 table), falling
    // back to the explicit background for pre-scheme signs.
    let palette = resolve_palette(sign, pack);
    let face = palette.face;

    // App.1 §2.3.1: straight-on arrow top, right-turn middle, left-turn bottom; proximity rank
    // (order) then breaks ties within a direction.
    let mut ordered: Vec<&Destination> = sign.destinations.iter().collect();
    ordered.sort_by_key(|d| (stack_rank(d.direction), d.order)); // stable, as OrderBy.ThenBy

    let mut widest: f64 = 0.0;
    for d in &ordered {
        widest = widest.max(measurer.width(&d.name, face)?);
    }

    // Arrow column spans the widest head-to-head across the standard arrow (App.1 Figure 5).
    // Absent an `arrows` section (a legacy pack) the layout is text only.
    let arrows = pack.arrows.as_ref().filter(|_| !ordered.is_empty());
    let arrow_col_w = arrows.map_or(0.0, arrow::total_width);
    let arrow_gap = if arrows.is_some() {
        p.word_spacing
    } else {
        0.0
    };

    // Embedded-symbol column (App.1 §2.1.3): sized by the symbol library entry, defaulting to
    // the 10 sw nominal pictogram box.
    let symbol_size = |id: &str| match pack.symbols.as_ref().and_then(|s| s.find(id)) {
        Some(spec) if spec.size_sw > 0.0 => spec.size_sw,
        _ => 10.0,
    };
    let symbol_col_w = ordered
        .iter()
        .filter_map(|d| d.symbol_id.as_deref())
        .map(symbol_size)
        .reduce(f64::max);
    let symbol_gap = if symbol_col_w.is_some() {
        p.word_spacing
    } else {
        0.0
    };
    let symbol_col_w = symbol_col_w.unwrap_or(0.0);

    let width = 2.0 * p.border_width
        + 2.0 * p.pad_side
        + arrow_col_w
        + arrow_gap
        + symbol_col_w
        + symbol_gap
        + widest;
    let height =
        2.0 * p.border_width + p.pad_top + ordered.len() as f64 * p.text_tile_height + p.pad_bottom;

    let mut prims = vec![rect(0.0, 0.0, width, height, palette.ground)];

    let x_arrow_left = p.border_width + p.pad_side;
    let x_arrow_right = x_arrow_left + arrow_col_w;
    let x_symbol = x_arrow_right + arrow_gap;
    let x_text = x_symbol + symbol_col_w + symbol_gap;

    let y_top = height - p.border_width - p.pad_top;
    let mut symbols = Vec::new();

    for (i, dest) in ordered.iter().enumerate() {
        let foot = y_top - (i + 1) as f64 * p.text_tile_height;
        let cell_top = y_top - i as f64 * p.text_tile_height;
        let cell_bot = cell_top - p.text_tile_height;
        let row_mid = (cell_top + cell_bot) / 2.0;

        // Directional arrow in the arrow column.
        if let Some(r) = arrows {
            build_row_arrow(
                &mut prims,
                &mut symbols,
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

        // Embedded symbol between the arrow and the destination (§2.1.3). Resolved symbols
        // (e.g. the parking "P" square) become real geometry; a symbol with no artwork stays a
        // placeholder and picks up the warning below.
        if let Some(sym) = dest.symbol_id.as_deref() {
            let size = symbol_size(sym);
            let rot = symbol::clamp_not_below_horizontal(symbol::rotation_for(dest.direction)); // §2.1.4
            prims.extend(symbol::resolve_inked(
                sym,
                size,
                Pt2::new(x_symbol, row_mid - size / 2.0),
                rot,
                measurer,
                palette.lettering,
            )?);
        }

        prims.push(Prim::Text(TextRun {
            origin: Pt2::new(x_text, foot),
            text: dest.name.clone(),
            face,
            x_height: 4.0,
            ink: Some(palette.lettering),
        }));
    }

    for prim in &prims {
        if let Prim::Symbol { id, .. } = prim {
            diags.push(Diagnostic::warning(
                "SYMBOL_UNRESOLVED",
                format!("Symbol '{id}' has no artwork; placeholder emitted"),
                pack.clause("symbolPlacement", "Uganda TSM Vol.2 App.1 §2.1.4"),
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

/// A compact per-row directional arrow (Figure 5 proportions) built as a shaft plus the
/// route-arm head. Straight-on points up, left-turn left, right-turn right; the head tip
/// reaches the leading edge of the row's cell.
#[allow(clippy::too_many_arguments)]
fn build_row_arrow(
    prims: &mut Vec<Prim>,
    symbols: &mut Vec<Vec<Pt2>>,
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
        // A per-row ahead arrow has only one text band of height: scale the whole standard
        // arrow to that run so its slender head never overruns the cell.
        let a_stroke = route_head::route_arm_stroke(r, stroke, cell_top - cell_bot)?;
        let head_len = route_head::route_head_length(r, a_stroke)?;
        let head = route_head::place_route_head(r, Pt2::new(cx, cell_top), 0.0, a_stroke)?;
        push_arrow(
            prims,
            symbols,
            Pt2::new(cx, cell_bot),
            Pt2::new(cx, cell_top - head_len),
            a_stroke,
            head,
            arm,
        );
        return Ok(());
    }

    // The arrow column is only the head's across wide, so a full-size side head would run past
    // the column and swallow the shaft. Fit the whole side arrow to the column's horizontal
    // run, exactly as the ahead arrow is fitted to its band.
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

    // +90° points −X (left); −90° points +X (right).
    let head = route_head::place_route_head(
        r,
        Pt2::new(tip_x, row_mid),
        if right { -90.0 } else { 90.0 },
        s_stroke,
    )?;
    push_arrow(
        prims,
        symbols,
        Pt2::new(tail_x, row_mid),
        Pt2::new(base_x, row_mid),
        s_stroke,
        head,
        arm,
    );
    Ok(())
}

// ─── direction signs, the original engine ────────────────────────────────────

/// Direction signs. App.1 §2.4 (Figure 7), Appendix 3 page G11.
///
/// Destinations (place names) are stacked left-aligned; a single arrow, not one per row as on
/// a stack-type sign, is centred between the top and bottom border and points the way to every
/// listed destination. With an `arrows` section the arrow is a compact shaft and head spanning
/// the interior; without one the sign is the text-only stack. With a `blocks` section the
/// destinations are laid out by [`DestinationColumn`]. Symbols and distances are not laid out
/// on direction signs.
pub(crate) fn direction(
    pack: &RulePack,
    measurer: &TextTileMeasurer,
    sign: &SignDefinition,
) -> Result<RawLayout, SignError> {
    refuse_embedded(sign, "direction")?;
    let diags = validate_constraints(sign, pack);
    let p = &pack.panel;
    let palette = resolve_palette(sign, pack);
    let face = palette.face;

    // §2.1.2: nearer destination (lower order) renders above; within equal proximity the
    // listed order breaks ties.
    let mut ordered: Vec<&Destination> = sign.destinations.iter().collect();
    ordered.sort_by_key(|d| (d.order, direction_rank(d.direction))); // stable, as OrderBy.ThenBy

    let column = match &pack.blocks {
        Some(_) if !ordered.is_empty() => {
            let dests: Vec<Destination> = ordered
                .iter()
                .map(|d| Destination {
                    symbol_id: None,
                    distance: None,
                    ..(*d).clone()
                })
                .collect();
            let refs: Vec<&Destination> = dests.iter().collect();
            Some(DestinationColumn::build(
                &refs,
                palette.ground,
                palette.lettering,
                face,
                pack,
                measurer,
                &|_| 0.0,
                false,
                false,
            )?)
        }
        _ => None,
    };

    let widest = match &column {
        Some(c) => c.width,
        None => {
            let mut widest: f64 = 0.0;
            for d in &ordered {
                widest = widest.max(measurer.width(&d.name, face)?);
            }
            widest
        }
    };
    let legend_h = match &column {
        Some(c) => c.height,
        None => ordered.len() as f64 * p.text_tile_height,
    };
    // §4.2.5 as on a stack-type sign: 1.5 sw below tiles, 2.5 sw below a panel or patch.
    let pad_bottom = match (&column, &pack.blocks) {
        (Some(c), Some(blocks)) if c.bottom != Edge::Tiles => blocks.panel_pad_bottom_patch,
        _ => p.pad_bottom,
    };

    let arrows = pack.arrows.as_ref().filter(|_| !ordered.is_empty());
    let arrow_col_w = arrows.map_or(0.0, arrow::total_width);
    let arrow_gap = if arrows.is_some() {
        p.word_spacing
    } else {
        0.0
    };

    let width = 2.0 * p.border_width + 2.0 * p.pad_side + arrow_col_w + arrow_gap + widest;
    let height = 2.0 * p.border_width + p.pad_top + legend_h + pad_bottom;

    // The plate: the ground, and the §2.15 border when the pack draws one.
    let mut prims = sign_plate::draw(pack, width, height, &palette);

    let x_text = p.border_width + p.pad_side + arrow_col_w + arrow_gap;
    let y_top = height - p.border_width - p.pad_top;

    let mut symbols = Vec::new();

    // One arrow, centred between the top and bottom borders, pointing ahead (up): the
    // destinations all lie on the same bearing.
    if let Some(r) = arrows {
        let arrow_cx = p.border_width + p.pad_side + arrow_col_w / 2.0;
        let tip_y = height - p.border_width - p.pad_top; // top of interior
        let tail_y = p.border_width + p.pad_bottom; // bottom of interior
        let stroke = route_head::route_arm_stroke(r, r.shaft_width, tip_y - tail_y)?;
        let head_len = route_head::route_head_length(r, stroke)?; // arm reserve that fits the run
        let head = route_head::place_route_head(r, Pt2::new(arrow_cx, tip_y), 0.0, stroke)?;
        push_arrow(
            &mut prims,
            &mut symbols,
            Pt2::new(arrow_cx, tail_y),
            Pt2::new(arrow_cx, tip_y - head_len),
            stroke,
            head,
            palette.arm,
        );
    }

    match &column {
        Some(c) => c.draw(&mut prims, x_text, y_top, measurer)?,
        None => {
            for (i, d) in ordered.iter().enumerate() {
                let foot = y_top - (i + 1) as f64 * p.text_tile_height;
                prims.push(Prim::Text(TextRun {
                    origin: Pt2::new(x_text, foot),
                    text: d.name.clone(),
                    face,
                    x_height: 4.0,
                    ink: Some(palette.lettering),
                }));
            }
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

// ─── confirmatory signs without destination blocks ───────────────────────────

/// Confirmatory signs for a pack without `blocks` (App.1 §2.5, Figure 8): names ranged left,
/// one per tile row, and each row's distance in a column at the right. A pack with a
/// `distances` section (UK TSM Chapter 7 §3.9) sets the name-to-distance gap and centres the
/// distances in their column; otherwise the gap is the word space and the distances are ranged
/// right. A confirmatory sign is meaningless without any distance, so that is a warning.
pub(crate) fn confirmatory(
    pack: &RulePack,
    measurer: &TextTileMeasurer,
    sign: &SignDefinition,
) -> Result<RawLayout, SignError> {
    let mut diags = validate_constraints(sign, pack);
    let p = &pack.panel;
    let palette = resolve_palette(sign, pack);
    let face = palette.face;

    let mut ordered: Vec<&Destination> = sign.destinations.iter().collect();
    ordered.sort_by_key(|d| (d.order, direction_rank(d.direction))); // stable, as OrderBy.ThenBy

    let mut widest_name: f64 = 0.0;
    for d in &ordered {
        widest_name = widest_name.max(measurer.width(&d.name, face)?);
    }
    // A distance is optional per row.
    let mut dist_w: Vec<Option<f64>> = Vec::with_capacity(ordered.len());
    for d in &ordered {
        dist_w.push(match d.distance.as_deref().filter(|s| !s.is_empty()) {
            Some(dist) => Some(measurer.width(dist, face)?),
            None => None,
        });
    }
    let has_distances = dist_w.iter().any(Option::is_some);
    if !ordered.is_empty() && !has_distances {
        diags.push(Diagnostic::warning(
            "CONFIRMATORY_NO_DISTANCE",
            "Confirmatory sign has no distances — a confirmatory sign confirms distance travelled"
                .to_string(),
            pack.clause("confirmatory", "Uganda TSM Vol.2 App.1 §2.5"),
        ));
    }

    let widest_dist = dist_w.iter().flatten().copied().fold(0.0, f64::max);
    let dist_rules = pack.distances.as_ref();
    let gap = if has_distances {
        dist_rules.map_or(p.word_spacing, |d| d.min_gap)
    } else {
        0.0
    };

    let width = 2.0 * p.border_width + 2.0 * p.pad_side + widest_name + gap + widest_dist;
    let height =
        2.0 * p.border_width + p.pad_top + ordered.len() as f64 * p.text_tile_height + p.pad_bottom;

    // The plate: the ground, and the §2.15 border when the pack draws one.
    let mut prims = sign_plate::draw(pack, width, height, &palette);

    let x_name_left = p.border_width + p.pad_side;
    let x_dist_right = width - p.border_width - p.pad_side;
    let y_top = height - p.border_width - p.pad_top;

    for (i, d) in ordered.iter().enumerate() {
        let foot = y_top - (i + 1) as f64 * p.text_tile_height;
        prims.push(Prim::Text(TextRun {
            origin: Pt2::new(x_name_left, foot),
            text: d.name.clone(),
            face,
            x_height: 4.0,
            ink: Some(palette.lettering),
        }));

        if let (Some(w), Some(dist)) = (dist_w[i], d.distance.as_deref()) {
            let dist_x = match dist_rules {
                None => x_dist_right - w,
                Some(_) => x_dist_right - widest_dist / 2.0 - w / 2.0,
            };
            prims.push(Prim::Text(TextRun {
                origin: Pt2::new(dist_x, foot),
                text: dist.to_string(),
                face,
                x_height: 4.0,
                ink: Some(palette.lettering),
            }));
        }
    }

    Ok(RawLayout {
        prims,
        diagnostics: diags,
        width,
        height,
        symbols: Vec::new(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn close(a: f64, b: f64) -> bool {
        (a - b).abs() < 1e-12
    }

    #[test]
    fn a_stroked_line_is_tested_as_the_rectangle_of_its_width() {
        let r = line_rect(Pt2::new(2.0, 1.0), Pt2::new(2.0, 9.0), 3.0);
        let want = [(3.5, 1.0), (3.5, 9.0), (0.5, 9.0), (0.5, 1.0)];
        assert_eq!(r.len(), 4);
        for (p, (x, y)) in r.iter().zip(want) {
            assert!(close(p.x, x) && close(p.y, y), "{p:?}");
        }
        assert!(line_rect(Pt2::new(2.0, 1.0), Pt2::new(2.0, 1.0), 3.0).is_empty());
        assert!(line_rect(Pt2::new(0.0, 0.0), Pt2::new(5.0, 0.0), 0.0).is_empty());
    }

    #[test]
    fn stack_rows_run_ahead_then_right_then_left() {
        assert!(stack_rank(Direction::Ahead) < stack_rank(Direction::Right));
        assert!(stack_rank(Direction::Right) < stack_rank(Direction::Left));
    }
}
