// SPDX-License-Identifier: LicenseRef-AfroBIM-Proprietary
// Copyright (c) 2026 AfroBIM. All rights reserved.
//! Assemblies, subassemblies, targets, the corridor model, sample lines and the new quantities,
//! against hand calculations.
//!
//! Every width, slope, depth, level and factor below is an arbitrary **example only** chosen to
//! make the hand calculation short. None is a design value.
//!
//! Set-up as in `corridor.rs`: a straight alignment east from (0, 0), stations 0 to 100, a point
//! at offset `o` is at northing −o, and the ground is the plane `z = 100 + c·o`.

use afrobim_alignment::{Alignment, AlignmentBuilder, CrossFallPoint, Profile, Pvi, Superelevation};
use afrobim_corridor::*;

fn close(got: f64, want: f64, what: &str) {
    assert!((got - want).abs() <= 1e-9 * (1.0 + want.abs()), "{what}: got {got}, want {want}");
}

fn straight(level: f64) -> Alignment {
    let mut a = AlignmentBuilder::new("Test", 0.0, 0.0, 0.0, 0.0).line(100.0).build().unwrap();
    a.profile = Some(Profile::new("FG", vec![Pvi::point(0.0, level), Pvi::point(100.0, level)]).unwrap());
    a
}

fn plane(cross: f64) -> Tin {
    Tin::from_grid(-20.0, -60.0, 20.0, 20.0, 8, 7, |_, y| 100.0 - cross * y).unwrap()
}

fn pct(p: f64) -> Slope {
    Slope::Percent(p)
}

fn one_region(asm: Assembly, freq: Frequency) -> Baseline {
    Baseline {
        name: "B".into(),
        alignment: "Test".into(),
        regions: vec![AssemblyRegion { name: "R".into(), start_station_m: 0.0, end_station_m: 100.0, assembly: asm, frequency: freq }],
        crossfall_table: vec![],
    }
}

fn section(asm: Assembly, align: &Alignment, ground: &Tin, station: f64) -> ModelSection {
    design_section_at(&one_region(asm, Frequency::uniform(20.0)), 0, align, ground, station).unwrap()
}

fn pt<'a>(m: &'a ModelSection, code: &'a str, side: Side) -> &'a DesignPoint {
    m.design.points_with_code(code).find(|p| p.side == Some(side)).unwrap_or_else(|| panic!("no point {code}"))
}

// ─── subassemblies ──────────────────────────────────────────────────────────────────────────

/// Lane 4 m at -2 %, layers Pave1 0.1 and Base 0.2 (vertical depths), centre at 102.
/// Right edge: (4, 102 - 0.08) = (4, 101.92). A layer is a parallelogram of horizontal width 4 and
/// vertical depth d, area 4 d: Pave1 0.4, Base 0.8 each side, so 0.8 and 1.6 for both sides.
/// Datum under the lane: (0, 102 - 0.3) = (0, 101.7) to (4, 101.62).
/// Earthworks line (no daylight part): (-4, 101.92) (-4, 101.62) (0, 101.7) (4, 101.62) (4, 101.92).
#[test]
fn lane_with_layers_gives_shapes_codes_and_datum() {
    let layers = vec![Layer::new("Pave1", 0.1).as_course("example asphalt"), Layer::new("Base", 0.2)];
    let asm = Assembly::symmetric("A", vec![library::lane("Lane", 4.0, pct(-2.0), layers)]);
    let m = section(asm, &straight(102.0), &plane(0.0), 50.0);
    close(pt(&m, "ETW", Side::Right).offset_m, 4.0, "ETW offset");
    close(pt(&m, "ETW", Side::Right).elevation_m, 101.92, "ETW level");
    close(pt(&m, "ETW", Side::Left).offset_m, -4.0, "left ETW");
    close(m.design.shape_area("Pave1"), 0.8, "Pave1 both sides");
    close(m.design.shape_area("Base"), 1.6, "Base both sides");
    let pave = m.design.shapes.iter().find(|s| s.code == "Pave1").unwrap();
    assert_eq!(pave.classification.ifc_entity.as_deref(), Some("IfcCourse"));
    assert_eq!(pave.classification.predefined_type.as_deref(), Some("PAVEMENT"));
    assert_eq!(pave.classification.material.as_deref(), Some("example asphalt"));
    // Link codes: Top on the surface, Pave under Pave1, Datum under Base.
    assert_eq!(m.design.links_with_code("Top").count(), 2);
    assert_eq!(m.design.links_with_code("Pave").count(), 2);
    let datum = m.design.links_with_code("Datum").find(|l| l.side == Some(Side::Right)).unwrap();
    close(datum.points[0].1, 101.7, "datum at the centre");
    close(datum.points[1].1, 101.62, "datum at the edge");
    let want = [(-4.0, 101.92), (-4.0, 101.62), (0.0, 101.7), (4.0, 101.62), (4.0, 101.92)];
    assert_eq!(m.section.earthworks.len(), want.len(), "{:?}", m.section.earthworks);
    for (g, w) in m.section.earthworks.iter().zip(want) {
        close(g.0, w.0, "earthworks offset");
        close(g.1, w.1, "earthworks level");
    }
}

/// Lane 4 at -2 %, kerb height 0.15, top width 0.2, bed 0.1, footway 2 at +1 %. Centre 102.
/// Lane edge (4, 101.92); kerb face up to (4, 102.07), top to (4.2, 102.07), the footway attaches
/// there and ends at (6.2, 102.07 + 0.02) = (6.2, 102.09). Kerb shape 0.2 (0.15 + 0.1) = 0.05.
/// The datum steps down at the kerb: (4, 101.92) to (4, 101.82), along to (4.2, 101.82), up to
/// (4.2, 102.07).
#[test]
fn kerb_and_footway() {
    let asm = Assembly::symmetric(
        "A",
        vec![library::lane("Lane", 4.0, pct(-2.0), vec![]), library::kerb("Kerb", 0.15, 0.2, 0.1), library::footway("Fw", 2.0, pct(1.0), vec![])],
    );
    let m = section(asm, &straight(102.0), &plane(0.0), 50.0);
    close(pt(&m, "Kerb_Back", Side::Right).elevation_m, 102.07, "kerb top");
    close(pt(&m, "EFw", Side::Right).offset_m, 6.2, "footway edge");
    close(pt(&m, "EFw", Side::Right).elevation_m, 102.09, "footway level");
    close(m.design.shape_area("Kerb"), 0.1, "two kerbs");
    let face = m.design.links_with_code("Face").find(|l| l.side == Some(Side::Right)).unwrap();
    close(face.points[0].1, 101.92, "face foot");
    close(face.points[1].1, 102.07, "face top");
    let right: Vec<(f64, f64)> = m.section.earthworks.iter().copied().filter(|p| p.0 >= 0.0).collect();
    let want = [(0.0, 102.0), (4.0, 101.92), (4.0, 101.82), (4.2, 101.82), (4.2, 102.07), (6.2, 102.09)];
    assert_eq!(right.len(), want.len(), "{right:?}");
    for (g, w) in right.iter().zip(want) {
        close(g.0, w.0, "offset");
        close(g.1, w.1, "level");
    }
}

