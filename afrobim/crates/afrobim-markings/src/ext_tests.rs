//! Tests of the second set and of the Uganda basis.
//!
//! Every catalogue test names the figure, table or page its numbers were read from in a comment.
//! The straight test alignment runs east from the origin, so a point at station s and offset o
//! (positive right) is at plan (s, -o), and an item turned by rotation 0 points east.

use afrobim_alignment::{AlignmentBuilder, Turn};
use afrobim_kernel::{Document, ElementId, ModuleRegistry, Pt, Severity};

use crate::basis::{Basis, Lookup};
use crate::catalogue::{LineType, MarkingColour, Pattern};
use crate::ext_catalogue::{ext_catalogue_for, ext_spec, ExtSpec, Family, GeometryStatus, Kind, Reference, Sel};
use crate::ext_def::{ExtDefinition, ExtLaid, ExtResolved, Setting, Shape, CATEGORY_EXT};
use crate::ext_lookup::*;
use crate::ext_module::*;
use crate::{MarkingDefinition, MarkingError};

fn straight() -> afrobim_alignment::Alignment {
    AlignmentBuilder::new("S", 0.0, 0.0, 0.0, 0.0).line(300.0).build().unwrap()
}

fn on(mut def: ExtDefinition) -> ExtDefinition {
    def.alignment = "S".into();
    def.geometry = Some(straight());
    def
}

fn lay(def: ExtDefinition) -> (ExtResolved, ExtLaid) {
    let (laid, notes) = on(def).lay_out();
    laid.unwrap_or_else(|| panic!("{notes:?}"))
}

fn errors(def: ExtDefinition) -> Vec<String> {
    on(def).lay_out().1.into_iter().filter(|n| n.severity == Severity::Error).map(|n| n.code).collect()
}

fn near(a: f64, b: f64) -> bool {
    (a - b).abs() < 1e-9
}

fn near2(a: (f64, f64), b: (f64, f64)) -> bool {
    near(a.0, b.0) && near(a.1, b.1)
}

fn poly(l: &ExtLaid, i: usize) -> &Vec<(f64, f64)> {
    match &l.pieces[i].shape {
        Shape::Poly(p) => p,
        other => panic!("{other:?}"),
    }
}

/// Shoelace formula, written here independently of the crate's own.
fn shoelace(p: &[(f64, f64)]) -> f64 {
    let mut s = 0.0;
    for i in 0..p.len() {
        let j = (i + 1) % p.len();
        s += p[i].0 * p[j].1 - p[j].0 * p[i].1;
    }
    s.abs() / 2.0
}

fn strip_edges(l: &ExtLaid, i: usize) -> (&Vec<(f64, f64)>, &Vec<(f64, f64)>) {
    match &l.pieces[i].shape {
        Shape::Strip { left, right, .. } => (left, right),
        other => panic!("{other:?}"),
    }
}

fn spec(id: &str) -> &'static ExtSpec {
    ext_spec(id).unwrap_or_else(|| panic!("{id}"))
}

fn lines_def(id: &str) -> crate::LinesDef {
    match spec(id).kind {
        Kind::Lines(l) => l,
        _ => panic!("{id}"),
    }
}

use Basis::{Uganda, Uk};

// ============================================================================ R2: UK

#[test]
fn diagram_1017_single_yellow_line() {
    // TSRGD 2016 Schedule 7 Part 4 item 2, diagram 1017, p.116: line width 50, 75 or 100 mm,
    // 150 mm from the edge of the carriageway; yellow.
    let s = spec("1017");
    assert_eq!((s.tsrgd, s.colour), ("S7-4-2", MarkingColour::Yellow));
    let l = lines_def("1017");
    assert_eq!(l.widths_mm, &[50, 75, 100]);
    assert_eq!(l.reference, Reference::Edge { clear_mm: 150 });
    assert_eq!(l.strands.len(), 1);
    assert!(s.citation.contains("p.116"));
    // The width is a choice the figure leaves: no width, no marking.
    assert_eq!(errors(ExtDefinition::linear("1017", Uk, 0.0, 20.0, 4.0)), ["marking.dimension.required"]);
    assert_eq!(errors(ExtDefinition::linear("1017", Uk, 0.0, 20.0, 4.0).with_width(80)), ["marking.dimension.not_prescribed"]);
    // Kerb 4.0 m to the right, 75 mm line: centre 4.0 - (0.150 + 0.0375) = 3.8125 m right, so
    // the plan y is -3.8125; the edges are 3.85 m and 3.775 m right of the alignment.
    let (r, l) = lay(ExtDefinition::linear("1017", Uk, 0.0, 20.0, 4.0).with_width(75));
    assert_eq!(r.colour, MarkingColour::Yellow);
    let (left, right) = strip_edges(&l, 0);
    assert!(near2(right[0], (0.0, -3.85)) && near2(left[0], (0.0, -3.775)) && near2(right[1], (20.0, -3.85)));
    // Quantities: continuous, 20 m by 75 mm.
    assert!(near(l.painted_length_m, 20.0) && near(l.painted_area_m2.unwrap(), 1.5));
    assert_eq!((l.number, l.part_marks), (1, 0));
    // A kerb on the left puts the line to the right of it, nearer the alignment.
    let (_, l) = lay(ExtDefinition::linear("1017", Uk, 0.0, 20.0, -4.0).with_width(100));
    let (left, right) = strip_edges(&l, 0);
    assert!(near2(left[0], (0.0, 4.0 - 0.15)) && near2(right[0], (0.0, 4.0 - 0.15 - 0.1)));
    assert_eq!(errors(ExtDefinition::linear("1017", Uk, 0.0, 20.0, 0.0).with_width(75)), ["marking.offset.required"]);
}

#[test]
fn diagram_1018_1_double_yellow_lines() {
    // TSRGD 2016 Schedule 7 Part 4 item 1, diagram 1018.1, p.116: each line 50, 75 or 100 mm,
    // the gap between them 50, 75 or 100 mm, 150 mm from the edge of the carriageway.
    let l = lines_def("1018.1");
    assert_eq!((l.widths_mm, l.separations_mm, l.strands.len()), (&[50u32, 75, 100][..], &[50u32, 75, 100][..], 2));
    assert_eq!(spec("1018.1").tsrgd, "S7-4-1");
    assert_eq!(errors(ExtDefinition::linear("1018.1", Uk, 0.0, 20.0, -4.0).with_width(100)), ["marking.dimension.required"]);
    // Kerb on the left at -4.0, 100 mm lines 75 mm apart: near line centre -4.0 + 0.15 + 0.05
    // = -3.8; far line centre -4.0 + 0.15 + 0.1 + 0.075 + 0.05 = -3.625. Left is +y in plan.
    let (_, l) = lay(ExtDefinition::linear("1018.1", Uk, 0.0, 20.0, -4.0).with_width(100).with_separation(75));
    let (l0, r0) = strip_edges(&l, 0);
    let (l1, r1) = strip_edges(&l, 1);
    assert!(near2(l0[0], (0.0, 3.8 + 0.05)) && near2(r0[0], (0.0, 3.8 - 0.05)));
    assert!(near2(l1[0], (0.0, 3.625 + 0.05)) && near2(r1[0], (0.0, 3.625 - 0.05)));
    assert!(near(l.painted_length_m, 40.0) && near(l.painted_area_m2.unwrap(), 4.0) && l.number == 2);
}

#[test]
fn stop_lines_1001_and_1002_1() {
    // TSRGD 2016 Schedule 14 Part 2 item 46, diagram 1001, p.468: continuous, 200 or 300 mm.
    let s = spec("1001");
    assert_eq!((s.tsrgd, s.family), ("S14-2-46", Family::StopLine));
    match s.kind {
        Kind::Transverse(t) => assert_eq!((t.pattern, t.rows, t.widths_mm), (Pattern::Continuous, 1, &[200u32, 300][..])),
        _ => panic!(),
    }
    // TSRGD 2016 Schedule 9 Part 6 item 1, diagram 1002.1, p.147: continuous, 400 mm.
    match spec("1002.1").kind {
        Kind::Transverse(t) => assert_eq!((t.pattern, t.widths_mm), (Pattern::Continuous, &[400u32][..])),
        _ => panic!(),
    }
    // A stop line at station 50 across a 7.3 m carriageway, from 3.65 m left to 3.65 m right.
    // Frame origin (50, +3.65); x runs right (plan -y); the line is 400 mm deep along the road.
    let (_, l) = lay(ExtDefinition::across("1002.1", Uk, 50.0, -3.65, 3.65));
    let p = poly(&l, 0);
    assert!(near2(p[0], (49.8, 3.65)) && near2(p[1], (49.8, -3.65)) && near2(p[2], (50.2, -3.65)) && near2(p[3], (50.2, 3.65)));
    assert!(near(l.painted_area_m2.unwrap(), 7.3 * 0.4) && near(l.painted_length_m, 7.3) && l.number == 1);
    // Diagram 1001 needs a width choice.
    assert_eq!(errors(ExtDefinition::across("1001", Uk, 50.0, -3.65, 3.65)), ["marking.dimension.required"]);
    let (_, l) = lay(ExtDefinition::across("1001", Uk, 50.0, -3.65, 3.65).with_width(300));
    assert!(near(l.painted_area_m2.unwrap(), 7.3 * 0.3));
    // Across only a lane, skewed by 90 degrees about the first end, the line runs along the road.
    let (_, l) = lay(ExtDefinition { rotation_deg: 90.0, ..ExtDefinition::across("1002.1", Uk, 50.0, 0.0, 3.0) });
    // theta = 90 deg: world = (x0 + x, y0 + y): the corner (x=0, y=-0.2) is (50, -0.2).
    assert!(near2(poly(&l, 0)[0], (50.0, -0.2)) && near2(poly(&l, 0)[2], (53.0, 0.2)));
}

