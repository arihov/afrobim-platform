//! The parameters of a sign definition that a schedule can set, and the legend text format.
//!
//! A `param:<name>` column sets one field of the [`SignDefinition`] the rule pack's catalog entry
//! seeds. The legend column sets the destinations of the sign types that have them.

use std::collections::BTreeMap;

use afrobim_signs::{
    Colour, ColourScheme, Destination, Direction, InformationTemplate, RoadClass, SignDefinition, SignKind, SignMirror, SignVariant,
};

/// What a parameter holds, for the documentation and the error messages.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ParamSpec {
    pub name: &'static str,
    /// The accepted values, as text.
    pub accepts: &'static str,
}

/// The parameters a schedule can set.
pub const PARAMS: &[ParamSpec] = &[
    ParamSpec { name: "speed_kph", accepts: "a whole number of km/h: the design speed that sets the text size band" },
    ParamSpec { name: "approach_class", accepts: "primary, secondary or local" },
    ParamSpec { name: "scheme", accepts: "auto, primary, secondary, information, tourist, temporary or motorway" },
    ParamSpec { name: "variant", accepts: "primary or secondary" },
    ParamSpec { name: "background", accepts: "white, black, green, blue, brown, yellow, red or grey" },
    ParamSpec { name: "mirror", accepts: "none, left or right" },
    ParamSpec { name: "text_lines", accepts: "a whole number of lines (supplementary plates)" },
    ParamSpec { name: "info_arrow", accepts: "ahead, left, right or none (information signs)" },
    ParamSpec { name: "info_distance", accepts: "text (information signs)" },
    ParamSpec { name: "info_template", accepts: "symbol_plate or service_panel (information signs)" },
    ParamSpec { name: "symbol_id", accepts: "a symbol id of the rule pack" },
];

pub fn is_known_param(name: &str) -> bool {
    PARAMS.iter().any(|p| p.name == name)
}

/// True for the sign types whose face is lettered destinations, so a schedule row for one must
/// give a legend.
pub fn takes_legend(kind: SignKind) -> bool {
    matches!(
        kind,
        SignKind::StackType
            | SignKind::MapTypeCrossroads
            | SignKind::MapTypeRoundabout
            | SignKind::MapTypeTee
            | SignKind::DirectionSign
            | SignKind::Confirmatory
            | SignKind::PlaceIdentification
            | SignKind::LanePreselection
            | SignKind::Diversion
    )
}

/// Sets one parameter. `name` is lower case. A blank `value` is never passed here: a blank cell
/// leaves the catalog's own value.
pub fn apply_param(def: &mut SignDefinition, name: &str, value: &str) -> Result<(), String> {
    let v = value.trim();
    let lower = v.to_lowercase();
    let bad = |what: &str| -> String { format!("'{v}' is not {what}") };
    match name {
        "speed_kph" => def.speed_kph = v.parse::<u32>().map_err(|_| bad("a whole number of km/h"))?,
        "approach_class" => {
            def.approach_class = match lower.as_str() {
                "primary" => RoadClass::Primary,
                "secondary" => RoadClass::Secondary,
                "local" => RoadClass::Local,
                _ => return Err(bad("primary, secondary or local")),
            }
        }
        "scheme" => {
            def.scheme = match lower.as_str() {
                "auto" => ColourScheme::Auto,
                "primary" => ColourScheme::Primary,
                "secondary" => ColourScheme::Secondary,
                "information" => ColourScheme::Information,
                "tourist" => ColourScheme::Tourist,
                "temporary" => ColourScheme::Temporary,
                "motorway" => ColourScheme::Motorway,
                _ => return Err(bad("a colour scheme")),
            }
        }
        "variant" => {
            def.variant = match lower.as_str() {
                "primary" => SignVariant::Primary,
                "secondary" => SignVariant::Secondary,
                _ => return Err(bad("primary or secondary")),
            }
        }
        "background" => {
            def.background = match lower.as_str() {
                "white" => Colour::White,
                "black" => Colour::Black,
                "green" => Colour::Green,
                "blue" => Colour::Blue,
                "brown" => Colour::Brown,
                "yellow" => Colour::Yellow,
                "red" => Colour::Red,
                "grey" | "gray" => Colour::Grey,
                _ => return Err(bad("a colour")),
            }
        }
        "mirror" => {
            def.mirror = match lower.as_str() {
                "none" => SignMirror::None,
                "left" => SignMirror::Left,
                "right" => SignMirror::Right,
                _ => return Err(bad("none, left or right")),
            }
        }
        "text_lines" => def.text_lines = Some(v.parse::<u32>().map_err(|_| bad("a whole number of lines"))?),
        "info_arrow" => {
            def.info_arrow = match lower.as_str() {
                "none" => None,
                "ahead" => Some(Direction::Ahead),
                "left" => Some(Direction::Left),
                "right" => Some(Direction::Right),
                _ => return Err(bad("ahead, left, right or none")),
            }
        }
        "info_distance" => def.info_distance = Some(v.to_string()),
        "info_template" => {
            def.info_template = match lower.as_str() {
                "symbol_plate" => InformationTemplate::SymbolPlate,
                "service_panel" => InformationTemplate::ServicePanel,
                _ => return Err(bad("symbol_plate or service_panel")),
            }
        }
        "symbol_id" => def.symbol_id = Some(v.to_string()),
        other => return Err(format!("there is no parameter '{other}'")),
    }
    Ok(())
}

