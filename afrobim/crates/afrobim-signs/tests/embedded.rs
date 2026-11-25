// SPDX-License-Identifier: LicenseRef-AfroBIM-Proprietary
// Copyright (c) 2026 AfroBIM. All rights reserved.
//! A guard on the `emb_*` golden fixtures (`tests/golden.rs` compares them to the C# engine
//! with every other fixture; this file checks that the set still covers what the embedded-sign
//! port claims) and the editor-facing queries.

use std::collections::BTreeSet;
use std::path::{Path, PathBuf};

use afrobim_signs::{EmbeddedSupport, EmbeddedType, SignDefinition, SignEngine, SignKind};
use serde::Deserialize;

#[derive(Deserialize)]
struct Fixture {
    definition: SignDefinition,
    diagnostics: Vec<FixtureDiagnostic>,
    primitives: Vec<serde_json::Value>,
}

#[derive(Deserialize)]
struct FixtureDiagnostic {
    code: String,
}

fn repo_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../..")
}

fn engine() -> SignEngine {
    let root = repo_root();
    SignEngine::load(&root.join("packs/UK-TSM-2019.json"), &root.join("fonts")).unwrap()
}

fn emb_fixtures() -> Vec<(String, Fixture)> {
    let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/golden");
    let mut out = Vec::new();
    for entry in std::fs::read_dir(&dir).unwrap() {
        let path = entry.unwrap().path();
        let name = path.file_stem().unwrap().to_str().unwrap().to_string();
        if !name.starts_with("emb_") || path.extension().is_none_or(|x| x != "json") {
            continue;
        }
        let text = std::fs::read_to_string(&path).unwrap();
        out.push((name, serde_json::from_str(&text).unwrap()));
    }
    out.sort_by(|a, b| a.0.cmp(&b.0));
    out
}

/// Red fills of a fixture: 3 points for a triangle frame, 48 for a roundel ring.
fn red_frames(fx: &Fixture) -> Vec<usize> {
    fx.primitives
        .iter()
        .filter(|p| p["type"] == "Fill" && p["colour"] == "Red")
        .map(|p| p["outline"].as_array().unwrap().len())
        .collect()
}

#[test]
fn the_emb_fixtures_cover_the_ported_features() {
    let engine = engine();
    let fixtures = emb_fixtures();
    assert!(fixtures.len() >= 33, "found {}", fixtures.len());

    let mut drawn: BTreeSet<(String, bool, bool)> = BTreeSet::new(); // kind, triangle, roundel
    let mut grounds_with_frames: BTreeSet<String> = BTreeSet::new();
    let mut speeds: BTreeSet<u32> = BTreeSet::new();
    let (mut two_together, mut not_placed, mut ignored) = (0, 0, 0);

    for (name, fx) in &fixtures {
        let embeds = fx.definition.embedded.as_deref().unwrap_or(&[]);
        assert!(!embeds.is_empty(), "{name} carries no embedded sign");
        let frames = red_frames(fx);
        let codes: Vec<&str> = fx.diagnostics.iter().map(|d| d.code.as_str()).collect();
        speeds.insert(fx.definition.speed_kph);

        match engine.embedded_support(fx.definition.kind) {
            EmbeddedSupport::Drawn => {
                // One red frame per embedded sign, in the order given.
                let want: Vec<usize> = embeds
                    .iter()
                    .map(|e| match e.kind {
                        EmbeddedType::WarningTriangle => 3,
                        EmbeddedType::RegulatoryRoundel => 48,
                    })
                    .collect();
                assert_eq!(frames, want, "{name}");
                assert!(!codes.contains(&"EMBEDDED_NOT_PLACED"), "{name}");
                drawn.insert((
                    format!("{:?}", fx.definition.kind),
                    want.contains(&3),
                    want.contains(&48),
                ));
                grounds_with_frames.insert(format!("{:?}", fx.definition.background));
                if embeds.len() >= 2 {
                    two_together += 1;
                }
            }
            EmbeddedSupport::NotPlaced => {
                assert!(frames.is_empty(), "{name}");
                let raised = codes.iter().filter(|c| **c == "EMBEDDED_NOT_PLACED");
                assert_eq!(raised.count(), 1, "{name}");
                not_placed += 1;
            }
            EmbeddedSupport::Ignored => {
                assert!(frames.is_empty(), "{name}");
                assert!(!codes.contains(&"EMBEDDED_NOT_PLACED"), "{name}");
                ignored += 1;
            }
        }
    }

    for kind in [
        "DirectionSign",
        "MapTypeCrossroads",
        "MapTypeTee",
        "MapTypeRoundabout",
    ] {
        for (triangle, roundel) in [(true, false), (false, true), (true, true)] {
            assert!(
                drawn.contains(&(kind.to_string(), triangle, roundel)),
                "no {kind} fixture with triangle={triangle} roundel={roundel}"
            );
        }
    }
    for ground in ["Green", "White", "Blue"] {
        assert!(grounds_with_frames.contains(ground), "no {ground} fixture");
    }
    assert!(two_together >= 4 && not_placed >= 3 && ignored >= 4);
    assert!(speeds.len() >= 6, "{speeds:?}");
}

#[test]
fn the_engine_says_where_embedded_signs_are_drawn() {
    let engine = engine();
    assert_eq!(
        SignEngine::embedded_types(),
        [
            EmbeddedType::WarningTriangle,
            EmbeddedType::RegulatoryRoundel
        ]
    );
    for kind in [
        SignKind::DirectionSign,
        SignKind::MapTypeCrossroads,
        SignKind::MapTypeTee,
        SignKind::MapTypeRoundabout,
    ] {
        assert_eq!(
            engine.embedded_support(kind),
            EmbeddedSupport::Drawn,
            "{kind:?}"
        );
    }
    assert_eq!(
        engine.embedded_support(SignKind::StackType),
        EmbeddedSupport::NotPlaced
    );
    for kind in [
        SignKind::Confirmatory,
        SignKind::PlaceIdentification,
        SignKind::LanePreselection,
        SignKind::Diversion,
        SignKind::WarningSign,
        SignKind::RegulatoryProhibitory,
        SignKind::InformationSign,
    ] {
        assert_eq!(
            engine.embedded_support(kind),
            EmbeddedSupport::Ignored,
            "{kind:?}"
        );
    }
    let rules = &engine.pack().embedded;
    assert_eq!(rules.embedded_signs.as_ref().unwrap().height_sw, 20.0);
    assert_eq!(
        rules.embedded_shapes.as_ref().unwrap().roundel_diameter,
        20.0
    );
}
