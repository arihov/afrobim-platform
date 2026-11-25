// SPDX-License-Identifier: LicenseRef-AfroBIM-Proprietary
// Copyright (c) 2026 AfroBIM. All rights reserved.
//! Place-identification signs. Port of C# `PlaceIdentificationLayout` (Uganda TSM Appendix 3
//! page G22: `G22.1` entry, `G22.2` exit).
//!
//! Both variants are a plain panel with the place names centred on it, in the colours and face
//! the sign's scheme or background resolves to (black Transport Heavy on a white ground). The
//! exit variant adds a grey diagonal band running top-right to bottom-left that crosses out
//! the name: the standard's "you are leaving this place" device. The variant is selected from
//! `SignDefinition::variant`: `SignVariant::Primary` is the entry, `SignVariant::Secondary`
//! (the default) the exit.
//!
//! The layout reads only the pack's panel dimensions, so it is the same geometry with any
//! pack. The UK pack has no catalog entry for it, the plate has no drawn border, and route
//! numbers are not drawn.

use afrobim_draw::Pt2;

use crate::direction::direction_rank;
use crate::measurer::TextTileMeasurer;
use crate::model::{Colour, Destination, SignDefinition, SignVariant};
use crate::pack::RulePack;
use crate::prim::{rect, Prim, TextRun};
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

    let mut names: Vec<&Destination> = sign.destinations.iter().collect();
    names.sort_by_key(|d| (d.order, direction_rank(d.direction))); // stable, as OrderBy.ThenBy

    let tiles = names
        .iter()
        .map(|d| measurer.width(&d.name, face))
        .collect::<Result<Vec<f64>, SignError>>()?;
    let widest = tiles.iter().copied().fold(0.0, f64::max);

    let width = 2.0 * p.border_width + 2.0 * p.pad_side + widest;
    let height =
        2.0 * p.border_width + p.pad_top + names.len() as f64 * p.text_tile_height + p.pad_bottom;

    let mut prims = vec![rect(0.0, 0.0, width, height, palette.ground)];

    // Exit strike-out band: a grey parallelogram swept from the top-right of the interior down
    // to the bottom-left. Its width is a modest provisional proportion (2 × border) of the C#
    // engine; the corner-to-corner extent is what reads as "exit".
    let is_exit = sign.variant == SignVariant::Secondary;
    if is_exit && !names.is_empty() {
        let band = 2.0 * p.border_width;
        let left = p.border_width;
        let right = width - p.border_width;
        let bottom = p.border_width;
        let top = height - p.border_width;
        // Parallelogram: top edge runs band-wide at the top-right corner, the bottom edge
        // band-wide at the bottom-left, joined by two parallel diagonals. All four vertices
        // stay within [left,right]×[bottom,top].
        prims.push(Prim::Fill {
            outline: vec![
                Pt2::new(right, top),          // top-right
                Pt2::new(right - band, top),   // top-left
                Pt2::new(left, bottom),        // bottom-left
                Pt2::new(left + band, bottom), // bottom-right
            ],
            colour: Colour::Grey,
        });
    }

    let y_top = height - p.border_width - p.pad_top;
    for (i, (d, tile_width)) in names.iter().zip(&tiles).enumerate() {
        let foot = y_top - (i + 1) as f64 * p.text_tile_height;
        let x = (width - tile_width) / 2.0; // centred name
        prims.push(Prim::Text(TextRun {
            origin: Pt2::new(x, foot),
            text: d.name.clone(),
            face,
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
