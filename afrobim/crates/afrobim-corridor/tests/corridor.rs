//! Sections, daylight, areas and volumes against closed-form values.
//!
//! Every width, slope, level and factor below is an arbitrary test number chosen to make the
//! hand calculation short. None is a design value.
//!
//! The usual set-up: a straight alignment running east from (0, 0), stations 0 to 100, so the
//! right of the road is south and a point at offset `o` is at northing −o. The ground is one
//! plane, `z = 100 + c·o` (rising to the right at `c` m/m), triangulated on a 20 m grid.

use afrobim_alignment::{Alignment, AlignmentBuilder, CrossFallPoint, Profile, Pvi, Superelevation, Turn};
use afrobim_corridor::{
    corridor_meshes, cut_fill_areas, earthworks, earthworks_csv, plan_lines, polygon_area, section_at, section_drawing, Corridor, CorridorDefinition,
    CorridorError, Daylight, LabelKind, Link, Slope, SlopeKind, Template, TemplateRegion, TemplateSide, Tin, VolumeOptions, EARTHWORKS_CSV_HEADER,
};

fn close(got: f64, want: f64, what: &str) {
    assert!((got - want).abs() <= 1e-9 * (1.0 + want.abs()), "{what}: got {got}, want {want}");
}

/// Straight east from (0, 0), 100 m, with a level profile.
fn straight(level: f64) -> Alignment {
    let mut a = AlignmentBuilder::new("Test", 0.0, 0.0, 0.0, 0.0).line(100.0).build().unwrap();
    a.profile = Some(Profile::new("FG", vec![Pvi::point(0.0, level), Pvi::point(100.0, level)]).unwrap());
    a
}

/// The plane z = 100 + cross × offset (offset = −northing) over x −20..120, y −60..60.
fn plane(cross: f64) -> Tin {
    Tin::from_grid(-20.0, -60.0, 20.0, 20.0, 8, 7, |_, y| 100.0 - cross * y).unwrap()
}

/// One 5 m lane each side falling at 2 %, cut slopes 1:1, fill slopes 1 vertical in 2 horizontal.
fn crowned(formation_depth_m: f64) -> Template {
    let lane = Link::new("lane", 5.0, Slope::Percent(-2.0));
    let lane = if formation_depth_m > 0.0 { lane.paved() } else { lane };
    Template::symmetric("crowned", vec![lane], Daylight { cut_run_per_rise: 1.0, fill_run_per_rise: 2.0 }, formation_depth_m)
}

fn build(template: Template, alignment: &Alignment, ground: &Tin) -> Corridor {
    Corridor::build(&CorridorDefinition::single("C", template, 20.0), alignment, ground).unwrap()
}

/// Full fill on ground rising to the right at 5 %, centreline at 102.
///
/// Finished: (−5, 101.9) (0, 102) (5, 101.9). Ground at offset o: 100 + 0.05 o.
/// Height above ground: 2.15 at −5, 2.00 at 0, 1.65 at +5.
/// Right fill slope falls 0.5 m/m while the ground rises 0.05: they close at 0.55 m/m, so the
///   daylight is 1.65 / 0.55 = 3 m out, at offset 8, level 100.4.
/// Left fill slope falls 0.5 m/m while the ground falls 0.05: closing 0.45 m/m, so 2.15 / 0.45
///   = 43/9 m out, at offset −(5 + 43/9), level 99.75 − 0.05 × 43/9.
/// Fill area = ½ (43/9)(2.15) + 5 (2.15 + 2)/2 + 5 (2 + 1.65)/2 + ½ (3)(1.65)
///           = 5.136111 + 10.375 + 9.125 + 2.475 = 27.111111 m².
/// Volume over 100 m of straight = 2711.1111 m³.
#[test]
fn full_fill_on_a_cross_slope() {
    let c = build(crowned(0.0), &straight(102.0), &plane(0.05));
    assert_eq!(c.sections.len(), 6, "0, 20, 40, 60, 80, 100");
    assert!(c.notes.is_empty(), "{:?}", c.notes);
    let area = 0.5 * (43.0 / 9.0) * 2.15 + 10.375 + 9.125 + 2.475;
    close(area, 27.0 + 1.0 / 9.0, "the hand sum");
    for s in &c.sections {
        let (l, r) = (s.left_daylight.unwrap(), s.right_daylight.unwrap());
        assert_eq!((l.kind, r.kind), (SlopeKind::Fill, SlopeKind::Fill));
        close(r.offset_m, 8.0, "right daylight offset");
        close(r.elevation_m, 100.4, "right daylight level");
        close(l.offset_m, -(5.0 + 43.0 / 9.0), "left daylight offset");
        close(l.elevation_m, 99.75 - 0.05 * 43.0 / 9.0, "left daylight level");
        close(s.fill_area_m2.unwrap(), area, "fill area");
        close(s.cut_area_m2.unwrap(), 0.0, "cut area");
        assert_eq!(s.cut_centroid_offset_m, None);
        // The same area as one polygon by the shoelace formula: along the earthworks line and
        // back along the ground between the two daylight points.
        let mut polygon = s.earthworks.clone();
        polygon.dedup();
        close(polygon_area(&polygon), area, "shoelace of the fill polygon");
        close(s.centre_elevation_m(), 102.0, "centre");
        close(s.ground_at(0.0).unwrap(), 100.0, "ground on the centreline");
    }
    assert_eq!(c.sections[1].label, "0+020.000");
    assert_eq!(c.sections[0].finished.iter().map(|p| p.code.as_str()).collect::<Vec<_>>(), ["lane", "CL", "lane"]);

    // Constant section on a straight: volume = area × length.
    let e = earthworks(&c.sections, &VolumeOptions::default());
    close(e.total_fill_m3, area * 100.0, "total fill");
    close(e.total_cut_m3, 0.0, "total cut");
    close(e.unmeasured_length_m, 0.0, "nothing unmeasured");
    close(e.rows[0].fill_volume_m3.unwrap(), 0.0, "first row");
    for (i, r) in e.rows.iter().enumerate().skip(1) {
        close(r.fill_volume_m3.unwrap(), area * 20.0, "one 20 m interval");
        close(r.cumulative_fill_m3, area * 20.0 * i as f64, "cumulative fill");
        close(r.mass_ordinate_m3, -area * 20.0 * i as f64, "mass ordinate = cut − fill");
    }
}

