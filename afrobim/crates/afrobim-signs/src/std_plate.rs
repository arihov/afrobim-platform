// SPDX-License-Identifier: LicenseRef-AfroBIM-Proprietary
// Copyright (c) 2026 AfroBIM. All rights reserved.
//! Supplementary plates, and the bordered plate outline they share with other rectangular
//! signs. Port of C# `SupplementaryPlateLayout` and `SignPlate`.
//!
//! A supplementary plate adds to or clarifies a main sign. The plate body always draws: a
//! rounded rectangle with a black border round a white (permanent) or yellow (temporary)
//! ground; everything inside is black. The border width, corner radius and the gutters are
//! read from the pack's `panel` section and are never hardcoded here.
//!
//! In a pack with a `signBorder` section (the UK pack) a plate with a legend is the UK TSM
//! Chapter 7 Figure 2-4 plate: black Transport Heavy lettering measured in its own face, one
//! centred line per `|`-separated part of [`SignDefinition::info_distance`], inside the
//! `signBorder` outline (1.5 sw inside, 3 sw outside radius). This is the plate
//! [`crate::std_tsrgd`] hangs under a warning or regulatory sign.
//!
//! The `symbol_id` names the plate profile, which selects the content when there is no UK
//! legend. These profiles and their representative content are the C# engine's, written for the
//! Uganda TSM (Vol.1 §D.9, Vol.2 App.1 §3.2 and App.3 S01–S21), and are reproduced as they are:
//! - `distance` (S01): one centred line, the distance ("800 m" is the sign-page example);
//! - `extension` (S02): the word "For" and a distance ("For 3.2 km");
//! - `direction` (S03): a single reversible arrow, pointing left unless mirrored right;
//! - `bus` / `truck` / `car` / `motorcycle` (S11–S14): a vehicle-category symbol; one with no
//!   artwork is a placeholder with a `SYMBOL_UNRESOLVED` warning;
//! - anything else (S21 variable text): 1, 2 or 3 centred lines of the sign-page examples, the
//!   count from [`SignDefinition::text_lines`].

use afrobim_draw::Pt2;

use crate::measurer::TextTileMeasurer;
use crate::model::{Colour, ColourScheme, Diagnostic, FontFace, SignDefinition, SignMirror};
use crate::pack::{PanelRules, RulePack};
use crate::prim::{Prim, TextRun};
use crate::std_arrow;
use crate::std_pack::{SignBorderRules, StdRules};
use crate::std_signs::{validate_constraints_default_clause, StdLayout};
use crate::symbol;
use crate::SignError;

const CLAUSE: &str = "Uganda TSM Vol.1 §D.9 / Vol.2 App.1 §3.2 · App.3 S01–S21";

/// Standard text x-height in sw (App.1 §1.1: sw = x-height ÷ 4).
const TEXT_X_HEIGHT_SW: f64 = 4.0;

// Content-box side for a single-symbol or arrow plate, in sw. The plate body is then sized to
// carry it with the pack's gutters; these are the C# engine's proportions, not manual values.
const SYMBOL_BOX_SW: f64 = 8.0;
/// Tail-to-tip length of the direction arrow.
const ARROW_SPAN_SW: f64 = 14.0;
/// Shaft stroke of the direction arrow.
const ARROW_STROKE_SW: f64 = 2.0;

/// The plate of a rectangular sign in a pack with a `signBorder` section (UK TSM Chapter 7
/// §2.15): the whole plate in the border colour with `outer_corner_radius` corners, then the
/// ground inset by the border width with the panel's corner radius. C# `SignPlate.Draw`.
pub(crate) fn bordered_plate(
    panel: &PanelRules,
    border: &SignBorderRules,
    width: f64,
    height: f64,
    border_colour: Colour,
    ground: Colour,
) -> Vec<Prim> {
    let b = panel.border_width;
    let segments = border.segments_per_corner;
    vec![
        plate_rounded_rect(
            0.0,
            0.0,
            width,
            height,
            border.outer_corner_radius,
            segments,
            border_colour,
        ),
        plate_rounded_rect(
            b,
            b,
            width - b,
            height - b,
            panel.corner_radius,
            segments,
            ground,
        ),
    ]
}

