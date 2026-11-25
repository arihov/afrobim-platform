// SPDX-License-Identifier: LicenseRef-AfroBIM-Proprietary
// Copyright (c) 2026 AfroBIM. All rights reserved.
use crate::assess::{Assessment, Outcome, EMBODIED_BASIS};
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
    o.push_str(&row(&[format!("embodied ({EMBODIED_BASIS})"), String::new(), String::new(), String::new(), opt(a.embodied_kgco2e)]));
    o.push_str(&row(&["of which transport (A4)", "", "", "", &opt(a.transport_kgco2e)]));
    o.push('\n');

    o.push_str(&row(&["Coverage"]));
    o.push_str(&row(&["counted", &a.counted.to_string()]));
    o.push_str(&row(&["counted with A4", &a.counted_with_a4.to_string()]));
    o.push_str(&row(&["counted without A4", &a.counted_without_a4.to_string()]));
    o.push_str(&row(&["counted with waste allowance", &a.counted_with_waste.to_string()]));
    o.push_str(&row(&["counted without waste allowance", &a.counted_without_waste.to_string()]));
    o.push_str(&row(&["excluded", &a.excluded.to_string()]));
    o.push_str(&row(&["unmatched", &a.unmatched.to_string()]));
    o.push_str(&row(&["energy lines", &a.energy_lines.to_string()]));
    o.push_str(&row(&["lines", &a.rows.len().to_string()]));
    o.push('\n');

    let l = &a.lighting;
    o.push_str(&row(&["Street lighting energy (not part of embodied carbon)"]));
    o.push_str(&row(&["luminaires", &if a.energy_lines > 0 { fig(l.luminaires) } else { String::new() }]));
    o.push_str(&row(&["installed_kw", &opt(l.installed_kw)]));
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
        assert!(csv.contains("\"embodied (A1-A3, plus A4 where a haul was entered)\",,,,6.000"));
        assert!(csv.contains("counted with A4,0\n") && csv.contains("counted without A4,1\n"));
        assert!(csv.contains("counted without waste allowance,1\n"));
        assert!(csv.contains("luminaires,\n") && csv.contains("installed_kw,\n"));
        assert!(csv.contains("Counted rules with no haul_km"), "the notes are exported");
        assert!(csv.contains("of which transport (A4),,,,\n"));
        assert!(csv.contains("kwh_per_year,\n"));
        assert!(csv.contains("unmatched,1\n"));
    }
}
