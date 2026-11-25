//! The user's factor file: carbon factors, conversions to tonnes, the transport factor and the
//! grid factor, read from and written to CSV. Nothing here supplies a number.

use std::collections::HashMap;

use serde::{Deserialize, Deserializer, Serialize, Serializer};

use crate::csv;

/// The factor-file columns, in order.
pub const FACTOR_COLUMNS: [&str; 9] =
    ["key", "material", "kgco2e_per_t", "t_per_m3", "t_per_m2", "t_per_m", "t_per_nr", "source", "edition"];

/// Lower-cases, trims and collapses runs of white space, so `"Pavement:  Hot mix asphalt "` and
/// `"pavement: hot mix asphalt"` are the same key.
pub fn normalise_key(s: &str) -> String {
    s.split_whitespace().collect::<Vec<_>>().join(" ").to_lowercase()
}

/// "Not entered" is stored as NaN, written as an empty CSV cell and as JSON `null`.
mod carbon_serde {
    use super::*;

    pub fn serialize<S: Serializer>(v: &f64, s: S) -> Result<S::Ok, S::Error> {
        if v.is_finite() {
            s.serialize_some(v)
        } else {
            s.serialize_none()
        }
    }

    pub fn deserialize<'de, D: Deserializer<'de>>(d: D) -> Result<f64, D::Error> {
        Ok(Option::<f64>::deserialize(d)?.unwrap_or(f64::NAN))
    }
}

fn not_entered() -> f64 {
    f64::NAN
}

/// One row of the factor file: a carbon factor and the conversions that turn a quantity into
/// tonnes. Every figure is the user's, with the source and edition they took it from.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct Factor {
    /// What rules refer to; matched case-insensitively with white space trimmed and collapsed.
    pub key: String,
    pub material: String,
    /// kgCO2e per tonne. NaN means "not entered" (an empty cell): the line is reported
    /// unmatched, never priced at zero. Use [`Factor::has_carbon`] to test.
    #[serde(with = "carbon_serde", default = "not_entered")]
    pub kgco2e_per_t: f64,
    pub t_per_m3: Option<f64>,
    pub t_per_m2: Option<f64>,
    pub t_per_m: Option<f64>,
    pub t_per_nr: Option<f64>,
    pub source: String,
    pub edition: String,
}

/// Equal, with NaN ("not entered") equal to NaN.
fn same(a: f64, b: f64) -> bool {
    a == b || (a.is_nan() && b.is_nan())
}

fn same_opt(a: Option<f64>, b: Option<f64>) -> bool {
    match (a, b) {
        (Some(x), Some(y)) => same(x, y),
        (None, None) => true,
        _ => false,
    }
}

/// Field-wise equality in which a NaN `kgco2e_per_t` ("not entered") equals another NaN, so a
/// factor left blank in the file compares equal to itself.
impl PartialEq for Factor {
    fn eq(&self, other: &Self) -> bool {
        self.key == other.key
            && self.material == other.material
            && same(self.kgco2e_per_t, other.kgco2e_per_t)
            && same_opt(self.t_per_m3, other.t_per_m3)
            && same_opt(self.t_per_m2, other.t_per_m2)
            && same_opt(self.t_per_m, other.t_per_m)
            && same_opt(self.t_per_nr, other.t_per_nr)
            && self.source == other.source
            && self.edition == other.edition
    }
}

impl Default for Factor {
    fn default() -> Self {
        Factor {
            key: String::new(),
            material: String::new(),
            kgco2e_per_t: f64::NAN,
            t_per_m3: None,
            t_per_m2: None,
            t_per_m: None,
            t_per_nr: None,
            source: String::new(),
            edition: String::new(),
        }
    }
}

impl Factor {
    /// True when a carbon figure was entered.
    pub fn has_carbon(&self) -> bool {
        self.kgco2e_per_t.is_finite()
    }
}