/// Raised median (half) 0.15 high, 0.5 wide, then a lane: the lane attaches at the foot of the
/// back, (0.5, 102), not at the top.
#[test]
fn raised_median_lets_the_lane_start_at_the_foot() {
    let asm = Assembly::symmetric("A", vec![library::median_raised("Med", 0.15, 0.5, 0.05), library::lane("Lane", 3.0, pct(0.0), vec![])]);
    let m = section(asm, &straight(102.0), &plane(0.0), 50.0);
    assert!(m.design.links_with_code("Back").count() == 2);
    close(pt(&m, "ETW", Side::Right).offset_m, 3.5, "lane edge");
    close(pt(&m, "ETW", Side::Right).elevation_m, 102.0, "lane level");
}

/// Trapezoid ditch: depth 0.5, bottom 1.0, inner run 2, outer run 3 from (0, 102):
/// (1, 101.5) (2, 101.5) (3.5, 102). Lined 0.1 thick (vertical): the lining is the strip between
/// the ditch surface and the surface lowered by 0.1, area 0.1 x 3.5 = 0.35.
#[test]
fn trapezoid_ditch_and_lined_drain() {
    let asm = Assembly::symmetric("A", vec![library::side_ditch_trapezoid("Ditch", 0.5, 1.0, 2.0, 3.0)]);
    let m = section(asm, &straight(102.0), &plane(0.0), 50.0);
    let r: Vec<_> = m.design.points.iter().filter(|p| p.side == Some(Side::Right)).collect();
    assert_eq!(r.len(), 3);
    close(r[0].offset_m, 1.0, "invert in");
    close(r[0].elevation_m, 101.5, "invert level");
    close(r[1].offset_m, 2.0, "invert out");
    close(r[2].offset_m, 3.5, "top out");
    close(r[2].elevation_m, 102.0, "top out level");
    assert!(r[2].codes.contains(&"Ditch_Top_Out".to_string()));

    let ditch = Ditch { shape: DitchShape::Trapezoid { bottom_width_m: 1.0 }, depth_m: 0.5, inner_run_per_rise: 2.0, outer_run_per_rise: 3.0 };
    let asm = Assembly::symmetric("A", vec![library::lined_drain("Drain", ditch, 0.1)]);
    let m = section(asm, &straight(102.0), &plane(0.0), 50.0);
    close(m.design.shape_area("Lining"), 0.7, "two linings");
    let datum = m.design.links_with_code("Datum").find(|l| l.side == Some(Side::Right)).unwrap();
    close(datum.points[1].1, 101.4, "lowered invert");
}

/// V-ditch depth 1, runs 2 and 2: (2, 101), (4, 102): three points on the way, two links.
#[test]
fn v_ditch_median_depressed_and_wall() {
    let asm = Assembly::symmetric("A", vec![library::side_ditch_v("V", 1.0, 2.0, 2.0)]);
    let m = section(asm, &straight(102.0), &plane(0.0), 50.0);
    close(pt(&m, "Ditch_Invert", Side::Right).offset_m, 2.0, "V invert");
    close(pt(&m, "Ditch_Top_Out", Side::Right).offset_m, 4.0, "V top");
    // Depressed median: half bottom 1, depth 0.5, side run 4: edge at (1 + 2, 102.5).
    let asm = Assembly::symmetric("A", vec![library::median_depressed("M", 1.0, 0.5, 4.0)]);
    let m = section(asm, &straight(102.0), &plane(0.0), 50.0);
    close(pt(&m, "Median_Edge", Side::Right).offset_m, 3.0, "median edge");
    close(pt(&m, "Median_Edge", Side::Right).elevation_m, 102.5, "median edge level");
    // Wall 0.3 thick, 1.2 deep: area 0.36 each.
    let asm = Assembly::symmetric("A", vec![library::retaining_edge("W", 0.3, 1.2)]);
    let m = section(asm, &straight(102.0), &plane(0.0), 50.0);
    close(m.design.shape_area("Wall"), 0.72, "two walls");
}

// ─── superelevation and targets ─────────────────────────────────────────────────────────────

/// Superelevation stored on the alignment: left -2 % at 0 and 100; right -2 % at 0, +6 % at 100.
/// At 50 the right is +2 %, so a lane 4 wide that follows it ends at 102 + 0.08 = 102.08 whatever
/// its entered slope (-3 %); the left is -2 %: 101.92.
/// With no alignment data the baseline's crossfall table gives the same.
#[test]
fn superelevation_from_the_alignment_and_from_the_table() {
    let lane = || vec![library::lane_superelevated("Lane", 4.0, pct(-3.0), vec![])];
    let mut a = straight(102.0);
    a.superelevation = Some(
        Superelevation::from_points(vec![CrossFallPoint::new(0.0, Some(-2.0), Some(-2.0)), CrossFallPoint::new(100.0, Some(-2.0), Some(6.0))]).unwrap(),
    );
    let m = section(Assembly::symmetric("A", lane()), &a, &plane(0.0), 50.0);
    close(pt(&m, "ETW", Side::Right).elevation_m, 102.08, "right edge");
    close(pt(&m, "ETW", Side::Left).elevation_m, 101.92, "left edge");

    let mut b = one_region(Assembly::symmetric("A", lane()), Frequency::uniform(20.0));
    b.crossfall_table = vec![CrossFallPoint::new(0.0, Some(-2.0), Some(-2.0)), CrossFallPoint::new(100.0, Some(-2.0), Some(6.0))];
    let m = design_section_at(&b, 0, &straight(102.0), &plane(0.0), 50.0).unwrap();
    close(pt(&m, "ETW", Side::Right).elevation_m, 102.08, "right edge from the table");
    // Outside the table the entered slope (-3 %) is kept.
    b.crossfall_table = vec![CrossFallPoint::new(60.0, Some(-2.0), Some(-2.0)), CrossFallPoint::new(100.0, Some(-2.0), Some(6.0))];
    let m = design_section_at(&b, 0, &straight(102.0), &plane(0.0), 50.0).unwrap();
    close(pt(&m, "ETW", Side::Right).elevation_m, 102.0 - 0.12, "entered slope outside the table");
}

