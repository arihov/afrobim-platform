//! The survey database in the application, part two: the observation editor with the face 1 and
//! face 2 reduction spreads, traverse analysis, the network adjustment with its results, applying
//! adjusted coordinates, figures, and the link of survey points to the drawing.

use afrobim_survey::*;
use std::result::Result;
use egui::RichText;

use crate::app::AfroBimApp;
use crate::background::Job;
use crate::sdb_ui::{fmt_opt, net_combo, parse_f};
use crate::survey_db as sd;
use crate::ui_kit::{self, Col, Dialog, Footer, Level};

// ─── observation editor ─────────────────────────────────────────────────

#[derive(Clone, Default)]
struct ObsRow {
    id: u64,
    target: String,
    face: usize,
    kind: usize,
    hz: String,
    zen: String,
    sd: String,
    hd: String,
    vd: String,
    th: String,
    desc: String,
    disabled: bool,
}

#[derive(Default)]
pub struct ObsForm {
    net: usize,
    setup: usize,
    loaded: Option<(usize, usize)>,
    rows: Vec<ObsRow>,
    station: String,
    ih: String,
    bs_point: String,
    bs_az: String,
    bs_read: String,
    equip: usize,
    message: String,
}

fn t(v: Option<f64>) -> String {
    v.map(|x| format!("{x}")).unwrap_or_default()
}

impl ObsForm {
    pub fn new(_db: &SurveyDatabase, net: usize) -> ObsForm {
        ObsForm { net, ..Default::default() }
    }

    fn load(&mut self, db: &SurveyDatabase) {
        self.loaded = Some((self.net, self.setup));
        self.message.clear();
        let Some(s) = db.networks.get(self.net).and_then(|n| n.setups.get(self.setup)) else {
            self.rows.clear();
            return;
        };
        self.station = s.station.clone();
        self.ih = format!("{}", s.instrument_height);
        self.bs_point = s.backsight_point.clone().unwrap_or_default();
        self.bs_az = t(s.backsight_azimuth_deg);
        self.bs_read = format!("{}", s.backsight_reading_deg);
        self.equip = s.equipment.as_ref().and_then(|e| db.equipment.iter().position(|x| &x.name == e)).map(|i| i + 1).unwrap_or(0);
        self.rows = s
            .observations
            .iter()
            .map(|o| ObsRow {
                id: o.id,
                target: o.target.clone(),
                face: match o.face {
                    Face::Single => 0,
                    Face::Face1 => 1,
                    Face::Face2 => 2,
                },
                kind: usize::from(o.hz_kind == HzKind::Angle),
                hz: t(o.hz_deg),
                zen: t(o.zenith_deg),
                sd: t(o.slope_distance),
                hd: t(o.horizontal_distance),
                vd: t(o.vertical_distance),
                th: format!("{}", o.target_height),
                desc: o.description.clone(),
                disabled: o.disabled,
            })
            .collect();
    }

    /// Writes the edited rows into the setup.
    fn apply(&self, db: &mut SurveyDatabase) -> Result<(), String> {
        let opt = |s: &str, what: &str| -> Result<Option<f64>, String> {
            if s.trim().is_empty() {
                Ok(None)
            } else {
                parse_f(s).map(Some).ok_or_else(|| format!("{what} '{s}' is not a number."))
            }
        };
        let ih = parse_f(&self.ih).ok_or("The instrument height is not a number.")?;
        let read = parse_f(&self.bs_read).ok_or("The backsight reading is not a number.")?;
        let az = opt(&self.bs_az, "The backsight azimuth")?;
        let equipment = (self.equip > 0).then(|| db.equipment.get(self.equip - 1).map(|e| e.name.clone())).flatten();
        let mut obs = Vec::new();
        for (i, r) in self.rows.iter().enumerate() {
            let w = |what: &str| format!("Row {}: {what}", i + 1);
            let mut o = Observation::new(r.target.trim());
            o.id = r.id;
            o.face = [Face::Single, Face::Face1, Face::Face2][r.face.min(2)];
            o.hz_kind = if r.kind == 1 { HzKind::Angle } else { HzKind::Direction };
            o.hz_deg = opt(&r.hz, &w("the horizontal reading"))?;
            o.zenith_deg = opt(&r.zen, &w("the zenith angle"))?;
            o.slope_distance = opt(&r.sd, &w("the slope distance"))?;
            o.horizontal_distance = opt(&r.hd, &w("the horizontal distance"))?;
            o.vertical_distance = opt(&r.vd, &w("the vertical distance"))?;
            o.target_height = parse_f(&r.th).ok_or_else(|| w("the target height is not a number."))?;
            o.description = r.desc.clone();
            o.disabled = r.disabled;
            if o.target.is_empty() {
                return Err(w("name the target."));
            }
            obs.push(o);
        }
        let net = db.networks.get_mut(self.net).ok_or("No such network.")?;
        let s = net.setups.get_mut(self.setup).ok_or("No such setup.")?;
        s.station = self.station.trim().to_string();
        s.instrument_height = ih;
        s.backsight_point = (!self.bs_point.trim().is_empty()).then(|| self.bs_point.trim().to_string());
        s.backsight_azimuth_deg = az;
        s.backsight_reading_deg = read;
        s.equipment = equipment;
        s.observations = obs;
        Ok(())
    }
}

