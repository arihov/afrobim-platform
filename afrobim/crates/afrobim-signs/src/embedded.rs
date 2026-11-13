//! Embedded restriction signs: the miniature warning-triangle and regulatory-roundel frames a
//! guidance panel carries when a restriction (height, weight, no entry ...) is essential to the
//! route choice (App.1 §2.1.3; pack sections `embeddedSigns` and `embeddedShapes`). Port of C#
//! `EmbeddedSignGeometry`, of the two pack records it reads, and of the placement loop the C#
//! layouts share (`MapSymbolLayout.EmbedOverArm`, and the same loop written out in
//! `DirectionSignLayout`).
//!
//! Both frames are drawn at the pack's fixed height (20 sw), and on a green ground a white
//! edge (0.5 sw) is added outside the frame so it reads against the background. Only the
//! standardised frame is drawn, as closed filled polygons: the inner restriction glyph (the
//! number or pictogram) is not drawn by the C# engine, so [`EmbeddedSign::symbol_id`] names the
//! restriction without changing the geometry.
//!
//! Where the C# engine places them with the UK pack:
//! - rectangular direction signs: in a column between the arrow and the legend, which widens
//!   the sign by the widest frame plus a word space (`direction.rs`);
//! - map-type signs: centred over the vertical arm (crossroads: above the junction; tee: on the
//!   approach) or inside the roundabout ring, without changing the sign's size
//!   (`map_symbol.rs`);
//! - panelled stack-type signs: not laid out; `EMBEDDED_NOT_PLACED` is raised (`stack.rs`);
//! - every other kind: ignored without a diagnostic, as in C#.

use afrobim_draw::Pt2;
use serde::Deserialize;

use crate::model::{Colour, EmbeddedSign, EmbeddedType, SignDefinition, SignKind};
use crate::pack::RulePack;
use crate::prim::Prim;
use crate::{SignEngine, SignError};

/// Chords round a roundel: the default of C# `EmbeddedSignGeometry.Disc` (a drawing setting).
pub(crate) const DISC_SEGMENTS: u32 = 48;

/// App.1 §2.1.3: the height of an embedded sign and the white edge it gets on a green ground,
/// in sw.
#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct EmbeddedSignRules {
    pub height_sw: f64,
    pub white_edge_on_green: f64,
}

/// The frame proportions of embedded signs, on top of the shared height in
/// [`EmbeddedSignRules`]. All in sw.
#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct EmbeddedShapeRules {
    pub triangle_border_width: f64,
    pub roundel_ring_width: f64,
    pub roundel_diameter: f64,
}

/// The `embeddedSigns` and `embeddedShapes` sections of a rule pack. [`RulePack::from_json`]
/// fills this view in from the pack text. A pack without `embeddedShapes` draws no embedded
/// signs, as in C#.
#[derive(Debug, Clone, PartialEq, Default, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct EmbeddedRules {
    #[serde(default)]
    pub embedded_signs: Option<EmbeddedSignRules>,
    #[serde(default)]
    pub embedded_shapes: Option<EmbeddedShapeRules>,
}

impl EmbeddedRules {
    /// Reads the two sections from pack JSON whose comments are already stripped; `path` only
    /// names the pack in error messages.
    pub(crate) fn from_stripped_json(stripped: &str, path: &str) -> Result<Self, SignError> {
        let rules: EmbeddedRules = serde_json::from_str(stripped)
            .map_err(|e| SignError::Pack(format!("Rule pack is not valid JSON: {path}: {e}")))?;
        rules.validate(path)?;
        Ok(rules)
    }

    /// The check of C# `RulePackLoader.ValidateOptionalSections` for `embeddedShapes`.
    fn validate(&self, path: &str) -> Result<(), SignError> {
        if let Some(es) = &self.embedded_shapes {
            if es.triangle_border_width <= 0.0
                || es.roundel_ring_width <= 0.0
                || es.roundel_diameter <= 0.0
            {
                return Err(SignError::Pack(format!(
                    "Rule pack 'embeddedShapes' values must be positive: {path}"
                )));
            }
        }
        Ok(())
    }

