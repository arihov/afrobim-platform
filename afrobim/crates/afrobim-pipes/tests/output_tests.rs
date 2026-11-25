// SPDX-License-Identifier: LicenseRef-AfroBIM-Proprietary
// Copyright (c) 2026 AfroBIM. All rights reserved.
//! Outputs, tables, quantities, parts, LandXML and serde. All values are test inputs. Coordinates
//! are at true UTM magnitude.

use afrobim_alignment::AlignmentBuilder;
use afrobim_pipes::*;

const E0: f64 = 512_345.678;
const N0: f64 = 9_960_000.125;

fn spec(d: f64, wall: f64) -> PipeSpec {
    PipeSpec { shape: PipeShape::Circular { diameter_m: d }, material: Some("example material A".into()), wall_thickness_m: Some(wall), roughness: Some(Roughness::Manning { n: 0.013 }), part: None }
}

fn flat(level: f64) -> GroundFn<impl Fn(f64, f64) -> f64> {
    GroundFn(move |_, _| level)
}

/// Three manholes 100 m apart, 10 m left of an alignment that runs due east from (E0, N0).
fn beside_alignment() -> (PipeNetwork, Vec<StructureId>, Vec<PipeId>) {
    let mut n = PipeNetwork::new("beside");
    let rs = RunSpec { structure_kind: StructureKind::Manhole, structure_prefix: "MH".into(), pipe_prefix: "P".into(), pipe: spec(0.6, 0.05), start_invert_m: 5.0, slope: 0.01 };
    let pts: Vec<RunPoint> = [20.0, 120.0, 220.0].iter().map(|de| RunPoint { easting_m: E0 + de, northing_m: N0 + 10.0, rim_level_m: 9.0 }).collect();
    let (s, p) = n.add_run(&pts, &rs).unwrap();
    for id in &s {
        let st = n.structure_mut(*id).unwrap();
        st.inner = Some(StructureInner::Circular { diameter_m: 1.2 });
        st.wall_thickness_m = Some(0.1);
        st.sump_level_m = Some(4.5);
    }
    (n, s, p)
}

#[test]
fn plan_geometry_has_footprints_and_centrelines() {
    let (n, _, _) = beside_alignment();
    let g = n.plan_geometry();
    assert_eq!(g.structures.len(), 3);
    assert_eq!(g.pipes.len(), 2);
    assert_eq!(g.structures[0].footprint, Footprint::Circle { radius_m: 0.7 });
    assert_eq!(g.structures[0].easting_m, E0 + 20.0);
    assert_eq!(g.pipes[0].centreline, vec![[E0 + 20.0, N0 + 10.0], [E0 + 120.0, N0 + 10.0]]);
    assert!((g.pipes[0].outer_width_m - 0.7).abs() < 1e-12); // 0.6 + 2 * 0.05
}

#[test]
fn profile_along_own_path_and_on_an_alignment() {
    let (n, s, p) = beside_alignment();
    let run = n.trace_run(s[0]).unwrap();
    assert_eq!(run, p);
    let ground = GroundFn(|e: f64, _| 9.0 + 0.001 * (e - E0));
    let v = n.run_profile(&run, Some(&ground), 25.0).unwrap();
    assert_eq!(v.axis, ProfileAxis::OwnPath);
    assert_eq!(v.structures.len(), 3);
    assert!((v.structures[1].x_m - 100.0).abs() < 1e-8);
    assert!((v.structures[2].x_m - 200.0).abs() < 1e-8);
    // inverts 5.0 -> 4.0 -> 3.0, crown 0.6 above
    assert!((v.pipes[0].invert_line[1][1] - 4.0).abs() < 1e-9);
    assert!((v.pipes[1].crown_line[1][1] - 3.6).abs() < 1e-9);
    assert_eq!(v.structures[0].lowest_invert_m, Some(5.0));
    assert_eq!(v.structures[1].lowest_invert_m, Some(4.0));
    // ground at the first structure: 9.0 + 0.001 * 20 = 9.02
    assert!((v.structures[0].ground_level_m.unwrap() - 9.02).abs() < 1e-9);
    // ground sampled every 25 m over two 100 m pipes: 5 samples each
    assert_eq!(v.ground_line.len(), 10);
    // projected to an alignment starting at station 1000 at (E0, N0) heading east
    let a = AlignmentBuilder::new("AL", 1000.0, E0, N0, 0.0).line(300.0).build().unwrap();
    let v = n.profile_on_alignment(&run, &a, None, 25.0).unwrap();
    assert_eq!(v.axis, ProfileAxis::Alignment { name: "AL".into() });
    assert!((v.structures[0].x_m - 1020.0).abs() < 1e-8);
    assert!((v.structures[2].x_m - 1220.0).abs() < 1e-8);
    // 10 m north of an alignment that runs east is on the left: negative offset
    assert!((v.structures[0].offset_m.unwrap() + 10.0).abs() < 1e-8, "{:?}", v.structures[0].offset_m);
    // the run must join end to start
    assert!(n.run_profile(&[p[1], p[0]], None, 10.0).is_err());
}

