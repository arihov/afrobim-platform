// SPDX-License-Identifier: LicenseRef-AfroBIM-Proprietary
// Copyright (c) 2026 AfroBIM. All rights reserved.
//! The sign definition: what the designer asks for. Mirrors the C# `SignCore.Model` records
//! (`SignDefinition`, `Destination` and their enums) field for field, so a schedule written by
//! either engine describes the same sign.

use serde::{Deserialize, Serialize};

pub use afrobim_draw::Colour;

/// Which way a destination lies from the junction.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default, Serialize, Deserialize)]
pub enum Direction {
    #[default]
    Ahead,
    Left,
    Right,
}

impl Direction {
    pub(crate) fn name(self) -> &'static str {
        match self {
            Direction::Ahead => "Ahead",
            Direction::Left => "Left",
            Direction::Right => "Right",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default, Serialize, Deserialize)]
pub enum RoadClass {
    #[default]
    Primary,
    Secondary,
    Local,
}

/// The layout family a sign belongs to: which geometry engine renders it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default, Serialize, Deserialize)]
pub enum SignKind {
    // Guidance (§2 of App.1): map-type / stack-type / direction / confirmatory / place / lane
    #[default]
    StackType,
    MapTypeCrossroads,
    MapTypeRoundabout,
    MapTypeTee,
    DirectionSign,
    Confirmatory,
    PlaceIdentification,
    LanePreselection,
    // Temporary diversion (App.3 G31/G32)
    Diversion,
    // Information series (App.3 §5)
    InformationSign,
    // Warning family
    WarningSign,
    // Regulatory family
    RegulatoryProhibitory,
    RegulatoryMandatory,
    Stop,
    GiveWay,
    // Supplementary plates (Vol.1 D.9)
    SupplementaryPlate,
}

impl SignKind {
    pub(crate) fn name(self) -> &'static str {
        match self {
            SignKind::StackType => "StackType",
            SignKind::MapTypeCrossroads => "MapTypeCrossroads",
            SignKind::MapTypeRoundabout => "MapTypeRoundabout",
            SignKind::MapTypeTee => "MapTypeTee",
            SignKind::DirectionSign => "DirectionSign",
            SignKind::Confirmatory => "Confirmatory",
            SignKind::PlaceIdentification => "PlaceIdentification",
            SignKind::LanePreselection => "LanePreselection",
            SignKind::Diversion => "Diversion",
            SignKind::InformationSign => "InformationSign",
            SignKind::WarningSign => "WarningSign",
            SignKind::RegulatoryProhibitory => "RegulatoryProhibitory",
            SignKind::RegulatoryMandatory => "RegulatoryMandatory",
            SignKind::Stop => "Stop",
            SignKind::GiveWay => "GiveWay",
            SignKind::SupplementaryPlate => "SupplementaryPlate",
        }
    }

    /// The canonical family token for a layout kind (C# `SignFamily.Token`), used to look up
    /// per-family rule-pack tables such as `textSizesByFamily`.
    pub(crate) fn family_token(self) -> &'static str {
        match self {
            SignKind::StackType => "stack",
            SignKind::MapTypeCrossroads => "map-crossroads",
            SignKind::MapTypeRoundabout => "map-roundabout",
            SignKind::MapTypeTee => "map-tee",
            SignKind::DirectionSign => "direction",
            SignKind::Confirmatory => "confirmatory",
            SignKind::PlaceIdentification => "place",
            SignKind::LanePreselection => "lane",
            SignKind::Diversion => "diversion",
            SignKind::InformationSign => "information",
            _ => "stack",
        }
    }
}

/// Vol.1 §B sign classification.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default, Serialize, Deserialize)]
pub enum SignClass {
    Warning,
    Regulatory,
    #[default]
    Guidance,
    Information,
    Supplementary,
}

/// A sign's colour scheme. `Auto` resolves through the catalog key, else the explicit
/// background. `Motorway` is the UK blue motorway scheme (UK TSM Chapter 7 §3.5).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default, Serialize, Deserialize)]
pub enum ColourScheme {
    #[default]
    Auto,
    Primary,
    Secondary,
    Information,
    Tourist,
    Temporary,
    Motorway,
}

