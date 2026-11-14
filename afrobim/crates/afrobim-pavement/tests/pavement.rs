//! Model, geometry, quantities and the kernel module, against closed-form values.

use afrobim_alignment::{Alignment, AlignmentBuilder, Turn};
use afrobim_draw::Primitive;
use afrobim_kernel::{Document, Module, Severity};
use afrobim_pavement::*;

fn close(a: f64, b: f64, tol: f64) -> bool {
    (a - b).abs() <= tol
}

fn straight(length: f64) -> Alignment {
    AlignmentBuilder::new("A1", 1000.0, 500_000.0, 30_000.0, 0.3).line(length).build().unwrap()
}

/// 100 m of left-hand curve of radius 200 m: the heading turns 0.5 rad.
fn arc() -> Alignment {
    AlignmentBuilder::new("C1", 0.0, 0.0, 0.0, 0.0).arc(200.0, 100.0, Turn::Left).build().unwrap()
}

fn stack() -> Vec<Layer> {
    vec![
        Layer::new("Surfacing", Material::HotMixAsphalt, 50.0),
        Layer::new("Base", Material::GranularBase, 150.0),
        Layer::new("Subbase", Material::GranularSubbase, 200.0),
    ]
}

fn shoulder(width: f64) -> Shoulder {
    Shoulder { width_m: width, crossfall_percent: 4.0, layers: ShoulderLayers::SameAsCarriageway }
}

fn road(alignment: Alignment) -> PavementDefinition {
    let mut def = PavementDefinition::new(alignment, 7.0, 2.5, stack());
    def.left_shoulder = Some(shoulder(1.5));
    def.right_shoulder = Some(shoulder(1.5));
    def
}

fn row<'a>(rows: &'a [LayerQuantity], strip: &str, layer: &str) -> &'a LayerQuantity {
    rows.iter().find(|r| r.strip == strip && r.layer == layer).unwrap()
}

#[test]
fn straight_quantities_are_width_times_length_times_thickness() {
    let rows = measure_definition("P1", &road(straight(100.0))).unwrap();
    assert_eq!(rows.len(), 9);
    let base = row(&rows, "Carriageway", "Base");
    assert!(close(base.area_m2, 700.0, 1e-9) && close(base.volume_m3, 105.0, 1e-9) && close(base.length_m, 100.0, 1e-12));
    assert_eq!(base.heading_change_rad, 0.0);
    for side in ["Left shoulder", "Right shoulder"] {
        let s = row(&rows, side, "Subbase");
        assert!(close(s.area_m2, 150.0, 1e-9) && close(s.volume_m3, 30.0, 1e-9));
    }
    let total: f64 = rows.iter().map(|r| r.volume_m3).sum();
    assert!(close(total, 10.0 * 100.0 * 0.4, 1e-9));
}

#[test]
fn on_a_curve_an_offset_strip_is_not_width_times_centreline_length() {
    let rows = measure_definition("P1", &road(arc())).unwrap();
    // The carriageway is centred on the alignment: width × length whatever the curvature.
    let c = row(&rows, "Carriageway", "Base");
    assert!(close(c.heading_change_rad, 0.5, 1e-12) && close(c.area_m2, 700.0, 1e-9));
    // The shoulders are sectors of an annulus about the curve centre: ½·Δθ·(R₂² − R₁²).
    // Left turn, so the right shoulder is outside (radii 203.5 to 205) and the left inside (195 to 196.5).
    let right = row(&rows, "Right shoulder", "Base");
    let left = row(&rows, "Left shoulder", "Base");
    assert!(close(right.area_m2, 0.25 * (205.0f64.powi(2) - 203.5f64.powi(2)), 1e-9));
    assert!(close(left.area_m2, 0.25 * (196.5f64.powi(2) - 195.0f64.powi(2)), 1e-9));
    assert!(close(right.area_m2, 153.1875, 1e-9) && close(left.area_m2, 146.8125, 1e-9));
    assert!(close(right.strip_length_m, 100.0 + 4.25 * 0.5, 1e-12));
    assert!(close(right.volume_m3, 153.1875 * 0.15, 1e-9));
}

#[test]
fn an_offset_carriageway_and_a_right_hand_curve() {
    let a = AlignmentBuilder::new("R", 0.0, 0.0, 0.0, 1.0).arc(150.0, 60.0, Turn::Right).build().unwrap();
    let mut def = PavementDefinition::new(a, 6.0, 2.5, stack());
    def.centre_offset_m = 5.0; // to the right: inside a right-hand curve, radii 142 to 148
    let rows = measure_definition("P", &def).unwrap();
    let theta = 60.0 / 150.0;
    assert!(close(rows[0].heading_change_rad, -theta, 1e-12));
    assert!(close(rows[0].area_m2, 0.5 * theta * (148.0f64.powi(2) - 142.0f64.powi(2)), 1e-9));
}

