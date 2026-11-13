//! The standard (catalog) signs through the public API: the sign catalog and the definition
//! each entry seeds (checked against what the C# engine's `CatalogSign.Build` gives, written by
//! `tools/GoldenDump` to `tests/golden/std/catalog.json`), and hand-worked dimensions of the
//! TSRGD signs, their plates, supplementary plates and information signs.
//!
//! The primitive-by-primitive comparison with the C# engine is in `golden.rs` (the `std_*`
//! fixtures); the last tests here check that those fixtures cover what the port claims.

use std::collections::BTreeSet;
use std::path::{Path, PathBuf};

use afrobim_draw::{Colour, Primitive, Pt2};
use afrobim_signs::{
    pictogram_library, CatalogEntry, ColourScheme, Destination, Direction, InformationTemplate,
    SignClass, SignDefinition, SignEngine, SignError, SignKind, SignVariant,
};
use serde::Deserialize;

fn repo_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../..")
}

fn engine() -> SignEngine {
    let root = repo_root();
    SignEngine::load(&root.join("packs/UK-TSM-2019.json"), &root.join("fonts")).unwrap()
}

fn close(a: f64, b: f64) -> bool {
    (a - b).abs() < 1e-9
}

fn bounds(outline: &[Pt2]) -> (f64, f64, f64, f64) {
    outline.iter().fold(
        (f64::MAX, f64::MAX, f64::MIN, f64::MIN),
        |(x0, y0, x1, y1), p| (x0.min(p.x), y0.min(p.y), x1.max(p.x), y1.max(p.y)),
    )
}

fn fill(p: &Primitive) -> (&[Pt2], Colour) {
    match p {
        Primitive::Fill { outline, colour } => (outline, *colour),
        other => panic!("expected a fill, got {other:?}"),
    }
}

fn glyphs(p: &Primitive) -> (&str, Option<Colour>, (f64, f64, f64, f64)) {
    match p {
        Primitive::Glyphs {
            text,
            ink,
            contours,
        } => {
            let all: Vec<Pt2> = contours.iter().flatten().copied().collect();
            (text, *ink, bounds(&all))
        }
        other => panic!("expected glyphs, got {other:?}"),
    }
}

fn entry<'a>(e: &'a SignEngine, key: &str) -> &'a CatalogEntry {
    e.catalog_entry(key)
        .unwrap_or_else(|| panic!("no catalog entry {key}"))
}

// ─── the catalog ──────────────────────────────────────────────────────────────

#[derive(Deserialize)]
struct CatalogFixture {
    pack: String,
    pictogram_ids: Vec<String>,
    entries: Vec<EntryFixture>,
}

#[derive(Deserialize)]
struct EntryFixture {
    key: String,
    name: String,
    family: String,
    series: String,
    scheme: String,
    kind: SignKind,
    class: SignClass,
    symbol_id: Option<String>,
    embedded_symbols: Option<Vec<String>>,
    template: Option<String>,
    diagram: Option<String>,
    citation: Option<String>,
    sizes: Option<SizesFixture>,
    definition: SignDefinition,
    definition_at_100: SignDefinition,
}

#[derive(Deserialize)]
struct SizesFixture {
    dimension: String,
    by_band_mm: Vec<Option<f64>>,
    alternative_by_band_mm: Option<Vec<Option<f64>>>,
    permitted_mm: Option<Vec<f64>>,
    source: String,
}

fn catalog_fixture() -> CatalogFixture {
    let path = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/golden/std/catalog.json");
    let text = std::fs::read_to_string(&path).unwrap_or_else(|e| panic!("{}: {e}", path.display()));
    serde_json::from_str(&text).unwrap_or_else(|e| panic!("{}: {e}", path.display()))
}

/// Every entry, field for field, and the definition it seeds, as the C# engine has them.
#[test]
fn the_catalog_and_its_definitions_match_the_reference_engine() {
    let e = engine();
    let fx = catalog_fixture();
    assert_eq!(e.pack_id(), fx.pack);
    assert_eq!(e.catalog().len(), fx.entries.len());
    assert_eq!(fx.entries.len(), 37);

    for (got, want) in e.catalog().iter().zip(&fx.entries) {
        let key = &want.key;
        assert_eq!(&got.key, key);
        assert_eq!(got.name, want.name, "{key}");
        assert_eq!(got.family, want.family, "{key}");
        assert_eq!(got.series, want.series, "{key}");
        assert_eq!(got.scheme, want.scheme, "{key}");
        assert_eq!(got.kind, want.kind, "{key}");
        assert_eq!(got.class, want.class, "{key}");
        assert_eq!(got.symbol_id, want.symbol_id, "{key}");
        assert_eq!(got.embedded_symbols, want.embedded_symbols, "{key}");
        assert_eq!(got.template, want.template, "{key}");
        assert_eq!(got.diagram, want.diagram, "{key}");
        assert_eq!(got.citation, want.citation, "{key}");
        assert_eq!(got.sizes.is_some(), want.sizes.is_some(), "{key}");
        if let (Some(g), Some(w)) = (&got.sizes, &want.sizes) {
            assert_eq!(g.dimension, w.dimension, "{key}");
            assert_eq!(g.by_band_mm, w.by_band_mm, "{key}");
            assert_eq!(g.alternative_by_band_mm, w.alternative_by_band_mm, "{key}");
            assert_eq!(g.permitted_mm, w.permitted_mm, "{key}");
            assert_eq!(g.source, w.source, "{key}");
        }
        assert_eq!(e.definition_for(got), want.definition, "{key}");
        assert_eq!(
            e.definition_for_speed(got, 100),
            want.definition_at_100,
            "{key}"
        );
    }
}