impl AfroBimApp {
    pub(crate) fn sdb_obs_window(&mut self, ctx: &egui::Context) {
        let Some(mut f) = self.civil_ui.sdb.obs.take() else { return };
        let Some(db) = self.civil_ui.sdb.db.clone() else { return };
        if f.loaded != Some((f.net, f.setup)) {
            f.load(&db);
        }
        let mut apply = false;
        let reduced = db.networks.get(f.net).and_then(|n| n.setups.get(f.setup).map(|s| reduce_setup(&db, n, s)));
        let out = Dialog::new("Observations", crate::glyphs::by_name("SURVEYOBS")).width(1020.0).resizable(true).max_body_height(640.0).help("The readings of one setup. Angles are in decimal degrees, distances in metres. A target observed on face 1 and face 2 is reduced to face 1, the rows are averaged and the spread is shown below. A disabled row stays in the database but is left out of the reductions and the adjustment. Edits are written when you press Apply.").show(
            ctx,
            |ui| {
                ui.horizontal(|ui| {
                    ui.label("Network");
                    net_combo(ui, "sdbo-net", &db, &mut f.net);
                    let n = db.networks.get(f.net).map(|n| n.setups.len()).unwrap_or(0);
                    if n == 0 {
                        ui.label(RichText::new("no setups").weak());
                    } else {
                        f.setup = f.setup.min(n - 1);
                        let names: Vec<String> = db.networks[f.net].setups.iter().map(|s| format!("{} ({})", s.station, s.observations.len())).collect();
                        ui.label("Setup");
                        ui_kit::combo_owned(ui, "sdbo-set", &mut f.setup, &names);
                    }
                });
                if f.rows.is_empty() && db.networks.get(f.net).is_none_or(|n| n.setups.is_empty()) {
                    return;
                }
                ui_kit::form(ui, "sdbo-h", |fm| {
                    fm.row("Station", |ui| {
                        ui_kit::text_field(ui, &mut f.station, "");
                    });
                    fm.row("Instrument height", |ui| {
                        crate::civil_dlg::num(ui, &mut f.ih, "m");
                    });
                    fm.row("Backsight point", |ui| {
                        ui_kit::text_field(ui, &mut f.bs_point, "name of a point with coordinates");
                    });
                    fm.row_note("Backsight azimuth", "when there is no backsight point", |ui| {
                        crate::civil_dlg::num_opt(ui, &mut f.bs_az, "deg").ok();
                    });
                    fm.row("Reading at the backsight", |ui| {
                        crate::civil_dlg::num(ui, &mut f.bs_read, "deg");
                    });
                    fm.row("Equipment", |ui| {
                        let mut items: Vec<String> = vec!["(none)".into()];
                        items.extend(db.equipment.iter().map(|e| e.name.clone()));
                        f.equip = f.equip.min(items.len() - 1);
                        ui_kit::combo_owned(ui, "sdbo-eq", &mut f.equip, &items);
                    });
                });
                ui_kit::scroll_x(ui, "sdbo-t", |ui| {
                    egui::Grid::new("sdbo-grid").num_columns(12).spacing([4.0, 3.0]).striped(true).show(ui, |ui| {
                        for h in ["Target", "Face", "Reading", "Hz (deg)", "Zenith", "Slope dist", "Horiz dist", "Vert dist", "Target ht", "Description", "Off", ""] {
                            ui_kit::th(ui, h);
                        }
                        ui.end_row();
                        let mut remove = None;
                        for (i, r) in f.rows.iter_mut().enumerate() {
                            ui.add(egui::TextEdit::singleline(&mut r.target).desired_width(60.0));
                            ui_kit::combo(ui, &format!("sdbo-face{i}"), &mut r.face, &["single", "face 1", "face 2"]);
                            ui_kit::combo(ui, &format!("sdbo-kind{i}"), &mut r.kind, &["direction", "angle"]);
                            for (cell, w) in [(&mut r.hz, 84.0), (&mut r.zen, 70.0), (&mut r.sd, 76.0), (&mut r.hd, 76.0), (&mut r.vd, 70.0), (&mut r.th, 50.0)] {
                                ui.add(egui::TextEdit::singleline(cell).desired_width(w).font(egui::TextStyle::Monospace));
                            }
                            ui.add(egui::TextEdit::singleline(&mut r.desc).desired_width(90.0));
                            ui.checkbox(&mut r.disabled, "");
                            if ui_kit::icon_button(ui, crate::glyphs::ui::TRASH, "Remove this reading").clicked() {
                                remove = Some(i);
                            }
                            ui.end_row();
                        }
                        if let Some(i) = remove {
                            f.rows.remove(i);
                        }
                    });
                });
                if ui.button("Add a reading").clicked() {
                    f.rows.push(ObsRow { th: "0".into(), ..Default::default() });
                }
                ui_kit::section(ui, "Reduction: face 1 and face 2 spreads", |ui| match &reduced {
                    Some(Ok(sets)) => {
                        let sp = |s: &Option<Spread>| s.map(|s| format!("{} rows, range {:.5}{}", s.n, s.range, s.std_dev.map(|d| format!(", s {d:.5}")).unwrap_or_default())).unwrap_or_else(|| "-".into());
                        let rows: Vec<Vec<String>> = sets
                            .iter()
                            .map(|s| vec![s.target.clone(), fmt_opt(s.hz_deg, 5), sp(&s.hz_spread), fmt_opt(s.hz_face_difference_deg, 5), fmt_opt(s.zenith_deg, 5), sp(&s.zenith_spread), fmt_opt(s.index_error_deg, 5), fmt_opt(s.slope_distance, 4), sp(&s.distance_spread), fmt_opt(s.horizontal_distance, 4), fmt_opt(s.height_difference, 4)])
                            .collect();
                        ui_kit::scroll_x(ui, "sdbo-r", |ui| {
                            ui_kit::results_table(ui, "sdbo-red", &["Target", "Hz", "Hz spread", "Face diff", "Zenith", "Zenith spread", "Index err", "Slope", "Distance spread", "Horiz", "dH"], &[Col::Text, Col::Num, Col::Text, Col::Num, Col::Num, Col::Text, Col::Num, Col::Num, Col::Text, Col::Num, Col::Num], &rows);
                        });
                        ui_kit::note(ui, "Angles in degrees, distances in metres. The spread of the rows is the range and the sample standard deviation; the face difference and the index error are shown when both faces were read.");
                    }
                    Some(Err(e)) => {
                        ui_kit::callout(ui, Level::Warning, e.to_string());
                    }
                    None => {}
                });
                if !f.message.is_empty() {
                    ui_kit::callout(ui, Level::Error, f.message.clone());
                }
            },
            |ui| {
                let o = Footer::new("Apply edits").icon(crate::glyphs::by_name("SURVEYOBS")).cancel(Some("Close")).show(ui);
                apply = o.primary;
                o
            },
        );
        if out.closed || out.inner.as_ref().is_some_and(|(_, fo)| fo.cancel) {
            self.civil_ui.sdb.net = f.net;
            return;
        }
        if apply {
            if let Some(db) = &mut self.civil_ui.sdb.db {
                let mut work = db.clone();
                match f.apply(&mut work) {
                    Ok(()) => {
                        work.log(&crate::transform_doc::now_iso(), "edit observations", &format!("setup {}", f.station));
                        *db = work;
                        f.message.clear();
                        self.status = format!("Saved the readings of the setup at {}.", f.station);
                    }
                    Err(e) => f.message = e,
                }
            }
            self.sdb_save();
        }
        self.civil_ui.sdb.obs = Some(f);
    }
}

// ─── traverse ───────────────────────────────────────────────────────────

#[derive(Default)]
pub struct TravForm {
    net: usize,
    stations: String,
    closure: usize,
    final_bearing: String,
    adjust_angles: bool,
    method: usize,
    report: Option<Result<TraverseReport, String>>,
    text: String,
}

impl TravForm {
    pub fn new(db: &SurveyDatabase, net: usize) -> TravForm {
        // Start from the order the setups were made in.
        let stations: Vec<String> = db.networks.get(net).map(|n| n.setups.iter().map(|s| s.station.clone()).collect()).unwrap_or_default();
        TravForm { net, stations: stations.join(", "), adjust_angles: false, ..Default::default() }
    }

    fn spec(&self, db: &SurveyDatabase) -> Result<TraverseSpec, String> {
        let name = db.networks.get(self.net).map(|n| n.name.clone()).ok_or("There is no network.")?;
        let stations: Vec<String> = self.stations.split([',', ' ', ';']).map(str::trim).filter(|s| !s.is_empty()).map(str::to_string).collect();
        let known = if self.final_bearing.trim().is_empty() { None } else { Some(parse_f(&self.final_bearing).ok_or("The known final bearing is not a number.")?) };
        Ok(TraverseSpec { network: name, stations, closure: [TraverseClosure::Open, TraverseClosure::Loop, TraverseClosure::ToKnownPoint][self.closure.min(2)].clone(), known_final_bearing_deg: known, adjust_angles: self.adjust_angles })
    }
}

