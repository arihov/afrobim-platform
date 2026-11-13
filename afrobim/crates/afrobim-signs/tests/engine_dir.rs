//! The destination-column family through the public API: direction, confirmatory, place
//! identification, lane pre-selection and diversion signs, checked by hand-worked dimensions
//! rather than by the golden fixtures.
//!
//! Tile widths used below (UK TSM Chapter 7 Appendix C, Transport Medium): "Kasese" 26.44 sw,
//! "Masheruka" 41.08 sw. UK pack: border 1.5 sw, legend 2.5 sw from the top and side borders
//! and 1.5 sw from the bottom, tile height 8 sw, word space 2.5 sw.

use std::path::{Path, PathBuf};

use afrobim_draw::{Colour, Primitive, Pt2};
use afrobim_signs::{
    ColourScheme, Destination, Direction, EmbeddedSign, EmbeddedType, RouteStatus, SignDefinition,
    SignEngine, SignError, SignKind, SignLayout, SignVariant,
};

fn repo_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../..")
}

fn engine() -> SignEngine {
    let root = repo_root();
    SignEngine::load(&root.join("packs/UK-TSM-2019.json"), &root.join("fonts")).unwrap()
}

/// The UK pack with one piece of its text replaced, loaded from a temporary file.
fn engine_with(name: &str, from: &str, to: &str) -> SignEngine {
    let root = repo_root();
    let text = std::fs::read_to_string(root.join("packs/UK-TSM-2019.json")).unwrap();
    assert!(text.contains(from), "the UK pack has no {from}");
    let path =
        std::env::temp_dir().join(format!("afrobim-signs-{}-{name}.json", std::process::id()));
    std::fs::write(&path, text.replacen(from, to, 1)).unwrap();
    let engine = SignEngine::load(&path, &root.join("fonts"));
    std::fs::remove_file(&path).ok();
    engine.unwrap()
}

fn sign(kind: SignKind, speed_kph: u32, destinations: Vec<Destination>) -> SignDefinition {
    SignDefinition {
        kind,
        ..SignDefinition::stack("S-1", speed_kph, destinations)
    }
}

fn close(a: f64, b: f64) -> bool {
    (a - b).abs() < 1e-6
}

fn bounds(outline: &[Pt2]) -> (f64, f64, f64, f64) {
    outline.iter().fold(
        (f64::MAX, f64::MAX, f64::MIN, f64::MIN),
        |(x0, y0, x1, y1), p| (x0.min(p.x), y0.min(p.y), x1.max(p.x), y1.max(p.y)),
    )
}

fn fills(layout: &SignLayout) -> Vec<(&[Pt2], Colour)> {
    layout
        .drawing
        .primitives
        .iter()
        .filter_map(|p| match p {
            Primitive::Fill { outline, colour } => Some((outline.as_slice(), *colour)),
            _ => None,
        })
        .collect()
}

fn lines(layout: &SignLayout) -> Vec<(Pt2, Pt2, f64, Option<Colour>)> {
    layout
        .drawing
        .primitives
        .iter()
        .filter_map(|p| match p {
            Primitive::Line {
                from,
                to,
                width,
                colour,
            } => Some((*from, *to, *width, *colour)),
            _ => None,
        })
        .collect()
}

fn legends(layout: &SignLayout) -> Vec<&str> {
    layout
        .drawing
        .primitives
        .iter()
        .filter_map(|p| match p {
            Primitive::Glyphs { text, .. } => Some(text.as_str()),
            _ => None,
        })
        .collect()
}

fn glyph_bounds(layout: &SignLayout, legend: &str) -> (f64, f64, f64, f64) {
    let contours = layout
        .drawing
        .primitives
        .iter()
        .find_map(|p| match p {
            Primitive::Glyphs { text, contours, .. } if text == legend => Some(contours),
            _ => None,
        })
        .unwrap_or_else(|| panic!("no legend '{legend}'"));
    let all: Vec<Pt2> = contours.iter().flatten().copied().collect();
    bounds(&all)
}

