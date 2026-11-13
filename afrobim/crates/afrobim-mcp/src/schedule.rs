//! The sign-schedule CSV reader: a port of the parsing half of C# `SignCore.Cli.ScheduleCsv` and
//! the token vocabulary of `SignCore.Model.ScheduleCodec`.
//!
//! Required header columns (case-insensitive, any order):
//!   `code, kind, speed_kph, approach_class, background, destinations`
//! Optional columns (defaulted when absent or blank):
//!   `variant, scheme, symbol, info_template, info_arrow, info_distance, class, catalog_key,
//!   mirror, text_lines, embedded, alignment, station, offset, side`
//!
//! A destination cell holds `;`-separated entries of the form
//! `Name:Direction:Order[:Distance[:Symbol[:RouteNumber[:Route[:NumberStatus]]]]]`.
//! Fields containing commas or quotes are wrapped in double quotes (RFC 4180).
//!
//! The Rust platform has no schedule reader of its own yet, so this one lives here; it should
//! move next to the sign engine once a schedule module exists there.

use std::collections::HashMap;
use std::path::Path;

use afrobim_signs::{
    Colour, ColourScheme, Destination, Direction, EmbeddedSign, EmbeddedType, InformationTemplate,
    RoadClass, RouteStatus, SignClass, SignDefinition, SignKind, SignLocation, SignMirror,
    SignSide, SignVariant,
};

/// The canonical header (C# `ScheduleCsv.WriteHeader`): the column order of a `schedule_row`.
pub const CANONICAL_HEADER: &str = "code,kind,speed_kph,approach_class,background,destinations,\
variant,scheme,symbol,info_template,info_arrow,info_distance,\
class,catalog_key,mirror,text_lines,embedded,\
alignment,station,offset,side";

const REQUIRED: [&str; 6] = [
    "code",
    "kind",
    "speed_kph",
    "approach_class",
    "background",
    "destinations",
];

/// A header or row that cannot be parsed (C# `CsvFormatException`).
#[derive(Debug, Clone, PartialEq)]
pub struct CsvError(pub String);

impl std::fmt::Display for CsvError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.0)
    }
}

fn err<T>(message: String) -> Result<T, CsvError> {
    Err(CsvError(message))
}

/// Column name (lower case) to index.
pub type Header = HashMap<String, usize>;

pub fn parse_header(line: &str) -> Result<Header, CsvError> {
    let mut map = Header::new();
    for (i, col) in split_line(line).iter().enumerate() {
        let name = col.trim().to_lowercase();
        if name.is_empty() {
            return err(format!("Header column {} is empty", i + 1));
        }
        if map.insert(name.clone(), i).is_some() {
            return err(format!("Duplicate header column '{name}'"));
        }
    }
    for req in REQUIRED {
        if !map.contains_key(req) {
            return err(format!("CSV is missing required column '{req}'"));
        }
    }
    Ok(map)
}