/// Lane widening: the right lane edge follows a target. Table (0, 3) to (100, 5): 4 at station 50.
/// The same target as a plan polyline from (0, -3) to (100, -5): at station 50, y = -4, offset 4.
/// An elevation target: level 103 from 102 at 25 % takes 4 m.
#[test]
fn offset_and_elevation_targets() {
    let mut asm = Assembly::new("A");
    asm.right.push(library::lane_to_offset("L", OffsetTarget::Table(vec![(0.0, 3.0), (100.0, 5.0)]), pct(0.0), vec![]));
    asm.left.push(library::lane_to_offset("L", OffsetTarget::Table(vec![(0.0, -3.0), (100.0, -5.0)]), pct(0.0), vec![]));
    let m = section(asm, &straight(102.0), &plane(0.0), 50.0);
    close(pt(&m, "ETW", Side::Right).offset_m, 4.0, "table widening right");
    close(pt(&m, "ETW", Side::Left).offset_m, -4.0, "table widening left");

    let mut asm = Assembly::new("A");
    asm.right.push(library::lane_to_offset("L", OffsetTarget::Polyline(vec![(0.0, -3.0), (100.0, -5.0)]), pct(0.0), vec![]));
    asm.left.push(library::lane_to_offset("L", OffsetTarget::Polyline(vec![(0.0, 3.0), (100.0, 5.0)]), pct(0.0), vec![]));
    let m = section(asm, &straight(102.0), &plane(0.0), 50.0);
    close(pt(&m, "ETW", Side::Right).offset_m, 4.0, "polyline widening right");
    close(pt(&m, "ETW", Side::Left).offset_m, -4.0, "polyline widening left");

    let mut asm = Assembly::new("A");
    asm.right.push(library::link_to_elevation("E", &["Top"], "PE", ElevationTarget::Fixed(103.0), pct(25.0)));
    let m = section(asm.clone(), &straight(102.0), &plane(0.0), 50.0);
    close(pt(&m, "PE", Side::Right).offset_m, 4.0, "to elevation");
    // A target the link cannot reach (below, at a rising slope) is an error naming the part.
    asm.right[0] = library::link_to_elevation("E", &["Top"], "PE", ElevationTarget::Fixed(101.0), pct(25.0));
    let err = design_section_at(&one_region(asm, Frequency::uniform(20.0)), 0, &straight(102.0), &plane(0.0), 50.0).unwrap_err();
    assert!(err.to_string().contains("subassembly 'E'"), "{err}");
    // A link to an offset.
    let mut asm = Assembly::new("A");
    asm.right.push(library::link_to_offset("O", &["Top"], "PO", OffsetTarget::Fixed(2.5), pct(-4.0)));
    let m = section(asm, &straight(102.0), &plane(0.0), 50.0);
    close(pt(&m, "PO", Side::Right).elevation_m, 102.0 - 0.1, "to offset");
}

// ─── daylight with benching ─────────────────────────────────────────────────────────────────

/// Fill on level ground at 100, hinge (4, 102), fill 1 vertical in 2 horizontal, benches every
/// 0.5 m of fall, 1 m wide, flat. Slope run per step 2 x 0.5 = 1 m. Outward:
/// (5, 101.5) (6, 101.5) (7, 101) (8, 101) (9, 100.5) (10, 100.5) (11, 100).
/// Height above ground along the earthworks line: 2 at 0 and 4, then 1.5, 1.5, 1, 1, 0.5, 0.5, 0.
/// Area of one side = 4 x 2 + (2+1.5)/2 + 1.5 + (1.5+1)/2 + 1 + (1+0.5)/2 + 0.5 + (0.5+0)/2
///   = 8 + 1.75 + 1.5 + 1.25 + 1 + 0.75 + 0.5 + 0.25 = 15; both sides 30.
/// Cut: hinge (4, 98), ground 100, 1:1, step 1 m, bench 2 m wide: (5, 99) (7, 99) (8, 100):
///   4 x 2 + (2+1)/2 + 2 x 1 + (1+0)/2 = 8 + 1.5 + 2 + 0.5 = 12; both sides 24.
#[test]
fn benched_daylight_in_fill_and_in_cut() {
    let bench = |h: f64, w: f64| Some(Bench { step_height_m: h, width_m: w, slope: pct(0.0) });
    let fill = Assembly::symmetric("A", vec![library::lane("L", 4.0, pct(0.0), vec![]), library::daylight_benched(1.0, 2.0, None, bench(0.5, 1.0))]);
    let m = section(fill, &straight(102.0), &plane(0.0), 50.0);
    let d = m.section.right_daylight.unwrap();
    assert_eq!(d.kind, SlopeKind::Fill);
    close(d.offset_m, 11.0, "fill daylight offset");
    close(d.elevation_m, 100.0, "fill daylight level");
    close(m.section.fill_area_m2.unwrap(), 30.0, "fill area");
    close(m.section.cut_area_m2.unwrap(), 0.0, "cut area");
    assert_eq!(m.design.links_with_code("Daylight_Fill").count(), 2);

    let cut = Assembly::symmetric("A", vec![library::lane("L", 4.0, pct(0.0), vec![]), library::daylight_benched(1.0, 2.0, bench(1.0, 2.0), None)]);
    let m = section(cut, &straight(98.0), &plane(0.0), 50.0);
    let d = m.section.left_daylight.unwrap();
    assert_eq!(d.kind, SlopeKind::Cut);
    close(d.offset_m, -8.0, "cut daylight offset");
    close(m.section.cut_area_m2.unwrap(), 24.0, "cut area");
}

// ─── old templates ──────────────────────────────────────────────────────────────────────────

fn crowned(depth: f64) -> Template {
    let lane = Link::new("lane", 5.0, pct(-2.0));
    let lane = if depth > 0.0 { lane.paved() } else { lane };
    Template::symmetric("crowned", vec![lane], Daylight { cut_run_per_rise: 1.0, fill_run_per_rise: 2.0 }, depth)
}