    /// Both sections, when the pack has `embeddedShapes`. `embeddedSigns` is a section the C#
    /// loader requires of every pack; a pack that has the shapes but not the height is a pack
    /// error here, never a guess.
    fn drawing_rules(
        &self,
    ) -> Result<Option<(&EmbeddedSignRules, &EmbeddedShapeRules)>, SignError> {
        let Some(shapes) = &self.embedded_shapes else {
            return Ok(None);
        };
        match &self.embedded_signs {
            Some(signs) => Ok(Some((signs, shapes))),
            None => Err(SignError::Pack(
                "Rule pack is missing section 'embeddedSigns'".to_string(),
            )),
        }
    }
}

/// Regular-polygon disc of `radius` sw centred at (`cx`, `cy`): a circle approximation the
/// renderer and the DXF round-trip already understand.
pub(crate) fn disc(cx: f64, cy: f64, radius: f64, fill: Colour, segments: u32) -> Prim {
    let outline = (0..segments)
        .map(|i| {
            let t = 2.0 * std::f64::consts::PI * f64::from(i) / f64::from(segments);
            Pt2::new(cx + radius * t.cos(), cy + radius * t.sin())
        })
        .collect();
    Prim::Fill {
        outline,
        colour: fill,
    }
}

/// Equilateral warning triangle, apex up, bounding height `height_sw` (20 sw), centred on
/// (`cx`, `cy`). A `border_sw`-thick `frame` band surrounds a `field` interior; with
/// `white_edge` an `edge_sw` band in `edge_colour` is added outside the frame (App.1 §2.1.3
/// green case). Primitives are emitted back to front so the DXF fill order is correct.
#[allow(clippy::too_many_arguments)]
pub(crate) fn warning_triangle(
    cx: f64,
    cy: f64,
    height_sw: f64,
    frame: Colour,
    field: Colour,
    border_sw: f64,
    white_edge: bool,
    edge_sw: f64,
    edge_colour: Colour,
) -> Vec<Prim> {
    let half_base = height_sw / 3.0_f64.sqrt(); // equilateral: base half-width
    let apex = (cx, cy + height_sw / 2.0);
    let base_l = (cx - half_base, cy - height_sw / 2.0);
    let base_r = (cx + half_base, cy - height_sw / 2.0);
    let verts = [apex, base_l, base_r];

    // Centroid + inradius let us inset (field) or outset (white edge) by a homothety.
    let centroid = (
        (apex.0 + base_l.0 + base_r.0) / 3.0,
        (apex.1 + base_l.1 + base_r.1) / 3.0,
    );
    let inradius = height_sw / 3.0;

    let tri = |factor: Option<f64>, fill: Colour| Prim::Fill {
        outline: verts
            .iter()
            .map(|v| match factor {
                Some(f) => Pt2::new(
                    centroid.0 + (v.0 - centroid.0) * f,
                    centroid.1 + (v.1 - centroid.1) * f,
                ),
                None => Pt2::new(v.0, v.1),
            })
            .collect(),
        colour: fill,
    };

    let mut prims = Vec::new();
    if white_edge {
        prims.push(tri(Some((inradius + edge_sw) / inradius), edge_colour));
    }
    prims.push(tri(None, frame));
    prims.push(tri(Some((inradius - border_sw) / inradius), field));
    prims
}

