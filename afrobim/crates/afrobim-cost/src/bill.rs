//! The bill of quantities: sections, items, the rules that map quantity lines to items, manual
//! quantities and adjustments, and the measurement of a [`QuantitySet`] against all of that with
//! a full audit trail.
//!
//! This module holds no item numbers, no descriptions and no rates of its own. A bill is entered
//! by the user, built from [`Bill::neutral`] (numbered sections the user renames) or loaded
//! from the user's own CSV ([`Bill::from_structure_csv`]).

use std::collections::{BTreeMap, BTreeSet};

use afrobim_kernel::ElementId;
use serde::{Deserialize, Serialize};

use crate::lines::same_unit;
use crate::{csv, CostError, Decimal, QuantityLine, QuantitySet, Result, Rounding, RoundingMode};

/// A section of the bill: a code and a title, both the user's.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Section {
    pub code: String,
    pub title: String,
}

/// How an item gets its quantity.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum ItemKind {
    /// From the model: the quantity lines mapped to it by [`MappingRule`]s, plus adjustments.
    Measured,
    /// Entered by hand, with the reason it is not measured from the model (a provisional
    /// quantity, work the model does not carry).
    Manual { quantity: Decimal, reason: String },
}

/// One item of the bill.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Item {
    pub code: String,
    /// The code of the section it sits in.
    pub section: String,
    pub description: String,
    pub unit: String,
    pub kind: ItemKind,
    /// The code looked up in the rate library; the item's own code when None.
    #[serde(default)]
    pub rate_code: Option<String>,
    /// The currency of the rate to take when the library holds several for the code; the
    /// estimate's reporting currency, then the first found, when None.
    #[serde(default)]
    pub rate_currency: Option<String>,
}

impl Item {
    /// A measured item.
    pub fn measured(code: &str, section: &str, description: &str, unit: &str) -> Item {
        Item {
            code: code.into(),
            section: section.into(),
            description: description.into(),
            unit: unit.into(),
            kind: ItemKind::Measured,
            rate_code: None,
            rate_currency: None,
        }
    }

    /// A manual item with its reason.
    pub fn manual(code: &str, section: &str, description: &str, unit: &str, quantity: Decimal, reason: &str) -> Item {
        Item { kind: ItemKind::Manual { quantity, reason: reason.into() }, ..Item::measured(code, section, description, unit) }
    }

    pub fn rate_code(&self) -> &str {
        self.rate_code.as_deref().unwrap_or(&self.code)
    }
}

/// A test on one attribute of a line. The key is an attribute name, or one of the line's own
/// fields `mark`, `description`, `unit`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum Match {
    /// Equal as text (trimmed, any case); two numbers are equal when equal as numbers (`100` and
    /// `100.0`).
    Equals(String),
    /// Equal to any of these, by the same rule.
    OneOf(Vec<String>),
    /// The text contains this (any case).
    Contains(String),
    /// The attribute exists and is not empty.
    Present,
    /// The attribute is a number at least this.
    AtLeast(Decimal),
    /// The attribute is a number at most this.
    AtMost(Decimal),
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Condition {
    pub key: String,
    pub test: Match,
}

impl Condition {
    pub fn equals(key: &str, value: &str) -> Condition {
        Condition { key: key.into(), test: Match::Equals(value.into()) }
    }

    fn holds(&self, line: &QuantityLine) -> bool {
        let value = match self.key.as_str() {
            "mark" => Some(line.mark.as_str()),
            "description" => Some(line.description.as_str()),
            "unit" => Some(line.unit.as_str()),
            k => line.attributes.get(k).map(String::as_str),
        };
        let Some(v) = value else { return false };
        match &self.test {
            Match::Equals(e) => text_equal(v, e),
            Match::OneOf(list) => list.iter().any(|e| text_equal(v, e)),
            Match::Contains(s) => v.to_lowercase().contains(&s.trim().to_lowercase()),
            Match::Present => !v.trim().is_empty(),
            Match::AtLeast(n) => Decimal::parse(v).is_ok_and(|x| x >= *n),
            Match::AtMost(n) => Decimal::parse(v).is_ok_and(|x| x <= *n),
        }
    }
}

fn text_equal(a: &str, b: &str) -> bool {
    if a.trim().eq_ignore_ascii_case(b.trim()) {
        return true;
    }
    matches!((Decimal::parse(a), Decimal::parse(b)), (Ok(x), Ok(y)) if x == y)
}

