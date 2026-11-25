// SPDX-License-Identifier: LicenseRef-AfroBIM-Proprietary
// Copyright (c) 2026 AfroBIM. All rights reserved.
//! Collectors: each turns one crate's quantity output into [`QuantityLine`]s.
//!
//! One collector per crate. Each measures the document's elements of that crate's category one at
//! a time through the crate's own take-off, so every line carries the element it came from. The
//! numbers are the crate's own; nothing is recomputed here. An element a crate cannot measure
//! goes to [`QuantitySet::skipped`] with the crate's reason.
//!
//! Lines of one element that measure the same thing in different ways (a post's length in metres
//! and the same post as a count of one, a sign's face area and its count) are alternatives for
//! the bill to choose between by mapping rule; mapping both to priced items would price the
//! element twice.

use std::iter::once;

use afrobim_corridor::{corridor_element, Earthworks, CORRIDOR_CATEGORY};
use afrobim_drainage::{DrainageQuantities, CULVERT_CATEGORY, DRAIN_CATEGORY};
use afrobim_fabrication::SignQuantity;
use afrobim_gantry::{GantryQuantities, GANTRY_CATEGORY};
use afrobim_kernel::{Document, Element, ElementId};
use afrobim_lighting::LightingQuantities;
use afrobim_markings::MarkingTakeoff;
use afrobim_pavement::{PavementQuantities, PAVEMENT_CATEGORY};
use afrobim_restraint::{EndTreatment, RestraintQuantities};
use afrobim_supports::{QuantityOptions, SupportQuantities, FOUNDATION_CATEGORY, POST_CATEGORY};

use crate::{Decimal, QuantitySet, SkippedSource};

/// The category of sign elements in a document.
pub const SIGN_CATEGORY: &str = "sign";

/// Inputs the collectors need that are not in the document.
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct CollectOptions {
    /// Steel density and foundation working space for the supports take-off.
    pub supports: QuantityOptions,
}

/// Number as attribute text: at most 3 decimals, no trailing zeros (`150`, `12.5`).
fn num(v: f64) -> String {
    Decimal::from_f64(v, 3).map(|d| d.to_string()).unwrap_or_else(|_| v.to_string())
}

fn skip(out: &mut QuantitySet, source: &str, mark: &str, reason: &str) {
    out.skipped.push(SkippedSource { source: source.to_string(), mark: mark.to_string(), reason: reason.to_string() });
}

fn elements_of<'a>(doc: &'a Document, category: &'a str) -> impl Iterator<Item = &'a Element> {
    doc.elements().iter().filter(move |e| e.category == category)
}

/// Every collector below, in order. `signs` are the sign take-off rows
/// (`afrobim_fabrication::measure`) for the document's signs, which need the sign engine and are
/// therefore measured by the caller.
pub fn collect_all(doc: &Document, signs: &[SignQuantity], options: &CollectOptions) -> QuantitySet {
    let mut out = QuantitySet::new();
    collect_signs(doc, signs, &mut out);
    collect_supports(doc, &options.supports, &mut out);
    collect_markings(doc, &mut out);
    collect_drainage(doc, &mut out);
    collect_gantries(doc, &mut out);
    collect_lighting(doc, &mut out);
    collect_restraint(doc, &mut out);
    collect_pavement(doc, &mut out);
    collect_corridors(doc, &mut out);
    out
}

