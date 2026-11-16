//! Small helpers the civil dialogs share: numeric fields that return their value, a chooser over
//! the objects of a category, and a find-by-name over the document.

use afrobim_kernel::{ElementId};
use egui::Ui;

use crate::ui_kit::{self, Check, NumRule};

/// A numeric field that returns its value when valid. A required field with nothing typed is
/// reported by the field itself.
pub fn num(ui: &mut Ui, text: &mut String, unit: &str) -> Option<f64> {
    ui_kit::num_field(ui, text, unit, &NumRule::required()).check.value()
}

/// As `num` for a field that may be left blank.
pub fn num_opt(ui: &mut Ui, text: &mut String, unit: &str) -> Result<Option<f64>, ()> {
    match ui_kit::num_field(ui, text, unit, &NumRule::default()).check {
        Check::Empty => Ok(None),
        Check::Ok(v) | Check::Warn(v, _) => Ok(Some(v)),
        Check::Bad(_) => Err(()),
    }
}

/// A numeric field with a rule.
pub fn num_rule(ui: &mut Ui, text: &mut String, unit: &str, rule: NumRule) -> Option<f64> {
    ui_kit::num_field(ui, text, unit, &rule).check.value()
}

pub fn text(ui: &mut Ui, s: &mut String, hint: &str) {
    ui_kit::text_field(ui, s, hint);
}

pub fn pick_from(ui: &mut Ui, id: &str, items: &[(ElementId, String)], sel: &mut Option<ElementId>) -> bool {
    let mut changed = false;
    let shown = sel.and_then(|s| items.iter().find(|i| i.0 == s)).map(|i| i.1.clone()).unwrap_or_else(|| "(none)".into());
    egui::ComboBox::from_id_salt(("civil-pick", id)).selected_text(shown).width(240.0).show_ui(ui, |ui| {
        for (eid, name) in items {
            if ui.selectable_label(*sel == Some(*eid), name).clicked() {
                *sel = Some(*eid);
                changed = true;
            }
        }
    });
    changed
}

pub fn fmt(v: f64, decimals: usize) -> String {
    format!("{v:.decimals$}")
}

/// A note shown in a dialog when nothing can be done yet.
pub fn needs(ui: &mut Ui, text: &str) {
    ui_kit::callout(ui, ui_kit::Level::Info, text);
}

/// As `num_opt` with a rule (for example greater than zero).
pub fn num_opt_rule(ui: &mut Ui, text: &mut String, unit: &str, rule: &NumRule) -> Result<Option<f64>, ()> {
    match ui_kit::num_field(ui, text, unit, rule).check {
        Check::Empty => Ok(None),
        Check::Ok(v) | Check::Warn(v, _) => Ok(Some(v)),
        Check::Bad(_) => Err(()),
    }
}

/// The value of a field's text under a rule, without drawing the field.
pub fn num_rule_peek(text: &str, rule: &NumRule) -> Option<f64> {
    ui_kit::check_number(text, rule).value()
}