#[test]
fn give_way_lines_1003_1_and_1003a() {
    // TSRGD 2016 Schedule 11 Part 4 item 1, diagram 1003.1, p.223: marks 1000, gaps 1000,
    // width 200 or 300 mm. Chapter 5 (2018) 6.2.2 p.62: 300 where the warning line is 1004.1.
    match spec("1003.1").kind {
        Kind::Transverse(t) => assert_eq!((t.pattern, t.widths_mm), (Pattern::Broken { mark_mm: 1000, gap_mm: 1000 }, &[200u32, 300][..])),
        _ => panic!(),
    }
    // 7.3 m: marks lead at 0, 2, 4, 6 and the last ends at 7 m: four whole marks.
    let (_, l) = lay(ExtDefinition::across("1003.1", Uk, 80.0, -3.65, 3.65).with_width(200));
    assert_eq!((l.number, l.part_marks), (4, 0));
    assert!(near(l.painted_length_m, 4.0) && near(l.painted_area_m2.unwrap(), 0.8));
    assert!(near2(poly(&l, 1)[0], (79.9, 3.65 - 2.0)) && near2(poly(&l, 1)[2], (80.1, 3.65 - 3.0)));
    // 6.5 m: three whole marks and 0.5 m of a fourth.
    let (_, l) = lay(ExtDefinition::across("1003.1", Uk, 80.0, 0.0, 6.5).with_width(300));
    assert_eq!((l.number, l.part_marks), (3, 1));
    assert!(near(l.painted_length_m, 3.5) && near(l.painted_area_m2.unwrap(), 1.05));

    // TSRGD 2016 Schedule 9 Part 6 item 3, diagram 1003A, p.148: two rows of 200 mm line,
    // 300 mm apart, marks 600, gaps 300.
    match spec("1003A").kind {
        Kind::Transverse(t) => assert_eq!((t.pattern, t.rows, t.row_gap_mm, t.widths_mm), (Pattern::Broken { mark_mm: 600, gap_mm: 300 }, 2, 300, &[200u32][..])),
        _ => panic!(),
    }
    // 3.0 m across: leads 0, 0.9, 1.8, 2.7; the fourth is cut at 3.0 (0.3 m). Per row 3 whole
    // and 1 part; painted length per row 3 x 0.6 + 0.3 = 2.1 m.
    let (_, l) = lay(ExtDefinition::across("1003A", Uk, 80.0, 0.0, 3.0));
    assert_eq!((l.number, l.part_marks), (6, 2));
    assert!(near(l.painted_length_m, 4.2) && near(l.painted_area_m2.unwrap(), 0.84));
    // Rows 0.7 m deep overall: the first is y in [-0.35, -0.15] in the frame, i.e. station
    // 79.65 to 79.85; the second 80.15 to 80.35.
    assert!(near(poly(&l, 0)[0].0, 79.65) && near(poly(&l, 0)[2].0, 79.85));
    assert!(near(poly(&l, 4)[0].0, 80.15) && near(poly(&l, 4)[2].0, 80.35));
}

/// Independent computation of the ring of the give way triangle: the outer triangle less the
/// hollow, from the sines and tangents of its half angle.
fn triangle_ring(base: f64, height: f64, side: f64, base_border: f64) -> f64 {
    let half = base / 2.0;
    let slant = (half * half + height * height).sqrt();
    let sin_a = half / slant;
    let apex_shift = side / sin_a; // the inner apex lies this far from the outer along the axis
    let inner_height_to_base_line = height - apex_shift - base_border;
    let inner_half_base = inner_height_to_base_line * half / height;
    let outer = base * height / 2.0;
    let inner = inner_half_base * inner_height_to_base_line;
    outer - inner
}

#[test]
fn give_way_triangle_1023a_and_m61() {
    // TSRGD 2016 Schedule 9 Part 6 item 4, diagram 1023A, p.148: 1250 base, 3750 high, 150
    // border, 600 (read as the base border). Uganda sign page M61 (PDF p.184): the same four.
    for id in ["1023A", "M61"] {
        match spec(id).kind {
            Kind::Triangle(t) => assert_eq!((t.base_mm, t.height_mm, t.side_border_mm, t.base_border_mm), (1250, 3750, 150, 600)),
            _ => panic!(),
        }
    }
    let (_, l) = lay(ExtDefinition::point("1023A", Uk, 100.0, 0.0, 0.0));
    // By hand: half angle with tan 0.625/3.75; sin = 0.625 / 3.80165 = 0.164399; the inner apex is
    // 0.150 / 0.164399 = 0.9124 m up from the outer apex; the hollow's base line is 0.600 m
    // above the base, its half width 2.2376 x 0.166667 = 0.37293; ring area
    // 2.34375 - 0.8344 = 1.5093 m2.
    let expected = triangle_ring(1.25, 3.75, 0.15, 0.6);
    assert!((expected - 1.5093).abs() < 1e-4, "{expected}");
    assert!(near(l.painted_area_m2.unwrap(), expected));
    // The three pieces tile the ring: their shoelace areas add to the same.
    let sum: f64 = (0..3).map(|i| shoelace(poly(&l, i))).sum();
    assert!(near(sum, expected));
    // Base at +y: base corners at station 100 + 1.875 = 101.875, lateral +-0.625.
    assert!(near2(poly(&l, 0)[0], (101.875, 0.625)) && near2(poly(&l, 0)[1], (101.875, -0.625)));
    // Apex at 98.125 on the axis.
    assert!(near2(poly(&l, 1)[1], (98.125, 0.0)));
    let (_, m) = lay(ExtDefinition::point("M61", Uganda, 100.0, 0.0, 0.0));
    assert!(near(m.painted_area_m2.unwrap(), expected));
    assert!(near(l.bounding_area_m2.unwrap(), 1.25 * 3.75));
}

#[test]
fn worded_markings_are_placeholder_blocks_1022_and_1024() {
    // Chapter 5 (2018) Table 11-2, p.112 (elongated tile widths): S 548, T 436, O 624, P 520,
    // L 428, W 732 mm. 11.2.1 p.106: SLOW 1600 mm at 40 mph or less, 2800 mm over 40 mph.
    assert_eq!(548 + 436 + 624 + 520, 2128);
    assert_eq!(548 + 428 + 624 + 732, 2332);
    match spec("1024").kind {
        Kind::Word(w) => assert_eq!((w.text, w.width_mm, w.heights), ("SLOW", 2332, &[(Sel::AtMost(40), 1600u32), (Sel::Over(40), 2800)][..])),
        _ => panic!(),
    }
    match spec("1022").kind {
        Kind::Word(w) => assert_eq!((w.text, w.width_mm), ("STOP", 2128)),
        _ => panic!(),
    }
    assert!(matches!(spec("1024").geometry, GeometryStatus::Placeholder(_)));
    // At 30 mph the SLOW block is 1600 long by 2332 wide: 3.7312 m2, and there is no painted area.
    let (_, l) = lay(ExtDefinition::point("1024", Uk, 100.0, 0.0, 0.0).with_speed(30, "30 mph limit"));
    assert!(l.placeholder && l.painted_area_m2.is_none() && near(l.bounding_area_m2.unwrap(), 1.6 * 2.332));
    // Block corners: half length 0.8 along (x), half width 1.166 across (y).
    assert!(near2(poly(&l, 0)[0], (99.2, 1.166)) && near2(poly(&l, 0)[2], (100.8, -1.166)));
    let (_, l) = lay(ExtDefinition::point("1024", Uk, 100.0, 0.0, 0.0).with_speed(50, "caller"));
    assert!(near(l.bounding_area_m2.unwrap(), 2.8 * 2.332));
    // STOP: both sizes match any speed, so the size must be chosen.
    assert_eq!(errors(ExtDefinition::point("1022", Uk, 100.0, 0.0, 0.0).with_speed(30, "")), ["marking.dimension.required"]);
    let (_, l) = lay(ExtDefinition::point("1022", Uk, 100.0, 0.0, 0.0).with_size(2800));
    assert!(near(l.bounding_area_m2.unwrap(), 2.8 * 2.128));
    assert_eq!(errors(ExtDefinition::point("1022", Uk, 100.0, 0.0, 0.0).with_size(2000)), ["marking.dimension.not_prescribed"]);
}

#[test]
fn lane_arrows_1038_follow_table_9_1() {
    // Chapter 5 (2018) Figure 9-1 and Table 9-1, p.97: length 4000, 6000 or 9000 mm; widths 500
    // (shaft 300), 750, 850, 1200 by panel. Table 9-1: 4 m at 40 mph or less, 6 m at 50 or 60, 9 m at 70.
    for (id, width) in [("1038-ahead", 500), ("1038-turn", 750), ("1038-ahead-and-turn", 850), ("1038-two-way", 1200)] {
        match spec(id).kind {
            Kind::Arrow(a) => {
                assert_eq!(a.sizes.iter().map(|s| (s.length_mm, s.width_mm)).collect::<Vec<_>>(), [(4000, Some(width)), (6000, Some(width)), (9000, Some(width))]);
            }
            _ => panic!(),
        }
        assert_eq!(spec(id).diagram, "1038");
    }
    for (mph, length) in [(30, 4000), (40, 4000), (50, 6000), (60, 6000), (70, 9000)] {
        let (_, l) = lay(ExtDefinition::point("1038-ahead", Uk, 100.0, 1.75, 0.0).with_speed(mph, "caller"));
        let (hl, hw) = (length as f64 / 2000.0, 0.25);
        // Centre (100, -1.75); points east: corners at x +-hl, y -1.75 +-hw.
        assert!(near2(poly(&l, 0)[0], (100.0 - hl, -1.75 + hw)) && near2(poly(&l, 0)[2], (100.0 + hl, -1.75 - hw)), "{mph}");
        assert!(near(l.bounding_area_m2.unwrap(), length as f64 / 1000.0 * 0.5));
        assert!(l.painted_area_m2.is_none() && l.number == 1);
    }
    // 45 mph has no row in Table 9-1.
    assert_eq!(errors(ExtDefinition::point("1038-ahead", Uk, 100.0, 0.0, 0.0).with_speed(45, "")), ["marking.speed.no_row"]);
    assert_eq!(errors(ExtDefinition::point("1038-ahead", Uk, 100.0, 0.0, 0.0)), ["marking.speed.missing"]);
    // Rotated 90 degrees the arrow points north (plan +y): half length now along y.
    let (_, l) = lay(ExtDefinition::point("1038-ahead", Uk, 100.0, 0.0, 90.0).with_size(4000));
    assert!(near2(poly(&l, 0)[0], (99.75, -2.0)));
    // Siting, Table 9-1.
    let s = lane_arrow_siting(40).unwrap();
    assert_eq!((s.arrow_length_m, s.first_from_line_m, s.second_from_first_m, s.third_from_second_m), (4, (15.0, 25.0), (30.0, 50.0), (30.0, 50.0)));
    assert_eq!(lane_arrow_siting(60).unwrap().arrow_length_m, 6);
    // 70 mph: "up to 1.5 times the above distances": 37.5, 75, 75 as the upper bounds.
    let s = lane_arrow_siting(70).unwrap();
    assert_eq!((s.arrow_length_m, s.first_from_line_m, s.second_from_first_m), (9, (15.0, 37.5), (30.0, 75.0)));
    assert!(lane_arrow_siting(45).is_none());
}