/// Full cut on the same ground, centreline at 97.
///
/// Finished: (−5, 96.9) (0, 97) (5, 96.9). Depth below ground: 2.85 at −5, 3.00 at 0, 3.35 at +5.
/// Right cut slope rises 1 m/m, ground rises 0.05: closing 0.95, daylight 3.35 / 0.95 m out.
/// Left cut slope rises 1 m/m, ground falls 0.05: closing 1.05, daylight 2.85 / 1.05 m out.
/// Cut area = ½ (2.85/1.05)(2.85) + 5 (2.85 + 3)/2 + 5 (3 + 3.35)/2 + ½ (3.35/0.95)(3.35)
///          = 3.867857 + 14.625 + 15.875 + 5.906579 = 40.274436 m².
#[test]
fn full_cut_on_a_cross_slope() {
    let c = build(crowned(0.0), &straight(97.0), &plane(0.05));
    let area: f64 = 0.5 * (2.85 / 1.05) * 2.85 + 14.625 + 15.875 + 0.5 * (3.35 / 0.95) * 3.35;
    assert!((area - 40.274436).abs() < 1e-6);
    let s = &c.sections[2];
    let (l, r) = (s.left_daylight.unwrap(), s.right_daylight.unwrap());
    assert_eq!((l.kind, r.kind), (SlopeKind::Cut, SlopeKind::Cut));
    close(l.offset_m, -(5.0 + 2.85 / 1.05), "left daylight");
    close(r.offset_m, 5.0 + 3.35 / 0.95, "right daylight");
    close(r.elevation_m, 96.9 + 3.35 / 0.95, "right daylight level: up the 1:1 slope");
    close(s.cut_area_m2.unwrap(), area, "cut area");
    close(s.fill_area_m2.unwrap(), 0.0, "fill area");
    let e = earthworks(&c.sections, &VolumeOptions::default());
    close(e.total_cut_m3, area * 100.0, "area × length");
    close(e.rows[5].mass_ordinate_m3, area * 100.0, "mass ordinate");
}

/// The formation is 0.5 m below both 5 m lanes. The side slopes still start from the finished
/// edge, so the daylight points do not move; the earthworks line drops 0.5 m over the 10 m of
/// lanes. Fill is 0.5 × 10 = 5 m² less than to the finished surface, cut is 5 m² more.
#[test]
fn the_formation_takes_the_pavement_out_of_the_earthworks() {
    let fill = build(crowned(0.5), &straight(102.0), &plane(0.05));
    let s = &fill.sections[0];
    close(s.fill_area_m2.unwrap(), 27.0 + 1.0 / 9.0 - 5.0, "fill to the formation");
    close(s.right_daylight.unwrap().offset_m, 8.0, "daylight unchanged");
    // Left daylight, up the slope to the finished edge, straight down 0.5, the formation, up 0.5, down the slope.
    let want = [(-(5.0 + 43.0 / 9.0), 99.75 - 0.05 * 43.0 / 9.0), (-5.0, 101.9), (-5.0, 101.4), (0.0, 101.5), (5.0, 101.4), (5.0, 101.9), (8.0, 100.4)];
    assert_eq!(s.earthworks.len(), want.len(), "{:?}", s.earthworks);
    for (g, w) in s.earthworks.iter().zip(want) {
        close(g.0, w.0, "earthworks offset");
        close(g.1, w.1, "earthworks level");
    }
    let cut = build(crowned(0.5), &straight(97.0), &plane(0.05));
    close(cut.sections[0].cut_area_m2.unwrap(), 0.5 * (2.85 / 1.05) * 2.85 + 14.625 + 15.875 + 0.5 * (3.35 / 0.95) * 3.35 + 5.0, "cut to the formation");

    // Paved on the right only: a 0.5 m step on the centreline, and half the difference.
    let mut one_sided = crowned(0.5);
    one_sided.left.links[0].paved = false;
    let s = &build(one_sided, &straight(102.0), &plane(0.05)).sections[0];
    close(s.fill_area_m2.unwrap(), 27.0 + 1.0 / 9.0 - 2.5, "fill, right lane paved");
}

