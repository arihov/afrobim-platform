//! Road-sign layout in stroke widths (sw): a Rust port of the SignCore C# engine.
//!
//! Scope today: UK **stack-type** advance direction signs (UK TSM Chapter 7 §4.2–4.3) with
//! the `UK-TSM-2019` rule pack, laid out exactly as the C# reference engine lays them out and
//! with lettering converted to Transport glyph outlines. Other sign kinds return
//! [`SignError::Unsupported`].
//!
//! Also laid out, with the same pack and against the same reference: rectangular direction
//! signs (§7.11), route confirmatory signs (§8), and the C# engine's place-identification,
//! lane pre-selection and temporary diversion signs.
//! Also ported, for the same pack: the sign catalog ([`SignEngine::catalog`] and
//! [`SignEngine::definition_for`]), the UK warning and regulatory signs drawn from TSRGD 2016
//! pack data with their associated plates, supplementary plates, information signs, and the
//! drawn pictogram library for destination symbols.
//!
//! ```no_run
//! use std::path::Path;
//! use afrobim_signs::{Destination, Direction, SignDefinition, SignEngine};
//!
//! let engine = SignEngine::load(Path::new("packs/UK-TSM-2019.json"), Path::new("fonts"))?;
//! let sign = SignDefinition::stack(
//!     "ADS-001",
//!     60,
//!     vec![Destination::new("Kasese", Direction::Ahead), Destination::new("Masheruka", Direction::Right)],
//! );
//! let layout = engine.layout(&sign)?;
//! let in_mm = layout.drawing.scaled_to_origin(layout.sw_mm);
//! # let _ = in_mm;
//! # Ok::<(), afrobim_signs::SignError>(())
//! ```

use std::path::{Path, PathBuf};

use afrobim_draw::{Drawing, Extents, Pt2};

pub mod arrow;
mod column;
mod confirmatory;
pub mod dir_rules;
mod direction;
mod diversion;
mod embedded;
mod family;
pub mod font;
mod lane;
mod map_plate;
pub mod map_rules;
mod map_symbol;
pub mod measurer;
mod model;
mod outliner;
mod overlap;
pub mod pack;
mod place;
pub mod pictogram_library;
mod prim;
mod route_head;
mod route_numbers;
mod sign_plate;
mod stack;
mod std_arrow;
mod std_catalog;
mod std_info;
pub mod std_pack;
mod std_plate;
mod std_signs;
mod std_tsrgd;
mod symbol;

pub use model::{
    Colour, ColourScheme, Destination, Diagnostic, Direction, EmbeddedSign, EmbeddedType, FontFace,
    InformationTemplate, RoadClass, RouteStatus, Severity, SignClass, SignDefinition, SignKind,
    SignLocation, SignMirror, SignSide, SignVariant,
};

pub use std_catalog::{CatalogEntry, SignSizing};

pub use embedded::{EmbeddedRules, EmbeddedShapeRules, EmbeddedSignRules, EmbeddedSupport};

use font::FontMetrics;
use measurer::TextTileMeasurer;
use pack::RulePack;
use prim::Prim;

#[derive(Debug, thiserror::Error)]
pub enum SignError {
    /// The rule pack could not be read or failed validation.
    #[error("{0}")]
    Pack(String),
    /// A font file could not be read.
    #[error("{0}")]
    Font(String),
    /// A legend uses a character the font has no glyph for. Silent zero-width text would
    /// produce a grossly undersized sign, so this is an error, not a default.
    #[error("No glyph for character '{character}' (U+{:04X}) in font '{font}'.", *.character as u32)]
    MissingGlyph { character: char, font: String },
    /// The sign needs something this port does not do yet.
    #[error("Not supported: {0}")]
    Unsupported(String),
}

/// A rule pack and the fonts it names, ready to lay signs out.
#[derive(Debug, Clone)]
pub struct SignEngine {
    pack: RulePack,
    medium: FontMetrics,
    heavy: FontMetrics,
    /// The standard-sign sections of the pack (catalog, TSRGD bodies, pictograms and sizes).
    std: std_pack::StdRules,
    /// The pack's sign catalog, in pack order.
    catalog: Vec<CatalogEntry>,
}