fn codes(layout: &SignLayout) -> Vec<&str> {
    layout.diagnostics.iter().map(|d| d.code.as_str()).collect()
}

// ─── direction ────────────────────────────────────────────────────────────────

/// UK TSM Chapter 7 §7.11 / Figure 7-4, one line. The 14 sw arrow (§4.2.2) sets the height:
/// 1.5 + 2.5 + 14 + 2.5 + 1.5 = 22. Width: 1.5 + 2.5 + 8 (arrow) + 2.5 + 26.44 + 2.5 + 1.5 =
/// 44.94. Of the 19 - 8 = 11 sw to spare inside the borders, 6 go above the 8 sw tile and 5
/// below (Figure 7-4 "H + 1": the gap above a tiled legend is 1 sw more than the gap below),
/// so the tile spans y = 6.5 to 14.5.
#[test]
fn a_one_line_direction_sign_is_a_stack_section_with_the_short_arrow() {
    let layout = engine()
        .layout(&sign(
            SignKind::DirectionSign,
            60,
            vec![Destination::new("Kasese", Direction::Ahead)],
        ))
        .unwrap();

    // Appendix E column 7 (direction signs): 100 mm x-height at 60 km/h, not the 125 mm of an
    // advance direction sign.
    assert_eq!((layout.sw_mm, layout.x_height_mm), (25.0, 100.0));
    assert!(layout.diagnostics.is_empty(), "{:?}", layout.diagnostics);
    let ext = layout.drawing.extents;
    assert!(close(ext.max.x, 44.94) && close(ext.max.y, 22.0), "{ext:?}");

    // The §2.15 border: the plate in the legend colour, the ground inset by the border.
    let fills = fills(&layout);
    assert_eq!(fills.len(), 3);
    assert_eq!(fills[0].1, Colour::White);
    let (x0, y0, x1, y1) = bounds(fills[0].0);
    assert!(close(x0, 0.0) && close(y0, 0.0) && close(x1, 44.94) && close(y1, 22.0));
    assert_eq!(fills[1].1, Colour::Green);
    let (x0, y0, x1, y1) = bounds(fills[1].0);
    assert!(close(x0, 1.5) && close(y0, 1.5) && close(x1, 43.44) && close(y1, 20.5));

    // The Figure 4-4 arrow as one filled outline, pointing ahead: 8 across, 14 long, 2.5 sw
    // from the left border and from the top and bottom borders.
    let (arrow, arrow_colour) = fills[2];
    assert_eq!((arrow.len(), arrow_colour), (9, Colour::White));
    let (x0, y0, x1, y1) = bounds(arrow);
    assert!(close(x0, 4.0) && close(x1, 12.0) && close(y0, 4.0) && close(y1, 18.0));
    assert!(lines(&layout).is_empty());

    // The legend starts 2.5 sw right of the arrow; its letters stand on the baseline 2 sw above
    // the tile foot (8.5; round letters dip a little below it) and stay inside the tile.
    assert_eq!(legends(&layout), ["Kasese"]);
    let (gx0, gy0, _, gy1) = glyph_bounds(&layout, "Kasese");
    assert!((14.5..15.5).contains(&gx0), "{gx0}");
    assert!((8.0..=8.5).contains(&gy0) && gy1 < 14.5, "{gy0} {gy1}");
}

/// The arrow points ahead whatever the destinations' directions, nearer destinations come
/// first, and distances and symbols are not laid out.
#[test]
fn a_direction_sign_stacks_nearest_first_under_one_ahead_arrow() {
    let mut far = Destination::new("Masheruka", Direction::Right)
        .with_order(1)
        .with_distance("12");
    far.symbol_id = Some("parking-p".into());
    let layout = engine()
        .layout(&sign(
            SignKind::DirectionSign,
            60,
            vec![far, Destination::new("Kasese", Direction::Left)],
        ))
        .unwrap();

    assert_eq!(legends(&layout), ["Kasese", "Masheruka"]);
    // Two lines need 2.5 + 8 + 8 + 1.5 = 20 inside the borders; the full 16 sw arrow with
    // 2.5 sw above and below needs 21 and sets the height.
    let ext = layout.drawing.extents;
    assert!(close(ext.max.y, 24.0), "{ext:?}");
    assert!(
        close(ext.max.x, 1.5 + 2.5 + 8.0 + 2.5 + 41.08 + 2.5 + 1.5),
        "{ext:?}"
    );
    let fills = fills(&layout);
    let (_, y0, _, y1) = bounds(fills[2].0);
    assert!(close(y1 - y0, 16.0) && close((y0 + y1) / 2.0, 12.0));
}

