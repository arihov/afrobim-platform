// SPDX-License-Identifier: LicenseRef-AfroBIM-Proprietary
// Copyright (c) 2026 AfroBIM. All rights reserved.
//! Warning and regulatory signs drawn to the Uganda Traffic Signs Manual: the plain warning
//! triangle, the prohibitory and mandatory discs with their slashes, arrows and numerals, the
//! Stop octagon and the Give Way triangle. Port of C# `WarningSignLayout` and
//! `RegulatorySignLayout` (the drawing they do when the pack has no TSRGD artwork for the
//! sign's symbol; a sign with TSRGD artwork is drawn by [`crate::std_tsrgd`]).
//!
//! Everything is authored in stroke-width units at a fixed canonical size (a 50 sw triangle
//! side or disc diameter): only the proportions are fixed here. The physical body size (Vol.1
//! Table D.1 / D.3 / D.4) is document metadata in the pack's `standardSizes`, never baked into
//! this geometry.
//!
//! The border, ring, slash, bar and numeral proportions are not stated numerically by the
//! Uganda manual. They are the pack's clearly labelled provisional UK-derived fractions
//! (`roundels-triangles.xml`), with the C# engine's own fallbacks when a pack carries none.

use afrobim_draw::Pt2;

use crate::arrow;
use crate::measurer::TextTileMeasurer;
use crate::model::{
    Colour, ColourScheme, Diagnostic, FontFace, SignDefinition, SignKind, SignMirror,
};
use crate::pack::{ArrowRules, RulePack};
use crate::prim::{Prim, TextRun};
use crate::route_head;
use crate::std_plate::flip_interior;
use crate::std_signs::StdLayout;
use crate::symbol;
use crate::ug_pack::RoundelTriangleRules;
use crate::SignError;

/// Canonical standalone triangle side and disc / octagon diameter in sw. A fixed scale in sw
/// space; the mm body size is metadata.
const DIA: f64 = 50.0;
const R: f64 = DIA / 2.0; // outer radius
const CX: f64 = DIA / 2.0; // centre of the plate square
const CY: f64 = DIA / 2.0;

const WARNING_CLAUSE: &str = "Uganda TSM Vol.1 §Warning Signs / Vol.2 App.3";
const REGULATORY_CLAUSE: &str = "Uganda TSM Vol.1 §Regulatory Signs / Vol.2 App.3";

// Fallbacks for the labelled UK-derived GAP fractions when the pack carries no provisional
// value (inventory §8.1 items 1–3, §6.4/§6.5/§6.6).
const DEFAULT_RING_FRACTION: f64 = 0.125; // red band / ring / white border ≈ 1/8 of the size
const DEFAULT_SLASH_FRACTION: f64 = 0.10; // slash stroke ≈ 1/10 · Ø
const DEFAULT_BAR_FRACTION: f64 = 0.125; // R01 white bar height ≈ 1/8 · Ø

/// What the layouts read from the pack.
struct Ctx<'a> {
    pack: &'a RulePack,
    roundels: Option<&'a RoundelTriangleRules>,
    measurer: &'a TextTileMeasurer<'a>,
}

/// Lays out a warning or regulatory sign the pack has no TSRGD artwork for. `diags` are the
/// constraint diagnostics already raised for the sign.
pub(crate) fn layout(
    pack: &RulePack,
    roundels: Option<&RoundelTriangleRules>,
    measurer: &TextTileMeasurer,
    sign: &SignDefinition,
    diags: Vec<Diagnostic>,
) -> Result<StdLayout, SignError> {
    let ctx = Ctx {
        pack,
        roundels,
        measurer,
    };
    match sign.kind {
        SignKind::WarningSign => ctx.warning(sign, diags),
        SignKind::Stop => ctx.stop(diags),
        SignKind::GiveWay => Ok(ctx.give_way(diags)),
        SignKind::RegulatoryMandatory => ctx.mandatory(sign, diags),
        SignKind::RegulatoryProhibitory => ctx.prohibitory(sign, diags),
        other => Err(SignError::Unsupported(format!(
            "RegulatorySignLayout cannot draw {}",
            other.name()
        ))),
    }
}

/// A temporary sign: the scheme says so, or a sign without a scheme has a yellow background.
fn is_temporary(sign: &SignDefinition) -> bool {
    sign.scheme == ColourScheme::Temporary
        || (sign.scheme == ColourScheme::Auto && sign.background == Colour::Yellow)
}