/// A rounded rectangle between two corners, each 90° arc drawn with `segments` chords,
/// anticlockwise from the bottom-left corner.
fn plate_rounded_rect(
    x0: f64,
    y0: f64,
    x1: f64,
    y1: f64,
    r: f64,
    segments: i64,
    colour: Colour,
) -> Prim {
    let r = r.min((x1 - x0).min(y1 - y0) / 2.0);
    let mut outline = Vec::new();
    for (cx, cy, start_deg) in [
        (x0 + r, y0 + r, 180.0), // bottom-left
        (x1 - r, y0 + r, 270.0), // bottom-right
        (x1 - r, y1 - r, 0.0),   // top-right
        (x0 + r, y1 - r, 90.0),  // top-left
    ] {
        for i in 0..=segments {
            let t = (start_deg + 90.0 * i as f64 / segments as f64) * std::f64::consts::PI / 180.0;
            outline.push(Pt2::new(cx + r * t.cos(), cy + r * t.sin()));
        }
    }
    Prim::Fill { outline, colour }
}

pub(crate) struct SupplementaryPlateLayout<'a> {
    pub pack: &'a RulePack,
    pub std: &'a StdRules,
    pub measurer: &'a TextTileMeasurer<'a>,
}

impl SupplementaryPlateLayout<'_> {
    pub fn layout(&self, sign: &SignDefinition) -> Result<StdLayout, SignError> {
        let diags = validate_constraints_default_clause(sign, self.pack);

        // Colours are family-fixed: white permanent / yellow temporary ground, black border,
        // black content. They are resolved directly, not through the guidance colour palette.
        let temporary = sign.scheme == ColourScheme::Temporary
            || (sign.scheme == ColourScheme::Auto && sign.background == Colour::Yellow);
        let ground = if temporary {
            Colour::Yellow
        } else {
            Colour::White
        };

        // The profile decides the body's interior only; the bordered ground always draws.
        let id = sign
            .symbol_id
            .as_deref()
            .unwrap_or("")
            .trim()
            .to_lowercase();
        match id.as_str() {
            "distance" => {
                let lines = self
                    .uk_legend(sign)
                    .unwrap_or_else(|| vec![distance_text(sign)]);
                self.text_plate(ground, diags, &lines)
            }
            "extension" => self.text_plate(ground, diags, &[extension_text(sign)]),
            "direction" => self.arrow_plate(sign, ground, diags),
            "bus" | "truck" | "car" | "motorcycle" => self.symbol_plate(sign, ground, diags, &id),
            // Variable text (S21) and any un-profiled plate: N centred lines, N from
            // `text_lines`. A UK plate carries its own legend instead (TSRGD associated-plate
            // legends).
            _ => {
                let lines = self
                    .uk_legend(sign)
                    .unwrap_or_else(|| variable_lines(sign.text_lines));
                self.text_plate(ground, diags, &lines)
            }
        }
    }

    // ─── text plates (S01 distance, S02 extension, S21 variable text) ─────────

    fn text_plate(
        &self,
        ground: Colour,
        mut diags: Vec<Diagnostic>,
        lines: &[String],
    ) -> Result<StdLayout, SignError> {
        let p = &self.pack.panel;
        // A UK plate is the Chapter 7 Figure 2-4 plate: black Transport Heavy lettering measured
        // in its own face, and the signBorder plate outline.
        let uk = self.std.sign_border.as_ref();
        let face = if uk.is_some() {
            FontFace::TransportHeavy
        } else {
            FontFace::TransportMedium
        };
        let tiles = lines
            .iter()
            .map(|l| self.measurer.width(l, face))
            .collect::<Result<Vec<f64>, SignError>>()?;
        let widest = tiles.iter().copied().fold(0.0, f64::max);

        let content_h = lines.len() as f64 * p.text_tile_height;
        let w = 2.0 * p.border_width + 2.0 * p.pad_side + widest;
        let h = 2.0 * p.border_width + p.pad_top + content_h + p.pad_bottom;

        let mut prims = match uk {
            Some(border) => bordered_plate(p, border, w, h, Colour::Black, ground),
            None => body(p, w, h, ground),
        };

        let y_top = h - p.border_width - p.pad_top;
        for (i, (line, tile)) in lines.iter().zip(&tiles).enumerate() {
            let foot = y_top - (i + 1) as f64 * p.text_tile_height; // one tile per line
            let x = (w - tile) / 2.0; // §3.2: all text lines centred
            prims.push(Prim::Text(TextRun {
                origin: Pt2::new(x, foot),
                text: line.clone(),
                face: FontFace::TransportHeavy,
                x_height: TEXT_X_HEIGHT_SW,
                ink: Some(Colour::Black),
            }));
        }

        finish(&prims, &mut diags);
        Ok(StdLayout::plain(prims, diags, w, h))
    }

    // ─── symbol plates (S11–S14 vehicle categories) ───────────────────────────

    fn symbol_plate(
        &self,
        sign: &SignDefinition,
        ground: Colour,
        mut diags: Vec<Diagnostic>,
        symbol_id: &str,
    ) -> Result<StdLayout, SignError> {
        let p = &self.pack.panel;
        let w = 2.0 * p.border_width + 2.0 * p.pad_side + SYMBOL_BOX_SW;
        let h = 2.0 * p.border_width + p.pad_top + SYMBOL_BOX_SW + p.pad_bottom;

        let mut prims = body(p, w, h, ground);

        // Centred symbol box: real black artwork where the library has it, else one placeholder.
        let box_min = Pt2::new((w - SYMBOL_BOX_SW) / 2.0, (h - SYMBOL_BOX_SW) / 2.0);
        prims.extend(symbol::resolve(
            symbol_id,
            SYMBOL_BOX_SW,
            box_min,
            0.0,
            self.measurer,
        )?);

        if sign.mirror == SignMirror::Right {
            flip_interior(&mut prims, 2, w / 2.0);
        }

        finish(&prims, &mut diags);
        Ok(StdLayout::plain(prims, diags, w, h))
    }

    // ─── direction arrow plate (S03, reversible) ──────────────────────────────

    fn arrow_plate(
        &self,
        sign: &SignDefinition,
        ground: Colour,
        mut diags: Vec<Diagnostic>,
    ) -> Result<StdLayout, SignError> {
        let p = &self.pack.panel;
        let head_len = std_arrow::head_back_length(ARROW_STROKE_SW);
        let across = std_arrow::head_across(ARROW_STROKE_SW); // 3 × stroke wide

        let w = 2.0 * p.border_width + 2.0 * p.pad_side + ARROW_SPAN_SW;
        let h = 2.0 * p.border_width + p.pad_top + across + p.pad_bottom;

        let mut prims = body(p, w, h, ground);

        // The base arrow points left; a right mirror flips it.
        let point_left = sign.mirror != SignMirror::Right;
        let (cx, cy, half_len) = (w / 2.0, h / 2.0, ARROW_SPAN_SW / 2.0);
        let dx = if point_left { -1.0 } else { 1.0 };
        let tip_x = cx + dx * half_len;
        let base_x = tip_x - dx * head_len;
        let tail_x = cx - dx * half_len;
        prims.push(Prim::Line {
            from: Pt2::new(tail_x, cy),
            to: Pt2::new(base_x, cy),
            width: ARROW_STROKE_SW,
            colour: Some(Colour::Black),
        });
        // +90° points −X (left); −90° points +X (right).
        prims.push(Prim::Fill {
            outline: std_arrow::place_head(
                Pt2::new(tip_x, cy),
                if point_left { 90.0 } else { -90.0 },
                ARROW_STROKE_SW,
            ),
            colour: Colour::Black,
        });

        finish(&prims, &mut diags);
        Ok(StdLayout::plain(prims, diags, w, h))
    }

    /// A UK plate's legend: `info_distance`, one line per `|`-separated part. `None` for a pack
    /// without a `signBorder` (the Uganda pack) or a plate with no legend, which keep the
    /// representative content.
    fn uk_legend(&self, sign: &SignDefinition) -> Option<Vec<String>> {
        self.std.sign_border.as_ref()?;
        let legend = sign
            .info_distance
            .as_deref()
            .filter(|l| !l.trim().is_empty())?;
        Some(legend_lines(legend))
    }
}

