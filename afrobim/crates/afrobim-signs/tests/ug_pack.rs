// SPDX-License-Identifier: LicenseRef-AfroBIM-Proprietary
// Copyright (c) 2026 AfroBIM. All rights reserved.
//! The Uganda rule pack (`packs/UG-TSM-2004`, a folder of XML files) through the public API:
//! loading it by manifest and by folder, its catalog and the definition each entry seeds, its
//! body-size ladder and roundel / triangle section (all checked against what the C# loader and
//! `CatalogSign.Build` give, written by `tools/GoldenDump` to `tests/golden/ug/`), and
//! hand-worked dimensions of the layouts only this pack takes.
//!
//! The primitive-by-primitive comparison with the C# engine is in `golden.rs` (the `ug_*`
//! fixtures); the last test here checks that those fixtures cover what the port claims.

use std::collections::BTreeSet;
use std::path::{Path, PathBuf};

use afrobim_draw::{Colour, Primitive, Pt2};
use afrobim_signs::ug_pack::SizeFamily;
use afrobim_signs::{
    ColourScheme, Destination, Direction, SignClass, SignDefinition, SignEngine, SignError,
    SignKind, SignMirror, SignVariant,
};
use serde::Deserialize;

fn repo_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../..")
}

fn engine() -> SignEngine {
    let root = repo_root();
    SignEngine::load(
        &root.join("packs/UG-TSM-2004/pack.xml"),
        &root.join("fonts"),
    )
    .expect("the Uganda pack and the Transport fonts load")
}

fn golden(file: &str) -> String {
    let path = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests/golden")
        .join(file);
    std::fs::read_to_string(&path).unwrap_or_else(|e| panic!("{}: {e}", path.display()))
}

fn close(a: f64, b: f64) -> bool {
    (a - b).abs() < 1e-9
}

fn sign(kind: SignKind, speed_kph: u32, destinations: Vec<Destination>) -> SignDefinition {
    SignDefinition {
        kind,
        ..SignDefinition::stack("UG", speed_kph, destinations)
    }
}

fn fills(prims: &[Primitive]) -> Vec<(&[Pt2], Colour)> {
    prims
        .iter()
        .filter_map(|p| match p {
            Primitive::Fill { outline, colour } => Some((outline.as_slice(), *colour)),
            _ => None,
        })
        .collect()
}

fn lines(prims: &[Primitive]) -> Vec<(Pt2, Pt2, f64)> {
    prims
        .iter()
        .filter_map(|p| match p {
            Primitive::Line {
                from, to, width, ..
            } => Some((*from, *to, *width)),
            _ => None,
        })
        .collect()
}

// ─── loading ──────────────────────────────────────────────────────────────────

#[test]
fn the_pack_loads_by_manifest_and_by_folder() {
    let root = repo_root();
    let by_manifest = engine();
    let by_folder = SignEngine::load(&root.join("packs/UG-TSM-2004"), &root.join("fonts")).unwrap();
    for e in [&by_manifest, &by_folder] {
        assert_eq!(e.pack_id(), "UG-TSM-2004");
        assert_eq!(e.pack_name(), "Uganda Traffic Signs Manual");
        assert_eq!(e.catalog().len(), 105);
    }
    assert_eq!(by_manifest.pack(), by_folder.pack());

    // App.1 §2.1.1 Table 1, §1.3, §2.1.2 and §2.6 Figure 5, as the section files state them.
    let p = by_manifest.pack();
    assert_eq!(p.text_sizes.len(), 3);
    assert_eq!(p.text_sizes[2].speed_max_kph, None);
    assert_eq!((p.panel.border_width, p.panel.text_tile_height), (1.5, 8.0));
    assert_eq!(p.constraints.max_destinations, 4);
    assert_eq!(p.constraints.max_destinations_per_direction, 2);
    let arrows = p.arrows.as_ref().unwrap();
    assert!(!arrows.is_chevron());
    assert_eq!((arrows.shaft_length, arrows.flare_angle_deg), (12.0, 70.0));
    // The Uganda pack has none of the UK TSM Chapter 7 sections but does carry the filled
    // map-type route symbol.
    assert!(p.stack.is_none() && p.blocks.is_none() && p.distances.is_none());
    assert!(p.sign_border.is_none() && p.panel.tile_widths.is_none());
    let ms = p.map_symbol.as_ref().unwrap();
    assert!(ms.reach_from_border() && ms.exit_min_to_point && !ms.stubs_cited);
    assert_eq!(
        p.roundabout.as_ref().unwrap().legend_to_ring_side,
        Some(2.0)
    );
    assert_eq!(p.palettes.as_ref().unwrap().entries.len(), 5);
    assert_eq!(
        p.symbols
            .as_ref()
            .unwrap()
            .find("AIRCRAFT")
            .unwrap()
            .size_sw,
        10.0
    );
}