#[test]
fn deflection_arrow_1014_and_table_2_6() {
    // Chapter 5 (2018) Figure 9-5, p.100: length 4500, 6000, 9000; shaft 150, 200, 300; head
    // extent 525, 700, 1050; head length 1500, 2000, 3000.
    match spec("1014").kind {
        Kind::Arrow(a) => {
            assert_eq!(a.sizes.iter().map(|s| (s.length_mm, s.width_mm)).collect::<Vec<_>>(), [(4500, Some(525)), (6000, Some(700)), (9000, Some(1050))]);
            assert_eq!(a.sizes[0].aux, &[("shaft width", 150), ("head length", 1500)]);
            assert_eq!(a.sizes[1].aux, &[("shaft width", 200), ("head length", 2000)]);
            assert_eq!(a.sizes[2].aux, &[("shaft width", 300), ("head length", 3000)]);
        }
        _ => panic!(),
    }
    for (v, length, width) in [(30, 4.5, 0.525), (55, 6.0, 0.7), (65, 9.0, 1.05)] {
        let (_, l) = lay(ExtDefinition::point("1014", Uk, 100.0, 0.0, 0.0).with_speed(v, "85th percentile"));
        assert!(near(l.bounding_area_m2.unwrap(), length * width), "{v}");
    }
    assert_eq!(errors(ExtDefinition::point("1014", Uk, 100.0, 0.0, 0.0).with_speed(75, "")), ["marking.speed.no_row"]);
    // Table 2-6, p.24.
    let rows: Vec<_> = [25, 35, 45, 55, 65].iter().map(|v| deflection_row(*v).unwrap()).collect();
    assert_eq!(rows.iter().map(|r| r.taper_one_in).collect::<Vec<_>>(), [40, 40, 45, 50, 55]);
    assert_eq!(rows.iter().map(|r| r.arrow_length_m).collect::<Vec<_>>(), [4.5, 4.5, 6.0, 6.0, 9.0]);
    assert_eq!(rows[0].tip_to_taper_m, [13.75, 43.75, 79.75]);
    assert_eq!(rows[1].tip_to_taper_m, [19.75, 55.75, 109.75]);
    assert_eq!(rows[2].tip_to_taper_m, [21.0, 66.0, 138.0]);
    assert_eq!(rows[3].tip_to_taper_m, [30.0, 84.0, 165.0]);
    assert_eq!(rows[4].tip_to_taper_m, [37.5, 91.5, 190.5]);
    assert!(deflection_row(71).is_none());
    // The arrow length of the table agrees with the length chosen for the same speed band.
    assert_eq!(rows[3].arrow_length_m * 1000.0, 6000.0);
}

#[test]
fn hatched_marking_1040() {
    // Chapter 5 (2018) Figure 2-8, p.24, and 2.7.1, p.23: boundary 100 or 150 mm; marks 4000 and
    // gaps 2000 at 40 mph or less, 6000 and 3000 over 40; diagonals 150 mm wide at 3000 spacing
    // (short module), 200 mm wide at 2000 spacing (long); 800 mm minimum, 8000 mm maximum between
    // the boundary lines; 45 degrees.
    let h = match spec("1040").kind {
        Kind::Hatch(h) => h,
        _ => panic!(),
    };
    let (s, lo) = (h.short.unwrap(), h.long);
    assert_eq!((s.boundary_mark_mm, s.boundary_gap_mm, s.spacing_mm, s.mark_width_mm), (4000, 2000, 3000, 150));
    assert_eq!((lo.boundary_mark_mm, lo.boundary_gap_mm, lo.spacing_mm, lo.mark_width_mm), (6000, 3000, 2000, 200));
    assert_eq!((h.boundary_widths_mm, h.strip_min_mm, h.strip_max_mm, h.split), (&[100u32, 150][..], Some(800), Some(8000), Some(40)));
    assert_eq!(spec("1040").tsrgd, "S11-4-23");
    let base = || ExtDefinition::linear("1040", Uk, 0.0, 24.0, 0.0).with_strip_width(3000).with_width(100);
    // Short module over 24 m: boundary marks lead at 0, 6, 12, 18 (the last ends at 22): four
    // whole per boundary, 32 m painted, 3.2 m2. Diagonals: v = 0.15 / sqrt 2 = 0.106066, a
    // diagonal needs 3 + 2v = 3.212132 m; k x 3 + 3.212132 <= 24 gives k = 0 to 6, seven
    // diagonals of 3 x 0.15 x sqrt 2 = 0.636396 m2.
    let (_, l) = lay(base().with_speed(30, "30 mph"));
    let v = 0.15 / 2f64.sqrt();
    assert!(near(3.0 + 2.0 * v, 3.212132034355964));
    assert_eq!(l.number, 8 + 7);
    assert!(near(l.painted_area_m2.unwrap(), 3.2 + 7.0 * 3.0 * 0.15 * 2f64.sqrt()), "{:?}", l.painted_area_m2);
    assert!(near(l.painted_length_m, 32.0 + 7.0 * 3.0 * 2f64.sqrt()));
    // Boundary lines: left boundary lateral -(1.5 + 0.05) = -1.55, i.e. plan y +1.55; first
    // mark runs 0 to 4 m.
    let (left, right) = strip_edges(&l, 0);
    assert!(near2(left[0], (0.0, 1.6)) && near2(right[0], (0.0, 1.5)) && near2(right[1], (4.0, 1.5)));
    // First diagonal (the first piece in the marks group): corners (x = +1.5, y = 0),
    // (+1.5, 2v), (-1.5, 3 + 2v), (-1.5, 3): lateral x is plan -y.
    let first_diag = l.pieces.iter().position(|p| p.group == 2).unwrap();
    let d = poly(&l, first_diag);
    assert!(near2(d[0], (0.0, -1.5)) && near2(d[1], (2.0 * v, -1.5)) && near2(d[2], (3.0 + 2.0 * v, 1.5)) && near2(d[3], (3.0, 1.5)));
    assert!(near(shoelace(d), 3.0 * 0.15 * 2f64.sqrt()));
    // The second diagonal starts 3 m after the first.
    assert!(near(poly(&l, first_diag + 1)[0].0, 3.0));
    // Long module at 50 mph: boundary marks 6 and gaps 3: leads 0, 9, 18; the last ends at 24:
    // three whole marks per boundary (36 m painted); diagonals 200 wide at 2 m: a diagonal
    // needs 3 + 2 x 0.141421 = 3.282843; k x 2 + 3.282843 <= 24 gives k = 0 to 10: eleven.
    let (_, l) = lay(base().with_speed(50, "caller"));
    assert_eq!(l.number, 6 + 11);
    let v2 = 0.2 / 2f64.sqrt();
    assert!(near(l.painted_area_m2.unwrap(), 36.0 * 0.1 + 11.0 * 3.0 * 0.2 * 2f64.sqrt()));
    assert!(near(3.0 + 2.0 * v2, 3.282842712474619));
    // The inputs the figure leaves: strip width, boundary width and speed.
    assert_eq!(errors(ExtDefinition::linear("1040", Uk, 0.0, 24.0, 0.0).with_width(100).with_speed(30, "")), ["marking.strip.required"]);
    assert_eq!(errors(ExtDefinition::linear("1040", Uk, 0.0, 24.0, 0.0).with_strip_width(700).with_width(100).with_speed(30, "")), ["marking.strip.range"]);
    assert_eq!(errors(ExtDefinition::linear("1040", Uk, 0.0, 24.0, 0.0).with_strip_width(8001).with_width(100).with_speed(30, "")), ["marking.strip.range"]);
    assert_eq!(errors(ExtDefinition::linear("1040", Uk, 0.0, 24.0, 0.0).with_strip_width(3000).with_speed(30, "")), ["marking.dimension.required"]);
    assert_eq!(errors(base()), ["marking.speed.missing"]);
    // Mirrored: the lean flips (first corner at x = -1.5).
    let (_, l) = lay(ExtDefinition { mirror: true, ..base().with_speed(30, "") });
    let first_diag = l.pieces.iter().position(|p| p.group == 2).unwrap();
    assert!(near2(poly(&l, first_diag)[0], (0.0, 1.5)));
    // The hatch lays on a curve: marks follow the alignment.
    let a = AlignmentBuilder::new("C", 0.0, 0.0, 0.0, 0.0).arc(200.0, 100.0, Turn::Left).build().unwrap();
    let mut def = ExtDefinition::linear("1040", Uk, 0.0, 24.0, 0.0).with_strip_width(3000).with_width(100).with_speed(30, "");
    def.geometry = Some(a);
    let l = def.lay_out().0.unwrap().1;
    // First diagonal corner (x = +1.5, y = 0): 1.5 m right of the start of a left-hand curve.
    let first_diag = l.pieces.iter().position(|p| p.group == 2).unwrap();
    assert!(near2(poly(&l, first_diag)[0], (0.0, -1.5)));
}