/// The lines of a plate legend: the `|`-separated parts, trimmed, empty ones dropped.
pub(crate) fn legend_lines(legend: &str) -> Vec<String> {
    legend
        .split('|')
        .map(str::trim)
        .filter(|l| !l.is_empty())
        .map(str::to_string)
        .collect()
}

/// The bordered ground of a profile plate: a black rounded-rectangle border frame with the
/// coloured ground inset by one border width.
fn body(p: &PanelRules, w: f64, h: f64, ground: Colour) -> Vec<Prim> {
    let b = p.border_width;
    let r_out = p.corner_radius.min(w.min(h) / 2.0);
    let r_in = (p.corner_radius - b).max(0.0);
    vec![
        rounded_rect(0.0, 0.0, w, h, r_out, Colour::Black),
        rounded_rect(b, b, w - 2.0 * b, h - 2.0 * b, r_in, ground),
    ]
}

/// A rounded rectangle (6 chords per corner, with the ends of the straight edges repeated) as
/// one anticlockwise ring from the bottom-left corner.
fn rounded_rect(x0: f64, y0: f64, w: f64, h: f64, r: f64, colour: Colour) -> Prim {
    const SEG: u32 = 6;
    let r = r.min(w.min(h) / 2.0);
    let mut pts = Vec::new();
    let arc = |pts: &mut Vec<Pt2>, cx: f64, cy: f64, a0: f64, a1: f64| {
        for i in 0..=SEG {
            let t = (a0 + (a1 - a0) * f64::from(i) / f64::from(SEG)) * std::f64::consts::PI / 180.0;
            pts.push(Pt2::new(cx + r * t.cos(), cy + r * t.sin()));
        }
    };
    arc(&mut pts, x0 + r, y0 + r, 180.0, 270.0); // bottom-left
    pts.push(Pt2::new(x0 + w - r, y0)); // bottom edge
    arc(&mut pts, x0 + w - r, y0 + r, 270.0, 360.0); // bottom-right
    pts.push(Pt2::new(x0 + w, y0 + h - r)); // right edge
    arc(&mut pts, x0 + w - r, y0 + h - r, 0.0, 90.0); // top-right
    pts.push(Pt2::new(x0 + r, y0 + h)); // top edge
    arc(&mut pts, x0 + r, y0 + h - r, 90.0, 180.0); // top-left
    pts.push(Pt2::new(x0, y0 + r)); // left edge
    Prim::Fill {
        outline: pts,
        colour,
    }
}

