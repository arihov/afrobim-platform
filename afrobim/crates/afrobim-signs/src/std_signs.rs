//! Layout of the standard (catalog) sign families: information signs, the UK warning and
//! regulatory signs of TSRGD 2016 with their plates, and supplementary plates.
//!
//! Port of the C# `SignLayout.Layout` pipeline for these kinds: dispatch to the family's
//! engine, the tile-table diagnostics, the sign-with-plate assembly, the overlap check for a
//! shaped border (C# `LayoutOverlapCheck.CheckRegions`), the route-number note, and then
//! lettering as glyph outlines.
//!
//! Warning and regulatory signs are drawn from TSRGD artwork where the pack carries it for the
//! sign's symbol ([`crate::std_tsrgd`]). Otherwise they are drawn as the Uganda TSM triangle,
//! discs, slashes and arrows of C# `WarningSignLayout` and `RegulatorySignLayout`, with the
//! proportions of the pack's `roundelsTriangles` section ([`crate::ug_signs`]).

use afrobim_draw::{Drawing, Extents, Pt2};

use crate::measurer::{self, TextTileMeasurer};
use crate::model::{Diagnostic, RouteStatus, SignDefinition, SignKind};
use crate::outliner::TextOutliner;
use crate::overlap::{clip, clipped_area, signed_area};
use crate::pack::{self, RulePack};
use crate::prim::{Prim, TextRun};
use crate::stack::validate_constraints;
use crate::std_info::InformationSignLayout;
use crate::std_pack::StdRules;
use crate::std_plate::SupplementaryPlateLayout;
use crate::std_tsrgd;
use crate::{SignEngine, SignError, SignLayout};

/// The clause the C# constraint validator cites when a layout does not pass it the pack's own
/// (the warning, regulatory and supplementary engines do not).
const DEFAULT_CONSTRAINT_CLAUSE: &str = "Uganda TSM Vol.2 App.1 §2.1.2";

/// Overlap area below this (sw²) is rounding, not an overlap.
const AREA_TOLERANCE: f64 = 1e-6;

/// What a standard-sign engine hands back before lettering is outlined.
#[derive(Debug, Clone)]
pub(crate) struct StdLayout {
    pub prims: Vec<Prim>,
    pub diagnostics: Vec<Diagnostic>,
    pub width: f64,
    pub height: f64,
    /// The outlines of the sign's symbols, as the layout registered them for the overlap check.
    pub symbols: Vec<Vec<Pt2>>,
    /// The areas lettering and symbols must stay inside (the inner edge of a shaped border, a
    /// plate inside its border). Each is a convex outline. Empty for a layout that is not
    /// checked.
    pub regions: Vec<Vec<Pt2>>,
}

impl StdLayout {
    /// A layout that registers nothing for the overlap check.
    pub fn plain(prims: Vec<Prim>, diagnostics: Vec<Diagnostic>, width: f64, height: f64) -> Self {
        StdLayout {
            prims,
            diagnostics,
            width,
            height,
            symbols: Vec::new(),
            regions: Vec::new(),
        }
    }
}

/// True for the kinds this module lays out.
pub(crate) fn handles(kind: SignKind) -> bool {
    matches!(
        kind,
        SignKind::InformationSign
            | SignKind::WarningSign
            | SignKind::RegulatoryProhibitory
            | SignKind::RegulatoryMandatory
            | SignKind::Stop
            | SignKind::GiveWay
            | SignKind::SupplementaryPlate
    )
}

/// The constraint diagnostics as the warning, regulatory and supplementary engines raise them:
/// citing the validator's default clause, not the pack's.
pub(crate) fn validate_constraints_default_clause(
    sign: &SignDefinition,
    pack: &RulePack,
) -> Vec<Diagnostic> {
    let mut diags = validate_constraints(sign, pack);
    for d in &mut diags {
        d.source = DEFAULT_CONSTRAINT_CLAUSE.to_string();
    }
    diags
}