#[test]
fn a_direction_sign_draws_route_numbers_with_the_stack_type_rules() {
    let layout = engine()
        .layout(&sign(
            SignKind::DirectionSign,
            60,
            vec![
                Destination::new("Kasese", Direction::Ahead).with_route("M5", RouteStatus::Primary)
            ],
        ))
        .unwrap();
    assert!(layout.diagnostics.is_empty(), "{:?}", layout.diagnostics);
    // A motorway number on a green sign stands on a blue patch (§3.2.3).
    assert!(fills(&layout).iter().any(|(_, c)| *c == Colour::Blue));
    assert_eq!(legends(&layout), ["Kasese", "M", "5"]);
}

#[test]
fn a_direction_sign_without_destinations_is_the_plate_alone() {
    let layout = engine()
        .layout(&sign(SignKind::DirectionSign, 60, vec![]))
        .unwrap();
    let ext = layout.drawing.extents;
    assert!(close(ext.max.x, 8.0) && close(ext.max.y, 7.0), "{ext:?}");
    assert_eq!(layout.drawing.primitives.len(), 2);
}

#[test]
fn embedded_signs_on_a_direction_sign_are_unsupported_not_dropped() {
    let mut s = sign(
        SignKind::DirectionSign,
        60,
        vec![Destination::new("Kasese", Direction::Ahead)],
    );
    s.embedded = Some(vec![EmbeddedSign {
        symbol_id: "height-limit".into(),
        kind: EmbeddedType::RegulatoryRoundel,
    }]);
    let err = engine().layout(&s).unwrap_err();
    assert!(matches!(err, SignError::Unsupported(_)), "{err}");
    assert!(err.to_string().contains("embedded"), "{err}");
}

#[test]
fn a_pack_without_stack_rules_has_no_ported_direction_layout() {
    let engine = engine_with("no-stack", "\"stack\":", "\"stackNotRead\":");
    let err = engine
        .layout(&sign(
            SignKind::DirectionSign,
            60,
            vec![Destination::new("Kasese", Direction::Ahead)],
        ))
        .unwrap_err();
    assert!(matches!(err, SignError::Unsupported(_)), "{err}");
    // The no-destination case does not need them.
    assert!(engine
        .layout(&sign(SignKind::DirectionSign, 60, vec![]))
        .is_ok());
}

// ─── confirmatory ─────────────────────────────────────────────────────────────

/// §3.9.1–3.9.2: one centred distance column, 7 sw clear of the longest name. Two tiled lines:
/// height 1.5 + 2.5 + 8 + 8 + 1.5 + 1.5 = 23. No arrow.
#[test]
fn a_confirmatory_sign_sets_distances_in_one_centred_column() {
    let layout = engine()
        .layout(&sign(
            SignKind::Confirmatory,
            100,
            vec![
                Destination::new("Kasese", Direction::Ahead).with_distance("7"),
                Destination::new("Masheruka", Direction::Ahead)
                    .with_order(1)
                    .with_distance("134"),
            ],
        ))
        .unwrap();

    // Appendix E column 9 (route confirmatory signs) at 100 km/h.
    assert_eq!((layout.sw_mm, layout.x_height_mm), (50.0, 200.0));
    assert!(layout.diagnostics.is_empty(), "{:?}", layout.diagnostics);
    assert_eq!(legends(&layout), ["Kasese", "Masheruka", "7", "134"]);
    assert_eq!(fills(&layout).len(), 2); // the bordered plate, nothing else
    assert!(close(layout.drawing.extents.max.y, 23.0));

    // The column starts 7 sw right of "Masheruka" (4 + 41.08 = 45.08), so "134" starts at
    // 52.08 and the sign ends 2.5 + 1.5 right of it; "7" is centred on "134".
    let width = layout.drawing.extents.max.x;
    let (x134, _, _, _) = glyph_bounds(&layout, "134");
    assert!((52.08..53.5).contains(&x134), "{x134}");
    let column_centre = (52.08 + (width - 4.0)) / 2.0;
    let (x7_0, _, x7_1, _) = glyph_bounds(&layout, "7");
    assert!(((x7_0 + x7_1) / 2.0 - column_centre).abs() < 0.6);
}