#[test]
fn the_pictogram_library_lists_the_reference_ids() {
    let fx = catalog_fixture();
    assert_eq!(pictogram_library::SUPPORTED_IDS.to_vec(), fx.pictogram_ids);
}

#[test]
fn a_catalog_entry_is_found_by_key_and_seeds_its_sign() {
    let e = engine();
    assert!(e.catalog_entry("no-such-key").is_none());

    let stack = entry(&e, "uk04.2");
    assert_eq!(stack.key, "UK04.2");
    assert_eq!(stack.name, "Stack-type ADS (S12-5-1 arrows, §4)");
    assert_eq!(
        (stack.kind, stack.class),
        (SignKind::StackType, SignClass::Guidance)
    );
    let d = e.definition_for(stack);
    assert_eq!((d.code.as_str(), d.speed_kph), ("UK04.2", 60));
    assert_eq!(
        (d.scheme, d.background),
        (ColourScheme::Secondary, Colour::White)
    );
    assert_eq!(d.variant, SignVariant::Secondary);
    assert_eq!(d.destinations.len(), 2);
    assert_eq!(d.info_template, InformationTemplate::ServicePanel);

    let bend = entry(&e, "D512");
    assert_eq!(bend.name, "Bend ahead (TSRGD diagram 512)");
    assert_eq!(
        (bend.kind, bend.class),
        (SignKind::WarningSign, SignClass::Warning)
    );
    assert_eq!(bend.diagram.as_deref(), Some("512"));
    assert_eq!(
        bend.citation.as_deref(),
        Some("TSRGD 2016 Schedule 2 Part 2 item 8")
    );
    let sizes = bend.sizes.as_ref().unwrap();
    assert_eq!(sizes.dimension, "height");
    assert_eq!(sizes.by_band_mm[2], Some(750.0));
    let d = e.definition_for(bend);
    assert_eq!(d.symbol_id.as_deref(), Some("512"));
    assert_eq!(d.catalog_key.as_deref(), Some("D512"));
    assert!(d.destinations.is_empty());

    assert_eq!(entry(&e, "D601.1").kind, SignKind::Stop);
    assert_eq!(entry(&e, "D602").kind, SignKind::GiveWay);
    assert_eq!(entry(&e, "D606").kind, SignKind::RegulatoryMandatory);
    assert_eq!(entry(&e, "D616").kind, SignKind::RegulatoryProhibitory);
    assert_eq!(entry(&e, "UK01.1").kind, SignKind::MapTypeRoundabout);

    // TSM Chapter 4 Appendix A bands, in km/h.
    assert_eq!(
        e.sign_size_bands_kph(),
        [Some(32), Some(48), Some(64), Some(80), Some(96), None]
    );
}

/// The 24 TSRGD diagrams lay out from the catalog with real artwork and no diagnostics.
#[test]
fn every_tsrgd_sign_lays_out_from_the_catalog() {
    let e = engine();
    let mut diagrams = 0;
    for entry in e.catalog().iter().filter(|c| c.diagram.is_some()) {
        diagrams += 1;
        let sign = e.definition_for(entry);
        let layout = e
            .layout(&sign)
            .unwrap_or_else(|err| panic!("{}: {err}", entry.key));
        assert!(
            layout.diagnostics.is_empty(),
            "{}: {:?}",
            entry.key,
            layout.diagnostics
        );
        assert!(layout.drawing.primitives.len() >= 3, "{}", entry.key);
        assert!(
            !layout
                .drawing
                .primitives
                .iter()
                .any(|p| matches!(p, Primitive::Symbol { .. })),
            "{}",
            entry.key
        );
        // The body is 50 sw in its nominal dimension, drawn at its tabled size.
        assert_eq!(layout.drawing.extents.height(), 50.0, "{}", entry.key);
        let mm = e.sign_size_mm(&sign).unwrap();
        assert_eq!(layout.sw_mm, mm / 50.0, "{}", entry.key);
        assert_eq!(layout.x_height_mm, 4.0 * layout.sw_mm, "{}", entry.key);
    }
    assert_eq!(diagrams, 24);
}