/// A side-hill section: ground rising to the right at 20 %, level 5 m lanes, centreline at 100.4.
///
/// Ground = 100 + 0.2 o meets the finished level 100.4 at o = +2: fill to the left of it, cut to
/// the right.
/// Cut: under the lane from 2 to 5 the depth grows from 0 to 0.6: ½ × 3 × 0.6 = 0.9. The 1:1 cut
///   slope closes on the ground at 0.8 m/m: 0.6 / 0.8 = 0.75 m out (offset 5.75): ½ × 0.75 × 0.6
///   = 0.225. Cut = 1.125 m².
/// Fill: under the lanes from −5 (height 1.4) to 2 (height 0): ½ × 7 × 1.4 = 4.9. The fill slope
///   (0.5 m/m down) closes on ground falling at 0.2 m/m at 0.3 m/m: 1.4 / 0.3 = 14/3 m out
///   (offset −29/3): ½ × (14/3) × 1.4 = 3.266667. Fill = 8.166667 m².
#[test]
fn a_side_hill_section_has_cut_and_fill() {
    let flat = Template::symmetric("flat", vec![Link::new("lane", 5.0, Slope::Percent(0.0))], Daylight { cut_run_per_rise: 1.0, fill_run_per_rise: 2.0 }, 0.0);
    let s = section_at(&flat, 0, &straight(100.4), &plane(0.2), 37.5).unwrap();
    let (l, r) = (s.left_daylight.unwrap(), s.right_daylight.unwrap());
    assert_eq!((l.kind, r.kind), (SlopeKind::Fill, SlopeKind::Cut));
    close(l.offset_m, -29.0 / 3.0, "left daylight");
    close(r.offset_m, 5.75, "right daylight");
    close(r.elevation_m, 101.15, "right daylight level");
    close(s.cut_area_m2.unwrap(), 1.125, "cut");
    close(s.fill_area_m2.unwrap(), 4.9 + 0.5 * (14.0 / 3.0) * 1.4, "fill");
    // Centroids. The cut is the triangle (2, 100.4) (5, 100.4) (5.75, 101.15): its centroid is at
    // the mean offset of its corners, (2 + 5 + 5.75) / 3 = 4.25.
    close(s.cut_centroid_offset_m.unwrap(), 4.25, "cut centroid");
    // The fill is the triangle (2, 100.4) (−5, 100.4) (−29/3, 98.0667): the ground is its long
    // side. Mean offset (2 − 5 − 29/3) / 3.
    close(s.fill_centroid_offset_m.unwrap(), (2.0 - 5.0 - 29.0 / 3.0) / 3.0, "fill centroid");

    // On a straight, a section between regular stations is the same: 37.5 is as good as 40.
    assert_eq!(s.label, "0+037.500");
}

/// The earthworks line level at 0 from 0 to 8, the ground a zigzag ±1 with a period of 4:
/// they cross at 1, 3, 5 and 7. Cut triangles: ½ on [0,1], 1 on [3,5], ½ on [7,8] = 2. Fill: 1 on
/// [1,3] and 1 on [5,7] = 2.
#[test]
fn areas_are_split_at_every_crossing() {
    let level = [(0.0, 0.0), (8.0, 0.0)];
    let zigzag = [(0.0, 1.0), (2.0, -1.0), (4.0, 1.0), (6.0, -1.0), (8.0, 1.0)];
    let a = cut_fill_areas(&level, &zigzag);
    close(a.cut_m2, 2.0, "cut");
    close(a.fill_m2, 2.0, "fill");
    // Each loop on its own by the shoelace formula.
    let cut = polygon_area(&[(0.0, 0.0), (0.0, 1.0), (1.0, 0.0)]) + polygon_area(&[(3.0, 0.0), (4.0, 1.0), (5.0, 0.0)]) + polygon_area(&[(7.0, 0.0), (8.0, 1.0), (8.0, 0.0)]);
    let fill = polygon_area(&[(1.0, 0.0), (2.0, -1.0), (3.0, 0.0)]) + polygon_area(&[(5.0, 0.0), (6.0, -1.0), (7.0, 0.0)]);
    close(a.cut_m2, cut, "cut against shoelace");
    close(a.fill_m2, fill, "fill against shoelace");
    // Moments: cut ½ × ⅓ + 1 × 4 + ½ × (8 − ⅓) = 8; fill 1 × 2 + 1 × 6 = 8.
    close(a.cut_moment_m3, 8.0, "cut moment");
    close(a.fill_moment_m3, 8.0, "fill moment");

    // Vertical steps: a 2 m wide, 1 m deep box in a line at 0, under ground at 0.5.
    // Cut = 6 × 0.5 + 2 × 1 = 5.
    let boxed = [(0.0, 0.0), (2.0, 0.0), (2.0, -1.0), (4.0, -1.0), (4.0, 0.0), (6.0, 0.0)];
    let a = cut_fill_areas(&boxed, &[(-1.0, 0.5), (7.0, 0.5)]);
    close(a.cut_m2, 5.0, "boxed cut");
    close(a.fill_m2, 0.0, "boxed fill");
    // Only the shared width counts: ground from 1 to 3 gives 1 × 0.5 + 1 × 1.5 = 2.
    close(cut_fill_areas(&boxed, &[(1.0, 0.5), (3.0, 0.5)]).cut_m2, 2.0, "shared width");
}

