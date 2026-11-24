//! Rules: which quantity lines carry carbon, and how. The default rules say only which category
//! is used and which are left out to avoid counting the same thing twice. They hold no numbers.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

/// What a line is used for.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum Use {
    /// Material carbon from the factor with this key.
    Material { key: String },
    /// Material carbon from the factor keyed `prefix + the line's attribute value`, e.g. prefix
    /// `pavement: ` and material `Hot mix asphalt` look up `pavement: Hot mix asphalt`.
    /// `attribute` may name several attributes separated by commas; their values are joined by
    /// a space and every one must be present. When an attribute is missing or blank the line
    /// uses the factor `fallback` if there is one, else it is unmatched.
    MaterialFromAttribute {
        attribute: String,
        prefix: String,
        #[serde(default)]
        fallback: Option<String>,
    },
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

    /// The factor key a counting rule asks for on a line with these attributes, or the reason
    /// there is none.
    pub fn key_for(&self, attributes: &BTreeMap<String, String>) -> Result<String, String> {
        match self {
            Use::Material { key } | Use::HaulOnly { key } => Ok(key.clone()),
            Use::MaterialFromAttribute { attribute, prefix, fallback } => {
                let names: Vec<&str> = attribute.split(',').map(str::trim).filter(|n| !n.is_empty()).collect();
                let value = |n: &str| attributes.get(n).map(|v| v.trim()).filter(|v| !v.is_empty());
                let values: Vec<&str> = names.iter().filter_map(|n| value(n)).collect();
                if !names.is_empty() && values.len() == names.len() {
                    Ok(format!("{prefix}{}", values.join(" ")))
                } else if let Some(f) = fallback {
                    Ok(f.clone())
                } else {
                    let missing = names.iter().find(|n| value(n).is_none()).copied().unwrap_or(attribute.as_str());
                    Err(format!("the line has no attribute '{missing}'"))
                }
            }
            _ => Err("this rule does not count material".to_string()),
        }
    }
}

/// One rule, matched by exact category and unit; the first matching rule wins.
#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct Rule {
    pub category: String,
    /// The unit this rule expects (`m`, `m2`, `m3`, `kg`, `t`, `nr`). A rule is chosen by category
    /// and unit together, so one category can have a rule per unit. A line in a unit that no rule
    /// of its category has is reported unmatched, never converted by guesswork.
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

impl Settings {
    /// Inputs that cannot be used, by name, so a window can say so rather than let a line fail
    /// quietly: a negative or non-finite waste or haul, or a lighting figure out of range.
    pub fn problems(&self) -> Vec<String> {
        let mut out = Vec::new();
        let bad = |v: Option<f64>| v.is_some_and(|x| !x.is_finite() || x < 0.0);
        for r in &self.rules {
            if bad(r.waste_pct) {
                out.push(format!("waste_pct of {} ({}) is not a non-negative number: lines using it are unmatched", r.category, r.unit));
            }
            if bad(r.haul_km) {
                out.push(format!("haul_km of {} ({}) is not a non-negative number: lines using it are unmatched", r.category, r.unit));
            }
        }
        let l = &self.lighting;
        if bad(l.hours_per_year) {
            out.push("hours_per_year is not a non-negative number: it is ignored".to_string());
        }
        if l.dimming_factor.is_some_and(|x| !x.is_finite() || !(0.0..=1.0).contains(&x)) {
            out.push("dimming_factor is not between 0 and 1: it is ignored".to_string());
        }
        if bad(l.design_life_years) {
            out.push("design_life_years is not a non-negative number: it is ignored".to_string());
        }
        out
    }
}

/// What the user changed on one default rule. A project stores only these, so a default rule
/// added in a later version reaches old projects.
#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct RuleOverride {
    pub category: String,
    pub unit: String,
    pub waste_pct: Option<f64>,
    pub haul_km: Option<f64>,
    /// Set only when the user changed how the category is used.
    pub use_: Option<Use>,
}

/// Everything the user changed from the defaults: per-rule overrides, rules the defaults do not
/// have, and the lighting inputs.
#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct Overrides {
    pub rules: Vec<RuleOverride>,
    pub extra_rules: Vec<Rule>,
    pub lighting: LightingUse,
}

fn same_unit(a: &str, b: &str) -> bool {
    a.trim().eq_ignore_ascii_case(b.trim())
}

