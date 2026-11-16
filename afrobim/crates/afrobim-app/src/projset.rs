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
use crate::ui_kit::{self, Col, Dialog, Footer, Level};

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
    /// 0 General, 1 Coordinate system, 2 Display units, 3 Speed limits.
    pub tab: usize,
    table_text: Vec<(String, String)>,
    crs: crate::crs_ui::CrsUi,
}

/// The index of the coordinate system tab.
pub const CRS_TAB: usize = 1;
/// The index of the display units tab.
pub const UNITS_TAB: usize = 2;

const TABS: [&str; 4] = ["General", "Coordinate system", "Display units", "Speed limits"];

impl AfroBimApp {
    /// PROJECTSETTINGS and UNITS: one dialog with the project's design standard, its display
    /// units and the speed band rule. Every change applies at once and is saved in the document.
    pub(crate) fn projset_window(&mut self, ctx: &egui::Context) {
        if !self.projset.open {
            return;
        }
        let mut st = afrobim_annotate::settings::load(&self.doc);
        let units_before = st.units.clone();
        let mut set_dims = self.anno.units_set_dims;
        let mut rule = speed_band_rule(&self.doc);
        let rule_before = rule.clone();
        let standard = self.standard.clone();
        let (elements, file) = (self.doc.elements().len(), self.title_for_pack());
        let mut tab = self.projset.tab;
        let mut std_pick: Option<usize> = None;
        let table_text = &mut self.projset.table_text;
        let crs_state = &mut self.projset.crs;
        let current_crs = afrobim_kernel::project_crs(&self.doc);
        let objects = crate::crs_ui::object_count(&self.doc);
        let mut crs_out = crate::crs_ui::CrsTabOut::default();
        let out = Dialog::new("Project settings", crate::glyphs::by_name("PROJECTSETTINGS"))
            .subtitle(format!("{file}  ·  {elements} element(s)"))
            .width(600.0)
            .help("These settings belong to the project and are saved in the document. Display units change how numbers are shown and typed, never what is stored (stored values stay metric).")
            .show(
                ctx,
                |ui| {
                    ui_kit::tabs(ui, &mut tab, &TABS);
                    match tab {
                        0 => general_tab(ui, &standard, &mut std_pick),
                        CRS_TAB => crs_out = crate::crs_ui::tab(ui, crs_state, &current_crs, objects),
                        UNITS_TAB => units_tab(ui, &mut st, &mut set_dims),
                        _ => speed_tab(ui, &mut rule, table_text),
                    }
                },
                |ui| Footer::new("Done").cancel(None).note("Changes apply at once and are saved in the document").show(ui),
            );
        self.projset.tab = tab;
        self.anno.units_set_dims = set_dims;
        if let Some(crs) = crs_out.set {
            self.apply_project_crs(crs);
        } else if crs_out.clear {
            self.remove_project_crs();
        }
        if let Some(i) = std_pick {
            self.run(ctx, crate::commands::Action::Standard(i as u8));
        }
        if st.units != units_before {
            if set_dims {
                for d in &mut st.dim_styles {
                    d.length_unit = st.units.length;
                    d.length_decimals = st.units.length_decimals;
                    d.angle_format = st.units.angle;
                    d.angle_decimals = st.units.angle_decimals;
                    d.station_decimals = st.units.station_decimals;
                }
            }
            afrobim_annotate::settings::store(&mut self.doc, &st);
            if set_dims {
                afrobim_annotate::settings::restyle(&mut self.doc);
            }
            self.sync_generated();
            self.schedule_commit_now();
        }
        if rule != rule_before {
            set_speed_band_rule(&mut self.doc, &rule);
            self.schedule_commit_now();
            self.status = "Speed band rule changed. Run the checks again to see it in their calculation lines.".into();
        }
        if out.closed || out.inner.is_some_and(|(_, f)| f.primary || f.cancel) {
            self.projset.open = false;
        }
    }
}

fn general_tab(ui: &mut egui::Ui, standard: &str, pick: &mut Option<usize>) {
    ui_kit::section(ui, "Design standard", |ui| {
        let labels: Vec<String> = crate::signs_glue::STANDARDS.iter().map(|(id, label, _)| format!("{label}  ({id})")).collect();
        let refs: Vec<&str> = labels.iter().map(String::as_str).collect();
        let mut i = crate::signs_glue::STANDARDS.iter().position(|(id, _, _)| *id == standard).unwrap_or(0);
        if ui_kit::radio_group(ui, &mut i, &refs) {
            *pick = Some(i);
        }
        ui.add_space(4.0);
        ui_kit::callout(ui, Level::Info, "The standard chooses the sign templates, the library and the rules the sign checks use. It is stored in the document.");
    });
    ui_kit::section(ui, "Other project data", |ui| {
        ui.label("The project's title-block information (project, client, contract number, revisions) is edited in Project information.");
    });
}

