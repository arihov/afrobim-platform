// SPDX-License-Identifier: LicenseRef-AfroBIM-Proprietary
// Copyright (c) 2026 AfroBIM. All rights reserved.
//! Map-type signs through the public API the desktop app codes against, and a guard on the
//! `map_*` golden fixtures (`tests/golden.rs` compares them to the C# engine with every other
//! fixture; this file checks that the set still covers what the map-type port claims).

use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};

use afrobim_draw::{Colour, Primitive, Pt2};
use afrobim_signs::{
    ColourScheme, Destination, Direction, EmbeddedSign, EmbeddedType, RoadClass, RouteStatus,
    Severity, SignDefinition, SignEngine, SignKind,
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

fn bounds(outline: &[Pt2]) -> (f64, f64, f64, f64) {
    outline.iter().fold(
        (f64::MAX, f64::MAX, f64::MIN, f64::MIN),
        |(x0, y0, x1, y1), p| (x0.min(p.x), y0.min(p.y), x1.max(p.x), y1.max(p.y)),
    )
}

/// The app's "Add map" sign (C# `AddMapCrossroads`).
fn app_default() -> SignDefinition {
    SignDefinition::map_crossroads(
        "ADS-001",
        60,
        vec![
            Destination::new("Left", Direction::Left),
            Destination::new("Ahead", Direction::Ahead),
            Destination::new("Right", Direction::Right),
        ],
    )
}

fn fills(layout: &afrobim_signs::SignLayout) -> Vec<(&[Pt2], Colour)> {
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

/// The app's default crossroads: 44 sw high (see the unit test of the same sign for the
/// dimension chain), lettered at the advance-direction-sign size for 60 km/h.
#[test]
fn the_app_default_crossroads_lays_out_through_the_engine() {
    let layout = engine().layout(&app_default()).unwrap();
    assert_eq!((layout.sw_mm, layout.x_height_mm), (31.25, 125.0));
    assert!(layout.diagnostics.is_empty(), "{:?}", layout.diagnostics);
    let ext = layout.drawing.extents;
    assert!(close(ext.min.x, 0.0) && close(ext.min.y, 0.0));
    assert!(close(ext.max.y, 44.0), "{}", ext.max.y);

    // The plate (white border fill, green ground), then the white route symbol; lettering is
    // glyph outlines, one primitive per legend, in white on the green ground.
    let prims = &layout.drawing.primitives;
    assert_eq!(prims.len(), 6);
    let fills = fills(&layout);
    assert_eq!(fills.len(), 3);
    assert_eq!(
        fills.iter().map(|f| f.1).collect::<Vec<_>>(),
        [Colour::White, Colour::Green, Colour::White]
    );
    let (x0, y0, x1, y1) = bounds(fills[0].0);
    assert!(close(x0, 0.0) && close(y0, 0.0) && close(x1, ext.max.x) && close(y1, 44.0));
    let (x0, y0, x1, y1) = bounds(fills[1].0);
    assert!(close(x0, 1.5) && close(y0, 1.5));
    assert!(close(x1, ext.max.x - 1.5) && close(y1, 42.5));
    // The symbol: approach base 1.5 sw above the border, ahead point 12.5 sw above the arms.
    let (_, y0, _, y1) = bounds(fills[2].0);
    assert!(close(y0, 3.0) && close(y1, 32.0));

    let legends: Vec<(&str, Option<Colour>)> = prims
        .iter()
        .filter_map(|p| match p {
            Primitive::Glyphs { text, ink, .. } => Some((text.as_str(), *ink)),
            _ => None,
        })
        .collect();
    assert_eq!(
        legends,
        [
            ("Ahead", Some(Colour::White)),
            ("Left", Some(Colour::White)),
            ("Right", Some(Colour::White))
        ]
    );
    // One filled symbol: no stroked arms, no arcs.
    assert!(!prims
        .iter()
        .any(|p| matches!(p, Primitive::Line { .. } | Primitive::Arc { .. })));
}

#[test]
fn all_three_map_kinds_lay_out_and_scale_with_speed() {
    let e = engine();
    for kind in [
        SignKind::MapTypeCrossroads,
        SignKind::MapTypeTee,
        SignKind::MapTypeRoundabout,
    ] {
        let mut sign = app_default();
        sign.kind = kind;
        let at_60 = e.layout(&sign).unwrap();
        sign.speed_kph = 100;
        let at_100 = e.layout(&sign).unwrap();
        // The layout is in stroke widths: the speed changes the scale, not the geometry.
        assert_eq!(at_60.drawing, at_100.drawing, "{kind:?}");
        assert_eq!((at_100.sw_mm, at_100.x_height_mm), (62.5, 250.0));
        assert!(!at_60.has_errors());
    }
}

/// A T-junction ignores forward destinations with a warning; a roundabout with no left exit
/// says that the manual does not give the gap it uses.
#[test]
fn map_diagnostics_reach_the_caller() {
    let e = engine();
    let mut tee = app_default();
    tee.kind = SignKind::MapTypeTee;
    let layout = e.layout(&tee).unwrap();
    assert_eq!(layout.diagnostics.len(), 1);
    assert_eq!(layout.diagnostics[0].code, "TEE_AHEAD_IGNORED");
    assert_eq!(layout.diagnostics[0].severity, Severity::Warning);
    assert!(!layout
        .drawing
        .primitives
        .iter()
        .any(|p| matches!(p, Primitive::Glyphs { text, .. } if text == "Ahead")));

    let mut ring = app_default();
    ring.kind = SignKind::MapTypeRoundabout;
    ring.destinations.retain(|d| d.direction != Direction::Left);
    let layout = e.layout(&ring).unwrap();
    assert_eq!(layout.diagnostics.len(), 1);
    assert_eq!(layout.diagnostics[0].code, "ROUNDABOUT_LEFT_GAP_UNCITED");
    assert_eq!(layout.diagnostics[0].severity, Severity::Info);
    assert!(!layout.has_errors());
}

/// Route numbers on a map-type sign follow the stack-type rules: a blue patch for a motorway
/// number, yellow numbers on green, a white panel for a non-primary route.
#[test]
fn route_numbers_patches_and_panels_are_drawn_on_map_signs() {
    let mut sign = app_default();
    sign.scheme = ColourScheme::Primary;
    sign.destinations = vec![
        Destination::new("Cambridge", Direction::Ahead).with_route("(M11)", RouteStatus::Primary),
        Destination::new("Royston", Direction::Left).with_route("A505", RouteStatus::Primary),
        Destination::new("Barton", Direction::Right).with_route("B1040", RouteStatus::NonPrimary),
    ];
    for kind in [SignKind::MapTypeCrossroads, SignKind::MapTypeRoundabout] {
        sign.kind = kind;
        let layout = engine().layout(&sign).unwrap();
        assert!(layout.diagnostics.is_empty(), "{:?}", layout.diagnostics);
        let fills = fills(&layout);
        assert!(fills.iter().any(|f| f.1 == Colour::Blue && f.0.len() == 4));
        // The non-primary panel: a white rectangle that is not the plate's border fill.
        assert!(fills
            .iter()
            .skip(3)
            .any(|f| f.1 == Colour::White && f.0.len() == 4));
        let ink = |legend: &str| {
            layout
                .drawing
                .primitives
                .iter()
                .find_map(|p| match p {
                    Primitive::Glyphs { text, ink, .. } if text == legend => Some(*ink),
                    _ => None,
                })
                .unwrap_or_else(|| panic!("no legend '{legend}'"))
        };
        assert_eq!(ink("505"), Some(Colour::Yellow));
        assert_eq!(ink("Barton"), Some(Colour::Black));
        assert_eq!(ink("Cambridge"), Some(Colour::White));
    }
}

#[test]
fn embedded_signs_on_a_map_sign_are_drawn_over_the_arm() {
    let engine = engine();
    let mut sign = app_default();
    let control = engine.layout(&sign).unwrap();
    sign.embedded = Some(vec![
        EmbeddedSign {
            symbol_id: "height-limit".into(),
            kind: EmbeddedType::RegulatoryRoundel,
        },
        EmbeddedSign {
            symbol_id: "crossroads-ahead".into(),
            kind: EmbeddedType::WarningTriangle,
        },
    ]);
    for kind in [
        SignKind::MapTypeCrossroads,
        SignKind::MapTypeTee,
        SignKind::MapTypeRoundabout,
    ] {
        sign.kind = kind;
        let layout = engine.layout(&sign).unwrap();
        let red: Vec<usize> = fills(&layout)
            .iter()
            .filter(|(_, c)| *c == Colour::Red)
            .map(|(o, _)| o.len())
            .collect();
        // The roundel's 48-sided ring above the three-sided triangle frame.
        assert_eq!(red, vec![48, 3], "{kind:?}");
        assert_eq!(
            engine.embedded_support(kind),
            afrobim_signs::EmbeddedSupport::Drawn
        );
    }
    // The sign's size does not change.
    sign.kind = SignKind::MapTypeCrossroads;
    let layout = engine.layout(&sign).unwrap();
    assert_eq!(layout.drawing.extents, control.drawing.extents);
    assert_eq!(layout.diagnostics, control.diagnostics);
}

// ─── the map_* golden fixtures ────────────────────────────────────────────────────

struct MapFixture {
    name: String,
    definition: SignDefinition,
    codes: Vec<String>,
    primitive_types: Vec<String>,
}

fn map_fixtures() -> Vec<MapFixture> {
    let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/golden");
    let mut paths: Vec<PathBuf> = std::fs::read_dir(&dir)
        .unwrap_or_else(|e| panic!("{}: {e}", dir.display()))
        .map(|e| e.unwrap().path())
        .filter(|p| {
            p.file_name()
                .and_then(|n| n.to_str())
                .is_some_and(|n| n.starts_with("map_") && n.ends_with(".json"))
        })
        .collect();
    paths.sort();
    paths
        .iter()
        .map(|p| {
            let text = std::fs::read_to_string(p).unwrap();
            let v: serde_json::Value =
                serde_json::from_str(&text).unwrap_or_else(|e| panic!("{}: {e}", p.display()));
            let strings = |list: &serde_json::Value, key: &str| -> Vec<String> {
                list.as_array()
                    .unwrap()
                    .iter()
                    .map(|x| x[key].as_str().unwrap().to_string())
                    .collect()
            };
            MapFixture {
                name: v["name"].as_str().unwrap().to_string(),
                definition: serde_json::from_value(v["definition"].clone()).unwrap(),
                codes: strings(&v["diagnostics"], "code"),
                primitive_types: strings(&v["primitives"], "type"),
            }
        })
        .collect()
}

/// Which of left, ahead and right a sign has destinations for.
fn arms(def: &SignDefinition) -> (bool, bool, bool) {
    let has = |dir| def.destinations.iter().any(|d| d.direction == dir);
    (
        has(Direction::Left),
        has(Direction::Ahead),
        has(Direction::Right),
    )
}

#[test]
fn the_map_fixtures_cover_the_ported_features() {
    let fixtures = map_fixtures();
    let mut by_kind: BTreeMap<&str, Vec<&MapFixture>> = BTreeMap::new();
    for fx in &fixtures {
        let prefix = match fx.definition.kind {
            SignKind::MapTypeCrossroads => "map_crossroads_",
            SignKind::MapTypeTee => "map_tee_",
            SignKind::MapTypeRoundabout => "map_roundabout_",
            other => panic!("{}: {other:?} is not a map-type sign", fx.name),
        };
        assert!(fx.name.starts_with(prefix), "{}", fx.name);
        by_kind.entry(prefix).or_default().push(fx);
        // A map-type sign is fills and lettering only: one filled symbol, no lines or arcs.
        assert!(
            fx.primitive_types
                .iter()
                .all(|t| t == "Fill" || t == "Glyphs"),
            "{}: {:?}",
            fx.name,
            fx.primitive_types
        );
    }
    assert_eq!(by_kind.len(), 3);

    // Every combination of present and absent arms: eight for a crossroads and a roundabout,
    // four (left, right) for a T-junction, which has no ahead arm.
    for (prefix, list) in &by_kind {
        assert!(list.len() >= 4, "{prefix}: {} fixtures", list.len());
        let combos: BTreeSet<(bool, bool, bool)> = list
            .iter()
            .map(|fx| {
                let (left, ahead, right) = arms(&fx.definition);
                (left, ahead && *prefix != "map_tee_", right)
            })
            .collect();
        let expected = if *prefix == "map_tee_" { 4 } else { 8 };
        assert_eq!(combos.len(), expected, "{prefix}: {combos:?}");

        let speeds: BTreeSet<u32> = list.iter().map(|fx| fx.definition.speed_kph).collect();
        assert!(speeds.len() >= 2, "{prefix}: {speeds:?}");
        let approaches: BTreeSet<String> = list
            .iter()
            .map(|fx| format!("{:?}", fx.definition.approach_class))
            .collect();
        assert!(approaches.len() >= 2, "{prefix}: {approaches:?}");
        let statuses: BTreeSet<String> = list
            .iter()
            .flat_map(|fx| &fx.definition.destinations)
            .filter(|d| d.route_number.is_some())
            .map(|d| format!("{:?}", d.route))
            .collect();
        assert!(statuses.len() >= 2, "{prefix}: {statuses:?}");
    }
    let all_statuses: BTreeSet<String> = fixtures
        .iter()
        .flat_map(|fx| &fx.definition.destinations)
        .map(|d| format!("{:?}", d.route))
        .collect();
    assert_eq!(all_statuses.len(), 4, "{all_statuses:?}");
    assert!(fixtures
        .iter()
        .any(|fx| fx.definition.approach_class == RoadClass::Local));
    assert!(fixtures
        .iter()
        .any(|fx| fx.definition.background == Colour::White));

    // Every diagnostic a map-type sign can raise with the UK pack.
    let codes: BTreeSet<&str> = fixtures
        .iter()
        .flat_map(|fx| fx.codes.iter().map(String::as_str))
        .collect();
    for code in [
        "TEE_AHEAD_IGNORED",
        "ROUNDABOUT_LEFT_GAP_UNCITED",
        "MAX_DESTINATIONS",
        "MAX_PER_DIRECTION",
        "NEARER_NOT_ABOVE",
        "TILE_WIDTH_FROM_FONT",
        "MOTORWAY_ALPHABET_NOT_MODELLED",
    ] {
        assert!(codes.contains(code), "no map fixture raises {code}");
    }
}