// ─── TSRGD signs ──────────────────────────────────────────────────────────────

/// Bend ahead at 60 km/h. TSM Chapter 4 Appendix A: 31 to 40 mph is a 750 mm triangle, so
/// 1 sw = 750 / 50 = 15 mm. The body is 50 sw high with 0.075 × 50 = 3.75 sw corners: the
/// sharp triangle is 53.75 high, 53.75 × 2/√3 wide, less 2(√3 − 1) × 3.75 for the two rounded
/// base corners. The white ground is 0.1 × 50 = 5 sw inside the red edges.
#[test]
fn a_warning_sign_is_the_tsrgd_triangle_at_its_appendix_a_size() {
    let e = engine();
    let sign = e.definition_for(entry(&e, "D512"));
    let layout = e.layout(&sign).unwrap();

    assert_eq!((layout.sw_mm, layout.x_height_mm), (15.0, 60.0));
    assert_eq!(e.sign_size_mm(&sign), Some(750.0));
    let width = 53.75 * 2.0 / 3f64.sqrt() - 2.0 * (3f64.sqrt() - 1.0) * 3.75;
    let ext = layout.drawing.extents;
    assert!(close(ext.width(), width) && close(ext.height(), 50.0));

    let prims = &layout.drawing.primitives;
    assert_eq!(prims.len(), 3);
    let (outer, border) = fill(&prims[0]);
    assert_eq!(border, Colour::Red);
    let (x0, y0, x1, y1) = bounds(outer);
    assert!(close(x0, 0.0) && close(y0, 0.0) && close(x1, width) && close(y1, 50.0));
    let (ground, white) = fill(&prims[1]);
    assert_eq!((ground.len(), white), (3, Colour::White));
    assert!(close(ground[0].y, 5.0) && close(ground[2].y, 43.75));
    // The pictogram is black and inside the ground.
    let (symbol, black) = fill(&prims[2]);
    assert_eq!(black, Colour::Black);
    let (sx0, sy0, sx1, sy1) = bounds(symbol);
    assert!(sx0 > ground[0].x && sx1 < ground[1].x && sy0 > 5.0 && sy1 < 43.75);
}

#[test]
fn the_sign_size_follows_the_speed_band() {
    let e = engine();
    let at = |key: &str, kph: u32| {
        let sign = e.definition_for_speed(entry(&e, key), kph);
        (e.sign_size_mm(&sign), e.layout(&sign).unwrap().sw_mm)
    };
    // TSM Chapter 4 Appendix A column 2.
    assert_eq!(at("D504.1", 32), (Some(600.0), 12.0));
    assert_eq!(at("D504.1", 48), (Some(600.0), 12.0));
    assert_eq!(at("D504.1", 64), (Some(750.0), 15.0));
    assert_eq!(at("D504.1", 80), (Some(900.0), 18.0));
    assert_eq!(at("D504.1", 120), (Some(1200.0), 24.0));
    // TSM Chapter 3 Table 3-2 (STOP) and Table 14-1 (speed limit).
    assert_eq!(at("D601.1", 48), (Some(750.0), 15.0));
    assert_eq!(at("D601.1", 80), (Some(1200.0), 24.0));
    assert_eq!(at("D670", 96), (Some(750.0), 15.0));
}

/// A band the table gives no size for: the sign is still drawn, flagged, and takes the
/// stack-type text size of its speed (30 km/h: 75 mm x-height, 18.75 mm sw).
#[test]
fn a_band_with_no_tabled_size_is_flagged() {
    let e = engine();
    let sign = e.definition_for_speed(entry(&e, "D632"), 30);
    assert_eq!(e.sign_size_mm(&sign), None);
    let layout = e.layout(&sign).unwrap();
    assert_eq!((layout.sw_mm, layout.x_height_mm), (18.75, 75.0));
    assert_eq!(layout.diagnostics.len(), 1);
    let d = &layout.diagnostics[0];
    assert_eq!(d.code, "SIGN_SIZE_UNSPECIFIED");
    assert!(
        d.message.contains("gives no size at 30 km/h"),
        "{}",
        d.message
    );
    assert!(
        d.message.ends_with("(600, 750, 900, 1200 mm)"),
        "{}",
        d.message
    );
    assert!(!layout.has_errors());
}

