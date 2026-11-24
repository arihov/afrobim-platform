use crate::assess::{Assessment, Outcome};
use crate::csv::row;

fn fig(v: f64) -> String {
    format!("{v:.3}")
}

fn opt(v: Option<f64>) -> String {
    v.map_or_else(String::new, fig)
}

/// The assessment as CSV in blocks separated by a blank line, each block opening with a title
/// row: the factor set, every line with its outcome and figures, totals by material, coverage,
/// the lighting block and the notes. An empty cell means "not calculated", never zero.
pub fn assessment_csv(a: &Assessment) -> String {
    let mut o = String::new();
    o.push_str(&row(&["Factor set"]));
    o.push_str(&row(&["name and edition", &a.factor_set]));
    o.push('\n');

    o.push_str(&row(&["Lines"]));
    o.push_str(&row(&["line", "mark", "category", "description", "outcome", "reason", "material_key", "tonnes", "waste_t", "a1a3_kgco2e", "a4_kgco2e", "total_kgco2e"]));
    for r in &a.rows {
        let (outcome, reason) = match &r.outcome {
            Outcome::Counted => ("counted", String::new()),
            Outcome::Excluded(w) => ("excluded", w.clone()),
            Outcome::Unmatched(w) => ("unmatched", w.clone()),
            Outcome::Energy => ("energy", String::new()),
        };
        let counted = r.outcome == Outcome::Counted;
        let f = |v: f64| if counted { fig(v) } else { String::new() };
        o.push_str(&row(&[
            r.line.to_string(),
            r.mark.clone(),
            r.category.clone(),
            r.description.clone(),
            outcome.to_string(),
            reason,
            r.material_key.clone(),
            f(r.tonnes),
            f(r.waste_t),
            f(r.a1a3_kg),
            f(r.a4_kg),
            f(r.total_kg),
        ]));
    }
    o.push('\n');

    o.push_str(&row(&["Totals by material"]));
    o.push_str(&row(&["key", "material", "tonnes", "waste_t", "kgco2e"]));
    for t in &a.by_material {
        o.push_str(&row(&[t.key.clone(), t.material.clone(), fig(t.tonnes), fig(t.waste_t), fig(t.kgco2e)]));
    }
    o.push_str(&row(&["embodied (A1-A3 plus A4)", "", "", "", &opt(a.embodied_kgco2e)]));
    o.push_str(&row(&["of which transport (A4)", "", "", "", &opt(a.transport_kgco2e)]));
    o.push('\n');

    o.push_str(&row(&["Coverage"]));
    o.push_str(&row(&["counted", &a.counted.to_string()]));
    o.push_str(&row(&["excluded", &a.excluded.to_string()]));
    o.push_str(&row(&["unmatched", &a.unmatched.to_string()]));
    o.push_str(&row(&["energy lines", &a.energy_lines.to_string()]));
    o.push_str(&row(&["lines", &a.rows.len().to_string()]));
    o.push('\n');

    let l = &a.lighting;
    o.push_str(&row(&["Street lighting energy (not part of embodied carbon)"]));
    o.push_str(&row(&["luminaires", &fig(l.luminaires)]));
    o.push_str(&row(&["installed_kw", &fig(l.installed_kw)]));
    o.push_str(&row(&["kwh_per_year", &opt(l.kwh_per_year)]));
    o.push_str(&row(&["kgco2e_per_year", &opt(l.kgco2e_per_year)]));
    o.push_str(&row(&["life_kgco2e", &opt(l.life_kgco2e)]));
    o.push_str(&row(&["lines_without_power", &l.lines_without_power.to_string()]));
    o.push_str(&row(&["missing inputs", &l.missing.join("; ")]));

    if !a.notes.is_empty() {
        o.push('\n');
        o.push_str(&row(&["Notes"]));
        for n in &a.notes {
            o.push_str(&row(&[n.as_str()]));
        }
    }
    o
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{assess, Factor, FactorSet, LineIn, Settings};

    #[test]
    fn csv_has_every_row_and_marks_uncalculated_cells_empty() {
        let set = FactorSet { name: "Test, file".into(), edition: "T1".into(), boundary: "A1-A3".into(), ..FactorSet::default() };
        let mut set = set;
        set.factors.push(Factor { key: "m".into(), material: "M".into(), kgco2e_per_t: 2.0, ..Factor::default() });
        let mut s = Settings::default();
        s.rules[0].category = "c".into();
        s.rules[0].unit = "t".into();
        s.rules[0].use_ = crate::Use::Material { key: "m".into() };
        let line = |id, cat: &str| LineIn { id, category: cat.into(), quantity: 3.0, unit: "t".into(), ..LineIn::default() };
        let a = assess(&[line(0, "c"), line(1, "nothing")], Some(&set), &s);
        let csv = assessment_csv(&a);
        assert!(csv.contains("\"Test, file (T1)\""));
        assert!(csv.contains("0,,c,,counted,,m,3.000,0.000,6.000,0.000,6.000"));
        assert!(csv.contains("1,,nothing,,unmatched,no rule for nothing,,,,,,"));
        assert!(csv.contains("embodied (A1-A3 plus A4),,,,6.000"));
        assert!(csv.contains("of which transport (A4),,,,\n"));
        assert!(csv.contains("kwh_per_year,\n"));
        assert!(csv.contains("unmatched,1\n"));
    }
}