fn contains_ignore_case(text: &str, part: &str) -> bool {
    text.to_lowercase().contains(part)
}

fn norm(s: Option<&str>) -> String {
    s.unwrap_or("").trim().to_lowercase()
}

/// Regular-polygon disc of `radius` sw (48 sides), a circle approximation (C#
/// `EmbeddedSignGeometry.Disc`).
pub(crate) fn disc(cx: f64, cy: f64, radius: f64, colour: Colour) -> Prim {
    const SEGMENTS: usize = 48;
    let outline = (0..SEGMENTS)
        .map(|i| {
            let t = 2.0 * std::f64::consts::PI * i as f64 / SEGMENTS as f64;
            Pt2::new(cx + radius * t.cos(), cy + radius * t.sin())
        })
        .collect();
    Prim::Fill { outline, colour }
}

/// Apex-up triangle occupying (0,0)..(side,height) with the base on y=0; point-down flips it.
fn triangle_vertices(side: f64, height: f64, point_down: bool) -> Vec<Pt2> {
    if point_down {
        vec![
            Pt2::new(0.0, height),
            Pt2::new(side, height),
            Pt2::new(side / 2.0, 0.0),
        ]
    } else {
        vec![
            Pt2::new(0.0, 0.0),
            Pt2::new(side, 0.0),
            Pt2::new(side / 2.0, height),
        ]
    }
}

/// The red frame and the ground inside it of an equilateral triangle of side [`DIA`], with its
/// centroid, inradius less the band, and height.
struct TriangleBody {
    prims: Vec<Prim>,
    centroid: Pt2,
    field_inradius: f64,
    height: f64,
}

fn triangle_body(point_down: bool, fraction: f64, ground: Colour) -> TriangleBody {
    let height = DIA * 3f64.sqrt() / 2.0; // equilateral height
    let r_out = DIA / (2.0 * 3f64.sqrt()); // inradius
    let centroid = if point_down {
        Pt2::new(DIA / 2.0, height * 2.0 / 3.0)
    } else {
        Pt2::new(DIA / 2.0, height / 3.0)
    };
    let outer = triangle_vertices(DIA, height, point_down);

    // Red band: the pack's labelled provisional fraction of the body side, clamped so a
    // legible ground always survives in the interior.
    let band = (fraction * DIA).min(r_out * 0.6);
    let k = (r_out - band) / r_out;
    let field = outer
        .iter()
        .map(|v| {
            Pt2::new(
                centroid.x + (v.x - centroid.x) * k,
                centroid.y + (v.y - centroid.y) * k,
            )
        })
        .collect();

    TriangleBody {
        prims: vec![
            Prim::Fill {
                outline: outer,
                colour: Colour::Red,
            }, // red frame first …
            Prim::Fill {
                outline: field,
                colour: ground,
            }, // … then the coloured ground inside it
        ],
        centroid,
        field_inradius: r_out - band,
        height,
    }
}

