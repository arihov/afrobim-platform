//! Rules: which quantity lines carry carbon, and how. The default rules say only which category
//! is used and which are left out to avoid counting the same thing twice. They hold no numbers.

use serde::{Deserialize, Serialize};

/// What a line is used for.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum Use {
    /// Material carbon from the factor with this key.
    Material { key: String },
    /// Material carbon from the factor keyed `prefix + the line's attribute value`, e.g. prefix
    /// `pavement: ` and material `Hot mix asphalt` look up `pavement: Hot mix asphalt`.
    MaterialFromAttribute { attribute: String, prefix: String },
    /// Moving material only: no material carbon, transport (A4) from the density of this factor,
    /// the rule's `haul_km` and the factor file's transport factor.
    HaulOnly { key: String },
    /// A luminaire line: counted in the lighting energy block, never in embodied carbon.
    LightingEnergy,
    /// Left out, with the reason shown in the report.
    Excluded { reason: String },
}

impl Default for Use {
    fn default() -> Self {
        Use::Excluded { reason: String::new() }
    }
}

impl Use {
    /// True when the rule can put carbon or transport on a line.
    pub fn is_counting(&self) -> bool {
        matches!(self, Use::Material { .. } | Use::MaterialFromAttribute { .. } | Use::HaulOnly { .. })
    }
}

/// One rule, matched by exact category; the first matching rule wins.
#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct Rule {
    pub category: String,
    /// The unit this rule expects (`m`, `m2`, `m3`, `kg`, `t`, `nr`). A line in another unit is
    /// reported unmatched, never converted by guesswork.
    pub unit: String,
    pub use_: Use,
    /// Waste as a percentage of the tonnes, entered by the user. None means no waste allowance.
    pub waste_pct: Option<f64>,
    /// Haul distance in kilometres, entered by the user. None means no transport.
    pub haul_km: Option<f64>,
}

/// Lighting energy inputs, all the user's.
#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct LightingUse {
    pub hours_per_year: Option<f64>,
    /// Energy fraction of full power over the burning hours, 0 to 1 inclusive.
    pub dimming_factor: Option<f64>,
    pub design_life_years: Option<f64>,
}

/// Everything the user sets besides the factor file.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Settings {
    pub rules: Vec<Rule>,
    pub lighting: LightingUse,
}

impl Default for Settings {
    fn default() -> Self {
        Settings { rules: default_rules(), lighting: LightingUse::default() }
    }
}

fn rule(category: &str, unit: &str, use_: Use) -> Rule {
    Rule { category: category.to_string(), unit: unit.to_string(), use_, waste_pct: None, haul_km: None }
}

fn material(key: &str) -> Use {
    Use::Material { key: key.to_string() }
}

fn out(reason: &str) -> Use {
    Use::Excluded { reason: reason.to_string() }
}