/// Assigns lines to an item: every line whose category matches and whose conditions all hold.
///
/// Rules are tried in order and a line goes to the first rule that fits. A line that also fits a
/// later rule is reported as ambiguous. The line's unit must be the item's unit unless the rule
/// gives a `factor`, which is the user's own conversion (the line quantity is multiplied by it).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct MappingRule {
    /// A name for the audit trail.
    pub name: String,
    /// The item code the lines go to.
    pub item: String,
    /// The line category, e.g. `marking.painted_area`; a trailing `*` matches a prefix
    /// (`marking.*`), and `*` alone matches every category.
    pub category: String,
    #[serde(default)]
    pub conditions: Vec<Condition>,
    /// Multiplier applied to the line quantity; None means 1 and requires equal units.
    #[serde(default)]
    pub factor: Option<Decimal>,
}

impl MappingRule {
    pub fn new(name: &str, item: &str, category: &str, conditions: Vec<Condition>) -> MappingRule {
        MappingRule { name: name.into(), item: item.into(), category: category.into(), conditions, factor: None }
    }

    fn category_matches(&self, category: &str) -> bool {
        match self.category.strip_suffix('*') {
            Some(prefix) => category.to_lowercase().starts_with(&prefix.to_lowercase()),
            None => self.category.eq_ignore_ascii_case(category),
        }
    }

    /// Whether the line is in scope of this rule (category and conditions).
    pub fn matches(&self, line: &QuantityLine) -> bool {
        self.category_matches(&line.category) && self.conditions.iter().all(|c| c.holds(line))
    }
}

/// A manual change to the measured quantity of an item, always with its reason.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Adjustment {
    pub item: String,
    /// Added to the measured quantity (negative to omit).
    pub delta: Decimal,
    pub reason: String,
}

/// Whether the amount of each item is rounded, or only totals are.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
pub enum AmountRounding {
    /// Each item amount is rounded to the currency's decimals; totals are sums of rounded
    /// amounts, so a total always agrees with the amounts printed above it.
    #[default]
    PerItem,
    /// Item amounts keep full precision; sections and totals are rounded.
    TotalsOnly,
}

/// Every rounding decision of a bill, in one place.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct RoundingRules {
    /// The quantity of an item after summing lines, adjustments and manual quantity.
    pub quantity: Rounding,
    /// Overrides by unit (lower case), e.g. `nr` rounded up to whole numbers.
    #[serde(default)]
    pub quantity_by_unit: BTreeMap<String, Rounding>,
    /// How amounts, converted amounts and additions are rounded; the decimals come from the
    /// currency (see `Currency::decimals`).
    #[serde(default)]
    pub amount_mode: RoundingMode,
    #[serde(default)]
    pub amount_rounding: AmountRounding,
    /// Rounding of a rate summed from a build-up.
    pub build_up: Rounding,
}

impl Default for RoundingRules {
    fn default() -> Self {
        RoundingRules {
            quantity: Rounding::half_up(3),
            quantity_by_unit: BTreeMap::new(),
            amount_mode: RoundingMode::HalfAwayFromZero,
            amount_rounding: AmountRounding::PerItem,
            build_up: Rounding::half_up(2),
        }
    }
}

impl RoundingRules {
    pub fn quantity_for(&self, unit: &str) -> Rounding {
        self.quantity_by_unit.get(&unit.trim().to_lowercase()).copied().unwrap_or(self.quantity)
    }

    /// The rounding of an amount in a currency with this many decimals.
    pub fn amount_for(&self, currency_decimals: u32) -> Rounding {
        Rounding::new(currency_decimals, self.amount_mode)
    }
}

/// The bill as the user defines it. Serialisable; derives nothing from the model.
#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
pub struct Bill {
    pub sections: Vec<Section>,
    pub items: Vec<Item>,
    pub rules: Vec<MappingRule>,
    #[serde(default)]
    pub adjustments: Vec<Adjustment>,
    #[serde(default)]
    pub rounding: RoundingRules,
}

/// A line that went to an item.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Contribution {
    /// The line (index in the [`QuantitySet`]).
    pub line: usize,
    pub element: Option<ElementId>,
    pub mark: String,
    pub category: String,
    /// The line's own quantity and unit.
    pub line_quantity: Decimal,
    pub line_unit: String,
    /// The rule that took it and its factor (1 when the rule has none).
    pub rule: String,
    pub factor: Decimal,
    /// `line_quantity * factor`: what it adds to the item, unrounded.
    pub quantity: Decimal,
}