/// A placeholder is drawn, and flagged, for a symbol with no artwork.
fn finish(prims: &[Prim], diags: &mut Vec<Diagnostic>) {
    for prim in prims {
        if let Prim::Symbol { id, .. } = prim {
            diags.push(Diagnostic::warning(
                "SYMBOL_UNRESOLVED",
                format!("Symbol '{id}' has no artwork; plate body drawn, placeholder emitted"),
                CLAUSE,
            ));
        }
    }
}

/// Reflects the interior primitives (index ≥ `from`) about the plate's vertical centre line
/// `cx`, leaving the bordered ground in place. A placeholder's origin is moved as the C# engine
/// moves it: to the mirror of its right edge less its size again, which is one box width left
/// of where its mirror image starts.
pub(crate) fn flip_interior(prims: &mut [Prim], from: usize, cx: f64) {
    let flip = |x: f64| 2.0 * cx - x;
    for prim in prims.iter_mut().skip(from) {
        match prim {
            Prim::Fill { outline, .. } => outline.iter_mut().for_each(|p| p.x = flip(p.x)),
            Prim::Line { from, to, .. } => {
                from.x = flip(from.x);
                to.x = flip(to.x);
            }
            Prim::Text(t) => t.origin.x = flip(t.origin.x),
            Prim::Symbol { origin, size, .. } => origin.x = flip(origin.x + *size) - *size,
        }
    }
}

/// S01 distance plate. "800 m" is the sign-page example (Uganda TSM Vol.2 p.153); a chosen
/// distance overrides it.
fn distance_text(sign: &SignDefinition) -> String {
    match sign.info_distance.as_deref() {
        Some(d) if !d.trim().is_empty() => d.to_string(),
        _ => "800 m".to_string(),
    }
}

/// S02 extension plate: the word "For" and a distance; "For 3.2 km" is the representative one.
fn extension_text(sign: &SignDefinition) -> String {
    match sign.info_distance.as_deref() {
        Some(d) if !d.trim().is_empty() => format!("For {d}"),
        _ => "For 3.2 km".to_string(),
    }
}

