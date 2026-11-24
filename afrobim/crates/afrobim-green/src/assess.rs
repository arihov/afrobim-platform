use std::collections::{BTreeMap, BTreeSet, HashSet};

use serde::{Deserialize, Serialize};

use crate::factors::{normalise_key, Factor, FactorSet};
use crate::rules::{Rule, Settings, Use};

/// What the embodied total covers, for labels: it is A1-A3 for every counted line and A4 only for
/// the lines whose rule has a haul distance (and a transport factor).
pub const EMBODIED_BASIS: &str = "A1-A3, plus A4 where a haul was entered";

/// Why a gantry's count line is unmatched when its steel mass line is missing.
const NO_STEEL_MASS: &str = "no steel mass: a member has no mass per metre";

/// A quantity line as the app hands it over (the crate does not depend on the cost crate).
#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct LineIn {
    pub id: usize,
    /// The model element the line was measured from, when it has one. Lines of one element and
    /// mark belong together (the parts of one gantry).
    pub element: Option<u64>,
    pub mark: String,
    pub category: String,
    pub description: String,
    pub quantity: f64,
    /// `m`, `m2`, `m3`, `kg`, `t` or `nr` (`no` is read as `nr`).
    pub unit: String,
    pub attributes: BTreeMap<String, String>,
}

/// What became of a line.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum Outcome {
    /// Carbon and/or transport was calculated.
    Counted,
    /// Left out on purpose, with the reason.
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
    /// Number of luminaires on the luminaire lines; 0 when there are none (see `energy_lines`
    /// in the assessment).
    pub luminaires: f64,
    /// Installed power. None when there are no luminaire lines or any of them lacks `power_w`:
    /// a partial sum would look like the whole.
    pub installed_kw: Option<f64>,
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
    /// Sum of the counted lines' `total_kg`: A1-A3 for every counted line, plus A4 for the lines
    /// that have a haul (see [`EMBODIED_BASIS`], `counted_with_a4`). None when no factor file is
    /// loaded or nothing was counted: never 0 standing for "zero carbon".
    pub embodied_kgco2e: Option<f64>,
    /// The A4 part of `embodied_kgco2e`. None when no counted line had a haul and a transport factor.
    pub transport_kgco2e: Option<f64>,
    pub counted: usize,
    /// Counted lines whose A4 was calculated (haul entered and a transport factor in the file).
    pub counted_with_a4: usize,
    /// Counted lines with no A4 in their total.
    pub counted_without_a4: usize,
    /// Counted lines whose rule has a waste allowance.
    pub counted_with_waste: usize,
    /// Counted lines with no waste allowance in their total.
    pub counted_without_waste: usize,
    pub excluded: usize,
    pub unmatched: usize,
    pub energy_lines: usize,
    pub lighting: LightingResult,
    pub notes: Vec<String>,
}

/// Units are compared trimmed, ignoring case; `no` is the other spelling of `nr`.
fn canonical_unit(u: &str) -> String {
    let l = u.trim().to_lowercase();
    if l == "no" {
        "nr".to_string()
    } else {
        l
    }
}

fn unit_eq(a: &str, b: &str) -> bool {
    canonical_unit(a) == canonical_unit(b)
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
    /// True when the A4 was calculated: a haul and a transport factor were both present.
    a4_applied: bool,
}

fn tonnes_of(line: &LineIn, f: &Factor, key: &str) -> Result<f64, String> {
    let q = line.quantity;
    let unit = canonical_unit(&line.unit);
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
                Some(a4) => Ok(Figures { tonnes, waste_t, a1a3: 0.0, a4, a4_applied: true }),
                None => Err(format!("{key}: no {}", missing.join(" and no "))),
            }
        }
        _ => {
            if !factor.has_carbon() {
                return Err(format!("{key}: no kgco2e_per_t in the factor file"));
            }
            Ok(Figures { tonnes, waste_t, a1a3: moved * factor.kgco2e_per_t, a4: a4.unwrap_or(0.0), a4_applied: a4.is_some() })
        }
    }
}