/// Sign faces (`afrobim-fabrication`): per sign `sign.face_area` m2, `sign.face` nr (one) and
/// `sign.legend` nr (characters of lettering). The element is the document's sign element whose
/// name equals the sign's code.
pub fn collect_signs(doc: &Document, signs: &[SignQuantity], out: &mut QuantitySet) {
    for s in signs {
        let element = elements_of(doc, SIGN_CATEGORY).find(|e| e.name == s.code).map(|e| e.id);
        let attrs = [
            ("kind", format!("{:?}", s.kind)),
            ("scheme", format!("{:?}", s.scheme)),
            ("ground", format!("{:?}", s.ground)),
            ("catalog_key", s.catalog_key.clone().unwrap_or_default()),
            ("speed_kph", s.speed_kph.to_string()),
            ("width_mm", num(s.width_mm)),
            ("height_mm", num(s.height_mm)),
        ];
        let desc = format!("Sign face {} ({} x {} mm)", s.code, num(s.width_mm), num(s.height_mm));
        out.push(element, &s.code, "sign.face_area", &desc, s.area_m2(), "m2", &attrs);
        out.push_decimal(element, &s.code, "sign.face", &desc, Decimal::ONE, "nr", &attrs);
        out.push_decimal(element, &s.code, "sign.legend", &format!("Legend characters of {}", s.code), Decimal::from_int(s.legend_characters as i64), "nr", &attrs);
    }
}

/// Posts and foundations (`afrobim-supports`): `post.length` m, `post.count` nr, `post.coating_area`
/// m2, `post.mass` kg; `foundation.concrete`, `foundation.blinding`, `foundation.excavation` m3,
/// `foundation.formwork` m2 (the side area), `foundation.count` nr.
pub fn collect_supports(doc: &Document, options: &QuantityOptions, out: &mut QuantitySet) {
    for e in doc.elements().iter().filter(|e| e.category == POST_CATEGORY || e.category == FOUNDATION_CATEGORY) {
        let q = match SupportQuantities::measure(once(e), options) {
            Ok(q) => q,
            Err(err) => {
                skip(out, "supports", &e.name, &err.to_string());
                continue;
            }
        };
        for s in &q.skipped {
            skip(out, "supports", &s.mark, &s.reason);
        }
        for p in &q.posts {
            let attrs = [
                ("section", p.section.clone()),
                ("material", p.material.clone()),
                ("finish", p.finish.clone()),
                ("length_mm", num(p.length_mm)),
                ("embedment_mm", num(p.embedment_mm)),
            ];
            let d = format!("Post {} {}", p.mark, p.section);
            out.push(Some(e.id), &p.mark, "post.length", &d, p.length_mm / 1000.0, "m", &attrs);
            out.push_decimal(Some(e.id), &p.mark, "post.count", &d, Decimal::ONE, "nr", &attrs);
            out.push(Some(e.id), &p.mark, "post.coating_area", &format!("{d}, coating"), p.coating_area_m2, "m2", &attrs);
            out.push(Some(e.id), &p.mark, "post.mass", &format!("{d}, mass"), p.mass_kg, "kg", &attrs);
        }
        for f in &q.foundations {
            let attrs = [("shape", f.shape.clone()), ("concrete", f.concrete.clone()), ("top_below_ground_mm", num(f.top_below_ground_mm))];
            let d = format!("Foundation {} {}", f.mark, f.shape);
            out.push(Some(e.id), &f.mark, "foundation.concrete", &format!("{d}, concrete"), f.concrete_m3, "m3", &attrs);
            out.push(Some(e.id), &f.mark, "foundation.blinding", &format!("{d}, blinding"), f.blinding_m3, "m3", &attrs);
            out.push(Some(e.id), &f.mark, "foundation.excavation", &format!("{d}, excavation"), f.excavation_m3, "m3", &attrs);
            out.push(Some(e.id), &f.mark, "foundation.formwork", &format!("{d}, side area"), f.side_area_m2, "m2", &attrs);
            out.push_decimal(Some(e.id), &f.mark, "foundation.count", &d, Decimal::ONE, "nr", &attrs);
        }
    }
}