fn units_tab(ui: &mut egui::Ui, st: &mut afrobim_annotate::settings::AnnotationSettings, set_dims: &mut bool) {
    use afrobim_annotate as ann;
    ui_kit::section(ui, "Lengths and stations", |ui| {
        let u = &mut st.units;
        ui_kit::form(ui, "units-length", |f| {
            f.row("Length", |ui| {
                let mut i = if u.length == ann::LengthUnit::Millimetre { 0 } else { 1 };
                if ui_kit::segmented(ui, &mut i, &["Millimetres", "Metres"]) {
                    u.length = if i == 0 { ann::LengthUnit::Millimetre } else { ann::LengthUnit::Metre };
                }
            });
            f.row("Length decimals", |ui| {
                ui_kit::decimals_field(ui, &mut u.length_decimals, 0..=6);
            });
            f.row_note("Station decimals", "1+250.500 has three", |ui| {
                ui_kit::decimals_field(ui, &mut u.station_decimals, 0..=6);
            });
        });
    });
    ui_kit::section(ui, "Areas, volumes and angles", |ui| {
        let u = &mut st.units;
        ui_kit::form(ui, "units-other", |f| {
            f.row("Area", |ui| {
                let mut i = if u.area == ann::AreaUnit::SquareMetre { 0 } else { 1 };
                if ui_kit::segmented(ui, &mut i, &["Square metres", "Hectares"]) {
                    u.area = if i == 0 { ann::AreaUnit::SquareMetre } else { ann::AreaUnit::Hectare };
                }
            });
            f.row("Area decimals", |ui| {
                ui_kit::decimals_field(ui, &mut u.area_decimals, 0..=6);
            });
            f.row_note("Volume decimals", "volumes are always cubic metres", |ui| {
                ui_kit::decimals_field(ui, &mut u.volume_decimals, 0..=6);
            });
            f.row("Angle", |ui| {
                let formats = [ann::AngleFormat::Decimal, ann::AngleFormat::Dms, ann::AngleFormat::QuadrantBearing, ann::AngleFormat::Azimuth];
                let labels: Vec<&str> = formats.iter().map(|f| crate::anno_ui::angle_label(*f)).collect();
                let mut i = formats.iter().position(|f| *f == u.angle).unwrap_or(0);
                if ui_kit::combo(ui, "angle-format", &mut i, &labels) {
                    u.angle = formats[i];
                }
            });
            f.row("Angle decimals", |ui| {
                ui_kit::decimals_field(ui, &mut u.angle_decimals, 0..=6);
            });
        });
        ui.add_space(4.0);
        ui_kit::toggle(ui, set_dims, "Also set the dimension styles to these units");
    });
    ui_kit::section(ui, "How they read", |ui| {
        let u = &st.units;
        let rows = vec![
            vec!["Length".to_string(), "12 345.6789 mm".to_string(), u.length_u(12_345.6789)],
            vec!["Station".to_string(), "1250.5 m".to_string(), u.station(1250.5)],
            vec!["Area".to_string(), "1 234 567 890 mm2".to_string(), u.area_u(1_234_567_890.0)],
            vec!["Volume".to_string(), "2 500 000 000 mm3".to_string(), format!("{} m3", u.volume(2_500_000_000.0))],
            vec!["Angle".to_string(), "37.5 degrees".to_string(), u.angle(37.5)],
            vec!["Direction".to_string(), "37.5 degrees".to_string(), u.direction(37.5)],
        ];
        ui_kit::results_table(ui, "units-examples", &["", "Stored", "Shown"], &[Col::Text, Col::Num, Col::Num], &rows);
        ui.add_space(4.0);
        crate::ui_kit::note(ui, "Typed lengths, stations, areas and angles are read in these formats (12.5, 12.5m, 12500mm, 1+250.500, 45d30m15s). Applied to the status bar coordinates, Properties position and size, measure results, dimension text, the typed point input of annotation commands and the station fields.");
    });
}

