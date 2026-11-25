// SPDX-License-Identifier: LicenseRef-AfroBIM-Proprietary
// Copyright (c) 2026 AfroBIM. All rights reserved.
//! Screenshot aids: `--aid <name>` fills a dialog with arbitrary test values so that its look can
//! be checked on a machine without a desktop (the headless screenshot cannot click). Every value
//! here is an arbitrary test value, not an engineering value, and nothing is kept in the document
//! except where an aid says so.

use crate::app::AfroBimApp;
use crate::check_inputs::{self as inputs, CulvertInputs};

impl AfroBimApp {
    pub(crate) fn apply_aid(&mut self, name: &str) {
        // A name ending in `-dialog` opens the dialog without the calculation window over it.
        let show_report = !name.ends_with("-dialog");
        let name = name.trim_end_matches("-dialog");
        match name {
            // The pavement design dialog with the South African method filled in and run.
            "sa-pavement" => {
                self.road.open_design(&self.doc, None);
                if let Some(d) = self.road.design.as_mut() {
                    d.form.method = crate::road_glue::SA_METHOD;
                    let t = &mut d.form.trh4;
                    t.category = Some(1);
                    t.daily_e80_per_lane = Some(300.0);
                    t.hv_growth_percent = Some(4.0);
                    t.e80_growth_percent = Some(2.0);
                    t.years_survey_to_opening = Some(1.0);
                    t.design_period_years = Some(20.0);
                    t.climate_region = Some(1);
                    t.subgrade_cbr_percent = Some(9.0);
                    if let (true, Ok(out)) = (show_report, d.form.run()) {
                        self.design_report = Some(("South Africa SAPEM / TRH4 (arbitrary test values)".into(), out.lines));
                    }
                }
            }
            // The rational method dialog on the SANRAL method, run on test values.
            "sanral-rational" => {
                self.checks.open_rational(&self.doc, None);
                if let Some(d) = self.checks.rational.as_mut() {
                    d.form.use_sanral = true;
                    let s = &mut d.form.sanral;
                    s.area_km2 = "2".into();
                    s.map_mm = "700".into();
                    s.slope = ["".into(), "100".into(), "".into(), "".into()];
                    s.permeability = ["".into(), "".into(), "100".into(), "".into()];
                    s.vegetation = ["".into(), "".into(), "100".into(), "".into()];
                    s.dolomite_percent = "0".into();
                    s.ft_class = Some(1);
                    s.alpha = "1".into();
                    s.beta = "0".into();
                    s.gamma = "0".into();
                    s.return_period = "20".into();
                    s.reaches = vec![inputs::ReachRow { kind: 1, r: String::new(), length: "3".into(), slope_or_velocity: "0.02".into() }];
                    s.point_rainfall_mm = "80".into();
                    s.rainfall_source = "arbitrary test value".into();
                    s.area_reduction = 2;
                    d.result = Some(d.form.run_flow());
                }
            }
            // A real culvert on the first alignment with the complete MoWT check entered, and its calculation shown.
            "culvert-complete" | "culvert-sanral" => {
                let Some((_, alignment)) = crate::alignment_glue::alignments(&self.doc).into_iter().next() else { return };
                let Some(a) = crate::alignment_glue::alignments(&self.doc).first().and_then(|(id, _)| crate::alignment_glue::alignment_of(&self.doc, *id)) else { return };
                let def = afrobim_drainage::PipeCulvertDefinition {
                    alignment,
                    station_m: a.start_station_m + a.length() * 0.3,
                    skew_deg: 0.0,
                    barrels: 1,
                    internal_diameter_mm: 900.0,
                    wall_thickness_mm: 100.0,
                    length_mm: 12_000.0,
                    barrel_spacing_mm: None,
                    invert_level_inlet_m: 10.24,
                    invert_level_outlet_m: 10.0,
                    inlet_side: afrobim_drainage::Side::Left,
                    centre_offset_m: 0.0,
                    road_level_m: None,
                    material: "as specified".into(),
                    placeholder: false,
                };
                let Ok(id) = afrobim_drainage::add_pipe_culvert(&mut self.doc, "CU-TEST", &def) else { return };
                let mut form = CulvertInputs { flow_m3_s: "0.3".into(), manning_n: "0.013".into(), entrance_ke: "0.5".into(), tailwater_m: "0.2".into(), inlet_edge: Some(1), allowable_headwater_m: "5".into(), ..Default::default() };
                if name == "culvert-sanral" {
                    form.set_basis(afrobim_drainage::design::DesignBasis::Sanral);
                    form.sanral = inputs::SanralCulvertEntries { slope_unit: Some(0), k_in: "0.5".into(), k_out: "1".into(), friction_value: "0.013".into(), h2_m: "0.2".into(), road_class: Some(2), ..Default::default() };
                }
                inputs::save(&mut self.doc, id, inputs::KEY_CULVERT, &form);
                self.sync_generated();
                let rows = self.check_rows_of(id);
                let mut lines = Vec::new();
                for r in &rows {
                    lines.push(format!("{}: {}. {}", r.check, r.status.label(), r.reason));
                    lines.extend(r.lines.iter().cloned());
                }
                if show_report {
                    self.design_report = Some(("CU-TEST (arbitrary test values)".into(), lines));
                }
                self.checks.open_culvert(&self.doc, id);
            }
            _ => {}
        }
    }
}