/// An old template, taken through `Assembly::from_template`, gives the same earthworks line,
/// daylight points and areas as `section_at` with the template, in fill and in cut, paved or not.
#[test]
fn old_templates_map_to_assemblies_with_the_same_sections() {
    for (level, depth) in [(102.0, 0.3), (98.0, 0.0), (101.0, 0.3), (99.5, 0.25)] {
        let t = crowned(depth);
        let (a, g) = (straight(level), plane(0.05));
        let old = section_at(&t, 0, &a, &g, 50.0).unwrap();
        let asm = Assembly::from_template(&t);
        let new = section(asm, &a, &g, 50.0).section;
        assert_eq!(old.earthworks.len(), new.earthworks.len(), "level {level}: {:?} vs {:?}", old.earthworks, new.earthworks);
        for (o, n) in old.earthworks.iter().zip(&new.earthworks) {
            close(n.0, o.0, "earthworks offset");
            close(n.1, o.1, "earthworks level");
        }
        close(new.cut_area_m2.unwrap(), old.cut_area_m2.unwrap(), "cut");
        close(new.fill_area_m2.unwrap(), old.fill_area_m2.unwrap(), "fill");
        assert_eq!(new.left_daylight, old.left_daylight);
        assert_eq!(new.right_daylight, old.right_daylight);
        assert_eq!(new.finished.iter().map(|p| p.code.as_str()).collect::<Vec<_>>(), ["lane", "CL", "lane"]);
    }
}

/// An assembly that is a template (strips, ditch, daylight) converts back; a kerb or a target or a
/// bench is refused with the part's name; layered strips of different depth are refused.
#[test]
fn assemblies_convert_to_templates_or_say_why_not() {
    let t = crowned(0.3);
    assert_eq!(Assembly::from_template(&t).to_template().unwrap(), t);
    let def = Assembly::from_template(&t).to_corridor_definition("C", 20.0).unwrap();
    assert!(Corridor::build(&def, &straight(102.0), &plane(0.05)).is_ok());

    let kerb = Assembly::symmetric("A", vec![library::kerb("Kerb", 0.1, 0.2, 0.0), library::daylight(1.0, 2.0)]);
    assert!(kerb.to_template().unwrap_err().contains("'Kerb'"));
    let bench = Assembly::symmetric("A", vec![library::daylight_benched(1.0, 2.0, Some(Bench { step_height_m: 1.0, width_m: 1.0, slope: pct(0.0) }), None)]);
    assert!(bench.to_template().unwrap_err().contains("benching"));
    let mixed = Assembly::symmetric(
        "A",
        vec![library::lane("L", 3.0, pct(-2.0), vec![Layer::new("A", 0.1)]), library::shoulder("S", 1.0, pct(-4.0), vec![Layer::new("B", 0.2)]), library::daylight(1.0, 2.0)],
    );
    assert!(mixed.to_template().unwrap_err().contains("formation depth"));
    let target = Assembly::symmetric("A", vec![library::lane_to_offset("W", OffsetTarget::Fixed(3.0), pct(0.0), vec![]), library::daylight(1.0, 2.0)]);
    assert!(target.to_template().unwrap_err().contains("target"));
    // A ditch becomes links.
    let ditch = Assembly::symmetric("A", vec![library::side_ditch_trapezoid("D", 0.5, 1.0, 2.0, 3.0), library::daylight(1.0, 2.0)]);
    assert_eq!(ditch.to_template().unwrap().right.links.len(), 3);
}

/// Entered values are checked and named.
#[test]
fn bad_parameters_are_refused_by_name() {
    let asm = Assembly::symmetric("A", vec![library::lane("Wide", 0.0, pct(0.0), vec![])]);
    let e = asm.check().unwrap_err();
    assert!(e.contains("'Wide'") && e.contains("width"), "{e}");
    let asm = Assembly::symmetric("A", vec![library::daylight(1.0, 2.0), library::lane("L", 3.0, pct(0.0), vec![])]);
    assert!(asm.check().unwrap_err().contains("last"));
    let asm = Assembly::symmetric("A", vec![library::lane("L", 3.0, pct(0.0), vec![Layer::new("X", -0.1)])]);
    assert!(asm.check().unwrap_err().contains("layer 'X'"));
}

// ─── the corridor model ─────────────────────────────────────────────────────────────────────

/// Region 1 from 0 to 40 with a tangent spacing of 20: 0, 20, 40. Region 2 from 40 to 100 with a
/// spacing of 30 (whole multiples: 60, 90) and an extra station at 55: 40, 55, 60, 90, 100. The
/// boundary at 40 has two sections. Critical stations and geometry points are off here.
#[test]
fn regions_and_frequencies_choose_the_stations() {
    let asm = Assembly::from_template(&crowned(0.0));
    let mut b = one_region(asm.clone(), Frequency { tangent_m: Some(20.0), ..Default::default() });
    b.regions[0].end_station_m = 40.0;
    b.regions.push(AssemblyRegion {
        name: "R2".into(),
        start_station_m: 40.0,
        end_station_m: 100.0,
        assembly: asm,
        frequency: Frequency { tangent_m: Some(30.0), extra_stations: vec![55.0], ..Default::default() },
    });
    let st = b.stations(&straight(102.0)).unwrap();
    let got: Vec<(f64, usize)> = st;
    assert_eq!(got, vec![(0.0, 0), (20.0, 0), (40.0, 0), (40.0, 1), (55.0, 1), (60.0, 1), (90.0, 1), (100.0, 1)]);
    // Gaps are refused.
    b.regions[1].start_station_m = 45.0;
    assert!(b.stations(&straight(102.0)).is_err());
}

/// Critical stations: a cross-fall stored at 30 gives a section there only when asked for.
#[test]
fn superelevation_critical_stations() {
    let mut a = straight(102.0);
    a.superelevation = Some(Superelevation::from_points(vec![CrossFallPoint::new(30.0, Some(-2.0), Some(-2.0)), CrossFallPoint::new(70.0, Some(-2.0), Some(-2.0))]).unwrap());
    let asm = Assembly::from_template(&crowned(0.0));
    let mut f = Frequency { tangent_m: Some(50.0), ..Default::default() };
    let b = one_region(asm.clone(), f.clone());
    assert_eq!(b.stations(&a).unwrap().iter().map(|s| s.0).collect::<Vec<_>>(), vec![0.0, 50.0, 100.0]);
    f.at_superelevation_critical = true;
    let b = one_region(asm, f);
    assert_eq!(b.stations(&a).unwrap().iter().map(|s| s.0).collect::<Vec<_>>(), vec![0.0, 30.0, 50.0, 70.0, 100.0]);
}