#[test]
fn section_crossings_at_a_station() {
    let (n, _, p) = beside_alignment();
    let a = AlignmentBuilder::new("AL", 1000.0, E0, N0, 0.0).line(300.0).build().unwrap();
    // station 1070 is 70 m along: E0 + 70, between the first two manholes (at 20 and 120): 50 m along pipe 1
    // invert there: 5.0 - 0.01 * 50 = 4.5; crown 5.1; outer top 5.15; outer bottom 4.45; offset -10 (left)
    let ground = flat(9.0);
    let sv = n.section_at_station(&a, 1070.0, 30.0, Some(&ground)).unwrap();
    assert_eq!(sv.crossings.len(), 1);
    let c = &sv.crossings[0];
    assert_eq!(c.pipe, p[0]);
    assert!((c.offset_m + 10.0).abs() < 1e-8, "{}", c.offset_m);
    assert!((c.invert_m - 4.5).abs() < 1e-8);
    assert!((c.crown_m - 5.1).abs() < 1e-8);
    assert!((c.outer_top_m - 5.15).abs() < 1e-8);
    assert!((c.outer_bottom_m - 4.45).abs() < 1e-8);
    assert_eq!(c.ground_level_m, Some(9.0));
    assert!(sv.structures.is_empty());
    // at station 1120 the section passes through the middle manhole (radius 0.7 + the pipes end there)
    let sv = n.section_at_station(&a, 1120.0, 30.0, None).unwrap();
    assert_eq!(sv.structures.len(), 1);
    assert!((sv.structures[0].offset_m + 10.0).abs() < 1e-8);
    assert!(sv.structures[0].distance_to_line_m < 1e-8);
    // a section that misses everything
    let sv = n.section_at_station(&a, 1250.0, 5.0, None).unwrap();
    assert!(sv.crossings.is_empty() && sv.structures.is_empty());
    assert!(n.section_at_station(&a, 5000.0, 5.0, None).is_err());
}

#[test]
fn tables_and_csv() {
    let (n, _, _) = beside_alignment();
    let st = n.structure_table();
    assert_eq!(st.len(), 3);
    assert_eq!(st[1].pipes_in, vec!["P1"]);
    assert_eq!(st[1].pipes_out, vec!["P2"]);
    assert_eq!(st[1].lowest_invert_m, Some(4.0));
    assert_eq!(st[1].depth_to_lowest_invert_m, Some(5.0)); // rim 9.0 - 4.0
    let pt = n.pipe_table();
    assert!((pt[0].slope - 0.01).abs() < 1e-9);
    assert!((pt[0].plan_length_m - 100.0).abs() < 1e-8);
    // coordinates are written in full and read back exactly
    let csv = n.structure_table_csv();
    let lines: Vec<&str> = csv.lines().collect();
    assert_eq!(lines.len(), 4);
    assert!(lines[0].starts_with("id,name,kind,easting_m"));
    let fields: Vec<&str> = lines[1].split(',').collect();
    assert_eq!(fields[3].parse::<f64>().unwrap(), E0 + 20.0);
    assert_eq!(fields[4].parse::<f64>().unwrap(), N0 + 10.0);
    let pcsv = n.pipe_table_csv();
    assert_eq!(pcsv.lines().count(), 3);
    assert!(pcsv.lines().nth(1).unwrap().contains("0.600 m dia"));
}