#[test]
fn stop_give_way_and_the_discs_have_their_tsrgd_bodies() {
    let e = engine();
    let body = |key: &str| {
        let layout = e.layout(&e.definition_for(entry(&e, key))).unwrap();
        let (outer, border) = fill(&layout.drawing.primitives[0]);
        let (inner, ground) = fill(&layout.drawing.primitives[1]);
        (
            outer.len(),
            border,
            inner.len(),
            ground,
            layout.drawing.extents.width(),
        )
    };
    // Octagon, white border, red ground, 50 sw across the flats.
    assert_eq!(body("D601.1"), (8, Colour::White, 8, Colour::Red, 50.0));
    // Discs are 96-gons.
    assert_eq!(body("D606"), (96, Colour::White, 96, Colour::Blue, 50.0));
    assert_eq!(body("D616"), (96, Colour::White, 96, Colour::Red, 50.0));
    assert_eq!(body("D619"), (96, Colour::Red, 96, Colour::White, 50.0));
    // Give way: the rounded triangle, point down.
    let layout = e.layout(&e.definition_for(entry(&e, "D602"))).unwrap();
    let (inner, _) = fill(&layout.drawing.primitives[1]);
    assert!(close(inner[0].y, 6.25) && close(inner[1].y, 45.0) && close(inner[2].y, 45.0));
}

/// Diagram 670 lets its numerals vary: `670:30` sets "30" in black Transport Heavy at the
/// height TSRGD draws them (0.38658 of the diameter), centred where TSRGD centres them.
#[test]
fn a_speed_limit_sets_the_chosen_numerals() {
    let e = engine();
    let mut sign = e.definition_for(entry(&e, "D670"));
    let numerals = |sign: &SignDefinition| {
        let layout = e.layout(sign).unwrap();
        assert_eq!(layout.drawing.primitives.len(), 3);
        let (text, ink, b) = glyphs(&layout.drawing.primitives[2]);
        (text.to_string(), ink, b)
    };
    let (text, ink, _) = numerals(&sign);
    assert_eq!((text.as_str(), ink), ("40", Some(Colour::Black)));

    sign.symbol_id = Some("670:30".into());
    let (text, ink, (x0, y0, x1, y1)) = numerals(&sign);
    assert_eq!((text.as_str(), ink), ("30", Some(Colour::Black)));
    let cap = 0.38658 * 50.0;
    // The digits stand on the baseline and are one capital height tall; the round ones
    // overshoot both by about half a stroke width at this size.
    assert!((y0 - (0.49744 * 50.0 - cap / 2.0)).abs() < 0.8, "{y0}");
    assert!((y1 - y0 - cap).abs() < 1.2, "{}", y1 - y0);
    assert!(((x0 + x1) / 2.0 - (25.0 + 0.00717 * 50.0)).abs() < 1.0);
}

/// TSM Chapter 4 §1.44: a temporary sign changes its white ground to yellow; the border and
/// the symbol stay. A ground that is not white is left alone.
#[test]
fn a_temporary_sign_has_a_yellow_ground() {
    let e = engine();
    let ground = |key: &str, scheme: ColourScheme, background: Colour| {
        let mut sign = e.definition_for(entry(&e, key));
        sign.scheme = scheme;
        sign.background = background;
        let layout = e.layout(&sign).unwrap();
        (
            fill(&layout.drawing.primitives[0]).1,
            fill(&layout.drawing.primitives[1]).1,
        )
    };
    assert_eq!(
        ground("D512", ColourScheme::Secondary, Colour::White),
        (Colour::Red, Colour::White)
    );
    assert_eq!(
        ground("D512", ColourScheme::Temporary, Colour::White),
        (Colour::Red, Colour::Yellow)
    );
    assert_eq!(
        ground("D512", ColourScheme::Auto, Colour::Yellow),
        (Colour::Red, Colour::Yellow)
    );
    assert_eq!(
        ground("D606", ColourScheme::Temporary, Colour::Yellow),
        (Colour::White, Colour::Blue)
    );
}

/// The Uganda-style drawing of warning and regulatory signs is not ported: without a TSRGD
/// pictogram for the symbol the sign is refused, not drawn differently.
#[test]
fn a_warning_or_regulatory_sign_without_tsrgd_artwork_is_unsupported() {
    let e = engine();
    for (kind, symbol) in [
        (SignKind::WarningSign, Some("hump")),
        (SignKind::WarningSign, None),
        (SignKind::RegulatoryProhibitory, Some("no-entry")),
        (SignKind::RegulatoryMandatory, Some("turn-left")),
        (SignKind::Stop, None),
        (SignKind::GiveWay, Some("")),
    ] {
        let mut sign = SignDefinition::stack("X-1", 60, Vec::new());
        sign.kind = kind;
        sign.symbol_id = symbol.map(str::to_string);
        let err = e.layout(&sign).unwrap_err();
        assert!(matches!(err, SignError::Unsupported(_)), "{kind:?}: {err}");
        assert!(err.to_string().contains("no TSRGD pictogram"), "{err}");
    }
}

// ─── sign with plate ──────────────────────────────────────────────────────────

