//! Unit tests of the map-type layout (`map_symbol.rs`), worked by hand from UK TSM Chapter 7
//! Figures 5-2, 5-3, 5-5, 5-7 and 5-13 rather than read back from the engine.

use afrobim_draw::Pt2;

use super::*;
use crate::font::tests::{heavy, medium};
use crate::font::FontMetrics;
use crate::model::{Colour, ColourScheme, EmbeddedSign, EmbeddedType, FontFace};
use crate::pack::tests::uk_pack;
use crate::prim::TextRun;

fn close(a: f64, b: f64) -> bool {
    (a - b).abs() < 1e-6
}

struct Fonts {
    medium: FontMetrics,
    heavy: FontMetrics,
}

fn fonts() -> Fonts {
    Fonts {
        medium: medium(),
        heavy: heavy(),
    }
}

fn try_lay(pack: &RulePack, fonts: &Fonts, sign: &SignDefinition) -> Result<RawLayout, SignError> {
    let measurer = TextTileMeasurer::new(&fonts.medium, &pack.panel, Some(&fonts.heavy));
    MapSymbolLayout::new(pack, &measurer)?.layout(sign)
}

fn lay(pack: &RulePack, fonts: &Fonts, sign: &SignDefinition) -> RawLayout {
    try_lay(pack, fonts, sign).expect("the sign lays out")
}

/// A legend's Appendix C tile width in Transport Medium.
fn tiles(pack: &RulePack, fonts: &Fonts, text: &str) -> f64 {
    TextTileMeasurer::new(&fonts.medium, &pack.panel, Some(&fonts.heavy))
        .width(text, FontFace::TransportMedium)
        .unwrap()
}

fn map(kind: SignKind, dests: Vec<Destination>) -> SignDefinition {
    SignDefinition {
        kind,
        ..SignDefinition::map_crossroads("M", 60, dests)
    }
}

fn l(name: &str) -> Destination {
    Destination::new(name, Direction::Left)
}

fn a(name: &str) -> Destination {
    Destination::new(name, Direction::Ahead)
}

fn r(name: &str) -> Destination {
    Destination::new(name, Direction::Right)
}

/// The route symbol: the one outline the layout registered.
fn symbol(layout: &RawLayout) -> &[Pt2] {
    assert_eq!(layout.symbols.len(), 1);
    &layout.symbols[0]
}

fn text<'a>(layout: &'a RawLayout, legend: &str) -> &'a TextRun {
    layout
        .prims
        .iter()
        .find_map(|p| match p {
            Prim::Text(t) if t.text == legend => Some(t),
            _ => None,
        })
        .unwrap_or_else(|| panic!("no legend '{legend}'"))
}

fn has(pts: &[Pt2], x: f64, y: f64) -> bool {
    pts.iter().any(|p| close(p.x, x) && close(p.y, y))
}

fn bounds(pts: &[Pt2]) -> (f64, f64, f64, f64) {
    pts.iter().fold(
        (f64::MAX, f64::MAX, f64::MIN, f64::MIN),
        |(x0, y0, x1, y1), p| (x0.min(p.x), y0.min(p.y), x1.max(p.x), y1.max(p.y)),
    )
}

/// The two ends of the approach arm's base, which stands 1.5 sw above the 1.5 sw border.
fn foot(sym: &[Pt2]) -> (f64, f64) {
    let mut xs: Vec<f64> = sym
        .iter()
        .filter(|p| close(p.y, 3.0))
        .map(|p| p.x)
        .collect();
    xs.sort_by(f64::total_cmp);
    assert_eq!(xs.len(), 2, "{xs:?}");
    (xs[0], xs[1])
}

fn codes(layout: &RawLayout) -> Vec<&str> {
    layout.diagnostics.iter().map(|d| d.code.as_str()).collect()
}

// ─── helpers ──────────────────────────────────────────────────────────────────────

/// §5.7.3: a 1 sw fillet between tangent points 1 sw either side of the corner, every vertex
/// on the circle centred in the ground.
#[test]
fn a_corner_is_rounded_by_a_fillet_tangent_to_both_edges() {
    let mut pts = Pts::new();
    // An edge running up (+Y) turns to run right (+X) at the origin.
    corner(&mut pts, (0.0, 0.0), (0.0, 1.0), (1.0, 0.0), 1.0);
    assert_eq!(pts.len(), FILLET_SEGMENTS + 1);
    assert!(close(pts[0].0, 0.0) && close(pts[0].1, -1.0));
    assert!(close(pts[8].0, 1.0) && close(pts[8].1, 0.0));
    for q in &pts {
        assert!(close((q.0 - 1.0).hypot(q.1 + 1.0), 1.0), "{q:?}");
    }
    // Half way round: 45° from each tangent point.
    let h = std::f64::consts::FRAC_1_SQRT_2;
    assert!(close(pts[4].0, 1.0 - h) && close(pts[4].1, -1.0 + h));

    let mut square = Pts::new();
    corner(&mut square, (2.0, 3.0), (0.0, 1.0), (1.0, 0.0), 0.0);
    assert_eq!(square, vec![(2.0, 3.0)]);
}

