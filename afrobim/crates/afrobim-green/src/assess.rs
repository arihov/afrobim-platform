use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

use crate::factors::{normalise_key, Factor, FactorSet};
use crate::rules::{Rule, Settings, Use};

/// A quantity line as the app hands it over (the crate does not depend on the cost crate).
#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct LineIn {
    pub id: usize,
    pub mark: String,
    pub category: String,
    pub description: String,
    pub quantity: f64,
    /// `m`, `m2`, `m3`, `kg`, `t` or `nr`.
    pub unit: String,
    pub attributes: BTreeMap<String, String>,
}

/// What became of a line.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum Outcome {
    /// Carbon and/or transport was calculated.
    Counted,
    /// Left out on purpose, with the rule's reason.
    Excluded(String),
    /// Could not be calculated, with the reason. Never costed at zero.
    Unmatched(String),
    /// A luminaire line, counted in the lighting block only.
    Energy,
}

/// One line's result.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Row {
    /// The line's `id`.
    pub line: usize,
    pub mark: String,
    pub category: String,
    pub description: String,
    pub outcome: Outcome,
    /// The factor key the rule resolved to; empty when the line never got that far.
    pub material_key: String,
    pub tonnes: f64,
    pub waste_t: f64,
    pub a1a3_kg: f64,
    pub a4_kg: f64,
    /// `a1a3_kg + a4_kg`.
    pub total_kg: f64,
}

/// Totals for one factor.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct MaterialTotal {
    pub key: String,
    pub material: String,
    pub tonnes: f64,
    pub waste_t: f64,
    /// A1-A3 plus A4 for the factor's lines.
    pub kgco2e: f64,
}

/// Street lighting energy. Every figure is None when an input it needs is missing.
#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
pub struct LightingResult {
    pub luminaires: f64,
    /// Over the luminaire lines that carry a `power_w` attribute.
    pub installed_kw: f64,
    pub kwh_per_year: Option<f64>,
    pub kgco2e_per_year: Option<f64>,
    pub life_kgco2e: Option<f64>,
    /// The inputs that are absent or unusable, by name.
    pub missing: Vec<String>,
    pub lines_without_power: usize,
}

/// The whole result.
#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
pub struct Assessment {
    /// Factor file name and edition; empty when no file was loaded.
    pub factor_set: String,
    pub rows: Vec<Row>,
    pub by_material: Vec<MaterialTotal>,
    /// Sum of the counted lines' `total_kg` (A1-A3 plus A4). None when no factor file is loaded
    /// or nothing was counted: never 0 standing for "zero carbon".
    pub embodied_kgco2e: Option<f64>,
    /// The A4 part of `embodied_kgco2e`. None when no counted line had haul and a transport factor.
    pub transport_kgco2e: Option<f64>,
    pub counted: usize,
    pub excluded: usize,
    pub unmatched: usize,
    pub energy_lines: usize,
    pub lighting: LightingResult,
    pub notes: Vec<String>,
}

fn unit_eq(a: &str, b: &str) -> bool {
    a.trim().eq_ignore_ascii_case(b.trim())
}

fn valid(v: Option<f64>) -> Result<Option<f64>, ()> {
    match v {
        Some(x) if x.is_finite() && x >= 0.0 => Ok(Some(x)),
        Some(_) => Err(()),
        None => Ok(None),
    }
}

struct Figures {
    tonnes: f64,
    waste_t: f64,
    a1a3: f64,
    a4: f64,
}

fn tonnes_of(line: &LineIn, f: &Factor, key: &str) -> Result<f64, String> {
    let q = line.quantity;
    let unit = line.unit.trim().to_lowercase();
    let per = |v: Option<f64>, name: &str| v.map(|x| q * x).ok_or_else(|| format!("{key}: no {name} in the factor file"));
    match unit.as_str() {
        "t" => Ok(q),
        "kg" => Ok(q / 1000.0),
        "m3" => per(f.t_per_m3, "t_per_m3"),
        "m2" => per(f.t_per_m2, "t_per_m2"),
        "m" => per(f.t_per_m, "t_per_m"),
        "nr" => per(f.t_per_nr, "t_per_nr"),
        other => Err(format!("{key}: the unit '{other}' cannot be turned into tonnes")),
    }
}