/// A factor file: its provenance and its factors.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct FactorSet {
    pub name: String,
    pub source: String,
    pub edition: String,
    /// The life-cycle boundary the factors cover, as the user states it, e.g. `A1-A3`.
    pub boundary: String,
    pub factors: Vec<Factor>,
    pub transport_kgco2e_per_tkm: Option<f64>,
    pub grid_kgco2e_per_kwh: Option<f64>,
    pub grid_source: String,
}

impl PartialEq for FactorSet {
    fn eq(&self, other: &Self) -> bool {
        self.name == other.name
            && self.source == other.source
            && self.edition == other.edition
            && self.boundary == other.boundary
            && self.factors == other.factors
            && same_opt(self.transport_kgco2e_per_tkm, other.transport_kgco2e_per_tkm)
            && same_opt(self.grid_kgco2e_per_kwh, other.grid_kgco2e_per_kwh)
            && self.grid_source == other.grid_source
    }
}

impl FactorSet {
    /// The factor for a key, matched with [`normalise_key`]; the first such row wins.
    pub fn find(&self, key: &str) -> Option<&Factor> {
        let k = normalise_key(key);
        self.factors.iter().find(|f| normalise_key(&f.key) == k)
    }

    /// Name and edition for display, e.g. `My file (2024)`.
    pub fn label(&self) -> String {
        match (self.name.trim().is_empty(), self.edition.trim().is_empty()) {
            (true, true) => "(unnamed factor file)".to_string(),
            (false, true) => self.name.trim().to_string(),
            (true, false) => format!("({})", self.edition.trim()),
            (false, false) => format!("{} ({})", self.name.trim(), self.edition.trim()),
        }
    }
}

impl FactorSet {
    /// The provenance line for the window and exports, built only from what the file states:
    /// `Source: ICE v5.0 (Circular Ecology), A1-A3`. Rows whose own source differs from the
    /// file's (a supplier's EPD, the client's figures) are counted in a trailing clause. Nothing
    /// stated gives `Source: not stated`.
    pub fn source_label(&self) -> String {
        let src = self.source.trim();
        let mut parts: Vec<&str> = Vec::new();
        if !src.is_empty() {
            parts.push(src);
        }
        let b = self.boundary.trim();
        if !b.is_empty() {
            parts.push(b);
        }
        let mut out = if parts.is_empty() { "Source: not stated".to_string() } else { format!("Source: {}", parts.join(", ")) };
        let other = self.factors.iter().filter(|f| f.has_carbon() && !f.source.trim().is_empty() && !src.is_empty() && !f.source.trim().starts_with(src)).count();
        if other > 0 {
            out.push_str(&format!("; {other} material(s) use their own source"));
        }
        out
    }
}

fn number_cell(text: &str, column: &str, line: usize) -> Result<Option<f64>, String> {
    let t = text.trim();
    if t.is_empty() {
        return Ok(None);
    }
    match t.parse::<f64>() {
        Ok(v) if v.is_finite() && v >= 0.0 => Ok(Some(v)),
        _ => Err(format!("line {line}: {column}: '{t}' is not a non-negative finite number")),
    }
}