#[test]
fn a_missing_pack_or_manifest_is_a_pack_error() {
    let root = repo_root();
    let fonts = root.join("fonts");
    for path in [root.join("packs/no-such-pack/pack.xml"), root.join("fonts")] {
        let err = SignEngine::load(&path, &fonts).unwrap_err();
        assert!(matches!(err, SignError::Pack(_)), "{err}");
        assert!(
            err.to_string().starts_with("Rule pack not found: "),
            "{err}"
        );
    }
    // The fonts are found as for a JSON pack.
    let err = SignEngine::load(&root.join("packs/UG-TSM-2004"), &root.join("packs")).unwrap_err();
    assert!(matches!(err, SignError::Font(_)), "{err}");
}

// ─── what the C# loader reads ─────────────────────────────────────────────────

#[derive(Deserialize)]
struct PackFixture {
    id: String,
    name: String,
    source: String,
    body_sizes: Vec<BodySizeFixture>,
    roundels: RoundelsFixture,
}

#[derive(Deserialize)]
struct BodySizeFixture {
    speed_kph: u32,
    class: String,
    warning_mm: f64,
    regulatory_mm: f64,
    regulatory_low_speed_mm: f64,
    supplementary_x_height_mm: f64,
}

#[derive(Deserialize)]
struct RoundelsFixture {
    warning_shape: String,
    give_way_shape: String,
    stop_shape: String,
    regulatory_disc_shape: String,
    stop_orientation: String,
    slashes: Vec<SlashFixture>,
    provisional: Vec<ProvisionalFixture>,
}

#[derive(Deserialize)]
struct SlashFixture {
    id: String,
    applies_to: String,
    colour: String,
    direction: String,
    count: i64,
}

#[derive(Deserialize)]
struct ProvisionalFixture {
    key: String,
    fraction: f64,
    source: String,
}