/// The ground stops 7 m to the right of the centreline. The right fill slope needs offset 8.8
/// (level ground at 100, edge at 101.9, 1.9 × 2 = 3.8 m out) and leaves the surface first.
#[test]
fn no_daylight_is_reported_not_guessed() {
    let narrow = Tin::from_grid(-20.0, -7.0, 20.0, 9.0, 8, 4, |_, _| 100.0).unwrap(); // y −7..20
    let c = build(crowned(0.0), &straight(102.0), &narrow);
    let s = &c.sections[1];
    assert!(s.right_daylight.is_none());
    close(s.left_daylight.unwrap().offset_m, -8.8, "the left side still daylights");
    assert_eq!((s.cut_area_m2, s.fill_area_m2), (None, None));
    assert_eq!(s.notes, vec!["No daylight on the right: the fill slope leaves the ground surface before meeting it.".to_string()]);
    assert_eq!(s.earthworks.last().copied(), Some((5.0, 101.9)), "the line stops at the template edge");
    assert_eq!(c.notes.len(), 1);
    assert!(c.notes[0].starts_with("6 of 6 sections have no cut and fill areas"), "{:?}", c.notes);

    let e = earthworks(&c.sections, &VolumeOptions::default());
    assert_eq!(e.rows[1].fill_volume_m3, None);
    close(e.unmeasured_length_m, 100.0, "the whole length");
    close(e.total_fill_m3, 0.0, "nothing is counted");
    assert_eq!(earthworks_csv(&e).lines().nth(2).unwrap(), "0+020.000,,,,,0.000,0.000,0.000");
    let lines = plan_lines(&c);
    assert!(lines.right_daylight.is_empty());
    assert_eq!(lines.left_daylight.len(), 1);

    // A template whose edge is itself off the surface, and a centreline off the surface.
    let wide = Template::symmetric("wide", vec![Link::new("lane", 9.0, Slope::Percent(0.0))], Daylight { cut_run_per_rise: 1.0, fill_run_per_rise: 2.0 }, 0.0);
    let s = section_at(&wide, 0, &straight(102.0), &narrow, 50.0).unwrap();
    assert_eq!(s.notes[0], "No daylight on the right: the outer end of the template is off the ground surface.");
    let away = Tin::from_grid(-20.0, 30.0, 20.0, 10.0, 8, 3, |_, _| 100.0).unwrap();
    let s = section_at(&wide, 0, &straight(102.0), &away, 50.0).unwrap();
    assert_eq!(s.notes, vec!["The centreline is off the ground surface.".to_string()]);
    assert!(s.ground.is_empty() && s.left_daylight.is_none() && s.cut_area_m2.is_none());
}

/// The edge of the template exactly on the ground: no slope at all on that side.
#[test]
fn an_edge_on_the_ground_needs_no_slope() {
    let flat = Template::symmetric("flat", vec![Link::new("lane", 5.0, Slope::Percent(0.0))], Daylight { cut_run_per_rise: 1.0, fill_run_per_rise: 2.0 }, 0.0);
    let s = section_at(&flat, 0, &straight(100.0), &plane(0.0), 10.0).unwrap();
    assert_eq!(s.left_daylight.unwrap().kind, SlopeKind::AtGround);
    close(s.right_daylight.unwrap().offset_m, 5.0, "at the edge");
    assert_eq!((s.cut_area_m2, s.fill_area_m2), (Some(0.0), Some(0.0)));
    assert_eq!(s.earthworks.len(), 3);
}

/// Links that follow superelevation take the stored cross-fall of their side; the others, and
/// every link where nothing is stored, keep the entered slope.
#[test]
fn links_follow_the_stored_cross_fall() {
    let mut a = straight(102.0);
    // Stored from 0 to 60 only: left −4 %, right +4 % (one plane rising to the right). At 80 nothing.
    a.superelevation = Some(
        Superelevation::from_points(vec![CrossFallPoint::new(0.0, Some(-4.0), Some(4.0)), CrossFallPoint::new(60.0, Some(-4.0), None)]).unwrap(),
    );
    let links = vec![Link::new("lane", 5.0, Slope::Percent(-2.0)).following_superelevation(), Link::new("shoulder", 2.0, Slope::Ratio { run: 4.0, rise: -1.0 })];
    let t = Template::symmetric("se", links, Daylight { cut_run_per_rise: 1.0, fill_run_per_rise: 2.0 }, 0.0);
    let level = |s: &afrobim_corridor::Section, i: usize| s.finished[i].elevation_m;

    let s = section_at(&t, 0, &a, &plane(0.0), 0.0).unwrap();
    assert_eq!(s.finished.iter().map(|p| p.offset_m).collect::<Vec<_>>(), [-7.0, -5.0, 0.0, 5.0, 7.0]);
    close(level(&s, 1), 102.0 - 0.2, "left lane at −4 %");
    close(level(&s, 0), 102.0 - 0.2 - 0.5, "left shoulder keeps its 1 in 4 fall");
    close(level(&s, 3), 102.0 + 0.2, "right lane at +4 %");
    close(level(&s, 4), 102.0 + 0.2 - 0.5, "right shoulder");

    // At 30 the right side has no value on one of the two stated stations: entered −2 %.
    let s = section_at(&t, 0, &a, &plane(0.0), 30.0).unwrap();
    close(level(&s, 1), 102.0 - 0.2, "left still stored");
    close(level(&s, 3), 102.0 - 0.1, "right falls back to the entered slope");
    // Beyond the stored range both sides are the entered slope.
    let s = section_at(&t, 0, &a, &plane(0.0), 80.0).unwrap();
    close(level(&s, 1), 102.0 - 0.1, "left entered");

    // The corridor puts a section on each stored station: 0, 20, 40, 60, 80, 100 already has 60.
    let c = build(t, &a, &plane(0.0));
    assert_eq!(c.sections.len(), 6);
}