/// Works out a counting rule for one line, or says why it cannot.
fn evaluate(line: &LineIn, rule: &Rule, key: &str, set: Option<&FactorSet>) -> Result<Figures, String> {
    let Some(set) = set else {
        return Err("no factor file loaded".to_string());
    };
    let Some(factor) = set.find(key) else {
        return Err(format!("no factor for {key}"));
    };
    let waste = valid(rule.waste_pct).map_err(|()| format!("{key}: waste_pct is not a non-negative number"))?;
    let haul = valid(rule.haul_km).map_err(|()| format!("{key}: haul_km is not a non-negative number"))?;
    let tonnes = tonnes_of(line, factor, key)?;
    let waste_t = waste.map_or(0.0, |p| tonnes * p / 100.0);
    let moved = tonnes + waste_t;
    let transport = set.transport_kgco2e_per_tkm.filter(|v| v.is_finite() && *v >= 0.0);
    let a4 = match (haul, transport) {
        (Some(h), Some(t)) => Some(moved * h * t),
        _ => None,
    };
    match &rule.use_ {
        Use::HaulOnly { .. } => {
            let mut missing = Vec::new();
            if haul.is_none() {
                missing.push("haul_km on the rule");
            }
            if transport.is_none() {
                missing.push("transport_kgco2e_per_tkm in the factor file");
            }
            match a4 {
                Some(a4) => Ok(Figures { tonnes, waste_t, a1a3: 0.0, a4 }),
                None => Err(format!("{key}: no {}", missing.join(" and no "))),
            }
        }
        _ => {
            if !factor.has_carbon() {
                return Err(format!("{key}: no kgco2e_per_t in the factor file"));
            }
            Ok(Figures { tonnes, waste_t, a1a3: moved * factor.kgco2e_per_t, a4: a4.unwrap_or(0.0) })
        }
    }
}