/// Bend ahead with "Adverse camber" at 64 km/h. The triangle is 750 mm (15 mm sw). TSM Chapter
/// 4 Appendix B row 513.1 gives a 75 mm x-height under a 750 mm triangle, so the plate's sw is
/// 75 / 4 = 18.75 mm = 1.25 sign sw. The Figure 2-4 plate is 1.5 + 2.5 + 8 + 1.5 + 1.5 = 15
/// plate sw high = 18.75 sign sw. No gap is stated, so the plate is butted to the sign: the
/// whole is 18.75 + 50 high and the sign's foot is at 18.75.
#[test]
fn a_warning_sign_and_its_plate_are_one_object_sized_by_appendix_b() {
    let e = engine();
    let mut sign = e.definition_for_speed(entry(&e, "D512"), 64);
    sign.info_distance = Some("Adverse camber".into());
    let layout = e.layout(&sign).unwrap();

    assert_eq!(layout.sw_mm, 15.0);
    let ext = layout.drawing.extents;
    assert!(close(ext.height(), 68.75), "{}", ext.height());

    let prims = &layout.drawing.primitives;
    assert_eq!(prims.len(), 6);
    let (triangle, red) = fill(&prims[0]);
    assert_eq!(red, Colour::Red);
    let (_, ty0, _, ty1) = bounds(triangle);
    assert!(close(ty0, 18.75) && close(ty1, 68.75));

    // The plate: black border, white ground, black Transport Heavy legend, centred under the sign.
    let (plate, black) = fill(&prims[3]);
    let (px0, py0, px1, py1) = bounds(plate);
    assert_eq!(black, Colour::Black);
    assert!(close(py0, 0.0) && close(py1, 18.75));
    assert!(close((px0 + px1) / 2.0, ext.width() / 2.0));
    let (ground, white) = fill(&prims[4]);
    let (gx0, gy0, _, _) = bounds(ground);
    assert_eq!(white, Colour::White);
    assert!(close(gx0 - px0, 1.5 * 1.25) && close(gy0, 1.5 * 1.25));
    let (text, ink, (lx0, ly0, lx1, ly1)) = glyphs(&prims[5]);
    assert_eq!((text, ink), ("Adverse camber", Some(Colour::Black)));
    assert!(lx0 > gx0 && lx1 < px1 && ly0 > gy0 && ly1 < py1);

    let codes: Vec<&str> = layout.diagnostics.iter().map(|d| d.code.as_str()).collect();
    assert_eq!(codes, ["PLATE_GAP_UNSPECIFIED"]);
}

/// Appendix B note 1: under a 600 mm triangle the plate is 50 mm at 20 mph or less, else 62.5.
/// 1 sw = 12 mm, so the 15-plate-sw plate is 15 × 12.5 / 12 or 15 × 15.625 / 12 sign sw high.
#[test]
fn a_plate_under_a_600mm_triangle_is_smaller_at_low_speed() {
    let e = engine();
    let plate_height = |kph: u32| {
        let mut sign = e.definition_for_speed(entry(&e, "D504.1"), kph);
        sign.info_distance = Some("100 yds".into());
        e.layout(&sign).unwrap().drawing.extents.height() - 50.0
    };
    assert!(close(plate_height(30), 15.0 * 12.5 / 12.0));
    assert!(close(plate_height(48), 15.0 * 15.625 / 12.0));
}

#[test]
fn a_plate_with_no_tabled_x_height_is_not_drawn_and_says_so() {
    let e = engine();
    for (key, kph, legend, why) in [
        ("D616", 48, "Except buses", "covers the legend"),
        ("D557.1", 48, "Humps for 1 mile", "covers the legend"),
        ("D632", 30, "For 2 miles", "the sign has no tabled size"),
        (
            "D616",
            30,
            "For 2 miles",
            "gives no x-height for a 600 mm sign at 30 km/h",
        ),
    ] {
        let mut sign = e.definition_for_speed(entry(&e, key), kph);
        let bare = e.layout(&sign).unwrap();
        sign.info_distance = Some(legend.into());
        let layout = e.layout(&sign).unwrap();
        assert_eq!(layout.drawing, bare.drawing, "{key}");
        let d = layout.diagnostics.last().unwrap();
        assert_eq!(d.code, "PLATE_SIZE_UNSPECIFIED", "{key}");
        assert!(d.message.contains(why), "{key}: {}", d.message);
    }
}

// ─── supplementary plates ─────────────────────────────────────────────────────

fn plate(symbol: Option<&str>, legend: Option<&str>) -> SignDefinition {
    let mut sign = SignDefinition::stack("S-1", 60, Vec::new());
    sign.kind = SignKind::SupplementaryPlate;
    sign.class = SignClass::Supplementary;
    sign.background = Colour::White;
    sign.symbol_id = symbol.map(str::to_string);
    sign.info_distance = legend.map(str::to_string);
    sign
}