/// Sections stand on the interval, the geometry points and the ends.
#[test]
fn sections_stand_on_geometry_points() {
    // Line 0..30, arc to 70, line to 100; a 30 m parabola on a PVI at 50 (35 to 65).
    let mut a = AlignmentBuilder::new("G", 0.0, 0.0, 0.0, 0.0).line(30.0).arc(500.0, 40.0, Turn::Left).line(30.0).build().unwrap();
    a.profile = Some(Profile::new("FG", vec![Pvi::point(0.0, 102.0), Pvi::parabola(50.0, 102.5, 30.0), Pvi::point(100.0, 102.0)]).unwrap());
    let big = Tin::from_grid(-50.0, -100.0, 50.0, 50.0, 5, 5, |_, _| 100.0).unwrap();
    let c = build(crowned(0.0), &a, &big);
    let stations: Vec<f64> = c.sections.iter().map(|s| s.station_m).collect();
    assert_eq!(stations, [0.0, 20.0, 30.0, 35.0, 40.0, 50.0, 60.0, 65.0, 70.0, 80.0, 100.0]);
    // Curvature each side of the tangent point at 30, and inside the arc.
    assert_eq!((c.sections[2].curvature_back, c.sections[2].curvature_ahead), (0.0, 1.0 / 500.0));
    assert_eq!((c.sections[4].curvature_back, c.sections[4].curvature_ahead), (1.0 / 500.0, 1.0 / 500.0));
    close(c.sections[5].centre_elevation_m(), a.profile.as_ref().unwrap().elevation_at(50.0).unwrap(), "the profile gives the centre level");

    // Entered limits: within the alignment they are kept; beyond it they are cut back with a note.
    let mut d = CorridorDefinition::single("C", crowned(0.0), 25.0);
    d.start_station_m = Some(10.0);
    d.end_station_m = Some(250.0);
    let c = Corridor::build(&d, &straight(102.0), &plane(0.0)).unwrap();
    assert_eq!(c.sections.iter().map(|s| s.station_m).collect::<Vec<_>>(), [10.0, 25.0, 50.0, 75.0, 100.0]);
    assert_eq!((c.start_station_m, c.end_station_m), (10.0, 100.0));
    assert_eq!(c.notes.len(), 1);
}

/// Two regions: 5 m lanes to station 50, 7 m lanes after it. Level ground at 100, centre at 102,
/// lanes level, fill slopes 1 in 2 (4 m out for the 2 m height).
/// Region 1 area = 10 × 2 + 2 × ½ × 4 × 2 = 28. Region 2 = 14 × 2 + 8 = 36.
/// The change is a step at 50: volume = 28 × 50 + 36 × 50 = 3200.
#[test]
fn template_regions_change_in_a_step() {
    let t = |w: f64| Template::symmetric("t", vec![Link::new("lane", w, Slope::Percent(0.0))], Daylight { cut_run_per_rise: 1.0, fill_run_per_rise: 2.0 }, 0.0);
    let d = CorridorDefinition {
        name: "two".to_string(),
        regions: vec![TemplateRegion { start_station_m: 0.0, template: t(5.0) }, TemplateRegion { start_station_m: 50.0, template: t(7.0) }],
        interval_m: 20.0,
        start_station_m: None,
        end_station_m: None,
        volumes: VolumeOptions::default(),
    };
    let c = Corridor::build(&d, &straight(102.0), &plane(0.0)).unwrap();
    let rows: Vec<(f64, usize, f64)> = c.sections.iter().map(|s| (s.station_m, s.region, s.fill_area_m2.unwrap())).collect();
    let want = [(0.0, 0, 28.0), (20.0, 0, 28.0), (40.0, 0, 28.0), (50.0, 0, 28.0), (50.0, 1, 36.0), (60.0, 1, 36.0), (80.0, 1, 36.0), (100.0, 1, 36.0)];
    assert_eq!(rows.len(), want.len());
    for (g, w) in rows.iter().zip(want) {
        assert_eq!((g.0, g.1), (w.0, w.1));
        close(g.2, w.2, "area");
    }
    let e = earthworks(&c.sections, &d.volumes);
    close(e.total_fill_m3, 3200.0, "28 × 50 + 36 × 50");
    close(e.rows[4].fill_volume_m3.unwrap(), 0.0, "nothing between the two sections at 50");
    // No top strip joins the two templates: 3 + 3 intervals of 2 links.
    assert_eq!(corridor_meshes(&c).top.triangles.len(), 6 * 2 * 2);

    // Regions must be in order.
    let mut bad = d.clone();
    bad.regions[1].start_station_m = 0.0;
    assert!(matches!(Corridor::build(&bad, &straight(102.0), &plane(0.0)), Err(CorridorError::Definition(m)) if m.contains("region 2 must start after region 1")));
}