impl Overrides {
    /// The settings in use: the current default rules with the overrides laid over them, then
    /// the extra rules.
    pub fn apply(&self) -> Settings {
        let mut rules = default_rules();
        for r in &mut rules {
            if let Some(o) = self.rules.iter().find(|o| o.category == r.category && same_unit(&o.unit, &r.unit)) {
                if o.waste_pct.is_some() {
                    r.waste_pct = o.waste_pct;
                }
                if o.haul_km.is_some() {
                    r.haul_km = o.haul_km;
                }
                if let Some(u) = &o.use_ {
                    r.use_ = u.clone();
                }
            }
        }
        rules.extend(self.extra_rules.iter().cloned());
        Settings { rules, lighting: self.lighting.clone() }
    }

    /// The overrides that turn the default rules into these settings: only the fields that
    /// differ from the defaults.
    pub fn of(settings: &Settings) -> Overrides {
        let defaults = default_rules();
        let mut o = Overrides { lighting: settings.lighting.clone(), ..Overrides::default() };
        for r in &settings.rules {
            match defaults.iter().find(|d| d.category == r.category && same_unit(&d.unit, &r.unit)) {
                None => o.extra_rules.push(r.clone()),
                Some(d) => {
                    let ov = RuleOverride {
                        category: r.category.clone(),
                        unit: d.unit.clone(),
                        waste_pct: r.waste_pct.filter(|_| r.waste_pct != d.waste_pct),
                        haul_km: r.haul_km.filter(|_| r.haul_km != d.haul_km),
                        use_: (r.use_ != d.use_).then(|| r.use_.clone()),
                    };
                    if ov.waste_pct.is_some() || ov.haul_km.is_some() || ov.use_.is_some() {
                        o.rules.push(ov);
                    }
                }
            }
        }
        o
    }

