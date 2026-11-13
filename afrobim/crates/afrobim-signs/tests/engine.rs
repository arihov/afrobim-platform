//! The public API the desktop app codes against, checked by hand-worked dimensions from
//! UK TSM Chapter 7 rather than by the golden fixtures.

use std::path::{Path, PathBuf};

use afrobim_draw::{Colour, Primitive};
use afrobim_signs::{
    Destination, Direction, RouteStatus, SignDefinition, SignEngine, SignError, SignKind,
};

fn repo_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../..")
}

fn engine() -> SignEngine {
    let root = repo_root();
    SignEngine::load(&root.join("packs/UK-TSM-2019.json"), &root.join("fonts")).unwrap()
}

fn close(a: f64, b: f64) -> bool {
    (a - b).abs() < 1e-6
}

fn bounds(outline: &[afrobim_draw::Pt2]) -> (f64, f64, f64, f64) {
    outline.iter().fold(
        (f64::MAX, f64::MAX, f64::MIN, f64::MIN),
        |(x0, y0, x1, y1), p| (x0.min(p.x), y0.min(p.y), x1.max(p.x), y1.max(p.y)),
    )
}

#[test]
fn the_engine_names_its_pack() {
    let e = engine();
    assert_eq!(e.pack_id(), "UK-TSM-2019");
    assert_eq!(e.pack_name(), "UK Traffic Signs Manual (Chapter 7)");
}

/// The app's seed sign, worked by hand.
///
/// Ahead section: "Kasese" is 26.44 sw of Appendix C tiles on one line, so the vertical arrow
/// is the 14 sw one (§4.2.2) and sets the height: 2.5 + 14 + 2.5 = 19. Right section:
/// "Masheruka" is 41.08 sw; the 16 sw arrow lies across (8 sw), so the height is
/// max(2.5 + 8 + 1.5, 2.5 + 8 + 2.5) = 13 and the width 2.5 + 16 + 2.5 + 41.08 + 2.5 = 64.58.
/// Sign: 64.58 + 2 × 1.5 border = 67.58 wide; 19 + 1 divider + 13 + 2 × 1.5 = 36 high.
#[test]
fn the_seed_sign_has_the_chapter_7_dimensions() {
    let sign = SignDefinition::stack(
        "ADS-001",
        60,
        vec![
            Destination::new("Kasese", Direction::Ahead),
            Destination::new("Masheruka", Direction::Right),
        ],
    );
    let layout = engine().layout(&sign).unwrap();

    assert_eq!((layout.sw_mm, layout.x_height_mm), (31.25, 125.0));
    assert!(layout.diagnostics.is_empty(), "{:?}", layout.diagnostics);
    assert!(!layout.has_errors());
    let ext = layout.drawing.extents;
    assert!(close(ext.min.x, 0.0) && close(ext.min.y, 0.0));
    assert!(
        close(ext.width(), 67.58) && close(ext.height(), 36.0),
        "{ext:?}"
    );

    let prims = &layout.drawing.primitives;
    let kinds: Vec<&str> = prims
        .iter()
        .map(|p| match p {
            Primitive::Fill { .. } => "fill",
            Primitive::Glyphs { .. } => "glyphs",
            _ => "other",
        })
        .collect();
    // Face in the border colour, then per section: ground, arrow, legend.
    assert_eq!(
        kinds,
        ["fill", "fill", "fill", "glyphs", "fill", "fill", "glyphs"]
    );

    let fill = |i: usize| match &prims[i] {
        Primitive::Fill { outline, colour } => (bounds(outline), *colour),
        other => panic!("{other:?}"),
    };
    // Border and divider show white (the lettering colour) around the green sections.
    assert_eq!(fill(0).1, Colour::White);
    let ((x0, y0, x1, y1), ground) = fill(1);
    assert_eq!(ground, Colour::Green);
    assert!(close(x0, 1.5) && close(x1, 66.08) && close(y1, 34.5) && close(y0, 15.5));
    let ((_, y0, _, y1), _) = fill(4);
    assert!(close(y1, 14.5) && close(y0, 1.5)); // 1 sw divider below the ahead section

    // Ahead arrow: 8 wide, 14 long, 2.5 from the left border and centred in its section.
    let ((x0, y0, x1, y1), arm) = fill(2);
    assert_eq!(arm, Colour::White);
    assert!(close(x0, 4.0) && close(x1, 12.0) && close(y0, 18.0) && close(y1, 32.0));
    // Right arrow: 16 long, 2.5 from the right border, pointing right.
    let ((x0, y0, x1, y1), _) = fill(5);
    assert!(close(x1, 63.58) && close(x0, 47.58) && close(y0, 4.0) && close(y1, 12.0));

    // Legends: 2.5 sw from their arrow; the right-hand one ends 2.5 sw short of its arrow.
    match (&prims[3], &prims[6]) {
        (
            Primitive::Glyphs {
                text: a,
                ink: ia,
                contours: ca,
            },
            Primitive::Glyphs {
                text: b,
                contours: cb,
                ..
            },
        ) => {
            assert_eq!((a.as_str(), b.as_str()), ("Kasese", "Masheruka"));
            assert_eq!(*ia, Some(Colour::White));
            let (ax0, ..) = bounds(&ca.concat());
            let (_, _, bx1, _) = bounds(&cb.concat());
            // Ink sits inside its tiles: tiles start at 14.5 and end at 45.08.
            assert!(ax0 > 14.5 && ax0 < 15.5, "{ax0}");
            assert!(bx1 < 45.08 && bx1 > 44.0, "{bx1}");
        }
        other => panic!("{other:?}"),
    }
}