/// The pack's identity, the Vol.1 Table D.1 / D.3 / D.4 body sizes by speed and the roundel /
/// triangle section, as the C# loader and `StandardSizeRules` give them.
#[test]
fn the_uganda_only_sections_match_the_reference_loader() {
    let e = engine();
    let fx: PackFixture = serde_json::from_str(&golden("ug/pack.json")).unwrap();
    assert_eq!(e.pack_id(), fx.id);
    assert_eq!(e.pack_name(), fx.name);
    assert_eq!(e.pack().source, fx.source);

    let sizes = e.standard_sizes().expect("standardSizes");
    assert_eq!(fx.body_sizes.len(), 6);
    for b in &fx.body_sizes {
        let s = b.speed_kph;
        assert_eq!(sizes.for_speed(s).unwrap().class, b.class, "{s}");
        let at = |family, low| sizes.body_size_mm(family, s, low).unwrap();
        assert_eq!(at(SizeFamily::Warning, false), b.warning_mm, "{s}");
        assert_eq!(at(SizeFamily::Regulatory, false), b.regulatory_mm, "{s}");
        assert_eq!(
            at(SizeFamily::Regulatory, true),
            b.regulatory_low_speed_mm,
            "{s}"
        );
        assert_eq!(
            at(SizeFamily::Supplementary, false),
            b.supplementary_x_height_mm,
            "{s}"
        );
    }
    let medium = sizes.for_class("medium").unwrap();
    assert_eq!(
        (medium.supplementary_sw_mm, medium.supplementary_cap_mm),
        (18.75, 105.0)
    );

    let rt = e.roundels_triangles().expect("roundelsTriangles");
    assert_eq!(rt.warning_shape, fx.roundels.warning_shape);
    assert_eq!(rt.give_way_shape, fx.roundels.give_way_shape);
    assert_eq!(rt.stop_shape, fx.roundels.stop_shape);
    assert_eq!(rt.regulatory_disc_shape, fx.roundels.regulatory_disc_shape);
    assert_eq!(rt.stop_orientation, fx.roundels.stop_orientation);
    assert_eq!(rt.slashes.len(), fx.roundels.slashes.len());
    for (g, w) in rt.slashes.iter().zip(&fx.roundels.slashes) {
        assert_eq!(
            (&g.id, &g.applies_to, &g.colour, &g.direction, g.count),
            (&w.id, &w.applies_to, &w.colour, &w.direction, w.count)
        );
    }
    assert_eq!(rt.provisional_fractions.len(), 5);
    assert_eq!(
        rt.provisional_fractions.len(),
        fx.roundels.provisional.len()
    );
    for (g, w) in rt
        .provisional_fractions
        .iter()
        .zip(&fx.roundels.provisional)
    {
        assert_eq!(
            (&g.key, g.fraction, &g.source),
            (&w.key, w.fraction, &w.source)
        );
        assert_eq!(rt.fraction(&w.key), Some(w.fraction));
    }

    // The UK pack carries neither section.
    let root = repo_root();
    let uk = SignEngine::load(&root.join("packs/UK-TSM-2019.json"), &root.join("fonts")).unwrap();
    assert!(uk.standard_sizes().is_none() && uk.roundels_triangles().is_none());
}

// ─── the catalog ──────────────────────────────────────────────────────────────

#[derive(Deserialize)]
struct CatalogFixture {
    pack: String,
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
    sizes: Option<serde_json::Value>,
    definition: SignDefinition,
    definition_at_100: SignDefinition,
}

/// Every entry, field for field, and the definition it seeds, as the C# engine has them.
#[test]
fn the_catalog_and_its_definitions_match_the_reference_engine() {
    let e = engine();
    let fx: CatalogFixture = serde_json::from_str(&golden("ug/catalog.json")).unwrap();
    assert_eq!(e.pack_id(), fx.pack);
    assert_eq!(e.catalog().len(), fx.entries.len());
    // 18 guidance + 23 information + 21 warning + 33 regulatory + 10 supplementary pages.
    assert_eq!(fx.entries.len(), 105);

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
        // The XML catalog carries no TSRGD diagram, citation or size table.
        assert_eq!(got.diagram, want.diagram, "{key}");
        assert_eq!(got.citation, want.citation, "{key}");
        assert!(got.sizes.is_none() && want.sizes.is_none(), "{key}");
        assert_eq!(e.definition_for(got), want.definition, "{key}");
        assert_eq!(
            e.definition_for_speed(got, 100),
            want.definition_at_100,
            "{key}"
        );
    }

    let series = |s: &str| e.catalog().iter().filter(|c| c.series == s).count();
    assert_eq!(
        (
            series("G"),
            series("I"),
            series("W"),
            series("R"),
            series("S")
        ),
        (18, 23, 21, 33, 10)
    );
}

