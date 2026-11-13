//! Placing signs on an alignment, and the alignment as a kernel element.

use std::f64::consts::FRAC_PI_2;

use afrobim_alignment::{
    location_report, place, plan_drawing, Alignment, AlignmentBuilder, AlignmentModule, PlacementError, SignLocation, SignSide, TrafficSide, Turn,
    CATEGORY,
};
use afrobim_draw::Primitive;
use afrobim_kernel::{Document, Module, ModuleRegistry, Pt, Severity};

fn assert_close(got: f64, want: f64, tol: f64, what: &str) {
    assert!((got - want).abs() <= tol, "{what}: got {got}, want {want} (off by {:e})", got - want);
}

fn location(station: f64, offset: Option<f64>, side: Option<SignSide>) -> SignLocation {
    SignLocation { alignment: Some("MC01".into()), station: Some(station), offset, side }
}

/// East for 200 m from (1000, 2000) at station 100, then a quarter circle to the left, R = 100.
fn road() -> Alignment {
    AlignmentBuilder::new("MC01", 100.0, 1000.0, 2000.0, 0.0).line(200.0).arc(100.0, 100.0 * FRAC_PI_2, Turn::Left).build().unwrap()
}

#[test]
fn place_combines_the_geometry_with_the_placement_rules() {
    let a = road();
    // Left of an eastbound line is north. Left-hand traffic: a left-side sign serves traffic
    // travelling east (up-station), so rotation = 0° − 90° = 270°.
    let p = place(&a, &location(150.0, Some(-3.0), None), TrafficSide::Left).unwrap();
    assert_eq!(p.warning, None);
    assert_eq!(p.positions.len(), 1);
    let s = p.positions[0];
    assert_eq!(s.side, SignSide::Left);
    assert_close(s.x_mm, 1_050_000.0, 1e-6, "x");
    assert_close(s.y_mm, 2_003_000.0, 1e-6, "y");
    assert_close(s.rotation_deg, 270.0, 1e-9, "rotation");
    assert_eq!((s.station_m, s.offset_m), (150.0, -3.0));

    // Both sides at the end of the curve, where the road heads north: left is west, right is east.
    let end = a.end_station_m();
    let p = place(&a, &location(end, Some(4.0), Some(SignSide::Both)), TrafficSide::Left).unwrap();
    assert_eq!(p.positions.len(), 2);
    let (l, r) = (p.positions[0], p.positions[1]);
    assert_eq!((l.side, r.side), (SignSide::Left, SignSide::Right));
    assert_close(l.x_mm, 1_296_000.0, 1e-6, "left x");
    assert_close(r.x_mm, 1_304_000.0, 1e-6, "right x");
    assert_close(l.y_mm, 2_100_000.0, 1e-6, "left y");
    assert_close(r.y_mm, 2_100_000.0, 1e-6, "right y");
    assert_close(l.rotation_deg, 0.0, 1e-9, "left serves northbound traffic: 90° − 90°");
    assert_close(r.rotation_deg, 180.0, 1e-9, "right serves southbound traffic: 270° − 90°");
    // Right-hand traffic swaps the two facings.
    let q = place(&a, &location(end, Some(4.0), Some(SignSide::Both)), TrafficSide::Right).unwrap();
    assert_close(q.positions[0].rotation_deg, 180.0, 1e-9, "left");
    assert_close(q.positions[1].rotation_deg, 0.0, 1e-9, "right");

    // On the curve, the placed point reads back as the station and offset it was given.
    let p = place(&a, &location(350.0, Some(5.5), None), TrafficSide::Left).unwrap();
    let s = p.positions[0];
    let (station, offset) = a.station_offset(s.x_mm / 1000.0, s.y_mm / 1000.0).unwrap();
    assert_close(station, 350.0, 1e-8, "station");
    assert_close(offset, 5.5, 1e-8, "offset");
    let heading_deg = a.point_at(350.0).unwrap().heading_rad.to_degrees(); // 28.6…° into the curve
    assert_close(s.rotation_deg, heading_deg + 180.0 - 90.0, 1e-9, "right-side sign faces down-station traffic");

    // The side wins over the offset's sign, with the reference's warning.
    let p = place(&a, &location(150.0, Some(2.5), Some(SignSide::Left)), TrafficSide::Left).unwrap();
    assert_eq!(p.positions[0].offset_m, -2.5);
    assert_eq!(p.warning.as_deref(), Some("offset 2.5 m is on the other side from side=left; placed at -2.5 m"));

    // What cannot be placed.
    assert_eq!(place(&a, &location(150.0, None, None), TrafficSide::Left), Err(PlacementError::NeedsSide));
    assert_eq!(place(&a, &SignLocation::default(), TrafficSide::Left), Err(PlacementError::NoStation));
    let off = place(&a, &location(99.0, Some(1.0), None), TrafficSide::Left).unwrap_err();
    assert_eq!(off.to_string(), "station 0+099.000 is off alignment 'MC01', which runs from 0+100.000 to 0+457.080");
    assert!(location(1.0, None, None).is_placeable() && !SignLocation::default().is_placeable());

    // Ready for the kernel, and for the location report.
    let site = s.site_placement(&a.name);
    assert_eq!((site.x, site.y, site.z, site.rotation_deg), (s.x_mm, s.y_mm, 0.0, s.rotation_deg));
    assert_eq!((site.alignment.as_deref(), site.station_m, site.offset_m), (Some("MC01"), Some(350.0), Some(5.5)));
    let csv = location_report::write(&[s.placed_sign("R61", &a.name), l.placed_sign("W1,a", &a.name)]);
    let lines: Vec<&str> = csv.lines().collect();
    assert_eq!(lines[0], location_report::HEADER);
    assert!(lines[1].starts_with("R61,MC01,350.000,5.500,right,"), "{csv}");
    assert_eq!(lines[2], "\"W1,a\",MC01,457.080,-4.000,left,1296.000,2100.000,0.0000");
}

