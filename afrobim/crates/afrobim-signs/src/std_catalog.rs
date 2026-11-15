//! The sign catalog: the rule pack's registered sign pages and the ready-to-lay-out
//! [`SignDefinition`] each one seeds.
//!
//! Port of C# `CatalogSign` (the seed builder shared by the desktop app's standard-sign browser
//! and the CLI `sign` command), `SignFamily.ToKind` / `ToScheme` and `SignRef`. Picking a sign
//! page from the catalog produces a sign with the right layout family, colour scheme, route
//! variant (read back from the `.1`/`.2` key suffix), an initial design speed and ground colour,
//! and placeholder destinations the designer then edits.

use serde::{Deserialize, Serialize};

use crate::model::{
    Colour, ColourScheme, Destination, Direction, InformationTemplate, RoadClass, SignClass,
    SignDefinition, SignKind, SignMirror, SignVariant,
};
use crate::std_pack::{PackCatalogEntry, PackSignSizing, StdRules};
use crate::SignEngine;

/// The size of a warning or regulatory sign by 85th percentile speed band, as the pack tables
/// it for a catalog entry. `by_band_mm` has one size per speed band of the pack (for the UK
/// pack: up to 20, 30, 40, 50 and 60 mph, then over 60 mph, held in km/h); `None` means the
/// size table gives no size for that band. `dimension` names what the size measures: a
/// triangle's or octagon's `height`, a disc's `diameter`. `permitted_mm` are the sizes TSRGD
/// 2016 prescribes for the diagram; `alternative_by_band_mm` the bracketed alternatives of the
/// size table. `source` cites the table.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SignSizing {
    pub dimension: String,
    pub by_band_mm: Vec<Option<f64>>,
    pub alternative_by_band_mm: Option<Vec<Option<f64>>>,
    pub permitted_mm: Option<Vec<f64>>,
    pub source: String,
}

impl SignSizing {
    fn from_pack(s: &PackSignSizing) -> SignSizing {
        SignSizing {
            dimension: s.dimension.clone(),
            by_band_mm: s.by_band_mm.clone(),
            alternative_by_band_mm: s.alternative_by_band_mm.clone(),
            permitted_mm: s.permitted_mm.clone(),
            source: s.source.clone(),
        }
    }
}

/// One registered sign page of the rule pack, for a sign library.
///
/// `key`, `name`, `family`, `series`, `scheme`, `symbol_id`, `embedded_symbols`, `template`,
/// `diagram`, `citation` and `sizes` are the pack's own fields (C# `SignCatalogEntry`). `kind`
/// is the layout family `family` names and `class` the Vol.1 §B class of the page, both as
/// [`SignEngine::definition_for`] sets them on the sign.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CatalogEntry {
    /// The canonical sign-page reference, e.g. `UK04.1` or `D504.1`.
    pub key: String,
    /// The sign's title, e.g. "Crossroads ahead (TSRGD diagram 504.1)".
    pub name: String,
    /// The layout token the pack writes, e.g. `stack`, `warning`, `regulatory-mandatory`.
    pub family: String,
    /// The sign-page series letters, e.g. `UK` or `D`.
    pub series: String,
    /// The colour scheme the pack names, e.g. `primary`; empty when it names none.
    pub scheme: String,
    /// The layout family `family` resolves to.
    pub kind: SignKind,
    /// The class of the sign page.
    pub class: SignClass,
    /// The symbol the page draws; for a TSRGD sign, its diagram number.
    pub symbol_id: Option<String>,
    pub embedded_symbols: Option<Vec<String>>,
    /// An information page's plate template, when the pack names one.
    pub template: Option<String>,
    /// The TSRGD 2016 diagram number of a UK warning or regulatory sign.
    pub diagram: Option<String>,
    /// The schedule item the diagram is prescribed by.
    pub citation: Option<String>,
    /// The sign's size by speed band, when the pack tables one.
    pub sizes: Option<SignSizing>,
}