/// The value of each parameter of a definition, as `apply_param` reads it. Parameters that are
/// unset (`None`) are left out.
pub fn read_params(def: &SignDefinition) -> BTreeMap<String, String> {
    let mut m = BTreeMap::new();
    m.insert("speed_kph".to_string(), def.speed_kph.to_string());
    m.insert("approach_class".to_string(), format!("{:?}", def.approach_class).to_lowercase());
    m.insert("scheme".to_string(), format!("{:?}", def.scheme).to_lowercase());
    m.insert("variant".to_string(), format!("{:?}", def.variant).to_lowercase());
    m.insert("background".to_string(), format!("{:?}", def.background).to_lowercase());
    m.insert("mirror".to_string(), format!("{:?}", def.mirror).to_lowercase());
    if let Some(n) = def.text_lines {
        m.insert("text_lines".to_string(), n.to_string());
    }
    m.insert("info_arrow".to_string(), def.info_arrow.map_or("none".to_string(), |d| format!("{d:?}").to_lowercase()));
    if let Some(d) = &def.info_distance {
        m.insert("info_distance".to_string(), d.clone());
    }
    m.insert(
        "info_template".to_string(),
        match def.info_template {
            InformationTemplate::SymbolPlate => "symbol_plate",
            InformationTemplate::ServicePanel => "service_panel",
        }
        .to_string(),
    );
    if let Some(s) = &def.symbol_id {
        m.insert("symbol_id".to_string(), s.clone());
    }
    m
}

// ---- legend -------------------------------------------------------------------------------

const SPECIAL: [char; 5] = ['|', '@', '#', '~', '\\'];

fn escape(s: &str) -> String {
    let mut out = String::new();
    for c in s.chars() {
        if SPECIAL.contains(&c) {
            out.push('\\');
        }
        out.push(c);
    }
    out
}

/// Parses the legend text into destinations.
///
/// Destinations are separated by `|`. Each is `Name`, then optionally `@Direction` (Ahead, Left
/// or Right; Ahead when left out), `#distance` and `~route number`, in that order. A literal
/// `| @ # ~ \` in a name is written with a backslash before it. A destination's proximity order
/// is its position among the destinations of the same direction, nearest first.
pub fn parse_legend(text: &str) -> Result<Vec<Destination>, String> {
    // Split into items on unescaped '|', keeping escapes for the second pass.
    let mut items: Vec<String> = vec![String::new()];
    let mut chars = text.chars();
    while let Some(c) = chars.next() {
        match c {
            '\\' => {
                let n = chars.next().ok_or("the legend ends with a lone backslash")?;
                let cur = items.last_mut().unwrap();
                cur.push('\\');
                cur.push(n);
            }
            '|' => items.push(String::new()),
            c => items.last_mut().unwrap().push(c),
        }
    }
    let mut out: Vec<Destination> = Vec::new();
    for (i, item) in items.iter().enumerate() {
        if item.trim().is_empty() {
            return Err(format!("destination {} of the legend is empty", i + 1));
        }
        // Split the item into name and tagged parts at unescaped @ # ~.
        let mut parts: Vec<(char, String)> = vec![(' ', String::new())];
        let mut it = item.chars();
        while let Some(c) = it.next() {
            match c {
                '\\' => {
                    let n = it.next().unwrap_or('\\');
                    parts.last_mut().unwrap().1.push(n);
                }
                '@' | '#' | '~' => parts.push((c, String::new())),
                c => parts.last_mut().unwrap().1.push(c),
            }
        }
        let name = parts[0].1.trim().to_string();
        if name.is_empty() {
            return Err(format!("destination {} of the legend has no name", i + 1));
        }
        let mut d = Destination::new(name, Direction::Ahead);
        let mut seen = String::new();
        for (tag, value) in &parts[1..] {
            if seen.contains(*tag) {
                return Err(format!("destination '{}' has more than one '{tag}' part", d.name));
            }
            seen.push(*tag);
            let value = value.trim();
            match tag {
                '@' => {
                    d.direction = match value.to_lowercase().as_str() {
                        "ahead" => Direction::Ahead,
                        "left" => Direction::Left,
                        "right" => Direction::Right,
                        _ => return Err(format!("destination '{}': '{value}' is not a direction (Ahead, Left or Right)", d.name)),
                    }
                }
                '#' => d.distance = (!value.is_empty()).then(|| value.to_string()),
                '~' => d.route_number = (!value.is_empty()).then(|| value.to_string()),
                _ => unreachable!(),
            }
        }
        out.push(d);
    }
    // Proximity order within a direction.
    for dir in [Direction::Ahead, Direction::Left, Direction::Right] {
        let mut n = 0;
        for d in out.iter_mut().filter(|d| d.direction == dir) {
            d.order = n;
            n += 1;
        }
    }
    Ok(out)
}