/// Reads a factor file.
///
/// Layout: optional header comment lines `# name: ...`, `# source: ...`, `# edition: ...`,
/// `# boundary: ...`, `# transport_kgco2e_per_tkm: ...`, `# grid_kgco2e_per_kwh: ...`,
/// `# grid_source: ...` (other `#` lines are ignored), then the header row
/// `key,material,kgco2e_per_t,t_per_m3,t_per_m2,t_per_m,t_per_nr,source,edition` and one row per
/// factor. Empty cells are None (an empty `kgco2e_per_t` means "not entered"). Fields may be
/// quoted, with `""` for a quote; a field cannot contain a line break. Negative, non-finite or
/// unreadable numbers, repeated keys and short rows are errors that name the 1-based line.
pub fn factors_from_csv(text: &str) -> Result<FactorSet, String> {
    let text = text.strip_prefix('\u{feff}').unwrap_or(text);
    let mut set = FactorSet::default();
    let mut header_seen = false;
    let mut seen: HashMap<String, usize> = HashMap::new();
    for (i, raw) in text.lines().enumerate() {
        let n = i + 1;
        let line = raw.trim();
        if line.is_empty() {
            continue;
        }
        if let Some(comment) = line.strip_prefix('#') {
            if !header_seen {
                header_comment(&mut set, comment, n)?;
            }
            continue;
        }
        let fields = csv::split(raw).map_err(|e| format!("line {n}: {e}"))?;
        if !header_seen {
            let names: Vec<String> = fields.iter().map(|f| f.trim().to_lowercase()).collect();
            if names != FACTOR_COLUMNS {
                return Err(format!("line {n}: expected the header row {}", FACTOR_COLUMNS.join(",")));
            }
            header_seen = true;
            continue;
        }
        if fields.len() != FACTOR_COLUMNS.len() {
            return Err(format!("line {n}: expected {} fields, found {}", FACTOR_COLUMNS.len(), fields.len()));
        }
        let key = fields[0].trim().to_string();
        if key.is_empty() {
            return Err(format!("line {n}: the key is empty"));
        }
        if let Some(first) = seen.insert(normalise_key(&key), n) {
            return Err(format!("line {n}: the key '{key}' repeats line {first}"));
        }
        set.factors.push(Factor {
            key,
            material: fields[1].trim().to_string(),
            kgco2e_per_t: number_cell(&fields[2], FACTOR_COLUMNS[2], n)?.unwrap_or(f64::NAN),
            t_per_m3: number_cell(&fields[3], FACTOR_COLUMNS[3], n)?,
            t_per_m2: number_cell(&fields[4], FACTOR_COLUMNS[4], n)?,
            t_per_m: number_cell(&fields[5], FACTOR_COLUMNS[5], n)?,
            t_per_nr: number_cell(&fields[6], FACTOR_COLUMNS[6], n)?,
            source: fields[7].trim().to_string(),
            edition: fields[8].trim().to_string(),
        });
    }
    if !header_seen {
        return Err("no header row (key,material,kgco2e_per_t,...) found".to_string());
    }
    Ok(set)
}

fn header_comment(set: &mut FactorSet, comment: &str, n: usize) -> Result<(), String> {
    let Some((name, value)) = comment.split_once(':') else {
        return Ok(());
    };
    let value = value.trim();
    match name.trim() {
        "name" => set.name = value.to_string(),
        "source" => set.source = value.to_string(),
        "edition" => set.edition = value.to_string(),
        "boundary" => set.boundary = value.to_string(),
        "grid_source" => set.grid_source = value.to_string(),
        "transport_kgco2e_per_tkm" => set.transport_kgco2e_per_tkm = number_cell(value, "transport_kgco2e_per_tkm", n)?,
        "grid_kgco2e_per_kwh" => set.grid_kgco2e_per_kwh = number_cell(value, "grid_kgco2e_per_kwh", n)?,
        _ => {}
    }
    Ok(())
}

fn number_out(v: Option<f64>) -> String {
    match v {
        Some(x) if x.is_finite() => x.to_string(),
        _ => String::new(),
    }
}

/// Writes a factor file that [`factors_from_csv`] reads back unchanged. Line breaks inside text
/// are written as spaces.
pub fn factors_to_csv(set: &FactorSet) -> String {
    let mut out = String::new();
    for (k, v) in [("name", &set.name), ("source", &set.source), ("edition", &set.edition), ("boundary", &set.boundary)] {
        out.push_str(&format!("# {k}: {}\n", one_line(v)));
    }
    out.push_str(&format!("# transport_kgco2e_per_tkm: {}\n", number_out(set.transport_kgco2e_per_tkm)));
    out.push_str(&format!("# grid_kgco2e_per_kwh: {}\n", number_out(set.grid_kgco2e_per_kwh)));
    out.push_str(&format!("# grid_source: {}\n", one_line(&set.grid_source)));
    out.push_str(&csv::row(&FACTOR_COLUMNS));
    for f in &set.factors {
        out.push_str(&csv::row(&[
            f.key.clone(),
            f.material.clone(),
            number_out(Some(f.kgco2e_per_t)),
            number_out(f.t_per_m3),
            number_out(f.t_per_m2),
            number_out(f.t_per_m),
            number_out(f.t_per_nr),
            f.source.clone(),
            f.edition.clone(),
        ]));
    }
    out
}