pub fn parse_row(line: &str, header: &Header, row: usize) -> Result<SignDefinition, CsvError> {
    let cols = split_line(line);
    let get = |key: &str| -> Result<&str, CsvError> {
        match cols.get(header[key]) {
            Some(c) => Ok(c.trim()),
            None => err(format!("row {row}: missing column '{key}'")),
        }
    };
    // Optional columns: absent, or blank, fall back to the model default.
    let opt = |key: &str| -> &str {
        header
            .get(key)
            .and_then(|i| cols.get(*i))
            .map_or("", |c| c.trim())
    };
    let non_empty =
        |s: &str| -> Option<String> { (!s.trim().is_empty()).then(|| s.trim().to_string()) };

    let code = get("code")?;
    if code.is_empty() {
        return err(format!("row {row}: 'code' is empty"));
    }
    let kind_token = get("kind")?;
    let Some(kind) = parse_kind(kind_token) else {
        return err(format!(
            "row {row}: unknown kind '{kind_token}' (expected a schedule kind token such as stack | map-crossroads | direction | \
             information | warning | regulatory-prohibitory | stop | give-way | supplementary)"
        ));
    };
    let speed_text = get("speed_kph")?;
    let Ok(speed) = speed_text.parse::<i64>() else {
        return err(format!(
            "row {row}: 'speed_kph' is not an integer: '{speed_text}'"
        ));
    };
    let Ok(speed_kph) = u32::try_from(speed) else {
        return err(format!(
            "row {row}: 'speed_kph' must not be negative: '{speed_text}'"
        ));
    };
    let approach_text = get("approach_class")?;
    let approach_class = match approach_text.to_lowercase().as_str() {
        "primary" => RoadClass::Primary,
        "secondary" => RoadClass::Secondary,
        "local" => RoadClass::Local,
        _ => {
            return err(format!(
                "row {row}: unknown approach_class '{approach_text}'"
            ))
        }
    };
    let background_text = get("background")?;
    let Some(background) = parse_colour(background_text) else {
        return err(format!(
            "row {row}: unknown background colour '{background_text}'"
        ));
    };
    let destinations = parse_destinations(get("destinations")?, row)?;

    let scheme_text = opt("scheme");
    let scheme = match scheme_text.to_lowercase().as_str() {
        "" | "auto" => ColourScheme::Auto,
        "primary" => ColourScheme::Primary,
        "secondary" => ColourScheme::Secondary,
        "information" => ColourScheme::Information,
        "tourist" => ColourScheme::Tourist,
        "temporary" => ColourScheme::Temporary,
        "motorway" => ColourScheme::Motorway,
        _ => {
            return err(format!(
                "row {row}: unknown scheme '{scheme_text}' (expected auto | primary | secondary | information | tourist | temporary | motorway)"
            ))
        }
    };
    let variant_text = opt("variant");
    let variant = match variant_text.to_lowercase().as_str() {
        "" | "secondary" => SignVariant::Secondary,
        "primary" => SignVariant::Primary,
        _ => {
            return err(format!(
                "row {row}: unknown variant '{variant_text}' (expected primary | secondary)"
            ))
        }
    };
    let template_text = opt("info_template");
    let info_template = match normalise(template_text).as_str() {
        "" | "service-panel" | "service" | "panel" => InformationTemplate::ServicePanel,
        "symbol-plate" | "symbol" | "plate" => InformationTemplate::SymbolPlate,
        _ => return err(format!("row {row}: unknown info_template '{template_text}' (expected service-panel | symbol-plate)")),
    };
    let arrow_text = opt("info_arrow");
    let info_arrow = match arrow_text.to_lowercase().as_str() {
        "" | "none" | "-" => None,
        "ahead" => Some(Direction::Ahead),
        "left" => Some(Direction::Left),
        "right" => Some(Direction::Right),
        _ => {
            return err(format!(
            "row {row}: unknown info_arrow '{arrow_text}' (expected none | ahead | left | right)"
        ))
        }
    };
    let class_text = opt("class");
    let class = match normalise(class_text).as_str() {
        "" | "guidance" => SignClass::Guidance,
        "warning" => SignClass::Warning,
        "regulatory" => SignClass::Regulatory,
        "information" => SignClass::Information,
        "supplementary" => SignClass::Supplementary,
        _ => {
            return err(format!(
                "row {row}: unknown class '{class_text}' (expected warning | regulatory | guidance | information | supplementary)"
            ))
        }
    };
    let mirror_text = opt("mirror");
    let mirror = match normalise(mirror_text).as_str() {
        "" | "none" | "-" => SignMirror::None,
        "left" => SignMirror::Left,
        "right" => SignMirror::Right,
        _ => {
            return err(format!(
                "row {row}: unknown mirror '{mirror_text}' (expected none | left | right)"
            ))
        }
    };
    let lines_text = opt("text_lines");
    let text_lines = if lines_text.is_empty() {
        None
    } else {
        match lines_text.parse::<u32>() {
            Ok(n) if n > 0 => Some(n),
            _ => {
                return err(format!(
                    "row {row}: text_lines must be a positive integer, got '{lines_text}'"
                ))
            }
        }
    };
    let embedded_text = opt("embedded");
    let Some(embedded) = parse_embedded(embedded_text) else {
        return err(format!("row {row}: malformed embedded '{embedded_text}' (expected symbol-id:type pairs separated by ';')"));
    };

    // Location along an alignment: all four optional.
    let metres = |col: &str| -> Result<Option<f64>, CsvError> {
        let s = opt(col);
        if s.is_empty() {
            return Ok(None);
        }
        match parse_metres(s) {
            Some(m) => Ok(Some(m)),
            None => err(format!("row {row}: {col} must be a number of metres (e.g. 1250.5, -3.5 or 1+250.5), got '{s}'")),
        }
    };
    let side_text = opt("side");
    let side = match normalise(side_text).as_str() {
        "" => None,
        "left" | "l" => Some(SignSide::Left),
        "right" | "r" => Some(SignSide::Right),
        "both" | "b" => Some(SignSide::Both),
        _ => {
            return err(format!(
                "row {row}: unknown side '{side_text}' (expected left | right | both)"
            ))
        }
    };
    let location = SignLocation {
        alignment: non_empty(opt("alignment")),
        station: metres("station")?,
        offset: metres("offset")?,
        side,
    };
    let location = (location != SignLocation::default()).then_some(location);

    Ok(SignDefinition {
        code: code.to_string(),
        kind,
        speed_kph,
        approach_class,
        destinations,
        background,
        class,
        scheme,
        variant,
        symbol_id: non_empty(opt("symbol")),
        embedded,
        catalog_key: non_empty(opt("catalog_key")),
        info_template,
        info_arrow,
        info_distance: non_empty(opt("info_distance")),
        mirror,
        text_lines,
        location,
    })
}