fn lane_assembly(width: f64) -> Assembly {
    let layers = vec![Layer::new("Pave1", 0.1).as_course("example asphalt"), Layer::new("Base", 0.2)];
    Assembly::symmetric("A", vec![library::lane("Lane", width, pct(-2.0), layers), library::daylight(1.0, 2.0)])
}

/// Level 102 on flat ground at 100, lane 4 at -2 %: hinge 101.92, fill 1 in 2: the daylight is
/// 1.92 x 2 = 3.84 m beyond the lane, offset 7.84, level 100. The Top surface spans 8 m for 100 m
/// (area 800 m²); Top and Daylight together span 15.68 m (1568 m²), and a daylight boundary keeps
/// all of it. The surface level at offset 2 is 102 - 0.04 = 101.96.
#[test]
fn corridor_surfaces_from_link_codes() {
    let model = CorridorModel { name: "C".into(), baselines: vec![one_region(lane_assembly(4.0), Frequency::uniform(20.0))], volumes: Default::default() };
    let built = build_model(&model, &[straight(102.0)], &plane(0.0)).unwrap();
    let b = &built.baselines[0];
    assert_eq!(b.sections.len(), 6);
    let top = corridor_surface(b, &["Top"], SurfaceBoundary::None).unwrap();
    close(top.plan_area(), 800.0, "top area");
    close(top.elevation_at(50.0, -2.0).unwrap(), 101.96, "level at offset 2");
    let both = corridor_surface(b, &["Top", "Daylight"], SurfaceBoundary::Daylight).unwrap();
    close(both.plan_area(), 15.68 * 100.0, "top and daylight area");
    let ring = daylight_boundary(b);
    assert_eq!(ring.len(), 12);
    close(ring[0].1.abs(), 7.84, "daylight ring");
    let datum = corridor_surface(b, &["Datum"], SurfaceBoundary::None).unwrap();
    close(datum.elevation_at(50.0, 0.0).unwrap(), 101.7, "datum at the centre");
}

/// Feature lines from the ETW code: one per side, six points (stations 0 to 100 by 20), at
/// offset 4, level 101.92: northing -4 on the right, +4 on the left.
#[test]
fn feature_lines_follow_point_codes() {
    let model = CorridorModel { name: "C".into(), baselines: vec![one_region(lane_assembly(4.0), Frequency::uniform(20.0))], volumes: Default::default() };
    let built = build_model(&model, &[straight(102.0)], &plane(0.0)).unwrap();
    let lines = feature_lines(&built.baselines[0], "ETW");
    assert_eq!(lines.len(), 2);
    let right = lines.iter().find(|l| l.side == Some(Side::Right)).unwrap();
    assert_eq!(right.points.len(), 6);
    close(right.points[2][0], 40.0, "x");
    close(right.points[2][1], -4.0, "y");
    close(right.points[2][2], 101.92, "z");
    let left = lines.iter().find(|l| l.side == Some(Side::Left)).unwrap();
    close(left.points[0][1], 4.0, "left y");
    assert!(feature_lines(&built.baselines[0], "nothing").is_empty());
}

/// Region 1 (0 to 40) a lane 4 wide, region 2 (40 to 100) 5 wide, both Pave1 0.1 and Base 0.2.
/// Pave1 area, both sides: 0.8 then 1.0. Sections at 0, 20, 40, 40, 60, 80, 100 (two at 40).
/// Pave1: 40 x 0.8 + 0 + 60 x 1.0 = 92 m³. Base: 40 x 1.6 + 60 x 2.0 = 184 m³.
#[test]
fn material_volumes_by_shape_code() {
    let mut b = one_region(lane_assembly(4.0), Frequency::uniform(20.0));
    b.regions[0].end_station_m = 40.0;
    b.regions.push(AssemblyRegion { name: "R2".into(), start_station_m: 40.0, end_station_m: 100.0, assembly: lane_assembly(5.0), frequency: Frequency::uniform(20.0) });
    let built = build_baseline(&b, &straight(102.0), &plane(0.0)).unwrap();
    assert_eq!(built.sections.len(), 7);
    let st: Vec<MaterialStation> = built.sections.iter().map(|m| MaterialStation { station_m: m.section.station_m, label: &m.section.label, design: &m.design }).collect();
    let t = material_volumes(&st);
    let total = |c: &str| t.totals.iter().find(|x| x.shape_code == c).unwrap();
    close(total("Pave1").volume_m3, 92.0, "Pave1");
    close(total("Base").volume_m3, 184.0, "Base");
    assert_eq!(total("Pave1").classification.ifc_entity.as_deref(), Some("IfcCourse"));
    close(t.rows[6].cumulative_m3["Pave1"], 92.0, "cumulative");
    assert!(t.notes.is_empty(), "{:?}", t.notes);
}

/// With constant sections the two methods agree: fill on ground at 100, level 102, lane 4 at -2 %:
/// fill area = (7.84 + 8)/2 ... computed by the engine; here only that average end area and
/// prismoidal give the same total (both L x A) and that it matches 100 x the section's area.
#[test]
fn prismoidal_and_average_end_area_agree_on_a_constant_section() {
    let b = one_region(lane_assembly(4.0), Frequency::uniform(25.0));
    let (a, g) = (straight(102.0), plane(0.0));
    let built = build_baseline(&b, &a, &g).unwrap();
    let area = built.sections[0].section.fill_area_m2.unwrap();
    let avg = baseline_earthworks(&b, &a, &g, &built, &QuantityOptions::default()).unwrap();
    let pri = baseline_earthworks(&b, &a, &g, &built, &QuantityOptions { method: VolumeMethod::Prismoidal, ..Default::default() }).unwrap();
    close(avg.total_fill_m3, 100.0 * area, "average end area");
    close(pri.total_fill_m3, 100.0 * area, "prismoidal");
    // And with the existing function on the plain sections.
    let old = earthworks(&built.plain_sections(), &VolumeOptions::default());
    close(old.total_fill_m3, avg.total_fill_m3, "existing earthworks()");
}

// ─── quantities ─────────────────────────────────────────────────────────────────────────────

fn end(station: f64, cut: f64) -> EndArea {
    EndArea { station_m: station, label: format!("{station}"), cut_m2: Some(cut), fill_m2: Some(0.0), cut_centroid_offset_m: None, fill_centroid_offset_m: None, curvature_back: 0.0, curvature_ahead: 0.0 }
}