/// Writes destinations as legend text, and lists what the schedule format cannot carry (symbol,
/// route status, a proximity order other than the position in the list), which is lost.
pub fn write_legend(dests: &[Destination]) -> (String, Vec<String>) {
    let mut lost = Vec::new();
    let mut parts = Vec::new();
    for d in dests {
        let mut s = escape(&d.name);
        s.push_str(&format!("@{:?}", d.direction));
        if let Some(x) = d.distance.as_deref().filter(|x| !x.is_empty()) {
            s.push('#');
            s.push_str(&escape(x));
        }
        if let Some(x) = d.route_number.as_deref().filter(|x| !x.is_empty()) {
            s.push('~');
            s.push_str(&escape(x));
        }
        parts.push(s);
        if d.symbol_id.as_deref().is_some_and(|x| !x.is_empty()) {
            lost.push(format!("the symbol of '{}'", d.name));
        }
        if d.route != afrobim_signs::RouteStatus::Unspecified || d.number_status != afrobim_signs::RouteStatus::Unspecified {
            lost.push(format!("the route status of '{}'", d.name));
        }
    }
    // Order is rebuilt from the position among the same direction.
    for dir in [Direction::Ahead, Direction::Left, Direction::Right] {
        for (n, d) in dests.iter().filter(|d| d.direction == dir).enumerate() {
            if d.order != n as i32 {
                lost.push(format!("the proximity order of '{}'", d.name));
            }
        }
    }
    (parts.join(" | "), lost)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn legend_parses_and_writes() {
        let d = parse_legend("Kasese@Ahead#12 km | Masheruka@Right | Fort Portal@Ahead").unwrap();
        assert_eq!(d.len(), 3);
        assert_eq!((d[0].name.as_str(), d[0].direction, d[0].distance.as_deref(), d[0].order), ("Kasese", Direction::Ahead, Some("12 km"), 0));
        assert_eq!((d[1].direction, d[1].order), (Direction::Right, 0));
        assert_eq!((d[2].direction, d[2].order), (Direction::Ahead, 1));
        let (text, lost) = write_legend(&d);
        assert!(lost.is_empty());
        assert_eq!(parse_legend(&text).unwrap(), d);
    }

    #[test]
    fn legend_escapes_and_errors() {
        let d = parse_legend(r"A\|B@Left~A11").unwrap();
        assert_eq!((d[0].name.as_str(), d[0].route_number.as_deref()), ("A|B", Some("A11")));
        assert_eq!(parse_legend(&write_legend(&d).0).unwrap(), d);
        assert!(parse_legend("A@Up").unwrap_err().contains("not a direction"));
        assert!(parse_legend("A | ").unwrap_err().contains("empty"));
        assert!(parse_legend("@Left").unwrap_err().contains("no name"));
        assert!(parse_legend("A@Left@Right").unwrap_err().contains("more than one"));
    }

    #[test]
    fn params_apply_and_read_back() {
        let mut def = SignDefinition::stack("X", 60, vec![]);
        apply_param(&mut def, "speed_kph", "80").unwrap();
        apply_param(&mut def, "scheme", "Primary").unwrap();
        apply_param(&mut def, "background", "Blue").unwrap();
        apply_param(&mut def, "info_arrow", "left").unwrap();
        let m = read_params(&def);
        assert_eq!((m["speed_kph"].as_str(), m["scheme"].as_str(), m["background"].as_str(), m["info_arrow"].as_str()), ("80", "primary", "blue", "left"));
        assert!(apply_param(&mut def, "speed_kph", "fast").is_err());
        assert!(apply_param(&mut def, "colour", "red").is_err());
        for (k, v) in &m {
            let mut again = def.clone();
            apply_param(&mut again, k, v).unwrap();
            assert_eq!(again, def, "{k}");
        }
    }
}