#[test]
fn a_confirmatory_sign_without_distances_is_flagged() {
    let layout = engine()
        .layout(&sign(
            SignKind::Confirmatory,
            100,
            vec![Destination::new("Kasese", Direction::Ahead)],
        ))
        .unwrap();
    assert_eq!(codes(&layout), ["CONFIRMATORY_NO_DISTANCE"]);
    assert_eq!(layout.diagnostics[0].source, "UK TSM Chapter 7 §8");
    assert!(!layout.has_errors());
    // 1.5 + 2.5 + 26.44 + 2.5 + 1.5 wide; 1.5 + 2.5 + 8 + 1.5 + 1.5 high.
    let ext = layout.drawing.extents;
    assert!(close(ext.max.x, 34.44) && close(ext.max.y, 15.0), "{ext:?}");
}

#[test]
fn a_confirmatory_sign_without_destinations_is_the_plate_alone() {
    let layout = engine()
        .layout(&sign(SignKind::Confirmatory, 100, vec![]))
        .unwrap();
    assert!(layout.diagnostics.is_empty());
    let ext = layout.drawing.extents;
    assert!(close(ext.max.x, 8.0) && close(ext.max.y, 7.0), "{ext:?}");
}

#[test]
fn a_pack_without_block_rules_has_no_ported_confirmatory_layout() {
    let engine = engine_with("no-blocks", "\"blocks\":", "\"blocksNotRead\":");
    let err = engine
        .layout(&sign(
            SignKind::Confirmatory,
            100,
            vec![Destination::new("Kasese", Direction::Ahead).with_distance("7")],
        ))
        .unwrap_err();
    assert!(matches!(err, SignError::Unsupported(_)), "{err}");
}

#[test]
fn a_character_the_font_cannot_draw_is_an_error_on_every_kind() {
    for kind in [
        SignKind::DirectionSign,
        SignKind::Confirmatory,
        SignKind::PlaceIdentification,
        SignKind::LanePreselection,
        SignKind::Diversion,
    ] {
        let err = engine()
            .layout(&sign(
                kind,
                60,
                vec![Destination::new("Kasese \u{4e2d}", Direction::Ahead)],
            ))
            .unwrap_err();
        assert!(
            matches!(
                err,
                SignError::MissingGlyph {
                    character: '\u{4e2d}',
                    ..
                }
            ),
            "{kind:?}: {err}"
        );
    }
}

// ─── place identification ─────────────────────────────────────────────────────

/// Entry: a plain 1.5 + 2.5 + 26.44 + 2.5 + 1.5 by 1.5 + 2.5 + 8 + 1.5 + 1.5 panel with the name
/// centred, black Transport Heavy on white. No drawn border, no band.
#[test]
fn a_place_entry_sign_is_the_name_centred_on_a_plain_panel() {
    let mut s = sign(
        SignKind::PlaceIdentification,
        100,
        vec![Destination::new("Kampala", Direction::Ahead)],
    );
    s.background = Colour::White;
    s.variant = SignVariant::Primary;
    let layout = engine().layout(&s).unwrap();

    assert!(layout.diagnostics.is_empty(), "{:?}", layout.diagnostics);
    let ext = layout.drawing.extents;
    assert!(close(ext.max.y, 15.0), "{ext:?}");
    let fills = fills(&layout);
    assert_eq!(fills.len(), 1);
    assert_eq!((fills[0].0.len(), fills[0].1), (4, Colour::White));
    assert!(matches!(
        &layout.drawing.primitives[1],
        Primitive::Glyphs { text, ink: Some(Colour::Black), .. } if text == "Kampala"
    ));
    // Centred: the ink is as far from the left edge as from the right, within the difference
    // of the first and last letters' side bearings.
    let (gx0, _, gx1, _) = glyph_bounds(&layout, "Kampala");
    assert!((gx0 - (ext.max.x - gx1)).abs() < 1.0, "{gx0} {gx1}");
}