/// A constant one-sided section on a circular arc, where Pappus's theorem is exact.
///
/// Left-hand arc of radius 200 m, 100 m long (0.5 rad). Level ground at 100, centre at 102. No
/// links on the left; one level 6 m link on the right; fill slopes 1 in 2 (4 m out).
/// Area = ½ × 4 × 2 (left slope) + 6 × 2 + ½ × 4 × 2 (right slope) = 20 m².
/// Moment about the centreline = 4 × (−4/3) + 12 × 3 + 4 × (6 + 4/3) = 60, so the centroid is
/// 3 m to the right: on the outside of a left-hand curve, radius 203 m.
/// Exact volume = area × angle × centroid radius = 20 × 0.5 × 203 = 2030 m³.
/// By centreline length alone: 20 × 100 = 2000 m³.
#[test]
fn curvature_correction_matches_pappus() {
    let mut a = AlignmentBuilder::new("Arc", 0.0, 0.0, 0.0, 0.0).arc(200.0, 100.0, Turn::Left).build().unwrap();
    a.profile = Some(Profile::new("FG", vec![Pvi::point(0.0, 102.0), Pvi::point(100.0, 102.0)]).unwrap());
    let ground = Tin::from_grid(-100.0, -100.0, 50.0, 50.0, 7, 7, |_, _| 100.0).unwrap();
    let day = Daylight { cut_run_per_rise: 1.0, fill_run_per_rise: 2.0 };
    let t = Template {
        name: "one-sided".to_string(),
        left: TemplateSide { links: vec![], daylight: day },
        right: TemplateSide { links: vec![Link::new("verge", 6.0, Slope::Percent(0.0))], daylight: day },
        formation_depth_m: 0.0,
    };
    let c = build(t, &a, &ground);
    for s in &c.sections {
        close(s.fill_area_m2.unwrap(), 20.0, "area");
        close(s.fill_centroid_offset_m.unwrap(), 3.0, "centroid");
        close(s.left_daylight.unwrap().offset_m, -4.0, "left daylight from the centreline");
    }
    let plain = earthworks(&c.sections, &VolumeOptions::default());
    close(plain.total_fill_m3, 2000.0, "uncorrected");
    let corrected = earthworks(&c.sections, &VolumeOptions { curvature_correction: true, ..VolumeOptions::default() });
    close(corrected.total_fill_m3, 2030.0, "Pappus");
    // Turning the other way puts the same area on the inside: radius 197 m.
    let mut right = AlignmentBuilder::new("Arc", 0.0, 0.0, 0.0, 0.0).arc(200.0, 100.0, Turn::Right).build().unwrap();
    right.profile = a.profile.clone();
    let c = build(c_template(day), &right, &ground);
    close(earthworks(&c.sections, &VolumeOptions { curvature_correction: true, ..VolumeOptions::default() }).total_fill_m3, 1970.0, "inside of the curve");
}

fn c_template(day: Daylight) -> Template {
    Template {
        name: "one-sided".to_string(),
        left: TemplateSide { links: vec![], daylight: day },
        right: TemplateSide { links: vec![Link::new("verge", 6.0, Slope::Percent(0.0))], daylight: day },
        formation_depth_m: 0.0,
    }
}