#[test]
fn a_fillet_takes_the_short_way_round_and_adds_interior_points_only() {
    let at = |deg: f64| {
        let t = deg.to_radians();
        (2.0 * t.cos(), 2.0 * t.sin())
    };
    // From bearing 170° to −170° about the origin: 20° through 180°, not 340° through 0°.
    let mut pts = Pts::new();
    fillet(&mut pts, (0.0, 0.0), 2.0, at(170.0), at(-170.0));
    assert_eq!(pts.len(), FILLET_SEGMENTS - 1);
    assert!(pts.iter().all(|q| q.0 < -1.9), "{pts:?}");
    assert!(close(pts[3].0, -2.0) && close(pts[3].1, 0.0));

    let mut none = Pts::new();
    fillet(&mut none, (0.0, 0.0), 0.0, (1.0, 0.0), (0.0, 1.0));
    assert!(none.is_empty());
}

#[test]
fn angles_are_written_like_the_csharp_custom_format() {
    assert_eq!(one_decimal(30.0), "30");
    assert_eq!(one_decimal(20.5), "20.5");
    assert_eq!(one_decimal(24.96), "25");
    assert_eq!(one_decimal(24.94), "24.9");
}

/// Figure 5-13 with the approach arm only: the ring is 7r inside and 12r outside, drawn
/// through whole degrees, with radial cut edges 30° and 60° round from the approach axis.
#[test]
fn the_ring_outline_runs_round_the_outside_then_back_round_the_inside() {
    let ms = uk_pack().map_symbol.unwrap();
    let approach = RingArm {
        bearing_deg: 270.0,
        width: 6.0,
        reach: 20.5,
        pointed: false,
    };
    let ring = ring_outline(0.0, 0.0, 12.0, 7.0, &ms, &[approach], -60.0, -30.0);
    let pts = &ring.points;
    let at = |x: f64, y: f64| pts.iter().any(|q| close(q.0, x) && close(q.1, y));

    // Starts on the outside at the far cut edge and ends on the inside at the same bearing.
    let (sin30, cos30) = (0.5, 3f64.sqrt() / 2.0);
    assert!(close(pts[0].0, 12.0 * cos30) && close(pts[0].1, -12.0 * sin30));
    let last = pts[pts.len() - 1];
    assert!(close(last.0, 7.0 * cos30) && close(last.1, -7.0 * sin30));
    // The near cut edge is radial, at 300°.
    assert!(at(12.0 * sin30, -12.0 * cos30) && at(7.0 * sin30, -7.0 * cos30));
    // The ring's top, left and right are exact vertices on both circles.
    for radius in [12.0, 7.0] {
        assert!(at(0.0, radius) && at(-radius, 0.0) && at(radius, 0.0));
    }

    // The approach arm: 6 sw wide, square ended 20.5 sw below the centre.
    assert!(at(-3.0, -20.5) && at(3.0, -20.5));
    // Its 1r fillets are tangent to the ring where a 13r circle meets the lines 4 sw either
    // side of the arm axis.
    let delta = 4f64.atan2(153f64.sqrt()).to_degrees();
    assert!(close(ring.first_after_cut, 270.0 - delta));
    assert!(close(ring.last_before_cut, 270.0 + delta));
    // Every vertex is on the ring, on a fillet or on the arm: none is inside 7r.
    assert!(pts.iter().all(|q| q.0.hypot(q.1) > 7.0 - 1e-9));
    // One degree per chord: 330° of outer ring less the arm's 2 × 17.92°, and 330° of inner.
    let on = |radius: f64| pts.iter().filter(|q| close(q.0.hypot(q.1), radius)).count();
    assert_eq!(on(7.0), 331);
    assert_eq!(on(12.0), 331 - 35 + 2);
}

#[test]
fn exits_are_met_in_order_from_the_cut_out_and_pointed() {
    let ms = uk_pack().map_symbol.unwrap();
    let arm = |bearing_deg, pointed| RingArm {
        bearing_deg,
        width: 4.0,
        reach: 24.0,
        pointed,
    };
    // Given out of order; the outline meets right (0°), ahead (90°), left (180°), approach.
    let arms = [
        arm(270.0, false),
        arm(180.0, true),
        arm(0.0, true),
        arm(90.0, true),
    ];
    let ring = ring_outline(50.0, 40.0, 12.0, 7.0, &ms, &arms, -60.0, -30.0);
    let index = |x: f64, y: f64| {
        ring.points
            .iter()
            .position(|q| close(q.0, x) && close(q.1, y))
            .unwrap_or_else(|| panic!("no vertex at ({x}, {y})"))
    };
    let (right, ahead, left) = (index(74.0, 40.0), index(50.0, 64.0), index(26.0, 40.0));
    assert!(right < ahead && ahead < left);
    // 45° points: the shoulders are half the width back from the point.
    assert!(close(ring.points[right - 1].0, 72.0) && close(ring.points[right - 1].1, 38.0));
    assert!(close(ring.points[right + 1].0, 72.0) && close(ring.points[right + 1].1, 42.0));
    // The first arm after the cut-out is the right exit; the last is the approach.
    let delta = 3f64.atan2((13f64 * 13.0 - 9.0).sqrt()).to_degrees();
    assert!(close(ring.first_after_cut, -delta));
    assert!(close(ring.last_before_cut, 270.0 + delta));
}

// ─── crossroads and T-junction ────────────────────────────────────────────────────