/// Exit (the default variant): a grey band two border widths wide from the top-right of the
/// interior to its bottom-left, under the names.
#[test]
fn a_place_exit_sign_adds_the_grey_band() {
    let s = sign(
        SignKind::PlaceIdentification,
        60,
        vec![
            Destination::new("Kasese", Direction::Ahead),
            Destination::new("Masheruka", Direction::Ahead).with_order(1),
        ],
    );
    assert_eq!(s.variant, SignVariant::Secondary);
    let layout = engine().layout(&s).unwrap();

    // 1.5 + 2.5 + 41.08 + 2.5 + 1.5 wide; two tiles: 1.5 + 2.5 + 16 + 1.5 + 1.5 high.
    let ext = layout.drawing.extents;
    assert!(close(ext.max.x, 49.08) && close(ext.max.y, 23.0), "{ext:?}");
    let fills = fills(&layout);
    assert_eq!(fills.len(), 2);
    let (band, colour) = fills[1];
    assert_eq!(colour, Colour::Grey);
    let expected = [(47.58, 21.5), (44.58, 21.5), (1.5, 1.5), (4.5, 1.5)];
    assert_eq!(band.len(), 4);
    for (p, (x, y)) in band.iter().zip(expected) {
        assert!(close(p.x, x) && close(p.y, y), "{p:?} vs ({x}, {y})");
    }
    assert_eq!(legends(&layout), ["Kasese", "Masheruka"]);
}

#[test]
fn route_numbers_on_a_place_sign_are_flagged_as_not_drawn() {
    let layout = engine()
        .layout(&sign(
            SignKind::PlaceIdentification,
            60,
            vec![Destination::new("Kasese", Direction::Ahead)
                .with_route("A109", RouteStatus::Primary)],
        ))
        .unwrap();
    assert_eq!(codes(&layout), ["ROUTE_NUMBER_NOT_DRAWN"]);
    assert!(layout.diagnostics[0]
        .message
        .contains("Place identification signs"));
    assert_eq!(legends(&layout), ["Kasese"]);
}

// ─── lane pre-selection ───────────────────────────────────────────────────────