fn traverse_text(r: &TraverseReport) -> String {
    use std::fmt::Write;
    let mut s = String::from("Traverse report\n\nCourses (bearing from the readings, distance reduced)\n");
    for (i, c) in r.courses.iter().enumerate() {
        let _ = writeln!(s, "  {:>2}  bearing {:>12.6} deg   distance {:>12.4} m", i + 1, c.bearing_deg, c.distance);
    }
    if !r.angles_deg.is_empty() {
        let _ = writeln!(s, "\nTurned angles (deg): {}", r.angles_deg.iter().map(|a| format!("{a:.6}")).collect::<Vec<_>>().join(", "));
    }
    if let Some(a) = r.angular_misclosure_deg {
        let _ = writeln!(s, "Angular misclosure {a:.6} deg ({:.1} arc-seconds)", a * 3600.0);
    }
    if let Some(m) = &r.misclosure {
        let _ = writeln!(s, "\nLinear misclosure {:.4} m (dE {:.4}, dN {:.4}) over a perimeter of {:.3} m; relative precision {}", m.linear, m.error_e, m.error_n, m.perimeter, m.precision_denominator.map(|n| format!("1 in {n:.0}")).unwrap_or_else(|| "exact".into()));
    }
    for (label, adj) in [("Bowditch (compass rule)", &r.bowditch), ("Transit rule", &r.transit)] {
        if let Some(a) = adj {
            let _ = writeln!(s, "\n{label}: adjusted points");
            for (i, p) in a.points.iter().enumerate() {
                let _ = writeln!(s, "  {:>2}  E {:>14.4}  N {:>14.4}", i + 1, p.e, p.n);
            }
        }
    }
    s
}

impl AfroBimApp {
    pub(crate) fn sdb_trav_window(&mut self, ctx: &egui::Context) {
        let Some(mut f) = self.civil_ui.sdb.trav.take() else { return };
        let Some(db) = self.civil_ui.sdb.db.clone() else { return };
        let mut run = false;
        let mut save = false;
        let mut to_adjust = false;
        let out = Dialog::new("Traverse analysis", crate::glyphs::by_name("SURVEYTRAVERSE")).width(760.0).resizable(true).max_body_height(620.0).help("Choose the stations in the order they were occupied. The courses come from the readings of the setups (the distance from either direction, the bearings chained from the turned angles). The closure is reported and adjusted by the compass (Bowditch) or the transit rule; the least squares adjustment of the same observations is the network adjustment.").show(
            ctx,
            |ui| {
                ui_kit::form(ui, "sdbt", |fm| {
                    fm.row("Network", |ui| {
                        net_combo(ui, "sdbt-net", &db, &mut f.net);
                    });
                    fm.row_note("Stations, in order", "names separated by commas; repeat the first to close a loop", |ui| {
                        ui_kit::text_field_wide(ui, &mut f.stations, "A, P1, P2, P3, P4, A");
                    });
                    fm.row("Closure", |ui| {
                        ui_kit::combo(ui, "sdbt-cl", &mut f.closure, &["Open (no check)", "Loop (back to the first station)", "To a known point"]);
                    });
                    fm.row_note("Known final bearing", "optional: checks the angles; degrees", |ui| {
                        crate::civil_dlg::num_opt(ui, &mut f.final_bearing, "deg").ok();
                    });
                    fm.row("Distribute the angular misclosure", |ui| {
                        ui.checkbox(&mut f.adjust_angles, "");
                    });
                });
                ui.horizontal(|ui| {
                    if ui.button("Run the traverse").clicked() {
                        run = true;
                    }
                    if ui.button("Least squares: open the network adjustment").clicked() {
                        to_adjust = true;
                    }
                });
                match &f.report {
                    None => {}
                    Some(Err(e)) => {
                        ui_kit::callout(ui, Level::Error, e.clone());
                    }
                    Some(Ok(r)) => {
                        ui_kit::section(ui, "Result", |ui| {
                            ui_kit::segmented(ui, &mut f.method, &["Unadjusted", "Bowditch", "Transit"]);
                            let rows: Vec<Vec<String>> = r.courses.iter().enumerate().map(|(i, c)| vec![(i + 1).to_string(), format!("{:.6}", c.bearing_deg), format!("{:.4}", c.distance)]).collect();
                            ui_kit::results_table(ui, "sdbt-c", &["Course", "Bearing (deg)", "Distance (m)"], &[Col::Num, Col::Num, Col::Num], &rows);
                            if let Some(m) = &r.misclosure {
                                ui_kit::callout(ui, Level::Info, format!("Linear misclosure {:.4} m over {:.3} m: {}. Judge it against the requirement of the project: no limit is supplied here.", m.linear, m.perimeter, m.precision_denominator.map(|n| format!("1 in {n:.0}")).unwrap_or_else(|| "exact".into())));
                            }
                            if let Some(a) = r.angular_misclosure_deg {
                                ui.label(format!("Angular misclosure {a:.6} deg ({:.1} arc-seconds)", a * 3600.0));
                            }
                            let pts: Vec<(f64, f64)> = match f.method {
                                1 => r.bowditch.as_ref().map(|a| a.points.iter().map(|p| (p.e, p.n)).collect()),
                                2 => r.transit.as_ref().map(|a| a.points.iter().map(|p| (p.e, p.n)).collect()),
                                _ => Some(r.unadjusted_points.iter().map(|p| (p.e, p.n)).collect()),
                            }
                            .unwrap_or_default();
                            if pts.is_empty() && f.method > 0 {
                                ui_kit::note(ui, "This method needs a closing traverse (a loop or a known end point).");
                            }
                            let rows: Vec<Vec<String>> = pts.iter().enumerate().map(|(i, p)| vec![(i + 1).to_string(), format!("{:.4}", p.0), format!("{:.4}", p.1)]).collect();
                            ui_kit::results_table(ui, "sdbt-p", &["Point", "Easting", "Northing"], &[Col::Num, Col::Num, Col::Num], &rows);
                            if ui.button("Save the report...").clicked() {
                                save = true;
                            }
                        });
                    }
                }
            },
            |ui| Footer::new("Close").cancel(None).show(ui),
        );
        if out.closed || out.inner.as_ref().is_some_and(|(_, fo)| fo.primary) {
            self.civil_ui.sdb.net = f.net;
            return;
        }
        if run {
            f.report = Some(f.spec(&db).and_then(|s| traverse_from_setups(&db, &s).map_err(|e| e.to_string())));
            if let Some(Ok(r)) = &f.report {
                f.text = traverse_text(r);
            }
        }
        if save {
            if let Some(p) = rfd::FileDialog::new().set_file_name("traverse-report.txt").save_file() {
                match std::fs::write(&p, &f.text) {
                    Ok(()) => self.status = format!("Saved the traverse report to {}.", p.display()),
                    Err(e) => self.status = format!("The report could not be saved: {e}"),
                }
            }
        }
        if to_adjust {
            self.civil_ui.sdb.net = f.net;
            self.civil_run_pending = Some(crate::civil_cmds::Cv::SurveyAdjust);
        }
        self.civil_ui.sdb.trav = Some(f);
    }
}

// ─── network adjustment ─────────────────────────────────────────────────

pub struct AdjustForm {
    net: usize,
    tab: usize,
    mode: usize,
    max_iter: String,
    convergence: String,
    ellipse_conf: String,
    alpha: String,
    blunder: String,
    scale_post: bool,
    centring: bool,
    equip: usize,
    excluded: String,
    snoop: bool,
    snoop_max: String,
    ell_scale: String,
    rel_a: usize,
    rel_b: usize,
    message: String,
    example: bool,
}

impl AdjustForm {
    pub fn new(db: &SurveyDatabase, net: usize, example: bool) -> AdjustForm {
        let _ = db;
        let mut f = AdjustForm { net, tab: usize::from(example), mode: 0, max_iter: String::new(), convergence: String::new(), ellipse_conf: String::new(), alpha: String::new(), blunder: String::new(), scale_post: false, centring: true, equip: 0, excluded: String::new(), snoop: false, snoop_max: String::new(), ell_scale: String::new(), rel_a: 0, rel_b: 1, message: String::new(), example };
        if example {
            // The demonstration fills EXAMPLE values so that the results can be seen. They are
            // marked as such in the dialog and are not recommendations.
            f.max_iter = "20".into();
            f.convergence = "1e-10".into();
            f.ellipse_conf = "0.95".into();
            f.alpha = "0.05".into();
            f.blunder = format!("{:.4}", normal_two_sided_critical(0.05));
            f.snoop_max = "3".into();
            f.ell_scale = "50".into();
        }
        f
    }