/// UK TSM Chapter 7 Figure 2-4: 1.5 sw border, 2.5 sw to the tiles at the top and sides and
/// 1.5 sw below them, 8 sw tiles. Two lines: 1.5 + 2.5 + 16 + 1.5 + 1.5 = 23 sw high.
#[test]
fn a_uk_plate_is_the_figure_2_4_plate_in_transport_heavy() {
    let e = engine();
    let layout = e
        .layout(&plate(Some("text"), Some("Single file|traffic")))
        .unwrap();
    assert!(layout.diagnostics.is_empty(), "{:?}", layout.diagnostics);
    // A plate on its own takes the stack-type size of its speed.
    assert_eq!((layout.sw_mm, layout.x_height_mm), (31.25, 125.0));
    let ext = layout.drawing.extents;
    assert!(close(ext.height(), 23.0));

    let prims = &layout.drawing.primitives;
    assert_eq!(prims.len(), 4);
    assert_eq!(fill(&prims[0]).1, Colour::Black);
    assert_eq!(bounds(fill(&prims[0]).0), (0.0, 0.0, ext.width(), 23.0));
    assert_eq!(fill(&prims[1]).1, Colour::White);
    assert_eq!(
        bounds(fill(&prims[1]).0),
        (1.5, 1.5, ext.width() - 1.5, 21.5)
    );
    let (first, ink, (ax0, ay0, ax1, _)) = glyphs(&prims[2]);
    let (second, _, (bx0, _, bx1, by1)) = glyphs(&prims[3]);
    assert_eq!(
        (first, second, ink),
        ("Single file", "traffic", Some(Colour::Black))
    );
    // The wider line sets the width (4 sw from each edge); both are centred, one above the other.
    assert!(ax0 > 4.0 && ax1 < ext.width() - 4.0);
    assert!(((ax0 + ax1) / 2.0 - (bx0 + bx1) / 2.0).abs() < 1.0);
    assert!(ay0 > by1);
}

#[test]
fn profile_plates_draw_their_arrow_symbol_or_example_text() {
    let e = engine();
    // The direction arrow: a 2 sw shaft and a head, 14 sw tail to tip, pointing left.
    let arrow = e.layout(&plate(Some("direction"), None)).unwrap();
    let prims = &arrow.drawing.primitives;
    assert_eq!(prims.len(), 4);
    assert!(close(
        arrow.drawing.extents.width(),
        2.0 * 1.5 + 2.0 * 2.5 + 14.0
    ));
    let Primitive::Line {
        from,
        to,
        width,
        colour,
    } = &prims[2]
    else {
        panic!("the shaft is a line")
    };
    assert_eq!((*width, *colour), (2.0, Some(Colour::Black)));
    assert!(from.x > to.x && from.y == to.y);
    let (hx0, _, hx1, _) = bounds(fill(&prims[3]).0);
    assert!(hx0 < to.x && close(hx1, to.x));

    // No artwork for the truck: a placeholder, flagged.
    let truck = e.layout(&plate(Some("truck"), None)).unwrap();
    assert!(
        matches!(truck.drawing.primitives.last(), Some(Primitive::Symbol { id, .. }) if id == "truck")
    );
    assert_eq!(truck.diagnostics.len(), 1);
    assert_eq!(truck.diagnostics[0].code, "SYMBOL_UNRESOLVED");

    // The bus is drawn.
    let bus = e.layout(&plate(Some("bus"), None)).unwrap();
    assert!(bus.diagnostics.is_empty());
    assert!(bus.drawing.primitives.len() > 3);

    // Variable text without a legend: the sign-page example for the line count.
    let mut three = plate(None, None);
    three.text_lines = Some(3);
    let texts: Vec<String> = e
        .layout(&three)
        .unwrap()
        .drawing
        .primitives
        .iter()
        .filter_map(|p| match p {
            Primitive::Glyphs { text, .. } => Some(text.clone()),
            _ => None,
        })
        .collect();
    assert_eq!(texts, ["Except", "buses", "and taxis"]);
}

// ─── information signs ────────────────────────────────────────────────────────

fn info(symbol: &str, template: InformationTemplate) -> SignDefinition {
    let mut sign = SignDefinition::stack("I-1", 60, Vec::new());
    sign.kind = SignKind::InformationSign;
    sign.class = SignClass::Information;
    sign.scheme = ColourScheme::Information;
    sign.background = Colour::Blue;
    sign.symbol_id = Some(symbol.to_string());
    sign.info_template = template;
    sign
}