/// Appendix 3 variant suffix: `.1` = primary route (green ground), `.2` = secondary/local.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default, Serialize, Deserialize)]
pub enum SignVariant {
    Primary,
    #[default]
    Secondary,
}

/// Warning/regulatory left/right symbol mirror.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default, Serialize, Deserialize)]
pub enum SignMirror {
    #[default]
    None,
    Left,
    Right,
}

/// App.1 §2.1.3 embedded symbol geometry within a guidance panel.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum EmbeddedType {
    WarningTriangle,
    RegulatoryRoundel,
}

/// App.3 §5 information-sign plate template.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default, Serialize, Deserialize)]
pub enum InformationTemplate {
    SymbolPlate,
    #[default]
    ServicePanel,
}

/// UK TSM Chapter 7 §3.2 route status. It sets the colour of the panel a destination sits on
/// and of the patch behind its route number: blue for motorways, dark green for primary
/// routes, white for non-primary routes.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default, Serialize, Deserialize)]
pub enum RouteStatus {
    #[default]
    Unspecified,
    Motorway,
    Primary,
    NonPrimary,
}

/// One destination of a sign.
///
/// `order` is the proximity rank within a direction; 0 is nearer (App.1 §2.1.2).
/// `route_number` is written as it appears on the sign, e.g. `A11` or `(M11)` (brackets mean
/// the route is reached via another one). `route` is the status of the route leaving the
/// junction towards the destination, which decides the destination's panel. `number_status`
/// is the status of the numbered route, which decides its patch; when unspecified it is
/// inferred (motorway for `M…` and `…(M)` numbers, otherwise `route`).
#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
pub struct Destination {
    pub name: String,
    pub direction: Direction,
    #[serde(default)]
    pub order: i32,
    #[serde(default)]
    pub symbol_id: Option<String>,
    #[serde(default)]
    pub distance: Option<String>,
    #[serde(default)]
    pub route_number: Option<String>,
    #[serde(default)]
    pub route: RouteStatus,
    #[serde(default)]
    pub number_status: RouteStatus,
}

impl Destination {
    /// A destination with proximity order 0 and nothing else set.
    pub fn new(name: impl Into<String>, direction: Direction) -> Self {
        Destination {
            name: name.into(),
            direction,
            ..Default::default()
        }
    }

    /// Sets the proximity rank within the direction (0 is nearest, drawn on top).
    pub fn with_order(mut self, order: i32) -> Self {
        self.order = order;
        self
    }

    /// Sets the distance legend, e.g. `"12"`.
    pub fn with_distance(mut self, distance: impl Into<String>) -> Self {
        self.distance = Some(distance.into());
        self
    }

    /// Sets the route number and the status of the route towards the destination.
    pub fn with_route(mut self, number: impl Into<String>, route: RouteStatus) -> Self {
        self.route_number = Some(number.into());
        self.route = route;
        self
    }
}

/// A warning triangle or regulatory roundel embedded in a guidance panel (App.1 §2.1.3).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct EmbeddedSign {
    pub symbol_id: String,
    #[serde(rename = "type")]
    pub kind: EmbeddedType,
}

/// Which side of the road a sign stands on, looking in the direction of increasing station.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum SignSide {
    Left,
    Right,
    Both,
}

/// Where a sign stands along a road alignment. Station and offset are in metres.
#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
pub struct SignLocation {
    #[serde(default)]
    pub alignment: Option<String>,
    #[serde(default)]
    pub station: Option<f64>,
    #[serde(default)]
    pub offset: Option<f64>,
    #[serde(default)]
    pub side: Option<SignSide>,
}

/// The full definition of a sign to be laid out.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SignDefinition {
    pub code: String,
    pub kind: SignKind,
    pub speed_kph: u32,
    #[serde(default)]
    pub approach_class: RoadClass,
    #[serde(default)]
    pub destinations: Vec<Destination>,
    pub background: Colour,
    #[serde(default)]
    pub class: SignClass,
    #[serde(default)]
    pub scheme: ColourScheme,
    #[serde(default)]
    pub variant: SignVariant,
    #[serde(default)]
    pub symbol_id: Option<String>,
    #[serde(default)]
    pub embedded: Option<Vec<EmbeddedSign>>,
    #[serde(default)]
    pub catalog_key: Option<String>,
    #[serde(default)]
    pub info_template: InformationTemplate,
    #[serde(default)]
    pub info_arrow: Option<Direction>,
    #[serde(default)]
    pub info_distance: Option<String>,
    #[serde(default)]
    pub mirror: SignMirror,
    #[serde(default)]
    pub text_lines: Option<u32>,
    #[serde(default)]
    pub location: Option<SignLocation>,
}

