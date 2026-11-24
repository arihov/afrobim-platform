//! The 5D estimate in the app: collecting quantities from the model, storing the estimate and the
//! user's payment records in the document, and the join between the programme and the cost
//! crate (which are independent crates).
//!
//! Nothing here holds a rate, an exchange rate or a percentage. The primary currency is the
//! label UGX with 0 decimals; every value is entered by the user or loaded from the user's file.

use std::collections::{BTreeMap, BTreeSet};

use afrobim_cost::{
    add_estimate, collect_all, estimate_of, estimates, CollectOptions, Currency, Decimal, Estimate, EstimateElement, EstimateResult, EstimateSummary, MeasuredBill, PaymentTerms, Phasing, PhasingTarget,
    QuantitySet, ESTIMATE_CATEGORY,
};
use afrobim_fabrication::SignQuantity;
use afrobim_kernel::{Document, ElementId};
use afrobim_schedule::{ElementStates, Programme, Schedule};

use crate::schedule_glue::cost_date;

/// Property keys on the estimate element for what the crate's own data does not hold.
pub const PROP_TERMS: &str = "cost.payment_terms";
pub const PROP_CERTIFICATES: &str = "cost.certificates";
pub const PROP_SNAPSHOT: &str = "cost.model_snapshot.";

/// The primary currency label and its decimals. A label only: no exchange rate comes with it.
pub const PRIMARY_CURRENCY: &str = "UGX";
pub const PRIMARY_DECIMALS: u32 = 0;

/// A new, empty estimate in the primary currency.
pub fn new_estimate(name: &str) -> Estimate {
    Estimate::new(name, Currency::new(PRIMARY_CURRENCY, PRIMARY_DECIMALS))
}

/// The sign take-off rows of the document's signs, measured the way Export quantities measures
/// them (`afrobim_fabrication::measure` on the engine's layout of each sign). Empty without an engine.
pub fn sign_rows(engine: Option<&afrobim_signs::SignEngine>, doc: &Document) -> Vec<SignQuantity> {
    let Some(engine) = engine else { return Vec::new() };
    let elements: Vec<&afrobim_kernel::Element> = doc.elements().iter().collect();
    crate::signs_glue::measure_rows(engine, &crate::signs_glue::definitions(&elements))
}

/// Every quantity line of the model.
pub fn collect(doc: &Document, signs: &[SignQuantity]) -> QuantitySet {
    let mut set = collect_all(doc, signs, &CollectOptions::default());
    // `afrobim-cost` does not collect the second marking set (`marking_ext`); the app adds its lines.
    crate::ext_markings_glue::collect_into(doc, &mut set);
    // The civil quantities sent from EARTHWORKS: category, description, quantity, unit.
    for e in doc.elements().iter().filter(|e| e.category == crate::civil::CORRIDOR || e.category == crate::civil::PIPENET || e.category == crate::civil::GRADING) {
        let Some(text) = e.properties.get(crate::civil_quantities::PROP_BILL) else { continue };
        match serde_json::from_str::<Vec<(String, String, f64, String)>>(text) {
            Ok(lines) => {
                for (category, description, quantity, unit) in lines {
                    set.push(Some(e.id), &e.name, &category, &description, quantity, &unit, &[]);
                }
            }
            // An unreadable bill property is not an empty one: say that its lines are missing.
            Err(err) => set.skipped.push(afrobim_cost::SkippedSource { source: "civil bill".to_string(), mark: e.name.clone(), reason: format!("the property {} could not be read: {err}", crate::civil_quantities::PROP_BILL) }),
        }
    }
    set
}

/// The estimate element's id and name for each estimate in the document.
pub fn list(doc: &Document) -> Vec<(ElementId, String)> {
    estimates(doc)
}

/// Adds an estimate element to the document.
pub fn create(doc: &mut Document, name: &str) -> Result<(ElementId, Estimate), String> {
    let est = new_estimate(name);
    let id = add_estimate(doc, name, &est).map_err(|e| e.to_string())?;
    Ok((id, est))
}