#[test]
fn the_module_draws_the_plan_at_the_origin_and_says_where_the_origin_is() {
    let a = road();
    let mut doc = Document::default();
    let id = doc.add(CATEGORY, "MC01", Pt::new(0.0, 0.0), 0.0, 0.0, "Alignments", serde_json::to_value(&a).unwrap());
    let mut registry = ModuleRegistry::default();
    registry.register(Box::new(AlignmentModule));
    assert_eq!(AlignmentModule.category(), "alignment");
    let g = registry.generate(doc.find(id).unwrap()).unwrap();

    let d = &g.derived;
    assert_eq!(d["length_m"], "357.080");
    assert_eq!((d["start_station"].as_str(), d["end_station"].as_str()), ("0+100.000", "0+457.080"));
    assert_eq!((d["start_station_m"].as_str(), d["end_station_m"].as_str()), ("100.000", "457.080"));
    assert_eq!((d["elements"].as_str(), d["lines"].as_str(), d["arcs"].as_str(), d["spirals"].as_str()), ("2", "1", "1", "0"));
    assert_eq!(d["min_radius_m"], "100.000");
    assert_eq!(d["tick_interval_m"], "10");
    assert!(g.notes.is_empty() && g.solids.is_empty());

    // Everything is hairlines inside the extents, whose lower-left corner is the origin.
    let plan = plan_drawing(&a).unwrap();
    assert_eq!(plan.drawing, g.drawing);
    let ext = g.drawing.extents;
    assert_eq!((ext.min.x, ext.min.y), (0.0, 0.0));
    let (mut lo_x, mut lo_y, mut hi_x, mut hi_y) = (f64::MAX, f64::MAX, f64::MIN, f64::MIN);
    for p in &g.drawing.primitives {
        let Primitive::Line { from, to, width, colour } = p else { panic!("{p:?}") };
        assert_eq!((*width, *colour), (0.0, None));
        for q in [from, to] {
            (lo_x, lo_y, hi_x, hi_y) = (lo_x.min(q.x), lo_y.min(q.y), hi_x.max(q.x), hi_y.max(q.y));
        }
    }
    assert_eq!((lo_x, lo_y), (0.0, 0.0));
    assert_eq!((hi_x, hi_y), (ext.max.x, ext.max.y));

    // Drawing millimetres + origin = plan metres. The first primitive starts at the start of the alignment.
    let Primitive::Line { from, to, .. } = &g.drawing.primitives[0] else { unreachable!() };
    assert_close(plan.origin_x_m + from.x / 1000.0, 1000.0, 1e-9, "start easting");
    assert_close(plan.origin_y_m + from.y / 1000.0, 2000.0, 1e-9, "start northing");
    assert_close(plan.origin_x_m + to.x / 1000.0, 1200.0, 1e-9, "end of the straight");
    assert_eq!(d["plan_origin_x_m"], format!("{:.6}", plan.origin_x_m));
    assert_eq!(d["plan_origin_y_m"], format!("{:.6}", plan.origin_y_m));
    // The extent is the road's own box grown by the ticks: 2 m at the major tick on station 100
    // (west and south of the start) and at station 400 on the curve.
    assert_close(plan.origin_x_m, 1000.0, 1e-9, "origin easting");
    assert_close(plan.origin_y_m, 1998.0, 1e-9, "origin northing: a 2 m tick south of the start");
    assert_close(ext.max.y, 102_000.0, 1e-6, "height: 100 m of rise plus the tick");

    // Ticks at 100, 110, …, 450: 36 of them, square to the road, 2 m long (4 m on the hundreds).
    let chords = a.polyline(plan.max_chord_m).len() - 1;
    let ticks = &g.drawing.primitives[chords..];
    assert_eq!(ticks.len(), 36);
    for (i, t) in ticks.iter().enumerate() {
        let Primitive::Line { from, to, .. } = t else { unreachable!() };
        let station = 100.0 + 10.0 * i as f64;
        let at = a.point_at(station).unwrap();
        let mid = ((from.x + to.x) / 2000.0 + plan.origin_x_m, (from.y + to.y) / 2000.0 + plan.origin_y_m);
        assert_close(mid.0, at.x, 1e-9, "tick is centred on the road");
        assert_close(mid.1, at.y, 1e-9, "tick is centred on the road");
        let along = (to.x - from.x) * at.heading_rad.cos() + (to.y - from.y) * at.heading_rad.sin();
        assert_close(along, 0.0, 1e-6, "tick is square to the road");
        let length_m = from.distance_to(*to) / 1000.0;
        assert_close(length_m, if (station / 100.0).fract() == 0.0 { 4.0 } else { 2.0 }, 1e-9, "tick length");
    }
    // Chords stay within 5 mm of the arc: sagitta c²/8R.
    assert!(plan.max_chord_m.powi(2) / (8.0 * 100.0) <= 0.005 + 1e-12);

    // A longer road takes a wider interval; the count stays at or under fifty.
    let long = AlignmentBuilder::new("long", 0.0, 0.0, 0.0, 1.0).line(12_345.0).build().unwrap();
    assert_eq!(plan_drawing(&long).unwrap().tick_interval_m, 500.0);
    let short = AlignmentBuilder::new("short", 0.0, 0.0, 0.0, 1.0).line(30.0).build().unwrap();
    assert_eq!(plan_drawing(&short).unwrap().tick_interval_m, 1.0);
}

