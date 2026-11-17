//! Existing ground profiles (sampled from a surface along an alignment, kept up to date) and
//! profile views drawn in the model.

use afrobim_kernel::{ElementId, SelectMode};

use crate::app::AfroBimApp;
use crate::civil::{self, Bounds, ProfileData, ProfileViewData, PROFILE, PROFILE_VIEW};
use crate::civil_dlg::{self as d};
use crate::ui_kit::{self, Dialog, Footer, NumRule};

#[derive(Default)]
pub struct ProfileUi {
    pub ground: Option<GroundForm>,
    pub view: Option<ViewForm>,
}

pub struct GroundForm {
    edit: Option<ElementId>,
    name: String,
    alignment: Option<ElementId>,
    surface: Option<ElementId>,
    interval: String,
}

pub struct ViewForm {
    edit: Option<ElementId>,
    name: String,
    alignment: Option<ElementId>,
    profiles: Vec<ElementId>,
    show_design: bool,
    exaggeration: String,
    datum: String,
    e: String,
    n: String,
    bands: bool,
    band_height: String,
}

impl AfroBimApp {
    fn alignment_name_of(&self, id: Option<ElementId>) -> Option<String> {
        id.and_then(|i| crate::alignment_glue::alignment_of(&self.doc, i)).map(|a| a.name)
    }

    fn alignment_id_named(&self, name: &str) -> Option<ElementId> {
        crate::alignment_glue::alignments(&self.doc).into_iter().find(|(id, _)| self.alignment_name_of(Some(*id)).as_deref() == Some(name)).map(|a| a.0)
    }

    pub(crate) fn civil_profile_cmd(&mut self, cv: crate::civil_cmds::Cv) {
        let aligns = crate::alignment_glue::alignments(&self.doc);
        if aligns.is_empty() {
            self.status = "There is no alignment yet. Import one (LANDXML) or draw one (ALIGNDESIGN) first.".into();
            return;
        }
        let sel = self.doc.primary();
        match cv {
            crate::civil_cmds::Cv::GroundProfile => {
                let editing = sel.filter(|id| self.doc.find(*id).is_some_and(|e| e.category == PROFILE));
                let f = match editing.and_then(|id| civil::get::<ProfileData>(&self.doc, id, PROFILE).map(|d| (id, d))) {
                    Some((id, pd)) => GroundForm { edit: Some(id), name: self.doc.find(id).map(|e| e.name.clone()).unwrap_or_default(), alignment: self.alignment_id_named(&pd.alignment), surface: Some(ElementId(pd.surface)).filter(|s| self.doc.find(*s).is_some()), interval: pd.interval_m.to_string() },
                    None => GroundForm { edit: None, name: String::new(), alignment: aligns.first().map(|a| a.0), surface: civil::surface_choices(&self.doc).first().map(|s| s.0), interval: String::new() },
                };
                self.civil_ui.prof.ground = Some(f);
            }
            _ => {
                let editing = sel.filter(|id| self.doc.find(*id).is_some_and(|e| e.category == PROFILE_VIEW));
                let f = match editing.and_then(|id| civil::get_profile_view(&self.doc, id).map(|d| (id, d))) {
                    Some((id, pv)) => ViewForm { edit: Some(id), name: self.doc.find(id).map(|e| e.name.clone()).unwrap_or_default(), alignment: self.alignment_id_named(&pv.alignment), profiles: pv.profiles.iter().map(|p| ElementId(*p)).collect(), show_design: pv.show_design, exaggeration: pv.vertical_exaggeration.to_string(), datum: pv.datum.map(|v| v.to_string()).unwrap_or_default(), e: format!("{:.3}", pv.insertion[0]), n: format!("{:.3}", pv.insertion[1]), bands: pv.show_bands, band_height: pv.band_height_m.to_string() },
                    None => {
                        let al = aligns.first().map(|a| a.0);
                        // A suggested place: under the alignment's plan, clear of it. It is a layout choice only; type another.
                        let (e, n) = al.and_then(|i| crate::alignment_glue::alignment_of(&self.doc, i)).map(|a| {
                            let pl = a.polyline(5.0);
                            (pl.iter().map(|p| p.0).fold(f64::INFINITY, f64::min), pl.iter().map(|p| p.1).fold(f64::INFINITY, f64::min) - 120.0)
                        }).unwrap_or((0.0, 0.0));
                        ViewForm { edit: None, name: String::new(), alignment: al, profiles: civil::of_category(&self.doc, PROFILE).into_iter().map(|p| p.0).collect(), show_design: true, exaggeration: String::new(), datum: String::new(), e: format!("{e:.3}"), n: format!("{n:.3}"), bands: true, band_height: String::new() }
                    }
                };
                self.civil_ui.prof.view = Some(f);
            }
        }
    }