    fn options(&self, db: &SurveyDatabase) -> Result<AdjustOptions, String> {
        let need = |what: &str, t: &str| parse_f(t).ok_or_else(|| format!("{what} is an input: type it."));
        let it = need("The most iterations", &self.max_iter)?;
        if it < 1.0 {
            return Err("The most iterations must be at least 1.".into());
        }
        let conv = need("The convergence criterion", &self.convergence)?;
        let conf = need("The confidence level of the error ellipses", &self.ellipse_conf)?;
        if !(conf > 0.0 && conf < 1.0) {
            return Err("The confidence level is between 0 and 1.".into());
        }
        let alpha = need("The significance level of the global test", &self.alpha)?;
        if !(alpha > 0.0 && alpha < 1.0) {
            return Err("The significance level is between 0 and 1.".into());
        }
        let blunder = need("The blunder threshold", &self.blunder)?;
        let mut o = AdjustOptions::new(it as usize, conv, conf, alpha, blunder);
        o.scale_by_posteriori = self.scale_post;
        o.include_centring = self.centring;
        o.default_equipment = (self.equip > 0).then(|| db.equipment.get(self.equip - 1).map(|e| e.name.clone())).flatten();
        o.excluded = self.excluded.split(',').map(str::trim).filter(|s| !s.is_empty()).map(str::to_string).collect();
        Ok(o)
    }
}

fn ellipse_ring(centre: (f64, f64), el: &Ellipse, scale: f64) -> Vec<[f64; 2]> {
    let az = el.azimuth_major_deg.to_radians();
    let (u, v) = ((az.sin(), az.cos()), (az.cos(), -az.sin()));
    (0..=48)
        .map(|k| {
            let t = k as f64 / 48.0 * std::f64::consts::TAU;
            let (a, b) = (el.semi_major * scale * t.cos(), el.semi_minor * scale * t.sin());
            [(centre.0 + a * u.0 + b * v.0) * 1000.0, (centre.1 + a * u.1 + b * v.1) * 1000.0]
        })
        .collect()
}