/// An item with everything behind its quantity.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct MeasuredItem {
    pub item: Item,
    pub contributions: Vec<Contribution>,
    /// Sum of the contributions, unrounded.
    pub from_lines: Decimal,
    pub adjustments: Vec<Adjustment>,
    /// The manual quantity of a manual item.
    pub manual: Option<Decimal>,
    /// `from_lines + adjustments + manual`, unrounded.
    pub unrounded: Decimal,
    /// The quantity of the bill: `unrounded` rounded by the bill's rule for the unit.
    pub quantity: Decimal,
}

impl MeasuredItem {
    /// The distinct elements behind the item, in id order.
    pub fn elements(&self) -> Vec<ElementId> {
        self.contributions.iter().filter_map(|c| c.element).collect::<BTreeSet<_>>().into_iter().collect()
    }

    /// The quantity an element contributes to this item (unrounded).
    pub fn quantity_of(&self, element: ElementId) -> Decimal {
        self.contributions.iter().filter(|c| c.element == Some(element)).map(|c| c.quantity).sum()
    }
}

/// Why a line is not in the bill.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum UnmappedReason {
    /// No rule matches its category and attributes.
    NoRule,
    /// A rule matches, but its item is measured in another unit and the rule has no factor.
    UnitMismatch { rule: String, line_unit: String, item_unit: String },
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct UnmappedLine {
    pub line: QuantityLine,
    pub reason: UnmappedReason,
}

/// A line that more than one rule fits; it went to `used`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct AmbiguousLine {
    pub line: usize,
    pub used: String,
    pub also: Vec<String>,
}

/// A bill measured against a quantity set.
#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
pub struct MeasuredBill {
    /// In the bill's order.
    pub items: Vec<MeasuredItem>,
    /// Lines no item took. Never dropped: they are here, in line order.
    pub unmapped: Vec<UnmappedLine>,
    pub ambiguous: Vec<AmbiguousLine>,
    /// Lines offered, and lines that reached an item. `mapped + unmapped.len() == offered`.
    pub lines_offered: usize,
    pub lines_mapped: usize,
}

impl MeasuredBill {
    pub fn item(&self, code: &str) -> Option<&MeasuredItem> {
        self.items.iter().find(|i| i.item.code == code)
    }

    /// Unmapped quantity by category and unit.
    pub fn unmapped_totals(&self) -> Vec<(String, String, usize, Decimal)> {
        let mut map: BTreeMap<(String, String), (usize, Decimal)> = BTreeMap::new();
        for u in &self.unmapped {
            let e = map.entry((u.line.category.clone(), u.line.unit.clone())).or_default();
            e.0 += 1;
            e.1 += u.line.quantity;
        }
        map.into_iter().map(|((c, u), (n, q))| (c, u, n, q)).collect()
    }
}

impl Bill {
    /// A bill of numbered, untitled-in-substance sections the user renames: codes `1`..`count`,
    /// titles `Section 1`... No items, no rules.
    pub fn neutral(count: usize) -> Bill {
        Bill {
            sections: (1..=count).map(|i| Section { code: i.to_string(), title: format!("Section {i}") }).collect(),
            ..Bill::default()
        }
    }

    pub fn section(&self, code: &str) -> Option<&Section> {
        self.sections.iter().find(|s| s.code == code)
    }

    pub fn item(&self, code: &str) -> Option<&Item> {
        self.items.iter().find(|i| i.code == code)
    }

    /// Everything wrong with the structure; empty when it is sound.
    pub fn validate(&self) -> Vec<String> {
        let mut p = Vec::new();
        let mut seen = BTreeSet::new();
        for s in &self.sections {
            if s.code.trim().is_empty() {
                p.push("a section has no code".to_string());
            } else if !seen.insert(s.code.as_str()) {
                p.push(format!("section code '{}' is used twice", s.code));
            }
        }
        let mut codes = BTreeSet::new();
        for i in &self.items {
            if i.code.trim().is_empty() {
                p.push("an item has no code".to_string());
            } else if !codes.insert(i.code.as_str()) {
                p.push(format!("item code '{}' is used twice", i.code));
            }
            if self.section(&i.section).is_none() {
                p.push(format!("item '{}' is in section '{}', which does not exist", i.code, i.section));
            }
            if i.unit.trim().is_empty() {
                p.push(format!("item '{}' has no unit", i.code));
            }
            if let ItemKind::Manual { reason, .. } = &i.kind {
                if reason.trim().is_empty() {
                    p.push(format!("manual item '{}' has no reason", i.code));
                }
            }
        }
        for r in &self.rules {
            match self.item(&r.item) {
                None => p.push(format!("rule '{}' maps to item '{}', which does not exist", r.name, r.item)),
                Some(i) if i.kind != ItemKind::Measured => {
                    p.push(format!("rule '{}' maps to manual item '{}'; only measured items take lines", r.name, r.item))
                }
                _ => {}
            }
            if r.category.trim().is_empty() {
                p.push(format!("rule '{}' has no category", r.name));
            }
            if r.factor.is_some_and(|f| f.is_zero()) {
                p.push(format!("rule '{}' has a factor of zero", r.name));
            }
        }
        for a in &self.adjustments {
            if self.item(&a.item).is_none() {
                p.push(format!("an adjustment refers to item '{}', which does not exist", a.item));
            }
            if a.reason.trim().is_empty() {
                p.push(format!("the adjustment of item '{}' has no reason", a.item));
            }
        }
        p
    }