/// S21 variable text: the standard's example strings over 1, 2 or 3 centred lines
/// (Uganda TSM Vol.2 p.160).
fn variable_lines(lines: Option<u32>) -> Vec<String> {
    let text: &[&str] = match lines.unwrap_or(1) {
        3.. => &["Except", "buses", "and taxis"],
        2 => &["School", "buses"],
        _ => &["Except buses"],
    };
    text.iter().map(|s| s.to_string()).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn outline(p: &Prim) -> &[Pt2] {
        match p {
            Prim::Fill { outline, .. } => outline,
            _ => panic!("not a fill"),
        }
    }

    fn bounds(o: &[Pt2]) -> (f64, f64, f64, f64) {
        o.iter()
            .fold((f64::MAX, f64::MAX, f64::MIN, f64::MIN), |b, p| {
                (b.0.min(p.x), b.1.min(p.y), b.2.max(p.x), b.3.max(p.y))
            })
    }

    /// UK TSM Chapter 7 Figure 2-4: border 1.5 sw, 3 sw outside radius, 1.5 sw inside radius,
    /// so the two arcs are concentric.
    #[test]
    fn the_bordered_plate_has_concentric_corners() {
        let pack = crate::pack::tests::uk_pack();
        let border = SignBorderRules {
            outer_corner_radius: 3.0,
            segments_per_corner: 12,
        };
        let plate = bordered_plate(
            &pack.panel,
            &border,
            40.0,
            20.0,
            Colour::Black,
            Colour::White,
        );
        assert_eq!(plate.len(), 2);
        let (outer, inner) = (outline(&plate[0]), outline(&plate[1]));
        assert_eq!((outer.len(), inner.len()), (52, 52));
        assert_eq!(bounds(outer), (0.0, 0.0, 40.0, 20.0));
        assert_eq!(bounds(inner), (1.5, 1.5, 38.5, 18.5));
        // Bottom-left corner: both arcs start on the left edge and turn about (3, 3).
        assert!((outer[0].x - 0.0).abs() < 1e-12 && (outer[0].y - 3.0).abs() < 1e-12);
        assert!((inner[0].x - 1.5).abs() < 1e-12 && (inner[0].y - 3.0).abs() < 1e-12);
        for p in &outer[..13] {
            assert!((p.distance_to(Pt2::new(3.0, 3.0)) - 3.0).abs() < 1e-12);
        }
        for p in &inner[..13] {
            assert!((p.distance_to(Pt2::new(3.0, 3.0)) - 1.5).abs() < 1e-12);
        }
    }

    #[test]
    fn a_legend_splits_into_trimmed_lines() {
        assert_eq!(
            legend_lines(" Single file | traffic "),
            ["Single file", "traffic"]
        );
        assert_eq!(legend_lines("For 2 miles"), ["For 2 miles"]);
        assert_eq!(legend_lines("a||b| "), ["a", "b"]);
        assert!(legend_lines(" | ").is_empty());
    }

    #[test]
    fn the_representative_content_follows_the_sign_page_examples() {
        assert_eq!(variable_lines(None), ["Except buses"]);
        assert_eq!(variable_lines(Some(1)), ["Except buses"]);
        assert_eq!(variable_lines(Some(2)), ["School", "buses"]);
        assert_eq!(variable_lines(Some(5)), ["Except", "buses", "and taxis"]);
        let mut sign = SignDefinition::stack("S", 60, Vec::new());
        assert_eq!(distance_text(&sign), "800 m");
        assert_eq!(extension_text(&sign), "For 3.2 km");
        sign.info_distance = Some("2 miles".into());
        assert_eq!(distance_text(&sign), "2 miles");
        assert_eq!(extension_text(&sign), "For 2 miles");
    }

    #[test]
    fn flipping_leaves_the_body_and_mirrors_the_interior() {
        let mut prims = vec![
            crate::prim::rect(0.0, 0.0, 10.0, 4.0, Colour::Black),
            crate::prim::rect(1.0, 1.0, 9.0, 3.0, Colour::White),
            crate::prim::rect(2.0, 1.0, 3.0, 2.0, Colour::Black),
            Prim::Symbol {
                origin: Pt2::new(1.0, 1.0),
                id: "truck".into(),
                size: 2.0,
                rotation_deg: 0.0,
            },
        ];
        flip_interior(&mut prims, 2, 5.0);
        assert_eq!(bounds(outline(&prims[1])), (1.0, 1.0, 9.0, 3.0));
        assert_eq!(bounds(outline(&prims[2])), (7.0, 1.0, 8.0, 2.0));
        // As in C#: the placeholder's origin is the mirror of its right edge less its size
        // again, so the box (1..3) is placed at 5, not at 7 where its mirror image starts.
        assert!(matches!(&prims[3], Prim::Symbol { origin, .. } if origin.x == 5.0));
    }
}