#[test]
fn a_catalog_entry_seeds_its_sign_and_every_entry_lays_out() {
    let e = engine();
    let bend = e.catalog_entry("w01.2").expect("W01.2");
    assert_eq!(bend.name, "Sharp curve — right");
    assert_eq!(
        (bend.kind, bend.class),
        (SignKind::WarningSign, SignClass::Warning)
    );
    let d = e.definition_for(bend);
    assert_eq!((d.code.as_str(), d.speed_kph), ("W01.2", 60));
    assert_eq!(d.symbol_id.as_deref(), Some("bend"));
    assert_eq!(d.mirror, SignMirror::Right);
    assert!(d.destinations.is_empty());

    let stack = e.catalog_entry("G04.1").unwrap();
    let d = e.definition_for(stack);
    assert_eq!(
        (d.kind, d.scheme),
        (SignKind::StackType, ColourScheme::Primary)
    );
    assert_eq!(
        (d.variant, d.background),
        (SignVariant::Primary, Colour::Green)
    );

    for entry in e.catalog() {
        for speed in [30, 60, 100] {
            let def = e.definition_for_speed(entry, speed);
            let layout = e
                .layout(&def)
                .unwrap_or_else(|err| panic!("{} at {speed}: {err}", entry.key));
            assert!(!layout.has_errors(), "{}", entry.key);
            assert!(!layout.drawing.primitives.is_empty(), "{}", entry.key);
            // App.1 §2.1.1 Table 1 for every family: sw 25 / 37.5 / 50 mm.
            let want = match speed {
                30 => 25.0,
                60 => 37.5,
                _ => 50.0,
            };
            assert_eq!(layout.sw_mm, want, "{}", entry.key);
            assert_eq!(layout.x_height_mm, 4.0 * want, "{}", entry.key);
        }
    }
}

// ─── hand-worked layouts ──────────────────────────────────────────────────────

/// App.1 §2.3, Figure 6. Three rows of 8 sw tiles: 1.5 + 2.5 + 3 × 8 + 1.5 + 1.5 = 31 high.
/// The arrow column is the Figure 5 arrow width (3 + 2 × 3 = 9 sw) from 4 to 13, the names
/// start one word space (2.5) after it at 15.5, and rows run ahead, right, left (§2.3.1). Each
/// arrow is fitted to its run so its head takes half of it: 4 sw of the 8 sw row for the ahead
/// arrow, 4.5 sw of the 9 sw column for the side arrows.
#[test]
fn a_stack_sign_has_an_arrow_per_row_in_its_own_column() {
    let e = engine();
    let layout = e
        .layout(&sign(
            SignKind::StackType,
            60,
            vec![
                Destination::new("Kitagata", Direction::Left),
                Destination::new("Kasese", Direction::Ahead),
                Destination::new("Masheruka", Direction::Right),
            ],
        ))
        .unwrap();
    assert!(layout.diagnostics.is_empty());
    assert!(close(layout.drawing.extents.max.y, 31.0));
    let prims = &layout.drawing.primitives;

    let texts: Vec<(&str, f64)> = prims
        .iter()
        .filter_map(|p| match p {
            Primitive::Glyphs { text, contours, .. } => Some((
                text.as_str(),
                contours
                    .iter()
                    .flatten()
                    .map(|q| q.y)
                    .fold(f64::MAX, f64::min),
            )),
            _ => None,
        })
        .collect();
    let names: Vec<&str> = texts.iter().map(|t| t.0).collect();
    assert_eq!(names, ["Kasese", "Masheruka", "Kitagata"]);
    assert!(texts[0].1 > texts[1].1 && texts[1].1 > texts[2].1);

    let depth = 4.0 + 1.0 / 70f64.to_radians().tan();
    let shafts = lines(prims);
    assert_eq!(shafts.len(), 3);
    // Ahead, in the top row (y from 19 to 27): up the column centre line to the head.
    let (from, to, width) = shafts[0];
    assert!(close(width, 4.0 / depth));
    assert!(close(from.x, 8.5) && close(from.y, 19.0) && close(to.y, 27.0 - 4.0));
    // Right, in the middle row: from the column's left edge to the head 4.5 sw short of 13.
    let (from, to, width) = shafts[1];
    assert!(close(width, 4.5 / depth));
    assert!(close(from.x, 4.0) && close(to.x, 13.0 - 4.5) && close(from.y, 15.0));
    // Left, in the bottom row.
    let (from, to, _) = shafts[2];
    assert!(close(from.x, 13.0) && close(to.x, 4.0 + 4.5) && close(from.y, 7.0));

    // The ground is the plain square plate; the heads are the 14-point Figure 5 outline.
    let f = fills(prims);
    assert_eq!((f[0].0.len(), f[0].1), (4, Colour::Green));
    assert_eq!(f.iter().filter(|(o, _)| o.len() == 14).count(), 3);
    assert!(f[1..].iter().all(|(_, c)| *c == Colour::White));
}