/// Lays out a sign of one of the kinds [`handles`] accepts, lettering as glyph outlines.
pub(crate) fn layout(engine: &SignEngine, sign: &SignDefinition) -> Result<SignLayout, SignError> {
    let pack = &engine.pack;
    let std = &engine.std;
    let measurer = TextTileMeasurer::new(&engine.medium, &pack.panel, Some(&engine.heavy));

    let roundels = engine.extra.roundels_triangles.as_ref();
    let mut result = dispatch(pack, std, roundels, &measurer, sign)?;

    // A character the pack's tile table does not list was measured by the font; say so.
    for prim in &result.prims {
        let Prim::Text(t) = prim else { continue };
        let missing = measurer.font_fallbacks(&t.text, t.face)?;
        let (false, Some(table)) = (missing.is_empty(), measurer::table_for(&pack.panel, t.face))
        else {
            continue;
        };
        let listed: Vec<String> = missing.iter().map(|c| format!("'{c}'")).collect();
        result.diagnostics.push(Diagnostic::warning(
            "TILE_WIDTH_FROM_FONT",
            format!(
                "'{}' in {}: {} not in the tile table; the font's advance width is used",
                t.text,
                t.face.name(),
                listed.join(" ")
            ),
            &table.source,
        ));
    }

    // A UK warning or regulatory sign with a plate legend is laid out with its plate as one
    // sign (TSRGD 2016 associated plates, sized by TSM Chapters 3 and 4).
    let has_legend = sign
        .info_distance
        .as_deref()
        .is_some_and(|l| !l.trim().is_empty());
    if has_legend {
        if let Some(art) = std_tsrgd::resolve(std, sign) {
            result = std_tsrgd::with_plate(pack, std, &measurer, sign, &art, result)?;
        }
    }

    // Geometry overlap check: the sign's lettering against its symbols, and the symbols
    // against the inside edge of the border.
    let overlaps = check_regions(pack, &measurer, &result)?;
    result.diagnostics.extend(overlaps);

    // Route numbers, patches and panels are drawn by the guidance layouts only. Here, say so
    // rather than drop them silently.
    if sign.destinations.iter().any(|d| {
        d.route_number.as_ref().is_some_and(|n| !n.is_empty())
            || d.route != RouteStatus::Unspecified
    }) {
        result.diagnostics.push(Diagnostic::warning(
            "ROUTE_NUMBER_NOT_DRAWN",
            format!(
                "Route numbers, patches and panels are not drawn on {} signs yet",
                family_label(sign.kind)
            ),
            pack.clause("routeNumbers", &pack.source),
        ));
    }

    // Lettering as Transport glyph outlines.
    let mut diags = result.diagnostics;
    let outliner = TextOutliner {
        measurer: &measurer,
        medium: &engine.medium,
        heavy: &engine.heavy,
        panel: &pack.panel,
    };
    let primitives = outliner.apply(result.prims, &mut diags)?;

    // A TSRGD sign's stroke width is its tabled size over the nominal dimension, with the
    // x-height four stroke widths. Any other sign takes the band of its family's text-size
    // table (C# `SignFamily.TextSizeFor`).
    let (sw_mm, x_height_mm) = match std_tsrgd::sw_mm(std, sign) {
        Some(sw) => (sw, 4.0 * sw),
        None => {
            let size = pack::text_size_for_family(pack, sign.kind.family_token(), sign.speed_kph)?;
            (size.sw_mm, size.x_height_mm)
        }
    };

    Ok(SignLayout {
        drawing: Drawing {
            primitives,
            extents: Extents {
                min: Pt2::new(0.0, 0.0),
                max: Pt2::new(result.width, result.height),
            },
        },
        sw_mm,
        x_height_mm,
        diagnostics: diags,
    })
}

fn dispatch(
    pack: &RulePack,
    std: &StdRules,
    roundels: Option<&crate::ug_pack::RoundelTriangleRules>,
    measurer: &TextTileMeasurer,
    sign: &SignDefinition,
) -> Result<StdLayout, SignError> {
    match sign.kind {
        SignKind::InformationSign => InformationSignLayout { pack, measurer }.layout(sign),
        SignKind::SupplementaryPlate => SupplementaryPlateLayout {
            pack,
            std,
            measurer,
        }
        .layout(sign),
        kind if std_tsrgd::is_warning_or_regulatory(kind) => {
            let diags = validate_constraints_default_clause(sign, pack);
            // A pack with TSRGD artwork for this symbol (the UK pack) draws the TSRGD sign: its
            // body from the pack and the interior read from the diagram.
            match std_tsrgd::resolve(std, sign) {
                Some(art) => std_tsrgd::layout(pack, std, measurer, sign, &art, diags),
                // Otherwise the Uganda TSM drawing: the plain triangle, discs, octagon and
                // give-way triangle (ug_signs.rs).
                None => crate::ug_signs::layout(pack, roundels, measurer, sign, diags),
            }
        }
        kind => Err(SignError::Unsupported(format!(
            "sign kind {} ('{}') is not a standard-sign family",
            kind.name(),
            sign.code
        ))),
    }
}

