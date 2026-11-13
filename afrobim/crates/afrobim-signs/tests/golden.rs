//! Golden tests against the C# reference engine.
//!
//! Every `tests/golden/<name>.json` was written by `tools/GoldenDump` (C#): a sign definition
//! laid out by SignCore with the UK pack and the repo's Transport fonts, exactly as the desktop
//! app does it, lettering outlined. The Rust engine must reproduce it: same sizes, extents,
//! diagnostics, primitives in order with the same kinds, colours and text, every coordinate
//! and width within 1e-6 sw, and every glyph contour point within 1e-3 sw.
//!
//! Regenerate the fixtures with `dotnet run` in `tools/GoldenDump`.

use std::path::{Path, PathBuf};

use afrobim_draw::{Colour, Primitive, Pt2};
use afrobim_signs::{Diagnostic, Severity, SignDefinition, SignEngine};
use serde::Deserialize;

/// Tolerance for plate, panel, patch and arrow geometry, in sw.
const GEOMETRY_TOLERANCE: f64 = 1e-6;
/// Tolerance for glyph contour points, in sw.
const GLYPH_TOLERANCE: f64 = 1e-3;

#[derive(Deserialize)]
struct Fixture {
    name: String,
    /// The rule pack the fixture was laid out with, as a path from the repository root; the UK
    /// pack when absent.
    #[serde(default)]
    pack: Option<String>,
    definition: SignDefinition,
    sw_mm: f64,
    x_height_mm: f64,
    extents: FixtureExtents,
    diagnostics: Vec<FixtureDiagnostic>,
    primitives: Vec<FixturePrimitive>,
}

#[derive(Deserialize)]
struct FixtureExtents {
    min: [f64; 2],
    max: [f64; 2],
}

#[derive(Deserialize)]
struct FixtureDiagnostic {
    severity: Severity,
    code: String,
    message: String,
    source: String,
}

#[derive(Deserialize)]
#[serde(tag = "type")]
enum FixturePrimitive {
    Fill {
        colour: Colour,
        outline: Vec<[f64; 2]>,
    },
    Line {
        from: [f64; 2],
        to: [f64; 2],
        width: f64,
        colour: Option<Colour>,
    },
    Arc {
        centre: [f64; 2],
        radius: f64,
        start_deg: f64,
        end_deg: f64,
        colour: Option<Colour>,
    },
    Glyphs {
        text: String,
        ink: Option<Colour>,
        contours: Vec<Vec<[f64; 2]>>,
    },
    Symbol {
        origin: [f64; 2],
        id: String,
        size: f64,
        rotation_deg: f64,
    },
}

fn repo_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../..")
}

fn engine() -> SignEngine {
    let root = repo_root();
    SignEngine::load(&root.join("packs/UK-TSM-2019.json"), &root.join("fonts"))
        .expect("the UK pack and the Transport fonts load")
}

/// One engine per rule pack the fixtures name.
struct Engines {
    uk: SignEngine,
    others: std::collections::BTreeMap<String, SignEngine>,
}

impl Engines {
    fn load(fixtures: &[Fixture]) -> Engines {
        let root = repo_root();
        let mut others = std::collections::BTreeMap::new();
        for pack in fixtures.iter().filter_map(|fx| fx.pack.as_ref()) {
            if !others.contains_key(pack) {
                let engine = SignEngine::load(&root.join(pack), &root.join("fonts"))
                    .unwrap_or_else(|e| panic!("{pack}: {e}"));
                others.insert(pack.clone(), engine);
            }
        }
        Engines {
            uk: engine(),
            others,
        }
    }

    fn of(&self, fx: &Fixture) -> &SignEngine {
        match &fx.pack {
            Some(pack) => &self.others[pack],
            None => &self.uk,
        }
    }
}

fn fixtures() -> Vec<Fixture> {
    let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/golden");
    let mut paths: Vec<PathBuf> = std::fs::read_dir(&dir)
        .unwrap_or_else(|e| panic!("{}: {e}", dir.display()))
        .map(|e| e.unwrap().path())
        .filter(|p| p.extension().is_some_and(|x| x == "json"))
        .collect();
    paths.sort();
    paths
        .iter()
        .map(|p| {
            let text = std::fs::read_to_string(p).unwrap();
            serde_json::from_str(&text).unwrap_or_else(|e| panic!("{}: {e}", p.display()))
        })
        .collect()
}