/// Assesses the lines. See the crate documentation for every formula.
pub fn assess(lines: &[LineIn], factors: Option<&FactorSet>, settings: &Settings) -> Assessment {
    let mut a = Assessment { factor_set: factors.map(FactorSet::label).unwrap_or_default(), ..Assessment::default() };
    let mut totals: BTreeMap<String, MaterialTotal> = BTreeMap::new();
    let mut energy: Vec<&LineIn> = Vec::new();
    let mut transport_used = false;
    let mut haul_without_factor = false;
    let transport_factor = factors.and_then(|s| s.transport_kgco2e_per_tkm).is_some_and(|v| v.is_finite() && v >= 0.0);

    for line in lines {
        let mut row = Row {
            line: line.id,
            mark: line.mark.clone(),
            category: line.category.clone(),
            description: line.description.clone(),
            outcome: Outcome::Counted,
            material_key: String::new(),
            tonnes: 0.0,
            waste_t: 0.0,
            a1a3_kg: 0.0,
            a4_kg: 0.0,
            total_kg: 0.0,
        };
        row.outcome = match settings.rules.iter().find(|r| r.category == line.category) {
            None => Outcome::Unmatched(format!("no rule for {}", line.category)),
            Some(rule) => match &rule.use_ {
                Use::Excluded { reason } => Outcome::Excluded(reason.clone()),
                _ if !unit_eq(&rule.unit, &line.unit) => {
                    Outcome::Unmatched(format!("the rule for {} expects {}, the line is in {}", rule.category, rule.unit.trim(), line.unit.trim()))
                }
                Use::LightingEnergy if !line.quantity.is_finite() => Outcome::Unmatched("the quantity is not a finite number".to_string()),
                Use::LightingEnergy => {
                    energy.push(line);
                    Outcome::Energy
                }
                _ if !line.quantity.is_finite() => Outcome::Unmatched("the quantity is not a finite number".to_string()),
                counting => {
                    let key = match counting {
                        Use::Material { key } | Use::HaulOnly { key } => Ok(key.clone()),
                        Use::MaterialFromAttribute { attribute, prefix } => match line.attributes.get(attribute) {
                            Some(v) if !v.trim().is_empty() => Ok(format!("{prefix}{}", v.trim())),
                            _ => Err(format!("the line has no attribute '{attribute}'")),
                        },
                        _ => Err(String::new()),
                    };
                    match key {
                        Err(why) => Outcome::Unmatched(why),
                        Ok(key) => {
                            row.material_key = key.clone();
                            match evaluate(line, rule, &key, factors) {
                                Err(why) => Outcome::Unmatched(why),
                                Ok(f) => {
                                    row.tonnes = f.tonnes;
                                    row.waste_t = f.waste_t;
                                    row.a1a3_kg = f.a1a3;
                                    row.a4_kg = f.a4;
                                    row.total_kg = f.a1a3 + f.a4;
                                    if rule.haul_km.is_some() {
                                        if transport_factor {
                                            transport_used = true;
                                        } else {
                                            haul_without_factor = true;
                                        }
                                    }
                                    Outcome::Counted
                                }
                            }
                        }
                    }
                }
            },
        };
        match &row.outcome {
            Outcome::Counted => {
                a.counted += 1;
                if let Some(factor) = factors.and_then(|s| s.find(&row.material_key)) {
                    let t = totals.entry(normalise_key(&factor.key)).or_insert_with(|| MaterialTotal {
                        key: factor.key.trim().to_string(),
                        material: factor.material.clone(),
                        tonnes: 0.0,
                        waste_t: 0.0,
                        kgco2e: 0.0,
                    });
                    t.tonnes += row.tonnes;
                    t.waste_t += row.waste_t;
                    t.kgco2e += row.total_kg;
                }
            }
            Outcome::Excluded(_) => a.excluded += 1,
            Outcome::Unmatched(_) => a.unmatched += 1,
            Outcome::Energy => a.energy_lines += 1,
        }
        a.rows.push(row);
    }

    a.by_material = totals.into_values().collect();
    if factors.is_some() && a.counted > 0 {
        a.embodied_kgco2e = Some(a.rows.iter().filter(|r| r.outcome == Outcome::Counted).map(|r| r.total_kg).sum());
        if transport_used {
            a.transport_kgco2e = Some(a.rows.iter().filter(|r| r.outcome == Outcome::Counted).map(|r| r.a4_kg).sum());
        }
    }
    a.lighting = lighting(&energy, factors, settings);

    if factors.is_none() {
        a.notes.push("No factor file is loaded: no material line could be calculated.".to_string());
    }
    if a.unmatched > 0 {
        a.notes.push(format!("{} line(s) are unmatched and are not in the totals: see the reasons on each row.", a.unmatched));
    }
    if haul_without_factor {
        a.notes.push("A rule has haul_km but the factor file has no transport_kgco2e_per_tkm: that transport is not counted.".to_string());
    }
    if a.lighting.lines_without_power > 0 {
        a.notes.push(format!(
            "{} luminaire line(s) have no usable power_w attribute: lighting energy is not calculated until every line has one.",
            a.lighting.lines_without_power
        ));
    }
    a
}