/// A white (non-primary) sign is black Transport Heavy with a black border (§2.1.1).
#[test]
fn a_white_sign_is_lettered_in_black() {
    let mut sign =
        SignDefinition::stack("W", 50, vec![Destination::new("Kyenjojo", Direction::Left)]);
    sign.background = Colour::White;
    let layout = engine().layout(&sign).unwrap();
    assert_eq!((layout.sw_mm, layout.x_height_mm), (31.25, 125.0));
    let colours: Vec<Option<Colour>> = layout
        .drawing
        .primitives
        .iter()
        .map(|p| match p {
            Primitive::Fill { colour, .. } => Some(*colour),
            Primitive::Glyphs { ink, .. } => *ink,
            _ => None,
        })
        .collect();
    assert_eq!(
        colours,
        [
            Some(Colour::Black),
            Some(Colour::White),
            Some(Colour::Black),
            Some(Colour::Black)
        ]
    );
}

/// §3.2.3: a motorway number on a primary (green) sign stands on a blue patch with a 0.5 sw
/// white border (§3.3.1); a primary number on green is plain yellow tiles (§2.1.1).
#[test]
fn route_numbers_get_patches_and_inks_by_status() {
    let sign = SignDefinition::stack(
        "R",
        60,
        vec![
            Destination::new("Gulu", Direction::Ahead).with_route("(M11)", RouteStatus::Primary),
            Destination::new("Lira", Direction::Left).with_route("A11", RouteStatus::Primary),
        ],
    );
    let layout = engine().layout(&sign).unwrap();
    let prims = &layout.drawing.primitives;
    let blue = prims.iter().find_map(|p| match p {
        Primitive::Fill {
            outline,
            colour: Colour::Blue,
        } => Some(bounds(outline)),
        _ => None,
    });
    let (_, y0, _, y1) = blue.expect("a blue patch");
    assert!(close(y1 - y0, 9.5)); // Figure 3-11: 1.5 rise + 8 tile
    let yellow: Vec<&str> = prims
        .iter()
        .filter_map(|p| match p {
            Primitive::Glyphs {
                text,
                ink: Some(Colour::Yellow),
                ..
            } => Some(text.as_str()),
            _ => None,
        })
        .collect();
    assert_eq!(yellow, ["A", "11"]); // Figure 3-9: letter and number are separate tile groups
}

#[test]
fn other_sign_kinds_are_unsupported() {
    let mut sign =
        SignDefinition::stack("M", 60, vec![Destination::new("Kasese", Direction::Ahead)]);
    sign.kind = SignKind::WarningSign;
    let err = engine().layout(&sign).unwrap_err();
    assert!(matches!(err, SignError::Unsupported(_)), "{err}");
    assert!(err.to_string().contains("WarningSign"), "{err}");
}

#[test]
fn a_symbol_drawn_from_the_unported_artwork_library_is_unsupported_not_faked() {
    let mut d = Destination::new("Entebbe", Direction::Ahead);
    d.symbol_id = Some("Aircraft".into());
    let err = engine()
        .layout(&SignDefinition::stack("S", 60, vec![d]))
        .unwrap_err();
    assert!(matches!(err, SignError::Unsupported(_)), "{err}");
}

#[test]
fn a_character_the_font_cannot_draw_is_an_error() {
    let sign = SignDefinition::stack(
        "G",
        60,
        vec![Destination::new("Kasese \u{4E2D}", Direction::Ahead)],
    );
    let err = engine().layout(&sign).unwrap_err();
    assert!(
        matches!(
            err,
            SignError::MissingGlyph {
                character: '\u{4E2D}',
                ..
            }
        ),
        "{err}"
    );
}

#[test]
fn loading_reports_a_missing_pack_or_font() {
    let root = repo_root();
    assert!(matches!(
        SignEngine::load(&root.join("packs/none.json"), &root.join("fonts")),
        Err(SignError::Pack(_))
    ));
    assert!(matches!(
        SignEngine::load(&root.join("packs/UK-TSM-2019.json"), &root.join("packs")),
        Err(SignError::Font(_))
    ));
}

#[test]
fn the_sign_scales_to_millimetres_through_the_drawing() {
    let sign = SignDefinition::stack(
        "ADS",
        100,
        vec![Destination::new("Kampala", Direction::Ahead)],
    );
    let layout = engine().layout(&sign).unwrap();
    assert_eq!(layout.sw_mm, 62.5); // 250 mm x-height at 100 km/h
    let mm = layout.drawing.scaled_to_origin(layout.sw_mm);
    assert!(close(
        mm.extents.height(),
        layout.drawing.extents.height() * 62.5
    ));
}