/// Regulatory roundel: a `ring_sw`-thick `frame` ring around a `field` disc, overall diameter
/// `diameter_sw` (20 sw). On green grounds `white_edge` adds an `edge_sw` band outside the
/// ring (App.1 §2.1.3).
#[allow(clippy::too_many_arguments)]
pub(crate) fn regulatory_roundel(
    cx: f64,
    cy: f64,
    diameter_sw: f64,
    frame: Colour,
    field: Colour,
    ring_sw: f64,
    white_edge: bool,
    edge_sw: f64,
    edge_colour: Colour,
) -> Vec<Prim> {
    let r = diameter_sw / 2.0;
    let mut prims = Vec::new();
    if white_edge {
        prims.push(disc(cx, cy, r + edge_sw, edge_colour, DISC_SEGMENTS));
    }
    prims.push(disc(cx, cy, r, frame, DISC_SEGMENTS));
    prims.push(disc(cx, cy, r - ring_sw, field, DISC_SEGMENTS));
    prims
}

/// Horizontal extent (sw) of an embedded frame: an equilateral warning triangle is wider than
/// it is tall (base = 2·height/√3), while a roundel's width equals its diameter. Hosts size
/// their embedded column from the widest frame.
pub(crate) fn frame_width(kind: EmbeddedType, height_sw: f64, shapes: &EmbeddedShapeRules) -> f64 {
    match kind {
        EmbeddedType::WarningTriangle => height_sw * 2.0 / 3.0_f64.sqrt(),
        EmbeddedType::RegulatoryRoundel => shapes.roundel_diameter,
    }
}

/// Draws one embedded sign centred at (`cx`, `cy`), resolving the standard's colours: a
/// red-framed white warning triangle or a red-ringed white roundel, each given the white outer
/// edge on a green ground (App.1 §2.1.3). The inner restriction glyph is not drawn.
pub(crate) fn draw(
    embedded: &EmbeddedSign,
    cx: f64,
    cy: f64,
    ground: Colour,
    signs: &EmbeddedSignRules,
    shapes: &EmbeddedShapeRules,
) -> Vec<Prim> {
    let white_edge = ground == Colour::Green;
    let edge = signs.white_edge_on_green;
    match embedded.kind {
        EmbeddedType::WarningTriangle => warning_triangle(
            cx,
            cy,
            signs.height_sw,
            Colour::Red,
            Colour::White,
            shapes.triangle_border_width,
            white_edge,
            edge,
            Colour::White,
        ),
        EmbeddedType::RegulatoryRoundel => regulatory_roundel(
            cx,
            cy,
            shapes.roundel_diameter,
            Colour::Red,
            Colour::White,
            shapes.roundel_ring_width,
            white_edge,
            edge,
            Colour::White,
        ),
    }
}

/// The sign's embedded signs, when it has any.
fn embeds_of(sign: &SignDefinition) -> Option<&[EmbeddedSign]> {
    sign.embedded.as_deref().filter(|e| !e.is_empty())
}

/// The width of the column a direction sign gives its embedded signs: the widest frame. `None`
/// when the sign carries none or the pack has no `embeddedShapes` section (C# `hasEmbedded`).
pub(crate) fn column_width(
    sign: &SignDefinition,
    pack: &RulePack,
) -> Result<Option<f64>, SignError> {
    let Some(embeds) = embeds_of(sign) else {
        return Ok(None);
    };
    let Some((signs, shapes)) = pack.embedded.drawing_rules()? else {
        return Ok(None);
    };
    Ok(Some(
        embeds
            .iter()
            .map(|e| frame_width(e.kind, signs.height_sw, shapes))
            .fold(f64::NEG_INFINITY, f64::max),
    ))
}

/// Draws the sign's embedded signs centred on `cx`, sharing the span from `span_top` down to
/// `span_bot` evenly: sign `i` of `n` sits at the middle of slot `i` (C# `EmbedOverArm` and the
/// loop of `DirectionSignLayout`). Nothing is drawn when the sign carries none or the pack has
/// no `embeddedShapes` section.
pub(crate) fn draw_stacked(
    prims: &mut Vec<Prim>,
    sign: &SignDefinition,
    pack: &RulePack,
    cx: f64,
    span_top: f64,
    span_bot: f64,
    ground: Colour,
) -> Result<(), SignError> {
    let Some(embeds) = embeds_of(sign) else {
        return Ok(());
    };
    let Some((signs, shapes)) = pack.embedded.drawing_rules()? else {
        return Ok(());
    };
    let slot = (span_top - span_bot) / embeds.len() as f64;
    for (i, e) in embeds.iter().enumerate() {
        let cy = span_top - (i as f64 + 0.5) * slot;
        prims.extend(draw(e, cx, cy, ground, signs, shapes));
    }
    Ok(())
}

