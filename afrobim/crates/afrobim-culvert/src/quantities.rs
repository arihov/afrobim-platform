//! Quantities: the payable items and the quantity lines.
//!
//! The lines have the same shape as `afrobim-cost`'s quantity lines (category, description,
//! quantity, unit, attributes) but are this crate's own type, so the crate does not depend on the
//! cost crate. The app maps them with the `culvert.*` categories listed in the README.

use std::collections::BTreeMap;
use std::f64::consts::PI;

use serde::{Deserialize, Serialize};

use crate::bbs::Bbs;
use crate::layout::{EndLayout, Layout};
use crate::model::{Barrel, CulvertInput};
use crate::road::Ogl;

/// One measured quantity.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct QuantityLine {
    /// `culvert.<item>`, e.g. `culvert.surround`.
    pub category: String,
    pub description: String,
    pub quantity: f64,
    /// `m`, `m3`, `kg` or `nr`.
    pub unit: String,
    /// What distinguishes the line for mapping: concrete class, pipe size, bar diameter.
    pub attributes: BTreeMap<String, String>,
}

/// Volumes of one end structure, m3.
#[derive(Debug, Clone, Copy, PartialEq, Default, Serialize, Deserialize)]
pub struct EndQuantities {
    pub headwall_m3: f64,
    pub wings_m3: f64,
    pub apron_m3: f64,
    pub toe_m3: f64,
    /// Blinding under the whole footprint of the end structure.
    pub blinding_m3: f64,
    /// Plan area of the footprint, m2.
    pub footprint_m2: f64,
    pub weep_holes_m: f64,
    pub weep_holes_nr: u32,
    pub excavation_m3: f64,
}

impl EndQuantities {
    pub fn concrete_m3(&self) -> f64 {
        self.headwall_m3 + self.wings_m3 + self.apron_m3 + self.toe_m3
    }
}

/// The quantities of a culvert.
#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
pub struct Quantities {
    /// Pipe length per cell, m (pipe barrels).
    pub pipe_length_m: f64,
    pub cells: u32,
    pub barrel_blinding_m3: f64,
    pub bedding_m3: f64,
    pub surround_m3: f64,
    /// Concrete of a box barrel, m3.
    pub box_concrete_m3: f64,
    pub left: EndQuantities,
    pub right: EndQuantities,
    pub end_blinding_m3: f64,
    pub headwalls_m3: f64,
    pub wings_m3: f64,
    pub aprons_m3: f64,
    pub toes_m3: f64,
    pub end_structures_m3: f64,
    pub excavation_m3: f64,
    pub backfill_m3: f64,
    pub weep_holes_m: f64,
    pub weep_holes_nr: u32,
    pub steel_kg: f64,
    pub lines: Vec<QuantityLine>,
}

fn end_quantities(e: &EndLayout, layout: &Layout, input: &CulvertInput, ogl: &Ogl) -> EndQuantities {
    let s = &e.spec;
    let mm3 = 1e-9;
    let mm2 = 1e-6;
    let hw_area = e.headwall_width_mm * s.headwall_thickness_mm;
    let mut headwall = hw_area * s.headwall_height_mm * mm3;
    if input.options.deduct_barrel_opening {
        let opening = match input.barrel {
            Barrel::Pipe(p) => p.cells as f64 * PI * (p.outside_diameter_mm() / 2.0).powi(2),
            Barrel::Box(b) => b.cells as f64 * b.clear_span_mm * b.clear_height_mm,
        };
        headwall = (headwall - opening * s.headwall_thickness_mm * mm3).max(0.0);
    }
    let wing_each = s.wing.thickness_mm * s.wing.length_mm * (s.wing.height_at_headwall_mm + s.wing.height_at_end_mm) / 2.0 * mm3;
    let apron_area = (e.headwall_width_mm + e.apron_end_width_mm) / 2.0 * s.apron.length_mm;
    let toe_area = s.toe.width_mm * e.overall_width_mm;
    let wing_area = 2.0 * s.wing.thickness_mm * s.wing.length_mm;
    let footprint = hw_area + apron_area + toe_area + wing_area;
    // Excavation to the underside of the blinding at the foot of the headwall.
    let s_end = if e.side == crate::road::Side::Left { -layout.left_len_mm } else { layout.right_len_mm };
    let ground = ogl.level_at(s_end / 1000.0 * layout.skew_deg.to_radians().sin());
    let depth_m = (ground - (e.headwall_base_m - s.blinding_mm / 1000.0)).max(0.0);
    EndQuantities {
        headwall_m3: headwall,
        wings_m3: 2.0 * wing_each,
        apron_m3: apron_area * s.apron.thickness_mm * mm3,
        toe_m3: toe_area * e.toe_total_depth_mm * mm3,
        blinding_m3: footprint * s.blinding_mm * mm3,
        footprint_m2: footprint * mm2,
        weep_holes_m: s.weep.count_per_headwall as f64 * s.headwall_thickness_mm / 1000.0,
        weep_holes_nr: s.weep.count_per_headwall,
        excavation_m3: footprint * mm2 * depth_m,
    }
}