impl AfroBimApp {
    pub(crate) fn sdb_adjust_window(&mut self, ctx: &egui::Context) {
        let Some(mut f) = self.civil_ui.sdb.adjust.take() else { return };
        let Some(db) = self.civil_ui.sdb.db.clone() else { return };
        let result = self.civil_ui.sdb.result.clone();
        let running = self.bg.survey.is_some();
        let mut run = false;
        let mut draw = false;
        let mut clear = false;
        let mut save = false;
        let mut compute_blunder = false;
        let mut apply = false;
        let tabs = ["Options", "Coordinates", "Residuals", "Statistics", "Ellipses", "Relative precision", "Report"];
        let out = Dialog::new("Network adjustment", crate::glyphs::by_name("SURVEYADJUST")).width(900.0).resizable(true).max_body_height(640.0).help("Least squares adjustment of the network: 2D (plan, from the directions, distances, azimuths, baselines and weighted control) or levelling. Every option is an input. The results are adjusted coordinates, residuals with their standardised residuals and flags, the variance factor and the global test, error ellipses and the precision between two points. The adjustment runs in the background and can be cancelled.").show(
            ctx,
            |ui| {
                ui_kit::tabs(ui, &mut f.tab, &tabs);
                match f.tab {
                    0 => {
                        if f.example {
                            ui_kit::callout(ui, Level::Info, "EXAMPLE VALUES for the demonstration: filled in so that the results can be seen. They are not recommendations; type your own.");
                        }
                        ui_kit::form(ui, "sdba-o", |fm| {
                            fm.row("Network", |ui| {
                                net_combo(ui, "sdba-net", &db, &mut f.net);
                            });
                            fm.row("Adjustment", |ui| {
                                ui_kit::segmented(ui, &mut f.mode, &["2D (plan)", "Levelling (heights)"]);
                            });
                            fm.row("Most iterations", |ui| {
                                crate::civil_dlg::num(ui, &mut f.max_iter, "");
                            });
                            fm.row_note("Convergence criterion", "stop when the squared step norm is at most this", |ui| {
                                crate::civil_dlg::num(ui, &mut f.convergence, "");
                            });
                            fm.row_note("Confidence level of the ellipses", "between 0 and 1", |ui| {
                                crate::civil_dlg::num(ui, &mut f.ellipse_conf, "");
                            });
                            fm.row_note("Significance level of the global test", "between 0 and 1", |ui| {
                                crate::civil_dlg::num(ui, &mut f.alpha, "");
                            });
                            fm.row_note("Blunder threshold on |w|", "or compute it from the significance level", |ui| {
                                ui.horizontal(|ui| {
                                    crate::civil_dlg::num(ui, &mut f.blunder, "");
                                    if ui.button("Compute from the significance level").clicked() {
                                        compute_blunder = true;
                                    }
                                });
                            });
                            fm.row("Scale covariances by the a posteriori factor", |ui| {
                                ui.checkbox(&mut f.scale_post, "");
                            });
                            fm.row("Include centring errors in the weights", |ui| {
                                ui.checkbox(&mut f.centring, "");
                            });
                            fm.row("Equipment where a setup has none", |ui| {
                                let mut items: Vec<String> = vec!["(none: such setups cannot be weighted)".into()];
                                items.extend(db.equipment.iter().map(|e| e.name.clone()));
                                f.equip = f.equip.min(items.len() - 1);
                                ui_kit::combo_owned(ui, "sdba-eq", &mut f.equip, &items);
                            });
                            fm.row_note("Leave out observations", "keys from the Residuals table, separated by commas", |ui| {
                                ui_kit::text_field(ui, &mut f.excluded, "");
                            });
                            fm.row_note("Data snooping", "remove the largest flagged observation and adjust again", |ui| {
                                ui.horizontal(|ui| {
                                    ui.checkbox(&mut f.snoop, "");
                                    ui.add_enabled_ui(f.snoop, |ui| crate::civil_dlg::num(ui, &mut f.snoop_max, "most removals"));
                                });
                            });
                        });
                        ui.horizontal(|ui| {
                            if ui.add_enabled(!running, egui::Button::new("Run the adjustment")).clicked() {
                                run = true;
                            }
                            if running {
                                ui.spinner();
                                ui.label("Running in the background...");
                            }
                        });
                    }
                    1 => match &result {
                        Some((_, sd::AdjOut::Plan { adjustment: a, .. })) => {
                            let rows: Vec<Vec<String>> = a.points.iter().map(|p| vec![p.name.clone(), format!("{:.4}", p.e), format!("{:.4}", p.n), format!("{:.4}", p.sigma_e), format!("{:.4}", p.sigma_n), format!("{:.4}", p.ellipse.semi_major), format!("{:.4}", p.ellipse.semi_minor), format!("{:.2}", p.ellipse.azimuth_major_deg), format!("{:.4}", p.shift.0.hypot(p.shift.1)), if p.fixed { "fixed".into() } else { String::new() }]).collect();
                            ui_kit::scroll_x(ui, "sdba-c", |ui| ui_kit::results_table(ui, "sdba-coords", &["Point", "Easting", "Northing", "sE", "sN", "Semi-major", "Semi-minor", "Azimuth", "Shift", ""], &[Col::Text, Col::Num, Col::Num, Col::Num, Col::Num, Col::Num, Col::Num, Col::Num, Col::Num, Col::Text], &rows));
                            if ui.button("Apply adjusted coordinates to the database...").clicked() {
                                apply = true;
                            }
                        }
                        Some((_, sd::AdjOut::Level { adjustment: a, .. })) => {
                            let rows: Vec<Vec<String>> = a.heights.iter().map(|h| vec![h.name.clone(), format!("{:.4}", h.z), format!("{:.4}", h.sigma), if h.fixed { "fixed".into() } else { String::new() }]).collect();
                            ui_kit::results_table(ui, "sdba-h", &["Point", "Height", "Standard error", ""], &[Col::Text, Col::Num, Col::Num, Col::Text], &rows);
                            if ui.button("Apply adjusted heights to the database...").clicked() {
                                apply = true;
                            }
                        }
                        None => ui_kit::note(ui, "Run the adjustment first."),
                    },
                    2 => match result.as_ref().map(|r| &r.1) {
                        Some(sd::AdjOut::Plan { adjustment: a, .. }) => residual_table(ui, &a.observations, a.blunder_threshold),
                        Some(sd::AdjOut::Level { adjustment: a, .. }) => residual_table(ui, &a.observations, 0.0),
                        None => ui_kit::note(ui, "Run the adjustment first."),
                    },
                    3 => match result.as_ref().map(|r| &r.1) {
                        Some(sd::AdjOut::Plan { adjustment: a, removed, snoop_note, .. }) => {
                            stat_lines(ui, a.n_observations, a.n_unknowns, a.dof, a.vtpv, a.sigma0_sq_posteriori, &a.global_test);
                            ui.label(format!("Iterations {}; {}; last step norm {:.3e}.", a.iterations, if a.converged { "converged" } else { "DID NOT CONVERGE" }, a.last_step_norm));
                            if !removed.is_empty() {
                                ui_kit::callout(ui, Level::Warning, format!("Data snooping removed: {}. {snoop_note}", removed.iter().map(|(k, w)| format!("{k} (|w| {w:.2})")).collect::<Vec<_>>().join(", ")));
                            }
                            let rows: Vec<Vec<String>> = a.orientations.iter().map(|o| vec![o.station.clone(), format!("{:.5}", o.orientation_deg), format!("{:.2}", o.sigma_arcsec)]).collect();
                            ui_kit::results_table(ui, "sdba-or", &["Setup at", "Orientation (deg)", "Standard error (arc-sec)"], &[Col::Text, Col::Num, Col::Num], &rows);
                        }
                        Some(sd::AdjOut::Level { adjustment: a, .. }) => stat_lines(ui, a.n_observations, a.n_unknowns, a.dof, a.vtpv, a.sigma0_sq_posteriori, &a.global_test),
                        None => ui_kit::note(ui, "Run the adjustment first."),
                    },
                    4 => {
                        ui_kit::note(ui, "Error ellipses are drawn in the plan as a temporary overlay (not saved) at the scale you type: a scale of 100 draws an ellipse 100 times the size of the error ellipse, so that it can be seen.");
                        ui_kit::form(ui, "sdba-e", |fm| {
                            fm.row_note("Scale", "an input: the plan scale of the ellipse", |ui| {
                                crate::civil_dlg::num_rule(ui, &mut f.ell_scale, "times", ui_kit::NumRule::required().above(0.0));
                            });
                        });
                        ui.horizontal(|ui| {
                            if ui.add_enabled(matches!(result, Some((_, sd::AdjOut::Plan { .. }))), egui::Button::new("Draw the ellipses")).clicked() {
                                draw = true;
                            }
                            if ui.button("Clear them").clicked() {
                                clear = true;
                            }
                        });
                    }
                    5 => match result.as_ref().map(|r| &r.1) {
                        Some(sd::AdjOut::Plan { adjustment: a, .. }) => {
                            let names: Vec<String> = a.points.iter().map(|p| p.name.clone()).collect();
                            if names.len() < 2 {
                                ui_kit::note(ui, "The network has fewer than two points.");
                            } else {
                                f.rel_a = f.rel_a.min(names.len() - 1);
                                f.rel_b = f.rel_b.min(names.len() - 1);
                                ui.horizontal(|ui| {
                                    ui.label("From");
                                    ui_kit::combo_owned(ui, "sdba-ra", &mut f.rel_a, &names);
                                    ui.label("to");
                                    ui_kit::combo_owned(ui, "sdba-rb", &mut f.rel_b, &names);
                                });
                                match a.relative_precision(&names[f.rel_a], &names[f.rel_b]) {
                                    Ok(r) => {
                                        ui_kit::form(ui, "sdba-rp", |fm| {
                                            fm.row("Distance", |ui| {
                                                ui.label(format!("{:.4} m", r.distance));
                                            });
                                            fm.row("Azimuth", |ui| {
                                                ui.label(format!("{:.5} deg", r.azimuth_deg));
                                            });
                                            fm.row("Standard error of the distance", |ui| {
                                                ui.label(format!("{:.4} m", r.sigma_distance));
                                            });
                                            fm.row("Standard error of the azimuth", |ui| {
                                                ui.label(format!("{:.2} arc-seconds", r.sigma_azimuth_arcsec));
                                            });
                                            fm.row("Relative precision", |ui| {
                                                ui.label(r.ratio.map(|n| format!("1 in {n:.0}")).unwrap_or_else(|| "-".into()));
                                            });
                                            fm.row("Relative error ellipse", |ui| {
                                                ui.label(format!("{:.4} by {:.4} m at azimuth {:.1} deg ({:.0} % confidence)", r.ellipse.semi_major, r.ellipse.semi_minor, r.ellipse.azimuth_major_deg, r.ellipse.confidence * 100.0));
                                            });
                                        });
                                    }
                                    Err(e) => {
                                        ui_kit::callout(ui, Level::Warning, e.to_string());
                                    }
                                }
                            }
                        }
                        _ => ui_kit::note(ui, "Run a 2D adjustment first."),
                    },
                    _ => match result.as_ref().map(|r| match &r.1 {
                        sd::AdjOut::Plan { report, .. } | sd::AdjOut::Level { report, .. } => report.clone(),
                    }) {
                        Some(text) => {
                            egui::ScrollArea::both().max_height(380.0).show(ui, |ui| {
                                ui.label(RichText::new(&text).monospace().size(11.0));
                            });
                            if ui.button("Save the report...").clicked() {
                                save = true;
                            }
                        }
                        None => ui_kit::note(ui, "Run the adjustment first."),
                    },
                }
                if !f.message.is_empty() {
                    ui_kit::callout(ui, Level::Error, f.message.clone());
                }
            },
            |ui| Footer::new("Close").cancel(None).show(ui),
        );
        if out.closed || out.inner.as_ref().is_some_and(|(_, fo)| fo.primary) {
            self.civil_ui.sdb.net = f.net;
            return;
        }
        if compute_blunder {
            match parse_f(&f.alpha).filter(|a| *a > 0.0 && *a < 1.0) {
                Some(a) => f.blunder = format!("{:.4}", normal_two_sided_critical(a)),
                None => f.message = "Type the significance level (between 0 and 1) first.".into(),
            }
        }
        if run {
            match f.options(&db) {
                Err(e) => f.message = e,
                Ok(options) => {
                    let snoop_max = if f.snoop { parse_f(&f.snoop_max).map(|v| v.max(0.0) as usize) } else { None };
                    match (f.snoop, snoop_max) {
                        (true, None) => f.message = "Type how many observations data snooping may remove.".into(),
                        _ => {
                            f.message.clear();
                            let net = db.networks.get(f.net).map(|n| n.name.clone()).unwrap_or_default();
                            let req = sd::AdjRequest { db: db.clone(), network: net.clone(), mode: if f.mode == 0 { sd::AdjMode::Plan2d } else { sd::AdjMode::Levelling }, options, snoop_max };
                            let fp = crate::background::fingerprint(&self.doc);
                            self.bg.survey = Some(Job::spawn(&format!("Adjusting network {net}"), fp, move || sd::run_adjustment(&req).map(|o| (req.network.clone(), o))));
                            self.status = format!("Adjusting network {net} in the background.");
                        }
                    }
                }
            }
        }
        if draw || clear {
            self.civil_ui.overlay.lines.retain(|l| l.width != 2.6);
            if draw {
                match (parse_f(&f.ell_scale).filter(|s| *s > 0.0), &result) {
                    (Some(scale), Some((_, sd::AdjOut::Plan { adjustment, .. }))) => {
                        for p in &adjustment.points {
                            self.civil_ui.overlay.lines.push(crate::civil_scene::SLine::new(ellipse_ring((p.e, p.n), &p.ellipse, scale), [0xE0, 0x6C, 0x2C], 2.6));
                        }
                        self.status = format!("Drew {} error ellipse(s) at {scale} times (a temporary overlay).", adjustment.points.len());
                    }
                    _ => f.message = "Type the scale of the ellipses and run a 2D adjustment first.".into(),
                }
            }
        }
        if save {
            if let Some(p) = rfd::FileDialog::new().set_file_name("adjustment-report.txt").save_file() {
                let text = match result.as_ref().map(|r| &r.1) {
                    Some(sd::AdjOut::Plan { report, .. }) | Some(sd::AdjOut::Level { report, .. }) => report.clone(),
                    None => String::new(),
                };
                match std::fs::write(&p, text) {
                    Ok(()) => self.status = format!("Saved the adjustment report to {}.", p.display()),
                    Err(e) => self.status = format!("The report could not be saved: {e}"),
                }
            }
        }
        if apply {
            self.civil_ui.sdb.net = f.net;
            self.civil_run_pending = Some(crate::civil_cmds::Cv::SurveyApply);
        }
        self.civil_ui.sdb.adjust = Some(f);
    }
}