/// The family name the route-number note uses (C# `SignFamily.Label` of the kind's token: the
/// warning, regulatory and supplementary kinds share the stack token).
fn family_label(kind: SignKind) -> &'static str {
    if kind == SignKind::InformationSign {
        "Information"
    } else {
        "Stack type"
    }
}

// ─── overlap check for a shaped border ────────────────────────────────────────

type Poly = Vec<(f64, f64)>;

/// A layout that registers regions is checked against them: lettering must not run into the
/// sign's symbols (`LAYOUT_LEGEND_OVERLAPS_SYMBOL`) and every symbol must lie inside one region
/// (`LAYOUT_SYMBOL_IN_BORDER`). Lettering is tested by its measured box from the baseline to
/// the capital height. (The C# check also tests lettering against the regions by its glyph
/// outlines when it is handed an outliner; the desktop app hands it none, and neither does
/// this port.) Only overlap with area counts; shapes that merely touch do not.
fn check_regions(
    pack: &RulePack,
    measurer: &TextTileMeasurer,
    result: &StdLayout,
) -> Result<Vec<Diagnostic>, SignError> {
    let mut diags = Vec::new();
    if result.regions.is_empty() {
        return Ok(diags);
    }
    let clause = pack.clause("layoutOverlap", &pack.source);
    let poly = |o: &Vec<Pt2>| o.iter().map(|p| (p.x, p.y)).collect::<Poly>();
    let regions: Vec<Poly> = result.regions.iter().map(poly).collect();
    let symbols: Vec<Poly> = result
        .symbols
        .iter()
        .map(poly)
        .filter(|o| o.len() >= 3)
        .collect();

    for prim in &result.prims {
        let Prim::Text(t) = prim else { continue };
        let w = measurer.width(&t.text, t.face)? * (t.x_height / 4.0);
        let (x0, y0) = (t.origin.x, t.baseline().y);
        let (x1, y1) = (x0 + w, y0 + t.x_height * TextRun::CAP_HEIGHT_PER_X_HEIGHT);
        if symbols
            .iter()
            .any(|s| clipped_area(s, x0, y0, x1, y1) > AREA_TOLERANCE)
        {
            diags.push(Diagnostic::warning(
                "LAYOUT_LEGEND_OVERLAPS_SYMBOL",
                format!("Legend '{}' runs into the sign's symbol", t.text),
                clause,
            ));
        }
    }

    let inside_some = |s: &Poly| {
        let area = signed_area(s).abs();
        regions
            .iter()
            .any(|r| area - clip_convex_area(s, r) <= AREA_TOLERANCE)
    };
    if symbols.iter().any(|s| !inside_some(s)) {
        diags.push(Diagnostic::warning(
            "LAYOUT_SYMBOL_IN_BORDER",
            "A symbol crosses the inside edge of the border".to_string(),
            clause,
        ));
    }
    Ok(diags)
}