    pub(crate) fn civil_profile_windows(&mut self, ctx: &egui::Context) {
        self.ground_window(ctx);
        self.view_window(ctx);
    }

    fn ground_window(&mut self, ctx: &egui::Context) {
        let Some(mut f) = self.civil_ui.prof.ground.take() else { return };
        let aligns = crate::alignment_glue::alignments(&self.doc);
        let surfaces = civil::surface_choices(&self.doc);
        let pos = NumRule::required().above(0.0);
        let interval = d::num_rule_peek(&f.interval, &pos);
        let ok = f.alignment.is_some() && f.surface.is_some() && interval.is_some();
        let out = Dialog::new(if f.edit.is_some() { "Edit ground profile" } else { "Existing ground profile" }, crate::glyphs::by_name("GROUNDPROFILE")).width(500.0).help("The profile is sampled from the surface at the stations of the alignment, at the spacing you enter, and is kept up to date: it changes when the alignment or the surface does. Stations off the surface have no level.").show(
            ctx,
            |ui| {
                ui_kit::form(ui, "gp", |fm| {
                    fm.row("Name", |ui| d::text(ui, &mut f.name, "e.g. Existing ground"));
                    fm.row("Alignment", |ui| {
                        d::pick_from(ui, "gp-al", &aligns, &mut f.alignment);
                    });
                    fm.row("Surface", |ui| {
                        d::pick_from(ui, "gp-sf", &surfaces, &mut f.surface);
                    });
                    fm.row_note("Spacing of samples", "Along the alignment, in metres", |ui| {
                        d::num_rule(ui, &mut f.interval, "m", pos);
                    });
                });
                ui.horizontal(|ui| {
                    if ui.button("Example value").on_hover_text("10 m: a demonstration number, not from any standard").clicked() {
                        f.interval = "10".into();
                    }
                });
            },
            |ui| Footer::new(if f.edit.is_some() { "Apply" } else { "Create profile" }).icon(crate::glyphs::by_name("GROUNDPROFILE")).enabled(ok, "Choose the alignment and the surface and enter a spacing").show(ui),
        );
        if out.closed || out.inner.as_ref().is_some_and(|(_, fo)| fo.cancel) {
            return;
        }
        if out.inner.as_ref().is_some_and(|(_, fo)| fo.primary) {
            if let (Some(al), Some(sf), Some(iv)) = (self.alignment_name_of(f.alignment), f.surface, interval) {
                let data = ProfileData { alignment: al, surface: sf.0, interval_m: iv, imported: vec![] };
                let v = serde_json::to_value(&data).unwrap_or_default();
                match f.edit {
                    Some(id) => {
                        civil::set(&mut self.doc, id, v, None);
                        if !f.name.trim().is_empty() {
                            if let Some(e) = self.doc.find_mut(id) {
                                e.name = f.name.trim().to_string();
                            }
                        }
                    }
                    None => {
                        let name = if f.name.trim().is_empty() { format!("{} ground", data.alignment) } else { f.name.trim().to_string() };
                        let id = civil::add(&mut self.doc, PROFILE, &name, v, None, "Sampled from a surface");
                        self.doc.select(&[id], SelectMode::Replace);
                    }
                }
                self.status = "Ground profile saved. It follows the alignment and the surface.".into();
                self.schedule_commit_now();
            }
            return;
        }
        self.civil_ui.prof.ground = Some(f);
    }