fn residual_table(ui: &mut egui::Ui, obs: &[ObsResult], threshold: f64) {
    let rows: Vec<Vec<String>> = obs
        .iter()
        .map(|o| {
            let ang = o.kind.is_angular();
            let sc = if ang { 3600.0 } else { 1.0 };
            vec![o.key.clone(), format!("{:?}", o.kind), format!("{}", o.from), o.to.clone(), format!("{:.5}", o.observed), format!("{:.5}", o.adjusted), format!("{:.4}{}", o.residual * sc, if ang { "\"" } else { "" }), format!("{:.4}", o.redundancy), o.std_residual.map(|w| format!("{w:.2}")).unwrap_or_else(|| "-".into()), if o.flagged { "FLAGGED".into() } else { String::new() }]
        })
        .collect();
    ui_kit::note(ui, format!("Residual = adjusted minus observed (angular residuals in arc-seconds). Standardised residual w = v / sd(v); flagged when |w| exceeds {threshold:.3}. An observation with no redundancy is uncontrolled: no w."));
    ui_kit::scroll_x(ui, "sdba-r", |ui| {
        egui::ScrollArea::vertical().max_height(380.0).show(ui, |ui| {
            ui_kit::results_table(ui, "sdba-resid", &["Key", "Kind", "From", "To", "Observed", "Adjusted", "Residual", "Redundancy", "w", ""], &[Col::Text, Col::Text, Col::Text, Col::Text, Col::Num, Col::Num, Col::Num, Col::Num, Col::Num, Col::Text], &rows);
        });
    });
}

fn stat_lines(ui: &mut egui::Ui, n_obs: usize, n_unk: usize, dof: isize, vtpv: f64, s0: Option<f64>, g: &Option<GlobalTest>) {
    ui_kit::form(ui, "sdba-st", |fm| {
        fm.row("Observations", |ui| {
            ui.label(n_obs.to_string());
        });
        fm.row("Unknowns", |ui| {
            ui.label(n_unk.to_string());
        });
        fm.row("Degrees of freedom", |ui| {
            ui.label(dof.to_string());
        });
        fm.row("v'Pv", |ui| {
            ui.label(format!("{vtpv:.4}"));
        });
        fm.row("A posteriori variance factor", |ui| {
            ui.label(s0.map(|v| format!("{v:.4}")).unwrap_or_else(|| "-".into()));
        });
    });
    match g {
        Some(g) => {
            ui_kit::callout(
                ui,
                if g.passes { Level::Success } else { Level::Warning },
                format!(
                    "Global test (chi-square, alpha {}): statistic {:.4}, limits {:.4} to {:.4}: {}. A failure means the stated standard errors or the model are wrong, or there is a blunder.",
                    g.alpha,
                    g.statistic,
                    g.lower_critical,
                    g.upper_critical,
                    if g.passes { "within the limits" } else { "OUTSIDE the limits" }
                ),
            );
        }
        None => {
            ui_kit::note(ui, "No redundancy: no global test is possible.");
        }
    }
}

// ─── apply ──────────────────────────────────────────────────────────────

pub struct ApplyForm {
    net: String,
    out: sd::AdjOut,
}

impl ApplyForm {
    pub fn new(net: String, out: sd::AdjOut) -> ApplyForm {
        ApplyForm { net, out }
    }
}

impl AfroBimApp {
    pub(crate) fn sdb_apply_window(&mut self, ctx: &egui::Context) {
        let Some(f) = self.civil_ui.sdb.apply.take() else { return };
        let Some(db) = self.civil_ui.sdb.db.clone() else { return };
        let mut go = false;
        let rows: Vec<Vec<String>> = match &f.out {
            sd::AdjOut::Plan { adjustment, .. } => adjustment
                .points
                .iter()
                .filter(|p| !p.fixed)
                .map(|p| {
                    let old = db.network(&f.net).ok().and_then(|n| n.point(&p.name)).map(|s| (s.e.unwrap_or(f64::NAN), s.n.unwrap_or(f64::NAN)));
                    let (oe, on) = old.unwrap_or((f64::NAN, f64::NAN));
                    vec![p.name.clone(), format!("{oe:.4}"), format!("{on:.4}"), format!("{:.4}", p.e), format!("{:.4}", p.n), format!("{:.4}", (p.e - oe).hypot(p.n - on))]
                })
                .collect(),
            sd::AdjOut::Level { adjustment, .. } => adjustment.heights.iter().filter(|h| !h.fixed).map(|h| vec![h.name.clone(), String::new(), String::new(), format!("{:.4}", h.z), String::new(), String::new()]).collect(),
        };
        let out = Dialog::new("Apply adjusted coordinates", crate::glyphs::by_name("SURVEYAPPLY")).width(720.0).resizable(true).max_body_height(520.0).help("Writes the adjusted coordinates into the points of the network that were not fixed, and records it in the change log. The adjustment itself is not changed. Point sets in the drawing that are linked to this network follow when you run SURVEYUPDATE.").show(
            ctx,
            |ui| {
                ui_kit::callout(ui, Level::Warning, format!("{} point(s) of network {} will take their adjusted values. The old values stay in the change log only as text; keep a copy of the database file if you may want them back.", rows.len(), f.net));
                ui_kit::results_table(ui, "sdbap", &["Point", "Easting now", "Northing now", "Adjusted E", "Adjusted N", "Shift (m)"], &[Col::Text, Col::Num, Col::Num, Col::Num, Col::Num, Col::Num], &rows);
            },
            |ui| {
                let o = Footer::new("Apply").icon(crate::glyphs::by_name("SURVEYAPPLY")).enabled(!rows.is_empty(), "There is nothing to apply").show(ui);
                go = o.primary;
                o
            },
        );
        if out.closed || out.inner.as_ref().is_some_and(|(_, fo)| fo.cancel) {
            return;
        }
        if go {
            if let Some(db) = &mut self.civil_ui.sdb.db {
                match sd::apply_adjusted(db, &f.net, &f.out, &crate::transform_doc::now_iso()) {
                    Ok(n) => self.status = format!("Applied the adjusted values of {n} point(s) of network {} to the survey database.", f.net),
                    Err(e) => self.status = format!("Not applied: {e}"),
                }
            }
            self.sdb_save();
            return;
        }
        self.civil_ui.sdb.apply = Some(f);
    }
}