    /// Measures the quantity set against the bill. Fails only when the structure is unsound.
    ///
    /// Each line goes to the first rule that matches it and whose unit fits. The result keeps, for
    /// every item, the lines and elements behind it; lines no rule took are in `unmapped`.
    pub fn measure(&self, set: &QuantitySet) -> Result<MeasuredBill> {
        let problems = self.validate();
        if !problems.is_empty() {
            return Err(CostError::Structure(problems));
        }
        let mut items: Vec<MeasuredItem> = self
            .items
            .iter()
            .map(|i| MeasuredItem {
                item: i.clone(),
                contributions: Vec::new(),
                from_lines: Decimal::ZERO,
                adjustments: self.adjustments.iter().filter(|a| a.item == i.code).cloned().collect(),
                manual: match &i.kind {
                    ItemKind::Manual { quantity, .. } => Some(*quantity),
                    ItemKind::Measured => None,
                },
                unrounded: Decimal::ZERO,
                quantity: Decimal::ZERO,
            })
            .collect();
        let index: BTreeMap<&str, usize> = self.items.iter().enumerate().map(|(n, i)| (i.code.as_str(), n)).collect();
        let mut out = MeasuredBill { lines_offered: set.lines.len(), ..MeasuredBill::default() };

        for line in &set.lines {
            let mut usable: Vec<(&MappingRule, usize)> = Vec::new();
            let mut first_mismatch: Option<UnmappedReason> = None;
            for rule in self.rules.iter().filter(|r| r.matches(line)) {
                let n = index[rule.item.as_str()];
                let unit = &self.items[n].unit;
                if rule.factor.is_none() && !same_unit(unit, &line.unit) {
                    first_mismatch.get_or_insert(UnmappedReason::UnitMismatch {
                        rule: rule.name.clone(),
                        line_unit: line.unit.clone(),
                        item_unit: unit.clone(),
                    });
                    continue;
                }
                usable.push((rule, n));
            }
            let Some(&(rule, n)) = usable.first() else {
                out.unmapped.push(UnmappedLine { line: line.clone(), reason: first_mismatch.unwrap_or(UnmappedReason::NoRule) });
                continue;
            };
            if usable.len() > 1 {
                out.ambiguous.push(AmbiguousLine {
                    line: line.id,
                    used: rule.name.clone(),
                    also: usable[1..].iter().map(|(r, _)| r.name.clone()).collect(),
                });
            }
            let factor = rule.factor.unwrap_or(Decimal::ONE);
            let quantity = line.quantity.mul_rounded(factor, Rounding::half_up(crate::decimal::DECIMALS));
            items[n].contributions.push(Contribution {
                line: line.id,
                element: line.element,
                mark: line.mark.clone(),
                category: line.category.clone(),
                line_quantity: line.quantity,
                line_unit: line.unit.clone(),
                rule: rule.name.clone(),
                factor,
                quantity,
            });
            out.lines_mapped += 1;
        }

        for m in &mut items {
            m.from_lines = m.contributions.iter().map(|c| c.quantity).sum();
            m.unrounded = m.from_lines + m.adjustments.iter().map(|a| a.delta).sum::<Decimal>() + m.manual.unwrap_or_default();
            m.quantity = m.unrounded.round(self.rounding.quantity_for(&m.item.unit));
        }
        out.items = items;
        Ok(out)
    }

    // --- CSV: structure and rules --------------------------------------------------------

    /// Loads sections and items from the user's CSV, replacing this bill's sections and items.
    /// Columns `kind,code,section,description,unit`; `kind` is `section` or `item`; a section row
    /// puts its title in `description`; an item row names its section by code. Rules,
    /// adjustments and rounding are kept.
    pub fn from_structure_csv(&mut self, text: &str) -> Result<()> {
        let rows = csv::parse_with_header(text, &STRUCTURE_HEADER)?;
        let (mut sections, mut items) = (Vec::new(), Vec::new());
        for (n, r) in rows.iter().enumerate() {
            let (code, section, description, unit) = (r[1].trim(), r[2].trim(), r[3].trim(), r[4].trim());
            match r[0].trim().to_lowercase().as_str() {
                "section" => sections.push(Section { code: code.into(), title: description.into() }),
                "item" => items.push(Item::measured(code, section, description, unit)),
                other => return Err(CostError::Csv(format!("row {}: kind must be 'section' or 'item', not '{other}'", n + 2))),
            }
        }
        self.sections = sections;
        self.items = items;
        Ok(())
    }