/// Polygon area of a strip traced point by point from the alignment itself.
fn traced_area(a: &Alignment, s0: f64, s1: f64, o0: f64, o1: f64) -> f64 {
    let n = 20_000;
    let mut pts = Vec::new();
    for i in 0..=n {
        pts.push(a.point_at_offset(s0 + (s1 - s0) * i as f64 / n as f64, o0).unwrap());
    }
    for i in (0..=n).rev() {
        pts.push(a.point_at_offset(s0 + (s1 - s0) * i as f64 / n as f64, o1).unwrap());
    }
    let mut sum = 0.0;
    for i in 0..pts.len() {
        let (p, q) = (pts[i], pts[(i + 1) % pts.len()]);
        sum += p.0 * q.1 - q.0 * p.1;
    }
    (0.5 * sum).abs()
}

#[test]
fn the_strip_formula_holds_through_a_spiral_and_part_of_an_alignment() {
    // Tangent, spiral in to R = 100 over 60 m (turning 60/200 = 0.3 rad), then the curve.
    let a = AlignmentBuilder::new("S", 0.0, 10.0, 20.0, 0.7).line(40.0).spiral(60.0, None, Some(100.0), Turn::Left).arc(100.0, 50.0, Turn::Left).build().unwrap();
    assert!(close(heading_change_rad(&a, 40.0, 100.0), 0.3, 1e-12));
    assert!(close(heading_change_rad(&a, 0.0, 150.0), 0.3 + 0.5, 1e-12));
    // From 20 m (on the tangent) to 130 m (30 m into the curve): 0.3 + 30/100.
    let (s0, s1) = (20.0, 130.0);
    let turn = heading_change_rad(&a, s0, s1);
    assert!(close(turn, 0.6, 1e-12));
    let area = strip_area_m2(s1 - s0, turn, 3.5, 5.5);
    assert!(close(area, 2.0 * (110.0 + 4.5 * 0.6), 1e-12));
    assert!(close(area, traced_area(&a, s0, s1, 3.5, 5.5), 1e-4));

    let mut def = PavementDefinition::new(a, 7.0, 2.5, stack());
    (def.start_station_m, def.end_station_m) = (s0, s1);
    def.right_shoulder = Some(shoulder(2.0));
    let rows = measure_definition("P", &def).unwrap();
    assert!(close(row(&rows, "Right shoulder", "Base").area_m2, area, 1e-12));
}

#[test]
fn slabs_are_closed_face_outwards_and_sit_where_the_alignment_puts_them() {
    let def = road(straight(100.0));
    let slabs = solids(&def).unwrap();
    assert_eq!(slabs.solids.len(), 9);
    assert_eq!(slabs.sections, 2);
    let rows = measure_definition("P1", &def).unwrap();
    for (mesh, q) in slabs.solids.iter().zip(&rows) {
        assert_eq!(mesh.name, format!("{}: {}", q.strip, q.layer));
        // A closed, outward-wound mesh has exactly the prism's volume on a straight.
        assert!(close(mesh.volume() / 1.0e9, q.volume_m3, 1e-6), "{}: {} vs {}", mesh.name, mesh.volume() / 1.0e9, q.volume_m3);
    }
    // Levels: 0 at the crown, −2.5% × 3.5 m at the carriageway edge, a further −4% × 1.5 m at the
    // shoulder edge; the subbase bottom is 400 mm below.
    let top = &slabs.solids[3]; // Carriageway: Surfacing
    assert_eq!(top.name, "Carriageway: Surfacing");
    let zs: Vec<f64> = top.vertices.iter().map(|v| v[2]).collect();
    assert!(close(zs.iter().cloned().fold(f64::MIN, f64::max), 0.0, 1e-9));
    assert!(close(zs.iter().cloned().fold(f64::MAX, f64::min), -87.5 - 50.0, 1e-9));
    let lowest = slabs.solids.iter().flat_map(|m| m.vertices.iter().map(|v| v[2])).fold(f64::MAX, f64::min);
    assert!(close(lowest, -87.5 - 60.0 - 400.0, 1e-9));
    // The plan bounding box has its corner on the origin, and the origin is on the map.
    let min_x = slabs.solids.iter().flat_map(|m| m.vertices.iter().map(|v| v[0])).fold(f64::MAX, f64::min);
    let min_y = slabs.solids.iter().flat_map(|m| m.vertices.iter().map(|v| v[1])).fold(f64::MAX, f64::min);
    assert!(close(min_x, 0.0, 1e-9) && close(min_y, 0.0, 1e-9));
    // First vertex of the carriageway surfacing: the left carriageway edge at the start station.
    let (x, y) = def.alignment.point_at_offset(1000.0, -3.5).unwrap();
    assert!(close(slabs.origin_x_m + top.vertices[0][0] / 1000.0, x, 1e-9));
    assert!(close(slabs.origin_y_m + top.vertices[0][1] / 1000.0, y, 1e-9));
}