/// One column per lane, left to right by order, each 2.5 + max(8, name) + 2.5 wide. Height:
/// 1.5 + 2.5 + 12.5 (arrow band, routeArms.minLengthVertical) + 2.5 + 8 + 1.5 + 1.5 = 30.
#[test]
fn lanes_are_columns_with_an_arrow_over_each_name() {
    let layout = engine()
        .layout(&sign(
            SignKind::LanePreselection,
            80,
            vec![
                Destination::new("Masheruka", Direction::Right).with_order(2),
                Destination::new("Kasese", Direction::Left),
                Destination::new("A", Direction::Ahead).with_order(1),
            ],
        ))
        .unwrap();

    assert!(layout.diagnostics.is_empty(), "{:?}", layout.diagnostics);
    assert_eq!(legends(&layout), ["Kasese", "A", "Masheruka"]);
    let ext = layout.drawing.extents;
    // "A" (5.44 sw) is narrower than the 8 sw arrow, so its lane is 2.5 + 8 + 2.5 wide.
    let (w1, w2, w3) = (31.44, 13.0, 46.08);
    assert!(
        close(ext.max.x, 3.0 + w1 + w2 + w3) && close(ext.max.y, 30.0),
        "{ext:?}"
    );

    let lines = lines(&layout);
    // Three shafts and the two lane-divide lines; the divides are a border width wide and
    // span the interior between the pads.
    assert_eq!(lines.len(), 5);
    let divides: Vec<_> = lines.iter().filter(|l| close(l.2, 1.5)).collect();
    assert_eq!(divides.len(), 2);
    assert!(close(divides[0].0.x, 1.5 + w1) && close(divides[1].0.x, 1.5 + w1 + w2));
    for d in &divides {
        assert!(close(d.0.y, 3.0) && close(d.1.y, 26.0) && d.3 == Some(Colour::White));
    }

    // The ahead arrow of the middle lane: an 8/3 sw shaft up the lane centre from the foot of
    // the band (13.5) to the back of its head, a triangle 8 sw across and 16/3 sw long with
    // its tip at the top of the interior (26).
    let cx = 1.5 + w1 + w2 / 2.0;
    let shaft = lines
        .iter()
        .find(|l| close(l.0.x, cx) && close(l.1.x, cx))
        .unwrap();
    assert!(close(shaft.2, 8.0 / 3.0));
    assert!(close(shaft.0.y, 13.5) && close(shaft.1.y, 26.0 - 16.0 / 3.0));
    let heads: Vec<_> = fills(&layout)
        .into_iter()
        .filter(|(o, _)| o.len() == 3)
        .collect();
    assert_eq!(heads.len(), 3);
    let (x0, y0, x1, y1) = bounds(heads[1].0);
    assert!(close(x0, cx - 4.0) && close(x1, cx + 4.0));
    assert!(close(y1, 26.0) && close(y0, 26.0 - 16.0 / 3.0));

    // The left-turn arrow of the first lane lies across the middle of the band, its tip at
    // the left of the 8 sw cell.
    let (x0, y0, x1, y1) = bounds(heads[0].0);
    let cx1 = 1.5 + w1 / 2.0;
    assert!(close(x0, cx1 - 4.0) && close(x1, cx1 - 4.0 + 16.0 / 3.0));
    assert!(close((y0 + y1) / 2.0, 19.75));
}

#[test]
fn a_lane_sign_without_lanes_is_the_bare_panel() {
    let layout = engine()
        .layout(&sign(SignKind::LanePreselection, 80, vec![]))
        .unwrap();
    let ext = layout.drawing.extents;
    assert!(close(ext.max.x, 3.0) && close(ext.max.y, 15.0), "{ext:?}");
    assert_eq!(layout.drawing.primitives.len(), 1);
}

#[test]
fn lane_and_diversion_arrows_need_a_ported_route_arm_head_and_route_arm_rules() {
    let lane = sign(
        SignKind::LanePreselection,
        80,
        vec![Destination::new("Kasese", Direction::Ahead)],
    );
    let diversion = SignDefinition {
        kind: SignKind::Diversion,
        ..lane.clone()
    };

    // A pack with Figure 5 (flared) arrows draws the barbed head, which is not ported.
    let flared = engine_with("flared", "\"style\": \"chevron\"", "\"style\": \"flared\"");
    for s in [&lane, &diversion] {
        let err = flared.layout(s).unwrap_err();
        assert!(matches!(err, SignError::Unsupported(_)), "{err}");
    }

    // The lane arrow band is routeArms.minLengthVertical; the C# loader requires the section.
    let no_arms = engine_with("no-arms", "\"routeArms\":", "\"routeArmsNotRead\":");
    let err = no_arms.layout(&lane).unwrap_err();
    assert!(matches!(err, SignError::Pack(_)), "{err}");
    assert!(err.to_string().contains("routeArms"), "{err}");
    assert!(no_arms.layout(&diversion).is_ok());
}

// ─── diversion ────────────────────────────────────────────────────────────────