/// Collects every mismatch of one fixture, so a failure shows the whole picture.
struct Report {
    fixture: String,
    problems: Vec<String>,
    worst_geometry: f64,
    worst_glyph: f64,
}

impl Report {
    fn fail(&mut self, what: String) {
        if self.problems.len() < 25 {
            self.problems.push(what);
        }
    }

    fn number(&mut self, what: &str, got: f64, want: f64, tolerance: f64) {
        let diff = (got - want).abs();
        if tolerance == GLYPH_TOLERANCE {
            self.worst_glyph = self.worst_glyph.max(diff);
        } else {
            self.worst_geometry = self.worst_geometry.max(diff);
        }
        if diff.is_nan() || diff > tolerance {
            self.fail(format!(
                "{what}: got {got}, want {want} (off by {diff:e}, tolerance {tolerance:e})"
            ));
        }
    }

    fn point(&mut self, what: &str, got: &Pt2, want: &[f64; 2], tolerance: f64) {
        self.number(&format!("{what}.x"), got.x, want[0], tolerance);
        self.number(&format!("{what}.y"), got.y, want[1], tolerance);
    }

    fn same<T: PartialEq + std::fmt::Debug>(&mut self, what: &str, got: &T, want: &T) {
        if got != want {
            self.fail(format!("{what}: got {got:?}, want {want:?}"));
        }
    }
}

fn kind(p: &Primitive) -> &'static str {
    match p {
        Primitive::Fill { .. } => "Fill",
        Primitive::Line { .. } => "Line",
        Primitive::Arc { .. } => "Arc",
        Primitive::Glyphs { .. } => "Glyphs",
        Primitive::Symbol { .. } => "Symbol",
    }
}

fn compare_primitive(r: &mut Report, i: usize, got: &Primitive, want: &FixturePrimitive) {
    let at = format!("primitive {i}");
    match (got, want) {
        (
            Primitive::Fill { outline, colour },
            FixturePrimitive::Fill {
                colour: want_colour,
                outline: want_outline,
            },
        ) => {
            r.same(&format!("{at} fill colour"), colour, want_colour);
            r.same(
                &format!("{at} fill point count"),
                &outline.len(),
                &want_outline.len(),
            );
            for (j, (g, w)) in outline.iter().zip(want_outline).enumerate() {
                r.point(&format!("{at} fill point {j}"), g, w, GEOMETRY_TOLERANCE);
            }
        }
        (
            Primitive::Line {
                from,
                to,
                width,
                colour,
            },
            FixturePrimitive::Line {
                from: wf,
                to: wt,
                width: ww,
                colour: wc,
            },
        ) => {
            r.same(&format!("{at} line colour"), colour, wc);
            r.point(&format!("{at} line from"), from, wf, GEOMETRY_TOLERANCE);
            r.point(&format!("{at} line to"), to, wt, GEOMETRY_TOLERANCE);
            r.number(&format!("{at} line width"), *width, *ww, GEOMETRY_TOLERANCE);
        }
        (
            Primitive::Arc {
                centre,
                radius,
                start_deg,
                end_deg,
                colour,
            },
            FixturePrimitive::Arc {
                centre: wc,
                radius: wr,
                start_deg: ws,
                end_deg: we,
                colour: wcol,
            },
        ) => {
            r.same(&format!("{at} arc colour"), colour, wcol);
            r.point(&format!("{at} arc centre"), centre, wc, GEOMETRY_TOLERANCE);
            r.number(
                &format!("{at} arc radius"),
                *radius,
                *wr,
                GEOMETRY_TOLERANCE,
            );
            r.number(
                &format!("{at} arc start"),
                *start_deg,
                *ws,
                GEOMETRY_TOLERANCE,
            );
            r.number(&format!("{at} arc end"), *end_deg, *we, GEOMETRY_TOLERANCE);
        }
        (
            Primitive::Glyphs {
                contours,
                text,
                ink,
            },
            FixturePrimitive::Glyphs {
                text: wt,
                ink: wi,
                contours: wc,
            },
        ) => {
            r.same(&format!("{at} glyph text"), text, wt);
            r.same(&format!("{at} glyph ink"), ink, wi);
            r.same(
                &format!("{at} '{wt}' contour count"),
                &contours.len(),
                &wc.len(),
            );
            for (c, (g, w)) in contours.iter().zip(wc).enumerate() {
                r.same(
                    &format!("{at} '{wt}' contour {c} point count"),
                    &g.len(),
                    &w.len(),
                );
                for (j, (gp, wp)) in g.iter().zip(w).enumerate() {
                    r.point(
                        &format!("{at} '{wt}' contour {c} point {j}"),
                        gp,
                        wp,
                        GLYPH_TOLERANCE,
                    );
                }
            }
        }
        (
            Primitive::Symbol {
                origin,
                id,
                size,
                rotation_deg,
            },
            FixturePrimitive::Symbol {
                origin: wo,
                id: wid,
                size: ws,
                rotation_deg: wr,
            },
        ) => {
            r.same(&format!("{at} symbol id"), id, wid);
            r.point(
                &format!("{at} symbol origin"),
                origin,
                wo,
                GEOMETRY_TOLERANCE,
            );
            r.number(&format!("{at} symbol size"), *size, *ws, GEOMETRY_TOLERANCE);
            r.number(
                &format!("{at} symbol rotation"),
                *rotation_deg,
                *wr,
                GEOMETRY_TOLERANCE,
            );
        }
        (got, want) => {
            let want = match want {
                FixturePrimitive::Fill { .. } => "Fill",
                FixturePrimitive::Line { .. } => "Line",
                FixturePrimitive::Arc { .. } => "Arc",
                FixturePrimitive::Glyphs { .. } => "Glyphs",
                FixturePrimitive::Symbol { .. } => "Symbol",
            };
            r.fail(format!("{at}: got a {}, want a {want}", kind(got)));
        }
    }
}

