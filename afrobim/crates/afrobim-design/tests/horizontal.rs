//! Hand-worked horizontal tests. The working is in the comments; every figure can be checked
//! with a calculator.

use afrobim_alignment::Pi;
use afrobim_design::*;

fn close(a: f64, b: f64, tol: f64, what: &str) {
    assert!((a - b).abs() <= tol, "{what}: got {a}, expected {b} (tolerance {tol})");
}

fn plain() -> HorizontalDesign {
    // PI0 (0,0) -> PI1 (1000,0) -> PI2 at 60 degrees from east, 1000 m long:
    // (1000 + 1000 cos 60, 1000 sin 60) = (1500, 866.0254037844386).
    HorizontalDesign::new("A", 0.0, vec![Pi::point(0.0, 0.0), Pi::curve(1000.0, 0.0, 300.0), Pi::point(1500.0, 866.025_403_784_438_6)])
}

#[test]
fn simple_circular_curve() {
    // Delta = 60 deg = pi/3, R = 300.
    //   T  = R tan(Delta/2)       = 300 tan 30  = 300 x 0.5773502692 = 173.2050808
    //   Lc = R Delta              = 300 x 1.0471975512 = 314.1592654
    //   E  = R (sec 30 - 1)       = 300 x (1.1547005384 - 1) = 46.4101615
    //   M  = R (1 - cos 30)       = 300 x (1 - 0.8660254038) = 40.1923789
    // TS station = 1000 - 173.2050808 = 826.7949192; PT = 826.7949192 + 314.1592654 = 1140.9541846.
    // Total length = (1000 - T) + Lc + (1000 - T) = 1653.5898385 + 314.1592654 = 1967.7491038.
    let s = plain().solve().unwrap();
    let c = s.curve_at_pi(1).unwrap();
    close(c.deflection_rad.to_degrees(), 60.0, 1e-9, "deflection");
    close(c.tangent_in_m, 173.205_080_756_887_7, 1e-9, "T");
    close(c.tangent_out_m, 173.205_080_756_887_7, 1e-9, "T out");
    close(c.arc_length_m, 314.159_265_358_979_3, 1e-9, "Lc");
    close(c.external_m, 46.410_161_513_775_45, 1e-9, "E");
    close(c.mid_ordinate_m, 40.192_378_864_668_42, 1e-9, "M");
    close(c.ts_station_m, 826.794_919_243_112_3, 1e-9, "TS");
    close(c.st_station_m, 1_140.954_184_602_091_6, 1e-9, "PT");
    close(c.pi_station_m, 1000.0, 1e-9, "PI station");
    close(s.alignment.length(), 1_967.749_103_845_203_8, 1e-8, "length");
    // The arc's centre is R from TS square to the tangent, on the left: (826.795, 300).
    close(c.centre.0, 826.794_919_243_112_3, 1e-9, "centre x");
    close(c.centre.1, 300.0, 1e-9, "centre y");
    assert!(s.alignment.joints().iter().all(|j| j.gap_m < 1e-9 && j.heading_change_rad.abs() < 1e-12));
    // Ends on the second tangent, heading 60 degrees.
    let (x, y, h) = s.alignment.elements.last().unwrap().end();
    close(x, 1500.0, 1e-8, "end x");
    close(y, 866.025_403_784_438_6, 1e-8, "end y");
    close(h.to_degrees(), 60.0, 1e-9, "end heading");
}