#[test]
fn slabs_follow_a_curve_within_the_chord_tolerance() {
    let def = road(arc());
    let slabs = solids(&def).unwrap();
    assert!(slabs.sections > 10);
    let rows = measure_definition("P1", &def).unwrap();
    for (mesh, q) in slabs.solids.iter().zip(&rows) {
        let v = mesh.volume() / 1.0e9;
        assert!(v > 0.0 && (v - q.volume_m3).abs() < 2e-3 * q.volume_m3, "{}: {v} vs {}", mesh.name, q.volume_m3);
    }
}

#[test]
fn a_seal_has_area_but_no_slab_and_no_volume() {
    let mut def = PavementDefinition::new(straight(50.0), 6.0, 3.0, vec![Layer::new("Surface dressing", Material::SurfaceDressing, 0.0), Layer::new("Base", Material::GranularBase, 150.0)]);
    def.crossfall_shape = CrossfallShape::OneWay { low_side: Side::Right };
    let rows = measure_definition("P", &def).unwrap();
    assert!(close(rows[0].area_m2, 300.0, 1e-9) && rows[0].volume_m3 == 0.0);
    let slabs = solids(&def).unwrap();
    assert_eq!(slabs.solids.len(), 1);
    // One plane falling to the right: +90 mm at the left edge, −90 mm at the right.
    let zs: Vec<f64> = slabs.solids[0].vertices.iter().map(|v| v[2]).collect();
    assert!(close(zs.iter().cloned().fold(f64::MIN, f64::max), 90.0, 1e-9));
    assert!(close(zs.iter().cloned().fold(f64::MAX, f64::min), -90.0 - 150.0, 1e-9));
    assert!(close(slabs.solids[0].volume() / 1.0e9, 45.0, 1e-6));
}

#[test]
fn the_cross_section_is_to_scale_and_carries_its_labels() {
    let def = road(straight(100.0));
    let s = cross_section(&def, &SectionOptions::default());
    let d = &s.drawing;
    assert_eq!((d.extents.min.x, d.extents.min.y), (0.0, 0.0));
    // 3 strips × 3 layers in the section and 3 boxes in the one structure column (the shoulders
    // share the carriageway's stack).
    let fills: Vec<&Vec<afrobim_draw::Pt2>> = d.primitives.iter().filter_map(|p| if let Primitive::Fill { outline, .. } = p { Some(outline) } else { None }).collect();
    assert_eq!(fills.len(), 12);
    // The carriageway base in the section: 7 000 wide, 150 thick measured vertically, crowned.
    let base = fills[4];
    assert_eq!(base.len(), 6);
    let width = base.iter().map(|p| p.x).fold(f64::MIN, f64::max) - base.iter().map(|p| p.x).fold(f64::MAX, f64::min);
    assert!(close(width, 7000.0, 1e-9));
    assert!(close(base[1].y - base[4].y, 150.0, 1e-9), "crown top to crown bottom");
    assert!(close(base[1].y - base[0].y, 87.5, 1e-9), "2.5% over 3.5 m");
    let texts: Vec<&str> = s.labels.iter().map(|l| l.text.as_str()).collect();
    for expected in ["Carriageway", "7.000 m", "fall 2.5%", "Left shoulder", "1.500 m", "fall 4%", "50 mm Surfacing (Hot mix asphalt)", "150 mm Base (Granular base)"] {
        assert!(texts.contains(&expected), "{expected} not in {texts:?}");
    }
    assert!(texts.contains(&"Left shoulder, carriageway, right shoulder"));
    // A structure column box: thickness × column scale.
    let column_base = fills[10];
    assert!(close(column_base[0].y - column_base[2].y, 150.0 * s.column_scale, 1e-9));
    // Every primitive is inside the stated extents.
    for p in &d.primitives {
        if let Primitive::Line { from, to, .. } = p {
            for q in [from, to] {
                assert!(q.x >= -1e-9 && q.y >= -1e-9 && q.x <= d.extents.max.x + 1e-9 && q.y <= d.extents.max.y + 1e-9);
            }
        }
    }
    let tall = cross_section(&def, &SectionOptions { vertical_exaggeration: 5.0, text_height_mm: Some(80.0) });
    assert_eq!(tall.labels[0].height_mm, 80.0);
}