/// The app's "Add map" sign, worked by hand from Figures 5-2 and 5-7.
///
/// Vertical: side tiles 1.5 sw above the 1.5 sw border, 8 sw tall, 2.5 sw below 6 sw arms, so
/// the arms' centre line is at 1.5 + 1.5 + 8 + 2.5 + 3 = 16.5. The vertical arm's point is
/// 12.5 sw above the top of the side arms (16.5 + 3 + 12.5 = 32) and "Ahead" (no descender)
/// sits on it: tile top 40, plus 2.5 and the border: 44 high.
#[test]
fn the_app_default_crossroads_has_the_figure_5_7_dimensions() {
    let (pack, fonts) = (uk_pack(), fonts());
    let sign =
        SignDefinition::map_crossroads("ADS-001", 60, vec![l("Left"), a("Ahead"), r("Right")]);
    assert_eq!(sign.kind, SignKind::MapTypeCrossroads);
    assert_eq!(sign.approach_class, RoadClass::Primary);
    assert_eq!(sign.background, Colour::Green);
    let layout = lay(&pack, &fonts, &sign);
    assert!(layout.diagnostics.is_empty(), "{:?}", layout.diagnostics);
    assert!(close(layout.height, 44.0), "{}", layout.height);

    let sym = symbol(&layout);
    let (min_x, min_y, max_x, max_y) = bounds(sym);
    // §5.7.2: the approach arm stands 1.5 sw above the border, 6 sw wide (§5.2.1).
    assert!(close(min_y, 3.0));
    let (xv0, xv1) = foot(sym);
    assert!(close(xv1 - xv0, 6.0));

    // Figure 5-7 "45°": the ahead point is 3 sw above its shoulders.
    assert!(close(max_y, 32.0));
    assert!(has(sym, (xv0 + xv1) / 2.0, 32.0));
    assert!(has(sym, xv0, 29.0) && has(sym, xv1, 29.0));

    // §5.3.4: each side arm's point is two thirds along its block from the vertical arm.
    let (w_left, w_right) = (tiles(&pack, &fonts, "Left"), tiles(&pack, &fonts, "Right"));
    let (left, right) = (text(&layout, "Left"), text(&layout, "Right"));
    assert!(close(left.origin.x, 4.0) && close(left.origin.y, 3.0));
    assert!(close(right.origin.y, 3.0));
    assert!(close(min_x, 4.0 + w_left / 3.0));
    assert!(close(max_x, right.origin.x + w_right * 2.0 / 3.0));
    assert!(has(sym, min_x, 16.5) && has(sym, max_x, 16.5));
    assert!(has(sym, min_x + 3.0, 19.5) && has(sym, min_x + 3.0, 13.5));
    // Figure 5-2: each block is at least 4 sw beside the vertical arm, and further when the
    // arm would otherwise be under 12 sw along its shorter side, from the vertical arm to the
    // shoulder of the 3 sw point (§5.4.4).
    for (gap, w) in [
        (xv0 - (4.0 + w_left), w_left),
        (right.origin.x - xv1, w_right),
    ] {
        assert!(close(gap, 4f64.max(12.0 + 3.0 - w * 2.0 / 3.0)), "{gap}");
    }
    assert!(xv0 - (min_x + 3.0) >= 12.0 - 1e-6);
    assert!((max_x - 3.0) - xv1 >= 12.0 - 1e-6);
    // "Left" is short enough for the 12 sw minimum to set its gap.
    assert!(close(xv0 - (min_x + 3.0), 12.0));
    assert!(close(layout.width, right.origin.x + w_right + 2.5 + 1.5));

    // §5.3.5: the ahead block is centred over the vertical arm, on its point.
    let ahead = text(&layout, "Ahead");
    assert!(close(ahead.origin.y, 32.0));
    assert!(close(
        ahead.origin.x + tiles(&pack, &fonts, "Ahead") / 2.0,
        (xv0 + xv1) / 2.0
    ));

    // §5.7.3: the four junction corners are 1 sw fillets; every vertex within the corner's
    // 1 sw square lies on the circle centred in the ground.
    for (corner_x, corner_y, sx, sy) in [
        (xv1, 13.5, 1.0, -1.0),
        (xv1, 19.5, 1.0, 1.0),
        (xv0, 19.5, -1.0, 1.0),
        (xv0, 13.5, -1.0, -1.0),
    ] {
        let (ccx, ccy) = (corner_x + sx, corner_y + sy);
        let arc: Vec<&Pt2> = sym
            .iter()
            .filter(|p| {
                let (dx, dy) = ((p.x - corner_x) * sx, (p.y - corner_y) * sy);
                (-1e-9..=1.0 + 1e-9).contains(&dx) && (-1e-9..=1.0 + 1e-9).contains(&dy)
            })
            .collect();
        assert_eq!(arc.len(), FILLET_SEGMENTS + 1);
        for p in arc {
            assert!(close((p.x - ccx).hypot(p.y - ccy), 1.0));
        }
    }

    // The plate: the §2.15 border fill, the ground, then the symbol in the arm colour.
    assert!(matches!(
        &layout.prims[0],
        Prim::Fill {
            colour: Colour::White,
            ..
        }
    ));
    assert!(matches!(
        &layout.prims[1],
        Prim::Fill {
            colour: Colour::Green,
            ..
        }
    ));
    assert!(matches!(
        &layout.prims[2],
        Prim::Fill { colour: Colour::White, outline } if outline == sym
    ));
}