#[test]
fn quantities_by_size_material_and_kind() {
    let (mut n, s, p) = beside_alignment();
    n.pipe_mut(p[1]).unwrap().shape = PipeShape::Circular { diameter_m: 0.9 };
    n.structure_mut(s[2]).unwrap().kind = StructureKind::Outfall;
    let l = n.pipe_length_quantities();
    assert_eq!(l.len(), 2);
    let a = l.iter().find(|r| r.size == "0.600 m dia").unwrap();
    assert_eq!(a.count, 1);
    assert!((a.plan_length_m - 100.0).abs() < 1e-8);
    // sloping length = sqrt(100^2 + 1^2) = 100.005
    assert!((a.sloping_length_m - 100.00499987).abs() < 1e-6);
    let c = n.structure_count_quantities();
    assert_eq!(c.iter().find(|r| r.kind == StructureKind::Manhole).unwrap().count, 2);
    assert_eq!(c.iter().find(|r| r.kind == StructureKind::Outfall).unwrap().count, 1);
    assert!(n.pipe_length_csv().contains("0.900 m dia"));
    assert!(n.structure_count_csv().starts_with("kind,part,count"));
}

#[test]
fn trench_excavation_hand_worked() {
    // one pipe 100 m long, level, invert 8.0, D 0.6, wall 0.05, flat ground 10.0, bedding 0.15, trench 1.0 m wide.
    // trench bottom = 8.0 - 0.05 - 0.15 = 7.80, depth d = 2.20 m everywhere.
    // vertical sides: V = w d L = 1.0 * 2.2 * 100 = 220 m3; bands [1.5]: 0-1.5 is 150, over 1.5 is 70.
    // side slope z = 0.5: A = w d + z d^2 = 2.2 + 0.5 * 4.84 = 4.62 m2, V = 462 m3.
    // bands: 0-1.5: w*1.5 + z (2*2.2*1.5 - 1.5^2) = 1.5 + 0.5 * (6.6 - 2.25) = 3.675 -> 367.5; rest 94.5.
    // bedding = 1.0 * 0.15 * 100 = 15 m3;  pipe displacement = pi * 0.35^2 * 100 = 38.4845 m3
    let mut n = PipeNetwork::new("trench");
    let a = n.add_structure("a", StructureKind::Manhole, E0, N0, 10.0).unwrap();
    let b = n.add_structure("b", StructureKind::Manhole, E0 + 100.0, N0, 10.0).unwrap();
    n.connect(a, b, &spec(0.6, 0.05), 8.0, 8.0, None).unwrap();
    let mut ts = TrenchSpec { width: TrenchWidth::Fixed { width_m: 1.0 }, bedding_thickness_m: 0.15, side_slope: 0.0, depth_bands_m: vec![1.5], station_spacing_m: 10.0 };
    let q = n.trench_quantities(&flat(10.0), &ts).unwrap();
    assert!((q.total_excavation_m3 - 220.0).abs() < 1e-6, "{}", q.total_excavation_m3);
    assert!((q.band_totals[0].volume_m3 - 150.0).abs() < 1e-6);
    assert!((q.band_totals[1].volume_m3 - 70.0).abs() < 1e-6);
    assert_eq!(q.band_totals[1].to_m, None);
    assert!((q.total_bedding_m3 - 15.0).abs() < 1e-6);
    assert!((q.total_pipe_displacement_m3 - std::f64::consts::PI * 0.35 * 0.35 * 100.0).abs() < 1e-6);
    ts.side_slope = 0.5;
    let q = n.trench_quantities(&flat(10.0), &ts).unwrap();
    assert!((q.total_excavation_m3 - 462.0).abs() < 1e-6);
    assert!((q.band_totals[0].volume_m3 - 367.5).abs() < 1e-6);
    assert!((q.band_totals[1].volume_m3 - 94.5).abs() < 1e-6);
    // width from the pipe: outside 0.7 + 2 * 0.3 = 1.3 m
    ts.side_slope = 0.0;
    ts.width = TrenchWidth::OutsideWidthPlus { each_side_m: 0.3 };
    let q = n.trench_quantities(&flat(10.0), &ts).unwrap();
    assert!((q.rows[0].trench_width_m - 1.3).abs() < 1e-12);
    assert!((q.total_excavation_m3 - 1.3 * 2.2 * 100.0).abs() < 1e-6);
    // ground sloping 0.01 along the pipe: depth goes 2.2 -> 3.2, average end area is exact for vertical sides
    let sloping = GroundFn(|e: f64, _| 10.0 + 0.01 * (e - E0));
    ts.width = TrenchWidth::Fixed { width_m: 1.0 };
    let q = n.trench_quantities(&sloping, &ts).unwrap();
    assert!((q.total_excavation_m3 - 2.7 * 100.0).abs() < 1e-6);
    // above-ground pipe has no trench
    let q = n.trench_quantities(&flat(5.0), &ts).unwrap();
    assert_eq!(q.total_excavation_m3, 0.0);
    // inputs are checked
    ts.station_spacing_m = 0.0;
    assert!(n.trench_quantities(&flat(10.0), &ts).is_err());
    assert!(q.to_csv().contains("band_from_m"));
}