/// A pyramid-like area A = c s², c = 2, over L = 10: A1 = 0, Am = 2 x 5² = 50, A2 = 200.
/// Exact volume c L³/3 = 2000/3 = 666.667.
/// Prismoidal: 10/6 (0 + 4 x 50 + 200) = 10/6 x 400 = 666.667.
/// Average end area: 10 x (0 + 200)/2 = 1000, which over-states it by 50 %.
#[test]
fn prismoidal_is_exact_where_average_end_area_is_not() {
    let ends = [end(0.0, 0.0), end(10.0, 200.0)];
    let mids = [Some(end(5.0, 50.0))];
    let avg = earthwork_table(&ends, &[], &QuantityOptions::default());
    close(avg.total_cut_m3, 1000.0, "average end area");
    let pri = earthwork_table(&ends, &mids, &QuantityOptions { method: VolumeMethod::Prismoidal, ..Default::default() });
    close(pri.total_cut_m3, 2000.0 / 3.0, "prismoidal");
    // Without the middle area the prismoidal volume is not computed and the length is unmeasured.
    let none = earthwork_table(&ends, &[None], &QuantityOptions { method: VolumeMethod::Prismoidal, ..Default::default() });
    assert_eq!(none.rows[1].cut_volume_m3, None);
    close(none.unmeasured_length_m, 10.0, "unmeasured");
    // Factors: cut x 1.25 in the mass ordinate, cumulative unfactored.
    let f = earthwork_table(&ends, &[], &QuantityOptions { volumes: VolumeOptions { cut_factor: 1.25, fill_factor: 1.0, curvature_correction: false }, ..Default::default() });
    close(f.rows[1].mass_ordinate_m3, 1250.0, "mass ordinate");
    close(f.rows[1].cumulative_cut_m3, 1000.0, "cumulative cut");
}

/// Existing ground at 100 (plane c = 0); proposed falls along the road, z = 100 - 0.01 x.
/// Swath 3 left, 5 right: area at x = 8 x 0.01 x = 0.08 x (cut, existing above proposed).
/// Lines at 0, 50, 100: A = 0, 4, 8. Average: 50 x 4/2 + 50 x 12/2 = 100 + 300 = 400 = the integral
/// of 0.08 x over 0 to 100 (400), as the area is linear. Prismoidal gives the same 400.
/// A level proposed surface at 99 gives 8 m² and 800 m³.
#[test]
fn volumes_between_two_surfaces_over_sample_lines() {
    let a = straight(100.0);
    let existing = plane(0.0);
    let falling = Tin::from_grid(-20.0, -60.0, 20.0, 20.0, 8, 7, |x, _| 100.0 - 0.01 * x).unwrap();
    let mut g = SampleLineGroup::new("SL", &a);
    assert_eq!(g.add_stations(&a, &SampleStations::Interval { interval_m: 50.0, start_station_m: None, end_station_m: None }, 3.0, 5.0, "SL").unwrap(), 3);
    let avg = surface_earthworks(&g, &a, &existing, &falling, &QuantityOptions::default()).unwrap();
    close(avg.rows[1].cut_area_m2.unwrap(), 4.0, "area at 50");
    close(avg.total_cut_m3, 400.0, "average end area");
    close(avg.total_fill_m3, 0.0, "no fill");
    let pri = surface_earthworks(&g, &a, &existing, &falling, &QuantityOptions { method: VolumeMethod::Prismoidal, ..Default::default() }).unwrap();
    close(pri.total_cut_m3, 400.0, "prismoidal");
    let level = Tin::from_grid(-20.0, -60.0, 20.0, 20.0, 8, 7, |_, _| 99.0).unwrap();
    close(surface_earthworks(&g, &a, &existing, &level, &QuantityOptions::default()).unwrap().total_cut_m3, 800.0, "level proposed");
    // Filling instead: proposed 101 gives fill 8 x 100 = 800.
    let high = Tin::from_grid(-20.0, -60.0, 20.0, 20.0, 8, 7, |_, _| 101.0).unwrap();
    let t = surface_earthworks(&g, &a, &existing, &high, &QuantityOptions::default()).unwrap();
    close(t.total_fill_m3, 800.0, "fill");
    let csv = quantity_csv(&t);
    assert!(csv.starts_with(QUANTITY_CSV_HEADER));
}

/// Mass curve 0, 100, 0 at stations 0, 50, 100. At level c in (0, 100) the curve crosses at
/// c/2 and 100 - c/2: a haul of 100 - c. Total moment = integral of (100 - c) dc = 5000 (the area
/// of the triangle). Free haul 40: the haul exceeds 40 where c < 60, so overhaul volume 60,
/// free-haul volume 40, overhaul moment = integral over 0..60 of (60 - c) dc = 1800.
#[test]
fn mass_haul_with_a_free_haul_distance() {
    let h = mass_haul(&[0.0, 50.0, 100.0], &[0.0, 100.0, 0.0], 40.0).unwrap();
    close(h.balanced_volume_m3, 100.0, "balanced");
    close(h.total_haul_moment_m3m, 5000.0, "total moment");
    close(h.overhaul_volume_m3, 60.0, "overhaul volume");
    close(h.free_haul_volume_m3, 40.0, "free haul volume");
    close(h.overhaul_moment_m3m, 1800.0, "overhaul moment");
    close(h.residual_m3, 0.0, "residual");
    // A free haul of 100 or more makes everything free.
    let h = mass_haul(&[0.0, 50.0, 100.0], &[0.0, 100.0, 0.0], 100.0).unwrap();
    close(h.overhaul_volume_m3, 0.0, "all free");
    close(h.overhaul_moment_m3m, 0.0, "all free moment");
    // Borrow first: 0, -60, 0 at 0, 30, 100: haul L(c) = 100 (c + 60)/60 over c in (-60, 0).
    // Total moment 60 x 100/2 = 3000. Free haul 50: L > 50 for c > -30: overhaul volume 30;
    // overhaul moment = 0.5 x 30 x 50 = 750.
    let h = mass_haul(&[0.0, 30.0, 100.0], &[0.0, -60.0, 0.0], 50.0).unwrap();
    close(h.total_haul_moment_m3m, 3000.0, "borrow moment");
    close(h.overhaul_volume_m3, 30.0, "borrow overhaul volume");
    close(h.overhaul_moment_m3m, 750.0, "borrow overhaul moment");
    // A curve that only climbs balances nothing: all of it leaves the length.
    let h = mass_haul(&[0.0, 50.0, 100.0], &[0.0, 100.0, 150.0], 10.0).unwrap();
    close(h.balanced_volume_m3, 0.0, "nothing balanced");
    close(h.residual_m3, 150.0, "residual");
    assert!(mass_haul(&[0.0], &[0.0], 1.0).is_err());
    assert!(mass_haul(&[0.0, 1.0], &[0.0, 1.0], -1.0).is_err());
}