/// Figure 5-5: a direction without destinations is a square stub as long as it is wide, 2.5 sw
/// from the nearer border; a side stub is at least 4 sw above the bottom border.
#[test]
fn a_direction_without_destinations_gets_a_square_stub() {
    let (pack, fonts) = (uk_pack(), fonts());
    let layout = lay(
        &pack,
        &fonts,
        &map(
            SignKind::MapTypeCrossroads,
            vec![a("Hayford"), r("Westfield")],
        ),
    );
    let sym = symbol(&layout);
    let (stub_end, _, _, _) = bounds(sym);
    assert!(close(stub_end, 1.5 + 2.5));
    let mut end: Vec<f64> = sym
        .iter()
        .filter(|p| close(p.x, stub_end))
        .map(|p| p.y)
        .collect();
    end.sort_by(f64::total_cmp);
    assert_eq!(end.len(), 2);
    assert!(close(end[1] - end[0], 6.0));
    assert!(close(foot(sym).0 - stub_end, 6.0));
    // With turns one way only the ahead block is ranged against the side border.
    assert!(close(text(&layout, "Hayford").origin.x, 4.0));
    assert!(layout.diagnostics.is_empty());

    // No destinations at all: three stubs round a 6 sw square junction. The side stubs' centre
    // line is 1.5 + 4 + 3 above the bottom; the ahead stub ends 2.5 sw below the top border.
    let empty = lay(&pack, &fonts, &map(SignKind::MapTypeCrossroads, vec![]));
    assert!(close(empty.width, 2.0 * (1.5 + 2.5 + 6.0) + 6.0));
    assert!(close(empty.height, 1.5 + 4.0 + 6.0 + 6.0 + 2.5 + 1.5));
    let (x0, y0, x1, y1) = bounds(symbol(&empty));
    assert!(close(x0, 4.0) && close(x1, empty.width - 4.0));
    assert!(close(y0, 3.0) && close(y1, empty.height - 4.0));
}

/// §5.2.1: 6 sw for primary routes and motorways, 4 sw for non-primary routes, the approach
/// width when no status is given; the vertical arm follows the approach road.
#[test]
fn arm_widths_follow_the_route_status_and_the_approach_class() {
    let (pack, fonts) = (uk_pack(), fonts());
    let mut sign = map(
        SignKind::MapTypeCrossroads,
        vec![
            a("Hayford"),
            l("Barton").with_route("B1040", RouteStatus::NonPrimary),
            r("Royston").with_route("A505", RouteStatus::Primary),
        ],
    );
    sign.approach_class = RoadClass::Local;
    sign.scheme = ColourScheme::Secondary;
    let layout = lay(&pack, &fonts, &sign);
    let sym = symbol(&layout);
    let (min_x, _, max_x, _) = bounds(sym);
    let yc = sym.iter().find(|p| close(p.x, min_x)).unwrap().y;
    // Left: 4 sw wide, so a 2 sw point; right: 6 sw wide, a 3 sw point.
    assert!(has(sym, min_x + 2.0, yc + 2.0) && has(sym, min_x + 2.0, yc - 2.0));
    assert!(has(sym, max_x - 3.0, yc + 3.0) && has(sym, max_x - 3.0, yc - 3.0));
    // The local approach is 2.5 sw wide.
    let (xv0, xv1) = foot(sym);
    assert!(close(xv1 - xv0, 2.5));
    // On a white sign the symbol is black.
    assert!(layout
        .prims
        .iter()
        .any(|p| matches!(p, Prim::Fill { colour: Colour::Black, outline } if outline == sym)));
}

/// Figure 5-2: tiles sit on the point of the vertical arm; a line with a descender, a patch
/// or a panel is 1.5 sw above it.
#[test]
fn a_descender_or_patch_is_kept_clear_of_the_arm_point() {
    let (pack, fonts) = (uk_pack(), fonts());
    let apex = |layout: &RawLayout| bounds(symbol(layout)).3;

    let plain = lay(
        &pack,
        &fonts,
        &map(SignKind::MapTypeCrossroads, vec![a("Ahead")]),
    );
    assert!(close(text(&plain, "Ahead").origin.y, apex(&plain)));

    let descender = lay(
        &pack,
        &fonts,
        &map(SignKind::MapTypeCrossroads, vec![a("Kyenjojo")]),
    );
    assert!(close(
        text(&descender, "Kyenjojo").origin.y,
        apex(&descender) + 1.5
    ));

    // A motorway number on a green sign stands on a blue patch with a white edge, below three
    // names: the patch is the column's foot.
    let dests: Vec<Destination> = ["Aber", "Bala", "Conwy"]
        .iter()
        .enumerate()
        .map(|(i, n)| {
            a(n).with_order(i as i32)
                .with_route("M5", RouteStatus::Primary)
        })
        .collect();
    let patched = lay(&pack, &fonts, &map(SignKind::MapTypeCrossroads, dests));
    let patch_foot = patched
        .prims
        .iter()
        .filter_map(|p| match p {
            Prim::Fill {
                colour: Colour::Blue,
                outline,
            } => Some(bounds(outline).1),
            _ => None,
        })
        .fold(f64::MAX, f64::min);
    // 0.5 sw of white edge (§3.3.1) below the blue, then 1.5 sw to the point.
    assert!(
        close(patch_foot, apex(&patched) + 1.5 + 0.5),
        "{patch_foot}"
    );
}