#[test]
fn parts_from_csv_and_json() {
    let c = PartsCatalogue::from_csv("example", include_str!("data/example_parts.csv")).unwrap();
    assert_eq!(c.parts.len(), 5);
    assert_eq!(c.pipe_parts().count(), 3);
    assert_eq!(c.structure_parts().count(), 2);
    let p1 = c.get("EXAMPLE-P1").unwrap();
    assert_eq!(p1.name, "Example pipe A, test only"); // a quoted comma
    assert_eq!(p1.ifc_entity.as_deref(), Some("IFCPIPESEGMENT"));
    assert_eq!(p1.ifc_predefined_type, None);
    assert_eq!(p1.cost_item_code.as_deref(), Some("EX-0001"));
    assert_eq!(p1.roughness, Some(Roughness::Manning { n: 0.013 }));
    assert_eq!(c.get("EXAMPLE-P3").unwrap().pipe_shape, Some(PipeShape::Box { width_m: 1.2, height_m: 0.8 }));
    // JSON and CSV round trips
    let back = PartsCatalogue::from_json(&c.to_json()).unwrap();
    assert_eq!(back, c);
    let again = PartsCatalogue::from_csv("example", &c.to_csv()).unwrap();
    assert_eq!(again.parts, c.parts);
    // a JSON file as a user would write it
    let json = include_str!("data/example_parts.json");
    let j = PartsCatalogue::from_json(json).unwrap();
    assert_eq!(j.parts.len(), 2);
    assert!(j.parts.iter().any(|p| matches!(&p.pipe_shape, Some(PipeShape::Egg { profile, .. }) if profile.len() == 3)));
    // errors
    assert!(PartsCatalogue::from_csv("x", "id,family\nA,pipe\n").is_err()); // no shape
    assert!(PartsCatalogue::from_csv("x", "id,family,shape,diameter_m\nA,pipe,circular,abc\n").is_err());
    assert!(PartsCatalogue::from_csv("x", "id,family,shape,diameter_m\nA,pipe,circular,0.3\nA,pipe,circular,0.4\n").is_err()); // duplicate id
    assert!(PartsCatalogue::from_csv("x", "id,family,shape,diameter_m,manning_n,ks_m\nA,pipe,circular,0.3,0.013,0.001\n").is_err());
    assert!(PartsCatalogue::from_csv("x", "id,family,shape,diameter_m\nA,pipe,circular,-0.3\n").is_err());
}

fn rich_network() -> PipeNetwork {
    let mut n = PipeNetwork::new("storm & <east> \"1\"");
    n.network_type = NetworkType::Storm;
    n.status = NetworkStatus::Existing;
    n.source = Some("record 42, test".into());
    n.surface_name = Some("EG".into());
    n.alignment_name = Some("AL".into());
    let a = n.add_structure("A", StructureKind::CatchBasin, E0 + 0.123456789, N0 + 0.987654321, 12.5).unwrap();
    let b = n.add_structure("B", StructureKind::Manhole, E0 + 40.0, N0 + 3.0, 12.25).unwrap();
    let c = n.add_structure("C", StructureKind::Headwall, E0 + 80.0, N0 + 6.0, 11.0).unwrap();
    let d = n.add_structure("D", StructureKind::Junction, E0 + 40.0, N0 + 43.0, 12.0).unwrap();
    {
        let s = n.structure_mut(b).unwrap();
        s.inner = Some(StructureInner::Circular { diameter_m: 1.2 });
        s.wall_thickness_m = Some(0.1);
        s.sump_level_m = Some(8.1);
        s.part = Some("EXAMPLE-S1".into());
        s.loss_coefficient = Some(0.25);
        s.description = Some("main chamber".into());
    }
    {
        let s = n.structure_mut(a).unwrap();
        s.inflow_m3_s = 0.05;
        s.wall_thickness_m = Some(0.07);
    }
    n.structure_mut(d).unwrap().inner = Some(StructureInner::Rectangular { length_m: 0.9, width_m: 0.6 });
    let p1 = n.connect(a, b, &spec(0.45, 0.05), 10.25, 10.0, Some("P-A-B")).unwrap();
    n.pipe_mut(p1).unwrap().design_flow_m3_s = Some(0.0625);
    n.pipe_mut(p1).unwrap().part = Some("EXAMPLE-P1".into());
    let p2 = n.connect(b, c, &spec(0.6, 0.0), 9.9, 9.5, Some("P-B-C")).unwrap();
    n.pipe_mut(p2).unwrap().via = Some(PipeVia { easting_m: E0 + 60.0, northing_m: N0 + 3.5 });
    let p3 = n.connect(d, b, &PipeSpec { shape: PipeShape::Box { width_m: 0.9, height_m: 0.6 }, material: None, wall_thickness_m: None, roughness: Some(Roughness::ColebrookWhite { ks_m: 0.0015 }), part: None }, 10.1, 10.0, Some("P-D-B")).unwrap();
    n.pipe_mut(p3).unwrap().description = Some("box <inlet>".into());
    n
}