#[test]
fn chevron_markings_1041_and_1042() {
    // Chapter 5 (2018) Figure 2-16, p.30: the 1040 widths and spacings with chevrons of 90
    // degrees included angle.
    let h = match spec("1041").kind {
        Kind::Hatch(h) => h,
        _ => panic!(),
    };
    assert!(h.chevrons);
    assert_eq!((h.short.unwrap().spacing_mm, h.long.spacing_mm, h.short.unwrap().mark_width_mm, h.long.mark_width_mm), (3000, 2000, 150, 200));
    assert_eq!(spec("1041").tsrgd, "S11-4-27");
    let base = || ExtDefinition::linear("1041", Uk, 0.0, 24.0, 0.0).with_strip_width(3000).with_width(100).with_speed(30, "");
    // Each chevron needs 1.5 + 2v = 1.712132 m: k x 3 + 1.712132 <= 24 gives k = 0 to 7, eight
    // chevrons of 2 x (2v x 1.5) = 0.636396 m2.
    let (_, l) = lay(base());
    assert_eq!(l.number, 8 + 8);
    let v = 0.15 / 2f64.sqrt();
    assert!(near(l.painted_area_m2.unwrap(), 3.2 + 8.0 * 3.0 * 0.15 * 2f64.sqrt()));
    // First chevron, right arm (apex towards approaching traffic = down-station): apex at
    // (0, 0); outer end at lateral 1.5, 1.5 m forward plus 2v.
    let first = l.pieces.iter().position(|p| p.group == 2).unwrap();
    let arm = poly(&l, first);
    assert!(near2(arm[0], (0.0, 0.0)) && near2(arm[1], (2.0 * v, 0.0)) && near2(arm[2], (1.5 + 2.0 * v, -1.5)) && near2(arm[3], (1.5, -1.5)));
    assert!(near(shoelace(arm) + shoelace(poly(&l, first + 1)), 3.0 * 0.15 * 2f64.sqrt()));
    // Apex forward (mirror): the apex end is at the far end of the chevron's own length.
    let (_, m) = lay(ExtDefinition { mirror: true, ..base() });
    let first = m.pieces.iter().position(|p| p.group == 2).unwrap();
    assert!(near2(poly(&m, first)[0], (1.5 + 2.0 * v, 0.0)) && near2(poly(&m, first)[3], (2.0 * v, -1.5)));

    // Chapter 5 (2018) Figure 2-17, p.31, and 2.11.2: continuous boundary lines 150 or 200 mm,
    // chevrons 150 or 200 mm wide at a pitch of 3400, 500 mm minimum and 8000 mm maximum
    // between the boundary lines.
    let h = match spec("1042").kind {
        Kind::Hatch(h) => h,
        _ => panic!(),
    };
    assert_eq!((h.boundary_widths_mm, h.long.spacing_mm, h.strip_min_mm, h.strip_max_mm, h.boundary_continuous), (&[150u32, 200][..], 3400, Some(500), Some(8000), true));
    assert_eq!(spec("1042").tsrgd, "S9-6-22");
    // 20 m, 3 m between 200 mm boundary lines: each chevron needs 1.712132; k x 3.4 + 1.712132
    // <= 20 gives k = 0 to 5, six chevrons; the boundaries are two continuous 20 m lines.
    let (_, l) = lay(ExtDefinition::linear("1042", Uk, 0.0, 20.0, 0.0).with_strip_width(3000).with_width(200));
    assert_eq!(l.number, 2 + 6);
    assert!(near(l.painted_area_m2.unwrap(), 2.0 * 20.0 * 0.2 + 6.0 * 3.0 * 0.15 * 2f64.sqrt()));
    assert_eq!(errors(ExtDefinition::linear("1042", Uk, 0.0, 20.0, 0.0).with_strip_width(400).with_width(200)), ["marking.strip.range"]);
}

#[test]
fn road_studs_follow_the_stud_tables() {
    // Chapter 5 (2018) Table 2-1, p.12: 1008 100 mm at 40 or less 12 m, 150 mm 6 m; 1008.1 100 mm
    // 18 m, 150 mm 9 m.
    let sp = |line: &str, w: Option<u32>, v: Option<u32>| stud_rows(Uk, line, w, v).iter().filter_map(|r| r.urban_m).collect::<Vec<_>>();
    assert_eq!(sp("1008", Some(100), Some(30)), [12.0]);
    assert_eq!(sp("1008", Some(150), Some(30)), [6.0]);
    assert_eq!(sp("1008.1", Some(100), Some(60)), [18.0]);
    assert_eq!(sp("1008.1", Some(150), Some(60)), [9.0]);
    // 1008 over 40 mph is the Note 1 row: 100 mm 12 m.
    assert_eq!(sp("1008", Some(100), Some(60)), [12.0]);
    // Table 2-2, p.15: lane lines 12 m and 18 m at any width.
    assert_eq!(sp("1005", None, Some(30)), [12.0]);
    assert_eq!(sp("1005.1", None, Some(60)), [18.0]);
    // Table 2-3, p.17: warning lines 6 m and 9 m.
    assert_eq!(sp("1004", Some(100), Some(30)), [6.0]);
    assert_eq!(sp("1004", Some(150), Some(30)), [6.0]);
    assert_eq!(sp("1004.1", Some(100), Some(60)), [9.0]);
    assert_eq!(sp("1004.1", Some(150), Some(60)), [9.0]);
    // Table 2-5, p.20: 1010 8 m; 1012.1 hard shoulder or motorway 18 m; no hard shoulder not
    // normally used; 1009A and the emergency refuge area not used; lane-drop see 7.4.
    let rows = stud_rows(Uk, "1010", None, None);
    assert_eq!(rows.iter().filter_map(|r| r.urban_m).collect::<Vec<_>>(), [8.0]);
    assert!(rows.iter().any(|r| r.text == "See 7.4" && r.urban_m.is_none()));
    assert!(rows.iter().any(|r| r.text == "Not used"));
    assert_eq!(sp("1012.1", Some(150), None), [18.0]);
    assert_eq!(sp("1012.1", Some(200), None), [18.0]);
    assert!(stud_rows(Uk, "1012.1", None, None).iter().any(|r| r.text == "Not normally used"));
    assert_eq!(stud_rows(Uk, "1009A", None, None)[0].text, "Not used");
    // 3.3.1, p.36: double white lines 3 m to 4.5 m, 4.5 m recommended.
    let dw = &stud_rows(Uk, "1013.1D-L", None, None)[0];
    assert_eq!((dw.range_m, dw.urban_m, dw.colour), (Some((3.0, 4.5)), Some(4.5), StudColour::White));
    // 4.4.1 and 4.5.2, p.48 to 49: nearside edge studs red; chevrons red at 3 m.
    assert_eq!(stud_rows(Uk, "1012.1", Some(150), None)[0].colour, StudColour::Red);
    assert_eq!((stud_rows(Uk, "1041", None, None)[0].colour, stud_rows(Uk, "1041", None, None)[0].urban_m), (StudColour::Red, Some(3.0)));
    assert_eq!(stud_rows(Uk, "1010", None, None)[0].colour, StudColour::Green);
    // 4.5.3, p.49: 18 m reduced to 9 m on a bend under 450 m radius.
    assert_eq!((uk_stud_spacing_on_bend(18.0, 449.0), uk_stud_spacing_on_bend(18.0, 450.0), uk_stud_spacing_on_bend(12.0, 100.0)), (9.0, 18.0, 12.0));

    // Count over a known length: 0 to 90 m at 18 m is studs at 0, 18, 36, 54, 72 and 90.
    let (r, l) = lay(ExtDefinition::studs(Uk, 0.0, 90.0, 0.0, 18000, StudColour::White));
    assert_eq!(l.number, 6);
    assert!(near2(poly(&l, 5)[0], (89.95, 0.05)));
    assert_eq!(r.stud_colour, Some(StudColour::White));
    assert!(l.painted_area_m2.is_none());
    // 0 to 100 m: the last stud at 90 (108 is past the end): still six.
    assert_eq!(lay(ExtDefinition::studs(Uk, 0.0, 100.0, 0.0, 18000, StudColour::White)).1.number, 6);
    // Double white line studs: 0 to 90 m at 4.5 m is 21 studs.
    assert_eq!(lay(ExtDefinition::studs(Uk, 0.0, 90.0, 0.0, 4500, StudColour::White)).1.number, 21);
    // Spacing is measured along the offset line: on the outside of a 200 m left-hand bend,
    // 3.5 m out, 90 m of station is 91.575 m of line: still six studs at 18 m, and the fifth
    // is 72 m along the line, at station 72 / 1.0175.
    let a = AlignmentBuilder::new("C", 0.0, 0.0, 0.0, 0.0).arc(200.0, 150.0, Turn::Left).build().unwrap();
    let (stations, length) = crate::layout::stations_along(&a, 0.0, 90.0, 3.5, 18.0).unwrap();
    assert!(near(length, 91.575) && stations.len() == 6 && near(stations[4], 72.0 / 1.0175));
    // Studs need a spacing and a colour.
    assert_eq!(errors(ExtDefinition { spacing_mm: None, ..ExtDefinition::studs(Uk, 0.0, 90.0, 0.0, 18000, StudColour::White) }), ["marking.stud.spacing"]);
    assert_eq!(errors(ExtDefinition { stud_colour: None, ..ExtDefinition::studs(Uk, 0.0, 90.0, 0.0, 18000, StudColour::White) }), ["marking.stud.colour"]);
}

// =========================================================================== R3: Uganda