/// A laid-out sign.
#[derive(Debug, Clone, PartialEq)]
pub struct SignLayout {
    /// The sign in stroke widths, Y up, lower-left of the plate on the origin.
    pub drawing: Drawing,
    /// 1 sw in mm for this sign.
    pub sw_mm: f64,
    pub x_height_mm: f64,
    pub diagnostics: Vec<Diagnostic>,
}

impl SignLayout {
    /// True when a diagnostic of `Severity::Error` was raised.
    pub fn has_errors(&self) -> bool {
        self.diagnostics
            .iter()
            .any(|d| d.severity == Severity::Error)
    }
}

impl SignEngine {
    /// `pack_path`: a rule pack JSON file; `fonts_dir`: folder holding the Transport TTFs named
    /// in the pack (`fonts.onDark.file` and `fonts.onLight.file`).
    pub fn load(pack_path: &Path, fonts_dir: &Path) -> Result<SignEngine, SignError> {
        let pack = RulePack::load(pack_path)?;
        let medium = FontMetrics::load(&find_font(fonts_dir, &pack.fonts.on_dark.file))?;
        let heavy = FontMetrics::load(&find_font(fonts_dir, &pack.fonts.on_light.file))?;
        let std = std_pack::StdRules::load(pack_path, &pack)?;
        let catalog = std_catalog::entries(&std);
        Ok(SignEngine {
            pack,
            medium,
            heavy,
            std,
            catalog,
        })
    }

    pub fn pack_id(&self) -> &str {
        &self.pack.id
    }

    pub fn pack_name(&self) -> &str {
        &self.pack.name
    }

    /// The loaded rule pack.
    pub fn pack(&self) -> &RulePack {
        &self.pack
    }