impl CatalogEntry {
    pub(crate) fn from_pack(e: &PackCatalogEntry) -> CatalogEntry {
        let kind = kind_of_family(&e.family);
        CatalogEntry {
            key: e.key.clone(),
            name: e.name.clone(),
            family: e.family.clone(),
            series: e.series.clone(),
            scheme: e.scheme.clone(),
            kind,
            class: class_for(&e.series, kind),
            symbol_id: e.symbol_id.clone(),
            embedded_symbols: e.embedded_symbols.clone(),
            template: e.template.clone(),
            diagram: e.diagram.clone(),
            citation: e.citation.clone(),
            sizes: e.sizes.as_ref().map(SignSizing::from_pack),
        }
    }
}

/// Every catalog entry of the pack, in pack order.
pub(crate) fn entries(std: &StdRules) -> Vec<CatalogEntry> {
    std.catalog
        .iter()
        .flat_map(|c| &c.entries)
        .map(CatalogEntry::from_pack)
        .collect()
}

impl SignEngine {
    /// The rule pack's sign catalog, in pack order: every standard sign the pack registers.
    /// Empty for a pack without a catalog.
    pub fn catalog(&self) -> &[CatalogEntry] {
        &self.catalog
    }

    /// The catalog entry with this key (case is not significant).
    pub fn catalog_entry(&self, key: &str) -> Option<&CatalogEntry> {
        self.catalog
            .iter()
            .find(|e| e.key.eq_ignore_ascii_case(key))
    }

    /// The sign a catalog entry seeds, at the default design speed of its family
    /// (C# `CatalogSign.Build`).
    pub fn definition_for(&self, entry: &CatalogEntry) -> SignDefinition {
        build(entry, None)
    }

    /// As [`SignEngine::definition_for`], at a chosen design speed.
    pub fn definition_for_speed(&self, entry: &CatalogEntry, speed_kph: u32) -> SignDefinition {
        build(entry, Some(speed_kph))
    }

    /// The upper bounds, in km/h, of the speed bands that size the pack's warning and
    /// regulatory signs: the bands [`SignSizing::by_band_mm`] is indexed by. The last band is
    /// open (`None`). Empty for a pack without a size table.
    pub fn sign_size_bands_kph(&self) -> &[Option<i64>] {
        self.std
            .sign_sizes
            .as_ref()
            .map_or(&[], |s| s.speed_bands_kph.as_slice())
    }

    /// The size in mm (triangle or octagon height, disc diameter) the pack tables for a
    /// warning or regulatory sign at its design speed, or `None` when the sign is not drawn
    /// from TSRGD pack data or its size table gives no size for the speed
    /// (C# `TsrgdSign.SizeMm`).
    pub fn sign_size_mm(&self, sign: &SignDefinition) -> Option<f64> {
        let art = crate::std_tsrgd::resolve(&self.std, sign)?;
        crate::std_tsrgd::size_mm(&self.std, sign, &art)
    }
}

/// The sign a catalog entry seeds, without an engine: the same definition as
/// [`SignEngine::definition_for`] (at the family's default design speed) or
/// [`SignEngine::definition_for_speed`] (when `speed_kph` is given). It needs only the entry, so
/// code that plans signs before any fonts are loaded can build the definition.
pub fn definition_from_entry(entry: &CatalogEntry, speed_kph: Option<u32>) -> SignDefinition {
    build(entry, speed_kph)
}