pub fn load(doc: &Document, id: ElementId) -> Option<Estimate> {
    estimate_of(doc, id).map(|e| e.estimate)
}

/// Stores the estimate in its element together with the summary of the result it gave, when there is one.
pub fn store(doc: &mut Document, id: ElementId, est: &Estimate, result: Option<&EstimateResult>, skipped: usize) -> Result<(), String> {
    let data = EstimateElement { estimate: est.clone(), computed: result.map(|r| EstimateSummary::of(r, skipped)) };
    let value = serde_json::to_value(&data).map_err(|e| e.to_string())?;
    let e = doc.find_mut(id).filter(|e| e.category == ESTIMATE_CATEGORY).ok_or("The estimate is no longer in the document.")?;
    e.data = value;
    Ok(())
}

pub fn property<T: serde::de::DeserializeOwned>(doc: &Document, id: ElementId, key: &str) -> Option<T> {
    doc.find(id)?.properties.get(key).and_then(|v| serde_json::from_str(v).ok())
}

pub fn set_property<T: serde::Serialize>(doc: &mut Document, id: ElementId, key: &str, value: &T) {
    if let (Some(e), Ok(text)) = (doc.find_mut(id), serde_json::to_string(value)) {
        e.properties.insert(key.to_string(), text);
    }
}

pub fn terms_of(doc: &Document, id: ElementId) -> PaymentTerms {
    property(doc, id, PROP_TERMS).unwrap_or_default()
}

/// Warnings about elements that two lines measure in two ways: an element mapped through two
/// different quantity categories (a post as length and as a count, a sign as area and as a count)
/// is paid for twice when both items are priced, as the cost crate documents.
pub fn double_count_warnings(measured: &MeasuredBill) -> Vec<String> {
    let mut by_element: BTreeMap<ElementId, (String, BTreeSet<(String, String)>)> = BTreeMap::new();
    for item in &measured.items {
        for c in &item.contributions {
            if let Some(e) = c.element {
                let entry = by_element.entry(e).or_insert_with(|| (c.mark.clone(), BTreeSet::new()));
                entry.1.insert((c.category.clone(), item.item.code.clone()));
            }
        }
    }
    let mut out = Vec::new();
    for (mark, pairs) in by_element.values() {
        // Earthworks cut and fill are different work, not two ways of measuring the same work.
        let categories: BTreeSet<&String> = pairs.iter().map(|p| &p.0).filter(|c| !c.starts_with("earthworks.")).collect();
        if categories.len() > 1 {
            let list: Vec<String> = pairs.iter().filter(|p| !p.0.starts_with("earthworks.")).map(|(cat, item)| format!("{cat} in item {item}")).collect();
            out.push(format!("{mark} is measured by lines of different kinds ({}). If these lines measure the same work and more than one of the items is priced, it is paid for more than once.", list.join("; ")));
        }
    }
    out
}

/// Cash-flow windows taken from the programme: for each measured item, from the earliest early
/// start to the latest early finish of the activities its elements are linked to. Items with no
/// linked element are listed in the notes and get no window.
pub fn phasing_from_programme(measured: &MeasuredBill, prog: &Programme, sched: &Schedule, states: &ElementStates) -> (Vec<Phasing>, Vec<String>) {
    let _ = prog;
    let mut out = Vec::new();
    let mut notes = Vec::new();
    for item in &measured.items {
        let elements = item.elements();
        if elements.is_empty() {
            notes.push(format!("Item {} has no measured element, so it has no linked activity.", item.item.code));
            continue;
        }
        let mut activities: BTreeSet<String> = BTreeSet::new();
        for e in &elements {
            if let Some(st) = states.get(*e) {
                activities.extend(st.links.iter().map(|l| l.activity.clone()));
            }
        }
        let rows: Vec<_> = activities.iter().filter_map(|a| sched.row(a)).collect();
        let (Some(start), Some(finish)) = (rows.iter().map(|r| r.early_start).min(), rows.iter().map(|r| r.early_finish).max()) else {
            notes.push(format!("Item {}: none of its {} element(s) is linked to an activity.", item.item.code, elements.len()));
            continue;
        };
        out.push(Phasing { target: PhasingTarget::Item(item.item.code.clone()), start: cost_date(start), finish: cost_date(finish), distribution: Default::default() });
    }
    (out, notes)
}