fn speed_tab(ui: &mut egui::Ui, rule: &mut SpeedBandRule, table_text: &mut Vec<(String, String)>) {
    ui_kit::section(ui, "Speed limits in km/h against UK thresholds in mph", |ui| {
        crate::ui_kit::note(ui, "Used by the restraint check, sign visibility and the road marking choice. The rule is your decision for the project.");
        ui.add_space(4.0);
        let labels = ["Round up within conversion rounding (default)", "Always the next band up", "Round up within a tolerance in mph", "Exact conversion", "Ask each time", "A table of limits"];
        let order = [Choice::RoundUp, Choice::NextBandUp, Choice::Tolerance, Choice::Exact, Choice::Ask, Choice::Table];
        let mut i = order.iter().position(|c| *c == choice_of(rule)).unwrap_or(0);
        if ui_kit::radio_group(ui, &mut i, &labels) {
            *rule = match order[i] {
                Choice::RoundUp => SpeedBandRule::default(),
                Choice::NextBandUp => SpeedBandRule::RoundUp { tolerance: RoundUpTolerance::NextBandUp },
                Choice::Tolerance => SpeedBandRule::RoundUp { tolerance: RoundUpTolerance::Mph(1.0) },
                Choice::Exact => SpeedBandRule::Exact,
                Choice::Ask => SpeedBandRule::AskEachTime,
                Choice::Table => SpeedBandRule::Table(vec![(80.0, 50.0)]),
            };
            table_text.clear();
        }
        if let SpeedBandRule::RoundUp { tolerance: RoundUpTolerance::Mph(x) } = rule {
            ui.add_space(4.0);
            ui_kit::form(ui, "speed-tol", |f| {
                f.row("Tolerance", |ui| {
                    ui_kit::f64_field(ui, x, "mph", 0.0..=20.0, 0.1);
                });
            });
        }
        if let SpeedBandRule::Table(rows) = rule {
            if table_text.len() != rows.len() {
                *table_text = rows.iter().map(|(a, b)| (a.to_string(), b.to_string())).collect();
            }
            let mut remove = None;
            ui.add_space(4.0);
            egui::Grid::new("speed-table").num_columns(3).spacing([8.0, 4.0]).show(ui, |ui| {
                ui.label(RichText::new("km/h limit").small().strong());
                ui.label(RichText::new("mph to use").small().strong());
                ui.label("");
                ui.end_row();
                for (i, row) in table_text.iter_mut().enumerate() {
                    ui.add(egui::TextEdit::singleline(&mut row.0).desired_width(70.0));
                    ui.add(egui::TextEdit::singleline(&mut row.1).desired_width(70.0));
                    if ui_kit::icon_button(ui, crate::glyphs::ui::TRASH, "Remove this row").clicked() {
                        remove = Some(i);
                    }
                    ui.end_row();
                }
            });
            if ui.button(crate::theme::icon_text(crate::glyphs::ui::PLUS, "Add a row", crate::theme::FONT_BODY, crate::theme::tokens_ui(ui).text)).clicked() {
                table_text.push((String::new(), String::new()));
            }
            if let Some(i) = remove {
                table_text.remove(i);
            }
            *rows = table_text.iter().filter_map(|(a, b)| Some((a.trim().parse::<f64>().ok()?, b.trim().parse::<f64>().ok()?))).collect();
            crate::ui_kit::note(ui, "A row is used once both numbers are valid.");
        }
        ui.add_space(6.0);
        ui_kit::callout(ui, Level::Info, definition(rule));
    });
    ui_kit::section(ui, "Examples under this rule", |ui| {
        let tmp = {
            let mut d = Document::default();
            set_speed_band_rule(&mut d, rule);
            d
        };
        let mut rows = Vec::new();
        for (what, th) in [("Restraint (50 mph)", restraint_thresholds()), ("Visibility", visibility_thresholds()), ("Road markings", marking_thresholds())] {
            for limit in [80.0, 100.0] {
                let r = resolve(&tmp, limit, false, &th);
                rows.push(vec![what.to_string(), format!("{limit} km/h"), r.band_note.clone()]);
            }
        }
        ui_kit::results_table(ui, "speed-examples", &["Check", "Limit", "Result"], &[Col::Text, Col::Num, Col::Text], &rows);
        ui.add_space(4.0);
        crate::ui_kit::note(ui, "The thresholds belong to each check and are not part of the rule: restraint 50 mph; visibility 20, 30, 40, 50, 60 mph; markings 40, 50, 60, 70 mph.");
    });
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