/// The service panel: a 14 sw symbol on a white square 14 + 2 × 2.5 = 19 sw, inside the
/// border (1.5) and gutters (2.5 top and sides, 1.5 bottom). With an arrow band (one 8 sw tile
/// and a 2.5 sw word space): 3 + 5 + 19 = 27 wide, 3 + 2.5 + 19 + 10.5 + 1.5 = 36.5 high.
#[test]
fn an_information_panel_carries_its_symbol_arrow_and_distance() {
    let e = engine();
    let mut sign = info("aircraft", InformationTemplate::ServicePanel);
    sign.info_arrow = Some(Direction::Ahead);
    let layout = e.layout(&sign).unwrap();
    assert!(layout.diagnostics.is_empty(), "{:?}", layout.diagnostics);
    let ext = layout.drawing.extents;
    assert!(
        close(ext.width(), 27.0) && close(ext.height(), 36.5),
        "{ext:?}"
    );

    let prims = &layout.drawing.primitives;
    assert_eq!(fill(&prims[0]).1, Colour::Blue);
    assert_eq!(fill(&prims[1]).1, Colour::White);
    assert_eq!(bounds(fill(&prims[1]).0), (4.0, 13.5, 23.0, 32.5));
    // The aircraft is black on the white panel, inside it.
    let (aircraft, black) = fill(&prims[2]);
    assert_eq!(black, Colour::Black);
    let (ax0, ay0, ax1, ay1) = bounds(aircraft);
    assert!(ax0 >= 6.5 && ax1 <= 20.5 && ay0 >= 16.0 && ay1 <= 30.0);
    // The arrow: a white shaft on the centre line with a triangle head, in the band below.
    let n = prims.len();
    let Primitive::Line {
        from, to, colour, ..
    } = &prims[n - 2]
    else {
        panic!("the arrow shaft is a line")
    };
    assert_eq!(*colour, Some(Colour::White));
    assert!(close(from.x, 13.5) && close(to.x, 13.5) && close(from.y, 3.0));
    let (head, white) = fill(&prims[n - 1]);
    assert_eq!((head.len(), white), (3, Colour::White));
    assert!(close(bounds(head).3, 11.0) && close(bounds(head).1, to.y));

    // A distance adds a second band and is set in the scheme's face and colour.
    sign.info_distance = Some("400 yds".into());
    let with_distance = e.layout(&sign).unwrap();
    assert!(close(with_distance.drawing.extents.height(), 47.0));
    let (text, ink, _) = glyphs(with_distance.drawing.primitives.last().unwrap());
    assert_eq!((text, ink), ("400 yds", Some(Colour::White)));
}

#[test]
fn an_information_plate_carries_its_symbol_on_the_ground() {
    let e = engine();
    // 14 sw symbol with 2.5 sw each side, inside gutters and border: 27 wide, 26 high.
    let layout = e
        .layout(&info("one-way", InformationTemplate::SymbolPlate))
        .unwrap();
    let ext = layout.drawing.extents;
    assert!(
        close(ext.width(), 27.0) && close(ext.height(), 26.0),
        "{ext:?}"
    );
    assert!(layout.drawing.primitives[1..]
        .iter()
        .all(|p| fill(p).1 == Colour::White));

    // Hospital is the letter H, as tall as the symbol box.
    let hospital = e
        .layout(&info("hospital", InformationTemplate::SymbolPlate))
        .unwrap();
    let (text, ink, (_, y0, _, y1)) = glyphs(&hospital.drawing.primitives[1]);
    assert_eq!((text, ink), ("H", Some(Colour::White)));
    assert!((y1 - y0 - 14.0).abs() < 0.2, "{}", y1 - y0);

    // No artwork: a placeholder, flagged against the pack's own clause.
    let castle = e
        .layout(&info("castle", InformationTemplate::SymbolPlate))
        .unwrap();
    assert!(
        matches!(&castle.drawing.primitives[1], Primitive::Symbol { id, size, .. } if id == "castle" && *size == 14.0)
    );
    assert_eq!(castle.diagnostics[0].code, "SYMBOL_UNRESOLVED");
    assert_eq!(
        castle.diagnostics[0].source,
        "TSRGD 2016 Schedule 12 Part 11"
    );
}

// ─── pictograms on stack-type signs ───────────────────────────────────────────

/// A destination symbol the library draws is real geometry in the legend colour, turned to
/// its row's direction; the pack gives it no size, which is still flagged.
#[test]
fn a_destination_pictogram_is_drawn_in_the_legend_colour() {
    let e = engine();
    let mut airport = Destination::new("Entebbe", Direction::Left);
    airport.symbol_id = Some("aircraft".into());
    let layout = e
        .layout(&SignDefinition::stack("S", 60, vec![airport]))
        .unwrap();
    let codes: Vec<&str> = layout.diagnostics.iter().map(|d| d.code.as_str()).collect();
    assert_eq!(codes, ["SYMBOL_SIZE_UNSPECIFIED"]);
    assert!(!layout
        .drawing
        .primitives
        .iter()
        .any(|p| matches!(p, Primitive::Symbol { .. })));
    // The aircraft's last shape is its nose cone: white, 10 sw box, pointing left.
    let nose = layout
        .drawing
        .primitives
        .iter()
        .filter_map(|p| match p {
            Primitive::Fill {
                outline,
                colour: Colour::White,
            } if outline.len() == 3 => Some(outline),
            _ => None,
        })
        .next_back()
        .unwrap();
    let tip = nose[2];
    assert!(nose.iter().all(|p| p.x >= tip.x), "{nose:?}");
}