/// The category-to-use table, with no numbers (`waste_pct` and `haul_km` are None). Within each
/// physical quantity exactly one line carries the material and the overlapping ones are excluded
/// with a reason. The keys named here (`concrete`, `structural steel`, ...) are looked up in the
/// user's factor file; none of them has a figure behind it in this crate.
pub fn default_rules() -> Vec<Rule> {
    let pave = "same layer as its volume";
    let mass = "mass carries the post";
    let barrier = "in the barrier's mass per metre";
    let steel = "structural steel mass carries the gantry";
    vec![
        rule("pavement.layer_volume", "m3", Use::MaterialFromAttribute { attribute: "material".into(), prefix: "pavement: ".into() }),
        rule("pavement.layer_area", "m2", out(pave)),
        rule("earthworks.fill", "m3", Use::HaulOnly { key: "earth fill".into() }),
        rule("earthworks.cut", "m3", out("counted once, through fill haul")),
        rule("drain.excavation", "m3", out("excavation: haul only if entered on fill")),
        rule("foundation.excavation", "m3", out("excavation: haul only if entered on fill")),
        rule("foundation.concrete", "m3", Use::MaterialFromAttribute { attribute: "concrete".into(), prefix: "concrete ".into() }),
        rule("foundation.blinding", "m3", material("concrete blinding")),
        rule("foundation.count", "nr", out("a count, not a material")),
        rule("foundation.formwork", "m2", out("formwork is not counted unless a rule is added for it")),
        rule("gantry.steel_mass", "kg", material("structural steel")),
        rule("gantry.foundation_concrete", "m3", material("concrete")),
        rule("gantry.count", "nr", out(steel)),
        rule("gantry.leg_length", "m", out(steel)),
        rule("gantry.boom_length", "m", out(steel)),
        rule("gantry.sign_area", "m2", out(steel)),
        rule("post.mass", "kg", material("steel post")),
        rule("post.length", "m", out(mass)),
        rule("post.count", "nr", out(mass)),
        rule("post.coating_area", "m2", out(mass)),
        rule("barrier.length", "m", material("safety barrier")),
        rule("barrier.posts", "nr", out(barrier)),
        rule("barrier.terminal", "nr", out(barrier)),
        rule("barrier.transition", "nr", out(barrier)),
        rule("drain.lining_volume", "m3", material("concrete")),
        rule("drain.length", "m", out("the lining volume carries the drain")),
        rule("drain.lining_area", "m2", out("the lining volume carries the drain")),
        rule("culvert.length", "m", material("culvert")),
        rule("culvert.barrels", "nr", out("the culvert length per metre carries the barrels")),
        rule("marking.painted_area", "m2", material("road marking")),
        rule("marking.length", "m", out("the painted area carries the marking")),
        rule("marking.painted_length", "m", out("the painted area carries the marking")),
        rule("marking.marks", "nr", out("the painted area carries the marking")),
        rule("sign.face_area", "m2", material("sign face")),
        rule("sign.face", "nr", out("the face area carries the sign")),
        rule("sign.legend", "nr", out("the face area carries the sign")),
        rule("lighting.luminaire", "nr", Use::LightingEnergy),
        rule("lighting.column", "nr", material("lighting column")),
    ]
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::{BTreeMap, BTreeSet};

    #[test]
    fn default_rules_carry_no_numbers() {
        let rules = default_rules();
        assert!(rules.iter().all(|r| r.waste_pct.is_none() && r.haul_km.is_none()));
        assert_eq!(Settings::default().lighting, LightingUse::default());
    }

    #[test]
    fn each_category_has_one_rule_and_each_exclusion_a_reason() {
        let rules = default_rules();
        let cats: BTreeSet<&str> = rules.iter().map(|r| r.category.as_str()).collect();
        assert_eq!(cats.len(), rules.len(), "a category has two rules");
        for r in &rules {
            if let Use::Excluded { reason } = &r.use_ {
                assert!(!reason.trim().is_empty(), "{} has no reason", r.category);
            }
        }
    }

    /// The families of the task statement hold some distinct materials (a foundation has concrete
    /// and blinding; a gantry has steel and a concrete footing). What must never happen is two
    /// rules counting the same physical quantity, so each group of overlapping categories has
    /// exactly one counting rule.
    #[test]
    fn one_counting_rule_per_physical_quantity() {
        let groups: [&[&str]; 14] = [
            &["pavement.layer_volume", "pavement.layer_area"],
            &["earthworks.fill", "earthworks.cut", "drain.excavation", "foundation.excavation"],
            &["foundation.concrete", "foundation.count", "foundation.formwork"],
            &["foundation.blinding"],
            &["gantry.steel_mass", "gantry.count", "gantry.leg_length", "gantry.boom_length", "gantry.sign_area"],
            &["gantry.foundation_concrete"],
            &["post.mass", "post.length", "post.count", "post.coating_area"],
            &["barrier.length", "barrier.posts", "barrier.terminal", "barrier.transition"],
            &["drain.lining_volume", "drain.length", "drain.lining_area"],
            &["culvert.length", "culvert.barrels"],
            &["marking.painted_area", "marking.length", "marking.painted_length", "marking.marks"],
            &["sign.face_area", "sign.face", "sign.legend"],
            &["lighting.column"],
            &["lighting.luminaire"],
        ];
        let rules = default_rules();
        let by_cat: BTreeMap<&str, &Rule> = rules.iter().map(|r| (r.category.as_str(), r)).collect();
        let mut covered = BTreeSet::new();
        for g in groups {
            let counting = g.iter().filter(|c| by_cat[**c].use_.is_counting()).count();
            let energy = g.iter().filter(|c| by_cat[**c].use_ == Use::LightingEnergy).count();
            assert_eq!(counting + energy, 1, "group {g:?}");
            covered.extend(g.iter().copied());
        }
        assert_eq!(covered.len(), rules.len(), "a default rule belongs to no group");
        // the specified keys
        assert_eq!(by_cat["earthworks.fill"].use_, Use::HaulOnly { key: "earth fill".into() });
        assert_eq!(by_cat["lighting.column"].use_, material("lighting column"));
    }

    #[test]
    fn settings_deserialise_with_missing_fields() {
        let s: Settings = serde_json::from_str("{}").unwrap();
        assert_eq!(s, Settings::default());
        let r: Rule = serde_json::from_str("{\"category\":\"x\"}").unwrap();
        assert_eq!(r.waste_pct, None);
        let s2: Settings = serde_json::from_str(&serde_json::to_string(&s).unwrap()).unwrap();
        assert_eq!(s2, s);
    }
}