    /// Lay the sign out in stroke widths, lettering as glyph outlines.
    ///
    /// Follows the C# desktop app: `SignLayout.Layout` with a measurer holding both faces,
    /// then `TextOutliner.Apply`.
    pub fn layout(&self, sign: &SignDefinition) -> Result<SignLayout, SignError> {
        // Map-type signs (UK TSM Chapter 7 §5): map_symbol.rs.
        if map_symbol::is_map_type(sign.kind) {
            return self.layout_map_type(sign);
        }
        // Information, warning, regulatory and supplementary signs: std_signs.rs.
        if std_signs::handles(sign.kind) {
            return std_signs::layout(self, sign);
        }
        let pack = &self.pack;
        let measurer = TextTileMeasurer::new(&self.medium, &pack.panel, Some(&self.heavy));

        let mut result = match sign.kind {
            SignKind::StackType => stack::StackSignLayout::new(pack, &measurer)?.layout(sign)?,
            SignKind::DirectionSign => direction::layout(pack, &measurer, sign)?,
            SignKind::Confirmatory => confirmatory::layout(pack, &measurer, sign)?,
            SignKind::PlaceIdentification => place::layout(pack, &measurer, sign)?,
            SignKind::LanePreselection => lane::layout(pack, &measurer, sign)?,
            SignKind::Diversion => diversion::layout(pack, &measurer, sign)?,
            _ => {
                return Err(SignError::Unsupported(format!(
                    "sign kind {} ('{}'): its layout is not ported",
                    sign.kind.name(),
                    sign.code
                )))
            }
        };
        let mut diags = std::mem::take(&mut result.diagnostics);

        // A character the pack's tile table does not list was measured by the font; say so.
        for prim in &result.prims {
            let Prim::Text(t) = prim else { continue };
            let missing = measurer.font_fallbacks(&t.text, t.face)?;
            let (false, Some(table)) =
                (missing.is_empty(), measurer::table_for(&pack.panel, t.face))
            else {
                continue;
            };
            let listed: Vec<String> = missing.iter().map(|c| format!("'{c}'")).collect();
            diags.push(Diagnostic::warning(
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

        // Geometry overlap check: legend tiles against the arrows the layout registered, and
        // both against the border.
        diags.extend(overlap::check(pack, &measurer, &result)?);

        // Route numbers, patches and panels are drawn by the panelled (UK) stack-type layout
        // and, in a pack with block rules, by direction and confirmatory signs, which reuse
        // its destination engine. Anywhere else, say so rather than drop them silently.
        let draws_route_numbers = match sign.kind {
            SignKind::StackType => pack.stack.is_some() && pack.blocks.is_some(),
            SignKind::DirectionSign | SignKind::Confirmatory => pack.blocks.is_some(),
            _ => false,
        };
        if !draws_route_numbers
            && sign.destinations.iter().any(|d| {
                d.route_number.as_ref().is_some_and(|n| !n.is_empty())
                    || d.route != RouteStatus::Unspecified
            })
        {
            diags.push(Diagnostic::warning(
                "ROUTE_NUMBER_NOT_DRAWN",
                format!(
                    "Route numbers, patches and panels are not drawn on {} signs yet",
                    family::label(sign.kind)
                ),
                pack.clause("routeNumbers", &pack.source),
            ));
        }

        // UK TSM Chapter 7 §10.11: most route numbers on motorway signs use the enlarged
        // Motorway alphabet, which the engine does not have, so they are drawn in Transport
        // Medium. Bracketed numbers and numbers on all-purpose road panels are Transport Medium
        // anyway (§10.2, §10.15-10.17); flag only an unbracketed number standing on the blue
        // motorway ground.
        if let (true, Some(blocks)) = (draws_route_numbers, &pack.blocks) {
            let is_motorway =
                stack::scheme_of(sign, pack).is_some_and(|s| s.eq_ignore_ascii_case("motorway"));
            if let (true, Some(motorway)) = (
                is_motorway,
                pack.palettes.as_ref().and_then(|p| p.find("motorway")),
            ) {
                let mut on_ground: Vec<&str> = Vec::new();
                for d in &sign.destinations {
                    let Some(n) = d.route_number.as_deref().filter(|n| !n.is_empty()) else {
                        continue;
                    };
                    let on_motorway_ground = blocks
                        .colour_for(d.route)
                        .is_none_or(|panel| panel == motorway.ground);
                    if n.trim_start().starts_with('(') || !on_motorway_ground {
                        continue;
                    }
                    let n = n.trim();
                    if !on_ground.iter().any(|o| o.eq_ignore_ascii_case(n)) {
                        on_ground.push(n);
                    }
                }
                if !on_ground.is_empty() {
                    diags.push(Diagnostic::warning(
                        "MOTORWAY_ALPHABET_NOT_MODELLED",
                        format!(
                            "Route number(s) {} drawn in Transport Medium; the Motorway alphabet and its block spacing \
                             (§10.11-10.12) are not modelled",
                            on_ground.join(", ")
                        ),
                        pack.clause("motorwayAlphabet", &pack.source),
                    ));
                }
            }
        }

        // Lettering as Transport glyph outlines.
        let outliner = outliner::TextOutliner {
            measurer: &measurer,
            medium: &self.medium,
            heavy: &self.heavy,
            panel: &pack.panel,
        };
        let primitives = outliner.apply(result.prims, &mut diags)?;

        // The x-height / stroke-width band: the family's own table when the pack has one (UK
        // Appendix E), otherwise the pack-wide speed table.
        let size = pack::text_size_for_family(pack, sign.kind.family_token(), sign.speed_kph)?;

        Ok(SignLayout {
            drawing: Drawing {
                primitives,
                extents: Extents {
                    min: Pt2::new(0.0, 0.0),
                    max: Pt2::new(result.width, result.height),
                },
            },
            sw_mm: size.sw_mm,
            x_height_mm: size.x_height_mm,
            diagnostics: diags,
        })
    }
}

/// The font file in `dir`: the name as the pack spells it, or the same name in another letter
/// case (packs are written on Windows, where `TRANM___.TTF` and `tranm___.ttf` are one file).
fn find_font(dir: &Path, file: &str) -> PathBuf {
    let exact = dir.join(file);
    if exact.is_file() {
        return exact;
    }
    std::fs::read_dir(dir)
        .ok()
        .and_then(|entries| {
            entries.flatten().map(|e| e.path()).find(|p| {
                p.file_name()
                    .and_then(|n| n.to_str())
                    .is_some_and(|n| n.eq_ignore_ascii_case(file))
            })
        })
        .unwrap_or(exact)
}