/// C# `CatalogSign.Build`.
pub(crate) fn build(entry: &CatalogEntry, speed_kph: Option<u32>) -> SignDefinition {
    let kind = kind_of_family(&entry.family);
    let scheme = scheme_named(&entry.scheme);
    let sign_ref = SignRef::parse(&entry.key);
    let variant = sign_ref
        .as_ref()
        .and_then(SignRef::variant)
        .unwrap_or(SignVariant::Secondary);
    // W/R mirror and Supplementary line count resolve from the same parsed key.
    let mirror = sign_ref.as_ref().map_or(SignMirror::None, SignRef::mirror);
    let text_lines = sign_ref.as_ref().and_then(SignRef::text_lines);

    // Information plates are symbol-driven; fall back to a drawable glyph when the catalog
    // entry has not registered one so the preview is never blank.
    let is_info = kind == SignKind::InformationSign;
    let symbol_id = if is_info {
        Some(
            entry
                .symbol_id
                .clone()
                .unwrap_or_else(|| "aircraft".to_string()),
        )
    } else {
        entry.symbol_id.clone()
    };

    let template = resolve_info_template(entry, kind);

    SignDefinition {
        code: entry.key.clone(),
        kind,
        speed_kph: speed_kph.unwrap_or_else(|| default_speed_kph(kind)),
        approach_class: RoadClass::Primary,
        destinations: sample_destinations(kind),
        background: ground_for(scheme),
        class: class_for(&entry.series, kind),
        scheme,
        variant,
        symbol_id,
        embedded: None,
        catalog_key: Some(entry.key.clone()),
        info_template: template,
        info_arrow: (is_info && template == InformationTemplate::ServicePanel)
            .then_some(Direction::Right),
        info_distance: None,
        mirror,
        text_lines,
        location: None,
    }
}

/// The layout family a pack's family token names (C# `SignFamily.ToKind`). An unknown token is
/// a stack-type sign, as in C#.
pub(crate) fn kind_of_family(family: &str) -> SignKind {
    match family.trim().to_lowercase().as_str() {
        "stack" | "stack-type" => SignKind::StackType,
        "map" | "map-crossroads" | "crossroads" => SignKind::MapTypeCrossroads,
        "map-roundabout" | "roundabout" => SignKind::MapTypeRoundabout,
        "map-tee" | "tee" | "t-junction" => SignKind::MapTypeTee,
        "direction" | "direction-sign" => SignKind::DirectionSign,
        "confirmatory" => SignKind::Confirmatory,
        "place" | "place-id" | "place-identification" => SignKind::PlaceIdentification,
        "lane" | "lane-preselection" | "lane-distributor" => SignKind::LanePreselection,
        "diversion" => SignKind::Diversion,
        "information" | "info" | "service" | "tourist" => SignKind::InformationSign,
        "warning" => SignKind::WarningSign,
        "regulatory-prohibitory" | "prohibitory" => SignKind::RegulatoryProhibitory,
        "regulatory-mandatory" | "mandatory" => SignKind::RegulatoryMandatory,
        "stop" => SignKind::Stop,
        "give-way" | "giveway" => SignKind::GiveWay,
        "supplementary" => SignKind::SupplementaryPlate,
        _ => SignKind::StackType,
    }
}

/// The colour scheme a pack's scheme name means (C# `SignFamily.ToScheme`).
pub(crate) fn scheme_named(scheme: &str) -> ColourScheme {
    match scheme.trim().to_lowercase().as_str() {
        "primary" => ColourScheme::Primary,
        "secondary" => ColourScheme::Secondary,
        "info" | "information" => ColourScheme::Information,
        "tourist" => ColourScheme::Tourist,
        "temp" | "temporary" | "diversion" => ColourScheme::Temporary,
        "motorway" => ColourScheme::Motorway,
        _ => ColourScheme::Auto,
    }
}

/// Which App.3 §5 plate an information page uses. An explicit entry template wins; otherwise
/// the standard's own numbering groups decide: road and traffic symbols (I01–I10) sit as a
/// single plate on the coloured ground, while services, places and tourist pages (I21 onward)
/// carry a white sub-panel with an optional arrow/distance.
pub(crate) fn resolve_info_template(entry: &CatalogEntry, kind: SignKind) -> InformationTemplate {
    if kind != SignKind::InformationSign {
        return InformationTemplate::ServicePanel;
    }
    match entry
        .template
        .as_deref()
        .map(|t| t.trim().to_lowercase())
        .as_deref()
    {
        Some("symbol-plate" | "plate" | "plain" | "symbol") => {
            return InformationTemplate::SymbolPlate
        }
        Some("service-panel" | "panel" | "service") => return InformationTemplate::ServicePanel,
        _ => {}
    }
    let number = SignRef::parse(&entry.key).map_or(0, |r| r.number);
    if number <= 20 {
        InformationTemplate::SymbolPlate
    } else {
        InformationTemplate::ServicePanel
    }
}