// ─── what the std_* golden fixtures cover ─────────────────────────────────────

fn golden_fixtures() -> Vec<(String, serde_json::Value)> {
    let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/golden");
    let mut out = Vec::new();
    for e in std::fs::read_dir(&dir).unwrap() {
        let path = e.unwrap().path();
        let name = path.file_stem().unwrap().to_str().unwrap().to_string();
        if path.extension().is_some_and(|x| x == "json") && name.starts_with("std_") {
            let text = std::fs::read_to_string(&path).unwrap();
            out.push((name, serde_json::from_str(&text).unwrap()));
        }
    }
    out.sort_by(|a, b| a.0.cmp(&b.0));
    out
}

/// Every catalog entry of a kind this port lays out has a fixture at its catalog definition.
#[test]
fn every_ported_catalog_entry_has_a_golden_fixture() {
    let e = engine();
    let fixtures = golden_fixtures();
    let mut covered = 0;
    for entry in e.catalog() {
        let ported = matches!(
            entry.kind,
            SignKind::StackType
                | SignKind::InformationSign
                | SignKind::WarningSign
                | SignKind::RegulatoryProhibitory
                | SignKind::RegulatoryMandatory
                | SignKind::Stop
                | SignKind::GiveWay
                | SignKind::SupplementaryPlate
        );
        if !ported {
            continue;
        }
        let name = format!("std_{}", entry.key.to_lowercase().replace('.', "_"));
        let (_, fx) = fixtures
            .iter()
            .find(|(n, _)| *n == name)
            .unwrap_or_else(|| panic!("no fixture {name}"));
        let definition: SignDefinition = serde_json::from_value(fx["definition"].clone()).unwrap();
        assert_eq!(definition, e.definition_for(entry), "{name}");
        covered += 1;
    }
    assert_eq!(covered, 27); // 24 TSRGD diagrams and the three stack-type entries
}

/// Every id the pictogram library accepts is drawn in some fixture.
#[test]
fn the_fixtures_draw_every_pictogram_id() {
    let mut ids = BTreeSet::new();
    for (name, fx) in golden_fixtures() {
        if !name.starts_with("std_pictograms_") {
            continue;
        }
        for d in fx["definition"]["destinations"].as_array().unwrap() {
            if let Some(id) = d["symbol_id"].as_str() {
                ids.insert(id.trim().to_lowercase());
            }
        }
        // None fell back to a placeholder.
        assert!(
            !fx["primitives"]
                .as_array()
                .unwrap()
                .iter()
                .any(|p| p["type"] == "Symbol"),
            "{name}"
        );
    }
    assert_eq!(ids.len(), 66, "{ids:?}");
    for id in pictogram_library::SUPPORTED_IDS {
        assert!(ids.contains(id), "{id}");
    }
}

#[test]
fn the_fixtures_cover_the_standard_sign_features() {
    let fixtures = golden_fixtures();
    let mut codes = BTreeSet::new();
    let mut kinds = BTreeSet::new();
    let mut plates = 0;
    for (_, fx) in &fixtures {
        for d in fx["diagnostics"].as_array().unwrap() {
            codes.insert(d["code"].as_str().unwrap().to_string());
        }
        let def = &fx["definition"];
        kinds.insert(def["kind"].as_str().unwrap().to_string());
        let warning_or_regulatory = !matches!(
            def["kind"].as_str().unwrap(),
            "StackType" | "InformationSign" | "SupplementaryPlate"
        );
        if warning_or_regulatory && def["info_distance"].is_string() {
            plates += 1;
        }
    }
    for code in [
        "SIGN_SIZE_UNSPECIFIED",
        "PLATE_SIZE_UNSPECIFIED",
        "PLATE_GAP_UNSPECIFIED",
        "SYMBOL_UNRESOLVED",
        "SYMBOL_SIZE_UNSPECIFIED",
        "ROUTE_NUMBER_NOT_DRAWN",
        "TILE_WIDTH_FROM_FONT",
        "MAX_DESTINATIONS",
        "MAX_PER_DIRECTION",
        "NEARER_NOT_ABOVE",
    ] {
        assert!(codes.contains(code), "no fixture raises {code}");
    }
    for kind in [
        "StackType",
        "InformationSign",
        "WarningSign",
        "RegulatoryProhibitory",
        "RegulatoryMandatory",
        "Stop",
        "GiveWay",
        "SupplementaryPlate",
    ] {
        assert!(kinds.contains(kind), "no fixture of kind {kind}");
    }
    assert!(plates >= 9, "{plates} sign-with-plate fixtures");
}