/// App.1 §2.1.2: at most four destinations, two per direction. Route numbers are not drawn by
/// this layout.
#[test]
fn the_uganda_limits_and_the_route_number_note_cite_the_uganda_manual() {
    let e = engine();
    let layout = e
        .layout(&sign(
            SignKind::StackType,
            60,
            vec![
                Destination::new("One", Direction::Left),
                Destination::new("Two", Direction::Left).with_order(1),
                Destination::new("Three", Direction::Left).with_order(2),
                Destination::new("Four", Direction::Ahead),
                Destination::new("Five", Direction::Right)
                    .with_route("A109", afrobim_signs::RouteStatus::Primary),
            ],
        ))
        .unwrap();
    assert!(layout.has_errors());
    let codes: Vec<&str> = layout.diagnostics.iter().map(|d| d.code.as_str()).collect();
    assert_eq!(
        codes,
        [
            "MAX_DESTINATIONS",
            "MAX_PER_DIRECTION",
            "ROUTE_NUMBER_NOT_DRAWN"
        ]
    );
    assert_eq!(
        layout.diagnostics[0].message,
        "5 destinations; maximum is 4"
    );
    assert_eq!(
        layout.diagnostics[0].source,
        "Uganda TSM Vol.2 App.1 §2.1.2"
    );
    assert_eq!(
        layout.diagnostics[2].message,
        "Route numbers, patches and panels are not drawn on Stack type signs yet"
    );
    assert_eq!(
        layout.diagnostics[2].source,
        "MoWHC Traffic Signs Manual Vol.2, July 2004, Appendix 1"
    );
}

/// App.1 §2.4, Figure 7: one arrow for the whole sign, spanning the interior (from 3 to 19 on
/// a two-line, 23 sw plate) on the centre line of the 9 sw arrow column. The 16 sw run fits
/// the head into 8 sw, so the stroke is 8 / (4 + 1 / tan 70°).
#[test]
fn a_direction_sign_has_one_stroked_arrow_beside_the_names() {
    let e = engine();
    let layout = e
        .layout(&sign(
            SignKind::DirectionSign,
            60,
            vec![
                Destination::new("Kasese", Direction::Ahead),
                Destination::new("Masheruka", Direction::Right).with_order(1),
            ],
        ))
        .unwrap();
    assert!(layout.diagnostics.is_empty());
    assert!(close(layout.drawing.extents.max.y, 23.0));
    let shafts = lines(&layout.drawing.primitives);
    assert_eq!(shafts.len(), 1);
    let (from, to, width) = shafts[0];
    let depth = 4.0 + 1.0 / 70f64.to_radians().tan();
    assert!(close(width, 8.0 / depth));
    assert!(close(from.x, 8.5) && close(from.y, 3.0));
    assert!(close(to.x, 8.5) && close(to.y, 19.0 - 8.0));
    let f = fills(&layout.drawing.primitives);
    assert_eq!(f[0].0.len(), 4); // the square ground: the Uganda pack draws no border ring
    assert_eq!(f.iter().filter(|(o, _)| o.len() == 14).count(), 1);
}