/// Vol.1 §B class implied by the layout family, else by the sign-page series letter.
pub(crate) fn class_for(series: &str, kind: SignKind) -> SignClass {
    match kind {
        SignKind::InformationSign => SignClass::Information,
        SignKind::WarningSign => SignClass::Warning,
        SignKind::RegulatoryProhibitory
        | SignKind::RegulatoryMandatory
        | SignKind::Stop
        | SignKind::GiveWay => SignClass::Regulatory,
        SignKind::SupplementaryPlate => SignClass::Supplementary,
        _ => match series.trim().to_uppercase().as_str() {
            "I" => SignClass::Information,
            "W" => SignClass::Warning,
            "R" => SignClass::Regulatory,
            "S" => SignClass::Supplementary,
            _ => SignClass::Guidance,
        },
    }
}

/// Initial geometry ground implied by the scheme, before palette resolution.
pub(crate) fn ground_for(scheme: ColourScheme) -> Colour {
    match scheme {
        ColourScheme::Primary => Colour::Green,
        ColourScheme::Secondary => Colour::White,
        ColourScheme::Information => Colour::Blue,
        ColourScheme::Tourist => Colour::Brown,
        ColourScheme::Temporary => Colour::Yellow,
        ColourScheme::Motorway => Colour::Blue,
        ColourScheme::Auto => Colour::Green,
    }
}

pub(crate) fn default_speed_kph(kind: SignKind) -> u32 {
    match kind {
        SignKind::MapTypeRoundabout => 50,
        SignKind::LanePreselection => 80,
        SignKind::PlaceIdentification | SignKind::Confirmatory => 100,
        _ => 60,
    }
}

/// Placeholder destinations each layout family can render immediately. Information, warning,
/// regulatory and supplementary signs are symbol-driven, not destination stacks, and get none.
pub(crate) fn sample_destinations(kind: SignKind) -> Vec<Destination> {
    let d = Destination::new;
    match kind {
        SignKind::InformationSign
        | SignKind::WarningSign
        | SignKind::RegulatoryProhibitory
        | SignKind::RegulatoryMandatory
        | SignKind::Stop
        | SignKind::GiveWay
        | SignKind::SupplementaryPlate => Vec::new(),
        SignKind::PlaceIdentification => vec![d("Kampala", Direction::Ahead)],
        SignKind::Confirmatory => vec![d("Kampala", Direction::Ahead).with_distance("40 km")],
        SignKind::Diversion => vec![
            d("Forward route", Direction::Ahead),
            d("Diversion route", Direction::Right),
        ],
        SignKind::StackType | SignKind::DirectionSign => {
            vec![
                d("Kasese", Direction::Ahead),
                d("Masheruka", Direction::Right),
            ]
        }
        _ => vec![
            d("Left", Direction::Left),
            d("Ahead", Direction::Ahead),
            d("Right", Direction::Right),
        ],
    }
}

/// A parsed sign-page key such as `G02.1`, `I04`, `W05.2`, `S21.3`, `UK04.1` or `D504.1`:
/// series letters, number and an optional positive suffix (C# `SignRef`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct SignRef {
    pub series: String,
    pub number: i32,
    pub suffix: Option<i32>,
}

impl SignRef {
    /// Series letters are upper-cased; the number is zero-insensitive. `None` for a key with
    /// no series letters, no number, or a suffix that is not a positive integer.
    pub fn parse(key: &str) -> Option<SignRef> {
        let s = key.trim().to_uppercase();
        let split = s.find(|c: char| c.is_ascii_digit()).unwrap_or(s.len());
        let (series, rest) = s.split_at(split);
        if series.is_empty() {
            return None;
        }
        let (num_text, suffix_text) = match rest.split_once('.') {
            Some((n, sfx)) => (n, Some(sfx)),
            None => (rest, None),
        };
        let number = parse_int(num_text)?;
        let suffix = match suffix_text {
            Some(text) => Some(parse_int(text).filter(|v| *v > 0)?),
            None => None,
        };
        Some(SignRef {
            series: series.to_string(),
            number,
            suffix,
        })
    }