    /// Migrates a full rule list stored by the first version of the module. That version's
    /// window could change only `waste_pct` and `haul_km`, so only those are kept: its stored
    /// `use_` values are the old defaults (for instance `post.mass` counted as material) and must
    /// not freeze over the current ones. What was entered on `post.mass` moves to `post.length`,
    /// which now carries the post, unless `post.length` has its own entry.
    pub fn from_legacy(rules: &[Rule], lighting: LightingUse) -> Overrides {
        let defaults = default_rules();
        let mut o = Overrides { lighting, ..Overrides::default() };
        fn put(o: &mut Overrides, category: &str, unit: &str, waste: Option<f64>, haul: Option<f64>) {
            if waste.is_none() && haul.is_none() {
                return;
            }
            match o.rules.iter_mut().find(|x| x.category == category && same_unit(&x.unit, unit)) {
                Some(x) => {
                    x.waste_pct = x.waste_pct.or(waste);
                    x.haul_km = x.haul_km.or(haul);
                }
                None => o.rules.push(RuleOverride { category: category.to_string(), unit: unit.to_string(), waste_pct: waste, haul_km: haul, use_: None }),
            }
        }
        // Entries on post.length itself win over those moved from post.mass, so they go first.
        for r in rules.iter().filter(|r| r.category != "post.mass") {
            match defaults.iter().find(|d| d.category == r.category && same_unit(&d.unit, &r.unit)) {
                Some(d) => put(&mut o, &d.category, &d.unit, r.waste_pct, r.haul_km),
                None => o.extra_rules.push(r.clone()),
            }
        }
        for r in rules.iter().filter(|r| r.category == "post.mass") {
            put(&mut o, "post.length", "m", r.waste_pct, r.haul_km);
        }
        o
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

fn from_attr(attribute: &str, prefix: &str, fallback: Option<&str>) -> Use {
    Use::MaterialFromAttribute { attribute: attribute.into(), prefix: prefix.into(), fallback: fallback.map(str::to_string) }
}

/// The category-to-use table, with no numbers (`waste_pct` and `haul_km` are None). Within each
/// physical quantity exactly one line carries the material and the overlapping ones are excluded
/// with a reason. The keys named here (`concrete`, `structural steel`, ...) are looked up in the
/// user's factor file; none of them has a figure behind it in this crate.
///
/// The civil bill lines added by the app (`Civil earthworks`, `Civil materials`, `Pipe networks`,
/// `Grading`) are excluded: they do not say whether a volume is cut, fill or net.
pub fn default_rules() -> Vec<Rule> {
    let pave = "same layer as its volume";
    let length = "the length carries the post";
    let barrier = "in the barrier's mass per metre";
    let steel = "structural steel mass carries the gantry";
    let mark = "the painted area carries the marking";
    let mark_ext = "a length is not counted as material; paint is counted from the painted area where the row has one (not for studs or placeholder blocks)";
    let civil = "civil bill lines do not say cut, fill or net; counting them could count the same earth twice";
    vec![
        rule("pavement.layer_volume", "m3", from_attr("material", "pavement: ", None)),
        rule("pavement.layer_area", "m2", out(pave)),
        rule("earthworks.fill", "m3", Use::HaulOnly { key: "earth fill".into() }),
        rule("earthworks.cut", "m3", out("counted once, through fill haul")),
        rule("drain.excavation", "m3", out("excavation: haul only if entered on fill")),
        rule("foundation.excavation", "m3", out("excavation: haul only if entered on fill")),
        rule("foundation.concrete", "m3", from_attr("concrete", "concrete ", None)),
        rule("foundation.blinding", "m3", material("concrete blinding")),
        rule("foundation.count", "nr", out("a count, not a material")),
        rule("foundation.formwork", "m2", out("formwork is not counted unless a rule is added for it")),
        rule("gantry.steel_mass", "kg", material("structural steel")),
        rule("gantry.foundation_concrete", "m3", material("concrete")),
        rule("gantry.count", "nr", out(steel)),
        rule("gantry.leg_length", "m", out(steel)),
        rule("gantry.boom_length", "m", out(steel)),
        rule("gantry.sign_area", "m2", out("a sign panel is not in the structural steel mass, and no panel material is counted from the gantry")),
        rule("post.length", "m", from_attr("section", "steel post ", Some("steel post"))),
        rule("post.mass", "kg", out("derived from a built-in steel density; posts are counted by length")),
        rule("post.count", "nr", out(length)),
        rule("post.coating_area", "m2", out(length)),
        rule("barrier.length", "m", from_attr("system", "barrier: ", Some("safety barrier"))),
        rule("barrier.posts", "nr", out(barrier)),
        rule("barrier.terminal", "nr", out(barrier)),
        rule("barrier.transition", "nr", out(barrier)),
        rule("drain.lining_volume", "m3", from_attr("lining", "drain lining: ", Some("concrete"))),
        rule("drain.length", "m", out("the lining volume carries the drain")),
        rule("drain.lining_area", "m2", out("the lining volume carries the drain")),
        rule("culvert.length", "m", from_attr("material,internal_diameter_mm", "culvert: ", Some("culvert"))),
        rule("culvert.barrels", "nr", out("the culvert length per metre carries the barrels")),
        rule("marking.painted_area", "m2", material("road marking")),
        rule("marking.length", "m", out(mark)),
        rule("marking.painted_length", "m", out(mark)),
        rule("marking.marks", "nr", out(mark)),
        rule("marking_ext.painted_area", "m2", material("road marking")),
        rule("marking_ext.length", "m", out(mark_ext)),
        rule("marking_ext.painted_length", "m", out(mark_ext)),
        rule("marking_ext.number", "nr", out("a count of marks, not a material; paint is counted from the painted area where the row has one")),
        rule("marking_ext.block_area", "m2", out("placeholder block, not paint")),
        rule("sign.face_area", "m2", material("sign face")),
        rule("sign.face", "nr", out("the face area carries the sign")),
        rule("sign.legend", "nr", out("the face area carries the sign")),
        rule("lighting.luminaire", "nr", Use::LightingEnergy),
        rule("lighting.column", "nr", material("lighting column")),
        rule("Civil earthworks", "m3", out(civil)),
        rule("Civil materials", "m3", out(civil)),
        rule("Pipe networks", "m", out(civil)),
        rule("Pipe networks", "nr", out(civil)),
        rule("Pipe networks", "m3", out(civil)),
        rule("Grading", "m3", out(civil)),
    ]
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::BTreeSet;

    #[test]
    fn default_rules_carry_no_numbers() {
        let rules = default_rules();
        assert!(rules.iter().all(|r| r.waste_pct.is_none() && r.haul_km.is_none()));
        assert_eq!(Settings::default().lighting, LightingUse::default());
    }

    #[test]
    fn each_category_and_unit_has_one_rule_and_each_exclusion_a_reason() {
        let rules = default_rules();
        let keys: BTreeSet<(&str, &str)> = rules.iter().map(|r| (r.category.as_str(), r.unit.as_str())).collect();
        assert_eq!(keys.len(), rules.len(), "a category and unit has two rules");
        for r in &rules {
            if let Use::Excluded { reason } = &r.use_ {
                assert!(!reason.trim().is_empty(), "{} has no reason", r.category);
            }
        }
    }

    /// The families of the task statement hold some distinct materials (a foundation has concrete
    /// and blinding; a gantry has steel and a concrete footing). What must never happen is two
    /// rules counting the same physical quantity, so each group of overlapping categories has at
    /// most one counting rule, and the number each is expected to have is stated.
    #[test]
    fn one_counting_rule_per_physical_quantity() {
        let groups: [(&[&str], usize); 18] = [
            (&["pavement.layer_volume", "pavement.layer_area"], 1),
            (&["earthworks.fill", "earthworks.cut", "drain.excavation", "foundation.excavation"], 1),
            (&["foundation.concrete", "foundation.count", "foundation.formwork"], 1),
            (&["foundation.blinding"], 1),
            (&["gantry.steel_mass", "gantry.count", "gantry.leg_length", "gantry.boom_length", "gantry.sign_area"], 1),
            (&["gantry.foundation_concrete"], 1),
            (&["post.mass", "post.length", "post.count", "post.coating_area"], 1),
            (&["barrier.length", "barrier.posts", "barrier.terminal", "barrier.transition"], 1),
            (&["drain.lining_volume", "drain.length", "drain.lining_area"], 1),
            (&["culvert.length", "culvert.barrels"], 1),
            (&["marking.painted_area", "marking.length", "marking.painted_length", "marking.marks"], 1),
            (&["marking_ext.painted_area", "marking_ext.length", "marking_ext.painted_length", "marking_ext.number"], 1),
            (&["marking_ext.block_area"], 0),
            (&["sign.face_area", "sign.face", "sign.legend"], 1),
            (&["lighting.column"], 1),
            (&["lighting.luminaire"], 1),
            (&["Civil earthworks", "Civil materials", "Grading"], 0),
            (&["Pipe networks"], 0),
        ];
        let rules = default_rules();
        let mut covered = 0;
        for (g, expected) in groups {
            let in_group: Vec<&Rule> = rules.iter().filter(|r| g.contains(&r.category.as_str())).collect();
            let counting = in_group.iter().filter(|r| r.use_.is_counting() || r.use_ == Use::LightingEnergy).count();
            assert_eq!(counting, expected, "group {g:?}");
            covered += in_group.len();
        }
        assert_eq!(covered, rules.len(), "a default rule belongs to no group");
        let by_cat = |c: &str| rules.iter().find(|r| r.category == c).unwrap().use_.clone();
        assert_eq!(by_cat("earthworks.fill"), Use::HaulOnly { key: "earth fill".into() });
        assert_eq!(by_cat("lighting.column"), material("lighting column"));
        assert_eq!(by_cat("marking_ext.painted_area"), material("road marking"));
    }

    #[test]
    fn posts_are_counted_by_length_not_by_the_built_in_mass() {
        let rules = default_rules();
        let get = |c: &str| rules.iter().find(|r| r.category == c).unwrap();
        assert_eq!(get("post.length").use_, from_attr("section", "steel post ", Some("steel post")));
        assert_eq!(get("post.length").unit, "m");
        assert_eq!(get("post.mass").use_, out("derived from a built-in steel density; posts are counted by length"));
        // the key follows the section, and falls back to the plain key when there is none
        let mut attrs = BTreeMap::new();
        assert_eq!(get("post.length").use_.key_for(&attrs).unwrap(), "steel post");
        attrs.insert("section".to_string(), " 60x3 CHS ".to_string());
        assert_eq!(get("post.length").use_.key_for(&attrs).unwrap(), "steel post 60x3 CHS");
        attrs.insert("section".to_string(), "  ".to_string());
        assert_eq!(get("post.length").use_.key_for(&attrs).unwrap(), "steel post");
    }

    #[test]
    fn several_attributes_make_one_key_and_a_missing_one_names_itself() {
        let u = from_attr("material,internal_diameter_mm", "culvert: ", None);
        let mut attrs = BTreeMap::new();
        attrs.insert("material".to_string(), "HDPE".to_string());
        assert_eq!(u.key_for(&attrs).unwrap_err(), "the line has no attribute 'internal_diameter_mm'");
        attrs.insert("internal_diameter_mm".to_string(), "600".to_string());
        assert_eq!(u.key_for(&attrs).unwrap(), "culvert: HDPE 600");
    }

    #[test]
    fn settings_deserialise_with_missing_fields() {
        let s: Settings = serde_json::from_str("{}").unwrap();
        assert_eq!(s, Settings::default());
        let r: Rule = serde_json::from_str("{\"category\":\"x\"}").unwrap();
        assert_eq!(r.waste_pct, None);
        let s2: Settings = serde_json::from_str(&serde_json::to_string(&s).unwrap()).unwrap();
        assert_eq!(s2, s);
        // the first version stored the attribute use without a fallback
        let u: Use = serde_json::from_str("{\"MaterialFromAttribute\":{\"attribute\":\"a\",\"prefix\":\"p \"}}").unwrap();
        assert_eq!(u, from_attr("a", "p ", None));
    }

    #[test]
    fn only_the_user_changes_are_kept_and_they_merge_over_the_defaults() {
        let mut s = Settings::default();
        assert_eq!(Overrides::of(&s), Overrides::default());
        let i = s.rules.iter().position(|r| r.category == "pavement.layer_volume").unwrap();
        s.rules[i].waste_pct = Some(4.0);
        s.rules[i].haul_km = Some(25.0);
        s.lighting.hours_per_year = Some(4000.0);
        let o = Overrides::of(&s);
        assert_eq!(o.rules, vec![RuleOverride { category: "pavement.layer_volume".into(), unit: "m3".into(), waste_pct: Some(4.0), haul_km: Some(25.0), use_: None }]);
        assert!(o.extra_rules.is_empty());
        assert_eq!(o.apply(), s);
        // stored form is small and survives JSON
        let json = serde_json::to_string(&o).unwrap();
        assert!(json.len() < 400, "{json}");
        assert_eq!(serde_json::from_str::<Overrides>(&json).unwrap(), o);
        // a changed use is kept; a rule the defaults lack is an extra rule
        s.rules[i].use_ = out("user choice");
        s.rules.push(rule("custom.thing", "m", material("custom")));
        let o = Overrides::of(&s);
        assert_eq!(o.rules[0].use_, Some(out("user choice")));
        assert_eq!(o.extra_rules.len(), 1);
        assert_eq!(o.apply(), s);
    }

    #[test]
    fn an_old_stored_rule_list_migrates_without_freezing_the_old_defaults() {
        // The shape the first version stored: the whole list, with post.mass counted and
        // post.length excluded, and the user's waste and haul on three rules.
        let old = r#"{"rules":[
            {"category":"pavement.layer_volume","unit":"m3","use_":{"MaterialFromAttribute":{"attribute":"material","prefix":"pavement: "}},"waste_pct":2.0,"haul_km":30.0},
            {"category":"post.mass","unit":"kg","use_":{"Material":{"key":"steel post"}},"waste_pct":5.0,"haul_km":null},
            {"category":"post.length","unit":"m","use_":{"Excluded":{"reason":"mass carries the post"}},"waste_pct":null,"haul_km":null},
            {"category":"marking.painted_area","unit":"m2","use_":{"Material":{"key":"road marking"}},"waste_pct":null,"haul_km":null}],
            "lighting":{"hours_per_year":4000.0,"dimming_factor":0.5,"design_life_years":null}}"#;
        let s: Settings = serde_json::from_str(old).unwrap();
        let o = Overrides::from_legacy(&s.rules, s.lighting.clone());
        assert_eq!(o.lighting.hours_per_year, Some(4000.0));
        let merged = o.apply();
        assert_eq!(merged.rules.len(), default_rules().len(), "rules added since reach the old project");
        let get = |c: &str| merged.rules.iter().find(|r| r.category == c).unwrap();
        assert_eq!(get("pavement.layer_volume").waste_pct, Some(2.0));
        assert_eq!(get("pavement.layer_volume").haul_km, Some(30.0));
        // posts are counted by length now; the waste entered on the mass moves with them
        assert!(get("post.length").use_.is_counting());
        assert_eq!(get("post.length").waste_pct, Some(5.0));
        assert!(matches!(get("post.mass").use_, Use::Excluded { .. }));
        assert_eq!(get("post.mass").waste_pct, None);
        assert!(get("marking_ext.painted_area").use_.is_counting());
        // only the user's figures are stored
        assert_eq!(o.rules.len(), 2);
        // an entry on post.length itself is not overwritten by the one from post.mass
        let mut s2 = s.clone();
        s2.rules[2].waste_pct = Some(7.0);
        assert_eq!(Overrides::from_legacy(&s2.rules, s2.lighting).apply().rules.iter().find(|r| r.category == "post.length").unwrap().waste_pct, Some(7.0));
    }

    #[test]
    fn unusable_inputs_are_named() {
        let mut s = Settings::default();
        assert!(s.problems().is_empty());
        s.rules[0].waste_pct = Some(-1.0);
        s.rules[1].haul_km = Some(f64::NAN);
        s.lighting.dimming_factor = Some(2.0);
        s.lighting.hours_per_year = Some(-5.0);
        assert_eq!(s.problems().len(), 4);
    }
}