/// App.1 §2.5, Figure 8: names ranged left from 4, distances ranged right against the side
/// padding, one word space (2.5 sw) between the longest name and the longest distance.
#[test]
fn a_confirmatory_sign_ranges_its_distances_right() {
    let e = engine();
    let two = |with_distances: bool| {
        let mut a = Destination::new("Kampala", Direction::Ahead);
        let mut b = Destination::new("Jinja", Direction::Ahead).with_order(1);
        if with_distances {
            a = a.with_distance("40");
            b = b.with_distance("120");
        }
        e.layout(&sign(SignKind::Confirmatory, 100, vec![a, b]))
            .unwrap()
    };
    let (with, without) = (two(true), two(false));
    assert!(with.diagnostics.is_empty());
    assert_eq!(without.diagnostics.len(), 1);
    assert_eq!(without.diagnostics[0].code, "CONFIRMATORY_NO_DISTANCE");
    assert_eq!(without.diagnostics[0].source, "Uganda TSM Vol.2 App.1 §2.5");
    assert!(close(with.drawing.extents.max.y, 23.0));

    let right_edge = |text: &str| {
        with.drawing
            .primitives
            .iter()
            .find_map(|p| match p {
                Primitive::Glyphs {
                    text: t, contours, ..
                } if t == text => Some(
                    contours
                        .iter()
                        .flatten()
                        .map(|q| q.x)
                        .fold(f64::MIN, f64::max),
                ),
                _ => None,
            })
            .unwrap()
    };
    // Both distances end near the same line (their last glyphs differ only by side bearing),
    // inside the right padding.
    let inner_right = with.drawing.extents.max.x - 4.0;
    for d in ["40", "120"] {
        let edge = right_edge(d);
        assert!(
            edge <= inner_right && edge > inner_right - 1.0,
            "{d}: {edge}"
        );
    }
    // The distance column and its gap widen the plate.
    assert!(with.drawing.extents.max.x > without.drawing.extents.max.x + 2.5);
}

/// Vol.1 §Warning Signs: a 50 sw equilateral triangle, red, with the ground inset by the
/// pack's provisional 1/8 band (6.25 sw); the `.2` plate mirrors the glyph about x = 25.
#[test]
fn a_warning_sign_is_the_plain_triangle_and_mirrors_its_symbol() {
    let e = engine();
    let lay = |mirror: SignMirror, scheme: ColourScheme| {
        let mut s = sign(SignKind::WarningSign, 60, vec![]);
        s.symbol_id = Some("bend".into());
        s.mirror = mirror;
        s.scheme = scheme;
        e.layout(&s).unwrap()
    };
    let left = lay(SignMirror::Left, ColourScheme::Auto);
    let right = lay(SignMirror::Right, ColourScheme::Temporary);
    let height = 25.0 * 3f64.sqrt();
    assert!(close(left.drawing.extents.max.x, 50.0) && close(left.drawing.extents.max.y, height));

    let (l, r) = (
        fills(&left.drawing.primitives),
        fills(&right.drawing.primitives),
    );
    assert_eq!((l[0].1, l[1].1), (Colour::Red, Colour::White));
    assert_eq!((r[0].1, r[1].1), (Colour::Red, Colour::Yellow)); // temporary: the ground only
    assert!(close(l[1].0[0].y, 6.25));
    assert!(l.len() > 2 && l.len() == r.len());
    for ((lo, lc), (ro, rc)) in l[2..].iter().zip(&r[2..]) {
        assert_eq!(lc, rc);
        for (p, q) in lo.iter().zip(ro.iter()) {
            assert!(close(p.x, 50.0 - q.x) && close(p.y, q.y));
        }
    }
}