impl Ctx<'_> {
    fn provisional(&self, key: &str, fallback: f64) -> f64 {
        self.roundels
            .and_then(|r| r.fraction(key))
            .unwrap_or(fallback)
    }

    fn ring_width(&self) -> f64 {
        self.provisional("roundel-ring-width", DEFAULT_RING_FRACTION) * DIA
    }

    fn slash_stroke(&self) -> f64 {
        self.provisional("slash-stroke-width", DEFAULT_SLASH_FRACTION) * DIA
    }

    // ─── warning: equilateral triangle, red border, white or yellow ground ────

    /// Warning signs (Uganda TSM Vol.1 §Warning Signs / Vol.2 App.3 pages W01–W49, W81): an
    /// equilateral triangle, apex up, with a red border band round a white (permanent) or
    /// yellow (temporary) ground; the temporary variant changes the ground only (Vol.1
    /// p.10–11). The internal symbol is black: drawn artwork when the pictogram library has the
    /// glyph, otherwise a placeholder and a `SYMBOL_UNRESOLVED` warning, so the triangle still
    /// renders. `SignMirror::Right` draws the glyph flipped about the vertical centre line (the
    /// `.1` = left / `.2` = right variants, Vol.1 p.8).
    fn warning(
        &self,
        sign: &SignDefinition,
        mut diags: Vec<Diagnostic>,
    ) -> Result<StdLayout, SignError> {
        // Colours are family-fixed (stated): white permanent / yellow temporary ground, red
        // border, black symbol.
        let ground = if is_temporary(sign) {
            Colour::Yellow
        } else {
            Colour::White
        };
        // Honour the pack-declared body shape rather than hardcoding an orientation silently.
        let point_down = self
            .roundels
            .is_some_and(|r| contains_ignore_case(&r.warning_shape, "point-down"));
        let fraction = self.provisional("red-triangle-border-width", DEFAULT_RING_FRACTION);
        let body = triangle_body(point_down, fraction, ground);
        let mut prims = body.prims;

        // The black symbol, centred on the incentre, sized to the interior field.
        if let Some(id) = sign.symbol_id.as_deref().filter(|id| !id.is_empty()) {
            let size = body.field_inradius * 1.5;
            let box_min = Pt2::new(body.centroid.x - size / 2.0, body.centroid.y - size / 2.0);
            let mut sym =
                symbol::resolve_inked(id, size, box_min, 0.0, self.measurer, Colour::Black)?;
            if sign.mirror == SignMirror::Right {
                flip_interior(&mut sym, 0, body.centroid.x);
            }
            for prim in &sym {
                if let Prim::Symbol { id, .. } = prim {
                    diags.push(Diagnostic::warning(
                        "SYMBOL_UNRESOLVED",
                        format!(
                            "Symbol '{id}' has no artwork; triangle body drawn, placeholder emitted"
                        ),
                        WARNING_CLAUSE,
                    ));
                }
            }
            prims.extend(sym);
        }

        Ok(StdLayout::plain(prims, diags, DIA, body.height))
    }

    // ─── STOP: regular octagon, red ground / white border / white word (R61) ───

    fn stop(&self, diags: Vec<Diagnostic>) -> Result<StdLayout, SignError> {
        // The octagon orientation is a documented gap (Vol.1 sizes it on the circle ladder
        // without saying across-flats or across-corners). The pack carries stopOrientation as
        // a labelled provisional value; honour it, defaulting to across-corners.
        let across_corners = self
            .roundels
            .is_none_or(|r| contains_ignore_case(&r.stop_orientation, "corner"));
        let ring = self.ring_width();

        let mut prims = vec![
            octagon(R, across_corners, Colour::White), // white border band …
            octagon(R - ring, across_corners, Colour::Red), // … around the red ground
        ];
        // White "STOP" word, centred.
        self.add_text(
            &mut prims,
            "STOP",
            FontFace::TransportMedium,
            R * 0.5,
            CY,
            Colour::White,
        )?;
        Ok(square(prims, diags))
    }

    // ─── GIVE WAY: equilateral point-down triangle, white ground / red border ──

    fn give_way(&self, diags: Vec<Diagnostic>) -> StdLayout {
        let point_down = self
            .roundels
            .is_none_or(|r| contains_ignore_case(&r.give_way_shape, "point-down"));
        let fraction = self.provisional("red-triangle-border-width", DEFAULT_RING_FRACTION);
        // The body (red frame + white ground) is the whole plate: Vol.2 p.106 gives Give Way no
        // internal symbol, so an empty white / red triangle is the complete artwork.
        let body = triangle_body(point_down, fraction, Colour::White);
        StdLayout::plain(body.prims, diags, DIA, body.height)
    }

    // ─── MANDATORY: blue disc / white border / white symbol (R71–R83) ──────────

    fn mandatory(
        &self,
        sign: &SignDefinition,
        diags: Vec<Diagnostic>,
    ) -> Result<StdLayout, SignError> {
        let ring = self.ring_width();
        let field_r = R - ring;

        let mut prims = vec![
            disc(CX, CY, R, Colour::White),      // white border …
            disc(CX, CY, field_r, Colour::Blue), // … around the blue field
        ];

        let id = norm(sign.symbol_id.as_deref());
        let ink = Colour::White; // mandatory symbols are white on blue (Vol.1 p.24)

        match id.as_str() {
            "ahead" | "straight-ahead" => self.add_centered_arrow(&mut prims, 0.0, ink, 1.0, 0.0),
            "turn-left" | "turn-right" => self.add_turn_arrow(&mut prims, ink, true),
            // diagonal down-left (base)
            "pass-left" | "pass-right" => self.add_centered_arrow(&mut prims, 135.0, ink, 1.0, 0.0),
            "pass-either-side" => {
                let off = 0.28 * DIA;
                self.add_centered_arrow(&mut prims, 135.0, ink, 0.72, -off); // down-left
                self.add_centered_arrow(&mut prims, -135.0, ink, 0.72, off); // down-right
            }
            // An unknown mandatory glyph: the blue disc still draws, with a placeholder and a
            // warning.
            "" => self.add_glyph(&mut prims, "mandatory", field_r, ink)?,
            other => self.add_glyph(&mut prims, other, field_r, ink)?,
        }

        if sign.mirror == SignMirror::Right {
            flip_interior(&mut prims, 2, CX); // keep the disc body, mirror the white artwork
        }
        Ok(square(prims, diags))
    }

    // ─── PROHIBITORY: circle, white / red / black + red slash, and the outliers ─

    /// Prohibitory signs (R01–R52, R65): a circle, usually a white ground inside a red ring
    /// with a black symbol and (for most) a red diagonal bar. The per-plate body colours and
    /// slash forms are stated (inventory §3, §6.5); only the stroke widths are the pack's
    /// provisional fractions. The symbol id names the plate's profile.
    fn prohibitory(
        &self,
        sign: &SignDefinition,
        diags: Vec<Diagnostic>,
    ) -> Result<StdLayout, SignError> {
        let ring = self.ring_width();
        let field_r = R - ring;
        let id = norm(sign.symbol_id.as_deref());
        let ink = Colour::Black; // prohibitory interiors are black (Vol.1 p.21)

        // `has_ink` marks the class / movement plates that carry a black interior symbol
        // beneath a red slash.
        let (mut red_ring, mut black_border, mut blue_ground, mut red_ground, mut has_ink) =
            (true, false, false, false, true);
        let mut slash = false;
        let mut slash_dir = "tl-br";
        let mut slash_count = 1;

        let mut prims: Vec<Prim> = Vec::new();

        match id.as_str() {
            // R01: red ground / white border / solid white horizontal bar.
            "no-entry" => {
                red_ring = false;
                red_ground = true;
                has_ink = false;
                prims.push(Prim::Line {
                    from: Pt2::new(CX - field_r, CY),
                    to: Pt2::new(CX + field_r, CY),
                    width: self.provisional("r01-bar-height", DEFAULT_BAR_FRACTION) * DIA,
                    colour: Some(Colour::White),
                });
            }
            // R47: blue ground / red ring / one red diagonal, top-left to bottom-right.
            "no-parking" => {
                blue_ground = true;
                has_ink = false;
                slash = true;
            }
            // R48: blue ground / red ring / two red diagonals.
            "no-stopping" => {
                blue_ground = true;
                has_ink = false;
                slash = true;
                slash_dir = "cross";
            }
            // R51: white ground / black border / four black diagonals.
            "end-of-restriction" => {
                red_ring = false;
                black_border = true;
                has_ink = false;
                slash = true;
                slash_dir = "quad-tr-bl";
                slash_count = 4;
            }
            // R52: and a grey speed figure beneath the four diagonals.
            "end-of-speed-limit" => {
                red_ring = false;
                black_border = true;
                has_ink = false;
                slash = true;
                slash_dir = "quad-tr-bl";
                slash_count = 4;
                self.add_text(
                    &mut prims,
                    &numeral_value(sign),
                    FontFace::TransportHeavy,
                    R * 0.42,
                    CY,
                    Colour::Grey,
                )?;
            }
            // R65: red up-arrow (left) + larger black down-arrow (right).
            "give-way-oncoming" => {
                has_ink = false;
                self.add_centered_arrow(&mut prims, 0.0, Colour::Red, 0.8, -field_r * 0.32);
                self.add_centered_arrow(&mut prims, 180.0, ink, 1.15, field_r * 0.32);
            }
            // R49: black numeral (km/h).
            "speed-limit" => {
                has_ink = false;
                self.add_text(
                    &mut prims,
                    &numeral_value(sign),
                    FontFace::TransportHeavy,
                    R * 0.42,
                    CY,
                    ink,
                )?;
            }
            // R46: black "STOP" underlined by a black bar.
            "stop-word" => {
                has_ink = false;
                self.add_text(
                    &mut prims,
                    "STOP",
                    FontFace::TransportHeavy,
                    R * 0.34,
                    CY + R * 0.06,
                    ink,
                )?;
                let uw = self.measurer.width("STOP", FontFace::TransportMedium)?
                    * ((R * 0.34 * 1.4) / 4.0)
                    * 0.92;
                prims.push(Prim::Line {
                    from: Pt2::new(CX - uw / 2.0, CY - R * 0.30),
                    to: Pt2::new(CX + uw / 2.0, CY - R * 0.30),
                    width: ring * 0.8,
                    colour: Some(ink),
                });
            }
            // R02–R45 class / movement plates: white / red / black + red slash (§6.5).
            _ => {
                slash = !id.is_empty();
                if id == "no-overtaking" {
                    slash_dir = "tr-bl"; // R43's slash runs the opposite way
                }
            }
        }

        if has_ink {
            self.add_prohibitory_ink(&mut prims, &id, field_r, ink)?;
        }

        // Permanent / temporary ground (inventory §5, Vol.1 p.22 / Vol.2 p.102): a temporary
        // class / figure / end-of plate changes the white ground to yellow only; the red ring
        // or black border and the black ink stay. The standard gives R01, R47/R48, R65 and the
        // mandatory series no temporary version, so those bodies are never recoloured.
        let no_temp = red_ground || blue_ground || id == "give-way-oncoming";
        let body_ring = if black_border {
            Colour::Black
        } else if red_ring {
            Colour::Red
        } else {
            Colour::White
        };
        let body_ground = if red_ground {
            Colour::Red
        } else if blue_ground {
            Colour::Blue
        } else if is_temporary(sign) && !no_temp {
            Colour::Yellow
        } else {
            Colour::White
        };
        // The body goes behind any interior already added.
        prims.insert(0, disc(CX, CY, field_r, body_ground));
        prims.insert(0, disc(CX, CY, R, body_ring));

        if slash {
            self.slash_bars(&mut prims, slash_dir, slash_count, field_r);
        }

        if sign.mirror == SignMirror::Right {
            flip_interior(&mut prims, 2, CX); // keep the disc body, flip artwork + slash together
        }
        Ok(square(prims, diags))
    }

    /// Black interior artwork for a generic prohibitory plate: a turn or U-turn arrow, a reused
    /// pictogram, or a placeholder for a figurative vehicle / horn silhouette.
    fn add_prohibitory_ink(
        &self,
        prims: &mut Vec<Prim>,
        id: &str,
        field_r: f64,
        ink: Colour,
    ) -> Result<(), SignError> {
        match id {
            "no-left-turn" | "no-right-turn" => self.add_turn_arrow(prims, ink, true),
            "no-u-turn" => self.add_u_turn_arrow(prims, ink, field_r),
            "no-buses" => self.add_glyph(prims, "bus", field_r, ink)?,
            "no-cycles" => self.add_glyph(prims, "bicycle", field_r, ink)?,
            "no-pedestrians" => self.add_glyph(prims, "pedestrian", field_r, ink)?,
            // Figurative / art-ambiguous class symbols and the horn: a placeholder, so the disc
            // and red slash still read and the status stays truthful.
            other => self.add_glyph(prims, placeholder_for(other), field_r, ink)?,
        }
        Ok(())
    }

    // ─── slash / bar forms (§6.5) ────────────────────────────────────────────

    fn slash_bars(&self, prims: &mut Vec<Prim>, direction: &str, count: usize, reach: f64) {
        let w = self.slash_stroke();
        const D: f64 = std::f64::consts::FRAC_1_SQRT_2; // cos 45° = sin 45°
        let mut bar = |x0: f64, y0: f64, x1: f64, y1: f64, width: f64, colour: Colour| {
            prims.push(Prim::Line {
                from: Pt2::new(x0, y0),
                to: Pt2::new(x1, y1),
                width,
                colour: Some(colour),
            });
        };
        let (tl_br, tr_bl) = (
            (
                CX - D * reach,
                CY + D * reach,
                CX + D * reach,
                CY - D * reach,
            ),
            (
                CX + D * reach,
                CY + D * reach,
                CX - D * reach,
                CY - D * reach,
            ),
        );
        match direction {
            "horizontal" => bar(CX - reach, CY, CX + reach, CY, w, Colour::Red),
            "tr-bl" => bar(tr_bl.0, tr_bl.1, tr_bl.2, tr_bl.3, w, Colour::Red),
            // R48: two red diagonals.
            "cross" => {
                bar(tl_br.0, tl_br.1, tl_br.2, tl_br.3, w, Colour::Red);
                bar(tr_bl.0, tr_bl.1, tr_bl.2, tr_bl.3, w, Colour::Red);
            }
            // R51 / R52: four narrow black diagonals.
            "quad-tr-bl" => {
                let bw = w * 0.5; // narrower than a prohibition slash (stated form; width is a gap)
                let step = reach * 0.6;
                let span = D * reach * 1.3;
                for i in 0..count {
                    let off = -1.5 * step + i as f64 * step; // spread along the ⊥ axis
                                                             // TR→BL line direction (−d,−d); ⊥ direction (d,−d) offsets the parallel bars.
                    let (ox, oy) = (D * off, -D * off);
                    bar(
                        CX + ox + D * span,
                        CY + oy + D * span,
                        CX + ox - D * span,
                        CY + oy - D * span,
                        bw,
                        Colour::Black,
                    );
                }
            }
            // "tl-br": a single red diagonal, top-left to bottom-right (stated, §6.5).
            _ => bar(tl_br.0, tl_br.1, tl_br.2, tl_br.3, w, Colour::Red),
        }
    }

    // ─── arrow helpers ───────────────────────────────────────────────────────

    /// The pack's standard arrow, or the Figure 5 values when it has no `arrows` section.
    fn fig5(&self) -> ArrowRules {
        self.pack.arrows.clone().unwrap_or(ArrowRules {
            shaft_length: 12.0,
            shaft_width: 3.0,
            head_length: 12.0,
            head_flare: 3.0,
            flare_angle_deg: 70.0,
            point_half_angle_deg: 45.0,
            inner_corner_radius: 2.0,
            tip_radius: 0.2,
            style: None,
            short_shaft_length: None,
        })
    }

    /// The standard arrow at `scale`, centred on the plate centre shifted by `x_offset` and
    /// rotated counter-clockwise from ahead.
    fn add_centered_arrow(
        &self,
        prims: &mut Vec<Prim>,
        rotation_deg: f64,
        fill: Colour,
        scale: f64,
        x_offset: f64,
    ) {
        let rules = scale_rules(self.fig5(), scale);
        let len = arrow::total_length(&rules, None);
        let rads = rotation_deg * std::f64::consts::PI / 180.0;
        // The canonical arrow points +Y; rotating it by θ maps its axis to (−sin θ, cos θ).
        let (dir_x, dir_y) = (-rads.sin(), rads.cos());
        let (mid_x, mid_y) = (CX + x_offset, CY);
        let tail = Pt2::new(mid_x - dir_x * len / 2.0, mid_y - dir_y * len / 2.0);
        prims.push(Prim::Fill {
            outline: arrow::place(&rules, tail, rotation_deg, None),
            colour: fill,
        });
    }

    /// A bent 90° turn arrow built from two thick shafts and a solid head, authored pointing
    /// left; the engine mirrors it for a `.2` (right) plate. An indicative form.
    fn add_turn_arrow(&self, prims: &mut Vec<Prim>, fill: Colour, left: bool) {
        let stroke = self.slash_stroke();
        let s = if left { -1.0 } else { 1.0 };
        let x0 = CX - s * DIA * 0.04;
        let y_bottom = CY - DIA * 0.24;
        let y_top = CY + DIA * 0.10;
        let x_tip = CX + s * DIA * 0.26;

        // vertical leg
        prims.push(Prim::Line {
            from: Pt2::new(x0, y_bottom),
            to: Pt2::new(x0, y_top),
            width: stroke,
            colour: Some(fill),
        });
        // horizontal leg
        prims.push(Prim::Line {
            from: Pt2::new(x0, y_top),
            to: Pt2::new(x_tip, y_top),
            width: stroke,
            colour: Some(fill),
        });
        prims.push(Prim::Fill {
            outline: route_head::place_head(
                Pt2::new(x_tip + s * stroke, y_top),
                if left { 90.0 } else { -90.0 },
                stroke,
            ),
            colour: fill,
        });
    }

    /// A squared U-turn arrow: up one leg, across the top, down the other with a head. Built
    /// from thick shafts and a solid head; an indicative form (Vol.2 p.95 describes the U-turn
    /// but the exact curve is not measurable).
    fn add_u_turn_arrow(&self, prims: &mut Vec<Prim>, fill: Colour, field_r: f64) {
        let stroke = self.slash_stroke();
        let dx = field_r * 0.42;
        let y_top = CY + field_r * 0.42;
        let y_bottom = CY - field_r * 0.5;
        let mut leg = |x0: f64, y0: f64, x1: f64, y1: f64| {
            prims.push(Prim::Line {
                from: Pt2::new(x0, y0),
                to: Pt2::new(x1, y1),
                width: stroke,
                colour: Some(fill),
            });
        };
        leg(CX - dx, y_bottom, CX - dx, y_top); // up leg
        leg(CX - dx, y_top, CX + dx, y_top); // top leg
        leg(CX + dx, y_top, CX + dx, y_bottom + stroke); // down leg
        prims.push(Prim::Fill {
            outline: route_head::place_head(Pt2::new(CX + dx, y_bottom), 180.0, stroke),
            colour: fill,
        }); // head down
    }

    // ─── reused pictogram / placeholder ──────────────────────────────────────

    fn add_glyph(
        &self,
        prims: &mut Vec<Prim>,
        symbol_id: &str,
        field_r: f64,
        ink: Colour,
    ) -> Result<(), SignError> {
        let size = field_r * 1.3; // glyph box scaled to the inner field
        let box_min = Pt2::new(CX - size / 2.0, CY - size / 2.0);
        prims.extend(symbol::resolve_inked(
            symbol_id,
            size,
            box_min,
            0.0,
            self.measurer,
            ink,
        )?);
        Ok(())
    }

    // ─── text / numeral ──────────────────────────────────────────────────────

    /// Lettering centred on the plate's vertical centre line with its tile centred on `cy`. Its
    /// width is measured in Transport Medium whatever face it is drawn in, as the C# engine
    /// measures it.
    fn add_text(
        &self,
        prims: &mut Vec<Prim>,
        text: &str,
        face: FontFace,
        xh: f64,
        cy: f64,
        ink: Colour,
    ) -> Result<(), SignError> {
        let w = self.measurer.width(text, FontFace::TransportMedium)? * (xh / 4.0);
        prims.push(Prim::Text(TextRun {
            origin: Pt2::new(CX - w / 2.0, cy - xh / 2.0),
            text: text.to_string(),
            face,
            x_height: xh,
            ink: Some(ink),
        }));
        Ok(())
    }
}