/// Figure 5-3: the T-junction's through arm is 4 sw below the top border and its top edge is
/// straight; forward destinations are ignored with a warning.
#[test]
fn a_tee_has_its_through_arm_4_sw_below_the_top_border() {
    let (pack, fonts) = (uk_pack(), fonts());
    let layout = lay(
        &pack,
        &fonts,
        &map(SignKind::MapTypeTee, vec![l("Kitagata"), r("Masheruka")]),
    );
    // 1.5 + 1.5 + 8 + 2.5 + 6 to the top of the arm, + 4 + 1.5.
    assert!(close(layout.height, 25.0));
    let sym = symbol(&layout);
    let (min_x, min_y, max_x, max_y) = bounds(sym);
    assert!(close(min_y, 3.0) && close(max_y, 19.5));
    // The top edge runs straight from one shoulder to the other: two vertices only.
    assert_eq!(sym.iter().filter(|p| close(p.y, 19.5)).count(), 2);
    assert!(has(sym, min_x + 3.0, 19.5) && has(sym, max_x - 3.0, 19.5));
    assert!(layout.diagnostics.is_empty());

    // Through arms of different widths (4 sw and 6 sw) step at the centre of the vertical arm.
    let stepped = lay(
        &pack,
        &fonts,
        &map(
            SignKind::MapTypeTee,
            vec![
                l("Milport").with_route("B12", RouteStatus::NonPrimary),
                r("Barton").with_route("A14", RouteStatus::Primary),
            ],
        ),
    );
    let sym = symbol(&stepped);
    let (xv0, xv1) = foot(sym);
    let cx = (xv0 + xv1) / 2.0;
    let top = bounds(sym).3;
    assert!(has(sym, cx, top) && has(sym, cx, top - 1.0));

    let ignored = lay(
        &pack,
        &fonts,
        &map(
            SignKind::MapTypeTee,
            vec![a("Nowhere"), l("Kitagata"), r("Masheruka")],
        ),
    );
    assert_eq!(codes(&ignored), ["TEE_AHEAD_IGNORED"]);
    let d = &ignored.diagnostics[0];
    assert_eq!(d.severity, Severity::Warning);
    assert_eq!(
        d.message,
        "1 forward destination(s) on a T-intersection: no through route exists"
    );
    assert_eq!(d.source, "UK TSM Chapter 7 §5.7");
    assert!(!ignored
        .prims
        .iter()
        .any(|p| matches!(p, Prim::Text(t) if t.text == "Nowhere")));
    assert!(close(ignored.height, 25.0));
}

/// Symbols and distances are not laid out on map-type signs, and a symbol id never reaches
/// the pictogram resolver.
#[test]
fn symbols_and_distances_are_left_off_map_type_signs() {
    let (pack, fonts) = (uk_pack(), fonts());
    let mut with = l("Gulu").with_distance("12");
    with.symbol_id = Some("aircraft".into());
    let plain = lay(&pack, &fonts, &map(SignKind::MapTypeTee, vec![l("Gulu")]));
    let decorated = lay(&pack, &fonts, &map(SignKind::MapTypeTee, vec![with]));
    assert_eq!(plain.prims, decorated.prims);
    assert!(decorated.diagnostics.is_empty());
}

// ─── roundabout ───────────────────────────────────────────────────────────────────

fn three_way() -> SignDefinition {
    map(
        SignKind::MapTypeRoundabout,
        vec![a("Kampala"), l("Nakawa"), r("Jinja")],
    )
}

/// Figure 5-13, worked by hand. The approach base is 1.5 sw above the border and the approach
/// arm 8.5 sw long, so the ring centre is 3 + 8.5 + 12 = 23.5 up. The ahead exit is 12 sw long
/// (point at 47.5); "Kampala" has a descender, so it sits 1.5 sw above the point: tile top 57,
/// plus 2.5 and the border: 61 high.
#[test]
fn the_roundabout_follows_figure_5_13() {
    let (pack, fonts) = (uk_pack(), fonts());
    let layout = lay(&pack, &fonts, &three_way());
    assert!(layout.diagnostics.is_empty(), "{:?}", layout.diagnostics);
    assert!(close(layout.height, 61.0), "{}", layout.height);

    let sym = symbol(&layout);
    let (min_x, min_y, max_x, max_y) = bounds(sym);
    assert!(close(min_y, 3.0) && close(max_y, 47.5));
    let cy = 23.5;
    // The side arms run through the ring centre.
    assert!(has(sym, min_x, cy) && has(sym, max_x, cy));
    let cx = sym.iter().find(|p| close(p.y, 47.5)).unwrap().x;

    // 7r inside, 12r outside; no vertex inside the island.
    assert!(has(sym, cx - 7.0, cy) && has(sym, cx + 7.0, cy) && has(sym, cx, cy + 7.0));
    assert!(sym.iter().all(|p| (p.x - cx).hypot(p.y - cy) > 7.0 - 1e-9));
    // Radial cut edges 30° and 60° below the horizontal, on both circles.
    let (sin30, cos30) = (0.5, 3f64.sqrt() / 2.0);
    for radius in [7.0, 12.0] {
        assert!(has(sym, cx + radius * cos30, cy - radius * sin30));
        assert!(has(sym, cx + radius * sin30, cy - radius * cos30));
    }
    // No ring vertex lies in the cut-out.
    assert!(!sym.iter().any(|p| {
        let bearing = (p.y - cy).atan2(p.x - cx).to_degrees();
        let radius = (p.x - cx).hypot(p.y - cy);
        bearing > -59.5 && bearing < -30.5 && radius < 12.0 + 1e-9
    }));

    // Para 5.39: every exit at least 12 sw from the outside of the ring.
    assert!((cx - 12.0) - min_x >= 12.0 - 1e-9);
    assert!(max_x - (cx + 12.0) >= 12.0 - 1e-9);

    // Side blocks: 2.5 sw below their 6 sw arm, outside the 14.5r circle at the corner nearest
    // the ring, 4 sw clear of the approach arm, and the arm's point two thirds along.
    let (w_left, w_right) = (
        tiles(&pack, &fonts, "Nakawa"),
        tiles(&pack, &fonts, "Jinja"),
    );
    let (left, right) = (text(&layout, "Nakawa"), text(&layout, "Jinja"));
    for t in [left, right] {
        assert!(close(t.origin.y + 8.0, cy - 3.0 - 2.5));
    }
    let top = cy - 5.5;
    let near_left = left.origin.x + w_left;
    assert!(close(left.origin.x, 4.0));
    assert!((near_left - cx).hypot(top - cy) >= 14.5 - 1e-9);
    assert!((right.origin.x - cx).hypot(top - cy) >= 14.5 - 1e-9);
    assert!(cx - 3.0 - near_left >= 4.0 - 1e-9);
    assert!(close(min_x, left.origin.x + w_left / 3.0));
    assert!(close(max_x, right.origin.x + w_right * 2.0 / 3.0));

    // The ahead block is centred over its arm, 1.5 sw above the point.
    let ahead = text(&layout, "Kampala");
    assert!(close(ahead.origin.y, 49.0));
    assert!(close(
        ahead.origin.x + tiles(&pack, &fonts, "Kampala") / 2.0,
        cx
    ));
}