#[test]
fn curve_with_equal_spirals() {
    // R = 300, Ls = 100, Delta = 60 deg.
    //   theta_s = Ls / 2R = 100 / 600 = 0.1666667 rad
    //   x = Ls (1 - th^2/10 + th^4/216 - th^6/9360) = 100 (1 - 0.00277778 + 0.00000357 - 0.000000002) = 99.722576
    //   y = Ls (th/3 - th^3/42 + th^5/1320)        = 100 (0.05555556 - 0.00011023 + 0.0000000974) = 5.544542
    //   p = y - R (1 - cos th) = 5.544542 - 300 x 0.01385677 = 5.544542 - 4.157031 = 1.387511
    //   k = x - R sin th       = 99.722576 - 300 x 0.16589613 = 99.722576 - 49.768839 = 49.953737
    //   Ts = (R + p) tan 30 + k = 301.387511 x 0.57735027 + 49.953737 = 174.006163 + 49.953737 = 223.9599
    //   Lc = R (Delta - 2 theta_s) = 300 (1.0471976 - 0.3333333) = 214.159265
    //   total = 2 x 100 + 214.159265 = 414.159265
    // Reference values to 12 digits from the Fresnel integrals (mpmath), see tests/data/gen_clothoid.py.
    let mut d = plain();
    d.pis[1] = Pi::spiral_curve(1000.0, 0.0, 300.0, 100.0, 100.0);
    let s = d.solve().unwrap();
    let c = s.curve_at_pi(1).unwrap();
    close(c.theta_in_rad, 0.166_666_666_666_666_67, 1e-15, "theta");
    close(c.spiral_in_xy.0, 99.722_579_217_827_45, 1e-10, "x");
    close(c.spiral_in_xy.1, 5.544_542_365_628_803, 1e-10, "y");
    close(c.shift_in_m, 1.387_511_834_506_32, 1e-9, "p");
    close(c.extension_in_m, 49.953_739_409_802_94, 1e-9, "k");
    close(c.tangent_in_m, 223.959_900_497_846_7, 1e-8, "Ts");
    close(c.arc_length_m, 214.159_265_358_979_3, 1e-9, "Lc");
    close(c.total_length_m, 414.159_265_358_979_3, 1e-9, "total");
    close(c.external_m, 48.012_322_176_087_49, 1e-7, "Es = (R + p) sec(D/2) - R");
    close(c.mid_ordinate_m, 18.908_053_004_915_74, 1e-9, "M of the arc");
    // Stations: TS = 1000 - Ts, SC = TS + 100, CS = SC + Lc, ST = CS + 100.
    close(c.ts_station_m, 1000.0 - 223.959_900_497_846_7, 1e-8, "TS");
    close(c.sc_station_m - c.ts_station_m, 100.0, 1e-9, "SC");
    close(c.cs_station_m - c.sc_station_m, 214.159_265_358_979_3, 1e-9, "CS");
    close(c.st_station_m - c.cs_station_m, 100.0, 1e-9, "ST");
    // Independent check of Ts by closure: laid out from TS, the curve must arrive on the second
    // tangent, heading 60 degrees, at a distance Ts from the PI.
    let n = s.alignment.elements.len();
    let (x, y, h) = s.alignment.elements[n - 2].end(); // the ST, end of the last curve element
    close(h.to_degrees(), 60.0, 1e-9, "end heading");
    close((x - 1000.0).hypot(y), 223.959_900_497_846_7, 1e-7, "ST to PI");
    close(y * 60f64.to_radians().cos() - (x - 1000.0) * 60f64.to_radians().sin(), 0.0, 1e-7, "ST lies on the second tangent (zero cross product with its direction)");
    assert_eq!(s.alignment.elements.len(), 5);
}

#[test]
fn unequal_spirals_close_on_the_tangents() {
    // The unequal-spiral tangent lengths are verified by closure rather than by a second use of
    // the same formula: lay the curve out from TS and require it to end on the out tangent at
    // T_out from the PI, heading along it.
    let mut d = HorizontalDesign::new("U", 0.0, vec![Pi::point(0.0, 0.0), Pi::spiral_curve(800.0, 100.0, 250.0, 60.0, 140.0), Pi::point(1500.0, 700.0)]);
    let s = d.solve().unwrap();
    let c = s.curve_at_pi(1).unwrap();
    assert!(c.tangent_in_m != c.tangent_out_m);
    let n = s.alignment.elements.len();
    let (x, y, h) = s.alignment.elements[n - 2].end(); // the ST
    let out = (700.0f64 - 100.0).atan2(1500.0 - 800.0);
    close(h, out, 1e-9, "end heading");
    close((x - 800.0).hypot(y - 100.0), c.tangent_out_m, 1e-7, "ST to PI");
    close((y - 100.0).atan2(x - 800.0), out, 1e-9, "ST on the out tangent");
    // Editing keeps working with unequal spirals.
    d.set_spirals(1, 80.0, 80.0).unwrap();
}

#[test]
fn reverse_curves_with_a_short_tangent_are_reported() {
    // PI0 (0,0), PI1 (500,0), PI2 (900,300), PI3 (1300,0): leg 1 and leg 2 are 3-4-5 triangles, 500 m long.
    // Leg 1 heading atan(3/4) = 36.8699 deg, left turn at PI1 of 36.8699 deg: tan(18.435) = 1/3
    //   (tan 2a = 2t/(1 - t^2) = (2/3)/(8/9) = 0.75 ok), R = 400, T1 = 400/3 = 133.333.
    // At PI2 the road turns right by 73.7398 deg: tan(36.8699) = 0.75, R = 400, T2 = 300.
    // Leg between PI1 and PI2 is 500 m: straight = 500 - 133.333 - 300 = 66.667 m, a reverse pair.
    let d = HorizontalDesign::new("R", 0.0, vec![Pi::point(0.0, 0.0), Pi::curve(500.0, 0.0, 400.0), Pi::curve(900.0, 300.0, 400.0), Pi::point(1300.0, 0.0)]);
    // Same triangle in the other order: heading 0, then 36.87 left at PI1 (leg 500), PI2 right.
    let s = d.solve().unwrap();
    let t = &s.tangents[1];
    assert_eq!(t.reverse, Some(true));
    close(t.straight_m, 500.0 - 400.0 / 3.0 - 300.0, 1e-9, "straight between the curves");
    close(s.curve_at_pi(1).unwrap().tangent_out_m, 400.0 / 3.0, 1e-9, "T1");
    close(s.curve_at_pi(2).unwrap().tangent_in_m, 300.0, 1e-9, "T2");
    // The curves touch end to start with no tangent when the leg is exactly T1 + T2.
    let mut d2 = d.clone();
    d2.pis[2] = Pi::curve(900.0, 300.0, 400.0);
    d2.pis[1] = Pi::curve(500.0, 0.0, 400.0);
    d2.pis[0] = Pi::point(0.0, 0.0);
    // Leg 1-2 is 500 m; make R2 such that T1 + T2 = 500: T2 = 366.667, R2 = 488.889.
    d2.pis[2].radius = 500.0 - 400.0 / 3.0;
    d2.pis[2].radius /= 0.75;
    let s2 = d2.solve().unwrap();
    assert!(s2.tangents[1].straight_m < 1e-6);
    assert!(s2.warnings.iter().any(|w| w.contains("no tangent between them") && w.contains("reverse")), "{:?}", s2.warnings);
}

