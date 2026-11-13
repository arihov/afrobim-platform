//! Route confirmatory signs. Port of C# `ConfirmatorySignLayout` for a pack with a `blocks`
//! section (UK TSM Chapter 7 §8; App.1 §2.5, Figure 8).
//!
//! A confirmatory sign repeats the destination names after a junction with their distances,
//! and there is no arrow. The names, route numbers, patches, panels and distances are laid out
//! by [`DestinationColumn`], the stack-type engine, with one distance column for the whole
//! sign, centred and at least `DistanceRules::min_gap` clear of the longest name (§3.9.1–3.9.2).
//! Chapter 7 gives route confirmatory signs no separate block rules; this follows the
//! stack-type behaviour (design spec §13). Symbols are not laid out on confirmatory signs.
//!
//! Not ported: the C# engine's original layout (a name column and a right-aligned or centred
//! distance column of plain text runs), which it uses for a pack without `blocks` (the Uganda
//! pack). Only its no-destination case, which every pack takes, is ported: the plate alone.

use crate::column::{DestinationColumn, Edge};
use crate::direction::direction_rank;
use crate::measurer::TextTileMeasurer;
use crate::model::{Destination, Diagnostic, SignDefinition};
use crate::pack::RulePack;
use crate::sign_plate;
use crate::stack::{resolve_palette, validate_constraints, RawLayout};
use crate::SignError;

pub(crate) fn layout(
    pack: &RulePack,
    measurer: &TextTileMeasurer,
    sign: &SignDefinition,
) -> Result<RawLayout, SignError> {
    let mut diags = validate_constraints(sign, pack);
    let p = &pack.panel;
    let palette = resolve_palette(sign, pack);

    let mut ordered: Vec<&Destination> = sign.destinations.iter().collect();
    ordered.sort_by_key(|d| (d.order, direction_rank(d.direction))); // stable, as OrderBy.ThenBy

    // Every name and distance is measured in the sign's face first, as the C# engine does, so
    // a character the font cannot draw is reported the same way.
    for d in &ordered {
        measurer.measure(&d.name, palette.face)?;
    }
    let mut has_distances = false;
    for d in &ordered {
        if let Some(dist) = d.distance.as_deref().filter(|s| !s.is_empty()) {
            measurer.measure(dist, palette.face)?;
            has_distances = true;
        }
    }
    // A distance is optional per row; a confirmatory sign is meaningless without any, so warn
    // (not error) when none are supplied.
    if !ordered.is_empty() && !has_distances {
        diags.push(Diagnostic::warning(
            "CONFIRMATORY_NO_DISTANCE",
            "Confirmatory sign has no distances — a confirmatory sign confirms distance travelled"
                .to_string(),
            pack.clause("confirmatory", "Uganda TSM Vol.2 App.1 §2.5"),
        ));
    }

    let border = p.border_width;
    if ordered.is_empty() {
        // The original engine with nothing to draw: the plate alone.
        let width = 2.0 * border + 2.0 * p.pad_side;
        let height = 2.0 * border + p.pad_top + p.pad_bottom;
        return Ok(RawLayout {
            // The plate: the ground, and the §2.15 border when the pack draws one.
            prims: sign_plate::draw(pack, width, height, &palette),
            diagnostics: diags,
            width,
            height,
            symbols: Vec::new(),
        });
    }

    let Some(blocks) = &pack.blocks else {
        return Err(SignError::Unsupported(format!(
            "confirmatory signs for rule pack '{}': it has no 'blocks' section, and only the destination-block \
             (UK TSM Chapter 7 §3.2-3.9) confirmatory layout is ported",
            pack.id
        )));
    };

    // The stack-type destination engine (routeNumbers clause), its distances aligned in one
    // centred column at least DistanceRules.MinGap clear of the longest name (§3.9.1).
    let dests: Vec<Destination> = ordered
        .into_iter()
        .map(|d| Destination {
            symbol_id: None,
            ..d.clone()
        })
        .collect();
    let dest_refs: Vec<&Destination> = dests.iter().collect();
    let column = DestinationColumn::build(
        &dest_refs,
        palette.ground,
        palette.lettering,
        palette.face,
        pack,
        measurer,
        &|_| 0.0,
        false,
        true,
    )?;

    // §4.2.5 as on a stack-type sign: 1.5 sw below tiles, 2.5 sw below a panel or patch.
    let pad_bottom = if column.bottom == Edge::Tiles {
        p.pad_bottom
    } else {
        blocks.panel_pad_bottom_patch
    };
    let width = 2.0 * border + 2.0 * p.pad_side + column.width;
    let height = 2.0 * border + p.pad_top + column.height + pad_bottom;

    // The plate: the ground, and the §2.15 border when the pack draws one.
    let mut prims = sign_plate::draw(pack, width, height, &palette);
    column.draw(
        &mut prims,
        border + p.pad_side,
        height - border - p.pad_top,
        measurer,
    )?;

    Ok(RawLayout {
        prims,
        diagnostics: diags,
        width,
        height,
        symbols: Vec::new(),
    })
}