fn compare(engine: &SignEngine, fx: &Fixture) -> Report {
    let mut r = Report {
        fixture: fx.name.clone(),
        problems: Vec::new(),
        worst_geometry: 0.0,
        worst_glyph: 0.0,
    };
    let layout = match engine.layout(&fx.definition) {
        Ok(l) => l,
        Err(e) => {
            r.fail(format!("layout failed: {e}"));
            return r;
        }
    };

    r.same("sw_mm", &layout.sw_mm, &fx.sw_mm);
    r.same("x_height_mm", &layout.x_height_mm, &fx.x_height_mm);
    r.point(
        "extents.min",
        &layout.drawing.extents.min,
        &fx.extents.min,
        GEOMETRY_TOLERANCE,
    );
    r.point(
        "extents.max",
        &layout.drawing.extents.max,
        &fx.extents.max,
        GEOMETRY_TOLERANCE,
    );

    let want_diags: Vec<Diagnostic> = fx
        .diagnostics
        .iter()
        .map(|d| Diagnostic {
            severity: d.severity,
            code: d.code.clone(),
            message: d.message.clone(),
            source: d.source.clone(),
        })
        .collect();
    r.same("diagnostics", &layout.diagnostics, &want_diags);

    let prims = &layout.drawing.primitives;
    r.same("primitive count", &prims.len(), &fx.primitives.len());
    let got_kinds: Vec<&str> = prims.iter().map(kind).collect();
    r.same(
        "primitive kinds",
        &got_kinds,
        &fx.primitives
            .iter()
            .map(|p| match p {
                FixturePrimitive::Fill { .. } => "Fill",
                FixturePrimitive::Line { .. } => "Line",
                FixturePrimitive::Arc { .. } => "Arc",
                FixturePrimitive::Glyphs { .. } => "Glyphs",
                FixturePrimitive::Symbol { .. } => "Symbol",
            })
            .collect(),
    );
    for (i, (got, want)) in prims.iter().zip(&fx.primitives).enumerate() {
        compare_primitive(&mut r, i, got, want);
    }
    r
}