#[test]
fn the_module_reports_breaks_and_refuses_what_is_not_an_alignment() {
    let mut doc = Document::default();
    let kinked = Alignment::from_pis("k", 0.0, &[afrobim_alignment::Pi::point(0.0, 0.0), afrobim_alignment::Pi::point(100.0, 0.0), afrobim_alignment::Pi::point(100.0, 50.0)]).unwrap();
    let id = doc.add(CATEGORY, "K", Pt::new(0.0, 0.0), 0.0, 0.0, "0", serde_json::to_value(&kinked).unwrap());
    let g = AlignmentModule.generate(doc.find(id).unwrap()).unwrap();
    assert_eq!(g.notes.len(), 1);
    assert_eq!(g.notes[0].severity, Severity::Warning);
    assert_eq!(g.notes[0].code, "alignment.joint");
    assert_eq!(g.notes[0].message, "At station 0+100.000 element 2 starts 0.000 m from the end of the one before and turns 90.0000° from it.");
    assert_eq!(g.derived["min_radius_m"], "none");

    let bad = doc.add(CATEGORY, "B", Pt::new(0.0, 0.0), 0.0, 0.0, "0", serde_json::json!({"speed_kph": 60}));
    assert!(AlignmentModule.generate(doc.find(bad).unwrap()).unwrap_err().contains("not a valid alignment"));
    let empty = doc.add(CATEGORY, "E", Pt::new(0.0, 0.0), 0.0, 0.0, "0", serde_json::json!({"name": "e", "elements": []}));
    assert_eq!(AlignmentModule.generate(doc.find(empty).unwrap()).unwrap_err(), "Alignment 'E' has no length.");
    // Hand-written data needs only the start, heading and length of a straight.
    let minimal = doc.add(CATEGORY, "M", Pt::new(0.0, 0.0), 0.0, 0.0, "0", serde_json::json!({"name": "m", "elements": [{"x": 0, "y": 0, "heading_rad": 0, "length": 100}]}));
    assert_eq!(AlignmentModule.generate(doc.find(minimal).unwrap()).unwrap().derived["length_m"], "100.000");
}