fn codes(def: &PavementDefinition, severity: Severity) -> Vec<String> {
    def.notes().into_iter().filter(|n| n.severity == severity).map(|n| n.code).collect()
}

#[test]
fn validation_notes() {
    let good = road(straight(100.0));
    assert!(codes(&good, Severity::Error).is_empty() && codes(&good, Severity::Warning).is_empty());
    assert_eq!(codes(&good, Severity::Info), ["PAVEMENT_CROSSFALL_AS_ENTERED"]);
    good.check().unwrap();

    let mut d = good.clone();
    d.end_station_m = 2000.0;
    assert_eq!(codes(&d, Severity::Error), ["PAVEMENT_STATIONS_OFF_ALIGNMENT"]);
    assert!(measure_definition("P", &d).is_err());
    let mut d = good.clone();
    d.end_station_m = d.start_station_m;
    assert_eq!(codes(&d, Severity::Error), ["PAVEMENT_STATIONS"]);
    let mut d = good.clone();
    d.carriageway_width_m = 0.0;
    d.layers.clear();
    assert_eq!(codes(&d, Severity::Error), ["PAVEMENT_WIDTH", "PAVEMENT_NO_LAYERS"]);
    let mut d = good.clone();
    d.layers[1].thickness_mm = -1.0;
    assert_eq!(codes(&d, Severity::Error), ["PAVEMENT_LAYER_THICKNESS"]);

    // A 12 m wide road on a 5 m radius reaches the centre of the curve.
    let tight = AlignmentBuilder::new("T", 0.0, 0.0, 0.0, 0.0).arc(5.0, 5.0, Turn::Left).build().unwrap();
    assert_eq!(codes(&PavementDefinition::new(tight, 12.0, 2.5, stack()), Severity::Error), ["PAVEMENT_WIDER_THAN_RADIUS"]);

    // The reminders quote the manual's own conditions.
    let mut d = PavementDefinition::new(straight(100.0), 7.0, 2.0, stack());
    d.layers[0].thickness_mm = 30.0;
    d.layers[2].thickness_mm = 250.0;
    d.right_shoulder = Some(shoulder(0.5));
    let notes = d.notes();
    let find = |code: &str| notes.iter().filter(|n| n.code == code).collect::<Vec<_>>();
    assert!(find("PAVEMENT_CROSSFALL_BELOW_GUIDE")[0].message.contains("section 1.2 c"));
    assert_eq!(find("PAVEMENT_SHOULDER_BELOW_GUIDE").len(), 2);
    assert!(find("PAVEMENT_ASPHALT_BELOW_GUIDE")[0].message.contains("40 mm"));
    assert_eq!(find("PAVEMENT_LAYER_IN_LIFTS").len(), 1);
    assert_eq!(find("PAVEMENT_LAYER_IN_LIFTS")[0].severity, Severity::Info);
}

#[test]
fn csv_schedules() {
    let mut doc = Document::default();
    let mut def = PavementDefinition::new(arc(), 7.0, 2.5, vec![Layer::new("Base", Material::GranularBase, 150.0)]);
    def.right_shoulder = Some(Shoulder { width_m: 1.5, crossfall_percent: 4.0, layers: ShoulderLayers::Own(vec![Layer::new("Gravel, shoulder", Material::Other("G30".to_string()), 150.0)]) });
    add_pavement(&mut doc, "PV-001", &def).unwrap();
    let q = PavementQuantities::measure(doc.elements());
    assert_eq!(
        layers_to_csv(&q.rows),
        "mark,strip,layer,material,thickness_mm,width_m,mean_offset_m,length_m,heading_change_rad,strip_length_m,area_m2,volume_m3\n\
         PV-001,Carriageway,Base,Granular base,150,7.000,0.000,100.000,0.500000,100.000,700.000,105.000\n\
         PV-001,Right shoulder,\"Gravel, shoulder\",G30,150,1.500,4.250,100.000,0.500000,102.125,153.188,22.978\n\
         TOTAL,2,,,,,,,,,853.188,127.978\n"
    );
    assert_eq!(
        summary_to_csv(&q),
        "item,unit,quantity\n\
         pavements,nr,1\n\
         road length,m,100.000\n\
         Base (Granular base) 150 mm: area,m2,700.000\n\
         Base (Granular base) 150 mm: compacted volume,m3,105.000\n\
         \"Gravel, shoulder (G30) 150 mm: area\",m2,153.188\n\
         \"Gravel, shoulder (G30) 150 mm: compacted volume\",m3,22.978\n\
         all layers: compacted volume,m3,127.978\n"
    );
    assert!(close(q.length_m(), 100.0, 1e-12) && close(q.volume_m3(), 127.978125, 1e-9));
}