/// Road markings (`afrobim-markings`): `marking.length` m (overall length), `marking.painted_length`
/// m, `marking.painted_area` m2, `marking.marks` nr (whole marks of a pattern).
pub fn collect_markings(doc: &Document, out: &mut QuantitySet) {
    for e in elements_of(doc, afrobim_markings::CATEGORY) {
        let t = MarkingTakeoff::measure(std::slice::from_ref(e));
        for s in &t.skipped {
            skip(out, "marking", &s.mark, &s.reason);
        }
        for r in &t.rows {
            let attrs = [
                ("line_type", r.line_type.clone()),
                ("diagram", r.diagram.clone()),
                ("width_mm", r.width_mm.to_string()),
                ("colour", r.colour.label().to_string()),
                ("material", r.material.clone()),
                ("alignment", r.alignment.clone()),
            ];
            let d = format!("{} {} mm {}", r.description, r.width_mm, r.colour.label());
            out.push(Some(e.id), &r.mark, "marking.length", &format!("{d}, overall length"), r.overall_length_m, "m", &attrs);
            out.push(Some(e.id), &r.mark, "marking.painted_length", &format!("{d}, painted length"), r.painted_length_m, "m", &attrs);
            out.push(Some(e.id), &r.mark, "marking.painted_area", &format!("{d}, painted area"), r.painted_area_m2, "m2", &attrs);
            out.push_decimal(Some(e.id), &r.mark, "marking.marks", &format!("{d}, marks"), Decimal::from_int(r.marks as i64), "nr", &attrs);
        }
    }
}

/// Side drains and pipe culverts (`afrobim-drainage`): `drain.length` m, `drain.excavation` m3,
/// `drain.lining_area` m2, `drain.lining_volume` m3 (the last two only when lined);
/// `culvert.length` m (all barrels) and `culvert.barrels` nr.
pub fn collect_drainage(doc: &Document, out: &mut QuantitySet) {
    for e in doc.elements().iter().filter(|e| e.category == DRAIN_CATEGORY || e.category == CULVERT_CATEGORY) {
        let q = DrainageQuantities::measure(once(e));
        for s in &q.skipped {
            skip(out, "drainage", &s.mark, &s.reason);
        }
        for d in &q.drains {
            let attrs = [
                ("section", d.section.clone()),
                ("lining", d.lining.clone()),
                ("lining_thickness_mm", num(d.lining_thickness_mm)),
                ("alignment", d.alignment.clone()),
            ];
            let n = format!("Drain {} {}", d.mark, d.section);
            out.push(Some(e.id), &d.mark, "drain.length", &n, d.length_m, "m", &attrs);
            out.push(Some(e.id), &d.mark, "drain.excavation", &format!("{n}, excavation"), d.excavation_m3, "m3", &attrs);
            if d.lining_area_m2 > 0.0 {
                out.push(Some(e.id), &d.mark, "drain.lining_area", &format!("{n}, lining area"), d.lining_area_m2, "m2", &attrs);
                out.push(Some(e.id), &d.mark, "drain.lining_volume", &format!("{n}, lining volume"), d.lining_volume_m3, "m3", &attrs);
            }
        }
        for c in &q.culverts {
            let attrs = [
                ("internal_diameter_mm", num(c.internal_diameter_mm)),
                ("wall_thickness_mm", num(c.wall_thickness_mm)),
                ("material", c.material.clone()),
                ("barrels", c.barrels.to_string()),
                ("alignment", c.alignment.clone()),
            ];
            let n = format!("Culvert {} DN{}", c.mark, num(c.internal_diameter_mm));
            out.push(Some(e.id), &c.mark, "culvert.length", &n, c.total_length_m, "m", &attrs);
            out.push_decimal(Some(e.id), &c.mark, "culvert.barrels", &format!("{n}, barrels"), Decimal::from_int(i64::from(c.barrels)), "nr", &attrs);
        }
    }
}

