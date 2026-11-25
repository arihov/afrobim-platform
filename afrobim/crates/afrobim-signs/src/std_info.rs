// SPDX-License-Identifier: LicenseRef-AfroBIM-Proprietary
// Copyright (c) 2026 AfroBIM. All rights reserved.
//! Information signs: a symbol on a white sub-panel with an optional arrow and distance, or a
//! symbol straight on the coloured ground. Port of C# `InformationSignLayout`.
//!
//! The two templates are the C# engine's reading of Uganda TSM Vol.2 App.3 §5:
//! - `ServicePanel` (services, places, tourist): a white square sub-panel carrying a black
//!   symbol, with an arrow band and a distance band beneath it in the ground's lettering
//!   colour, each one text tile tall and separated by a word space;
//! - `SymbolPlate` (road and traffic): the symbol alone on the ground, in the lettering colour.
//!
//! Gutters, border width, tile height and word spacing come from the pack's `panel` section,
//! colours and face from its palette. A symbol the pack's `symbols` section gives no size for
//! is drawn in the engine's 14 sw box. A symbol with no artwork is a positioned placeholder
//! with a `SYMBOL_UNRESOLVED` warning citing the pack's `information` clause.

use afrobim_draw::Pt2;

use crate::arrow;
use crate::measurer::TextTileMeasurer;
use crate::model::{Colour, Diagnostic, Direction, FontFace, InformationTemplate, SignDefinition};
use crate::pack::{ArrowRules, RulePack};
use crate::prim::{rect, Prim, TextRun};
use crate::stack::{resolve_palette, validate_constraints, Palette};
use crate::std_arrow;
use crate::std_signs::StdLayout;
use crate::symbol;
use crate::SignError;

/// The symbol box where the pack fixes no size: the C# engine's 14 sw services default.
const DEFAULT_SYMBOL_SIZE: f64 = 14.0;
/// Arrow shaft width in a pack with no `arrows` section (C# fallback).
const DEFAULT_ARROW_STROKE: f64 = 3.0;
/// Arrow band width in a pack with no `arrows` section (C# fallback).
const DEFAULT_ARROW_REACH: f64 = 9.0;

pub(crate) struct InformationSignLayout<'a> {
    pub pack: &'a RulePack,
    pub measurer: &'a TextTileMeasurer<'a>,
}

