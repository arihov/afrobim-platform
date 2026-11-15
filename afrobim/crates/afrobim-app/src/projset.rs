//! PROJECTSETTINGS: the project's rule for meeting a mph threshold with a km/h limit.
//!
//! UK requirements are written in mph bands. A posted limit in km/h (80 km/h is 49.71 mph) needs
//! one stated rule for how it meets a threshold, and that rule is the engineer's decision, kept
//! in the document (`afrobim_kernel::speed_band`). Every place in the app that maps a limit to a
//! UK band calls [`resolve`] here with the stored rule and the thresholds of its own check, and
//! puts the resulting sentence into the calculation lines.
//!
//! The thresholds are the checks' own, never built into the rule:
//! - the restraint check (DMRB CD 377 as the restraint engine reads it): one step, 50 mph;
//! - sign visibility: the upper figures of the rows of the visibility tables, 20, 30, 40, 50, 60 mph;
//! - UK road marking selection: the speed limits at which the marking tables change, 40, 50, 60, 70 mph.

use afrobim_kernel::speed_band::{resolve as kernel_resolve, Resolved, RoundUpTolerance, SpeedUnit};
use afrobim_kernel::{set_speed_band_rule, speed_band_rule, Document, SpeedBandRule};
use egui::RichText;

use crate::app::AfroBimApp;

/// The restraint check's one step, mph (`afrobim_restraint::design::SPEED_THRESHOLD_MPH`).
pub fn restraint_thresholds() -> Vec<f64> {
    vec![afrobim_restraint::design::SPEED_THRESHOLD_MPH]
}

/// The upper figures of the visibility tables' bands (`SpeedBandMph`: up to 20, 21 to 30, 31 to 40, 41 to 50, 51 to 60, over 60).
pub fn visibility_thresholds() -> Vec<f64> {
    vec![20.0, 30.0, 40.0, 50.0, 60.0]
}

/// The limits at which the road marking tables change (`afrobim_markings::SpeedBand`: 40 or less, over 40, 50, 60, 70).
pub fn marking_thresholds() -> Vec<f64> {
    vec![40.0, 50.0, 60.0, 70.0]
}

/// Resolves one limit against the thresholds of a check, with the document's stored rule.
pub fn resolve(doc: &Document, limit: f64, mph: bool, thresholds: &[f64]) -> Resolved {
    kernel_resolve(limit, if mph { SpeedUnit::Mph } else { SpeedUnit::Kmh }, thresholds, &speed_band_rule(doc))
}

/// Settles a question the rule leaves (`AskEachTime`): `answer` says whether the band is taken.
/// With no answer the result stays a question and `Resolved::ask` is set.
pub fn resolve_answered(doc: &Document, limit: f64, mph: bool, thresholds: &[f64], answer: Option<bool>) -> Resolved {
    let r = resolve(doc, limit, mph, thresholds);
    match (r.ask.is_some(), answer) {
        (true, Some(a)) => r.answered(a),
        _ => r,
    }
}

/// What the rule says in words, as the engine defines it. Shown beside the choice.
pub fn definition(rule: &SpeedBandRule) -> String {
    match rule {
        SpeedBandRule::RoundUp { tolerance: RoundUpTolerance::ConversionRounding } => "Round up within conversion rounding (the default). With v the exact mph and t the smallest threshold at or above it: if v is just under t, so that v rounds to t at the nearest whole mph (t - v at most 0.5 mph), the requirement of the band at t applies. 80 km/h (49.71 mph) is treated as 50 mph; 70 km/h (43.50 mph) is not treated as 50. It applies to limits converted from km/h; a limit given in mph is used exactly.".into(),
        SpeedBandRule::RoundUp { tolerance: RoundUpTolerance::NextBandUp } => "Always the next band up: any limit below a threshold is treated as that threshold, however far below it is.".into(),
        SpeedBandRule::RoundUp { tolerance: RoundUpTolerance::Mph(x) } => format!("Round up within {x} mph: a limit at most {x} mph below a threshold is treated as that threshold. It applies to limits in either unit."),
        SpeedBandRule::Exact => "Exact: the converted value is used as it is. 80 km/h stays 49.71 mph, which is below the 50 mph step.".into(),
        SpeedBandRule::AskEachTime => "Ask each time: the check uses the exact value and asks you whether to take the band, wherever the default rule would round up. Until you answer the check is not made.".into(),
        SpeedBandRule::Table(_) => "A table you enter: each km/h limit in it is given the mph value beside it; a limit that is not listed, or one given in mph, is used exactly and the note says so.".into(),
    }
}