#[test]
fn impossible_geometry_is_refused_in_plain_words() {
    // Overlap: R = 600 at PI2 needs T = 600 x 0.75 = 450; 133.333 + 450 = 583.333 > 500, short by 83.333.
    let d = HorizontalDesign::new("O", 0.0, vec![Pi::point(0.0, 0.0), Pi::curve(500.0, 0.0, 400.0), Pi::curve(900.0, 300.0, 600.0), Pi::point(1300.0, 0.0)]);
    let e = d.solve().unwrap_err();
    let msg = e.to_string();
    assert!(msg.contains("overlap by 83.333") && msg.contains("tangent between them is too short"), "{msg}");
    assert_eq!(e.problems()[0].index, Some(2));

    // Radius too small for spirals: Delta = 20 deg = 0.3491 rad; Ls 60 + 60 on R = 100 turns 0.6 rad = 34.4 deg.
    //   max total spiral for R = 100: 2 R Delta = 69.8 m;  min R for 120 m of spiral: 120 / (2 x 0.3491) = 171.9 m.
    let a = 20f64.to_radians();
    let d = HorizontalDesign::new("S", 0.0, vec![Pi::point(0.0, 0.0), Pi::spiral_curve(1000.0, 0.0, 100.0, 60.0, 60.0), Pi::point(1000.0 + 1000.0 * a.cos(), 1000.0 * a.sin())]);
    let msg = d.solve().unwrap_err().to_string();
    assert!(msg.contains("radius (100.0 m) is too small") && msg.contains("69.8 m") && msg.contains("171.9 m"), "{msg}");

    // A reversal, a coincident pair and too few PIs.
    assert!(HorizontalDesign::new("X", 0.0, vec![Pi::point(0.0, 0.0)]).solve().is_err());
    assert!(HorizontalDesign::new("X", 0.0, vec![Pi::point(0.0, 0.0), Pi::point(0.0, 0.0)]).solve().unwrap_err().to_string().contains("same point"));
    let back = HorizontalDesign::new("X", 0.0, vec![Pi::point(0.0, 0.0), Pi::curve(100.0, 0.0, 50.0), Pi::point(0.0, 0.0)]);
    assert!(back.solve().is_err());
}

#[test]
fn an_impossible_edit_is_refused_and_changes_nothing() {
    let mut d = plain();
    let before = d.clone();
    // Dragging PI1 so that the first leg is shorter than T: PI1 at (60, 0): T would be about 83 > 60.
    let e = d.move_pi(1, 60.0, 0.0).unwrap_err();
    assert!(e.to_string().contains("first leg"), "{e}");
    assert_eq!(d, before);
    assert!(d.set_radius(1, 5000.0).is_err());
    assert_eq!(d, before);
    assert!(d.delete_pi(9).is_err());
    // A good edit returns the recomputed solution.
    let s = d.set_radius(1, 400.0).unwrap();
    close(s.curve_at_pi(1).unwrap().tangent_in_m, 400.0 * 30f64.to_radians().tan(), 1e-9, "T after R = 400");
    // Insert and delete round trip.
    d.insert_pi(2, Pi::curve(1400.0, 500.0, 250.0)).unwrap();
    assert_eq!(d.pis.len(), 4);
    d.delete_pi(2).unwrap();
    assert_eq!(d.pis.len(), 3);
    // The drag limits for the UI: the radius range at PI1 is bounded by the legs.
    let (lo, hi) = d.feasible_radius_range(1).unwrap();
    assert!(lo > 0.0 && hi > 400.0 && hi < 1.0e7);
    // Just inside the upper limit is accepted, just outside is refused.
    let mut e1 = d.clone();
    assert!(e1.set_radius(1, hi * 0.999).is_ok());
    assert!(e1.set_radius(1, hi * 1.01).is_err());
}

#[test]
fn designed_alignment_is_a_normal_alignment() {
    let s = plain().solve().unwrap();
    let a = &s.alignment;
    // station/offset and point_at agree with each other on the designed geometry.
    let p = a.point_at(1000.0).unwrap();
    let (st, off) = a.station_offset(p.x, p.y).unwrap();
    close(st, 1000.0, 1e-6, "station");
    close(off, 0.0, 1e-6, "offset");
}