impl SignDefinition {
    /// A new stack-type sign with the defaults the desktop app gives one (C# `AddStack`):
    /// primary approach, green background, everything else unset.
    pub fn stack(code: &str, speed_kph: u32, destinations: Vec<Destination>) -> SignDefinition {
        SignDefinition {
            code: code.to_string(),
            kind: SignKind::StackType,
            speed_kph,
            approach_class: RoadClass::Primary,
            destinations,
            background: Colour::Green,
            class: SignClass::Guidance,
            scheme: ColourScheme::Auto,
            variant: SignVariant::Secondary,
            symbol_id: None,
            embedded: None,
            catalog_key: None,
            info_template: InformationTemplate::ServicePanel,
            info_arrow: None,
            info_distance: None,
            mirror: SignMirror::None,
            text_lines: None,
            location: None,
        }
    }
}

/// Lettering face. App.1 §1.2: Transport Medium on dark, Transport Heavy on light.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum FontFace {
    TransportMedium,
    TransportHeavy,
}

impl FontFace {
    pub(crate) fn name(self) -> &'static str {
        match self {
            FontFace::TransportMedium => "TransportMedium",
            FontFace::TransportHeavy => "TransportHeavy",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum Severity {
    Info,
    Warning,
    Error,
}

/// A finding about a laid-out sign. `source` is the clause of the standard it cites
/// (C# `Diagnostic.Clause`).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Diagnostic {
    pub severity: Severity,
    pub code: String,
    pub message: String,
    pub source: String,
}

impl Diagnostic {
    pub(crate) fn warning(code: &str, message: String, source: &str) -> Self {
        Diagnostic {
            severity: Severity::Warning,
            code: code.to_string(),
            message,
            source: source.to_string(),
        }
    }

    pub(crate) fn error(code: &str, message: String, source: &str) -> Self {
        Diagnostic {
            severity: Severity::Error,
            code: code.to_string(),
            message,
            source: source.to_string(),
        }
    }
}

pub(crate) fn colour_name(c: Colour) -> &'static str {
    match c {
        Colour::White => "White",
        Colour::Black => "Black",
        Colour::Green => "Green",
        Colour::Blue => "Blue",
        Colour::Brown => "Brown",
        Colour::Yellow => "Yellow",
        Colour::Red => "Red",
        Colour::Grey => "Grey",
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn stack_has_the_app_defaults() {
        let s = SignDefinition::stack(
            "ADS-001",
            60,
            vec![Destination::new("Kasese", Direction::Ahead)],
        );
        assert_eq!(s.kind, SignKind::StackType);
        assert_eq!(s.approach_class, RoadClass::Primary);
        assert_eq!(s.background, Colour::Green);
        assert_eq!(s.scheme, ColourScheme::Auto);
        assert_eq!(s.destinations[0].order, 0);
        assert_eq!(s.destinations[0].route, RouteStatus::Unspecified);
    }

    #[test]
    fn partial_json_loads_with_defaults() {
        let s: SignDefinition = serde_json::from_str(
            r#"{"code":"X","kind":"StackType","speed_kph":60,"background":"Green",
                "destinations":[{"name":"Kasese","direction":"Ahead"}]}"#,
        )
        .unwrap();
        assert_eq!(
            s,
            SignDefinition::stack("X", 60, vec![Destination::new("Kasese", Direction::Ahead)])
        );
    }

    #[test]
    fn definition_round_trips_through_json() {
        let mut s = SignDefinition::stack(
            "X",
            100,
            vec![Destination::new("Gulu", Direction::Left)
                .with_order(1)
                .with_distance("7")
                .with_route("(M11)", RouteStatus::Primary)],
        );
        s.embedded = Some(vec![EmbeddedSign {
            symbol_id: "h".into(),
            kind: EmbeddedType::RegulatoryRoundel,
        }]);
        let json = serde_json::to_string(&s).unwrap();
        assert_eq!(serde_json::from_str::<SignDefinition>(&json).unwrap(), s);
    }
}