/// Area of `poly` inside the convex polygon `clip_poly` (Sutherland–Hodgman against each of
/// its edges; `poly` may be concave).
fn clip_convex_area(poly: &[(f64, f64)], clip_poly: &[(f64, f64)]) -> f64 {
    let area = signed_area(clip_poly);
    if area == 0.0 || area.is_nan() {
        return 0.0;
    }
    let orient = area.signum();
    let mut p: Poly = poly.to_vec();
    for i in 0..clip_poly.len() {
        if p.is_empty() {
            break;
        }
        let a = clip_poly[i];
        let b = clip_poly[(i + 1) % clip_poly.len()];
        if (b.0 - a.0).abs() < 1e-15 && (b.1 - a.1).abs() < 1e-15 {
            continue;
        }
        let side = |q: (f64, f64)| orient * ((b.0 - a.0) * (q.1 - a.1) - (b.1 - a.1) * (q.0 - a.0));
        p = clip(
            &p,
            |q| side(q) >= -1e-12,
            |u, v| {
                let (su, sv) = (side(u), side(v));
                let t = su / (su - sv);
                (u.0 + t * (v.0 - u.0), u.1 + t * (v.1 - u.1))
            },
        );
    }
    if p.len() < 3 {
        0.0
    } else {
        signed_area(&p).abs()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn clipping_against_a_convex_region_keeps_the_area_inside() {
        // A right triangle with legs 10, wound either way.
        let tri = [(0.0, 0.0), (10.0, 0.0), (0.0, 10.0)];
        let tri_cw = [(0.0, 10.0), (10.0, 0.0), (0.0, 0.0)];
        let inside = [(1.0, 1.0), (3.0, 1.0), (3.0, 3.0), (1.0, 3.0)];
        let across = [(4.0, 4.0), (8.0, 4.0), (8.0, 8.0), (4.0, 8.0)];
        let outside = [(8.0, 8.0), (9.0, 8.0), (9.0, 9.0), (8.0, 9.0)];
        for region in [&tri[..], &tri_cw[..]] {
            assert!((clip_convex_area(&inside, region) - 4.0).abs() < 1e-12);
            // The hypotenuse x + y = 10 cuts the corner triangle (4,4) (6,4) (4,6) off the square.
            assert!((clip_convex_area(&across, region) - 2.0).abs() < 1e-12);
            assert_eq!(clip_convex_area(&outside, region), 0.0);
        }
        // A degenerate region holds nothing.
        assert_eq!(
            clip_convex_area(&inside, &[(0.0, 0.0), (5.0, 5.0), (10.0, 10.0)]),
            0.0
        );
    }

    /// A 20 sw square region with a symbol and a legend placed by the test.
    fn checked(symbol: [(f64, f64); 4], text_at: Option<Pt2>) -> Vec<String> {
        use crate::font::tests::{heavy, medium};
        let (pack, m, h) = (crate::pack::tests::uk_pack(), medium(), heavy());
        let measurer = TextTileMeasurer::new(&m, &pack.panel, Some(&h));
        let symbol: Vec<Pt2> = symbol.into_iter().map(|(x, y)| Pt2::new(x, y)).collect();
        let mut prims = vec![Prim::Fill {
            outline: symbol.clone(),
            colour: crate::model::Colour::Black,
        }];
        if let Some(origin) = text_at {
            prims.push(Prim::Text(TextRun {
                origin,
                text: "40".into(),
                face: crate::model::FontFace::TransportHeavy,
                x_height: 4.0,
                ink: None,
            }));
        }
        let layout = StdLayout {
            prims,
            diagnostics: Vec::new(),
            width: 20.0,
            height: 20.0,
            symbols: vec![symbol],
            regions: vec![vec![
                Pt2::new(2.0, 2.0),
                Pt2::new(18.0, 2.0),
                Pt2::new(18.0, 18.0),
                Pt2::new(2.0, 18.0),
            ]],
        };
        let diags = check_regions(&pack, &measurer, &layout).unwrap();
        assert!(diags.iter().all(|d| d.source == pack.source));
        diags.into_iter().map(|d| d.code).collect()
    }

    #[test]
    fn a_symbol_must_lie_inside_the_border() {
        let inside = [(4.0, 4.0), (8.0, 4.0), (8.0, 8.0), (4.0, 8.0)];
        let touching = [(2.0, 4.0), (8.0, 4.0), (8.0, 8.0), (2.0, 8.0)];
        let crossing = [(1.0, 4.0), (8.0, 4.0), (8.0, 8.0), (1.0, 8.0)];
        assert!(checked(inside, None).is_empty());
        assert!(checked(touching, None).is_empty());
        assert_eq!(checked(crossing, None), ["LAYOUT_SYMBOL_IN_BORDER"]);
    }

    /// Lettering is tested by its box from the baseline (2 sw above the tile foot at a 4 sw
    /// x-height) to the capital height (5.6 sw).
    #[test]
    fn lettering_must_not_run_into_a_symbol() {
        let symbol = [(4.0, 4.0), (8.0, 4.0), (8.0, 8.0), (4.0, 8.0)];
        assert_eq!(
            checked(symbol, Some(Pt2::new(6.0, 4.0))),
            ["LAYOUT_LEGEND_OVERLAPS_SYMBOL"]
        );
        // Beside the symbol, and with only the empty lower 2 sw of its tile over it.
        assert!(checked(symbol, Some(Pt2::new(8.0, 4.0))).is_empty());
        assert!(checked(symbol, Some(Pt2::new(6.0, 6.0))).is_empty());
    }

    #[test]
    fn the_route_number_note_names_the_family_by_its_token() {
        assert_eq!(family_label(SignKind::InformationSign), "Information");
        assert_eq!(family_label(SignKind::WarningSign), "Stack type");
        assert_eq!(family_label(SignKind::SupplementaryPlate), "Stack type");
    }

    #[test]
    fn only_the_standard_sign_families_are_handled_here() {
        assert!(handles(SignKind::WarningSign) && handles(SignKind::Stop));
        assert!(handles(SignKind::GiveWay) && handles(SignKind::RegulatoryMandatory));
        assert!(handles(SignKind::InformationSign) && handles(SignKind::SupplementaryPlate));
        assert!(!handles(SignKind::StackType) && !handles(SignKind::MapTypeTee));
        assert!(!handles(SignKind::DirectionSign) && !handles(SignKind::Diversion));
    }
}