/// Measures the culvert.
pub fn measure(input: &CulvertInput, layout: &Layout, bbs: &Bbs) -> Quantities {
    let ogl = match Ogl::from_points(&input.ogl) {
        Ok(o) => o,
        Err(_) => Ogl { points: Vec::new() },
    };
    let len_m = layout.barrel_len_mm / 1000.0;
    let w_sur = layout.surround_width_mm / 1000.0;
    let b = &input.bedding;
    let cells = input.barrel.cells();
    let mut q = Quantities { cells, ..Quantities::default() };
    q.barrel_blinding_m3 = w_sur * b.blinding_mm / 1000.0 * len_m;
    q.bedding_m3 = w_sur * b.bedding_mm / 1000.0 * len_m;
    match input.barrel {
        Barrel::Pipe(p) => {
            q.pipe_length_m = len_m;
            let od = p.outside_diameter_mm() / 1000.0;
            let area = w_sur * (od + b.surround_over_crown_mm / 1000.0) - cells as f64 * PI * od * od / 4.0;
            q.surround_m3 = area.max(0.0) * len_m;
        }
        Barrel::Box(x) => {
            let (ow, oh) = (input.barrel.outer_width_mm() / 1000.0, (x.clear_height_mm + x.top_slab_mm + x.bottom_slab_mm) / 1000.0);
            let hole = cells as f64 * (x.clear_span_mm / 1000.0 * x.clear_height_mm / 1000.0 - 4.0 * 0.5 * (x.haunch_mm / 1000.0).powi(2));
            let area = ow * oh - hole;
            q.box_concrete_m3 = area * len_m;
        }
    }
    q.left = end_quantities(&layout.left, layout, input, &ogl);
    q.right = end_quantities(&layout.right, layout, input, &ogl);
    for e in [&q.left, &q.right] {
        q.headwalls_m3 += e.headwall_m3;
        q.wings_m3 += e.wings_m3;
        q.aprons_m3 += e.apron_m3;
        q.toes_m3 += e.toe_m3;
        q.end_blinding_m3 += e.blinding_m3;
        q.weep_holes_m += e.weep_holes_m;
        q.weep_holes_nr += e.weep_holes_nr;
    }
    q.end_structures_m3 = q.headwalls_m3 + q.wings_m3 + q.aprons_m3 + q.toes_m3;

    // Trench excavation along the barrel, from the original ground to the underside of the blinding.
    let step = input.options.sample_step_mm;
    let n = (layout.barrel_len_mm / step).ceil().max(1.0) as usize;
    let ds = layout.barrel_len_mm / n as f64;
    let width_m = w_sur + 2.0 * b.working_space_mm / 1000.0;
    let sin = layout.skew_deg.to_radians().sin();
    let mut trench = 0.0;
    let mut backfill = 0.0;
    let layer = (b.blinding_mm + b.bedding_mm) / 1000.0;
    for i in 0..n {
        let s = -layout.left_len_mm + (i as f64 + 0.5) * ds;
        let base = layout.invert_at_m(s) - (input.barrel.below_invert_mm() + b.bedding_mm + b.blinding_mm) / 1000.0;
        let depth = (ogl.level_at(s / 1000.0 * sin) - base).max(0.0);
        trench += width_m * depth * ds / 1000.0;
        // What the structure fills of the trench: the blinding and bedding across the surround
        // width, then the surround (pipe) or the box body above them. The rest is backfilled.
        let filled = match input.barrel {
            Barrel::Pipe(p) => w_sur * depth.min(layer + (p.outside_diameter_mm() + b.surround_over_crown_mm) / 1000.0),
            Barrel::Box(x) => {
                let oh = (x.clear_height_mm + x.top_slab_mm + x.bottom_slab_mm) / 1000.0;
                w_sur * depth.min(layer) + input.barrel.outer_width_mm() / 1000.0 * (depth - layer).clamp(0.0, oh)
            }
        };
        backfill += (width_m * depth - filled).max(0.0) * ds / 1000.0;
    }
    q.excavation_m3 = trench + q.left.excavation_m3 + q.right.excavation_m3;
    q.backfill_m3 = backfill;
    q.steel_kg = bbs.total_kg;
    q.lines = lines(input, &q, bbs);
    q
}