/// Lower case, trimmed, underscores as hyphens (C# `ScheduleCodec.Normalize`).
fn normalise(token: &str) -> String {
    token.trim().to_lowercase().replace('_', "-")
}

/// Every token an exporter writes plus the legacy aliases; hyphens, underscores and case are
/// ignored (C# `ScheduleCodec.TryParseKind`).
pub fn parse_kind(token: &str) -> Option<SignKind> {
    const ALIASES: [(SignKind, &[&str]); 16] = [
        (SignKind::StackType, &["stack", "stack-type"]),
        (
            SignKind::MapTypeCrossroads,
            &["map", "map-crossroads", "crossroads", "maptypecross"],
        ),
        (
            SignKind::MapTypeRoundabout,
            &["map-roundabout", "roundabout"],
        ),
        (SignKind::MapTypeTee, &["map-tee", "tee", "t-junction"]),
        (SignKind::DirectionSign, &["direction", "direction-sign"]),
        (SignKind::Confirmatory, &["confirmatory"]),
        (
            SignKind::PlaceIdentification,
            &["place", "place-id", "place-identification"],
        ),
        (
            SignKind::LanePreselection,
            &["lane", "lane-preselection", "lane-distributor"],
        ),
        (SignKind::Diversion, &["diversion"]),
        (
            SignKind::InformationSign,
            &["information", "info", "service", "tourist"],
        ),
        (SignKind::WarningSign, &["warning"]),
        (
            SignKind::RegulatoryProhibitory,
            &["regulatory-prohibitory", "prohibitory"],
        ),
        (
            SignKind::RegulatoryMandatory,
            &["regulatory-mandatory", "mandatory"],
        ),
        (SignKind::Stop, &["stop"]),
        (SignKind::GiveWay, &["give-way", "giveway"]),
        (SignKind::SupplementaryPlate, &["supplementary"]),
    ];
    let slug = |s: &str| s.trim().to_lowercase().replace(['-', '_'], "");
    let wanted = slug(token);
    if wanted.is_empty() {
        return None;
    }
    ALIASES
        .iter()
        .find(|(_, names)| names.iter().any(|n| slug(n) == wanted))
        .map(|(kind, _)| *kind)
}