#[test]
fn uganda_dividing_lines_m01_to_m05() {
    // Uganda TSM Vol.2 App.3 sign page M01 (PDF p.161): continuous, 100 mm, yellow.
    let m01 = lines_def("M01");
    assert_eq!((m01.widths_mm, m01.strands.len()), (&[100u32][..], 1));
    assert_eq!(spec("M01").colour, MarkingColour::Yellow);
    let (r, l) = lay(ExtDefinition::linear("M01", Uganda, 0.0, 150.0, 0.0));
    assert!(near(l.painted_area_m2.unwrap(), 15.0) && r.colour == MarkingColour::Yellow);
    // M02 (PDF p.162): two 100 mm yellow lines 130 mm apart: centres 0.115 m either side.
    let m02 = lines_def("M02");
    assert_eq!((m02.widths_mm, m02.separations_mm), (&[100u32][..], &[130u32][..]));
    let (_, l) = lay(ExtDefinition::linear("M02", Uganda, 0.0, 150.0, 0.0));
    let (left, right) = strip_edges(&l, 0);
    // The left line is centred 0.115 m left (plan y +0.115): its edges are at y 0.165 and 0.065.
    assert!(near2(left[0], (0.0, 0.165)) && near2(right[0], (0.0, 0.065)));
    assert!(near(l.painted_area_m2.unwrap(), 30.0) && l.number == 2);
    // M03 (PDF p.163): urban 3.0 m mark 6.0 m gap, rural 4.0 and 8.0.
    let m03 = lines_def("M03");
    assert_eq!(m03.strands[0].urban, Pattern::Broken { mark_mm: 3000, gap_mm: 6000 });
    assert_eq!(m03.strands[0].rural, Pattern::Broken { mark_mm: 4000, gap_mm: 8000 });
    assert_eq!(errors(ExtDefinition::linear("M03", Uganda, 0.0, 90.0, 0.0)), ["marking.setting.required"]);
    // 90 m urban: leads 0, 9, ..., 81: ten marks of 3 m: 30 m, 3.0 m2. Rural: module 12 m, leads
    // 0 to 84 (eight), the last 84 to 88: 32 m, 3.2 m2.
    let (_, l) = lay(ExtDefinition::linear("M03", Uganda, 0.0, 90.0, 0.0).with_setting(Setting::Urban));
    assert_eq!((l.number, l.part_marks) , (10, 0));
    assert!(near(l.painted_length_m, 30.0) && near(l.painted_area_m2.unwrap(), 3.0));
    let (_, l) = lay(ExtDefinition::linear("M03", Uganda, 0.0, 90.0, 0.0).with_setting(Setting::Rural));
    assert_eq!(l.number, 8);
    assert!(near(l.painted_length_m, 32.0) && near(l.painted_area_m2.unwrap(), 3.2));
    // M04 (PDF p.164): one M01 and one M03, 130 mm apart; the side of the broken line is the caller's.
    let (_, l) = lay(ExtDefinition::linear("M04-L", Uganda, 0.0, 90.0, 0.0).with_setting(Setting::Urban));
    assert_eq!(l.number, 10 + 1);
    assert!(near(l.painted_length_m, 30.0 + 90.0));
    let (left, _) = strip_edges(&l, 0);
    // The broken line is the left one: centre 0.115 left = plan y +0.115, edge +0.165.
    assert!(near2(left[0], (0.0, 0.165)));
    let (_, l) = lay(ExtDefinition::linear("M04-R", Uganda, 0.0, 90.0, 0.0).with_setting(Setting::Urban));
    let (left, _) = strip_edges(&l, 0);
    // Right line broken: the left line is one continuous strip of 90 m, then ten marks.
    assert!(near2(left[0], (0.0, 0.165)) && near2(*left.last().unwrap(), (90.0, 0.165)) && l.pieces.len() == 1 + 10);
    // M05 (PDF p.165): 100 mm yellow, marks 2000, gaps 150, lateral extent 500 (300).
    match spec("M05").kind {
        Kind::ZigZag(z) => assert_eq!((z.width_mm, z.mark_mm, z.gap_mm, z.amplitudes_mm), (100, 2000, 150, &[500u32, 300][..])),
        _ => panic!(),
    }
    // 21.5 m: marks start every 2.15 m; the tenth starts at 19.35 and ends at 21.35 <= 21.5: ten.
    // Centre line excursion 0.5 - 0.1 = 0.4 m; slope length sqrt(2^2 + 0.4^2) = 2.039608; area
    // 0.1 x 2.039608 each.
    let (_, l) = lay(ExtDefinition::linear("M05", Uganda, 0.0, 21.5, 0.0));
    assert_eq!(l.number, 10);
    assert!(near(l.painted_area_m2.unwrap(), 10.0 * 0.1 * (4.0f64 + 0.16).sqrt()));
    assert!(near(l.painted_length_m, 10.0 * (4.0f64 + 0.16).sqrt()));
    // First mark: from lateral -0.2 to +0.2 over 2 m; thickness across 0.1 x 2.039608 / 2.
    let t = 0.1 * (4.0f64 + 0.16).sqrt() / 2.0;
    assert!(near2(poly(&l, 0)[0], (0.0, 0.2 + t / 2.0)));
    assert!(near(shoelace(poly(&l, 0)), 0.1 * (4.0f64 + 0.16).sqrt()));
    // The second goes back the other way.
    assert!(near2(poly(&l, 1)[0], (2.15, -0.2 + t / 2.0)));
    // The alternative extent 300.
    let (_, l) = lay(ExtDefinition::linear("M05", Uganda, 0.0, 21.5, 0.0).with_size(300));
    assert!(near(l.painted_length_m, 10.0 * (4.0f64 + 0.04).sqrt()));
    assert_eq!(errors(ExtDefinition::linear("M05", Uganda, 0.0, 21.5, 0.0).with_size(400)), ["marking.dimension.not_prescribed"]);
}

#[test]
fn uganda_lane_edge_and_no_parking_lines() {
    // M11 (PDF p.166) continuous 100 white; M12 (PDF p.167) broken 100 white, 3/6 urban, 4/8 rural.
    assert_eq!(lines_def("M11").widths_mm, &[100]);
    assert_eq!(spec("M12").colour, MarkingColour::White);
    let (_, l) = lay(ExtDefinition::linear("M12", Uganda, 0.0, 90.0, 1.8).with_setting(Setting::Urban));
    assert_eq!(l.number, 10);
    // M21 (PDF p.168): 100 mm white, 100 mm from the carriageway edge: edge 3.65 m right, centre
    // 3.65 - (0.1 + 0.05) = 3.5 m: plan y -3.5.
    let (_, l) = lay(ExtDefinition::linear("M21", Uganda, 0.0, 50.0, 3.65));
    let (left, right) = strip_edges(&l, 0);
    assert!(near2(left[0], (0.0, -3.45)) && near2(right[0], (0.0, -3.55)));
    // M22 (PDF p.169): 100 (200) white, 1.0 m marks 1.0 m gaps: 10 m is five marks.
    assert_eq!(lines_def("M22").widths_mm, &[100, 200]);
    assert_eq!(lines_def("M22").strands[0].urban, Pattern::Broken { mark_mm: 1000, gap_mm: 1000 });
    let (_, l) = lay(ExtDefinition::linear("M22", Uganda, 0.0, 10.0, 3.5).with_width(200));
    assert_eq!(l.number, 5);
    assert!(near(l.painted_area_m2.unwrap(), 1.0));
    assert_eq!(errors(ExtDefinition::linear("M22", Uganda, 0.0, 10.0, 3.5)), ["marking.dimension.required"]);
    // M23 (PDF p.170): 100 mm yellow about 250 mm out from the kerb: kerb 4.0 m left, centre at
    // -4.0 + 0.25 + 0.05 = -3.7 (plan y +3.7).
    let (r, l) = lay(ExtDefinition::linear("M23", Uganda, 0.0, 30.0, -4.0));
    let (left, _) = strip_edges(&l, 0);
    assert!(near2(left[0], (0.0, 3.75)) && r.colour == MarkingColour::Yellow);
    assert!(near(l.painted_area_m2.unwrap(), 3.0));
}

#[test]
fn uganda_transverse_markings_m31_to_m34() {
    // M31 (PDF p.171): continuous 400 mm white. M32 (PDF p.172): broken 400 mm; 600 and 300.
    match spec("M31").kind {
        Kind::Transverse(t) => assert_eq!((t.pattern, t.widths_mm), (Pattern::Continuous, &[400u32][..])),
        _ => panic!(),
    }
    match spec("M32").kind {
        Kind::Transverse(t) => assert_eq!((t.pattern, t.widths_mm, t.rows), (Pattern::Broken { mark_mm: 600, gap_mm: 300 }, &[400u32][..], 1)),
        _ => panic!(),
    }
    let (_, l) = lay(ExtDefinition::across("M31", Uganda, 60.0, -3.65, 0.0));
    assert!(near(l.painted_area_m2.unwrap(), 3.65 * 0.4) && near2(poly(&l, 0)[0], (59.8, 3.65)));
    // 3.65 m: leads 0, 0.9, 1.8, 2.7, 3.6; the last is cut at 3.65: four whole, one 0.05 m part.
    let (_, l) = lay(ExtDefinition::across("M32", Uganda, 60.0, -3.65, 0.0));
    assert_eq!((l.number, l.part_marks), (4, 1));
    assert!(near(l.painted_length_m, 2.4 + 0.05) && near(l.painted_area_m2.unwrap(), 2.45 * 0.4));
    // M33 (PDF p.173): stripes 500 to 700, gaps 500 to 700, width 3.0 to 5.0 m.
    let zebra = || ExtDefinition { count: Some(5), gap_mm: Some(600), ..ExtDefinition::across("M33", Uganda, 60.0, -2.0, 2.0).with_width(600) };
    let (_, l) = lay(zebra());
    assert_eq!(l.number, 5);
    assert!(near(l.painted_area_m2.unwrap(), 5.0 * 0.6 * 4.0));
    // Second stripe 1.2 to 1.8 m along the road from the crossing's reference station.
    assert!(near(poly(&l, 1)[0].0, 61.2) && near(poly(&l, 1)[2].0, 61.8));
    assert_eq!(errors(ExtDefinition { count: Some(5), gap_mm: Some(600), ..ExtDefinition::across("M33", Uganda, 60.0, 0.0, 2.9).with_width(600) }), ["marking.crossing.width"]);
    assert_eq!(errors(ExtDefinition { width_mm: Some(800), ..zebra() }), ["marking.dimension.not_prescribed"]);
    assert_eq!(errors(ExtDefinition { gap_mm: Some(1000), ..zebra() }), ["marking.dimension.not_prescribed"]);
    assert_eq!(errors(ExtDefinition { count: None, ..zebra() }), ["marking.dimension.required"]);
    // M34 (PDF p.174): two 100 mm broken lines, marks 500 and gaps 500, 2.5 to 5.0 m apart.
    let signal = |w: f64| ExtDefinition { length_mm: Some(4000), ..ExtDefinition::across("M34", Uganda, 60.0, 0.0, w) };
    let (_, l) = lay(signal(3.0));
    assert_eq!((l.number, l.part_marks), (8, 0));
    assert!(near(l.painted_area_m2.unwrap(), 8.0 * 0.5 * 0.1));
    // First mark of the first line: x 0 to 0.1, y 0 to 0.5: plan (60 + y, -x). The first mark of
    // the second line starts 3.0 + 0.1 m across.
    assert!(near2(poly(&l, 0)[0], (60.0, 0.0)) && near2(poly(&l, 0)[2], (60.5, -0.1)));
    assert!(near2(poly(&l, 4)[0], (60.0, -3.1)) && near2(poly(&l, 4)[2], (60.5, -3.2)));
    assert_eq!(errors(signal(2.4)), ["marking.crossing.width"]);
    // M35 is catalogued only.
    assert_eq!(errors(ExtDefinition::across("M35", Uganda, 60.0, 0.0, 3.0)), ["marking.geometry.catalogue_only"]);
}