#[test]
fn every_fixture_matches_the_reference_engine() {
    let fixtures = fixtures();
    let engines = Engines::load(&fixtures);
    assert!(
        fixtures.len() >= 7,
        "expected the golden fixtures, found {}",
        fixtures.len()
    );

    let mut failures = Vec::new();
    let (mut worst_geometry, mut worst_glyph) = (0.0f64, 0.0f64);
    for fx in &fixtures {
        let report = compare(engines.of(fx), fx);
        worst_geometry = worst_geometry.max(report.worst_geometry);
        worst_glyph = worst_glyph.max(report.worst_glyph);
        if !report.problems.is_empty() {
            failures.push(format!(
                "{}:\n  {}",
                report.fixture,
                report.problems.join("\n  ")
            ));
        }
    }
    println!(
        "{} fixtures; worst non-glyph deviation {worst_geometry:e} sw, worst glyph deviation {worst_glyph:e} sw",
        fixtures.len()
    );
    assert!(
        failures.is_empty(),
        "{} fixture(s) differ from the C# engine:\n{}",
        failures.len(),
        failures.join("\n")
    );
}

/// The `dir_` fixtures must cover the destination-column family: every kind, its diagnostics,
/// the bordered plate, the exit band and the stroked lane and diversion arrows.
#[test]
fn the_dir_fixtures_cover_the_destination_column_family() {
    use afrobim_signs::SignKind;

    let fixtures: Vec<Fixture> = fixtures()
        .into_iter()
        .filter(|fx| fx.name.starts_with("dir_"))
        .collect();
    let of = |kind: SignKind| -> Vec<&Fixture> {
        fixtures
            .iter()
            .filter(|fx| fx.definition.kind == kind)
            .collect()
    };
    let draws = |fxs: &[&Fixture], f: &dyn Fn(&FixturePrimitive) -> bool| {
        fxs.iter().any(|fx| fx.primitives.iter().any(f))
    };
    let raises = |fxs: &[&Fixture], code: &str| {
        fxs.iter()
            .any(|fx| fx.diagnostics.iter().any(|d| d.code == code))
    };
    let is_line = |p: &FixturePrimitive| matches!(p, FixturePrimitive::Line { .. });
    // The §2.15 border: a plate ring of four 12-segment corner arcs.
    let is_plate_ring = |p: &FixturePrimitive| matches!(p, FixturePrimitive::Fill { outline, .. } if outline.len() == 52);
    // The Figure 4-4 arrow: one filled outline of nine vertices.
    let is_chevron = |p: &FixturePrimitive| matches!(p, FixturePrimitive::Fill { outline, .. } if outline.len() == 9);

    let kinds = [
        SignKind::DirectionSign,
        SignKind::Confirmatory,
        SignKind::PlaceIdentification,
        SignKind::LanePreselection,
        SignKind::Diversion,
    ];
    for kind in kinds {
        let fxs = of(kind);
        assert!(fxs.len() >= 3, "{kind:?}: {} fixtures", fxs.len());
        assert!(
            fxs.iter().any(|fx| fx.definition.destinations.is_empty()),
            "{kind:?}"
        );
        assert!(
            fxs.iter().any(|fx| !fx.diagnostics.is_empty()),
            "{kind:?} raises no diagnostic"
        );
        let speeds: std::collections::BTreeSet<u32> =
            fxs.iter().map(|fx| fx.definition.speed_kph).collect();
        assert!(speeds.len() >= 2, "{kind:?}: {speeds:?}");
    }
    assert_eq!(
        fixtures.len(),
        kinds.iter().map(|k| of(*k).len()).sum::<usize>()
    );

    let direction = of(SignKind::DirectionSign);
    assert!(draws(&direction, &is_plate_ring) && draws(&direction, &is_chevron));
    assert!(!draws(&direction, &is_line));
    for code in [
        "MAX_DESTINATIONS",
        "MAX_PER_DIRECTION",
        "NEARER_NOT_ABOVE",
        "TILE_WIDTH_FROM_FONT",
        "MOTORWAY_ALPHABET_NOT_MODELLED",
    ] {
        assert!(raises(&direction, code), "{code}");
    }
    for colour in [Colour::Green, Colour::White, Colour::Blue] {
        assert!(
            direction.iter().any(|fx| fx
                .primitives
                .iter()
                .any(|p| matches!(p, FixturePrimitive::Fill { colour: c, outline } if *c == colour && outline.len() == 52))),
            "{colour:?} direction plate"
        );
    }

    let confirmatory = of(SignKind::Confirmatory);
    assert!(draws(&confirmatory, &is_plate_ring));
    assert!(!draws(&confirmatory, &is_line) && !draws(&confirmatory, &is_chevron));
    assert!(raises(&confirmatory, "CONFIRMATORY_NO_DISTANCE"));
    assert!(confirmatory.iter().any(|fx| fx
        .definition
        .destinations
        .iter()
        .any(|d| d.distance.is_some() && d.route_number.is_some())));
    assert!(draws(&confirmatory, &|p| matches!(
        p,
        FixturePrimitive::Fill {
            colour: Colour::Brown,
            ..
        }
    )));

    let place = of(SignKind::PlaceIdentification);
    assert!(draws(&place, &|p| matches!(
        p,
        FixturePrimitive::Fill {
            colour: Colour::Grey,
            ..
        }
    )));
    assert!(!draws(&place, &is_plate_ring));
    assert!(raises(&place, "ROUTE_NUMBER_NOT_DRAWN"));

    for kind in [SignKind::LanePreselection, SignKind::Diversion] {
        let fxs = of(kind);
        assert!(draws(&fxs, &is_line), "{kind:?}");
        // The triangle route-arm head.
        assert!(
            draws(
                &fxs,
                &|p| matches!(p, FixturePrimitive::Fill { outline, .. } if outline.len() == 3)
            ),
            "{kind:?}"
        );
        assert!(raises(&fxs, "ROUTE_NUMBER_NOT_DRAWN"), "{kind:?}");
        for dir in [
            afrobim_signs::Direction::Ahead,
            afrobim_signs::Direction::Left,
            afrobim_signs::Direction::Right,
        ] {
            assert!(
                fxs.iter().any(|fx| fx
                    .definition
                    .destinations
                    .iter()
                    .any(|d| d.direction == dir)),
                "{kind:?} {dir:?}"
            );
        }
    }
    assert!(draws(&of(SignKind::Diversion), &|p| matches!(
        p,
        FixturePrimitive::Fill {
            colour: Colour::Yellow,
            ..
        }
    )));
}