fn parse_colour(s: &str) -> Option<Colour> {
    Some(match s.to_lowercase().as_str() {
        "white" => Colour::White,
        "black" => Colour::Black,
        "green" => Colour::Green,
        "blue" => Colour::Blue,
        "brown" => Colour::Brown,
        "yellow" => Colour::Yellow,
        "red" => Colour::Red,
        "grey" | "gray" => Colour::Grey,
        _ => return None,
    })
}

/// `Some(None)` for a blank cell, `None` for a malformed one.
fn parse_embedded(cell: &str) -> Option<Option<Vec<EmbeddedSign>>> {
    let mut list = Vec::new();
    for part in cell.split(';').map(str::trim).filter(|p| !p.is_empty()) {
        let bits: Vec<&str> = part.split(':').map(str::trim).collect();
        if bits.len() != 2 || bits[0].is_empty() {
            return None;
        }
        let kind = match normalise(bits[1]).as_str() {
            "warning-triangle" | "warningtriangle" | "warning" => EmbeddedType::WarningTriangle,
            "regulatory-roundel" | "regulatoryroundel" | "roundel" => {
                EmbeddedType::RegulatoryRoundel
            }
            _ => return None,
        };
        list.push(EmbeddedSign {
            symbol_id: bits[0].to_string(),
            kind,
        });
    }
    Some((!list.is_empty()).then_some(list))
}

/// A station or offset in metres: a plain number (`1250.5`, `-3.5`) or a chainage written
/// `km+metres` (`1+250.5` = 1250.5 m).
fn parse_metres(text: &str) -> Option<f64> {
    let t = text.trim();
    // A leading '+' is a sign and "1e+3" an exponent, not a chainage separator.
    let plus = t
        .char_indices()
        .skip(1)
        .find(|(_, c)| *c == '+')
        .map(|(i, _)| i);
    if let Some(plus) = plus.filter(|p| !matches!(t.as_bytes()[p - 1], b'e' | b'E')) {
        let (km, m) = (&t[..plus], &t[plus + 1..]);
        let k: i64 = km.parse().ok()?;
        let rest: f64 = m.parse().ok()?;
        if !rest.is_finite() || !(0.0..1000.0).contains(&rest) || m.starts_with('-') || k < 0 {
            return None;
        }
        return Some(k as f64 * 1000.0 + rest);
    }
    t.parse::<f64>().ok().filter(|m| m.is_finite())
}

fn parse_destinations(cell: &str, row: usize) -> Result<Vec<Destination>, CsvError> {
    // A blank cell is legal: information-series and some place signs carry no destinations.
    let mut list = Vec::new();
    for p in cell.split(';').map(str::trim).filter(|p| !p.is_empty()) {
        let bits: Vec<&str> = p.split(':').map(str::trim).collect();
        if bits.len() < 3 || bits.len() > 8 {
            return err(format!(
                "row {row}: destination '{p}' must be Name:Direction:Order[:Distance[:Symbol[:RouteNumber[:Route[:NumberStatus]]]]]"
            ));
        }
        let direction = match bits[1].to_lowercase().as_str() {
            "ahead" => Direction::Ahead,
            "left" => Direction::Left,
            "right" => Direction::Right,
            _ => {
                return err(format!(
                    "row {row}: destination '{p}' has unknown direction '{}'",
                    bits[1]
                ))
            }
        };
        let Ok(order) = bits[2].parse::<i32>() else {
            return err(format!(
                "row {row}: destination '{p}' has non-integer order '{}'",
                bits[2]
            ));
        };
        if bits[0].is_empty() {
            return err(format!("row {row}: destination '{p}' has empty name"));
        }
        let field = |i: usize| bits.get(i).filter(|b| !b.is_empty()).map(|b| b.to_string());
        let status = |i: usize| -> Result<RouteStatus, CsvError> {
            let Some(token) = bits.get(i) else {
                return Ok(RouteStatus::Unspecified);
            };
            match token.to_lowercase().as_str() {
                "" => Ok(RouteStatus::Unspecified),
                "motorway" | "m" => Ok(RouteStatus::Motorway),
                "primary" | "p" => Ok(RouteStatus::Primary),
                "non-primary" | "nonprimary" | "np" => Ok(RouteStatus::NonPrimary),
                _ => err(format!(
                    "row {row}: destination '{p}' has unknown route status '{token}' (expected motorway, primary or non-primary)"
                )),
            }
        };
        list.push(Destination {
            name: bits[0].to_string(),
            direction,
            order,
            distance: field(3),
            symbol_id: field(4),
            route_number: field(5),
            route: status(6)?,
            number_status: status(7)?,
        });
    }
    Ok(list)
}