// ─── sample lines ───────────────────────────────────────────────────────────────────────────

/// Interval 25 from 0 to 100: five lines. A listed station adds one; one on an existing line is
/// skipped. Per-line swaths are editable. Ground: plane c = 0.05 so z = 100 + 0.05 o; swath 10 each
/// side gives ends (-10, 99.5) and (10, 100.5); after editing line 1 to 5 left and 15 right the
/// line runs (-5, 99.75) to (15, 100.75).
#[test]
fn sample_lines_and_section_views() {
    let a = straight(102.0);
    let g = plane(0.05);
    let mut group = SampleLineGroup::new("G", &a);
    let n = group.add_stations(&a, &SampleStations::Interval { interval_m: 25.0, start_station_m: None, end_station_m: None }, 10.0, 10.0, "SL").unwrap();
    assert_eq!(n, 5);
    assert_eq!(group.add_stations(&a, &SampleStations::Listed(vec![30.0, 50.0]), 10.0, 10.0, "X").unwrap(), 1);
    assert_eq!(group.add_stations(&a, &SampleStations::GeometryPoints, 10.0, 10.0, "Geo").unwrap(), 0);
    assert!(group.add_stations(&a, &SampleStations::Listed(vec![500.0]), 10.0, 10.0, "X").is_err());
    assert!(group.add_line("bad", 10.0, 0.0, 0.0).is_err());
    assert_eq!(group.lines.iter().map(|l| l.station_m).collect::<Vec<_>>(), vec![0.0, 25.0, 30.0, 50.0, 75.0, 100.0]);
    assert_eq!(group.lines[1].name, "SL 0+025.000");
    group.set_swath(1, 5.0, 15.0).unwrap();
    group.rename_line(1, "Culvert").unwrap();

    let baseline = one_region(lane_assembly(4.0), Frequency::uniform(20.0));
    let top = Tin::from_grid(-20.0, -60.0, 20.0, 20.0, 8, 7, |_, y| 101.0 - 0.0 * y).unwrap();
    let views = section_views(&group, &a, &[("existing", &g), ("proposed", &top)], Some(CorridorSampling { baseline: &baseline, ground: &g })).unwrap();
    assert_eq!(views.len(), 6);
    let v = &views[1];
    assert_eq!(v.line.name, "Culvert");
    let run = &v.surface("existing").unwrap().runs[0];
    close(run[0].0, -5.0, "left end offset");
    close(run[0].1, 99.75, "left end level");
    close(run[run.len() - 1].0, 15.0, "right end offset");
    close(run[run.len() - 1].1, 100.75, "right end level");
    close(v.surface("proposed").unwrap().runs[0][0].1, 101.0, "proposed level");
    let d = v.design.as_ref().unwrap();
    assert!(d.geometry.points_with_code("ETW").count() == 2);
    close(v.profile_elevation_m.unwrap(), 102.0, "profile");
    // Material volumes between the sample lines: Pave1 0.8 m² everywhere, 100 m long: 80 m³.
    let t = views_material_volumes(&views);
    close(t.totals.iter().find(|x| x.shape_code == "Pave1").unwrap().volume_m3, 80.0, "Pave1 between sample lines");

    // A group for another alignment is refused.
    let mut other = group.clone();
    other.alignment = "Elsewhere".into();
    assert!(section_views(&other, &a, &[], None).is_err());
}

// ─── serde ──────────────────────────────────────────────────────────────────────────────────