/// The fixtures must cover what the port claims: glyphs in both faces, panels and patches,
/// placeholders and diagnostics. Guards against a regenerated set that silently lost a case.
#[test]
fn the_fixtures_cover_the_ported_features() {
    let fixtures = fixtures();
    let has = |f: &dyn Fn(&FixturePrimitive) -> bool| {
        fixtures.iter().any(|fx| fx.primitives.iter().any(f))
    };
    assert!(has(&|p| matches!(
        p,
        FixturePrimitive::Glyphs {
            ink: Some(Colour::White),
            ..
        }
    )));
    assert!(has(&|p| matches!(
        p,
        FixturePrimitive::Glyphs {
            ink: Some(Colour::Black),
            ..
        }
    )));
    assert!(has(&|p| matches!(
        p,
        FixturePrimitive::Glyphs {
            ink: Some(Colour::Yellow),
            ..
        }
    )));
    assert!(has(&|p| matches!(
        p,
        FixturePrimitive::Fill {
            colour: Colour::Blue,
            ..
        }
    )));
    assert!(has(&|p| matches!(
        p,
        FixturePrimitive::Fill {
            colour: Colour::Brown,
            ..
        }
    )));
    assert!(has(&|p| matches!(p, FixturePrimitive::Symbol { .. })));
    // Pictogram bars and plate arrows are stroked lines. Nothing ported draws an arc; the
    // fixture format still carries them.
    assert!(has(&|p| matches!(p, FixturePrimitive::Line { .. })));
    assert!(!has(&|p| matches!(p, FixturePrimitive::Arc { .. })));
    assert!(fixtures
        .iter()
        .any(|fx| fx.diagnostics.iter().any(|d| d.severity == Severity::Error)));
    assert!(fixtures
        .iter()
        .any(|fx| fx.definition.destinations.is_empty()));
    let speeds: std::collections::BTreeSet<u32> =
        fixtures.iter().map(|fx| fx.definition.speed_kph).collect();
    assert!(speeds.len() >= 4, "{speeds:?}");
}