/// The choice shown in the window.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Choice {
    RoundUp,
    NextBandUp,
    Tolerance,
    Exact,
    Ask,
    Table,
}

fn choice_of(rule: &SpeedBandRule) -> Choice {
    match rule {
        SpeedBandRule::RoundUp { tolerance: RoundUpTolerance::ConversionRounding } => Choice::RoundUp,
        SpeedBandRule::RoundUp { tolerance: RoundUpTolerance::NextBandUp } => Choice::NextBandUp,
        SpeedBandRule::RoundUp { tolerance: RoundUpTolerance::Mph(_) } => Choice::Tolerance,
        SpeedBandRule::Exact => Choice::Exact,
        SpeedBandRule::AskEachTime => Choice::Ask,
        SpeedBandRule::Table(_) => Choice::Table,
    }
}

#[derive(Default)]
pub struct ProjSetUi {
    pub open: bool,
    table_text: Vec<(String, String)>,
}

impl AfroBimApp {
    pub(crate) fn projset_window(&mut self, ctx: &egui::Context) {
        if !self.projset.open {
            return;
        }
        let mut open = true;
        let mut rule = speed_band_rule(&self.doc);
        let before = rule.clone();
        crate::win::new(ctx, "Project settings").open(&mut open).default_width(560.0).show(ctx, |ui| {
            ui.label(RichText::new("Speed limits in km/h against UK thresholds in mph").strong());
            ui.label(RichText::new("Used by the restraint check, sign visibility and the road marking choice. The rule is your decision for the project and is saved in the document.").small().weak());
            ui.add_space(6.0);
            let mut c = choice_of(&rule);
            let before_c = c;
            for (v, label) in [
                (Choice::RoundUp, "Round up within conversion rounding (default)"),
                (Choice::NextBandUp, "Always the next band up"),
                (Choice::Tolerance, "Round up within a tolerance in mph"),
                (Choice::Exact, "Exact conversion"),
                (Choice::Ask, "Ask each time"),
                (Choice::Table, "A table of limits"),
            ] {
                ui.radio_value(&mut c, v, label);
            }
            if c != before_c {
                rule = match c {
                    Choice::RoundUp => SpeedBandRule::default(),
                    Choice::NextBandUp => SpeedBandRule::RoundUp { tolerance: RoundUpTolerance::NextBandUp },
                    Choice::Tolerance => SpeedBandRule::RoundUp { tolerance: RoundUpTolerance::Mph(1.0) },
                    Choice::Exact => SpeedBandRule::Exact,
                    Choice::Ask => SpeedBandRule::AskEachTime,
                    Choice::Table => SpeedBandRule::Table(vec![(80.0, 50.0)]),
                };
                self.projset.table_text.clear();
            }
            if let SpeedBandRule::RoundUp { tolerance: RoundUpTolerance::Mph(x) } = &mut rule {
                ui.horizontal(|ui| {
                    ui.label("Tolerance (mph)");
                    ui.add(egui::DragValue::new(x).speed(0.1).range(0.0..=20.0));
                });
            }
            if let SpeedBandRule::Table(rows) = &mut rule {
                if self.projset.table_text.len() != rows.len() {
                    self.projset.table_text = rows.iter().map(|(a, b)| (a.to_string(), b.to_string())).collect();
                }
                let mut remove = None;
                egui::Grid::new("speed-table").num_columns(3).spacing([8.0, 4.0]).show(ui, |ui| {
                    ui.label(RichText::new("km/h limit").strong());
                    ui.label(RichText::new("mph to use").strong());
                    ui.label("");
                    ui.end_row();
                    for (i, row) in self.projset.table_text.iter_mut().enumerate() {
                        ui.add(egui::TextEdit::singleline(&mut row.0).desired_width(70.0));
                        ui.add(egui::TextEdit::singleline(&mut row.1).desired_width(70.0));
                        if ui.small_button("x").clicked() {
                            remove = Some(i);
                        }
                        ui.end_row();
                    }
                });
                if ui.button("Add a row").clicked() {
                    self.projset.table_text.push((String::new(), String::new()));
                }
                if let Some(i) = remove {
                    self.projset.table_text.remove(i);
                }
                *rows = self.projset.table_text.iter().filter_map(|(a, b)| Some((a.trim().parse::<f64>().ok()?, b.trim().parse::<f64>().ok()?))).collect();
                ui.label(RichText::new("A row is used once both numbers are valid.").small().weak());
            }
            ui.add_space(6.0);
            ui.group(|ui| {
                ui.label(RichText::new("What this rule means").strong());
                ui.label(definition(&rule));
            });
            ui.add_space(6.0);
            ui.label(RichText::new("Examples under this rule").strong());
            let tmp = {
                let mut d = Document::default();
                set_speed_band_rule(&mut d, &rule);
                d
            };
            egui::Grid::new("speed-examples").num_columns(3).spacing([10.0, 3.0]).show(ui, |ui| {
                ui.label(RichText::new("Check").strong());
                ui.label(RichText::new("Limit").strong());
                ui.label(RichText::new("Result").strong());
                ui.end_row();
                for (what, th) in [("Restraint (50 mph)", restraint_thresholds()), ("Visibility", visibility_thresholds()), ("Road markings", marking_thresholds())] {
                    for limit in [80.0, 100.0] {
                        let r = resolve(&tmp, limit, false, &th);
                        ui.label(what);
                        ui.label(format!("{limit} km/h"));
                        ui.label(RichText::new(&r.band_note).small());
                        ui.end_row();
                    }
                }
            });
            ui.add_space(4.0);
            ui.label(RichText::new("The thresholds belong to each check and are not part of the rule: restraint 50 mph; visibility 20, 30, 40, 50, 60 mph; markings 40, 50, 60, 70 mph.").small().weak());
        });
        if rule != before {
            set_speed_band_rule(&mut self.doc, &rule);
            self.schedule_commit_now();
            self.status = "Speed band rule changed. Run the checks again to see it in their calculation lines.".into();
        }
        self.projset.open = open;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_default_rule_treats_80_kmh_as_50_mph_and_each_check_has_its_own_thresholds() {
        let doc = Document::default();
        let r = resolve(&doc, 80.0, false, &restraint_thresholds());
        assert_eq!(r.mph_used, 50.0);
        assert!(r.band_note.contains("treats it as 50 mph"), "{}", r.band_note);
        // The visibility thresholds reach the same 50 mph; the marking thresholds do too.
        assert_eq!(resolve(&doc, 80.0, false, &visibility_thresholds()).mph_used, 50.0);
        assert_eq!(resolve(&doc, 80.0, false, &marking_thresholds()).mph_used, 50.0);
        // 100 km/h (62.14 mph) is above every restraint and visibility threshold.
        assert!((resolve(&doc, 100.0, false, &visibility_thresholds()).mph_used - 62.137).abs() < 0.01);
        // A limit in mph is used exactly.
        assert_eq!(resolve(&doc, 50.0, true, &restraint_thresholds()).mph_used, 50.0);
    }

    #[test]
    fn the_stored_rule_changes_the_result_and_ask_each_time_waits_for_an_answer() {
        let mut doc = Document::default();
        set_speed_band_rule(&mut doc, &SpeedBandRule::Exact);
        let r = resolve(&doc, 80.0, false, &restraint_thresholds());
        assert!((r.mph_used - 49.7096).abs() < 1e-3);
        assert!(r.band_note.contains("exact conversion"));
        set_speed_band_rule(&mut doc, &SpeedBandRule::Table(vec![(80.0, 45.0)]));
        assert_eq!(resolve(&doc, 80.0, false, &restraint_thresholds()).mph_used, 45.0);
        set_speed_band_rule(&mut doc, &SpeedBandRule::AskEachTime);
        let q = resolve_answered(&doc, 80.0, false, &restraint_thresholds(), None);
        assert!(q.ask.is_some());
        assert_eq!(resolve_answered(&doc, 80.0, false, &restraint_thresholds(), Some(true)).mph_used, 50.0);
        assert!((resolve_answered(&doc, 80.0, false, &restraint_thresholds(), Some(false)).mph_used - 49.7096).abs() < 1e-3);
        // The rule survives save and open.
        let back = Document::from_json(&doc.to_json()).unwrap();
        assert_eq!(speed_band_rule(&back), SpeedBandRule::AskEachTime);
        assert!(definition(&SpeedBandRule::default()).contains("80 km/h (49.71 mph) is treated as 50 mph"));
    }
}