#[test]
fn uganda_arrow_m41_from_its_outline() {
    // M41 (PDF p.176): length 4000 (6000), head length 1575, head width 500, shaft 300 at the
    // tail tapering to 120 at the head, shaft length 2425 (= 4000 - 1575).
    let a = match spec("M41").kind {
        Kind::Arrow(a) => a,
        _ => panic!(),
    };
    let t = a.tapered.unwrap();
    assert_eq!((t.head_length_mm, t.head_width_mm, t.tail_width_mm, t.neck_width_mm, t.base_length_mm), (1575, 500, 300, 120, 4000));
    assert_eq!(4000 - 1575, 2425);
    assert_eq!(a.sizes.iter().map(|s| s.length_mm).collect::<Vec<_>>(), [4000, 6000]);
    // The outline, point by point, centred on its block (tail at y = -2 m, tip at +2 m) and
    // pointing east: its area by shoelace is 0.90300 m2: trapezoid (0.3 + 0.12) / 2 x 2.425 =
    // 0.50925 and head triangle 0.5 x 1.575 / 2 = 0.39375.
    let outline = [(-0.15, -2.0), (0.15, -2.0), (0.06, 0.425), (0.25, 0.425), (0.0, 2.0), (-0.25, 0.425), (-0.06, 0.425)];
    assert!(near(shoelace(&outline), 0.903));
    let (_, l) = lay(ExtDefinition::point("M41", Uganda, 100.0, 0.0, 0.0).with_speed(50, "50 km/h"));
    assert!(near(l.painted_area_m2.unwrap(), shoelace(&outline)) && near(l.painted_area_m2.unwrap(), 0.903));
    assert!(!l.placeholder && l.number == 1);
    // Frame: world = (100 + y, -x). Tip (0, 2) is (102, 0); tail corners (+-0.15, -2) are
    // (98, -+0.15); the head base corners (+-0.25, 0.425) are (100.425, -+0.25).
    let (shaft, head) = (poly(&l, 0), poly(&l, 1));
    assert!(near2(shaft[0], (98.0, 0.15)) && near2(shaft[1], (98.0, -0.15)) && near2(shaft[2], (100.425, -0.06)) && near2(shaft[3], (100.425, 0.06)));
    assert!(near2(head[0], (100.425, 0.25)) && near2(head[1], (100.425, -0.25)) && near2(head[2], (102.0, 0.0)));
    // The 6 m arrow: head and shaft lengths scaled by 1.5, widths unchanged: neck at y = 3 - 2.3625
    // = 0.6375; areas 0.21 x 3.6375 = 0.763875 and 0.5 x 0.5 x 2.3625 = 0.590625.
    let (_, l6) = lay(ExtDefinition::point("M41", Uganda, 100.0, 0.0, 0.0).with_speed(80, "80 km/h"));
    assert!(near(l6.painted_area_m2.unwrap(), 0.763875 + 0.590625));
    assert!(near2(poly(&l6, 0)[2], (100.6375, -0.06)));
    // Rotated 180: points down-station.
    let (_, back) = lay(ExtDefinition::point("M41", Uganda, 100.0, 0.0, 180.0).with_size(4000));
    assert!(near2(poly(&back, 1)[2], (98.0, 0.0)));
    // Siting, Uganda sign page M41: 15 m and a further 30 m; 6 m arrows 50 per cent more.
    assert_eq!(uganda_lane_arrow_siting(4), Some((15.0, 30.0)));
    assert_eq!(uganda_lane_arrow_siting(6), Some((15.0 * 1.5, 30.0 * 1.5)));
    assert_eq!(uganda_lane_arrow_siting(9), None);
    // The element with a size outside the sign page.
    assert_eq!(errors(ExtDefinition::point("M41", Uganda, 100.0, 0.0, 0.0).with_size(5000)), ["marking.dimension.not_prescribed"]);
}

#[test]
fn uganda_other_arrows_are_blocks() {
    // M42 (PDF p.177): 4000 (6000) by 600. M44 (PDF p.179): 8000 (16000) by 2100. M45 (PDF p.180):
    // 5000 (8000) by 800. M43 (PDF p.178): the overall width cannot be read: an input.
    let size = |id: &str, speed: u32| {
        let (_, l) = lay(ExtDefinition::point(id, Uganda, 100.0, 0.0, 0.0).with_speed(speed, ""));
        l.bounding_area_m2.unwrap()
    };
    assert!(near(size("M42", 50), 4.0 * 0.6) && near(size("M42", 80), 6.0 * 0.6));
    assert!(near(size("M44", 50), 8.0 * 2.1) && near(size("M44", 80), 16.0 * 2.1));
    assert!(near(size("M45", 50), 5.0 * 0.8) && near(size("M45", 80), 8.0 * 0.8));
    assert_eq!(errors(ExtDefinition::point("M43", Uganda, 100.0, 0.0, 0.0).with_speed(50, "")), ["marking.dimension.required"]);
    let (_, l) = lay(ExtDefinition::point("M43", Uganda, 100.0, 0.0, 0.0).with_speed(50, "").with_width(1850));
    assert!(near(l.bounding_area_m2.unwrap(), 4.0 * 1.85) && l.painted_area_m2.is_none());
    // 60 km/h is the arrow threshold: "exceeds 60 km/h" takes the long arrow.
    assert!(near(size("M42", 60), 4.0 * 0.6) && near(size("M42", 61), 6.0 * 0.6));
}

#[test]
fn uganda_island_marking_m53_and_text_marking_m62() {
    // M53 (PDF p.183): yellow bars 200 mm at 45 degrees, 2000 apart, 150 mm continuous borders.
    match spec("M53").kind {
        Kind::Hatch(h) => assert_eq!((h.long.mark_width_mm, h.long.spacing_mm, h.boundary_widths_mm, h.boundary_continuous, h.chevrons), (200, 2000, &[150u32][..], true, false)),
        _ => panic!(),
    }
    assert_eq!(spec("M53").colour, MarkingColour::Yellow);
    // 20 m, 3 m between the borders: a bar needs 3 + 2 x 0.141421 = 3.282843; k x 2 + 3.282843 <= 20
    // gives k = 0 to 8: nine bars; borders 2 x 20 x 0.15 = 6.0.
    let (_, l) = lay(ExtDefinition::linear("M53", Uganda, 0.0, 20.0, 0.0).with_strip_width(3000));
    assert_eq!(l.number, 2 + 9);
    assert!(near(l.painted_area_m2.unwrap(), 6.0 + 9.0 * 3.0 * 0.2 * 2f64.sqrt()));
    // M62 (PDF p.185): 2050 wide, 1600 or 2800 long.
    match spec("M62").kind {
        Kind::Word(w) => assert_eq!((w.width_mm, w.heights), (2050, &[(Sel::AtMost(60), 1600u32), (Sel::Over(60), 2800)][..])),
        _ => panic!(),
    }
    let (_, l) = lay(ExtDefinition::point("M62", Uganda, 100.0, 0.0, 0.0).with_size(2800));
    assert!(near(l.bounding_area_m2.unwrap(), 2.8 * 2.05));
    // Catalogued only: M35, M51, M52, M63, M71.
    for id in ["M35", "M51", "M52", "M63", "M71"] {
        assert!(matches!(spec(id).geometry, GeometryStatus::CatalogueOnly(_)), "{id}");
        assert_eq!(errors(ExtDefinition::point(id, Uganda, 100.0, 0.0, 0.0)), ["marking.geometry.catalogue_only"], "{id}");
    }
}

#[test]
fn uganda_road_studs_table_e_1() {
    // Uganda TSM Vol.1 Table E.1, p.40: colour and spacing (urban, rural), m.
    let row = |line: &str| stud_rows(Uganda, line, None, None).into_iter().next().unwrap();
    let r = row("M01");
    assert_eq!((r.colour, r.urban_m, r.rural_m), (StudColour::White, Some(6.0), Some(6.0)));
    for line in ["M02", "M04-L", "M04-R"] {
        let r = row(line);
        assert_eq!((r.colour, r.urban_m, r.rural_m), (StudColour::White, Some(6.0), Some(6.0)), "{line}");
    }
    for line in ["M03", "M12"] {
        let r = row(line);
        assert_eq!((r.colour, r.urban_m, r.rural_m), (StudColour::White, Some(18.0), Some(24.0)), "{line}");
    }
    let r = row("M21");
    assert_eq!((r.colour, r.urban_m, r.rural_m), (StudColour::Red, Some(18.0), Some(24.0)));
    assert!(r.text.contains("amber"));
    let r = row("M22");
    assert_eq!((r.colour, r.urban_m, r.rural_m), (StudColour::Green, Some(3.0), Some(3.0)));
    // M11 has no row: the table gives no studs for the continuous lane line.
    assert!(stud_rows(Uganda, "M11", None, None).is_empty());
    // Count: M03 rural, 0 to 240 m at 24 m: eleven studs; M22 at 3 m over 30 m: eleven.
    assert_eq!(lay(ExtDefinition::studs(Uganda, 0.0, 240.0, 0.0, 24000, StudColour::White)).1.number, 11);
    assert_eq!(lay(ExtDefinition::studs(Uganda, 0.0, 30.0, 0.0, 3000, StudColour::Green)).1.number, 11);
    // The sight distance table of M01.
    assert_eq!([50, 60, 80, 100].map(|v| uganda_overtaking_sight_distance_m(v).unwrap()), [150, 180, 240, 300]);
    assert_eq!(uganda_overtaking_sight_distance_m(70), None);
}

// ===================================================================== basis, lookup, element