/// Straight on, then right, then left, each row an arrow in an 8 sw column 2.5 sw left of the
/// name. The temporary scheme: yellow ground, black Transport Heavy.
#[test]
fn a_diversion_sign_lists_ahead_right_left_with_an_arrow_per_row() {
    let mut s = sign(
        SignKind::Diversion,
        60,
        vec![
            Destination::new("West", Direction::Left),
            Destination::new("East", Direction::Right),
            Destination::new("North", Direction::Ahead),
        ],
    );
    s.background = Colour::Yellow;
    s.scheme = ColourScheme::Temporary;
    let layout = engine().layout(&s).unwrap();

    assert!(layout.diagnostics.is_empty(), "{:?}", layout.diagnostics);
    assert_eq!(legends(&layout), ["North", "East", "West"]);
    assert!(close(
        layout.drawing.extents.max.y,
        1.5 + 2.5 + 24.0 + 1.5 + 1.5
    ));

    let fills = fills(&layout);
    assert_eq!((fills[0].0.len(), fills[0].1), (4, Colour::Yellow));
    assert!(layout
        .drawing
        .primitives
        .iter()
        .all(|p| !matches!(p, Primitive::Glyphs { ink, .. } if *ink != Some(Colour::Black))));

    // Rows are 8 sw tiles from y = 27 down. The ahead shaft climbs the first row at the
    // centre of the arrow column (x = 4 + 4); the turn shafts lie along the row middles.
    let lines = lines(&layout);
    assert_eq!(lines.len(), 3);
    assert!(lines
        .iter()
        .all(|l| close(l.2, 8.0 / 3.0) && l.3 == Some(Colour::Black)));
    assert!(close(lines[0].0.x, 8.0) && close(lines[0].0.y, 19.0));
    assert!(close(lines[0].1.y, 27.0 - 16.0 / 3.0));
    // Right: from the left of the column to the back of the head at its right.
    assert!(close(lines[1].0.x, 4.0) && close(lines[1].1.x, 12.0 - 16.0 / 3.0));
    assert!(close(lines[1].0.y, 15.0) && close(lines[1].1.y, 15.0));
    // Left: mirrored.
    assert!(close(lines[2].0.x, 12.0) && close(lines[2].1.x, 4.0 + 16.0 / 3.0));
    assert!(close(lines[2].0.y, 7.0));

    let heads: Vec<_> = fills.iter().filter(|(o, _)| o.len() == 3).collect();
    assert_eq!(heads.len(), 3);
    assert!(close(heads[0].0[0].x, 8.0) && close(heads[0].0[0].y, 27.0)); // tips
    assert!(close(heads[1].0[0].x, 12.0) && close(heads[1].0[0].y, 15.0));
    assert!(close(heads[2].0[0].x, 4.0) && close(heads[2].0[0].y, 7.0));
}

/// Without a scheme the sign keeps its background and the legacy lettering rule: white
/// Transport Medium on any ground that is not white.
#[test]
fn a_diversion_sign_without_a_scheme_takes_the_legacy_lettering_rule() {
    let mut s = sign(
        SignKind::Diversion,
        60,
        vec![Destination::new("Kasese", Direction::Ahead)],
    );
    s.background = Colour::Yellow;
    let layout = engine().layout(&s).unwrap();
    assert!(matches!(
        &layout.drawing.primitives.last().unwrap(),
        Primitive::Glyphs {
            ink: Some(Colour::White),
            ..
        }
    ));
    // 1.5 + 2.5 + 8 + 2.5 + 26.44 + 2.5 + 1.5.
    assert!(close(layout.drawing.extents.max.x, 44.94));
}

/// The design rules of App.1 §2.1.2 apply to every kind of the family.
#[test]
fn the_destination_limits_are_checked_on_every_kind() {
    let seven: Vec<Destination> = (0..7)
        .map(|i| Destination::new(format!("Place {i}"), Direction::Ahead).with_order(i))
        .collect();
    for kind in [
        SignKind::DirectionSign,
        SignKind::Confirmatory,
        SignKind::PlaceIdentification,
        SignKind::LanePreselection,
        SignKind::Diversion,
    ] {
        let layout = engine().layout(&sign(kind, 60, seven.clone())).unwrap();
        assert!(layout.has_errors(), "{kind:?}");
        let codes = codes(&layout);
        assert_eq!(
            codes[..2],
            ["MAX_DESTINATIONS", "MAX_PER_DIRECTION"],
            "{kind:?}"
        );
    }
}