/// What the engine does with [`SignDefinition::embedded`] on a sign of a given kind.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum EmbeddedSupport {
    /// The frames are drawn.
    Drawn,
    /// The kind does not lay them out: they are omitted and `EMBEDDED_NOT_PLACED` is raised.
    NotPlaced,
    /// The kind has no embedded signs: they are ignored without a diagnostic.
    Ignored,
}

impl SignEngine {
    /// What [`SignEngine::layout`] does with embedded signs on a sign of `kind` with the loaded
    /// pack, so an editor can offer them only where they are drawn. It mirrors the layouts: a
    /// direction or map-type sign draws them when the pack has an `embeddedShapes` section, a
    /// stack-type sign flags them as not placed, and every other kind ignores them.
    pub fn embedded_support(&self, kind: SignKind) -> EmbeddedSupport {
        match kind {
            SignKind::StackType => EmbeddedSupport::NotPlaced,
            SignKind::DirectionSign
            | SignKind::MapTypeCrossroads
            | SignKind::MapTypeTee
            | SignKind::MapTypeRoundabout
                if self.pack().embedded.embedded_shapes.is_some() =>
            {
                EmbeddedSupport::Drawn
            }
            _ => EmbeddedSupport::Ignored,
        }
    }

    /// The embedded sign types the engine can draw, in the order of [`EmbeddedType`].
    pub fn embedded_types() -> [EmbeddedType; 2] {
        [
            EmbeddedType::WarningTriangle,
            EmbeddedType::RegulatoryRoundel,
        ]
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::pack::tests::uk_pack;

    fn outline(p: &Prim) -> (&[Pt2], Colour) {
        match p {
            Prim::Fill { outline, colour } => (outline, *colour),
            other => panic!("not a fill: {other:?}"),
        }
    }

    fn bbox(pts: &[Pt2]) -> (f64, f64, f64, f64) {
        pts.iter().fold(
            (
                f64::INFINITY,
                f64::INFINITY,
                f64::NEG_INFINITY,
                f64::NEG_INFINITY,
            ),
            |(x0, y0, x1, y1), p| (x0.min(p.x), y0.min(p.y), x1.max(p.x), y1.max(p.y)),
        )
    }

    fn sign_with(kinds: &[EmbeddedType]) -> SignDefinition {
        let mut s = SignDefinition::stack("E", 60, Vec::new());
        s.embedded = Some(
            kinds
                .iter()
                .map(|k| EmbeddedSign {
                    symbol_id: "x".into(),
                    kind: *k,
                })
                .collect(),
        );
        s
    }

    #[test]
    fn the_uk_pack_has_both_sections() {
        let p = uk_pack();
        let signs = p.embedded.embedded_signs.as_ref().unwrap();
        assert_eq!(signs.height_sw, 20.0);
        assert_eq!(signs.white_edge_on_green, 0.5);
        let shapes = p.embedded.embedded_shapes.as_ref().unwrap();
        assert_eq!(shapes.triangle_border_width, 2.5);
        assert_eq!(shapes.roundel_ring_width, 2.5);
        assert_eq!(shapes.roundel_diameter, 20.0);
    }

    #[test]
    fn shape_values_must_be_positive() {
        let ok = r#"{"embeddedShapes":{"triangleBorderWidth":2.5,"roundelRingWidth":2.5,"roundelDiameter":20.0}}"#;
        assert!(EmbeddedRules::from_stripped_json(ok, "x.json").is_ok());
        for bad in [
            r#"{"embeddedShapes":{"triangleBorderWidth":0.0,"roundelRingWidth":2.5,"roundelDiameter":20.0}}"#,
            r#"{"embeddedShapes":{"triangleBorderWidth":2.5,"roundelRingWidth":-1.0,"roundelDiameter":20.0}}"#,
            r#"{"embeddedShapes":{"triangleBorderWidth":2.5,"roundelRingWidth":2.5,"roundelDiameter":0.0}}"#,
        ] {
            let err = EmbeddedRules::from_stripped_json(bad, "x.json").unwrap_err();
            assert!(matches!(err, SignError::Pack(_)), "{err}");
            assert!(err.to_string().contains("'embeddedShapes'"), "{err}");
        }
    }

    #[test]
    fn a_pack_without_the_sections_loads_and_draws_nothing() {
        let rules = EmbeddedRules::from_stripped_json("{}", "x.json").unwrap();
        assert_eq!(rules, EmbeddedRules::default());
        let mut pack = uk_pack();
        pack.embedded = rules;
        let sign = sign_with(&[EmbeddedType::RegulatoryRoundel]);
        assert_eq!(column_width(&sign, &pack).unwrap(), None);
        let mut prims = Vec::new();
        draw_stacked(&mut prims, &sign, &pack, 0.0, 10.0, 0.0, Colour::Green).unwrap();
        assert!(prims.is_empty());
    }

    #[test]
    fn shapes_without_the_height_section_are_a_pack_error_on_use() {
        let mut pack = uk_pack();
        pack.embedded.embedded_signs = None;
        let sign = sign_with(&[EmbeddedType::WarningTriangle]);
        let err = column_width(&sign, &pack).unwrap_err();
        assert!(matches!(err, SignError::Pack(_)), "{err}");
        assert!(err.to_string().contains("'embeddedSigns'"), "{err}");
        // A sign with nothing embedded never asks for the section.
        assert_eq!(column_width(&sign_with(&[]), &pack).unwrap(), None);
    }

    #[test]
    fn a_disc_is_a_regular_polygon_starting_on_the_x_axis() {
        let d = disc(3.0, -2.0, 5.0, Colour::Red, 48);
        let (pts, colour) = outline(&d);
        assert_eq!(colour, Colour::Red);
        assert_eq!(pts.len(), 48);
        assert!((pts[0].x - 8.0).abs() < 1e-12 && (pts[0].y + 2.0).abs() < 1e-12);
        for p in pts {
            assert!(((p.x - 3.0).hypot(p.y + 2.0) - 5.0).abs() < 1e-12);
        }
        // Counter-clockwise: a quarter of the way round is straight up.
        assert!((pts[12].x - 3.0).abs() < 1e-12 && (pts[12].y - 3.0).abs() < 1e-12);
    }

    #[test]
    fn a_triangle_is_20_high_apex_up_with_a_2_5_border() {
        let prims = warning_triangle(
            10.0,
            10.0,
            20.0,
            Colour::Red,
            Colour::White,
            2.5,
            false,
            0.5,
            Colour::White,
        );
        assert_eq!(prims.len(), 2);
        let (frame, c) = outline(&prims[0]);
        assert_eq!(c, Colour::Red);
        assert_eq!(frame.len(), 3);
        let (x0, y0, x1, y1) = bbox(frame);
        assert!((y1 - y0 - 20.0).abs() < 1e-12);
        assert!((x1 - x0 - 40.0 / 3.0_f64.sqrt()).abs() < 1e-12);
        assert!((frame[0].x - 10.0).abs() < 1e-12 && (frame[0].y - 20.0).abs() < 1e-12);

        // The field is the frame inset 2.5 sw on every side: its base rises by the border.
        let (field, c) = outline(&prims[1]);
        assert_eq!(c, Colour::White);
        let (_, fy0, _, fy1) = bbox(field);
        assert!((fy0 - (y0 + 2.5)).abs() < 1e-12);
        // An inset equilateral triangle loses three inradius-steps of height: 3 × 2.5.
        assert!((fy1 - fy0 - (20.0 - 7.5)).abs() < 1e-12);
    }

    #[test]
    fn the_white_edge_is_drawn_first_and_outset_half_a_stroke() {
        let prims = warning_triangle(
            0.0,
            0.0,
            20.0,
            Colour::Red,
            Colour::White,
            2.5,
            true,
            0.5,
            Colour::White,
        );
        assert_eq!(prims.len(), 3);
        let (edge, c) = outline(&prims[0]);
        assert_eq!(c, Colour::White);
        let (_, ey0, _, _) = bbox(edge);
        assert!((ey0 + 10.5).abs() < 1e-12);
        assert_eq!(outline(&prims[1]).1, Colour::Red);

        let prims = regulatory_roundel(
            0.0,
            0.0,
            20.0,
            Colour::Red,
            Colour::White,
            2.5,
            true,
            0.5,
            Colour::White,
        );
        let radii: Vec<f64> = prims.iter().map(|p| outline(p).0[0].x).collect();
        assert_eq!(radii, vec![10.5, 10.0, 7.5]);
        let colours: Vec<Colour> = prims.iter().map(|p| outline(p).1).collect();
        assert_eq!(colours, vec![Colour::White, Colour::Red, Colour::White]);
    }

    #[test]
    fn only_a_green_ground_gets_the_white_edge() {
        let p = uk_pack();
        let (signs, shapes) = p.embedded.drawing_rules().unwrap().unwrap();
        for kind in [
            EmbeddedType::WarningTriangle,
            EmbeddedType::RegulatoryRoundel,
        ] {
            let e = EmbeddedSign {
                symbol_id: "x".into(),
                kind,
            };
            assert_eq!(draw(&e, 0.0, 0.0, Colour::Green, signs, shapes).len(), 3);
            for ground in [Colour::White, Colour::Blue, Colour::Yellow, Colour::Brown] {
                assert_eq!(draw(&e, 0.0, 0.0, ground, signs, shapes).len(), 2);
            }
        }
    }

    #[test]
    fn the_column_is_as_wide_as_the_widest_frame() {
        let p = uk_pack();
        let roundel = sign_with(&[EmbeddedType::RegulatoryRoundel]);
        assert_eq!(column_width(&roundel, &p).unwrap(), Some(20.0));
        let both = sign_with(&[
            EmbeddedType::RegulatoryRoundel,
            EmbeddedType::WarningTriangle,
        ]);
        let w = column_width(&both, &p).unwrap().unwrap();
        assert!((w - 40.0 / 3.0_f64.sqrt()).abs() < 1e-12);
        assert_eq!(column_width(&sign_with(&[]), &p).unwrap(), None);
        let mut none = sign_with(&[]);
        none.embedded = None;
        assert_eq!(column_width(&none, &p).unwrap(), None);
    }

    #[test]
    fn stacked_signs_share_the_span_evenly_from_the_top() {
        let p = uk_pack();
        let sign = sign_with(&[
            EmbeddedType::RegulatoryRoundel,
            EmbeddedType::RegulatoryRoundel,
        ]);
        let mut prims = Vec::new();
        draw_stacked(&mut prims, &sign, &p, 7.0, 100.0, 20.0, Colour::White).unwrap();
        assert_eq!(prims.len(), 4);
        // Slots of 40 sw: centres at 80 and 40. A disc's first point is at the centre height.
        let (first, _) = outline(&prims[0]);
        let (second, _) = outline(&prims[2]);
        assert!((first[0].y - 80.0).abs() < 1e-12 && (first[0].x - 17.0).abs() < 1e-12);
        assert!((second[0].y - 40.0).abs() < 1e-12);
    }
}