    /// The structure as CSV, loadable by [`Bill::from_structure_csv`] (manual quantities are not
    /// part of the structure).
    pub fn structure_to_csv(&self) -> String {
        let mut out = csv::row(&STRUCTURE_HEADER);
        for s in &self.sections {
            out.push_str(&csv::row(&["section", &s.code, "", &s.title, ""]));
        }
        for i in &self.items {
            out.push_str(&csv::row(&["item", &i.code, &i.section, &i.description, &i.unit]));
        }
        out
    }

    /// Replaces the rules with those in the user's CSV. Columns
    /// `name,item,category,factor,conditions`; conditions are separated by `;` and written
    /// `key=value` (alternatives `a|b`), `key~text` (contains), `key>=number`, `key<=number`,
    /// `key?` (present). Values must not contain `;` or `|`.
    pub fn rules_from_csv(&mut self, text: &str) -> Result<()> {
        let rows = csv::parse_with_header(text, &RULES_HEADER)?;
        let mut rules = Vec::new();
        for (n, r) in rows.iter().enumerate() {
            let factor = if r[3].trim().is_empty() { None } else { Some(Decimal::parse(&r[3])?) };
            let mut conditions = Vec::new();
            for c in r[4].split(';').map(str::trim).filter(|c| !c.is_empty()) {
                conditions.push(parse_condition(c).map_err(|m| CostError::Csv(format!("row {}: {m}", n + 2)))?);
            }
            rules.push(MappingRule { name: r[0].trim().into(), item: r[1].trim().into(), category: r[2].trim().into(), conditions, factor });
        }
        self.rules = rules;
        Ok(())
    }

    /// The rules as CSV, loadable by [`Bill::rules_from_csv`].
    pub fn rules_to_csv(&self) -> String {
        let mut out = csv::row(&RULES_HEADER);
        for r in &self.rules {
            let conds = r.conditions.iter().map(condition_text).collect::<Vec<_>>().join(";");
            out.push_str(&csv::row(&[
                r.name.as_str(),
                &r.item,
                &r.category,
                &r.factor.map(|f| f.to_string()).unwrap_or_default(),
                &conds,
            ]));
        }
        out
    }
}

const STRUCTURE_HEADER: [&str; 5] = ["kind", "code", "section", "description", "unit"];
const RULES_HEADER: [&str; 5] = ["name", "item", "category", "factor", "conditions"];

fn parse_condition(c: &str) -> std::result::Result<Condition, String> {
    let num = |s: &str| Decimal::parse(s).map_err(|e| format!("condition '{c}': {e}"));
    if let Some((k, v)) = c.split_once(">=") {
        return Ok(Condition { key: k.trim().into(), test: Match::AtLeast(num(v)?) });
    }
    if let Some((k, v)) = c.split_once("<=") {
        return Ok(Condition { key: k.trim().into(), test: Match::AtMost(num(v)?) });
    }
    if let Some((k, v)) = c.split_once('~') {
        return Ok(Condition { key: k.trim().into(), test: Match::Contains(v.trim().into()) });
    }
    if let Some(k) = c.strip_suffix('?') {
        return Ok(Condition { key: k.trim().into(), test: Match::Present });
    }
    if let Some((k, v)) = c.split_once('=') {
        let alts: Vec<String> = v.split('|').map(|s| s.trim().to_string()).collect();
        let test = if alts.len() > 1 { Match::OneOf(alts) } else { Match::Equals(v.trim().into()) };
        return Ok(Condition { key: k.trim().into(), test });
    }
    Err(format!("condition '{c}' is not key=value, key~text, key>=n, key<=n or key?"))
}

fn condition_text(c: &Condition) -> String {
    match &c.test {
        Match::Equals(v) => format!("{}={v}", c.key),
        Match::OneOf(v) => format!("{}={}", c.key, v.join("|")),
        Match::Contains(v) => format!("{}~{v}", c.key),
        Match::Present => format!("{}?", c.key),
        Match::AtLeast(n) => format!("{}>={n}", c.key),
        Match::AtMost(n) => format!("{}<={n}", c.key),
    }
}