/// RFC 4180 line splitter: handles quoted fields and doubled quotes.
pub fn split_line(line: &str) -> Vec<String> {
    let mut fields = Vec::new();
    let mut buf = String::new();
    let mut in_quotes = false;
    let mut chars = line.chars().peekable();
    while let Some(c) = chars.next() {
        if in_quotes {
            if c == '"' {
                if chars.peek() == Some(&'"') {
                    buf.push('"');
                    chars.next();
                } else {
                    in_quotes = false;
                }
            } else {
                buf.push(c);
            }
        } else if c == '"' {
            in_quotes = true;
        } else if c == ',' {
            fields.push(std::mem::take(&mut buf));
        } else {
            buf.push(c);
        }
    }
    fields.push(buf);
    fields
}

/// One schedule CSV data line, read against the canonical column order.
pub fn parse_schedule_row(row: &str) -> Result<SignDefinition, CsvError> {
    if row.contains('\n') || row.contains('\r') {
        return err(
            "schedule_row must be a single CSV data line (no header, no newlines)".to_string(),
        );
    }
    let header = parse_header(CANONICAL_HEADER).expect("the canonical header parses");
    parse_row(row, &header, 1).map_err(|e| CsvError(format!("schedule_row: {e}")))
}

/// A parsed schedule row, or the reason it could not be parsed. `row` is the 1-based file line.
#[derive(Debug, Clone, PartialEq)]
pub struct ScheduleRow {
    pub row: usize,
    pub sign: Result<SignDefinition, String>,
}