    /// Guidance/Information colour semantics: `.1` is primary, `.2` secondary.
    pub fn variant(&self) -> Option<SignVariant> {
        match self.suffix {
            Some(1) => Some(SignVariant::Primary),
            Some(2) => Some(SignVariant::Secondary),
            _ => None,
        }
    }

    /// Warning/Regulatory left/right symbol mirror read from the suffix (series `W` and `R`).
    pub fn mirror(&self) -> SignMirror {
        match (self.series.as_str(), self.suffix) {
            ("W" | "R", Some(1)) => SignMirror::Left,
            ("W" | "R", Some(2)) => SignMirror::Right,
            _ => SignMirror::None,
        }
    }

    /// Supplementary variable-text line count (series `S` only): `.1/.2/.3`.
    pub fn text_lines(&self) -> Option<u32> {
        if self.series == "S" {
            self.suffix.and_then(|s| u32::try_from(s).ok())
        } else {
            None
        }
    }
}

/// An integer as C# `int.TryParse` reads one: optional surrounding white space, an optional
/// sign, then ASCII digits.
fn parse_int(text: &str) -> Option<i32> {
    let t = text.trim();
    let digits = t.strip_prefix(['+', '-']).unwrap_or(t);
    if digits.is_empty() || !digits.bytes().all(|b| b.is_ascii_digit()) {
        return None;
    }
    t.parse().ok()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn entry(key: &str, family: &str, series: &str, scheme: &str) -> CatalogEntry {
        CatalogEntry::from_pack(&PackCatalogEntry {
            key: key.into(),
            name: String::new(),
            family: family.into(),
            series: series.into(),
            scheme: scheme.into(),
            symbol_id: None,
            embedded_symbols: None,
            template: None,
            diagram: None,
            citation: None,
            sizes: None,
        })
    }

    #[test]
    fn keys_parse_into_series_number_and_suffix() {
        let r = SignRef::parse(" g02.1 ").unwrap();
        assert_eq!((r.series.as_str(), r.number, r.suffix), ("G", 2, Some(1)));
        let r = SignRef::parse("D504.1").unwrap();
        assert_eq!((r.series.as_str(), r.number, r.suffix), ("D", 504, Some(1)));
        let r = SignRef::parse("TS5").unwrap();
        assert_eq!((r.series.as_str(), r.number, r.suffix), ("TS", 5, None));
        for bad in [
            "", "  ", "12", "G", "G02.0", "G02.-1", "G02.x", "G02.", "G1A",
        ] {
            assert_eq!(SignRef::parse(bad), None, "{bad}");
        }
    }

    #[test]
    fn the_suffix_means_variant_mirror_or_line_count_by_series() {
        let g = SignRef::parse("G02.1").unwrap();
        assert_eq!(g.variant(), Some(SignVariant::Primary));
        assert_eq!(g.mirror(), SignMirror::None);
        assert_eq!(g.text_lines(), None);
        let w = SignRef::parse("W05.2").unwrap();
        assert_eq!(w.variant(), Some(SignVariant::Secondary));
        assert_eq!(w.mirror(), SignMirror::Right);
        assert_eq!(SignRef::parse("R12.1").unwrap().mirror(), SignMirror::Left);
        let s = SignRef::parse("S21.3").unwrap();
        assert_eq!(s.variant(), None);
        assert_eq!(s.text_lines(), Some(3));
        assert_eq!(SignRef::parse("I04").unwrap().variant(), None);
    }

    #[test]
    fn family_tokens_name_layout_kinds() {
        assert_eq!(kind_of_family(" Stack "), SignKind::StackType);
        assert_eq!(
            kind_of_family("map-roundabout"),
            SignKind::MapTypeRoundabout
        );
        assert_eq!(kind_of_family("tourist"), SignKind::InformationSign);
        assert_eq!(kind_of_family("warning"), SignKind::WarningSign);
        assert_eq!(
            kind_of_family("prohibitory"),
            SignKind::RegulatoryProhibitory
        );
        assert_eq!(
            kind_of_family("regulatory-mandatory"),
            SignKind::RegulatoryMandatory
        );
        assert_eq!(kind_of_family("give-way"), SignKind::GiveWay);
        assert_eq!(
            kind_of_family("supplementary"),
            SignKind::SupplementaryPlate
        );
        assert_eq!(kind_of_family("no-such-family"), SignKind::StackType);
        assert_eq!(scheme_named("Temp"), ColourScheme::Temporary);
        assert_eq!(scheme_named("motorway"), ColourScheme::Motorway);
        assert_eq!(scheme_named(""), ColourScheme::Auto);
    }

    #[test]
    fn a_stack_entry_seeds_the_app_sign() {
        let d = build(&entry("UK04.1", "stack", "UK", "primary"), None);
        assert_eq!(d.code, "UK04.1");
        assert_eq!(d.catalog_key.as_deref(), Some("UK04.1"));
        assert_eq!((d.kind, d.speed_kph), (SignKind::StackType, 60));
        assert_eq!(
            (d.background, d.scheme),
            (Colour::Green, ColourScheme::Primary)
        );
        assert_eq!(
            (d.variant, d.class),
            (SignVariant::Primary, SignClass::Guidance)
        );
        assert_eq!(d.destinations.len(), 2);
        assert_eq!(d.info_arrow, None);
        assert_eq!(
            build(&entry("UK04.2", "stack", "UK", "secondary"), Some(100)).speed_kph,
            100
        );
    }

    #[test]
    fn symbol_driven_families_get_no_destinations() {
        let mut e = entry("D512", "warning", "D", "secondary");
        e.symbol_id = Some("512".into());
        let d = build(&e, None);
        assert_eq!(
            (d.kind, d.class),
            (SignKind::WarningSign, SignClass::Warning)
        );
        assert_eq!(d.background, Colour::White);
        assert_eq!(d.symbol_id.as_deref(), Some("512"));
        assert!(d.destinations.is_empty());
        assert_eq!(d.mirror, SignMirror::None); // series D, not W
        let s = build(&entry("S21.2", "supplementary", "S", ""), None);
        assert_eq!((s.class, s.text_lines), (SignClass::Supplementary, Some(2)));
        assert_eq!(
            build(&entry("W05.2", "warning", "W", ""), None).mirror,
            SignMirror::Right
        );
    }

    #[test]
    fn information_pages_pick_their_plate_by_number_unless_the_entry_says() {
        let road = build(&entry("I07", "information", "I", "information"), None);
        assert_eq!(road.info_template, InformationTemplate::SymbolPlate);
        assert_eq!(road.info_arrow, None);
        assert_eq!(road.symbol_id.as_deref(), Some("aircraft")); // never a blank preview
        let service = build(&entry("I21", "information", "I", "information"), None);
        assert_eq!(service.info_template, InformationTemplate::ServicePanel);
        assert_eq!(service.info_arrow, Some(Direction::Right));
        assert_eq!(service.background, Colour::Blue);
        let mut named = entry("I30", "information", "I", "tourist");
        named.template = Some(" Plate ".into());
        assert_eq!(
            build(&named, None).info_template,
            InformationTemplate::SymbolPlate
        );
    }

    #[test]
    fn other_families_get_their_default_speed_and_destinations() {
        let r = build(&entry("UK01.1", "map-roundabout", "UK", "primary"), None);
        assert_eq!((r.speed_kph, r.destinations.len()), (50, 3));
        let c = build(&entry("UK07.1", "confirmatory", "UK", "primary"), None);
        assert_eq!(c.speed_kph, 100);
        assert_eq!(c.destinations[0].distance.as_deref(), Some("40 km"));
        assert_eq!(class_for("w", SignKind::StackType), SignClass::Warning);
        assert_eq!(class_for("UK", SignKind::StackType), SignClass::Guidance);
    }
}