/// The rule for a line: the first with its category and unit. Failing that, the line's category
/// is looked at only to give a reason, never to count.
enum Found<'a> {
    Rule(&'a Rule),
    Excluded(String),
    Unmatched(String),
}

fn find_rule<'a>(rules: &'a [Rule], line: &LineIn) -> Found<'a> {
    if let Some(r) = rules.iter().find(|r| r.category == line.category && unit_eq(&r.unit, &line.unit)) {
        return Found::Rule(r);
    }
    let same_category: Vec<&Rule> = rules.iter().filter(|r| r.category == line.category).collect();
    match same_category.first() {
        None => Found::Unmatched(format!("no rule for {}", line.category)),
        Some(Rule { use_: Use::Excluded { reason }, .. }) => Found::Excluded(reason.clone()),
        Some(_) => {
            let mut units: Vec<String> = Vec::new();
            for r in same_category.iter().filter(|r| !matches!(r.use_, Use::Excluded { .. })) {
                let u = r.unit.trim().to_string();
                if !units.contains(&u) {
                    units.push(u);
                }
            }
            Found::Unmatched(format!("the rule for {} expects {}, the line is in {}", line.category, units.join(" or "), line.unit.trim()))
        }
    }
}

/// What was worked out beside a row, for the tallies.
#[derive(Default, Clone)]
struct Meta {
    rule_category: String,
    a4_applied: bool,
    waste_entered: bool,
    haul_entered: bool,
}