/// Vol.1 §Regulatory Signs. Speed limit (R49): a red ring 6.25 sw wide round a white ground,
/// the design speed as black numerals. No temporary version recolours the no-entry sign.
#[test]
fn regulatory_discs_follow_their_profile() {
    let e = engine();
    let lay = |kind: SignKind, symbol: &str, speed: u32, scheme: ColourScheme| {
        let mut s = sign(kind, speed, vec![]);
        s.symbol_id = Some(symbol.into());
        s.scheme = scheme;
        e.layout(&s).unwrap()
    };
    let limit = lay(
        SignKind::RegulatoryProhibitory,
        "speed-limit",
        80,
        ColourScheme::Auto,
    );
    assert!(close(limit.drawing.extents.max.x, 50.0) && close(limit.drawing.extents.max.y, 50.0));
    let f = fills(&limit.drawing.primitives);
    assert_eq!(
        (f[0].0.len(), f[0].1, f[1].1),
        (48, Colour::Red, Colour::White)
    );
    assert!(close(f[1].0[0].x, 25.0 + 18.75));
    assert!(limit.drawing.primitives.iter().any(|p| matches!(
        p,
        Primitive::Glyphs { text, ink: Some(Colour::Black), .. } if text == "80"
    )));

    let temporary = lay(
        SignKind::RegulatoryProhibitory,
        "speed-limit",
        80,
        ColourScheme::Temporary,
    );
    assert_eq!(fills(&temporary.drawing.primitives)[1].1, Colour::Yellow);
    let no_entry = lay(
        SignKind::RegulatoryProhibitory,
        "no-entry",
        60,
        ColourScheme::Temporary,
    );
    let f = fills(&no_entry.drawing.primitives);
    assert_eq!((f[0].1, f[1].1), (Colour::White, Colour::Red));

    // End of restriction (R51): four black diagonals half a slash wide (2.5 sw).
    let end = lay(
        SignKind::RegulatoryProhibitory,
        "end-of-restriction",
        60,
        ColourScheme::Auto,
    );
    let bars = lines(&end.drawing.primitives);
    assert_eq!(bars.len(), 4);
    assert!(bars.iter().all(|b| close(b.2, 2.5)));
    assert_eq!(fills(&end.drawing.primitives)[0].1, Colour::Black);

    // Keep left / right (R75): the standard Figure 5 arrow, centred on the disc.
    let pass = lay(
        SignKind::RegulatoryMandatory,
        "pass-left",
        60,
        ColourScheme::Auto,
    );
    let f = fills(&pass.drawing.primitives);
    assert_eq!(
        (f[0].1, f[1].1, f[2].1),
        (Colour::White, Colour::Blue, Colour::White)
    );
    assert_eq!(f[2].0.len(), 12);
    let (sx, sy) = f[2]
        .0
        .iter()
        .fold((0.0, 0.0), |(x, y), p| (x + p.x, y + p.y));
    assert!((sx / 12.0 - 25.0).abs() < 3.0 && (sy / 12.0 - 25.0).abs() < 3.0);
}

// ─── fixture coverage ─────────────────────────────────────────────────────────

#[derive(Deserialize)]
struct Fixture {
    name: String,
    pack: Option<String>,
    definition: SignDefinition,
    diagnostics: Vec<FixtureDiagnostic>,
    primitives: Vec<serde_json::Value>,
}

#[derive(Deserialize)]
struct FixtureDiagnostic {
    code: String,
}