/// Cut on the first half, fill on the second, with entered factors.
///
/// Level ground at 100, level 5 m lanes, cut 1:1, fill 1 in 2. Profile 98 from 0 to 40, then a
/// straight grade up to 102 at 60, then 102: PVIs without curves.
/// At 98: cut = 10 × 2 + 2 × ½ × 2 × 2 = 24. At 102: fill = 10 × 2 + 2 × ½ × 4 × 2 = 28.
/// At 50 the road is on the ground: both zero.
/// Cut = 24 × 40 + ½ (24 + 0) × 10 = 1080. Fill = ½ (0 + 28) × 10 + 28 × 40 = 1260.
/// With a cut factor of 0.9 and a fill factor of 1.25: ordinate = 0.9 × 1080 − 1.25 × 1260 = −603.
#[test]
fn mass_ordinate_with_entered_factors_and_the_csv() {
    let mut a = straight(0.0);
    a.profile = Some(Profile::new("FG", vec![Pvi::point(0.0, 98.0), Pvi::point(40.0, 98.0), Pvi::point(60.0, 102.0), Pvi::point(100.0, 102.0)]).unwrap());
    let flat = Template::symmetric("flat", vec![Link::new("lane", 5.0, Slope::Percent(0.0))], Daylight { cut_run_per_rise: 1.0, fill_run_per_rise: 2.0 }, 0.0);
    let mut d = CorridorDefinition::single("C", flat, 10.0);
    d.volumes = VolumeOptions { cut_factor: 0.9, fill_factor: 1.25, curvature_correction: false };
    let c = Corridor::build(&d, &a, &plane(0.0)).unwrap();
    assert_eq!(c.sections.len(), 11);
    let e = earthworks(&c.sections, &d.volumes);
    close(e.total_cut_m3, 1080.0, "cut");
    close(e.total_fill_m3, 1260.0, "fill");
    close(e.rows[10].mass_ordinate_m3, 0.9 * 1080.0 - 1.25 * 1260.0, "mass ordinate");
    close(e.rows[4].mass_ordinate_m3, 0.9 * 960.0, "the high point of the mass curve, at 40");
    // With no factors the ordinate is cut minus fill.
    close(earthworks(&c.sections, &VolumeOptions::default()).rows[10].mass_ordinate_m3, 1080.0 - 1260.0, "unfactored");

    let csv = earthworks_csv(&e);
    let lines: Vec<&str> = csv.lines().collect();
    assert_eq!(lines.len(), 12);
    assert_eq!(lines[0], EARTHWORKS_CSV_HEADER);
    assert_eq!(lines[0], "Station,Cut area (m2),Fill area (m2),Cut volume (m3),Fill volume (m3),Cumulative cut (m3),Cumulative fill (m3),Mass ordinate (m3)");
    assert_eq!(lines[1], "0+000.000,24.000,0.000,0.000,0.000,0.000,0.000,0.000");
    assert_eq!(lines[2], "0+010.000,24.000,0.000,240.000,0.000,240.000,0.000,216.000");
    assert_eq!(lines[6], "0+050.000,0.000,0.000,120.000,0.000,1080.000,0.000,972.000");
    assert_eq!(lines[7], "0+060.000,0.000,28.000,0.000,140.000,1080.000,140.000,797.000");
    assert_eq!(lines[11], "0+100.000,0.000,28.000,0.000,280.000,1080.000,1260.000,-603.000");
    assert!(csv.ends_with('\n'));

    // Factors must be positive.
    d.volumes.fill_factor = 0.0;
    assert!(matches!(Corridor::build(&d, &a, &plane(0.0)), Err(CorridorError::Definition(m)) if m.contains("fill factor")));
}

#[test]
fn the_cross_section_drawing() {
    let c = build(crowned(0.5), &straight(102.0), &plane(0.05));
    let s = &c.sections[1];
    let d = section_drawing(s, 2.0);
    // Ground: 1 segment (a plane); earthworks: 6; finished: 2; centreline: 1.
    assert_eq!(d.drawing.primitives.len(), 10);
    close(d.origin_offset_m, s.ground[0].0, "the ground reaches furthest left");
    close(d.origin_level_m, s.ground[0].1, "and is lowest there");
    assert_eq!(d.vertical_exaggeration, 2.0);
    // The centre of the road, mapped as documented.
    let centre = d.labels.iter().find(|l| l.kind == LabelKind::Design && l.text.starts_with("CL")).unwrap();
    close(d.origin_offset_m + centre.at.x / 1000.0, 0.0, "offset from drawing x");
    close(d.origin_level_m + centre.at.y / 2000.0, 102.0, "level from drawing y");
    assert_eq!(centre.text, "CL +0.000 / 102.000");
    let text = |k: LabelKind| d.labels.iter().filter(|l| l.kind == k).map(|l| l.text.as_str()).collect::<Vec<_>>();
    assert_eq!(text(LabelKind::Title), ["Station 0+020.000"]);
    assert_eq!(text(LabelKind::Design), ["lane -5.000 / 101.900", "CL +0.000 / 102.000", "lane +5.000 / 101.900"]);
    assert_eq!(text(LabelKind::Daylight), ["fill daylight -9.778 / 99.511", "fill daylight +8.000 / 100.400"]);
    assert_eq!(text(LabelKind::Ground), ["ground 100.000"]);
    assert_eq!(text(LabelKind::Areas), ["Cut 0.000 m2   Fill 22.111 m2"]);
    assert!(d.drawing.extents.min.x == 0.0 && d.drawing.extents.min.y == 0.0 && d.drawing.extents.max.x > 17_000.0);
    // Natural scale when the exaggeration is nonsense.
    assert_eq!(section_drawing(s, 0.0).vertical_exaggeration, 1.0);
}