/// Assesses the lines. See the crate documentation for every formula.
pub fn assess(lines: &[LineIn], factors: Option<&FactorSet>, settings: &Settings) -> Assessment {
    let mut a = Assessment { factor_set: factors.map(FactorSet::label).unwrap_or_default(), ..Assessment::default() };
    let mut energy: Vec<&LineIn> = Vec::new();
    let mut rows: Vec<Row> = Vec::with_capacity(lines.len());
    let mut metas: Vec<Meta> = Vec::with_capacity(lines.len());
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
        let mut meta = Meta::default();
        row.outcome = match find_rule(&settings.rules, line) {
            Found::Excluded(reason) => Outcome::Excluded(reason),
            Found::Unmatched(why) => Outcome::Unmatched(why),
            Found::Rule(rule) => match &rule.use_ {
                Use::Excluded { reason } => Outcome::Excluded(reason.clone()),
                _ if !line.quantity.is_finite() => Outcome::Unmatched("the quantity is not a finite number".to_string()),
                _ if line.quantity < 0.0 => Outcome::Unmatched("negative quantity".to_string()),
                _ if line.quantity == 0.0 => Outcome::Excluded("quantity is 0: not modelled".to_string()),
                Use::LightingEnergy => {
                    energy.push(line);
                    Outcome::Energy
                }
                counting => match counting.key_for(&line.attributes) {
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
                                meta = Meta { rule_category: rule.category.clone(), a4_applied: f.a4_applied, waste_entered: rule.waste_pct.is_some(), haul_entered: rule.haul_km.is_some() };
                                if rule.haul_km.is_some() && !transport_factor {
                                    haul_without_factor = true;
                                }
                                Outcome::Counted
                            }
                        }
                    }
                },
            },
        };
        rows.push(row);
        metas.push(meta);
    }

    // A gantry whose members lack a mass per metre has no steel mass line, and the rest of its
    // lines are excluded on the ground that the steel mass carries it. Its count line says so,
    // so that its steel does not vanish without a word.
    let steel_rule_counts = settings.rules.iter().any(|r| r.category == "gantry.steel_mass" && r.use_.is_counting());
    if steel_rule_counts {
        let with_steel: HashSet<(Option<u64>, &str)> = lines.iter().filter(|l| l.category == "gantry.steel_mass").map(|l| (l.element, l.mark.as_str())).collect();
        for (row, line) in rows.iter_mut().zip(lines) {
            if line.category == "gantry.count" && matches!(row.outcome, Outcome::Excluded(_)) && !with_steel.contains(&(line.element, line.mark.as_str())) {
                row.outcome = Outcome::Unmatched(NO_STEEL_MASS.to_string());
            }
        }
    }

    let mut totals: BTreeMap<String, MaterialTotal> = BTreeMap::new();
    let mut no_haul_rules: BTreeSet<String> = BTreeSet::new();
    let mut no_waste_rules: BTreeSet<String> = BTreeSet::new();
    for (row, meta) in rows.iter().zip(&metas) {
        match &row.outcome {
            Outcome::Counted => {
                a.counted += 1;
                if meta.a4_applied {
                    a.counted_with_a4 += 1;
                } else {
                    a.counted_without_a4 += 1;
                }
                if meta.waste_entered {
                    a.counted_with_waste += 1;
                } else {
                    a.counted_without_waste += 1;
                    no_waste_rules.insert(meta.rule_category.clone());
                }
                if !meta.haul_entered {
                    no_haul_rules.insert(meta.rule_category.clone());
                }
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
    }

    a.by_material = totals.into_values().collect();
    if factors.is_some() && a.counted > 0 {
        a.embodied_kgco2e = Some(rows.iter().filter(|r| r.outcome == Outcome::Counted).map(|r| r.total_kg).sum());
        if a.counted_with_a4 > 0 {
            a.transport_kgco2e = Some(rows.iter().filter(|r| r.outcome == Outcome::Counted).map(|r| r.a4_kg).sum());
        }
    }
    a.rows = rows;
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
    if a.counted > 0 {
        a.notes.push(format!(
            "The embodied total is {EMBODIED_BASIS}: {} of {} counted line(s) include A4 and {} do not; {} include a waste allowance and {} do not.",
            a.counted_with_a4, a.counted, a.counted_without_a4, a.counted_with_waste, a.counted_without_waste
        ));
        if !no_haul_rules.is_empty() {
            a.notes.push(format!("Counted rules with no haul_km, so no transport (A4) is in their lines: {}.", join(&no_haul_rules)));
        }
        if !no_waste_rules.is_empty() {
            a.notes.push(format!("Counted rules with no waste_pct, so no waste allowance is in their lines: {}.", join(&no_waste_rules)));
        }
    }
    if a.lighting.lines_without_power > 0 {
        a.notes.push(format!(
            "{} luminaire line(s) have no usable power_w attribute: installed power and lighting energy are not calculated until every line has one.",
            a.lighting.lines_without_power
        ));
    }
    a
}

fn join(set: &BTreeSet<String>) -> String {
    set.iter().map(String::as_str).collect::<Vec<_>>().join(", ")
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
    let kw = (r.lines_without_power == 0).then_some(watts / 1000.0);
    r.installed_kw = kw;
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
    if let (Some(kw), Some(h), Some(d)) = (kw, hours, dimming) {
        let kwh = kw * h * d;
        r.kwh_per_year = Some(kwh);
        if let Some(g) = grid {
            let per_year = kwh * g;
            r.kgco2e_per_year = Some(per_year);
            r.life_kgco2e = life.map(|y| per_year * y);
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
        LineIn { id, element: None, mark: format!("M{id}"), category: category.into(), description: "test".into(), quantity, unit: unit.into(), attributes: BTreeMap::new() }
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
        assert_eq!(a.counted_with_a4, 0, "a haul with no transport factor is not an A4");
        assert_eq!(a.counted_without_a4, 1);
        assert!(a.notes.iter().any(|n| n.contains("transport_kgco2e_per_tkm")));
    }

    #[test]
    fn the_total_says_where_a4_and_waste_are_and_are_not_in_it() {
        let mut set = set_of(vec![factor("m", 100.0), factor("n", 10.0)]);
        set.transport_kgco2e_per_tkm = Some(0.1);
        let mut hauled = rule_for("hauled", "t", Use::Material { key: "m".into() });
        hauled.haul_km = Some(30.0);
        hauled.waste_pct = Some(10.0);
        let plain = rule_for("plain", "t", Use::Material { key: "n".into() });
        let a = assess(&[line(0, "hauled", 10.0, "t"), line(1, "plain", 20.0, "t"), line(2, "plain", 5.0, "t")], Some(&set), &settings_with(vec![hauled, plain]));
        assert_eq!((a.counted, a.counted_with_a4, a.counted_without_a4), (3, 1, 2));
        assert_eq!((a.counted_with_waste, a.counted_without_waste), (1, 2));
        // hauled: 11 t * 100 + 11 t * 30 km * 0.1 = 1100 + 33; plain: 25 t * 10 = 250
        assert_eq!(a.embodied_kgco2e, Some(1100.0 + 33.0 + 250.0));
        assert_eq!(a.transport_kgco2e, Some(33.0));
        let text = a.notes.join("\n");
        assert!(text.contains(EMBODIED_BASIS), "{text}");
        assert!(text.contains("no haul_km") && text.contains(": plain."), "{text}");
        assert!(text.contains("no waste_pct") && text.contains(": plain."), "{text}");
        assert!(!text.contains("hauled."), "a rule with a haul and a waste is not listed: {text}");
        // nothing counted: no such notes
        let a = assess(&[], Some(&set), &Settings::default());
        assert!(!a.notes.iter().any(|n| n.contains("haul_km")));
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
        assert_eq!(a.counted_with_a4, 1);
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
        assert_eq!(a.lighting.installed_kw, Some(1.0));
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
        // a line with no power: counted, no installed power (a partial sum would look whole), noted
        let bare = line(1, "lighting.luminaire", 2.0, "nr");
        s.lighting.dimming_factor = Some(1.0);
        let a = assess(&[l, bare], Some(&set), &s);
        assert_eq!(a.lighting.lines_without_power, 1);
        assert_eq!(a.lighting.installed_kw, None);
        assert_eq!(a.lighting.kwh_per_year, None);
        assert_eq!(a.lighting.luminaires, 12.0);
        assert!(a.notes.iter().any(|n| n.contains("power_w")));
        // no luminaires: no installed power either, not 0 kW
        let a = assess(&[], Some(&set), &s);
        assert_eq!(a.lighting.installed_kw, None);
        assert_eq!(a.energy_lines, 0);
    }

    #[test]
    fn lighting_needs_no_factor_file_for_installed_power() {
        let mut l = line(0, "lighting.luminaire", 4.0, "nr");
        l.attributes.insert("power_w".into(), "50".into());
        let a = assess(&[l], None, &Settings::default());
        assert_eq!(a.lighting.installed_kw, Some(0.2));
        assert_eq!(a.lighting.kwh_per_year, None);
        assert!(a.lighting.missing.iter().any(|m| m.starts_with("grid_kgco2e_per_kwh")));
    }

    /// Index of `post.length` among the default rules, which is also its line id in `mixed_lines`.
    fn post_index() -> usize {
        default_rules().iter().position(|r| r.category == "post.length").unwrap()
    }

    fn mixed_lines() -> Vec<LineIn> {
        let mut lines: Vec<LineIn> = default_rules().iter().enumerate().map(|(i, r)| line(i, &r.category, 1.0 + i as f64, &r.unit)).collect();
        let n = lines.len();
        lines.push(line(n, "something.unknown", 3.0, "m"));
        let mut asphalt = line(n + 1, "pavement.layer_volume", 5.0, "m3");
        asphalt.attributes.insert("material".into(), "Hot mix asphalt".into());
        lines.push(asphalt);
        lines.push(line(n + 2, "post.length", 5.0, "m3")); // wrong unit
        lines.push(line(n + 3, "post.length", f64::NAN, "m"));
        lines
    }

    #[test]
    fn partition_invariant() {
        let lines = mixed_lines();
        let mut set = set_of(vec![factor("steel post", 2.0), factor("pavement: hot mix asphalt", 3.0)]);
        set.factors[0].t_per_m = Some(0.5);
        set.factors[1].t_per_m3 = Some(2.0);
        let s = Settings { lighting: crate::LightingUse { hours_per_year: Some(1.0), ..Default::default() }, ..Settings::default() };
        for factors in [None, Some(&set)] {
            let a = assess(&lines, factors, &s);
            assert_eq!(a.counted + a.excluded + a.unmatched + a.energy_lines, lines.len());
            assert_eq!(a.rows.len(), lines.len());
            assert!(a.energy_lines >= 1 && a.excluded >= 1 && a.unmatched >= 1);
        }
        let a = assess(&lines, Some(&set), &s);
        assert_eq!(a.counted, 2, "post.length and the asphalt line: {:?}", a.rows.iter().filter(|r| r.outcome == Outcome::Counted).collect::<Vec<_>>());
        let post = a.rows.iter().find(|r| r.category == "post.length" && r.outcome == Outcome::Counted).unwrap();
        assert_eq!(post.tonnes, (1.0 + post_index() as f64) * 0.5);
        assert_eq!(a.by_material.len(), 2);
        // the post's own mass line is excluded with its reason, not counted twice
        let mass = a.rows.iter().find(|r| r.category == "post.mass").unwrap();
        assert!(matches!(&mass.outcome, Outcome::Excluded(w) if w.contains("built-in steel density")));
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
        let attr = rule_for("c", "m3", Use::MaterialFromAttribute { attribute: "material".into(), prefix: "p: ".into(), fallback: None });
        assert_eq!(reason(attr, line(0, "c", 1.0, "m3")), "the line has no attribute 'material'");
        assert_eq!(reason(rule_for("c", "m3", Use::Material { key: "m".into() }), line(0, "c", -2.0, "m3")), "negative quantity");
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
    fn a_second_rule_for_a_category_in_another_unit_is_reached() {
        let mut f = factor("m", 10.0);
        f.t_per_m2 = Some(0.5);
        let set = set_of(vec![f]);
        let rules = vec![rule_for("c", "m3", Use::Excluded { reason: "volume is not used".into() }), rule_for("c", "m2", Use::Material { key: "m".into() })];
        let a = assess(&[line(0, "c", 4.0, "m2"), line(1, "c", 4.0, "m3"), line(2, "c", 4.0, "kg")], Some(&set), &settings_with(rules));
        assert_eq!(a.rows[0].outcome, Outcome::Counted);
        assert_eq!(a.rows[0].total_kg, 4.0 * 0.5 * 10.0);
        assert_eq!(a.rows[1].outcome, Outcome::Excluded("volume is not used".into()));
        // a unit no rule has: the first rule of the category only gives the reason
        assert_eq!(a.rows[2].outcome, Outcome::Excluded("volume is not used".into()));
        // with two counting rules, the message names both units
        let rules = vec![rule_for("c", "m3", Use::Material { key: "m".into() }), rule_for("c", "m2", Use::Material { key: "m".into() })];
        let a = assess(&[line(0, "c", 1.0, "kg")], Some(&set), &settings_with(rules));
        assert_eq!(a.rows[0].outcome, Outcome::Unmatched("the rule for c expects m3 or m2, the line is in kg".into()));
    }

    #[test]
    fn the_other_spelling_of_number_is_read_as_nr() {
        let mut f = factor("m", 10.0);
        f.t_per_nr = Some(2.0);
        let set = set_of(vec![f]);
        let r = one(&set, rule_for("c", "nr", Use::Material { key: "m".into() }), line(0, "c", 3.0, " No "));
        assert_eq!(r.outcome, Outcome::Counted);
        assert_eq!(r.tonnes, 6.0);
    }

    #[test]
    fn zero_and_negative_quantities_are_not_counted() {
        let mut f = factor("m", 10.0);
        f.t_per_m3 = Some(2.0);
        let set = set_of(vec![f]);
        let rules = vec![rule_for("c", "m3", Use::Material { key: "m".into() }), rule_for("lighting.luminaire", "nr", Use::LightingEnergy)];
        let a = assess(&[line(0, "c", 0.0, "m3"), line(1, "lighting.luminaire", 0.0, "nr")], Some(&set), &settings_with(rules.clone()));
        assert_eq!(a.rows[0].outcome, Outcome::Excluded("quantity is 0: not modelled".into()));
        assert_eq!(a.rows[1].outcome, Outcome::Excluded("quantity is 0: not modelled".into()));
        assert_eq!((a.counted, a.excluded, a.energy_lines), (0, 2, 0));
        assert_eq!(a.embodied_kgco2e, None, "nothing non-zero was counted");
        // one real line beside a zero line: the total is the real one
        let a = assess(&[line(0, "c", 0.0, "m3"), line(1, "c", 3.0, "m3")], Some(&set), &settings_with(rules.clone()));
        assert_eq!((a.counted, a.excluded), (1, 1));
        assert_eq!(a.embodied_kgco2e, Some(60.0));
        // negative
        let a = assess(&[line(0, "c", -1.0, "m3"), line(1, "lighting.luminaire", -1.0, "nr")], Some(&set), &settings_with(rules));
        assert_eq!(a.rows[0].outcome, Outcome::Unmatched("negative quantity".into()));
        assert_eq!(a.rows[1].outcome, Outcome::Unmatched("negative quantity".into()));
        assert_eq!(a.embodied_kgco2e, None);
    }

    fn gantry_line(id: usize, element: u64, mark: &str, category: &str, unit: &str) -> LineIn {
        LineIn { element: Some(element), mark: mark.into(), ..line(id, category, 2.0, unit) }
    }

    #[test]
    fn a_gantry_without_steel_mass_says_so_instead_of_vanishing() {
        let mut f = factor("structural steel", 5.0);
        f.t_per_m = Some(0.1);
        let set = set_of(vec![f]);
        let lines = vec![
            // gantry G1 has steel mass: its count line stays excluded
            gantry_line(0, 1, "G1", "gantry.count", "nr"),
            gantry_line(1, 1, "G1", "gantry.leg_length", "m"),
            gantry_line(2, 1, "G1", "gantry.steel_mass", "kg"),
            // gantry G2 has none: a member has no mass per metre
            gantry_line(3, 1, "G2", "gantry.count", "nr"),
            gantry_line(4, 1, "G2", "gantry.leg_length", "m"),
            // the same mark on another element is another gantry
            gantry_line(5, 2, "G1", "gantry.count", "nr"),
        ];
        let a = assess(&lines, Some(&set), &Settings::default());
        assert!(matches!(&a.rows[0].outcome, Outcome::Excluded(w) if w.contains("steel mass carries")), "{:?}", a.rows[0]);
        assert_eq!(a.rows[2].outcome, Outcome::Counted);
        assert_eq!(a.rows[3].outcome, Outcome::Unmatched("no steel mass: a member has no mass per metre".into()));
        assert!(matches!(a.rows[4].outcome, Outcome::Excluded(_)));
        assert_eq!(a.rows[5].outcome, Outcome::Unmatched("no steel mass: a member has no mass per metre".into()));
        assert_eq!(a.counted + a.excluded + a.unmatched + a.energy_lines, lines.len());
        assert_eq!(a.unmatched, 2);
        assert!(a.notes.iter().any(|n| n.starts_with("2 line(s) are unmatched")));
        // the sign panel is not said to be in the steel mass
        let rules = default_rules();
        let Use::Excluded { reason } = &rules.iter().find(|r| r.category == "gantry.sign_area").unwrap().use_ else { panic!() };
        assert!(reason.contains("not in the structural steel mass"), "{reason}");
    }

    #[test]
    fn a_post_is_counted_from_its_length_by_section() {
        let mut steel = factor("steel post 60x3 CHS", 4.0);
        steel.t_per_m = Some(0.01);
        let mut plain = factor("steel post", 4.0);
        plain.t_per_m = Some(0.02);
        let set = set_of(vec![steel, plain]);
        let mut with_section = line(0, "post.length", 3.0, "m");
        with_section.attributes.insert("section".into(), "60x3 CHS".into());
        let no_section = line(1, "post.length", 3.0, "m");
        let mass = line(2, "post.mass", 24.0, "kg");
        let a = assess(&[with_section, no_section, mass], Some(&set), &Settings::default());
        assert_eq!(a.rows[0].material_key, "steel post 60x3 CHS");
        assert_eq!(a.rows[0].tonnes, 0.03);
        assert_eq!(a.rows[1].material_key, "steel post");
        assert_eq!(a.rows[1].tonnes, 0.06);
        assert_eq!(a.rows[2].outcome, Outcome::Excluded("derived from a built-in steel density; posts are counted by length".into()));
        assert_eq!(a.counted, 2);
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