impl InformationSignLayout<'_> {
    pub fn layout(&self, sign: &SignDefinition) -> Result<StdLayout, SignError> {
        let diags = validate_constraints(sign, self.pack);
        let palette = resolve_palette(sign, self.pack);
        if sign.info_template == InformationTemplate::ServicePanel {
            self.service_panel(sign, &palette, diags)
        } else {
            self.symbol_plate(sign, &palette, diags)
        }
    }

    // ─── services / places / tourist panel ───────────────────────────────────

    fn service_panel(
        &self,
        sign: &SignDefinition,
        palette: &Palette,
        mut diags: Vec<Diagnostic>,
    ) -> Result<StdLayout, SignError> {
        let p = &self.pack.panel;
        let b = p.border_width;
        let sym_size = self.symbol_size(sign.symbol_id.as_deref());
        let panel_side = sym_size + 2.0 * p.pad_side;

        let distance = sign
            .info_distance
            .as_deref()
            .filter(|d| !d.trim().is_empty());

        let dist_w = match distance {
            Some(d) => self.measurer.width(d, palette.face)?,
            None => 0.0,
        };
        let arrow_w = if sign.info_arrow.is_some() {
            self.arrow_reach()
        } else {
            0.0
        };
        let content_w = panel_side.max(dist_w.max(arrow_w));

        // Beneath the sub-panel: an arrow band then a distance band, each one text tile tall,
        // separated by a word space.
        let bands = f64::from(u8::from(sign.info_arrow.is_some()) + u8::from(distance.is_some()));
        let info_total = if bands == 0.0 {
            0.0
        } else {
            bands * p.text_tile_height + bands * p.word_spacing
        };

        let width = 2.0 * b + 2.0 * p.pad_side + content_w;
        let height = 2.0 * b + p.pad_top + panel_side + info_total + p.pad_bottom;

        let mut prims = vec![rect(0.0, 0.0, width, height, palette.ground)];

        let cx = width / 2.0;
        let inner_top = height - b - p.pad_top;
        let panel_bottom = inner_top - panel_side;

        // White square sub-panel, centred horizontally.
        let px0 = cx - panel_side / 2.0;
        prims.push(rect(
            px0,
            panel_bottom,
            px0 + panel_side,
            inner_top,
            Colour::White,
        ));

        // The symbol, centred on the sub-panel.
        let panel_cy = (inner_top + panel_bottom) / 2.0;
        self.emit_symbol(
            &mut prims,
            sign,
            sym_size,
            cx,
            panel_cy,
            Colour::Black,
            &mut diags,
        )?;

        // Arrow / distance bands beneath the panel, in the ground's lettering colour.
        let mut cursor_y = panel_bottom - p.word_spacing;
        if let Some(dir) = sign.info_arrow {
            self.build_arrow(&mut prims, dir, cx, cursor_y, palette.lettering);
            cursor_y -= p.text_tile_height + p.word_spacing;
        }
        if let Some(d) = distance {
            let y = cursor_y - p.text_tile_height;
            prims.push(Prim::Text(TextRun {
                origin: Pt2::new(cx - dist_w / 2.0, y),
                text: d.to_string(),
                face: palette.face,
                x_height: 4.0,
                ink: Some(palette.lettering),
            }));
        }

        Ok(StdLayout::plain(prims, diags, width, height))
    }

    // ─── road & traffic plain plate ──────────────────────────────────────────

    fn symbol_plate(
        &self,
        sign: &SignDefinition,
        palette: &Palette,
        mut diags: Vec<Diagnostic>,
    ) -> Result<StdLayout, SignError> {
        let p = &self.pack.panel;
        let b = p.border_width;
        let sym_size = self.symbol_size(sign.symbol_id.as_deref());
        let content = sym_size + 2.0 * p.pad_side;

        let width = 2.0 * b + 2.0 * p.pad_side + content;
        let height = 2.0 * b + p.pad_top + content + p.pad_bottom;

        let mut prims = vec![rect(0.0, 0.0, width, height, palette.ground)];

        let cx = width / 2.0;
        let cy = (height - b - p.pad_top + b + p.pad_bottom) / 2.0;
        self.emit_symbol(
            &mut prims,
            sign,
            sym_size,
            cx,
            cy,
            palette.lettering,
            &mut diags,
        )?;

        Ok(StdLayout::plain(prims, diags, width, height))
    }

    // ─── shared helpers ──────────────────────────────────────────────────────

    #[allow(clippy::too_many_arguments)]
    fn emit_symbol(
        &self,
        prims: &mut Vec<Prim>,
        sign: &SignDefinition,
        size: f64,
        cx: f64,
        cy: f64,
        ink: Colour,
        diags: &mut Vec<Diagnostic>,
    ) -> Result<(), SignError> {
        let box_min = Pt2::new(cx - size / 2.0, cy - size / 2.0);

        // Hospital / information letter-symbols are drawn as text; other pictograms go through
        // the resolver, which returns real vector geometry (the parking "P" square, the drawn
        // library) and only falls back to a positioned placeholder for a symbol with no artwork.
        if let Some(id) = sign.symbol_id.as_deref().filter(|id| !id.is_empty()) {
            if let Some(glyph) = letter_symbol(id) {
                // The glyph fills the symbol box: capital height = box size, centred on it.
                let cap = size;
                let xh = cap / TextRun::CAP_HEIGHT_PER_X_HEIGHT;
                let w = self.measurer.width(glyph, FontFace::TransportHeavy)? * (xh / 4.0);
                prims.push(Prim::Text(TextRun::from_baseline(
                    Pt2::new(cx - w / 2.0, cy - cap / 2.0),
                    glyph,
                    FontFace::TransportHeavy,
                    cap,
                    Some(ink),
                )));
                return Ok(());
            }
            prims.extend(symbol::resolve_inked(
                id,
                size,
                box_min,
                0.0,
                self.measurer,
                ink,
            )?);
        }

        for prim in prims.iter() {
            if let Prim::Symbol { id, .. } = prim {
                diags.push(Diagnostic::warning(
                    "SYMBOL_UNRESOLVED",
                    format!("Symbol '{id}' has no artwork; placeholder emitted"),
                    self.pack.clause("information", "Uganda TSM Vol.2 App.3 §5"),
                ));
            }
        }
        Ok(())
    }

    /// Compact shaft and head directional arrow sized to the info band. Ahead points up;
    /// left/right point sideways from the centre.
    fn build_arrow(
        &self,
        prims: &mut Vec<Prim>,
        dir: Direction,
        cx: f64,
        band_bottom: f64,
        colour: Colour,
    ) {
        let arrows: Option<&ArrowRules> = self.pack.arrows.as_ref();
        let stroke = arrows.map_or(DEFAULT_ARROW_STROKE, |a| a.shaft_width);
        let band_h = self.pack.panel.text_tile_height;
        let mid_y = band_bottom - band_h / 2.0;
        let line = |from: Pt2, to: Pt2, width: f64| Prim::Line {
            from,
            to,
            width,
            colour: Some(colour),
        };
        let fill = |outline: Vec<Pt2>| Prim::Fill { outline, colour };

        if dir == Direction::Ahead && !std_arrow::uses_figure5_head(arrows) {
            // Chevron-style (UK) packs keep the compact triangle head on the full shaft. The
            // shaft stops at the head's base: it is cut as a rectangle of its true width, so
            // running on to the tip would leave its square corners outside the head.
            let tri_head = std_arrow::route_head_length(arrows, stroke);
            let tip_y = mid_y + band_h / 2.0;
            prims.push(line(
                Pt2::new(cx, band_bottom - band_h),
                Pt2::new(cx, tip_y - tri_head),
                stroke,
            ));
            prims.push(fill(std_arrow::place_triangle_head(
                Pt2::new(cx, tip_y),
                0.0,
                stroke,
            )));
            return;
        }

        if dir == Direction::Ahead {
            // Scale the whole arrow to the band height so the standard head and shaft stay
            // about half each and never overflow the row.
            let a_stroke = std_arrow::fitting_stroke(stroke, band_h);
            let a_back = std_arrow::head_back_length(a_stroke);
            let tip_y = band_bottom; // top of band, matching the side-branch tip
            prims.push(line(
                Pt2::new(cx, band_bottom - band_h),
                Pt2::new(cx, tip_y - a_back),
                a_stroke,
            ));
            prims.push(fill(std_arrow::place_head(
                Pt2::new(cx, tip_y),
                0.0,
                a_stroke,
            )));
            return;
        }

        let right = dir == Direction::Right;
        let reach = self.arrow_reach();
        // The reach column is only the arrow's total width wide; a standard side head runs
        // deeper than that horizontally, so it is scaled to fit the run.
        let s_stroke = std_arrow::route_arm_stroke(arrows, stroke, reach);
        let head_len = std_arrow::route_head_length(arrows, s_stroke);
        let tip_x = if right {
            cx + reach / 2.0
        } else {
            cx - reach / 2.0
        };
        let base_x = if right {
            tip_x - head_len
        } else {
            tip_x + head_len
        };
        let tail_x = if right {
            cx - reach / 2.0
        } else {
            cx + reach / 2.0
        };
        prims.push(line(
            Pt2::new(tail_x, mid_y),
            Pt2::new(base_x, mid_y),
            s_stroke,
        ));
        prims.push(fill(std_arrow::place_route_head(
            arrows,
            Pt2::new(tip_x, mid_y),
            if right { -90.0 } else { 90.0 },
            s_stroke,
        )));
    }

    fn arrow_reach(&self) -> f64 {
        self.pack
            .arrows
            .as_ref()
            .map_or(DEFAULT_ARROW_REACH, arrow::total_width)
    }

    fn symbol_size(&self, symbol_id: Option<&str>) -> f64 {
        symbol_id
            .filter(|id| !id.is_empty())
            .and_then(|id| self.pack.symbols.as_ref()?.find(id))
            .map(|spec| spec.size_sw)
            .filter(|s| *s > 0.0)
            .unwrap_or(DEFAULT_SYMBOL_SIZE)
    }
}

/// The letter a hospital or information symbol is drawn as, or `None` for any other symbol.
fn letter_symbol(id: &str) -> Option<&'static str> {
    match id.trim().to_lowercase().as_str() {
        "h" | "hospital" | "hospital-h" => Some("H"),
        "i" | "information" | "information-i" => Some("i"),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hospital_and_information_are_letters() {
        assert_eq!(letter_symbol(" Hospital "), Some("H"));
        assert_eq!(letter_symbol("h"), Some("H"));
        assert_eq!(letter_symbol("information-i"), Some("i"));
        assert_eq!(letter_symbol("I"), Some("i"));
        assert_eq!(letter_symbol("hotel"), None);
        assert_eq!(letter_symbol(""), None);
    }
}