#[test]
fn everything_round_trips_through_json_and_old_documents_load() {
    let mut asm = Assembly::symmetric(
        "A",
        vec![
            library::lane_to_offset("Lane", OffsetTarget::Polyline(vec![(0.0, 1.0), (10.0, 2.0)]), pct(-2.0), vec![Layer::new("Pave1", 0.1).as_course("m")]),
            library::link_to_elevation("E", &["Top"], "P", ElevationTarget::Profile(Profile::new("P", vec![Pvi::point(0.0, 1.0), Pvi::point(10.0, 2.0)]).unwrap()), pct(10.0)),
            library::kerb("K", 0.1, 0.2, 0.0).classified(Classification::default().with_cost_code("C-1")).rename_code("Top", "KerbTop"),
            library::daylight_benched(1.0, 2.0, Some(Bench { step_height_m: 1.0, width_m: 1.0, slope: Slope::Ratio { run: 50.0, rise: 1.0 } }), None),
        ],
    );
    asm.baseline_offset_m = 0.5;
    let s = serde_json::to_string(&asm).unwrap();
    assert_eq!(serde_json::from_str::<Assembly>(&s).unwrap(), asm);
    let model = CorridorModel { name: "C".into(), baselines: vec![one_region(asm, Frequency::uniform(10.0))], volumes: VolumeOptions::default() };
    let s = serde_json::to_string(&model).unwrap();
    assert_eq!(serde_json::from_str::<CorridorModel>(&s).unwrap(), model);
    let mut group = SampleLineGroup::new("G", &straight(1.0));
    group.add_line("a", 5.0, 1.0, 2.0).unwrap();
    let s = serde_json::to_string(&group).unwrap();
    assert_eq!(serde_json::from_str::<SampleLineGroup>(&s).unwrap(), group);
    let s = serde_json::to_string(&mass_haul(&[0.0, 1.0], &[0.0, 1.0], 1.0).unwrap()).unwrap();
    assert!(s.contains("overhaul_moment_m3m"));

    // Defaults: a minimal assembly and baseline load.
    let a: Assembly = serde_json::from_str(r#"{"name":"x"}"#).unwrap();
    assert!(a.left.is_empty() && a.baseline_offset_m == 0.0);
    let b: Baseline = serde_json::from_str(r#"{"name":"b","alignment":"Test","regions":[]}"#).unwrap();
    assert!(b.crossfall_table.is_empty());

    // An old template document (as saved) still loads and maps to an assembly.
    let old = serde_json::to_string(&crowned(0.3)).unwrap();
    let t: Template = serde_json::from_str(&old).unwrap();
    let asm = Assembly::from_template(&t);
    assert_eq!(asm.left.len(), 2);
    assert_eq!(asm.to_template().unwrap(), t);
    let old_def = serde_json::to_string(&CorridorDefinition::single("C", t, 20.0)).unwrap();
    assert!(serde_json::from_str::<CorridorDefinition>(&old_def).is_ok());
}

// ─── one link, an offset target and an elevation target together ───────────────────────────

/// Centre 102. Right: the end is at offset 4 and level 103, so the link rises 1 m in 4 m (25 %) with
/// no slope entered. Left: offset -4, level 101: falls 1 m in 4 m. A table pair varies with the
/// station: at station 50 the offset is (3 + 5)/2 = 4 and the level (103 + 105)/2 = 104.
#[test]
fn a_link_takes_an_offset_target_and_an_elevation_target_together() {
    let mut asm = Assembly::new("A");
    asm.right.push(library::link_to_offset_and_elevation("R", &["Top"], "PR", OffsetTarget::Fixed(4.0), ElevationTarget::Fixed(103.0)));
    asm.left.push(library::link_to_offset_and_elevation("L", &["Top"], "PL", OffsetTarget::Fixed(-4.0), ElevationTarget::Fixed(101.0)));
    let m = section(asm, &straight(102.0), &plane(0.0), 50.0);
    close(pt(&m, "PR", Side::Right).offset_m, 4.0, "right end offset");
    close(pt(&m, "PR", Side::Right).elevation_m, 103.0, "right end level");
    close(pt(&m, "PL", Side::Left).offset_m, -4.0, "left end offset");
    close(pt(&m, "PL", Side::Left).elevation_m, 101.0, "left end level");

    let mut asm = Assembly::new("A");
    asm.right.push(library::link_to_offset_and_elevation("R", &["Top"], "PR", OffsetTarget::Table(vec![(0.0, 3.0), (100.0, 5.0)]), ElevationTarget::Table(vec![(0.0, 103.0), (100.0, 105.0)])));
    let m = section(asm, &straight(102.0), &plane(0.0), 50.0);
    close(pt(&m, "PR", Side::Right).offset_m, 4.0, "table offset");
    close(pt(&m, "PR", Side::Right).elevation_m, 104.0, "table level");

    // An offset that is not outward of the attachment point is an error naming the part.
    let mut asm = Assembly::new("A");
    asm.right.push(library::link_to_offset_and_elevation("R", &["Top"], "PR", OffsetTarget::Fixed(-1.0), ElevationTarget::Fixed(103.0)));
    let err = design_section_at(&one_region(asm, Frequency::uniform(20.0)), 0, &straight(102.0), &plane(0.0), 50.0).unwrap_err();
    assert!(err.to_string().contains("subassembly 'R'"), "{err}");
}

/// The target alignment runs east at northing -4 (offset 4 to the right of the baseline) from
/// x = 20, station 1000, profile 105 at 1000 rising 0.01 per metre. The link end at baseline station
/// 50 is at (50, -4): station 1030 of the target, level 105 + 0.3 = 105.3. Read at the baseline
/// station (50) the target profile would be off its range (1000 to 1200).
#[test]
fn an_elevation_target_is_read_at_the_station_of_the_target_alignment_nearest_the_section_point() {
    let mut target = AlignmentBuilder::new("T", 1000.0, 20.0, -4.0, 0.0).line(200.0).build().unwrap();
    target.profile = Some(Profile::new("TP", vec![Pvi::point(1000.0, 105.0), Pvi::point(1200.0, 107.0)]).unwrap());
    let nearest = ElevationTarget::NearestOnAlignment(Box::new(target.clone()));
    let mut asm = Assembly::new("A");
    asm.right.push(library::link_to_offset_and_elevation("R", &["Top"], "PR", OffsetTarget::Fixed(4.0), nearest.clone()));
    let m = section(asm, &straight(102.0), &plane(0.0), 50.0);
    close(pt(&m, "PR", Side::Right).elevation_m, 105.3, "level at the nearest station of the target");
    let m = section({ let mut a = Assembly::new("A"); a.right.push(library::link_to_offset_and_elevation("R", &["Top"], "PR", OffsetTarget::Fixed(4.0), nearest.clone())); a }, &straight(102.0), &plane(0.0), 90.0);
    close(pt(&m, "PR", Side::Right).elevation_m, 105.0 + 0.01 * 70.0, "level at station 90");

    // A link to an elevation alone reads at its attachment point: a lane 4 m wide then a link at 25 %
    // to the target level 105.3 ... the attachment point (4, level 102) is at (50, -4), station 1030.
    let mut asm = Assembly::new("A");
    asm.right.push(library::lane("Lane", 4.0, pct(0.0), vec![]));
    asm.right.push(library::link_to_elevation("E", &["Top"], "PE", nearest, pct(25.0)));
    let m = section(asm, &straight(102.0), &plane(0.0), 50.0);
    // (105.3 - 102) / 0.25 = 13.2 m further out, to offset 17.2.
    close(pt(&m, "PE", Side::Right).offset_m, 4.0 + 13.2, "link to the level of the target alignment");

    // No foot on the target (the section point is beyond its end) and no profile are errors that say so.
    let mut short = AlignmentBuilder::new("T", 1000.0, 20.0, -4.0, 0.0).line(10.0).build().unwrap();
    short.profile = target.profile.clone();
    let mut asm = Assembly::new("A");
    asm.right.push(library::link_to_offset_and_elevation("R", &["Top"], "PR", OffsetTarget::Fixed(4.0), ElevationTarget::NearestOnAlignment(Box::new(short))));
    let err = design_section_at(&one_region(asm, Frequency::uniform(20.0)), 0, &straight(102.0), &plane(0.0), 50.0).unwrap_err();
    assert!(err.to_string().contains("perpendicular foot"), "{err}");
    let mut bare = target.clone();
    bare.profile = None;
    let mut asm = Assembly::new("A");
    asm.right.push(library::link_to_offset_and_elevation("R", &["Top"], "PR", OffsetTarget::Fixed(4.0), ElevationTarget::NearestOnAlignment(Box::new(bare))));
    let err = design_section_at(&one_region(asm, Frequency::uniform(20.0)), 0, &straight(102.0), &plane(0.0), 50.0).unwrap_err();
    assert!(err.to_string().contains("has no profile"), "{err}");
}

#[test]
fn the_new_targets_round_trip_through_json() {
    let mut asm = Assembly::new("A");
    let t = AlignmentBuilder::new("T", 0.0, 0.0, 0.0, 0.0).line(10.0).build().unwrap();
    asm.right.push(library::link_to_offset_and_elevation("R", &["Top"], "PR", OffsetTarget::Fixed(4.0), ElevationTarget::NearestOnAlignment(Box::new(t))));
    let s = serde_json::to_string(&asm).unwrap();
    assert_eq!(serde_json::from_str::<Assembly>(&s).unwrap(), asm);
}
