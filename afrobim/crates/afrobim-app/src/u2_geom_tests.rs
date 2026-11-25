// SPDX-License-Identifier: LicenseRef-AfroBIM-Proprietary
// Copyright (c) 2026 AfroBIM. All rights reserved.
//! Glue test: the solids of each second-set marking land where the alignment puts the station.

use afrobim_kernel::{Document, ModuleRegistry};

use crate::alignment_glue;
use crate::ext_markings_glue::{self as glue, ExtRequest};

#[test]
fn the_solids_of_hatching_and_a_stop_line_lie_at_their_station() {
    let mut doc = Document::default();
    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../afrobim-alignment/tests/landxml/corridor.xml");
    alignment_glue::import_landxml(&mut doc, &path).unwrap();
    let (a, _) = alignment_glue::alignments(&doc).remove(0);
    let al = alignment_glue::alignment_of(&doc, a).unwrap();
    let s0 = al.start_station_m;
    // Arbitrary test values.
    let mut hatch = ExtRequest::new(afrobim_markings::Basis::Uk, "1040", s0 + 300.0, s0 + 360.0);
    hatch.strip_width_mm = Some(3000);
    hatch.width_mm = Some(100);
    hatch.speed = Some(40);
    let mut stop = ExtRequest::new(afrobim_markings::Basis::Uk, "1002.1", s0 + 400.0, s0 + 400.0);
    stop.offset_m = -3.5;
    let names = [glue::add(&mut doc, a, &hatch).unwrap(), glue::add(&mut doc, a, &stop).unwrap()];
    let mut modules = ModuleRegistry::default();
    glue::register(&mut modules);
    for (name, station) in names.iter().zip([s0 + 330.0, s0 + 400.0]) {
        let e = doc.elements().iter().find(|e| &e.name == name).unwrap();
        let site = e.site.as_ref().unwrap();
        let g = modules.generate(e).unwrap();
        let (mut lo, mut hi) = ([f64::INFINITY; 2], [f64::NEG_INFINITY; 2]);
        for m in &g.solids {
            for v in &m.vertices {
                for k in 0..2 {
                    lo[k] = lo[k].min(v[k]);
                    hi[k] = hi[k].max(v[k]);
                }
            }
        }
        // The solids are in millimetres from the placement's origin; the placement is in millimetres.
        let centre = ((site.x + (lo[0] + hi[0]) / 2.0) / 1000.0, (site.y + (lo[1] + hi[1]) / 2.0) / 1000.0);
        let want = al.point_at(station).unwrap();
        let d = ((centre.0 - want.x).powi(2) + (centre.1 - want.y).powi(2)).sqrt();
        eprintln!("{name}: solids centre {centre:?}, alignment point {:?}, distance {d:.2} m, site {:?}", (want.x, want.y), (site.x, site.y));
        assert!(d < 40.0, "{name}: the solids are {d:.1} m from the station");
        // The level: every solid lies within a thickness of the road level at the stations it
        // covers (the marking's own two ends bracket them on this gentle profile; 50 mm of slack for
        // a line across the road, whose ends are at slightly different stations on a bend), not at zero.
        let profile = al.profile.as_ref().unwrap();
        let (first, last) = (e.site.as_ref().unwrap().station_m.unwrap(), station * 2.0 - e.site.as_ref().unwrap().station_m.unwrap());
        let levels: Vec<f64> = (0..=20).map(|k| profile.elevation_at((first + (last - first) * k as f64 / 20.0).clamp(1000.0, 1900.0)).unwrap() * 1000.0).collect();
        let (lvl_lo, lvl_hi) = levels.iter().fold((f64::INFINITY, f64::NEG_INFINITY), |(a, b), v| (a.min(*v), b.max(*v)));
        let mut z = (f64::INFINITY, f64::NEG_INFINITY);
        for m in &g.solids {
            for v in &m.vertices {
                z = (z.0.min(v[2]), z.1.max(v[2]));
            }
        }
        eprintln!("{name}: solids z {z:?}, road level over its stations {lvl_lo:.0} to {lvl_hi:.0} mm");
        assert!(z.0 >= lvl_lo - 50.0 && z.1 <= lvl_hi + afrobim_markings::DISPLAY_THICKNESS_MM + 50.0, "{name}: solids at {z:?} mm, the road is at {lvl_lo:.0} to {lvl_hi:.0} mm");
    }
}