/// The `ug_` fixtures must cover what the port claims: every catalog entry, every kind at two
/// speeds or more, the stroked arrows with the Figure 5 head, and the diagnostics.
#[test]
fn the_ug_fixtures_cover_the_uganda_pack() {
    let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/golden");
    let fixtures: Vec<Fixture> = std::fs::read_dir(&dir)
        .unwrap()
        .map(|e| e.unwrap().path())
        .filter(|p| {
            p.file_name()
                .and_then(|n| n.to_str())
                .is_some_and(|n| n.starts_with("ug_") && n.ends_with(".json"))
        })
        .map(|p| serde_json::from_str(&std::fs::read_to_string(&p).unwrap()).unwrap())
        .collect();

    let (uganda, uk): (Vec<&Fixture>, Vec<&Fixture>) = fixtures
        .iter()
        .partition(|fx| fx.pack.as_deref() == Some("packs/UG-TSM-2004/pack.xml"));
    assert!(uk
        .iter()
        .all(|fx| fx.pack.as_deref() == Some("packs/UK-TSM-2019.json")
            && fx.name.starts_with("ug_ukpack_")));
    assert!(uk.len() >= 10, "{}", uk.len());

    let e = engine();
    for entry in e.catalog() {
        let name = format!("ug_cat_{}", entry.key.to_lowercase().replace('.', "_"));
        let fx = uganda
            .iter()
            .find(|fx| fx.name == name)
            .unwrap_or_else(|| panic!("no fixture {name}"));
        assert_eq!(fx.definition, e.definition_for(entry), "{name}");
    }

    let kinds = [
        SignKind::StackType,
        SignKind::MapTypeCrossroads,
        SignKind::MapTypeRoundabout,
        SignKind::MapTypeTee,
        SignKind::DirectionSign,
        SignKind::Confirmatory,
        SignKind::PlaceIdentification,
        SignKind::LanePreselection,
        SignKind::Diversion,
        SignKind::InformationSign,
        SignKind::WarningSign,
        SignKind::RegulatoryProhibitory,
        SignKind::RegulatoryMandatory,
        SignKind::Stop,
        SignKind::GiveWay,
        SignKind::SupplementaryPlate,
    ];
    let is = |p: &serde_json::Value, kind: &str| p["type"] == kind;
    for kind in kinds {
        let of: Vec<&&Fixture> = uganda
            .iter()
            .filter(|fx| fx.definition.kind == kind)
            .collect();
        let speeds: BTreeSet<u32> = of.iter().map(|fx| fx.definition.speed_kph).collect();
        assert!(of.len() >= 3 && speeds.len() >= 2, "{kind:?}: {speeds:?}");
    }
    // No Uganda sign carries embedded signs: those are not ported.
    assert!(uganda.iter().all(|fx| fx.definition.embedded.is_none()));

    let figure5_head = |p: &serde_json::Value| {
        is(p, "Fill") && p["outline"].as_array().is_some_and(|o| o.len() == 14)
    };
    for kind in [
        SignKind::StackType,
        SignKind::DirectionSign,
        SignKind::LanePreselection,
        SignKind::Diversion,
    ] {
        let of: Vec<&&Fixture> = uganda
            .iter()
            .filter(|fx| fx.definition.kind == kind)
            .collect();
        assert!(
            of.iter()
                .any(|fx| fx.primitives.iter().any(|p| is(p, "Line"))
                    && fx.primitives.iter().any(figure5_head)),
            "{kind:?}"
        );
        assert!(
            of.iter().any(|fx| fx.definition.destinations.is_empty()),
            "{kind:?}"
        );
    }
    // Nothing under the Uganda pack draws the Chapter 7 plate ring or an arc.
    assert!(uganda
        .iter()
        .all(|fx| fx.primitives.iter().all(|p| !is(p, "Arc"))));
    assert!(uganda
        .iter()
        .any(|fx| fx.primitives.iter().any(|p| is(p, "Symbol"))));
    assert!(uganda
        .iter()
        .any(|fx| fx.definition.mirror == SignMirror::Right));
    assert!(uganda
        .iter()
        .any(|fx| fx.definition.scheme == ColourScheme::Temporary));

    let codes: BTreeSet<&str> = uganda
        .iter()
        .flat_map(|fx| fx.diagnostics.iter().map(|d| d.code.as_str()))
        .collect();
    for code in [
        "MAX_DESTINATIONS",
        "MAX_PER_DIRECTION",
        "NEARER_NOT_ABOVE",
        "ROUTE_NUMBER_NOT_DRAWN",
        "SYMBOL_UNRESOLVED",
        "CONFIRMATORY_NO_DISTANCE",
        "MAP_SYMBOL_PROVISIONAL",
        "ROUNDABOUT_PROVISIONAL",
        "MAP_STUB_UNCITED",
    ] {
        assert!(codes.contains(code), "{code}");
    }
}