/// Figure 5-13 "14.5r": a side block long enough for its exit arm is set by the keep-out
/// circle: its top corner, 3 + 2.5 below the centre line, is 14.5 sw from the ring centre.
#[test]
fn a_long_side_block_sits_on_the_keep_out_circle() {
    let (pack, fonts) = (uk_pack(), fonts());
    let layout = lay(
        &pack,
        &fonts,
        &map(
            SignKind::MapTypeRoundabout,
            vec![l("Kampala City Centre"), r("Entebbe International Airport")],
        ),
    );
    let sym = symbol(&layout);
    let (_, min_y, _, max_y) = bounds(sym);
    // No ahead exit: the ring top is the symbol's top; the centre is 12 sw below it.
    let cy = max_y - 12.0;
    assert!(close(cy, min_y + 8.5 + 12.0));
    let cx = sym.iter().find(|p| close(p.y, max_y)).unwrap().x;
    let gap = (14.5f64 * 14.5 - 5.5 * 5.5).sqrt();
    let right = text(&layout, "Entebbe International Airport");
    assert!(close(right.origin.x - cx, gap));
    let left = text(&layout, "Kampala City Centre");
    let near_left = left.origin.x + tiles(&pack, &fonts, "Kampala City Centre");
    assert!(close(cx - near_left, gap));
    // Figure 5-13: 4 sw from the ring to the top border with no ahead arm.
    assert!(close(layout.height, max_y + 4.0 + 1.5));
}

/// Figure 5-13 sub-figures: with no arm that way the ring is 4 sw from the border. The figure
/// gives that gap for the top and right borders only, so the left one is flagged.
#[test]
fn a_roundabout_without_an_exit_keeps_the_ring_4_sw_from_that_border() {
    let (pack, fonts) = (uk_pack(), fonts());
    let empty = lay(&pack, &fonts, &map(SignKind::MapTypeRoundabout, vec![]));
    // 1.5 + 4 + 24 + 4 + 1.5 wide; 1.5 + 1.5 + 8.5 + 24 + 4 + 1.5 high.
    assert!(close(empty.width, 35.0) && close(empty.height, 41.0));
    let (x0, y0, x1, y1) = bounds(symbol(&empty));
    assert!(close(x0, 5.5) && close(x1, 29.5) && close(y0, 3.0) && close(y1, 35.5));

    assert_eq!(codes(&empty), ["ROUNDABOUT_LEFT_GAP_UNCITED"]);
    let d = &empty.diagnostics[0];
    assert_eq!(d.severity, Severity::Info);
    assert_eq!(
        d.message,
        "No left exit: the ring is placed 4 sw from the left border. Figure 5-13 gives this gap only for the top \
         and right borders"
    );
    assert_eq!(
        d.source,
        "UK TSM Chapter 7 (2013) paras 5.39-5.40, Figure 5-13"
    );

    let left_only = lay(
        &pack,
        &fonts,
        &map(SignKind::MapTypeRoundabout, vec![l("Nakawa")]),
    );
    assert!(left_only.diagnostics.is_empty());
    let (_, _, x1, y1) = bounds(symbol(&left_only));
    assert!(close(x1, left_only.width - 1.5 - 4.0));
    assert!(close(y1, left_only.height - 1.5 - 4.0));
}

/// Para 5.39: the ring is 5 sw wide whatever the route status; only the arms change.
#[test]
fn the_ring_stays_5_sw_wide_on_a_local_approach() {
    let (pack, fonts) = (uk_pack(), fonts());
    let mut sign = three_way();
    sign.approach_class = RoadClass::Local;
    let layout = lay(&pack, &fonts, &sign);
    let sym = symbol(&layout);
    let (_, min_y, _, max_y) = bounds(sym);
    let cx = sym.iter().find(|p| close(p.y, max_y)).unwrap().x;
    let cy = min_y + 8.5 + 12.0;
    assert!(has(sym, cx - 7.0, cy) && has(sym, cx, cy + 7.0));
    // The 2.5 sw approach arm.
    assert!(has(sym, cx - 1.25, min_y) && has(sym, cx + 1.25, min_y));
    // Exits with no route status take the approach width: 1.25 sw points.
    assert!(has(sym, cx - 1.25, max_y - 1.25) && has(sym, cx + 1.25, max_y - 1.25));
}