fn lighting(lines: &[&LineIn], factors: Option<&FactorSet>, settings: &Settings) -> LightingResult {
    let mut r = LightingResult::default();
    if lines.is_empty() {
        return r;
    }
    let mut watts = 0.0;
    for l in lines {
        r.luminaires += l.quantity;
        match l.attributes.get("power_w").and_then(|p| p.trim().parse::<f64>().ok()).filter(|p| p.is_finite() && *p >= 0.0) {
            Some(p) => watts += l.quantity * p,
            None => r.lines_without_power += 1,
        }
    }
    r.installed_kw = watts / 1000.0;
    let u = &settings.lighting;
    let hours = u.hours_per_year.filter(|v| v.is_finite() && *v >= 0.0);
    let dimming = u.dimming_factor.filter(|v| v.is_finite() && (0.0..=1.0).contains(v));
    let grid = factors.and_then(|s| s.grid_kgco2e_per_kwh).filter(|v| v.is_finite() && *v >= 0.0);
    let life = u.design_life_years.filter(|v| v.is_finite() && *v >= 0.0);
    if r.lines_without_power > 0 {
        r.missing.push("power_w on every luminaire line".to_string());
    }
    if hours.is_none() {
        r.missing.push("hours_per_year".to_string());
    }
    if dimming.is_none() {
        r.missing.push("dimming_factor (0 to 1)".to_string());
    }
    if grid.is_none() {
        r.missing.push(if factors.is_none() { "grid_kgco2e_per_kwh (no factor file loaded)" } else { "grid_kgco2e_per_kwh" }.to_string());
    }
    if life.is_none() {
        r.missing.push("design_life_years".to_string());
    }
    if r.lines_without_power == 0 {
        if let (Some(h), Some(d)) = (hours, dimming) {
            let kwh = r.installed_kw * h * d;
            r.kwh_per_year = Some(kwh);
            if let Some(g) = grid {
                let per_year = kwh * g;
                r.kgco2e_per_year = Some(per_year);
                r.life_kgco2e = life.map(|y| per_year * y);
            }
        }
    }
    r
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::default_rules;

    // Every number below is an invented test fixture chosen for easy hand arithmetic. None is a
    // real factor, density, waste allowance, distance, burning hours or grid figure.
    fn factor(key: &str, kg_per_t: f64) -> Factor {
        Factor { key: key.into(), material: format!("{key} material"), kgco2e_per_t: kg_per_t, ..Factor::default() }
    }

    fn line(id: usize, category: &str, quantity: f64, unit: &str) -> LineIn {
        LineIn { id, mark: format!("M{id}"), category: category.into(), description: "test".into(), quantity, unit: unit.into(), attributes: BTreeMap::new() }
    }

    fn rule_for(category: &str, unit: &str, use_: Use) -> Rule {
        Rule { category: category.into(), unit: unit.into(), use_, waste_pct: None, haul_km: None }
    }

    fn set_of(factors: Vec<Factor>) -> FactorSet {
        FactorSet { name: "Test".into(), edition: "T1".into(), boundary: "A1-A3".into(), factors, ..FactorSet::default() }
    }

    fn settings_with(rules: Vec<Rule>) -> Settings {
        Settings { rules, ..Settings::default() }
    }

    fn one(set: &FactorSet, rule: Rule, l: LineIn) -> Row {
        assess(&[l], Some(set), &settings_with(vec![rule])).rows.remove(0)
    }

    #[test]
    fn tonnes_by_unit() {
        let mut f = factor("m", 10.0);
        f.t_per_m3 = Some(2.0);
        f.t_per_m2 = Some(0.5);
        f.t_per_m = Some(0.25);
        f.t_per_nr = Some(4.0);
        let set = set_of(vec![f]);
        for (unit, q, tonnes) in [("m3", 3.0, 6.0), ("m2", 4.0, 2.0), ("m", 8.0, 2.0), ("nr", 2.0, 8.0), ("kg", 500.0, 0.5), ("t", 7.0, 7.0)] {
            let r = one(&set, rule_for("c", unit, Use::Material { key: "m".into() }), line(0, "c", q, unit));
            assert_eq!(r.outcome, Outcome::Counted, "{unit}");
            assert_eq!(r.tonnes, tonnes, "{unit}");
            assert_eq!(r.a1a3_kg, tonnes * 10.0, "{unit}");
            assert_eq!(r.total_kg, r.a1a3_kg);
        }
    }

    #[test]
    fn waste_and_haul_arithmetic() {
        let mut set = set_of(vec![factor("m", 100.0)]);
        set.transport_kgco2e_per_tkm = Some(0.1);
        // 100 t, 5 % waste: 5 t waste; (100 + 5) * 100 = 10,500 kg
        let mut r = rule_for("c", "t", Use::Material { key: "m".into() });
        r.waste_pct = Some(5.0);
        let row = one(&set, r, line(0, "c", 100.0, "t"));
        assert_eq!(row.waste_t, 5.0);
        assert_eq!(row.a1a3_kg, 10_500.0);
        assert_eq!(row.a4_kg, 0.0);
        // 10 t hauled 30 km at 0.1 kg/tkm: 30 kg
        let mut r = rule_for("c", "t", Use::Material { key: "m".into() });
        r.haul_km = Some(30.0);
        let row = one(&set, r, line(0, "c", 10.0, "t"));
        assert_eq!(row.a4_kg, 30.0);
        assert_eq!(row.a1a3_kg, 1000.0);
        assert_eq!(row.total_kg, 1030.0);
        // haul with no transport factor: counted, A4 not added, and a note says so
        let bare = set_of(vec![factor("m", 100.0)]);
        let mut r = rule_for("c", "t", Use::Material { key: "m".into() });
        r.haul_km = Some(30.0);
        let a = assess(&[line(0, "c", 10.0, "t")], Some(&bare), &settings_with(vec![r]));
        assert_eq!(a.rows[0].a4_kg, 0.0);
        assert_eq!(a.transport_kgco2e, None);
        assert!(a.notes.iter().any(|n| n.contains("transport_kgco2e_per_tkm")));
    }

    #[test]
    fn haul_only_counts_transport_alone() {
        let mut f = factor("fill", f64::NAN);
        f.t_per_m3 = Some(2.0);
        let mut set = set_of(vec![f]);
        let mut r = rule_for("e", "m3", Use::HaulOnly { key: "fill".into() });
        // no haul, no transport factor: both named
        let row = one(&set, r.clone(), line(0, "e", 10.0, "m3"));
        let Outcome::Unmatched(why) = &row.outcome else { panic!("{row:?}") };
        assert!(why.contains("haul_km") && why.contains("transport_kgco2e_per_tkm"), "{why}");
        r.haul_km = Some(30.0);
        set.transport_kgco2e_per_tkm = Some(0.1);
        let a = assess(&[line(0, "e", 10.0, "m3")], Some(&set), &settings_with(vec![r]));
        let row = &a.rows[0];
        assert_eq!(row.outcome, Outcome::Counted);
        assert_eq!(row.tonnes, 20.0);
        assert_eq!(row.a1a3_kg, 0.0);
        assert_eq!(row.a4_kg, 20.0 * 30.0 * 0.1);
        assert_eq!(a.embodied_kgco2e, Some(row.a4_kg));
        assert_eq!(a.transport_kgco2e, Some(row.a4_kg));
        assert_eq!(a.by_material.len(), 1);
    }

    #[test]
    fn lighting_energy_chain() {
        let mut set = set_of(vec![]);
        set.grid_kgco2e_per_kwh = Some(0.5);
        let mut l = line(0, "lighting.luminaire", 10.0, "nr");
        l.attributes.insert("power_w".into(), "100".into());
        let mut s = Settings {
            lighting: crate::LightingUse { hours_per_year: Some(4000.0), dimming_factor: Some(1.0), design_life_years: Some(20.0) },
            ..Settings::default()
        };
        let a = assess(std::slice::from_ref(&l), Some(&set), &s);
        assert_eq!(a.lighting.luminaires, 10.0);
        assert_eq!(a.lighting.installed_kw, 1.0);
        assert_eq!(a.lighting.kwh_per_year, Some(4000.0));
        assert_eq!(a.lighting.kgco2e_per_year, Some(2000.0));
        assert_eq!(a.lighting.life_kgco2e, Some(40_000.0));
        assert!(a.lighting.missing.is_empty());
        assert_eq!(a.energy_lines, 1);
        assert_eq!(a.embodied_kgco2e, None, "lighting energy never enters embodied carbon");
        // missing hours
        s.lighting.hours_per_year = None;
        let a = assess(std::slice::from_ref(&l), Some(&set), &s);
        assert_eq!(a.lighting.kwh_per_year, None);
        assert_eq!(a.lighting.kgco2e_per_year, None);
        assert_eq!(a.lighting.life_kgco2e, None);
        assert_eq!(a.lighting.missing, vec!["hours_per_year".to_string()]);
        // dimming is never assumed to be 1
        s.lighting.hours_per_year = Some(4000.0);
        s.lighting.dimming_factor = None;
        let a = assess(std::slice::from_ref(&l), Some(&set), &s);
        assert_eq!(a.lighting.kwh_per_year, None);
        assert!(a.lighting.missing.iter().any(|m| m.starts_with("dimming_factor")));
        // out of range dimming is unusable
        s.lighting.dimming_factor = Some(1.5);
        assert_eq!(assess(std::slice::from_ref(&l), Some(&set), &s).lighting.kwh_per_year, None);
        // a line with no power: counted, no energy figure, noted
        let bare = line(1, "lighting.luminaire", 2.0, "nr");
        s.lighting.dimming_factor = Some(1.0);
        let a = assess(&[l, bare], Some(&set), &s);
        assert_eq!(a.lighting.lines_without_power, 1);
        assert_eq!(a.lighting.kwh_per_year, None);
        assert_eq!(a.lighting.luminaires, 12.0);
        assert!(a.notes.iter().any(|n| n.contains("power_w")));
    }

    #[test]
    fn lighting_needs_no_factor_file_for_installed_power() {
        let mut l = line(0, "lighting.luminaire", 4.0, "nr");
        l.attributes.insert("power_w".into(), "50".into());
        let a = assess(&[l], None, &Settings::default());
        assert_eq!(a.lighting.installed_kw, 0.2);
        assert_eq!(a.lighting.kwh_per_year, None);
        assert!(a.lighting.missing.iter().any(|m| m.starts_with("grid_kgco2e_per_kwh")));
    }

    /// Index of `post.mass` among the default rules, which is also its line id in `mixed_lines`.
    fn post_index() -> usize {
        default_rules().iter().position(|r| r.category == "post.mass").unwrap()
    }

    fn mixed_lines() -> Vec<LineIn> {
        let mut lines: Vec<LineIn> = default_rules().iter().enumerate().map(|(i, r)| line(i, &r.category, 1.0 + i as f64, &r.unit)).collect();
        let n = lines.len();
        lines.push(line(n, "something.unknown", 3.0, "m"));
        let mut asphalt = line(n + 1, "pavement.layer_volume", 5.0, "m3");
        asphalt.attributes.insert("material".into(), "Hot mix asphalt".into());
        lines.push(asphalt);
        lines.push(line(n + 2, "post.mass", 5.0, "m3")); // wrong unit
        lines.push(line(n + 3, "post.mass", f64::NAN, "kg"));
        lines
    }

    #[test]
    fn partition_invariant() {
        let lines = mixed_lines();
        let mut set = set_of(vec![factor("steel post", 2.0), factor("pavement: hot mix asphalt", 3.0)]);
        set.factors[1].t_per_m3 = Some(2.0);
        let s = Settings { lighting: crate::LightingUse { hours_per_year: Some(1.0), ..Default::default() }, ..Settings::default() };
        for factors in [None, Some(&set)] {
            let a = assess(&lines, factors, &s);
            assert_eq!(a.counted + a.excluded + a.unmatched + a.energy_lines, lines.len());
            assert_eq!(a.rows.len(), lines.len());
            assert!(a.energy_lines >= 1 && a.excluded >= 1 && a.unmatched >= 1);
        }
        let a = assess(&lines, Some(&set), &s);
        assert_eq!(a.counted, 2, "post.mass and the asphalt line: {:?}", a.rows.iter().filter(|r| r.outcome == Outcome::Counted).collect::<Vec<_>>());
        let post = a.rows.iter().find(|r| r.category == "post.mass" && r.outcome == Outcome::Counted).unwrap();
        assert_eq!(post.tonnes, (1.0 + post_index() as f64) / 1000.0);
        assert_eq!(a.by_material.len(), 2);
    }

    #[test]
    fn no_factor_file_never_reports_zero() {
        let lines = mixed_lines();
        let a = assess(&lines, None, &Settings::default());
        assert_eq!(a.embodied_kgco2e, None);
        assert_eq!(a.transport_kgco2e, None);
        assert_eq!(a.counted, 0);
        assert!(a.factor_set.is_empty());
        assert!(a.rows.iter().any(|r| r.outcome == Outcome::Unmatched("no factor file loaded".into())));
        // every correct-unit material line says so
        let r = a.rows.iter().find(|r| r.line == post_index()).unwrap();
        assert_eq!(r.outcome, Outcome::Unmatched("no factor file loaded".into()));
        // a factor file in which nothing matches is also None, not zero
        let empty = set_of(vec![]);
        let a = assess(&lines, Some(&empty), &Settings::default());
        assert_eq!(a.embodied_kgco2e, None);
        assert!(a.notes.iter().any(|n| n.contains("unmatched")));
    }

    #[test]
    fn unmatched_reasons() {
        let mut f = factor("m", 1.0);
        f.t_per_m3 = Some(1.0);
        let set = set_of(vec![f, factor("nocarbon", f64::NAN)]);
        let reason = |rule: Rule, l: LineIn| match one(&set, rule, l).outcome {
            Outcome::Unmatched(w) => w,
            o => panic!("{o:?}"),
        };
        assert_eq!(reason(rule_for("c", "m3", Use::Material { key: "m".into() }), line(0, "c", 1.0, "m2")), "the rule for c expects m3, the line is in m2");
        assert_eq!(reason(rule_for("c", "m2", Use::Material { key: "m".into() }), line(0, "c", 1.0, "m2")), "m: no t_per_m2 in the factor file");
        assert_eq!(reason(rule_for("c", "nr", Use::Material { key: "m".into() }), line(0, "c", 1.0, "nr")), "m: no t_per_nr in the factor file");
        assert_eq!(reason(rule_for("c", "m3", Use::Material { key: "absent".into() }), line(0, "c", 1.0, "m3")), "no factor for absent");
        assert_eq!(reason(rule_for("c", "t", Use::Material { key: "nocarbon".into() }), line(0, "c", 1.0, "t")), "nocarbon: no kgco2e_per_t in the factor file");
        assert_eq!(reason(rule_for("c", "m3", Use::Material { key: "m".into() }), line(0, "other", 1.0, "m3")), "no rule for other");
        let mut bad = rule_for("c", "m3", Use::Material { key: "m".into() });
        bad.waste_pct = Some(-1.0);
        assert!(reason(bad, line(0, "c", 1.0, "m3")).contains("waste_pct"));
        let attr = rule_for("c", "m3", Use::MaterialFromAttribute { attribute: "material".into(), prefix: "p: ".into() });
        assert_eq!(reason(attr, line(0, "c", 1.0, "m3")), "the line has no attribute 'material'");
    }

    #[test]
    fn attribute_key_ignores_case_and_spacing() {
        let mut f = factor(" Pavement: hot mix asphalt ", 10.0);
        f.t_per_m3 = Some(2.0);
        let set = set_of(vec![f]);
        let mut l = line(0, "pavement.layer_volume", 10.0, "m3");
        l.attributes.insert("material".into(), "Hot mix asphalt".into());
        let a = assess(&[l], Some(&set), &Settings::default());
        let r = &a.rows[0];
        assert_eq!(r.outcome, Outcome::Counted);
        assert_eq!(r.material_key, "pavement: Hot mix asphalt");
        assert_eq!(r.total_kg, 200.0);
        assert_eq!(a.by_material[0].key, "Pavement: hot mix asphalt");
        assert_eq!(a.embodied_kgco2e, Some(200.0));
        assert!(a.factor_set.starts_with("Test"));
    }

    #[test]
    fn first_matching_rule_wins_and_excluded_needs_no_factor_file() {
        let rules = vec![
            rule_for("c", "m3", Use::Excluded { reason: "first".into() }),
            rule_for("c", "m3", Use::Material { key: "m".into() }),
        ];
        let a = assess(&[line(0, "c", 1.0, "m3")], None, &settings_with(rules));
        assert_eq!(a.rows[0].outcome, Outcome::Excluded("first".into()));
        assert_eq!(a.excluded, 1);
        assert_eq!(a.embodied_kgco2e, None);
    }

    #[test]
    fn serde_round_trip() {
        let mut f = factor("m", 1.0);
        f.t_per_m3 = Some(2.0);
        let set = set_of(vec![f]);
        let a = assess(&mixed_lines(), Some(&set), &Settings::default());
        let json = serde_json::to_string(&a).unwrap();
        let back: Assessment = serde_json::from_str(&json).unwrap();
        assert_eq!(back, a);
    }
}