/// Sign gantries and cantilevers (`afrobim-gantry`): `gantry.count` nr, `gantry.leg_length` m (all
/// legs), `gantry.boom_length` m, `gantry.foundation_concrete` m3, `gantry.sign_area` m2, and
/// `gantry.steel_mass` kg only when every member has a mass per metre.
pub fn collect_gantries(doc: &Document, out: &mut QuantitySet) {
    for e in elements_of(doc, GANTRY_CATEGORY) {
        let (rows, skipped) = GantryQuantities::measure_all(once(e));
        for (mark, reason) in &skipped {
            skip(out, "gantry", mark, reason);
        }
        for g in &rows {
            let attrs = [
                ("gantry_type", g.gantry_type.label().to_string()),
                ("alignment", g.alignment.clone()),
                ("span_or_reach_mm", num(g.span_or_reach_mm)),
                ("legs", g.leg_lengths_mm.len().to_string()),
            ];
            let n = format!("Gantry {} ({})", g.mark, g.gantry_type.label());
            out.push_decimal(Some(e.id), &g.mark, "gantry.count", &n, Decimal::ONE, "nr", &attrs);
            out.push(Some(e.id), &g.mark, "gantry.leg_length", &format!("{n}, legs"), g.leg_length_total_mm() / 1000.0, "m", &attrs);
            out.push(Some(e.id), &g.mark, "gantry.boom_length", &format!("{n}, boom"), g.boom_length_mm / 1000.0, "m", &attrs);
            out.push(Some(e.id), &g.mark, "gantry.foundation_concrete", &format!("{n}, foundation concrete"), g.foundation_concrete_m3, "m3", &attrs);
            out.push(Some(e.id), &g.mark, "gantry.sign_area", &format!("{n}, sign area"), g.sign_area_m2, "m2", &attrs);
            if let Some(m) = g.steel_mass_kg {
                out.push(Some(e.id), &g.mark, "gantry.steel_mass", &format!("{n}, steel"), m, "kg", &attrs);
            }
        }
    }
}

/// Street lighting (`afrobim-lighting`): per installation `lighting.column` nr by mounting height
/// and arrangement, `lighting.luminaire` nr by product and power.
pub fn collect_lighting(doc: &Document, out: &mut QuantitySet) {
    for e in elements_of(doc, afrobim_lighting::CATEGORY) {
        let q = LightingQuantities::of_elements(once(e));
        for s in &q.skipped {
            skip(out, "lighting", &s.name, &s.reason);
        }
        for c in &q.columns {
            let attrs = [("mounting_height_m", num(c.mounting_height_m)), ("arrangement", c.arrangement.clone())];
            let d = format!("Lighting columns {}, {} m, {}", e.name, num(c.mounting_height_m), c.arrangement);
            out.push_decimal(Some(e.id), &e.name, "lighting.column", &d, Decimal::from_int(c.columns as i64), "nr", &attrs);
        }
        for l in &q.luminaires {
            let attrs = [("product", l.product.clone()), ("power_w", num(l.power_w))];
            let d = format!("Luminaires {}, {} {} W", e.name, l.product, num(l.power_w));
            out.push_decimal(Some(e.id), &e.name, "lighting.luminaire", &d, Decimal::from_int(l.luminaires as i64), "nr", &attrs);
        }
    }
}