#[test]
fn the_module_the_document_helpers_and_the_round_trip() {
    let mut doc = Document::default();
    let def = road(arc());
    let id = add_pavement(&mut doc, "PV-001", &def).unwrap();
    let second = add_pavement(&mut doc, "PV-001", &def).unwrap();
    assert_eq!(doc.find(second).unwrap().name, "PV-002");
    let e = doc.find(id).unwrap().clone();
    assert_eq!((e.category.as_str(), e.layer.as_str()), (PAVEMENT_CATEGORY, PAVEMENT_LAYER));

    let module = PavementModule::default();
    assert_eq!((module.category(), module.display_name()), ("pavement", "Pavements"));
    let g = module.generate(&e).unwrap();
    assert_eq!(g.solids.len(), 9);
    assert!(close(e.width, g.drawing.extents.width(), 1e-9) && close(e.height, g.drawing.extents.height(), 1e-9));
    assert_eq!(g.derived["alignment"], "C1");
    assert_eq!(g.derived["length_m"], "100.000");
    assert_eq!(g.derived["heading_change_deg"], format!("{:.4}", 0.5f64.to_degrees()));
    assert_eq!(g.derived["overall_width_m"], "10.000");
    assert_eq!(g.derived["carriageway_thickness_mm"], "400");
    assert_eq!(g.derived["carriageway_area_m2"], "700.000");
    assert_eq!(g.derived["layers_volume_m3"], "400.000"); // (700 + 153.1875 + 146.8125) × 0.4
    // The site placement is the origin of the solids, unrotated.
    let slabs = solids(&def).unwrap();
    let site = e.site.clone().unwrap();
    assert!(close(site.x, slabs.origin_x_m * 1000.0, 1e-6) && close(site.y, slabs.origin_y_m * 1000.0, 1e-6));
    assert_eq!((site.z, site.rotation_deg, site.alignment.as_deref(), site.station_m), (0.0, 0.0, Some("C1"), Some(0.0)));

    // Save and load: the definition, and what is generated from it, come back the same.
    let loaded = Document::from_json(&doc.to_json()).unwrap();
    let again = loaded.find(id).unwrap();
    assert_eq!(PavementDefinition::from_value(&again.data).unwrap(), def);
    assert_eq!(module.generate(again).unwrap(), g);
    assert_eq!(pavements(&loaded).len(), 2);

    // An invalid definition is reported, not drawn.
    let mut bad = e.clone();
    let mut broken = def.clone();
    broken.carriageway_width_m = -1.0;
    bad.data = broken.to_value();
    let g = module.generate(&bad).unwrap();
    assert!(g.solids.is_empty() && g.drawing.primitives.is_empty() && g.notes.iter().any(|n| n.severity == Severity::Error));
    assert!(add_pavement(&mut doc, "X", &broken).is_err());
    bad.data = serde_json::json!({"nonsense": true});
    assert!(module.generate(&bad).is_err());

    // Editing, and following a changed alignment.
    let mut narrower = def.clone();
    narrower.carriageway_width_m = 6.0;
    update_pavement(&mut doc, id, &narrower).unwrap();
    assert_eq!(pavements(&doc)[0].2.carriageway_width_m, 6.0);
    let longer = AlignmentBuilder::new("C1", 0.0, 0.0, 0.0, 0.0).arc(300.0, 200.0, Turn::Left).build().unwrap();
    doc.add(afrobim_alignment::CATEGORY, "C1", afrobim_kernel::Pt::new(0.0, 0.0), 1.0, 1.0, "Alignments", serde_json::to_value(&longer).unwrap());
    assert_eq!(sync_alignments(&mut doc), vec![id, second]);
    assert_eq!(pavements(&doc)[0].2.alignment, longer);
    assert!(sync_alignments(&mut doc).is_empty());
}