fn one_line(s: &str) -> String {
    s.split(['\n', '\r']).map(str::trim).collect::<Vec<_>>().join(" ")
}

/// A factor file with the given `(key, material)` rows and every number blank, for the user to
/// fill from the source they choose. The header comments say what each column and setting is.
pub fn template_csv(keys: &[(&str, &str)]) -> String {
    let mut out = String::new();
    out.push_str("# Factor file for afrobim-green. Every number is yours: enter it from the source you choose and cite it.\n");
    out.push_str("# An empty cell means not entered. A line whose factor or conversion is empty is reported unmatched, never priced at zero.\n");
    out.push_str("# Setting lines below: give each a value after the colon. Text after the colon may not run onto a second line.\n");
    out.push_str("# name: \n# source: \n# edition: \n# boundary: \n");
    out.push_str("# transport_kgco2e_per_tkm: \n# grid_kgco2e_per_kwh: \n# grid_source: \n");
    out.push_str("# Column key - what the rules look up; matched ignoring case and extra spaces.\n");
    out.push_str("# Column material - the name shown in reports.\n");
    out.push_str("# Column kgco2e_per_t - kgCO2e per tonne of the material, for the boundary named above.\n");
    out.push_str("# Columns t_per_m3, t_per_m2, t_per_m, t_per_nr - tonnes per cubic metre, square metre, metre and item, for the units the model reports.\n");
    out.push_str("# Columns source, edition - where this row's numbers come from.\n");
    out.push_str(&csv::row(&FACTOR_COLUMNS));
    for (k, m) in keys {
        out.push_str(&csv::row(&[*k, *m, "", "", "", "", "", "", ""]));
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    // All figures in these tests are invented fixtures chosen for easy arithmetic.
    #[test]
    fn round_trip_with_comments_quotes_and_blanks() {
        let set = FactorSet {
            name: "Test file".into(),
            source: "invented".into(),
            edition: "T1".into(),
            boundary: "A1-A3".into(),
            factors: vec![
                Factor {
                    key: "steel, post".into(),
                    material: "Steel \"post\"".into(),
                    kgco2e_per_t: 2.5,
                    t_per_m3: None,
                    t_per_m2: Some(0.25),
                    t_per_m: Some(0.125),
                    t_per_nr: None,
                    source: "a, b".into(),
                    edition: "x".into(),
                },
                Factor { key: "blank".into(), material: "Blank".into(), ..Factor::default() },
            ],
            transport_kgco2e_per_tkm: Some(0.1),
            grid_kgco2e_per_kwh: None,
            grid_source: "none".into(),
        };
        let text = factors_to_csv(&set);
        let back = factors_from_csv(&text).unwrap();
        assert_eq!(back.name, set.name);
        assert_eq!(back.boundary, "A1-A3");
        assert_eq!(back.transport_kgco2e_per_tkm, Some(0.1));
        assert_eq!(back.grid_kgco2e_per_kwh, None);
        assert_eq!(back.factors[0], set.factors[0]);
        assert_eq!(back.factors[1].key, "blank");
        assert!(!back.factors[1].has_carbon());
        assert_eq!(back.factors[1].t_per_m3, None);
        // a second pass is stable
        assert_eq!(factors_to_csv(&back), text);
    }

    #[test]
    fn errors_name_the_line() {
        let head = "key,material,kgco2e_per_t,t_per_m3,t_per_m2,t_per_m,t_per_nr,source,edition\n";
        let e = factors_from_csv(&format!("# name: x\n{head}a,A,-1,,,,,,\n")).unwrap_err();
        assert!(e.starts_with("line 3:"), "{e}");
        let e = factors_from_csv(&format!("{head}a,A,1,,,,,,\nb,B,inf,,,,,,\n")).unwrap_err();
        assert!(e.starts_with("line 3:"), "{e}");
        let e = factors_from_csv(&format!("{head}a,A,1,,,,,,\nb,B,NaN,,,,,,\n")).unwrap_err();
        assert!(e.starts_with("line 3:"), "{e}");
        let e = factors_from_csv(&format!("{head}a,A,1,,,,,,\nA ,B,1,,,,,,\n")).unwrap_err();
        assert!(e.contains("line 3") && e.contains("line 2"), "{e}");
        let e = factors_from_csv(&format!("{head}a,\"A,1,,,,,,\n")).unwrap_err();
        assert!(e.starts_with("line 2:"), "{e}");
        let e = factors_from_csv(&format!("{head}a,A,1\n")).unwrap_err();
        assert!(e.starts_with("line 2:"), "{e}");
        let e = factors_from_csv("# transport_kgco2e_per_tkm: -3\nkey\n").unwrap_err();
        assert!(e.starts_with("line 1:"), "{e}");
        let e = factors_from_csv("a,b,c\n").unwrap_err();
        assert!(e.starts_with("line 1:"), "{e}");
        assert!(factors_from_csv("# only comments\n").is_err());
    }

    #[test]
    fn template_parses_back_with_every_number_none() {
        let t = template_csv(&[("concrete", "Concrete"), ("earth fill", "Earth, fill")]);
        let set = factors_from_csv(&t).unwrap();
        assert_eq!(set.factors.len(), 2);
        assert_eq!(set.factors[1].material, "Earth, fill");
        for f in &set.factors {
            assert!(!f.has_carbon());
            assert!(f.t_per_m3.is_none() && f.t_per_m2.is_none() && f.t_per_m.is_none() && f.t_per_nr.is_none());
        }
        assert!(set.transport_kgco2e_per_tkm.is_none() && set.grid_kgco2e_per_kwh.is_none());
        assert!(set.name.is_empty() && set.boundary.is_empty());
    }

    #[test]
    fn not_entered_survives_json() {
        let f = Factor { key: "k".into(), ..Factor::default() };
        let json = serde_json::to_string(&f).unwrap();
        assert!(json.contains("\"kgco2e_per_t\":null"));
        let back: Factor = serde_json::from_str(&json).unwrap();
        assert!(!back.has_carbon());
        let missing: Factor = serde_json::from_str("{\"key\":\"k\"}").unwrap();
        assert!(!missing.has_carbon());
    }

    #[test]
    fn a_blank_factor_equals_itself() {
        let set = FactorSet { factors: vec![Factor { key: "k".into(), ..Factor::default() }], ..FactorSet::default() };
        assert!(set.factors[0].kgco2e_per_t.is_nan());
        assert_eq!(set, set.clone());
        let mut other = set.clone();
        other.factors[0].kgco2e_per_t = 1.0;
        assert_ne!(set, other);
        let mut other = set.clone();
        other.grid_kgco2e_per_kwh = Some(0.5);
        assert_ne!(set, other);
        let back: FactorSet = serde_json::from_str(&serde_json::to_string(&set).unwrap()).unwrap();
        assert_eq!(back, set);
    }

    #[test]
    fn find_ignores_case_and_spaces() {
        let set = FactorSet { factors: vec![Factor { key: "Pavement:  Hot mix asphalt ".into(), ..Factor::default() }], ..FactorSet::default() };
        assert!(set.find("pavement: hot mix asphalt").is_some());
        assert!(set.find("pavement: granular base").is_none());
    }
}