/// Vehicle restraint systems (`afrobim-restraint`): `barrier.length` m, `barrier.posts` nr,
/// `barrier.terminal` nr (one line per terminal end) and `barrier.transition` nr (one line per
/// transition).
pub fn collect_restraint(doc: &Document, out: &mut QuantitySet) {
    for e in elements_of(doc, afrobim_restraint::CATEGORY) {
        let q = RestraintQuantities::measure(once(e));
        for s in &q.skipped {
            skip(out, "restraint", &s.mark, &s.reason);
        }
        for r in &q.runs {
            let attrs = [
                ("system", r.system.clone()),
                ("containment", r.containment.label().to_string()),
                ("working_width", r.working_width.label().to_string()),
                ("impact_severity", r.impact_severity.clone()),
                ("traffic_face", r.traffic_face.clone()),
                ("alignment", r.alignment.clone()),
            ];
            let n = format!("Barrier {} {} {}", r.mark, r.system, r.containment.label());
            out.push(Some(e.id), &r.mark, "barrier.length", &n, r.length_m, "m", &attrs);
            out.push_decimal(Some(e.id), &r.mark, "barrier.posts", &format!("{n}, posts"), Decimal::from_int(r.posts as i64), "nr", &attrs);
            for end in [&r.start_end, &r.end_end] {
                match end {
                    EndTreatment::Terminal { performance_class, description, .. } => {
                        let mut a = attrs.to_vec();
                        a.push(("terminal_class", performance_class.label()));
                        a.push(("terminal_description", description.clone()));
                        out.push_decimal(Some(e.id), &r.mark, "barrier.terminal", &format!("{n}, terminal {}", performance_class.label()), Decimal::ONE, "nr", &a);
                    }
                    EndTreatment::ConnectsTo { system, transition: Some(t), .. } => {
                        let mut a = attrs.to_vec();
                        a.push(("connects_to", system.label().to_string()));
                        a.push(("transition_containment", t.containment.map(|c| c.label().to_string()).unwrap_or_default()));
                        out.push_decimal(Some(e.id), &r.mark, "barrier.transition", &format!("{n}, transition to {}", system.label()), Decimal::ONE, "nr", &a);
                    }
                    _ => {}
                }
            }
        }
    }
}

/// Pavement layers (`afrobim-pavement`): for each layer of each strip `pavement.layer_area` m2 and
/// `pavement.layer_volume` m3 (compacted).
pub fn collect_pavement(doc: &Document, out: &mut QuantitySet) {
    for e in elements_of(doc, PAVEMENT_CATEGORY) {
        let q = PavementQuantities::measure(once(e));
        for s in &q.skipped {
            skip(out, "pavement", &s.mark, &s.reason);
        }
        for r in &q.rows {
            let attrs = [
                ("strip", r.strip.clone()),
                ("layer", r.layer.clone()),
                ("material", r.material.clone()),
                ("thickness_mm", num(r.thickness_mm)),
            ];
            let n = format!("{} {} ({}, {} mm), {}", r.mark, r.layer, r.material, num(r.thickness_mm), r.strip);
            out.push(Some(e.id), &r.mark, "pavement.layer_area", &n, r.area_m2, "m2", &attrs);
            out.push(Some(e.id), &r.mark, "pavement.layer_volume", &format!("{n}, volume"), r.volume_m3, "m3", &attrs);
        }
    }
}

/// Earthworks of corridor elements (`afrobim-corridor`): `earthworks.cut` and `earthworks.fill` m3
/// from each corridor's stored summary (`CorridorSummary`). A corridor not yet computed is skipped.
pub fn collect_corridors(doc: &Document, out: &mut QuantitySet) {
    for e in elements_of(doc, CORRIDOR_CATEGORY) {
        let Some(c) = corridor_element(doc, e.id) else {
            skip(out, "corridor", &e.name, "the corridor data cannot be read");
            continue;
        };
        let Some(s) = c.computed else {
            skip(out, "corridor", &e.name, "the corridor has not been computed");
            continue;
        };
        push_earthworks(out, Some(e.id), &e.name, s.total_cut_m3, s.total_fill_m3);
    }
}

/// Earthworks from an [`Earthworks`] result you hold (for instance with your own volume options),
/// attributed to a corridor element.
pub fn collect_earthworks(element: ElementId, mark: &str, earthworks: &Earthworks, out: &mut QuantitySet) {
    push_earthworks(out, Some(element), mark, earthworks.total_cut_m3, earthworks.total_fill_m3);
}

fn push_earthworks(out: &mut QuantitySet, element: Option<ElementId>, mark: &str, cut: f64, fill: f64) {
    let attrs = [("corridor", mark.to_string())];
    out.push(element, mark, "earthworks.cut", &format!("Earthworks {mark}, cut"), cut, "m3", &attrs);
    out.push(element, mark, "earthworks.fill", &format!("Earthworks {mark}, fill"), fill, "m3", &attrs);
}