/// A decimal typed by the user: empty is None, anything else must be a decimal number.
pub fn decimal_of(text: &str) -> Result<Option<Decimal>, String> {
    let t = text.trim();
    if t.is_empty() {
        return Ok(None);
    }
    Decimal::parse(t).map(Some).map_err(|e| e.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use afrobim_cost::{Bill, Item, MappingRule};
    use afrobim_kernel::Pt;

    #[test]
    fn the_estimate_survives_save_and_open() {
        let mut doc = Document::default();
        doc.add("sign", "S1", Pt::new(0.0, 0.0), 10.0, 10.0, "0", serde_json::Value::Null);
        let (id, mut est) = create(&mut doc, "Test estimate").unwrap();
        assert_eq!(est.reporting_currency, "UGX");
        assert_eq!(est.currencies.currency("UGX").unwrap().decimals, 0);
        // A fixture bill and rate: arbitrary numbers chosen for the test only.
        est.bill = Bill::neutral(1);
        est.bill.items.push(Item::measured("1.1", "1", "A thing", "nr"));
        est.bill.rules.push(MappingRule::new("things", "1.1", "sign.face", vec![]));
        est.rates.set(afrobim_cost::Rate {
            item_code: "1.1".into(),
            description: String::new(),
            unit: "nr".into(),
            rate: Decimal::parse("2").unwrap(),
            currency: "UGX".into(),
            source_note: "fixture".into(),
            date: None,
        });
        est.revision = "fixture rev".into();
        store(&mut doc, id, &est, None, 0).unwrap();
        set_property(&mut doc, id, PROP_TERMS, &PaymentTerms::default());
        let reopened = Document::from_json(&doc.to_json()).unwrap();
        assert_eq!(list(&reopened), vec![(id, "Test estimate".to_string())]);
        assert_eq!(load(&reopened, id).unwrap(), est);
        assert_eq!(terms_of(&reopened, id), PaymentTerms::default());
        // With a result, the stored summary is written too and the estimate is unchanged.
        let result = est.compute(&QuantitySet::new()).unwrap();
        let mut again = reopened.clone();
        store(&mut again, id, &est, Some(&result), 0).unwrap();
        let back = estimate_of(&again, id).unwrap();
        assert_eq!(back.estimate, est);
        assert!(back.computed.is_some());
    }

    #[test]
    fn an_element_measured_two_ways_is_flagged() {
        let mut set = QuantitySet::new();
        let e = Some(ElementId(5));
        set.push(e, "P1", "post.length", "", 2.0, "m", &[]);
        set.push(e, "P1", "post.count", "", 1.0, "nr", &[]);
        set.push(Some(ElementId(6)), "P2", "post.count", "", 1.0, "nr", &[]);
        let mut bill = Bill::neutral(1);
        bill.items.push(Item::measured("1", "1", "Post length", "m"));
        bill.items.push(Item::measured("2", "1", "Post count", "nr"));
        bill.rules.push(MappingRule::new("len", "1", "post.length", vec![]));
        bill.rules.push(MappingRule::new("cnt", "2", "post.count", vec![]));
        let measured = bill.measure(&set).unwrap();
        let w = double_count_warnings(&measured);
        assert_eq!(w.len(), 1, "{w:?}");
        assert!(w[0].starts_with("P1 is measured by lines of different kinds"));
    }
}