#[test]
fn landxml_round_trip_is_exact() {
    let n = rich_network();
    let xml = write_landxml_pipes(&[n.clone()], &WriteOptions::new("2026-11-16", "09:30:00")).unwrap();
    assert!(xml.contains("<PipeNetworks>") && xml.contains("pipeNetType=\"storm\"") && xml.contains("<CircStruct") && xml.contains("<RectPipe"));
    // Center text is northing first, with every digit
    assert!(xml.contains(&format!("<Center>{} {}</Center>", N0 + 0.987654321, E0 + 0.123456789)));
    let back = parse_landxml_pipes(&xml).unwrap();
    assert_eq!(back.networks.len(), 1);
    let m = &back.networks[0];
    assert_eq!(*m, n, "notes: {:?}", back.notes);
    // coordinates are bit-identical
    let s = m.structure_by_name("A").unwrap();
    assert_eq!(s.easting_m.to_bits(), (E0 + 0.123456789).to_bits());
    assert_eq!(s.northing_m.to_bits(), (N0 + 0.987654321).to_bits());
    // and twice through
    let xml2 = write_landxml_pipes(&back.networks, &WriteOptions::new("2026-11-16", "09:30:00")).unwrap();
    assert_eq!(xml, xml2);
}

#[test]
fn landxml_egg_profile_round_trips_and_isolated_structure_is_refused() {
    let mut n = PipeNetwork::new("egg");
    let a = n.add_structure("A", StructureKind::Manhole, E0, N0, 10.0).unwrap();
    let b = n.add_structure("B", StructureKind::Manhole, E0 + 10.0, N0, 10.0).unwrap();
    n.connect(a, b, &PipeSpec { shape: PipeShape::Egg { width_m: 0.8, height_m: 1.2, profile: vec![[0.0, 0.0], [0.4, 0.7], [1.2, 0.0]] }, material: None, wall_thickness_m: None, roughness: Some(Roughness::Manning { n: 0.013 }), part: None }, 5.0, 4.9, None).unwrap();
    let xml = write_landxml_pipes(&[n.clone()], &WriteOptions::new("2026-11-16", "00:00:00")).unwrap();
    assert_eq!(parse_landxml_pipes(&xml).unwrap().networks[0], n);
    n.add_structure("lonely", StructureKind::Manhole, E0 + 99.0, N0, 10.0).unwrap();
    assert!(write_landxml_pipes(&[n], &WriteOptions::new("2026-11-16", "00:00:00")).is_err());
}