#[test]
fn meshes_and_plan_lines() {
    let c = build(crowned(0.0), &straight(102.0), &plane(0.05));
    let m = corridor_meshes(&c);
    assert_eq!((m.origin_x_m, m.origin_y_m), (0.0, 0.0));
    // 5 intervals: 2 links and 2 fill slopes each, 2 triangles a strip.
    assert_eq!((m.top.triangles.len(), m.fill_slopes.triangles.len(), m.cut_slopes.triangles.len()), (20, 20, 0));
    // The left edge at station 0 is 5 m north of the origin at level 101.9, in millimetres.
    assert_eq!(m.top.vertices[0], [0.0, 5000.0, 101_900.0]);
    for mesh in [&m.top, &m.fill_slopes] {
        for t in &mesh.triangles {
            let [p, q, r] = t.map(|i| mesh.vertices[i as usize]);
            let up = (q[0] - p[0]) * (r[1] - p[1]) - (q[1] - p[1]) * (r[0] - p[0]);
            assert!(up > 0.0, "{} has a triangle facing down", mesh.name);
        }
    }
    // Top area in plan: 10 m × 100 m.
    let plan_area: f64 = m
        .top
        .triangles
        .iter()
        .map(|t| {
            let [p, q, r] = t.map(|i| m.top.vertices[i as usize]);
            ((q[0] - p[0]) * (r[1] - p[1]) - (q[1] - p[1]) * (r[0] - p[0])) / 2.0e6
        })
        .sum();
    close(plan_area, 1000.0, "top plan area");
    let cut = corridor_meshes(&build(crowned(0.0), &straight(97.0), &plane(0.05)));
    assert_eq!((cut.cut_slopes.triangles.len(), cut.fill_slopes.triangles.len()), (20, 0));

    let lines = plan_lines(&c);
    assert_eq!(lines.centreline.len(), 6);
    assert_eq!(lines.left_edge[0], (0.0, 5.0));
    assert_eq!(lines.right_edge[5], (100.0, -5.0));
    assert_eq!((lines.left_daylight.len(), lines.right_daylight.len()), (1, 1));
    close(lines.right_daylight[0][2].0, 40.0, "easting of the right daylight at station 40");
    close(lines.right_daylight[0][2].1, -8.0, "8 m to the right is 8 m south");
    close(lines.left_daylight[0][0].1, 5.0 + 43.0 / 9.0, "left is north");
}

#[test]
fn what_cannot_be_built_says_why() {
    let no_profile = AlignmentBuilder::new("Bare", 0.0, 0.0, 0.0, 0.0).line(100.0).build().unwrap();
    let d = CorridorDefinition::single("C", crowned(0.0), 20.0);
    assert_eq!(Corridor::build(&d, &no_profile, &plane(0.0)), Err(CorridorError::NoProfile("Bare".to_string())));
    assert_eq!(section_at(&crowned(0.0), 0, &straight(102.0), &plane(0.0), 100.5).unwrap_err(), CorridorError::Station(100.5));

    let refuse = |change: &dyn Fn(&mut CorridorDefinition), part: &str| {
        let mut d = CorridorDefinition::single("C", crowned(0.0), 20.0);
        change(&mut d);
        match Corridor::build(&d, &straight(102.0), &plane(0.0)) {
            Err(CorridorError::Definition(m)) => assert!(m.contains(part), "'{m}' does not mention '{part}'"),
            other => panic!("expected a definition error about '{part}', got {other:?}"),
        }
    };
    refuse(&|d| d.interval_m = 0.0, "interval");
    refuse(&|d| d.regions.clear(), "at least one template region");
    refuse(&|d| d.regions[0].template.left.links[0].width_m = 0.0, "left link 1 ('lane'): the width");
    refuse(&|d| d.regions[0].template.right.links[0].slope = Slope::Ratio { run: 0.0, rise: 1.0 }, "run of a slope ratio");
    refuse(&|d| d.regions[0].template.right.daylight.cut_run_per_rise = 0.0, "right side: the cut slope");
    refuse(&|d| d.regions[0].template.formation_depth_m = -0.1, "formation depth");

    let mut d = CorridorDefinition::single("C", crowned(0.0), 20.0);
    d.start_station_m = Some(100.0);
    assert!(matches!(Corridor::build(&d, &straight(102.0), &plane(0.0)), Err(CorridorError::NoLength { .. })));
}

#[test]
fn the_definition_round_trips_through_json() {
    let mut d = CorridorDefinition::single("C", crowned(0.3), 20.0);
    d.regions[0].template.left.links.push(Link::new("ditch_foreslope", 1.5, Slope::Ratio { run: 3.0, rise: -1.0 }).following_superelevation());
    d.regions.push(TemplateRegion { start_station_m: 55.0, template: crowned(0.0) });
    d.end_station_m = Some(90.0);
    d.volumes = VolumeOptions { cut_factor: 0.9, fill_factor: 1.1, curvature_correction: true };
    let json = serde_json::to_string(&d).unwrap();
    assert_eq!(serde_json::from_str::<CorridorDefinition>(&json).unwrap(), d);
    assert!(json.contains(r#""slope":{"percent":-2.0}"#) && json.contains(r#""slope":{"ratio":{"run":3.0,"rise":-1.0}}"#), "{json}");
    // What is left out takes the stated defaults: no factors, no correction, not paved.
    let short = r#"{"name":"C","interval_m":20.0,"regions":[{"start_station_m":0.0,"template":{"name":"t","formation_depth_m":0.0,
        "left":{"links":[{"code":"lane","width_m":3.0,"slope":{"percent":-2.5}}],"daylight":{"cut_run_per_rise":1.0,"fill_run_per_rise":2.0}},
        "right":{"links":[],"daylight":{"cut_run_per_rise":1.0,"fill_run_per_rise":2.0}}}}]}"#;
    let d: CorridorDefinition = serde_json::from_str(short).unwrap();
    assert_eq!(d.volumes, VolumeOptions::default());
    assert_eq!((d.volumes.cut_factor, d.volumes.fill_factor, d.volumes.curvature_correction), (1.0, 1.0, false));
    let lane = &d.regions[0].template.left.links[0];
    assert!(!lane.paved && !lane.follows_superelevation);
}