fn line(category: &str, description: &str, quantity: f64, unit: &str, attrs: &[(&str, String)]) -> QuantityLine {
    QuantityLine {
        category: category.to_string(),
        description: description.to_string(),
        quantity,
        unit: unit.to_string(),
        attributes: attrs.iter().map(|(k, v)| (k.to_string(), v.clone())).collect(),
    }
}

fn lines(input: &CulvertInput, q: &Quantities, bbs: &Bbs) -> Vec<QuantityLine> {
    let c = &input.concrete;
    let class = |s: &str| [("class", s.to_string())];
    let mut v = Vec::new();
    match input.barrel {
        Barrel::Pipe(p) => {
            for cell in 1..=q.cells {
                v.push(line("culvert.pipe_length", &format!("{:.0} mm concrete pipe, cell {cell}", p.internal_diameter_mm), q.pipe_length_m, "m", &[("internal_diameter_mm", format!("{:.0}", p.internal_diameter_mm)), ("cell", cell.to_string())]));
            }
            v.push(line("culvert.surround", "Concrete surround to pipes", q.surround_m3, "m3", &class(&c.surround)));
        }
        Barrel::Box(b) => {
            v.push(line("culvert.box_concrete", &format!("Box culvert {:.0} x {:.0} mm, {} cell(s)", b.clear_span_mm, b.clear_height_mm, b.cells), q.box_concrete_m3, "m3", &class(&c.barrel)));
        }
    }
    v.push(line("culvert.blinding_barrel", "Blinding under the barrel", q.barrel_blinding_m3, "m3", &class(&c.blinding)));
    v.push(line("culvert.bedding", "Bedding under the barrel", q.bedding_m3, "m3", &class(&c.bedding)));
    v.push(line("culvert.blinding_end", "Blinding under the end structures", q.end_blinding_m3, "m3", &class(&c.blinding)));
    v.push(line("culvert.headwall", "Headwalls", q.headwalls_m3, "m3", &class(&c.headwall)));
    v.push(line("culvert.wing", "Wing walls", q.wings_m3, "m3", &class(&c.wing)));
    v.push(line("culvert.apron", "Aprons", q.aprons_m3, "m3", &class(&c.apron)));
    v.push(line("culvert.toe", "Toe beams", q.toes_m3, "m3", &class(&c.toe)));
    v.push(line("culvert.end_structures", "End structures, total", q.end_structures_m3, "m3", &[]));
    v.push(line("culvert.excavation", "Excavation for the barrel and end structures", q.excavation_m3, "m3", &[]));
    v.push(line("culvert.backfill", "Backfill of the barrel trench", q.backfill_m3, "m3", &[]));
    v.push(line("culvert.weep_holes", "Weep holes, length", q.weep_holes_m, "m", &[("diameter_mm", format!("{:.0}", input.left_end.weep.diameter_mm))]));
    v.push(line("culvert.weep_holes_nr", "Weep holes, number", q.weep_holes_nr as f64, "nr", &[("diameter_mm", format!("{:.0}", input.left_end.weep.diameter_mm))]));
    for (d, kg) in &bbs.by_diameter_kg {
        v.push(line("culvert.steel", &format!("Reinforcement Y{d}"), *kg, "kg", &[("diameter_mm", d.to_string())]));
    }
    v
}

impl Quantities {
    /// The line of a category, if there is one (the first, for categories with several lines).
    pub fn find(&self, category: &str) -> Option<&QuantityLine> {
        self.lines.iter().find(|l| l.category == category)
    }
}