/// The figure a speed / end-of-speed plate shows. A template plate carries no chosen limit, so
/// the design speed on the sign stands in as the representative numeral (an editor overrides
/// it). This is a font-rendered number, never an invented standard value.
fn numeral_value(sign: &SignDefinition) -> String {
    (if sign.speed_kph > 0 {
        sign.speed_kph
    } else {
        50
    })
    .to_string()
}

/// The symbol id reported to the placeholder for an un-drawn figurative glyph. Mirrors the
/// vehicle categories in inventory §3.1 / §7.2.
fn placeholder_for(profile_id: &str) -> &str {
    match profile_id {
        "no-vehicles" | "no-motorcars" => "car",
        "no-trucks" | "no-overtaking-goods" => "truck",
        "no-motorcycles" => "motorcycle",
        "no-tractors" => "tractor",
        "no-oxcart" => "oxcart",
        "no-overtaking" => "car-pair",
        "no-horn" => "horn",
        "" => "symbol",
        other => other,
    }
}

fn scale_rules(r: ArrowRules, k: f64) -> ArrowRules {
    if k == 1.0 {
        return r;
    }
    ArrowRules {
        shaft_length: r.shaft_length * k,
        shaft_width: r.shaft_width * k,
        head_length: r.head_length * k,
        head_flare: r.head_flare * k,
        inner_corner_radius: r.inner_corner_radius * k,
        tip_radius: r.tip_radius * k,
        ..r
    }
}