    fn view_window(&mut self, ctx: &egui::Context) {
        let Some(mut f) = self.civil_ui.prof.view.take() else { return };
        let aligns = crate::alignment_glue::alignments(&self.doc);
        let al_name = self.alignment_name_of(f.alignment);
        let profiles: Vec<(ElementId, String)> = civil::of_category(&self.doc, PROFILE).into_iter().filter(|(id, _)| civil::get::<ProfileData>(&self.doc, *id, PROFILE).is_some_and(|p| Some(&p.alignment) == al_name.as_ref())).collect();
        let pos = NumRule::required().above(0.0);
        let ex = d::num_rule_peek(&f.exaggeration, &pos);
        let bh = if f.bands { d::num_rule_peek(&f.band_height, &pos) } else { Some(3.0) };
        let (e, n) = (f.e.trim().parse::<f64>().ok(), f.n.trim().parse::<f64>().ok());
        let datum = if f.datum.trim().is_empty() { Some(None) } else { f.datum.trim().parse::<f64>().ok().map(Some) };
        let ok = f.alignment.is_some() && ex.is_some() && bh.is_some() && e.is_some() && n.is_some() && datum.is_some();
        let out = Dialog::new(if f.edit.is_some() { "Edit profile view" } else { "Profile view" }, crate::glyphs::by_name("PROFILEVIEW")).width(540.0).help("The view is drawn in the model at the insertion point you give (its lower left corner): stations along the horizontal at the true scale, levels up at the vertical exaggeration you enter. The bands below the grid give station, ground level, design level and fill (+) or cut (-). A ground profile imported from LandXML (ProfSurf) shows here like a sampled one.").show(
            ctx,
            |ui| {
                ui_kit::form(ui, "pv", |fm| {
                    fm.row("Name", |ui| d::text(ui, &mut f.name, "e.g. Profile view 1"));
                    fm.row("Alignment", |ui| {
                        let before = f.alignment;
                        d::pick_from(ui, "pv-al", &aligns, &mut f.alignment);
                        if before != f.alignment {
                            f.profiles.clear();
                        }
                    });
                    fm.row("Ground profiles", |ui| {
                        ui.vertical(|ui| {
                            if profiles.is_empty() {
                                crate::ui_kit::note(ui, "None on this alignment: make one with GROUNDPROFILE.");
                            }
                            for (id, name) in &profiles {
                                let mut on = f.profiles.contains(id);
                                if ui.checkbox(&mut on, name).changed() {
                                    if on {
                                        f.profiles.push(*id);
                                    } else {
                                        f.profiles.retain(|p| p != id);
                                    }
                                }
                            }
                        });
                    });
                    fm.row("Design line", |ui| {
                        ui.checkbox(&mut f.show_design, "Draw the alignment's design profile");
                    });
                    fm.row_note("Vertical exaggeration", "Vertical scale over horizontal, for example 5 for 5:1", |ui| {
                        d::num_rule(ui, &mut f.exaggeration, "", pos);
                    });
                    fm.row_note("Datum level", "Blank: the lowest level shown, rounded down", |ui| {
                        d::num_opt(ui, &mut f.datum, "m").ok();
                    });
                    fm.row("Insertion easting", |ui| {
                        d::num(ui, &mut f.e, "m");
                    });
                    fm.row("Insertion northing", |ui| {
                        d::num(ui, &mut f.n, "m");
                    });
                    fm.row("Bands", |ui| {
                        ui.checkbox(&mut f.bands, "Station, ground, design and cut or fill");
                    });
                    if f.bands {
                        fm.row("Band row height", |ui| {
                            d::num_rule(ui, &mut f.band_height, "m", pos);
                        });
                    }
                });
                ui.horizontal(|ui| {
                    if ui.button("Example values").on_hover_text("Exaggeration 5 and a band row of 3 m: demonstration numbers, not from any standard").clicked() {
                        f.exaggeration = "5".into();
                        f.band_height = "3".into();
                    }
                });
            },
            |ui| Footer::new(if f.edit.is_some() { "Apply" } else { "Create profile view" }).icon(crate::glyphs::by_name("PROFILEVIEW")).enabled(ok, "Complete the alignment, the exaggeration and the insertion point").show(ui),
        );
        if out.closed || out.inner.as_ref().is_some_and(|(_, fo)| fo.cancel) {
            return;
        }
        if out.inner.as_ref().is_some_and(|(_, fo)| fo.primary) {
            if let (Some(al), Some(ex), Some(bh), Some(e), Some(n), Some(datum)) = (al_name, ex, bh, e, n, datum) {
                let data = ProfileViewData { alignment: al.clone(), profiles: f.profiles.iter().map(|p| p.0).collect(), show_design: f.show_design, vertical_exaggeration: ex, datum, insertion: [e, n], show_bands: f.bands, band_height_m: bh, cloud: vec![] };
                let v = serde_json::to_value(&data).unwrap_or_default();
                let b = Bounds { min_e: e, min_n: n, max_e: e + 100.0, max_n: n + 40.0 };
                match f.edit {
                    Some(id) => {
                        civil::set(&mut self.doc, id, v, Some(b));
                        if !f.name.trim().is_empty() {
                            if let Some(el) = self.doc.find_mut(id) {
                                el.name = f.name.trim().to_string();
                            }
                        }
                    }
                    None => {
                        let name = if f.name.trim().is_empty() { format!("{al} profile view") } else { f.name.trim().to_string() };
                        let id = civil::add(&mut self.doc, PROFILE_VIEW, &name, v, Some(b), "Drawn from the alignment and ground profiles");
                        self.doc.select(&[id], SelectMode::Replace);
                    }
                }
                self.status = "Profile view saved.".into();
                self.schedule_commit_now();
            }
            return;
        }
        self.civil_ui.prof.view = Some(f);
    }
}