// ─── pack options the UK pack does not set ────────────────────────────────────────

/// Values a pack marks as measured or borrowed are quoted as Info diagnostics, and a stub the
/// manual does not specify is flagged.
#[test]
fn provisional_and_uncited_values_raise_info_diagnostics() {
    let (mut pack, fonts) = (uk_pack(), fonts());
    pack.map_symbol.as_mut().unwrap().provisional = Some("points measured".into());
    pack.map_symbol.as_mut().unwrap().stubs_cited = false;
    pack.roundabout.as_mut().unwrap().provisional = Some("cut-out measured".into());

    let full = lay(
        &pack,
        &fonts,
        &map(
            SignKind::MapTypeCrossroads,
            vec![l("Left"), a("Ahead"), r("Right")],
        ),
    );
    assert_eq!(codes(&full), ["MAP_SYMBOL_PROVISIONAL"]);
    assert_eq!(full.diagnostics[0].severity, Severity::Info);
    assert_eq!(full.diagnostics[0].message, "points measured");
    // The UK pack names no "mapType" clause, so the pack's own source is cited.
    assert_eq!(full.diagnostics[0].source, pack.source);

    let stubbed = lay(&pack, &fonts, &map(SignKind::MapTypeTee, vec![l("Left")]));
    assert_eq!(
        codes(&stubbed),
        ["MAP_SYMBOL_PROVISIONAL", "MAP_STUB_UNCITED"]
    );
    assert_eq!(
        stubbed.diagnostics[1].message,
        "A direction without destinations is drawn as a stub, which this manual does not specify; the stub uses \
         2.5 sw from the border and 4 sw above the bottom border"
    );

    let ring = lay(&pack, &fonts, &three_way());
    assert_eq!(
        codes(&ring),
        ["MAP_SYMBOL_PROVISIONAL", "ROUNDABOUT_PROVISIONAL"]
    );
    assert_eq!(ring.diagnostics[1].message, "cut-out measured");

    // A blank note is no note.
    pack.map_symbol.as_mut().unwrap().provisional = Some("  ".into());
    pack.roundabout.as_mut().unwrap().provisional = None;
    assert!(lay(&pack, &fonts, &three_way()).diagnostics.is_empty());
}

/// Uganda App.1 Figures 2-3 options: square joins, the side arm's point a third of the way
/// from the border to the vertical arm ("b/3"), and the 12 sw minimum measured to the point.
#[test]
fn a_side_arm_can_be_pointed_from_the_border() {
    let (mut pack, fonts) = (uk_pack(), fonts());
    {
        let ms = pack.map_symbol.as_mut().unwrap();
        ms.fillet_radius = 0.0;
        ms.side_arm_reach_from = Some("border".into());
        ms.exit_min_to_point = true;
    }
    let layout = lay(
        &pack,
        &fonts,
        &map(SignKind::MapTypeTee, vec![l("Ae"), r("Masheruka")]),
    );
    let sym = symbol(&layout);
    let (min_x, _, max_x, _) = bounds(sym);
    let (xv0, xv1) = foot(sym);

    // Left: "Ae" is short, so b (border to the vertical arm) is widened until two thirds of
    // it is the 12 sw minimum: b = 18, the point 6 sw from the border.
    assert!(close(xv0 - 1.5, 18.0));
    assert!(close(min_x, 1.5 + 6.0));
    // Right: the block is long, so b is the 4 sw gap, the block and the 2.5 sw side pad; the
    // point is a third of b from the right border.
    let b = layout.width - 1.5 - xv1;
    assert!(close(b, 4.0 + tiles(&pack, &fonts, "Masheruka") + 2.5));
    assert!(close(max_x, layout.width - 1.5 - b / 3.0));
    // Square joins: the junction corners are single vertices.
    assert!(has(sym, xv0, 13.5) && has(sym, xv1, 13.5));
    assert_eq!(sym.len(), 2 + 1 + 3 + 3 + 1);
}

/// Uganda App.1 Figure 2 option: side tiles 2.0 sw outside the ring's vertical tangents
/// instead of the 14.5r keep-out circle.
#[test]
fn side_legends_can_be_kept_clear_of_the_ring_tangents() {
    let (mut pack, fonts) = (uk_pack(), fonts());
    {
        let rb = pack.roundabout.as_mut().unwrap();
        rb.legend_keep_out_radius = None;
        rb.legend_to_ring_side = Some(2.0);
    }
    let layout = lay(
        &pack,
        &fonts,
        &map(
            SignKind::MapTypeRoundabout,
            vec![l("Kampala City Centre"), r("Entebbe International Airport")],
        ),
    );
    let sym = symbol(&layout);
    let cx = sym.iter().find(|p| close(p.y, bounds(sym).3)).unwrap().x;
    let right = text(&layout, "Entebbe International Airport");
    assert!(close(right.origin.x - cx, 12.0 + 2.0));
    let left = text(&layout, "Kampala City Centre");
    let near_left = left.origin.x + tiles(&pack, &fonts, "Kampala City Centre");
    assert!(close(cx - near_left, 12.0 + 2.0));
}