/// Reads a schedule CSV: header first, blank and `#` lines skipped, rows numbered by file
/// line. An unreadable file, an empty file or a bad header fails the whole call.
pub fn read_schedule(path: &Path) -> Result<Vec<ScheduleRow>, CsvError> {
    let shown = path.display();
    let text = std::fs::read_to_string(path)
        .map_err(|e| CsvError(format!("could not read {shown}: {e}")))?;
    let text = text.strip_prefix('\u{feff}').unwrap_or(&text);
    let mut lines = text.lines().enumerate();
    let Some((_, first)) = lines.next() else {
        return err(format!("schedule is empty: {shown}"));
    };
    let header = parse_header(first).map_err(|e| CsvError(format!("{shown}: {e}")))?;
    Ok(lines
        .filter(|(_, line)| !line.trim().is_empty() && !line.trim_start().starts_with('#'))
        .map(|(i, line)| ScheduleRow {
            row: i + 1,
            sign: parse_row(line, &header, i + 1).map_err(|e| e.0),
        })
        .collect())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_minimal_row_gets_the_model_defaults() {
        let s = parse_schedule_row("ADS-1,stack,64,primary,green,Lampton:Ahead:0;Slough:Left:0")
            .unwrap();
        let mut expected = SignDefinition::stack(
            "ADS-1",
            64,
            vec![
                Destination::new("Lampton", Direction::Ahead),
                Destination::new("Slough", Direction::Left),
            ],
        );
        expected.approach_class = RoadClass::Primary;
        assert_eq!(s, expected);
    }

    #[test]
    fn every_optional_column_is_read() {
        let s = parse_schedule_row(
            "W-1,Warning,50,local,White,,primary,temporary,504.1,symbol_plate,left,\"Ford|100 yds\",warning,D504.1,right,2,h:roundel;x:warning-triangle,A1,1+250.5,-3.5,L",
        )
        .unwrap();
        assert_eq!(s.kind, SignKind::WarningSign);
        assert_eq!(s.approach_class, RoadClass::Local);
        assert_eq!(s.background, Colour::White);
        assert!(s.destinations.is_empty());
        assert_eq!(s.variant, SignVariant::Primary);
        assert_eq!(s.scheme, ColourScheme::Temporary);
        assert_eq!(s.symbol_id.as_deref(), Some("504.1"));
        assert_eq!(s.info_template, InformationTemplate::SymbolPlate);
        assert_eq!(s.info_arrow, Some(Direction::Left));
        assert_eq!(s.info_distance.as_deref(), Some("Ford|100 yds"));
        assert_eq!(s.class, SignClass::Warning);
        assert_eq!(s.catalog_key.as_deref(), Some("D504.1"));
        assert_eq!(s.mirror, SignMirror::Right);
        assert_eq!(s.text_lines, Some(2));
        let embedded = s.embedded.unwrap();
        assert_eq!(embedded[0].kind, EmbeddedType::RegulatoryRoundel);
        assert_eq!(embedded[1].kind, EmbeddedType::WarningTriangle);
        assert_eq!(
            s.location,
            Some(SignLocation {
                alignment: Some("A1".into()),
                station: Some(1250.5),
                offset: Some(-3.5),
                side: Some(SignSide::Left)
            })
        );
    }

    #[test]
    fn destinations_carry_distance_symbol_and_route() {
        let s = parse_schedule_row("X,direction,60,primary,green,Lampton:ahead:1:12:airport:(M11):primary:m;Slough:RIGHT:0:::A4:np").unwrap();
        let d = &s.destinations[0];
        assert_eq!(
            (d.name.as_str(), d.direction, d.order),
            ("Lampton", Direction::Ahead, 1)
        );
        assert_eq!(d.distance.as_deref(), Some("12"));
        assert_eq!(d.symbol_id.as_deref(), Some("airport"));
        assert_eq!(d.route_number.as_deref(), Some("(M11)"));
        assert_eq!(
            (d.route, d.number_status),
            (RouteStatus::Primary, RouteStatus::Motorway)
        );
        let d = &s.destinations[1];
        assert_eq!(
            (
                d.distance.as_deref(),
                d.symbol_id.as_deref(),
                d.route_number.as_deref()
            ),
            (None, None, Some("A4"))
        );
        assert_eq!(
            (d.route, d.number_status),
            (RouteStatus::NonPrimary, RouteStatus::Unspecified)
        );
    }

    #[test]
    fn kind_tokens_ignore_case_hyphens_and_underscores() {
        assert_eq!(
            parse_kind("Map_Roundabout"),
            Some(SignKind::MapTypeRoundabout)
        );
        assert_eq!(parse_kind("placeid"), Some(SignKind::PlaceIdentification));
        assert_eq!(parse_kind("GIVE-WAY"), Some(SignKind::GiveWay));
        assert_eq!(parse_kind("notakind"), None);
        assert_eq!(parse_kind(" "), None);
    }

    #[test]
    fn bad_rows_say_which_row_and_why() {
        let msg = |row: &str| parse_schedule_row(row).unwrap_err().0;
        assert!(
            msg("A,notakind,64,primary,green,X:Ahead:0").contains("row 1: unknown kind 'notakind'")
        );
        assert!(msg(",stack,64,primary,green,X:Ahead:0").contains("'code' is empty"));
        assert!(
            msg("A,stack,fast,primary,green,X:Ahead:0").contains("'speed_kph' is not an integer")
        );
        assert!(msg("A,stack,64,trunk,green,X:Ahead:0").contains("unknown approach_class 'trunk'"));
        assert!(
            msg("A,stack,64,primary,pink,X:Ahead:0").contains("unknown background colour 'pink'")
        );
        assert!(msg("A,stack,64,primary,green,X:Up:0").contains("unknown direction 'Up'"));
        assert!(msg("A,stack,64,primary,green,X:Ahead").contains("must be Name:Direction:Order"));
        assert!(msg("A,stack,64,primary,green,X:Ahead:first").contains("non-integer order"));
        assert!(msg("A,stack,64,primary").contains("missing column 'background'"));
        assert!(msg("A,stack,64,primary,green,X:Ahead:0,,,,,,,,,,0")
            .contains("text_lines must be a positive integer"));
        assert!(msg("A,stack\n,64").contains("single CSV data line"));
        assert!(msg("A,stack,64,primary,green,X:Ahead:0:::A1:trunk")
            .contains("unknown route status 'trunk'"));
        assert!(msg("A,stack,-5,primary,green,X:Ahead:0").contains("must not be negative"));
    }

    #[test]
    fn metres_accept_numbers_and_chainages() {
        assert_eq!(parse_metres("1250.5"), Some(1250.5));
        assert_eq!(parse_metres("-3.5"), Some(-3.5));
        assert_eq!(parse_metres("+3.5"), Some(3.5));
        assert_eq!(parse_metres("1+250.5"), Some(1250.5));
        assert_eq!(parse_metres("1e+3"), Some(1000.0));
        assert_eq!(parse_metres("1+1000"), None);
        assert_eq!(parse_metres("-1+250"), None);
        assert_eq!(parse_metres("abc"), None);
    }

    #[test]
    fn quoted_fields_keep_commas_and_doubled_quotes() {
        assert_eq!(
            split_line(r#"a,"b,c","d ""e""",,f"#),
            vec!["a", "b,c", "d \"e\"", "", "f"]
        );
    }

    #[test]
    fn header_columns_may_come_in_any_order_and_case() {
        let header =
            parse_header("Destinations,BACKGROUND,approach_class,speed_kph,kind,code").unwrap();
        let s = parse_row("Kasese:Ahead:0,green,primary,60,stack,S1", &header, 2).unwrap();
        assert_eq!((s.code.as_str(), s.speed_kph), ("S1", 60));
        assert!(parse_header("code,kind")
            .unwrap_err()
            .0
            .contains("missing required column 'speed_kph'"));
        assert!(parse_header("code,kind,code")
            .unwrap_err()
            .0
            .contains("Duplicate header column 'code'"));
    }

    #[test]
    fn a_schedule_file_skips_blank_and_comment_lines_and_numbers_rows_by_file_line() {
        let tmp = crate::testing::TempDir::new("schedule");
        let path = tmp.path().join("s.csv");
        std::fs::write(
            &path,
            "\u{feff}code,kind,speed_kph,approach_class,background,destinations\r\n# comment\r\n\r\nOK,stack,64,primary,green,Lampton:Ahead:0\r\nBAD,notakind,64,primary,green,Lampton:Ahead:0\r\n",
        )
        .unwrap();
        let rows = read_schedule(&path).unwrap();
        assert_eq!(rows.iter().map(|r| r.row).collect::<Vec<_>>(), vec![4, 5]);
        assert_eq!(rows[0].sign.as_ref().unwrap().code, "OK");
        assert!(rows[1]
            .sign
            .as_ref()
            .unwrap_err()
            .starts_with("row 5: unknown kind"));

        std::fs::write(&path, "").unwrap();
        assert!(read_schedule(&path)
            .unwrap_err()
            .0
            .contains("schedule is empty"));
        std::fs::write(&path, "code,kind\n").unwrap();
        assert!(read_schedule(&path)
            .unwrap_err()
            .0
            .contains("missing required column"));
        assert!(read_schedule(&tmp.path().join("absent.csv"))
            .unwrap_err()
            .0
            .contains("could not read"));
    }
}