#[test]
fn landxml_reads_a_file_from_another_program_in_feet() {
    // hand-written, no afrobim properties: kinds come from the structure elements, ids are numbered
    let xml = r#"<?xml version="1.0"?>
<LandXML xmlns="http://www.landxml.org/schema/LandXML-1.2" version="1.2" date="2026-01-01" time="00:00:00">
 <Units><Imperial areaUnit="squareFoot" linearUnit="foot" volumeUnit="cubicFeet" temperatureUnit="fahrenheit" pressureUnit="inHG" diameterUnit="inch" widthUnit="inch" heightUnit="inch" flowUnit="cubicFeetSecond"/></Units>
 <CgPoints><CgPoint name="1">1000 2000</CgPoint></CgPoints>
 <PipeNetworks><PipeNetwork name="N" pipeNetType="water">
  <Structs>
   <Struct name="S1" elevRim="100"><Center pntRef="1"/><CircStruct diameter="48" thickness="0.5"/><Invert elev="95" flowDir="out" refPipe="P1"/></Struct>
   <Struct name="S2" elevRim="99"><Center>1000 2100</Center><OutletStruct/><Invert elev="94" flowDir="in" refPipe="P1"/></Struct>
  </Structs>
  <Pipes><Pipe name="P1" refStart="S1" refEnd="S2"><CircPipe diameter="12" mannings="0.013" material="x" thickness="0.1"/><PipeFlow flowIn="2"/></Pipe>
         <Pipe name="Ch" refStart="S1" refEnd="S2"><Channel height="1" widthTop="2" widthBottom="1"/></Pipe></Pipes>
 </PipeNetwork></PipeNetworks></LandXML>"#;
    let r = parse_landxml_pipes(xml).unwrap();
    let n = &r.networks[0];
    assert_eq!(n.network_type, NetworkType::Water);
    assert_eq!(n.pipe_count(), 1);
    assert!(r.notes.iter().any(|t| t.contains("Channel")));
    let s1 = n.structure_by_name("S1").unwrap();
    assert!((s1.northing_m - 1000.0 * 0.3048).abs() < 1e-9 && (s1.easting_m - 2000.0 * 0.3048).abs() < 1e-9);
    assert_eq!(s1.inner, Some(StructureInner::Circular { diameter_m: 48.0 * 0.0254 }));
    assert!((s1.rim_level_m - 30.48).abs() < 1e-9);
    assert_eq!(n.structure_by_name("S2").unwrap().kind, StructureKind::Outfall);
    let p = n.pipe_by_name("P1").unwrap();
    assert_eq!(p.shape, PipeShape::Circular { diameter_m: 12.0 * 0.0254 });
    assert!((p.start_invert_m - 95.0 * 0.3048).abs() < 1e-9);
    assert!((p.design_flow_m3_s.unwrap() - 2.0 * 0.3048_f64.powi(3)).abs() < 1e-12);
    assert!(parse_landxml_pipes("<Other/>").is_err());
    assert!(parse_landxml_pipes("<LandXML><unclosed></LandXML>").is_err());
}

#[test]
fn serde_round_trips_everything() {
    let n = rich_network();
    let json = serde_json::to_string(&n).unwrap();
    let back: PipeNetwork = serde_json::from_str(&json).unwrap();
    assert_eq!(back, n);
    // large coordinates survive JSON exactly
    assert_eq!(back.structure_by_name("A").unwrap().northing_m.to_bits(), (N0 + 0.987654321).to_bits());
    let rules = RuleSet { name: "r".into(), min_slope: Some(0.01), match_mode: Some(MatchMode::Crown), ..RuleSet::default() };
    assert_eq!(serde_json::from_str::<RuleSet>(&serde_json::to_string(&rules).unwrap()).unwrap(), rules);
    let rep = n.check_hydraulics(&RuleSet::default(), &HydraulicSettings { kinematic_viscosity_m2_s: Some(1.0e-6) });
    assert_eq!(serde_json::from_str::<HydraulicReport>(&serde_json::to_string(&rep).unwrap()).unwrap(), rep);
    let gl = n.grade_lines(9.0, &HydraulicSettings { kinematic_viscosity_m2_s: Some(1.0e-6) });
    assert_eq!(serde_json::from_str::<GradeReport>(&serde_json::to_string(&gl).unwrap()).unwrap(), gl);
    let plan = n.plan_geometry();
    assert_eq!(serde_json::from_str::<PlanGeometry>(&serde_json::to_string(&plan).unwrap()).unwrap(), plan);
    let t = n.topology();
    assert_eq!(serde_json::from_str::<Topology>(&serde_json::to_string(&t).unwrap()).unwrap(), t);
    let f = Finding::new("x", Severity::Warning, vec![ElementRef::Pipe(PipeId(3)), ElementRef::Structure(StructureId(2))], "m".into());
    assert_eq!(serde_json::from_str::<Finding>(&serde_json::to_string(&f).unwrap()).unwrap(), f);
}

#[test]
fn mixed_networks_in_one_landxml_file() {
    let a = rich_network();
    let mut b = PipeNetwork::new("second");
    let s = b.add_structure("X", StructureKind::Manhole, E0, N0, 9.0).unwrap();
    let t = b.add_structure("Y", StructureKind::Manhole, E0 + 20.0, N0, 9.0).unwrap();
    b.connect(s, t, &spec(0.3, 0.05), 7.0, 6.8, None).unwrap();
    let xml = write_landxml_pipes(&[a.clone(), b.clone()], &WriteOptions::new("2026-11-16", "00:00:00")).unwrap();
    let r = parse_landxml_pipes(&xml).unwrap();
    assert_eq!(r.networks, vec![a, b]);
    let now = WriteOptions::now();
    assert_eq!(now.date.len(), 10);
}