/// Regular octagon inscribed in a circle of `radius`, centred on the plate square.
/// `across_corners` keeps a vertex at the top (the pack's labelled provisional orientation);
/// otherwise a flat sits at the top.
fn octagon(radius: f64, across_corners: bool, colour: Colour) -> Prim {
    let phase = if across_corners { 90.0 } else { 67.5 }; // vertex or flat at the vertical
    let outline = (0..8)
        .map(|i| {
            let t = (phase + 45.0 * i as f64) * std::f64::consts::PI / 180.0;
            Pt2::new(CX + radius * t.cos(), CY + radius * t.sin())
        })
        .collect();
    Prim::Fill { outline, colour }
}

/// A plate on the [`DIA`] square, with a warning for every placeholder it carries.
fn square(prims: Vec<Prim>, mut diags: Vec<Diagnostic>) -> StdLayout {
    for prim in &prims {
        if let Prim::Symbol { id, .. } = prim {
            diags.push(Diagnostic::warning(
                "SYMBOL_UNRESOLVED",
                format!("Symbol '{id}' has no artwork; regulatory disc drawn, placeholder emitted"),
                REGULATORY_CLAUSE,
            ));
        }
    }
    StdLayout::plain(prims, diags, DIA, DIA)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn close(a: f64, b: f64) -> bool {
        (a - b).abs() < 1e-9
    }

    fn outline(p: &Prim) -> (&[Pt2], Colour) {
        match p {
            Prim::Fill { outline, colour } => (outline, *colour),
            other => panic!("not a fill: {other:?}"),
        }
    }

    /// A 50 sw equilateral triangle: height 25√3, inradius 25/√3. The 1/8 band (6.25 sw) leaves
    /// a field scaled about the centroid by (r − 6.25) / r.
    #[test]
    fn the_triangle_band_is_a_fraction_of_the_side_inset_about_the_centroid() {
        let body = triangle_body(false, 0.125, Colour::White);
        let height = 25.0 * 3f64.sqrt();
        let r = 25.0 / 3f64.sqrt();
        assert!(close(body.height, height));
        assert!(close(body.centroid.x, 25.0) && close(body.centroid.y, height / 3.0));
        assert!(close(body.field_inradius, r - 6.25));
        let (outer, red) = outline(&body.prims[0]);
        assert_eq!(red, Colour::Red);
        assert!(close(outer[2].x, 25.0) && close(outer[2].y, height));
        let (field, ground) = outline(&body.prims[1]);
        assert_eq!(ground, Colour::White);
        // The field's base is one band above the outer base.
        assert!(close(field[0].y, 6.25) && close(field[1].y, 6.25));
        // A band wider than 0.6 of the inradius is clamped.
        assert!(close(
            triangle_body(false, 0.5, Colour::White).field_inradius,
            0.4 * r
        ));
        // Point down: the base is at the top and the apex on the foot.
        let down = triangle_body(true, 0.125, Colour::Yellow);
        let (outer, _) = outline(&down.prims[0]);
        assert!(close(outer[0].y, height) && close(outer[2].y, 0.0));
        assert!(close(down.centroid.y, height * 2.0 / 3.0));
    }

    #[test]
    fn the_octagon_has_a_vertex_or_a_flat_at_the_top() {
        let oct = octagon(25.0, true, Colour::White);
        let (corners, _) = outline(&oct);
        assert_eq!(corners.len(), 8);
        assert!(close(corners[0].x, 25.0) && close(corners[0].y, 50.0));
        let flat = octagon(25.0, false, Colour::White);
        let (flats, _) = outline(&flat);
        assert!(close(flats[0].y, flats[1].y) && close(flats[0].x + flats[1].x, 50.0));
    }

    #[test]
    fn a_disc_is_a_48_gon_on_its_circle() {
        let d = disc(25.0, 25.0, 10.0, Colour::Blue);
        let (o, c) = outline(&d);
        assert_eq!((o.len(), c), (48, Colour::Blue));
        assert!(close(o[0].x, 35.0) && close(o[0].y, 25.0));
        assert!(o
            .iter()
            .all(|p| close((p.x - 25.0).hypot(p.y - 25.0), 10.0)));
    }

    #[test]
    fn figurative_profiles_name_their_placeholder() {
        assert_eq!(placeholder_for("no-vehicles"), "car");
        assert_eq!(placeholder_for("no-overtaking-goods"), "truck");
        assert_eq!(placeholder_for("no-overtaking"), "car-pair");
        assert_eq!(placeholder_for(""), "symbol");
        assert_eq!(placeholder_for("no-camels"), "no-camels");
    }

    #[test]
    fn a_scaled_arrow_keeps_its_angles() {
        let r = ArrowRules {
            shaft_length: 12.0,
            shaft_width: 3.0,
            head_length: 12.0,
            head_flare: 3.0,
            flare_angle_deg: 70.0,
            point_half_angle_deg: 45.0,
            inner_corner_radius: 2.0,
            tip_radius: 0.2,
            style: None,
            short_shaft_length: None,
        };
        let half = scale_rules(r.clone(), 0.5);
        assert_eq!(
            (half.shaft_length, half.shaft_width, half.head_flare),
            (6.0, 1.5, 1.5)
        );
        assert_eq!(
            (half.flare_angle_deg, half.point_half_angle_deg),
            (70.0, 45.0)
        );
        assert!(close(
            arrow::total_length(&half, None),
            arrow::total_length(&r, None) / 2.0
        ));
        assert_eq!(scale_rules(r.clone(), 1.0), r);
    }
}