// ─── figures ────────────────────────────────────────────────────────────

pub struct FigForm {
    net: usize,
    chord: String,
    surface: Option<afrobim_kernel::ElementId>,
    prefixes: Vec<(String, String, String, bool)>,
    message: String,
}

impl FigForm {
    pub fn new(db: &SurveyDatabase, net: usize) -> FigForm {
        FigForm { net, chord: String::new(), surface: None, prefixes: db.figure_prefixes.iter().map(|p| (p.prefix.clone(), p.style.clone(), p.layer.clone(), p.breakline)).collect(), message: String::new() }
    }
}

impl AfroBimApp {
    pub(crate) fn sdb_figures_window(&mut self, ctx: &egui::Context) {
        let Some(mut f) = self.civil_ui.sdb.figures.take() else { return };
        let Some(db) = self.civil_ui.sdb.db.clone() else { return };
        let surfaces = crate::civil::of_category(&self.doc, crate::civil::SURFACE);
        if f.surface.is_none() {
            f.surface = surfaces.first().map(|s| s.0);
        }
        let (mut generate, mut draw, mut breaklines, mut save_prefix) = (false, false, false, false);
        let out = Dialog::new("Survey figures", crate::glyphs::by_name("SURVEYFIGURES")).width(760.0).resizable(true).max_body_height(620.0).help("Figures are the linework of the field codes: a point description such as EP B begins a figure, EP C continues it, EP E ends it, CLS closes it (BC, OC and EC make a curve through three points). The figure prefix table gives each figure its style, layer and whether it is a breakline. Figures are drawn in the plan from their points; the ones flagged as breaklines can be added to a surface definition.").show(
            ctx,
            |ui| {
                ui_kit::form(ui, "sdbf", |fm| {
                    fm.row("Network", |ui| {
                        net_combo(ui, "sdbf-net", &db, &mut f.net);
                    });
                    fm.row_note("Chord tolerance for curves", "an input: arcs are drawn as chords this close to the arc", |ui| {
                        crate::civil_dlg::num_rule(ui, &mut f.chord, "m", ui_kit::NumRule::required().above(0.0));
                    });
                });
                ui_kit::section(ui, "Figure prefixes", |ui| {
                    egui::Grid::new("sdbf-pre").num_columns(5).spacing([6.0, 4.0]).show(ui, |ui| {
                        for h in ["Prefix", "Style", "Layer", "Breakline", ""] {
                            ui_kit::th(ui, h);
                        }
                        ui.end_row();
                        let mut remove = None;
                        for (i, p) in f.prefixes.iter_mut().enumerate() {
                            ui.add(egui::TextEdit::singleline(&mut p.0).desired_width(70.0));
                            ui.add(egui::TextEdit::singleline(&mut p.1).desired_width(100.0));
                            ui.add(egui::TextEdit::singleline(&mut p.2).desired_width(100.0));
                            ui.checkbox(&mut p.3, "");
                            if ui_kit::icon_button(ui, crate::glyphs::ui::TRASH, "Remove").clicked() {
                                remove = Some(i);
                            }
                            ui.end_row();
                        }
                        if let Some(i) = remove {
                            f.prefixes.remove(i);
                        }
                    });
                    ui.horizontal(|ui| {
                        if ui.button("Add a prefix").clicked() {
                            f.prefixes.push(Default::default());
                        }
                        if ui.button("Keep the table").clicked() {
                            save_prefix = true;
                        }
                    });
                });
                let net = db.networks.get(f.net);
                ui_kit::section(ui, "Figures in the network", |ui| {
                    if ui.button("Generate figures from the linework codes").clicked() {
                        generate = true;
                    }
                    let rows: Vec<Vec<String>> = net.map(|n| n.figures.iter().map(|g| vec![g.name.clone(), g.vertices.len().to_string(), if g.closed { "yes".into() } else { "no".into() }, if g.breakline { "yes".into() } else { "no".into() }, g.layer.clone(), g.style.clone(), if g.from_linework { "linework".into() } else { "imported".into() }]).collect()).unwrap_or_default();
                    ui_kit::results_table(ui, "sdbf-t", &["Figure", "Vertices", "Closed", "Breakline", "Layer", "Style", "From"], &[Col::Text, Col::Num, Col::Text, Col::Text, Col::Text, Col::Text, Col::Text], &rows);
                });
                ui_kit::section(ui, "Into the drawing", |ui| {
                    if ui.button("Draw the figures in the plan").clicked() {
                        draw = true;
                    }
                    ui.horizontal(|ui| {
                        ui.label("Offer the breaklines to the surface");
                        crate::civil_dlg::pick_from(ui, "sdbf-surf", &surfaces, &mut f.surface);
                        if ui.add_enabled(f.surface.is_some(), egui::Button::new("Add as breaklines")).clicked() {
                            breaklines = true;
                        }
                    });
                    ui_kit::note(ui, "Breaklines are added to the surface definition (SURFACEDEF shows them and rebuilds the surface). Only figures flagged as breaklines, with a level on every vertex, are offered.");
                });
                if !f.message.is_empty() {
                    ui_kit::callout(ui, Level::Info, f.message.clone());
                }
            },
            |ui| Footer::new("Close").cancel(None).show(ui),
        );
        if out.closed || out.inner.as_ref().is_some_and(|(_, fo)| fo.primary) {
            self.civil_ui.sdb.net = f.net;
            return;
        }
        let time = crate::transform_doc::now_iso();
        let net_name = db.networks.get(f.net).map(|n| n.name.clone()).unwrap_or_default();
        if save_prefix {
            if let Some(d) = &mut self.civil_ui.sdb.db {
                d.figure_prefixes = f.prefixes.iter().filter(|p| !p.0.trim().is_empty()).map(|p| FigurePrefix { prefix: p.0.trim().to_string(), style: p.1.clone(), layer: p.2.clone(), breakline: p.3 }).collect();
            }
            self.sdb_save();
            f.message = "The figure prefix table was kept.".into();
        }
        if generate {
            if let Some(d) = &mut self.civil_ui.sdb.db {
                match d.generate_figures(&net_name, &time) {
                    Ok(w) => f.message = format!("Generated figures for {net_name}. {}", if w.is_empty() { String::new() } else { format!("{} warning(s): {}", w.len(), w.join("; ")) }),
                    Err(e) => f.message = e.to_string(),
                }
            }
            self.sdb_save();
        }
        let chord = parse_f(&f.chord).filter(|c| *c > 0.0);
        if draw || breaklines {
            match chord {
                None => f.message = "Type the chord tolerance first (an input, above zero).".into(),
                Some(c) => match sd::figure_records(self.civil_ui.sdb.db.as_ref().unwrap_or(&db), &net_name, c) {
                    Err(e) => f.message = e,
                    Ok(data) => {
                        if draw {
                            let id = sd::add_figures(&mut self.doc, &data);
                            self.status = format!("Drew {} figure(s) of network {net_name} in the plan. {} note(s).", data.figures.len(), data.notes.len());
                            self.doc.select(&[id], afrobim_kernel::SelectMode::Replace);
                            self.schedule_commit_now();
                        }
                        if breaklines {
                            let lines = sd::breaklines_of(&data);
                            match (f.surface, lines.is_empty()) {
                                (_, true) => f.message = "No figure is flagged as a breakline with a level on every vertex.".into(),
                                (Some(sid), false) => {
                                    if let Some(mut sdata) = crate::civil::get::<crate::civil::SurfaceData>(&self.doc, sid, crate::civil::SURFACE) {
                                        let n = lines.len();
                                        sdata.surface.definition.push(afrobim_terrain::Operation::AddBreaklines { lines });
                                        crate::civil::set(&mut self.doc, sid, serde_json::to_value(&sdata).unwrap_or_default(), None);
                                        f.message = format!("Added {n} breakline(s) to the surface definition. Open SURFACEDEF to see them in the operations.");
                                        self.status = f.message.clone();
                                        self.schedule_commit_now();
                                    }
                                }
                                _ => {}
                            }
                        }
                    }
                },
            }
        }
        self.civil_ui.sdb.figures = Some(f);
    }
}