/// Para 5.40: a cut-out under the recommended 25° is flagged, as is an arm that reaches into
/// it (the Figure 5-14 adjustment is not modelled).
#[test]
fn a_small_or_blocked_cut_out_is_flagged() {
    let (mut pack, fonts) = (uk_pack(), fonts());
    {
        let rb = pack.roundabout.as_mut().unwrap();
        rb.cut_out_from_approach_deg = Some(5.0);
        rb.cut_out_angle_deg = Some(20.5);
    }
    let layout = lay(&pack, &fonts, &three_way());
    assert_eq!(
        codes(&layout),
        ["ROUNDABOUT_CUT_OUT_TOO_SMALL", "ROUNDABOUT_CUT_OUT_BLOCKED"]
    );
    assert_eq!(
        layout.diagnostics[0].message,
        "Roundabout cut-out is 20.5°; para 5.40 recommends at least 25°"
    );
    assert_eq!(layout.diagnostics[0].severity, Severity::Warning);
    assert_eq!(
        layout.diagnostics[1].message,
        "An arm reaches into the roundabout cut-out (the Figure 5-14 adjustment is not modelled)"
    );
}

// ─── what is not ported ───────────────────────────────────────────────────────────

#[test]
fn a_pack_without_the_chapter_7_symbol_is_unsupported() {
    let (mut pack, fonts) = (uk_pack(), fonts());
    pack.map_symbol = None;
    let err = try_lay(&pack, &fonts, &three_way()).unwrap_err();
    assert!(matches!(err, SignError::Unsupported(_)), "{err}");
    assert!(err.to_string().contains("'mapSymbol'"), "{err}");
}

#[test]
fn a_pack_missing_a_map_section_or_ring_value_is_a_pack_error() {
    let fonts = fonts();
    let mut pack = uk_pack();
    pack.route_arms = None;
    let err = try_lay(&pack, &fonts, &three_way()).unwrap_err();
    assert!(matches!(err, SignError::Pack(_)), "{err}");
    assert!(err.to_string().contains("'routeArms'"), "{err}");

    let mut pack = uk_pack();
    pack.roundabout.as_mut().unwrap().ring_to_border = None;
    let err = try_lay(&pack, &fonts, &three_way()).unwrap_err();
    assert!(matches!(err, SignError::Pack(_)), "{err}");
    // The junction symbol needs none of the ring values.
    assert!(try_lay(&pack, &fonts, &map(SignKind::MapTypeTee, vec![l("Left")])).is_ok());
}

#[test]
fn embedded_signs_are_drawn_over_the_vertical_arm_without_resizing_the_sign() {
    let (pack, fonts) = (uk_pack(), fonts());
    let mut sign = three_way();
    let control = lay(&pack, &fonts, &sign);
    sign.embedded = Some(vec![]);
    assert_eq!(lay(&pack, &fonts, &sign).prims.len(), control.prims.len());

    sign.embedded = Some(vec![EmbeddedSign {
        symbol_id: "height-limit".into(),
        kind: EmbeddedType::RegulatoryRoundel,
    }]);
    let with = lay(&pack, &fonts, &sign);
    assert!(close(with.width, control.width) && close(with.height, control.height));
    // On the green ground: the white edge, the red ring and the white field, drawn last.
    assert_eq!(with.prims.len(), control.prims.len() + 3);
    assert_eq!(with.prims[..control.prims.len()], control.prims[..]);
    let discs: Vec<(&Vec<Pt2>, Colour)> = with.prims[control.prims.len()..]
        .iter()
        .map(|p| match p {
            Prim::Fill { outline, colour } => (outline, *colour),
            other => panic!("not a fill: {other:?}"),
        })
        .collect();
    let colours: Vec<Colour> = discs.iter().map(|d| d.1).collect();
    assert_eq!(colours, vec![Colour::White, Colour::Red, Colour::White]);

    // Centred on the vertical arm (its apex is the symbol's highest point), between the top of
    // the side arms and the foot of the arm's point.
    let apex = with.symbols[0]
        .iter()
        .fold(Pt2::new(0.0, f64::MIN), |m, p| if p.y > m.y { *p } else { m });
    let ring = discs[1].0;
    let (x0, x1) = ring.iter().fold((f64::MAX, f64::MIN), |(a, b), p| (a.min(p.x), b.max(p.x)));
    assert!(close(x1 - x0, 20.0), "{}", x1 - x0);
    assert!(close((x0 + x1) / 2.0, apex.x));
    assert!(ring[0].y < apex.y);

    // A pack without the frame proportions draws none, as in C#.
    let mut bare = uk_pack();
    bare.embedded.embedded_shapes = None;
    assert_eq!(lay(&bare, &fonts, &sign).prims.len(), control.prims.len());
}

#[test]
fn only_the_three_map_kinds_are_map_type() {
    assert!(is_map_type(SignKind::MapTypeCrossroads));
    assert!(is_map_type(SignKind::MapTypeTee));
    assert!(is_map_type(SignKind::MapTypeRoundabout));
    assert!(!is_map_type(SignKind::StackType));
    assert!(!is_map_type(SignKind::DirectionSign));
    assert_eq!(
        family_label(SignKind::MapTypeCrossroads),
        "Map · crossroads"
    );
    assert_eq!(family_label(SignKind::MapTypeTee), "Map · T-intersection");
    assert_eq!(
        family_label(SignKind::MapTypeRoundabout),
        "Map · roundabout"
    );
}