#[test]
fn a_type_the_uganda_manual_lacks_is_not_defined_and_the_uk_value_is_a_recorded_choice() {
    // The Uganda manual has no warning line, no hatched marking, no double white line.
    for family in [Family::WarningLine, Family::DoubleWhiteLine, Family::HatchedMarking] {
        match lookup_family(Uganda, family) {
            Lookup::NotDefined(n) => assert!(n.message().contains("not defined in the Uganda manual"), "{}", n.message()),
            Lookup::Found(_) => panic!("{family:?}"),
        }
        assert!(lookup_family(Uk, family).is_defined());
    }
    match lookup_family(Uganda, Family::WarningLine) {
        Lookup::NotDefined(n) => assert!(n.message().contains("1004")),
        _ => panic!(),
    }
    assert_eq!(lookup_family(Uganda, Family::StopLine).found(), Some(&["M31"][..]));
    assert_eq!(lookup_family(Uk, Family::StopLine).found(), Some(&["1001", "1002.1"][..]));
    // Families the UK crate does not catalogue are said so, not claimed undefined in the UK manual.
    match lookup_family(Uk, Family::PedestrianCrossing) {
        Lookup::NotDefined(n) => assert!(n.message().contains("not catalogued for the UK basis") && !n.message().contains("not defined in the UK manual")),
        _ => panic!(),
    }
    // Types by basis.
    assert!(matches!(lookup_type(Uk, "1040"), Lookup::Found(TypeRef::Ext(_))));
    assert!(matches!(lookup_type(Uk, "1012.1"), Lookup::Found(TypeRef::Legacy(LineType::Edge1012_1))));
    assert!(matches!(lookup_type(Uganda, "M31"), Lookup::Found(TypeRef::Ext(_))));
    assert!(matches!(lookup_type(Uganda, "1040"), Lookup::NotDefined(_)));
    assert!(matches!(lookup_type(Uganda, "1012.1"), Lookup::NotDefined(_)));
    assert!(matches!(lookup_type(Uk, "M31"), Lookup::NotDefined(_)));
    assert!(matches!(lookup_type(Uganda, "STUD"), Lookup::Found(TypeRef::Studs)));

    // A UK type on a Uganda element: an Error naming the Uganda manual...
    let hatch = || ExtDefinition::linear("1040", Uganda, 0.0, 24.0, 0.0).with_strip_width(3000).with_width(100).with_speed(30, "");
    let notes = on(hatch()).lay_out().1;
    assert_eq!(notes.iter().map(|n| (n.severity, n.code.as_str())).collect::<Vec<_>>(), [(Severity::Error, "marking.basis.not_defined")]);
    assert!(notes[0].message.contains("not defined in the Uganda manual"));
    // ...until the caller chooses the UK value, which is recorded in the element and its output.
    let (resolved, laid) = lay(hatch().with_uk_fallback("no Uganda equivalent; UK 1040 adopted for the design"));
    assert!(laid.number > 0);
    assert!(resolved.derived.iter().any(|(k, v)| k.starts_with("UK fallback") && v.contains("UK 1040 adopted")));
    let notes = on(hatch().with_uk_fallback("reason")).lay_out().1;
    assert!(notes.iter().any(|n| n.code == "marking.basis.uk_fallback" && n.severity == Severity::Info));
    // A Uganda sign on a UK element is refused too.
    assert_eq!(errors(ExtDefinition::across("M31", Uk, 60.0, -3.65, 0.0)), ["marking.basis.not_defined"]);
    assert_eq!(errors(ExtDefinition::across("9999", Uk, 60.0, -3.65, 0.0)), ["marking.type.unknown"]);

    // The first catalogue's elements: no basis recorded is UK, unchanged.
    let mut def = MarkingDefinition::new(LineType::Centre1008, 0.0, 60.0, 0.0, 30).with_width(100);
    def.alignment = "S".into();
    def.geometry = Some(straight());
    assert!(def.lay_out().0.is_some());
    assert!(!def.to_value().as_object().unwrap().contains_key("basis"));
    def.basis = Some(Uk);
    assert!(def.lay_out().0.is_some());
    def.basis = Some(Uganda);
    let notes = def.lay_out().1;
    assert_eq!(notes.iter().map(|n| n.code.as_str()).collect::<Vec<_>>(), ["marking.basis.not_defined"]);
    assert!(notes[0].message.contains("not defined in the Uganda manual"));
    def.uk_fallback = Some("caller's choice".into());
    let (laid, notes) = def.lay_out();
    assert!(laid.is_some() && notes.iter().any(|n| n.code == "marking.basis.uk_fallback"));
    // Stored before the field existed: reads as UK.
    let old: MarkingDefinition = serde_json::from_str(r#"{"line_type":"1008","start_station_m":0,"end_station_m":60,"speed_limit_mph":30}"#).unwrap();
    assert_eq!(old.basis, None);
}

#[test]
fn every_entry_is_catalogued_with_a_basis_and_a_source() {
    assert_eq!(ext_catalogue_for(Uk).count() + ext_catalogue_for(Uganda).count(), crate::ext_catalogue::ext_catalogue().len());
    // The Uganda manual defines these types (Vol.1 E.2 to E.4 and Vol.2 App.3 sign pages).
    let ids: Vec<&str> = ext_catalogue_for(Uganda).map(|s| s.id).collect();
    for id in ["M01", "M02", "M03", "M04-L", "M05", "M11", "M12", "M21", "M22", "M23", "M31", "M32", "M33", "M34", "M35", "M41", "M42", "M43", "M44", "M45", "M51", "M52", "M53", "M61", "M62", "M63", "M71"] {
        assert!(ids.contains(&id), "{id}");
    }
    // No stored Uganda entry is white where the manual says yellow: M01 to M05, M23, M53 yellow.
    for id in ["M01", "M02", "M03", "M04-L", "M05", "M23", "M53"] {
        assert_eq!(spec(id).colour, MarkingColour::Yellow, "{id}");
    }
    for s in crate::ext_catalogue::ext_catalogue() {
        assert_eq!(crate::ext_catalogue::ext_spec(s.id).unwrap().id, s.id);
    }
    // Colour override warns.
    let mut def = ExtDefinition::linear("M01", Uganda, 0.0, 30.0, 0.0);
    def.colour = Some(MarkingColour::White);
    let notes = on(def).lay_out().1;
    assert_eq!(notes.iter().map(|n| n.code.as_str()).collect::<Vec<_>>(), ["marking.colour"]);
}

fn document() -> (Document, ElementId) {
    let alignment = AlignmentBuilder::new("MC10", 0.0, 1000.0, 5000.0, std::f64::consts::FRAC_PI_2).line(200.0).arc(300.0, 100.0, Turn::Right).build().unwrap();
    let mut doc = Document::default();
    let id = doc.add(afrobim_alignment::CATEGORY, "MC10", Pt::new(0.0, 0.0), 1.0, 1.0, "Alignments", serde_json::to_value(&alignment).unwrap());
    (doc, id)
}

fn generate(doc: &Document, id: ElementId) -> afrobim_kernel::Generated {
    let mut modules = ModuleRegistry::default();
    modules.register(Box::new(ExtMarkingModule));
    modules.generate(doc.find(id).unwrap()).unwrap()
}

#[test]
fn elements_are_added_drawn_placed_and_measured() {
    let (mut doc, a) = document();
    // An arrow at station 100 on the heading-north tangent: tip at (1000, 5102).
    let arrow = add_ext_marking(&mut doc, a, "RX-001", &ExtDefinition::point("M41", Uganda, 100.0, 0.0, 0.0).with_speed(50, "50 km/h").with_material("thermoplastic")).unwrap();
    let e = doc.find(arrow).unwrap().clone();
    assert_eq!((e.category.as_str(), e.layer.as_str()), (CATEGORY_EXT, crate::LAYER));
    // The element's frame: the arrow spans x 999.75..1000.25 and y 5098..5102.
    assert!((e.width - 500.0).abs() < 1e-6 && (e.height - 4000.0).abs() < 1e-6);
    let site = e.site.clone().unwrap();
    assert!((site.x - 999_750.0).abs() < 1e-6 && (site.y - 5_098_000.0).abs() < 1e-6);
    let g = generate(&doc, arrow);
    assert!(g.notes.is_empty(), "{:?}", g.notes);
    assert_eq!(g.drawing.primitives.len(), 2);
    assert_eq!(g.derived["Basis"], "Uganda TSM (2004)");
    assert_eq!(g.derived["Painted area"], "0.903 m²");
    assert_eq!(g.derived["Speed note (caller)"], "50 km/h");
    assert!(g.derived["Source"].contains("M41") && g.derived["Source"].contains("July 2004"));
    // The solid is the two convex prisms: volume area x 5 mm (mm3).
    assert_eq!(g.solids.len(), 1);
    assert!((g.solids[0].volume() - 903_000.0 * crate::DISPLAY_THICKNESS_MM).abs() < 1.0, "{}", g.solids[0].volume());
    // A hatch on the curve: solids per group, volumes positive.
    let hatch = add_ext_marking(&mut doc, a, "RX-002", &ExtDefinition::linear("1040", Uk, 150.0, 250.0, 0.0).with_strip_width(3000).with_width(100).with_speed(30, "")).unwrap();
    let g = generate(&doc, hatch);
    assert_eq!(g.solids.len(), 3);
    assert!(g.solids.iter().all(|m| m.volume() > 0.0));
    // Studs.
    let studs = add_ext_marking(&mut doc, a, "RX-003", &ExtDefinition::studs(Uk, 0.0, 90.0, 0.0, 18000, StudColour::White)).unwrap();
    let g = generate(&doc, studs);
    assert_eq!((g.derived["Number"].as_str(), g.derived["Colour"].as_str()), ("6", "white"));
    assert_eq!(g.derived["Painted area"], "not available (placeholder block or studs)");
    // A placeholder.
    let word = add_ext_marking(&mut doc, a, "RX-004", &ExtDefinition::point("1024", Uk, 50.0, 0.0, 0.0).with_speed(30, "30 mph")).unwrap();
    let g = generate(&doc, word);
    assert!(g.notes.iter().any(|n| n.code == "marking.placeholder"));
    assert_eq!(g.derived["Block area"], "3.731 m²");
    // Refused: a Uganda basis UK type, stations outside.
    let bad = ExtDefinition::linear("1040", Uganda, 0.0, 24.0, 0.0).with_strip_width(3000).with_width(100).with_speed(30, "");
    match add_ext_marking(&mut doc, a, "RX-005", &bad) {
        Err(MarkingError::InvalidDefinition(m)) => assert!(m.contains("not defined in the Uganda manual"), "{m}"),
        other => panic!("{other:?}"),
    }
    assert!(add_ext_marking(&mut doc, a, "RX-006", &ExtDefinition::linear("M01", Uganda, 250.0, 400.0, 0.0)).is_err());
    // An edit and a sync.
    set_ext_marking(&mut doc, arrow, &ExtDefinition::point("M41", Uganda, 120.0, 0.0, 0.0).with_size(6000)).unwrap();
    assert_eq!(ext_marking_of(&doc, arrow).unwrap().station_m, 120.0);
    assert_eq!(sync_ext_markings(&mut doc), 0);
    let moved = AlignmentBuilder::new("MC10", 0.0, 1010.0, 5000.0, std::f64::consts::FRAC_PI_2).line(300.0).build().unwrap();
    doc.find_mut(a).unwrap().data = serde_json::to_value(&moved).unwrap();
    assert_eq!(sync_ext_markings(&mut doc), 4);

    // Quantities and CSV.
    let q = ExtTakeoff::measure(doc.elements());
    assert!(q.skipped.is_empty() && q.rows.len() == 4);
    let arrow_row = &q.rows[0];
    assert_eq!((arrow_row.type_id.as_str(), arrow_row.basis, arrow_row.number), ("M41", Uganda, 1));
    assert!(near(arrow_row.painted_area_m2.unwrap(), 0.763875 + 0.590625));
    assert_eq!(q.rows[2].number, 6);
    assert!(q.rows[3].placeholder && q.rows[3].painted_area_m2.is_none());
    let csv = ext_markings_to_csv(&q);
    let lines: Vec<&str> = csv.lines().collect();
    assert_eq!(lines[0], EXT_CSV_HEADER);
    let cols = EXT_CSV_HEADER.split(',').count();
    let fields = |l: &str| {
        let (mut n, mut quoted) = (1, false);
        for c in l.chars() {
            match c {
                '"' => quoted = !quoted,
                ',' if !quoted => n += 1,
                _ => {}
            }
        }
        n
    };
    assert!(lines[1..].iter().all(|l| fields(l) == cols), "{csv}");
    assert!(lines[1].starts_with("RX-001,M41,Uganda TSM (2004),M41,"));
    assert!(lines[4].contains(",true,"));
    assert!(lines[5].starts_with("TOTAL,4,"));
    // An invalid element is skipped and counted, not dropped.
    doc.add(CATEGORY_EXT, "RX-BAD", Pt::new(0.0, 0.0), 1.0, 1.0, crate::LAYER, serde_json::json!({"type_id": "M01", "station_m": 0.0}));
    let q = ExtTakeoff::measure(doc.elements());
    assert_eq!(q.skipped.len(), 1);
    assert!(ext_markings_to_csv(&q).contains("NOT MEASURED"));
}

#[test]
fn definitions_round_trip_and_old_data_reads() {
    let mut def = on(ExtDefinition::across("1003A", Uk, 80.0, 0.0, 3.0).with_material("thermoplastic").with_speed(30, "note"));
    def.uk_fallback = None;
    let text = serde_json::to_string(&def).unwrap();
    let back: ExtDefinition = serde_json::from_str(&text).unwrap();
    assert_eq!(back, def);
    let minimal: ExtDefinition = serde_json::from_str(r#"{"type_id":"M31","station_m":5.0}"#).unwrap();
    assert_eq!((minimal.basis, minimal.offset_m, minimal.rotation_deg), (Uk, 0.0, 0.0));
    // The first element category is untouched by the extension: the original module still reads
    // its own elements and rejects the extension's.
    assert!(MarkingDefinition::from_value(&def.to_value()).is_err());
}

/// A straight road running north from (1000, 5000), stations 0 to 300, rising 3 m over its length
/// (50 m at station 0, 53 m at station 300): the level at station s is 50 + s / 100 metres, and
/// plan Y is 5000 + s, so a vertex's expected level follows from its plan position alone.
fn rising_road() -> (Document, ElementId) {
    let mut alignment = AlignmentBuilder::new("MC10", 0.0, 1000.0, 5000.0, std::f64::consts::FRAC_PI_2).line(300.0).build().unwrap();
    alignment.profile = Some(afrobim_alignment::Profile::new("FG", vec![afrobim_alignment::Pvi::point(0.0, 50.0), afrobim_alignment::Pvi::point(300.0, 53.0)]).unwrap());
    let mut doc = Document::default();
    let id = doc.add(afrobim_alignment::CATEGORY, "MC10", Pt::new(0.0, 0.0), 1.0, 1.0, "Alignments", serde_json::to_value(&alignment).unwrap());
    (doc, id)
}

/// Every vertex of every solid of the marking, in site metres with its height in millimetres.
fn world_vertices(doc: &Document, id: ElementId) -> Vec<(f64, f64, f64)> {
    let g = generate(doc, id);
    let site = doc.find(id).unwrap().site.clone().unwrap();
    g.solids.iter().flat_map(|m| m.vertices.iter().map(|v| ((site.x + v[0]) / 1000.0, (site.y + v[1]) / 1000.0, v[2]))).collect()
}

#[test]
fn every_solid_of_every_kind_of_marking_sits_at_the_road_level_of_its_station() {
    // A defect this pins: the solids of polygons (stop lines, arrows, the triangle, the diagonals
    // of hatching, chevrons, studs) were built at height zero, while strips followed the profile,
    // so on a road at 50 m or more they hung 50 m below the pavement and, in the 3D view, looked
    // displaced from it. The level here is computed from the plan position alone (Y = 5000 + s,
    // level = 50 + s/100), not through the engine's own station lookup.
    let (mut doc, a) = rising_road();
    let level_at_y = |y_m: f64| (50.0 + (y_m - 5000.0) / 100.0) * 1000.0;
    let marks = [
        ("stop line", ExtDefinition::across("1002.1", Basis::Uk, 100.0, -3.5, 0.0)),
        ("give way triangle", ExtDefinition::point("1023A", Basis::Uk, 120.0, 0.0, 0.0)),
        ("lane arrow M41", ExtDefinition::point("M41", Basis::Uganda, 140.0, 1.8, 0.0).with_speed(60, "test")),
        ("arrow block", ExtDefinition::point("1038-ahead", Basis::Uk, 160.0, 1.75, 0.0).with_speed(30, "test")),
        ("hatching", ExtDefinition::linear("1040", Basis::Uk, 170.0, 200.0, 0.0).with_strip_width(3000).with_width(100).with_speed(40, "test")),
        ("chevrons", ExtDefinition::linear("1041", Basis::Uk, 210.0, 240.0, 0.0).with_strip_width(3000).with_width(100).with_speed(30, "test")),
        ("zig-zag", ExtDefinition::linear("M05", Basis::Uganda, 245.0, 270.0, 0.0)),
        ("studs", ExtDefinition::studs(Basis::Uk, 10.0, 90.0, 0.0, 9000, StudColour::White)),
        ("zebra", ExtDefinition { count: Some(5), gap_mm: Some(600), ..ExtDefinition::across("M33", Basis::Uganda, 275.0, -2.0, 2.0).with_width(600) }),
    ];
    for (what, def) in marks {
        let id = add_ext_marking(&mut doc, a, "RX-001", &def).unwrap_or_else(|e| panic!("{what}: {e}"));
        let vertices = world_vertices(&doc, id);
        assert!(!vertices.is_empty(), "{what}: no solids");
        // The prism is DISPLAY_THICKNESS_MM thick: the lowest vertex of each column is at the
        // level and the highest is the thickness above it.
        for (_, y, z) in &vertices {
            let level = level_at_y(*y);
            assert!(*z >= level - 1e-6 && *z <= level + crate::module::DISPLAY_THICKNESS_MM + 1e-6, "{what}: a vertex at y {y:.3} is at {z:.1} mm, the road level there is {level:.1} mm");
        }
        // It is not flat on the ground: the span of heights is the rise over the marking's length
        // (plus the thickness), which for a long marking is far more than the thickness.
        let (lo, hi) = vertices.iter().fold((f64::INFINITY, f64::NEG_INFINITY), |(lo, hi), v| (lo.min(v.2), hi.max(v.2)));
        assert!(lo > 50_000.0, "{what}: lowest vertex at {lo} mm");
        assert!(hi - lo >= crate::module::DISPLAY_THICKNESS_MM - 1e-6, "{what}");
    }
}

#[test]
fn a_marking_sits_over_the_pavement_strip_it_is_laid_on_in_plan_and_in_level() {
    // The lateral offset is positive to the right looking up-station. On this road, running
    // north, right is east: an offset of +1.8 m is at X = 1001.8, and -3.5 at X = 996.5.
    let (mut doc, a) = rising_road();
    let id = add_ext_marking(&mut doc, a, "RX-001", &ExtDefinition::point("M41", Basis::Uganda, 150.0, 1.8, 0.0).with_speed(60, "test")).unwrap();
    let v = world_vertices(&doc, id);
    let (lo, hi) = v.iter().fold(([f64::INFINITY; 3], [f64::NEG_INFINITY; 3]), |(lo, hi), p| ([lo[0].min(p.0), lo[1].min(p.1), lo[2].min(p.2)], [hi[0].max(p.0), hi[1].max(p.1), hi[2].max(p.2)]));
    let centre = ((lo[0] + hi[0]) / 2.0, (lo[1] + hi[1]) / 2.0);
    assert!((centre.0 - 1001.8).abs() < 0.05, "X centre {} should be 1001.8", centre.0);
    assert!((centre.1 - 5150.0).abs() < 0.05, "Y centre {} should be 5150", centre.1);
    // The arrow's own height sits on the road at its station, 51.5 m at station 150.
    assert!((lo[2] - 51_000.0).abs() < 600.0 && hi[2] > 51_500.0, "{lo:?} {hi:?}");
    let id = add_ext_marking(&mut doc, a, "RX-002", &ExtDefinition::across("1002.1", Basis::Uk, 100.0, -3.5, 0.0)).unwrap();
    let v = world_vertices(&doc, id);
    let (xlo, xhi) = v.iter().fold((f64::INFINITY, f64::NEG_INFINITY), |(lo, hi), p| (lo.min(p.0), hi.max(p.0)));
    assert!(xlo > 996.4 && xhi < 1000.1, "the stop line spans X {xlo}..{xhi}, expected 996.5..1000");
}