// ─── insert and update ──────────────────────────────────────────────────

pub struct InsertForm {
    net: usize,
    first: String,
    message: String,
}

impl InsertForm {
    pub fn new(_db: &SurveyDatabase, net: usize) -> InsertForm {
        InsertForm { net, first: String::new(), message: String::new() }
    }
}

impl AfroBimApp {
    pub(crate) fn sdb_insert_window(&mut self, ctx: &egui::Context) {
        let Some(mut f) = self.civil_ui.sdb.insert.take() else { return };
        let Some(db) = self.civil_ui.sdb.db.clone() else { return };
        let mut go = false;
        let first_ok = std::cell::Cell::new(false);
        let out = Dialog::new("Insert survey points", crate::glyphs::by_name("SURVEYINSERT")).width(560.0).help("Adds the points of a network to the drawing as a COGO point set. The set keeps a link to the database and the network: when the database changes (an import, an adjustment applied), SURVEYUPDATE brings the set up to date. A point whose name is a whole number keeps it as its point number; the others take the next free numbers from the one you give.").show(
            ctx,
            |ui| {
                ui_kit::form(ui, "sdbn", |fm| {
                    fm.row("Network", |ui| {
                        net_combo(ui, "sdbn-net", &db, &mut f.net);
                    });
                    fm.row_note("First free point number", "for points whose names are not numbers", |ui| {
                        crate::civil_dlg::num_rule(ui, &mut f.first, "", ui_kit::NumRule::required().min(1.0));
                    });
                });
                first_ok.set(parse_f(&f.first).is_some_and(|v| v >= 1.0));
                if let Some(n) = db.networks.get(f.net) {
                    ui_kit::note(ui, format!("{} of the {} points have coordinates.", n.points.iter().filter(|p| p.e.is_some() && p.n.is_some()).count(), n.points.len()));
                }
                if !f.message.is_empty() {
                    ui_kit::callout(ui, Level::Error, f.message.clone());
                }
            },
            |ui| {
                let o = Footer::new("Insert").icon(crate::glyphs::by_name("SURVEYINSERT")).enabled(first_ok.get(), "Type the first free point number").show(ui);
                go = o.primary;
                o
            },
        );
        if out.closed || out.inner.as_ref().is_some_and(|(_, fo)| fo.cancel) {
            self.civil_ui.sdb.net = f.net;
            return;
        }
        if go {
            let net = db.networks.get(f.net).map(|n| n.name.clone()).unwrap_or_default();
            let path = self.civil_ui.sdb.path.as_ref().map(|p| sd::stored_path(self.doc_path(), p)).unwrap_or_else(|| "(not saved)".into());
            match sd::insert_points(&mut self.doc, &db, &path, &net, parse_f(&f.first).unwrap_or(1.0) as u32) {
                Ok(id) => {
                    self.status = format!("Inserted the points of network {net} as a point set linked to the database.");
                    self.doc.select(&[id], afrobim_kernel::SelectMode::Replace);
                    self.schedule_commit_now();
                    self.zoom_to_element(id);
                    return;
                }
                Err(e) => f.message = e,
            }
        }
        self.civil_ui.sdb.insert = Some(f);
    }
}

#[derive(Default)]
pub struct UpdateForm {
    message: String,
}

impl AfroBimApp {
    pub(crate) fn sdb_update_window(&mut self, ctx: &egui::Context) {
        let Some(mut f) = self.civil_ui.sdb.update.take() else { return };
        let db = self.civil_ui.sdb.db.clone();
        let sets = sd::linked_sets(&self.doc, db.as_ref());
        let mut act: Option<(afrobim_kernel::ElementId, bool)> = None;
        let mut all = false;
        let out = Dialog::new("Update from the survey database", crate::glyphs::by_name("SURVEYUPDATE")).width(700.0).resizable(true).max_body_height(480.0).help("Point sets in the drawing that were inserted from the database are compared with it. Update moves the points whose coordinates changed, adds new points and removes the ones that are gone from the database. Check shows what would change without changing anything.").show_body(
            ctx,
            |ui| {
                if sets.is_empty() {
                    ui_kit::note(ui, "No point set in the drawing is linked to a survey database. Insert one with SURVEYINSERT.");
                }
                for (id, name, link, changed) in &sets {
                    ui.horizontal(|ui| {
                        ui.label(RichText::new(name).strong());
                        ui.label(match changed {
                            Some(true) => RichText::new("the database changed").color(crate::theme::warn()),
                            Some(false) => RichText::new("up to date").weak(),
                            None => RichText::new("database not open").weak(),
                        });
                        if ui.small_button("Check").clicked() {
                            act = Some((*id, false));
                        }
                        if ui.add_enabled(*changed == Some(true), egui::Button::new("Update").small()).clicked() {
                            act = Some((*id, true));
                        }
                    });
                    ui_kit::note(ui, format!("Network {} of {} ({})", link.network, link.db_name, link.db_path));
                }
                if sets.iter().any(|s| s.3 == Some(true)) && ui.button("Update all that changed").clicked() {
                    all = true;
                }
                if !f.message.is_empty() {
                    ui_kit::callout(ui, Level::Info, f.message.clone());
                }
            },
        );
        if out.closed {
            return;
        }
        if let Some(db) = db {
            let todo: Vec<(afrobim_kernel::ElementId, bool)> = if all { sets.iter().filter(|s| s.3 == Some(true)).map(|s| (s.0, true)).collect() } else { act.into_iter().collect() };
            for (id, apply) in todo {
                match sd::update_points(&mut self.doc, id, &db, apply) {
                    Ok(s) => {
                        f.message = format!("{}: {} moved, {} added, {} removed, {} unchanged{}.", self.doc.find(id).map(|e| e.name.clone()).unwrap_or_default(), s.moved.len(), s.added.len(), s.removed.len(), s.unchanged, if apply { " (updated)" } else { " (not changed)" });
                        if apply {
                            self.status = f.message.clone();
                            self.schedule_commit_now();
                        }
                    }
                    Err(e) => f.message = e,
                }
            }
        }
        self.civil_ui.sdb.update = Some(f);
    }
}
