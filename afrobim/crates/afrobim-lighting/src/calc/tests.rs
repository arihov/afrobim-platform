// SPDX-License-Identifier: LicenseRef-AfroBIM-Proprietary
// Copyright (c) 2026 AfroBIM. All rights reserved.
use std::f64::consts::PI;

use afrobim_alignment::AlignmentBuilder;
use afrobim_draw::Primitive;

use super::*;
use crate::layout::tests::{installation, spec, straight};
use crate::photometry::testdata::ies_simple;
use crate::photometry::{parse_ies, IesOptions, Photometry};
use crate::{layout_installation, layout_single_column, Arrangement, BracketAim, LightingData, LightingDefinition, SingleColumn};

fn close(a: f64, b: f64, tol: f64) -> bool {
    (a - b).abs() <= tol * b.abs().max(1.0)
}

/// Isotropic point source of `i` cd.
fn isotropic(i: f64) -> Photometry {
    let vert: Vec<f64> = (0..=12).map(|k| k as f64 * 15.0).collect();
    parse_ies(&ies_simple(&[0.0], &vert, &[vec![i; 13]]), IesOptions::default()).unwrap()
}

/// `i0 cos(gamma)` down to 90 degrees, nothing above (a Lambertian downward source), 1 degree table.
fn lambertian(i0: f64) -> Photometry {
    let vert: Vec<f64> = (0..=90).map(|k| k as f64).collect();
    let row: Vec<f64> = vert.iter().map(|g| i0 * g.to_radians().cos()).collect();
    parse_ies(&ies_simple(&[0.0], &vert, &[row]), IesOptions::default()).unwrap()
}

/// One column 5 m to the right of the alignment (heading east, so south of it) at station 150, its
/// bracket pointing north: the luminaire stands at (1150, 1996.5) at 10 m, offset 3.5 m.
fn single(tilt: f64) -> LightingData {
    let a = straight();
    let mut column = spec();
    column.tilt_deg = tilt;
    let def = SingleColumn { mark: "J1".to_string(), alignment: "A".to_string(), station_m: 150.0, offset_m: 5.0, aim: BracketAim::TowardAlignment, column, lighting_class: "C2".to_string() };
    let layout = layout_single_column(&a, &def).unwrap();
    LightingData { definition: LightingDefinition::Column(def), layout }
}

fn settings() -> CalcSettings {
    CalcSettings {
        maintenance_factor: 1.0,
        contribution_distance_m: 1000.0,
        orientation: Orientation::default(),
        areas: Vec::new(),
        plane_level_m: None,
        vertical: None,
        observers: Vec::new(),
        eir_lane_width_m: None,
    }
}

fn cell(station: f64, offset: f64) -> CalcArea {
    CalcArea {
        name: "P".to_string(),
        kind: AreaKind::Other,
        station_from_m: station - 0.001,
        station_to_m: station + 0.001,
        offset_from_m: offset - 0.001,
        offset_to_m: offset + 0.001,
        along: Division::Count(1),
        across: Division::Count(1),
    }
}

fn at(data: &LightingData, phot: &Photometry, s: &CalcSettings, station: f64, offset: f64) -> AreaResult {
    let mut s = s.clone();
    s.areas = vec![cell(station, offset)];
    let (frame, _) = GridFrame::from_layout(&data.layout).unwrap();
    let r = calculate(phot, data, &frame, &s, None).unwrap();
    r.areas.into_iter().next().unwrap()
}

#[test]
fn isotropic_point_source_gives_i_cos3_over_h2_at_hand_worked_points() {
    let data = single(0.0);
    let phot = isotropic(1000.0);
    // Directly below: E = I / h^2 = 1000 / 100 = 10 lx.
    let r = at(&data, &phot, &settings(), 150.0, 3.5);
    assert!(close(r.e_h_lx[0], 10.0, 1e-9), "{}", r.e_h_lx[0]);
    // 3 m along the road: d^2 = 9 + 100, E = I h / d^3 = 10000 / 109^1.5 = 8.7873 lx.
    let r = at(&data, &phot, &settings(), 153.0, 3.5);
    assert!(close(r.e_h_lx[0], 10000.0 / 109f64.powf(1.5), 1e-9) && close(r.e_h_lx[0], 8.7874, 1e-4), "{}", r.e_h_lx[0]);
    // 10 m along: gamma = 45 degrees, cos^3 = 0.35355, E = 1000 * 0.35355 / 100 = 3.5355 lx.
    let r = at(&data, &phot, &settings(), 160.0, 3.5);
    assert!(close(r.e_h_lx[0], 3.5355339, 1e-6), "{}", r.e_h_lx[0]);
    // The maintenance factor scales linearly.
    let mut s = settings();
    s.maintenance_factor = 0.8;
    assert!(close(at(&data, &phot, &s, 150.0, 3.5).e_h_lx[0], 8.0, 1e-9));
    // A plane raised by 4 m lowers the luminaire to 6 m above it: E = 1000 / 36.
    s.maintenance_factor = 1.0;
    s.plane_level_m = Some(4.0);
    assert!(close(at(&data, &phot, &s, 150.0, 3.5).e_h_lx[0], 1000.0 / 36.0, 1e-9));
}

#[test]
fn lambertian_downward_source_and_the_cosine_law() {
    // I = I0 cos(gamma): E_h = I0 cos^4(gamma) / h^2 under the same luminaire.
    let data = single(0.0);
    let phot = lambertian(1000.0);
    for dx in [0.0, 5.0, 10.0] {
        let r = at(&data, &phot, &settings(), 150.0 + dx, 3.5);
        let cos = 10.0 / (100.0 + dx * dx).sqrt();
        assert!(close(r.e_h_lx[0], 1000.0 * cos.powi(4) / 100.0, 1e-4), "dx {dx}: {}", r.e_h_lx[0]);
    }
    // At 90 degrees and beyond nothing is received.
    let r = at(&data, &phot, &settings(), 150.0 + 200.0, 3.5);
    assert!(r.e_h_lx[0] < 0.01 * 10.0);
}

#[test]
fn superposition_of_two_luminaires_and_the_contribution_distance() {
    // Two columns 40 m apart on the right; the point 20 m from each, on the luminaire line.
    let a = straight();
    let def = installation(Arrangement::SingleRight, 0.0, 40.0, 40.0);
    let layout = layout_installation(&a, &def).unwrap();
    assert_eq!(layout.columns.len(), 2);
    let data = LightingData { definition: LightingDefinition::Installation(def), layout };
    let phot = isotropic(1000.0);
    let r = at(&data, &phot, &settings(), 20.0, 3.5);
    // Each: d^2 = 400 + 100, E = 1000 * 10 / 500^1.5; two of them.
    let expect = 2.0 * 10000.0 / 500f64.powf(1.5);
    assert!(close(r.e_h_lx[0], expect, 1e-9), "{} against {expect}", r.e_h_lx[0]);
    assert!(close(expect, 1.78885, 1e-5));
    // The luminaires are 20 m away: a contribution distance of 19 m takes both out.
    let mut s = settings();
    s.contribution_distance_m = 19.0;
    assert_eq!(at(&data, &phot, &s, 20.0, 3.5).e_h_lx[0], 0.0);
    // A distance of 25 m takes both in.
    s.contribution_distance_m = 25.0;
    assert!(close(at(&data, &phot, &s, 20.0, 3.5).e_h_lx[0], expect, 1e-9));
}

#[test]
fn tilt_moves_a_lambertian_beam_by_hand_worked_amounts() {
    // Luminaire tilted 30 degrees, street end raised: the axis points 30 degrees from the nadir
    // towards the street (north). I = I0 cos(angle from the axis), h = 10.
    let data = single(30.0);
    let phot = lambertian(1000.0);
    let h = 10.0;
    // Directly below: angle from the axis is 30 degrees, E = I0 cos30 / h^2.
    let r = at(&data, &phot, &settings(), 150.0, 3.5);
    assert!(close(r.e_h_lx[0], 1000.0 * 30f64.to_radians().cos() / 100.0, 3e-5), "{}", r.e_h_lx[0]);
    // On the axis: north by h tan30; I = I0, E = I0 cos^3(30) / h^2.
    let north = h * 30f64.to_radians().tan();
    let r = at(&data, &phot, &settings(), 150.0, 3.5 - north);
    assert!(close(r.e_h_lx[0], 1000.0 * 30f64.to_radians().cos().powi(3) / 100.0, 3e-5), "{}", r.e_h_lx[0]);
    // South by h tan30: 60 degrees from the axis, I = I0/2, ray 30 degrees from vertical.
    let r = at(&data, &phot, &settings(), 150.0, 3.5 + north);
    assert!(close(r.e_h_lx[0], 0.5 * 1000.0 * 30f64.to_radians().cos().powi(3) / 100.0, 3e-5), "{}", r.e_h_lx[0]);
    // A tilt equal to the tilt during measurement leaves the distribution as measured.
    let mut p = lambertian(1000.0);
    p.measured_tilt_deg = 30.0;
    let r = at(&data, &p, &settings(), 150.0, 3.5);
    assert!(close(r.e_h_lx[0], 10.0, 3e-5), "{}", r.e_h_lx[0]);
}

#[test]
fn photometric_angles_by_hand() {
    // Bracket pointing north (pi/2): the street direction is north, left is west.
    let inst = LuminaireInstance { mark: "x".to_string(), x_m: 0.0, y_m: 0.0, z_m: 10.0, bracket_azimuth_rad: PI / 2.0, tilt_deg: 0.0 };
    let o = Orientation { c_toward_carriageway_deg: 90.0, counterclockwise: true };
    // The defaults per format.
    assert_eq!(Orientation::default(), Orientation { c_toward_carriageway_deg: 0.0, counterclockwise: true });
    assert_eq!(Orientation::for_format(&crate::photometry::SourceFormat::Eulumdat).c_toward_carriageway_deg, 270.0);
    let s45 = 45f64.to_radians().sin();
    let c45 = 45f64.to_radians().cos();
    // At gamma 0 (or 180) every C-plane is the same direction, so C is not compared there.
    let check = |d: [f64; 3], c: f64, g: f64, o: &Orientation, inst: &LuminaireInstance, measured: f64| {
        let (cc, gg) = photometric_angles(inst, o, measured, d);
        let c_ok = g.abs() < 1e-9 || (g - 180.0).abs() < 1e-9 || close(cc, c, 1e-9);
        assert!(c_ok && close(gg, g, 1e-9), "{d:?}: got C{cc} g{gg}, expected C{c} g{g}");
    };
    // North and 45 below horizontal: C90 (street side), gamma 45.
    check([0.0, s45, -c45], 90.0, 45.0, &o, &inst, 0.0);
    // East (right of the street direction): C0. West: C180. South (house side): C270.
    check([s45, 0.0, -c45], 0.0, 45.0, &o, &inst, 0.0);
    check([-s45, 0.0, -c45], 180.0, 45.0, &o, &inst, 0.0);
    check([0.0, -s45, -c45], 270.0, 45.0, &o, &inst, 0.0);
    // Straight down and straight up.
    check([0.0, 0.0, -1.0], 90.0, 0.0, &o, &inst, 0.0);
    check([0.0, 0.0, 1.0], 90.0, 180.0, &o, &inst, 0.0);
    // The other sense of C: east becomes C180.
    check([s45, 0.0, -c45], 180.0, 45.0, &Orientation { c_toward_carriageway_deg: 90.0, counterclockwise: false }, &inst, 0.0);
    // The C270 side towards the street: north is C270.
    check([0.0, s45, -c45], 270.0, 45.0, &Orientation { c_toward_carriageway_deg: 270.0, counterclockwise: true }, &inst, 0.0);
    // Tilt 30 raised towards the street: straight down is 30 degrees from the axis, on the house
    // side (C270).
    let tilted = LuminaireInstance { tilt_deg: 30.0, ..inst.clone() };
    check([0.0, 0.0, -1.0], 270.0, 30.0, &o, &tilted, 0.0);
    // The beam axis itself (30 degrees from the nadir towards north) reads gamma 0.
    let s30 = 30f64.to_radians().sin();
    let c30 = 30f64.to_radians().cos();
    check([0.0, s30, -c30], 90.0, 0.0, &o, &tilted, 0.0);
    // Measured at 10 degrees, installed at 30: a rotation of 20.
    let s20 = 20f64.to_radians().sin();
    let c20 = 20f64.to_radians().cos();
    check([0.0, s20, -c20], 90.0, 0.0, &o, &tilted, 10.0);
    // A bracket pointing east (0) with the same rules: north is then to the left, C180.
    let east = LuminaireInstance { bracket_azimuth_rad: 0.0, ..inst };
    check([0.0, s45, -c45], 180.0, 45.0, &o, &east, 0.0);
}

#[test]
fn vertical_and_semi_cylindrical_illuminance_hand_worked() {
    // Observer 6 m south of the luminaire line, at 1.5 m; the luminaire is 10 m up and due north.
    let data = single(0.0);
    let phot = isotropic(1000.0);
    let mut s = settings();
    s.vertical = Some(VerticalSettings { height_m: 1.5, facing: Facing::Bearings(vec![0.0, 90.0, 180.0]) });
    // Point at offset 3.5 + 6 = 9.5 (south), station 150.
    let r = at(&data, &phot, &s, 150.0, 9.5);
    let rho = 6.0;
    let d2: f64 = 36.0 + 8.5 * 8.5;
    let d3 = d2.powf(1.5);
    assert_eq!(r.vertical.len(), 3);
    // Facing north (towards the luminaire): E_v = I rho / d^3.
    assert!(close(r.vertical[0].ev_lx[0], 1000.0 * rho / d3, 1e-9));
    // Facing east: the luminaire is at right angles, E_v = 0. Facing south: 0.
    assert!(r.vertical[1].ev_lx[0] < 1e-9 && r.vertical[2].ev_lx[0] < 1e-9);
    // Semi-cylindrical: facing the source I 2 rho / (pi d^3); at right angles I rho / (pi d^3); away 0.
    assert!(close(r.vertical[0].esc_lx[0], 1000.0 * 2.0 * rho / (PI * d3), 1e-9));
    assert!(close(r.vertical[1].esc_lx[0], 1000.0 * rho / (PI * d3), 1e-9));
    assert!(r.vertical[2].esc_lx[0] < 1e-9);
    // Directly under the luminaire there is no vertical illuminance and no semi-cylindrical.
    let r = at(&data, &phot, &s, 150.0, 3.5);
    assert!(r.vertical.iter().all(|d| d.ev_lx[0] < 1e-9 && d.esc_lx[0] < 1e-9));
    // Along the alignment: both directions are produced, labelled.
    s.vertical = Some(VerticalSettings { height_m: 1.5, facing: Facing::AlongAlignment });
    let r = at(&data, &phot, &s, 140.0, 3.5);
    assert_eq!(r.vertical.len(), 2);
    assert_eq!(r.vertical[0].label, "towards up-station");
    assert!(close(r.vertical[0].bearing_deg, 90.0, 1e-9), "east");
    // The luminaire is 10 m up-station: facing up-station sees it, facing down-station does not.
    assert!(r.vertical[0].ev_lx[0] > 0.0 && r.vertical[1].ev_lx[0] < 1e-9);
}

#[test]
fn semi_cylindrical_definition_matches_numerical_integration_over_the_half_cylinder() {
    // Mean over the curved surface of an infinitesimal vertical half-cylinder facing n0 of the
    // illuminance from a point source: integrate numerically over the azimuth of the surface normal.
    let data = single(0.0);
    let phot = isotropic(1000.0);
    let (rho, hz) = (6.0, 8.5);
    let d = f64::sqrt(rho * rho + hz * hz);
    let i = 1000.0;
    // Source due north of the point; n0 at bearing b: delta = angle between n0 and north.
    for b in [0.0f64, 30.0, 90.0, 135.0, 180.0, 250.0] {
        let delta = b.to_radians();
        let n = 200_000;
        let mut sum = 0.0;
        for k in 0..n {
            // psi: azimuth of the surface normal within 90 degrees of n0, measured from north.
            let psi = delta - PI / 2.0 + PI * (k as f64 + 0.5) / n as f64;
            let cos_to_source = (rho / d) * (psi.cos()); // source at azimuth 0 (north)
            if cos_to_source > 0.0 {
                sum += i * cos_to_source / (d * d);
            }
        }
        let numeric = sum / n as f64;
        let mut s = settings();
        s.vertical = Some(VerticalSettings { height_m: 1.5, facing: Facing::Bearings(vec![b]) });
        let r = at(&data, &phot, &s, 150.0, 9.5);
        assert!(close(r.vertical[0].esc_lx[0], numeric, 1e-6), "bearing {b}: {} against {numeric}", r.vertical[0].esc_lx[0]);
    }
}

#[test]
fn stats_and_the_grid_over_an_area() {
    let st = Stats::of(&[2.0, 4.0, 6.0, 8.0]);
    assert!(close(st.average, 5.0, 1e-12) && st.minimum == 2.0 && st.maximum == 8.0);
    assert!(close(st.uniformity_min_avg, 0.4, 1e-12) && close(st.uniformity_min_max, 0.25, 1e-12));
    // Grid: cells of equal size, points at centres; spacing gives ceil(length / spacing) cells.
    let data = single(0.0);
    let phot = isotropic(1000.0);
    let mut s = settings();
    s.areas = vec![CalcArea {
        name: "A".to_string(),
        kind: AreaKind::Carriageway,
        station_from_m: 100.0,
        station_to_m: 110.0,
        offset_from_m: -2.0,
        offset_to_m: 1.0,
        along: Division::Spacing(3.0),
        across: Division::Count(2),
    }];
    let (frame, _) = GridFrame::from_layout(&data.layout).unwrap();
    let r = calculate(&phot, &data, &frame, &s, None).unwrap();
    let a = &r.areas[0];
    assert_eq!((a.n_along, a.n_across), (4, 2), "10 m at 3 m spacing: 4 cells of 2.5 m");
    assert!(close(a.points[0].station_m, 101.25, 1e-12) && close(a.points[0].offset_m, -1.25, 1e-12));
    assert!(close(a.points[7].station_m, 108.75, 1e-12) && close(a.points[7].offset_m, 0.25, 1e-12));
    // Heading east, right is south: offset -1.25 is 1.25 m north of the alignment (y = 2000).
    assert!(close(a.points[0].east_m, 1000.0 + 101.25, 1e-9) && close(a.points[0].north_m, 2001.25, 1e-9));
    let mean = a.e_h_lx.iter().sum::<f64>() / 8.0;
    assert!(close(a.e_h.average, mean, 1e-12) && a.e_h.minimum <= a.e_h.average && a.e_h.average <= a.e_h.maximum);
    // The straight frame and the alignment itself agree on a straight alignment.
    let alignment = straight();
    let r2 = calculate(&phot, &data, &GridFrame::Alignment(&alignment), &s, None).unwrap();
    for (x, y) in r.areas[0].e_h_lx.iter().zip(&r2.areas[0].e_h_lx) {
        assert!(close(*x, *y, 1e-9));
    }
    // Bad settings are refused with the reason.
    let mut bad = s.clone();
    bad.maintenance_factor = 1.2;
    assert!(matches!(calculate(&phot, &data, &frame, &bad, None), Err(CalcError::InvalidSettings(m)) if m.contains("maintenance")));
    bad = s.clone();
    bad.contribution_distance_m = 0.0;
    assert!(calculate(&phot, &data, &frame, &bad, None).is_err());
    bad = s.clone();
    bad.areas[0].offset_to_m = -5.0;
    assert!(matches!(calculate(&phot, &data, &frame, &bad, None), Err(CalcError::InvalidSettings(m)) if m.contains("offset")));
    bad = s.clone();
    bad.areas.clear();
    assert!(calculate(&phot, &data, &frame, &bad, None).is_err());
}

#[test]
fn a_curved_layout_is_flagged_for_the_straight_frame() {
    // 100 m straight, then a 150 m arc of radius 200 m (about 43 degrees), then 100 m straight.
    let a = AlignmentBuilder::new("C", 0.0, 0.0, 0.0, 0.0).line(100.0).arc(200.0, 150.0, afrobim_alignment::Turn::Left).line(100.0).build().unwrap();
    let def = installation(Arrangement::SingleRight, 0.0, a.end_station_m(), 25.0);
    let mut def = def;
    def.alignment = "C".to_string();
    let layout = layout_installation(&a, &def).unwrap();
    let (_, warning) = GridFrame::from_layout(&layout).unwrap();
    assert!(warning.is_some_and(|w| w.contains("turns by")));
    let (_, none) = GridFrame::from_layout(&layout_installation(&straight(), &installation(Arrangement::SingleRight, 0.0, 200.0, 25.0)).unwrap()).unwrap();
    assert!(none.is_none());
}

fn rtable_text(row_values: &str) -> String {
    format!("name: test\nunit: 1e-4\nbeta: 0 90 180\nrow: 0 {row_values}\nrow: 1 {row_values}\nrow: 4 {row_values}\n")
}

#[test]
fn luminance_from_an_r_table_with_hand_worked_beta() {
    let data = single(0.0);
    let phot = isotropic(1000.0);
    // r = 100, 200, 300 (x 1e-4 sr^-1) at beta 0, 90, 180.
    let table = RTable::parse(&rtable_text("100 200 300")).unwrap();
    let area = CalcArea { name: "C".to_string(), kind: AreaKind::Carriageway, ..cell(150.0, 3.5) };
    let run = |obs: ObserverPosition| {
        let mut s = settings();
        s.areas = vec![area.clone()];
        s.observers = vec![ObserverSpec { name: "O".to_string(), position: obs, height_m: 1.5 }];
        let (frame, _) = GridFrame::from_layout(&data.layout).unwrap();
        // The point under the luminaire has rho = 0: beta is taken as 0. Move the point 3 m east
        // (along the road) instead, so the luminaire is 3 m west of it, rho = 3, tan(gamma) = 0.3.
        s.areas[0] = CalcArea { name: "C".to_string(), kind: AreaKind::Carriageway, ..cell(153.0, 3.5) };
        calculate(&phot, &data, &frame, &s, Some(&table)).unwrap().areas.remove(0)
    };
    // Luminaire is west of the point (down-station). Observer far west: same direction, beta = 0.
    let r = run(ObserverPosition::Plan { east_m: -5000.0, north_m: 1996.5 });
    let rho_h = (9.0f64 + 100.0).sqrt();
    // tan(gamma) = 0.3: the rows are constant in tan(gamma), r = 100 x 1e-4 = 0.01. L = I r / h^2.
    let _ = rho_h;
    assert!(close(r.luminance[0].l_cd_m2[0], 1000.0 * 0.01 / 100.0, 1e-9), "beta 0: {}", r.luminance[0].l_cd_m2[0]);
    // Observer far east: opposite direction, beta = 180, r = 0.03.
    let r = run(ObserverPosition::Plan { east_m: 6000.0, north_m: 1996.5 });
    assert!(close(r.luminance[0].l_cd_m2[0], 1000.0 * 0.03 / 100.0, 1e-9), "beta 180: {}", r.luminance[0].l_cd_m2[0]);
    // Observer far south: at right angles, beta = 90, r = 0.02. Stats follow.
    let r = run(ObserverPosition::Plan { east_m: 1153.0, north_m: -5000.0 });
    assert!(close(r.luminance[0].l_cd_m2[0], 1000.0 * 0.02 / 100.0, 1e-9), "beta 90: {}", r.luminance[0].l_cd_m2[0]);
    assert!(close(r.luminance[0].stats.uniformity_min_avg, 1.0, 1e-12));
    // Q0 scaling: x 0.10 / 0.07.
    let scaled = RTable::parse(&format!("q0_table: 0.07\nq0_surface: 0.10\n{}", rtable_text("100 200 300"))).unwrap();
    assert!(close(scaled.r(90.0, 0.5).unwrap(), 0.02 * 0.10 / 0.07, 1e-12));
    // A station-and-offset observer resolves through the frame; luminance needs an r-table.
    let mut s = settings();
    s.areas = vec![area.clone()];
    s.observers = vec![ObserverSpec { name: "O".to_string(), position: ObserverPosition::StationOffset { station_m: -4000.0, offset_m: 3.5 }, height_m: 1.5 }];
    let (frame, _) = GridFrame::from_layout(&data.layout).unwrap();
    assert!(calculate(&phot, &data, &frame, &s, None).is_err());
    assert!(calculate(&phot, &data, &frame, &s, Some(&table)).is_ok());
}

#[test]
fn r_table_parsing_and_lookup() {
    let t = RTable::parse("# comment\nname: x\nunit: 1e-4\nbeta: 0 90 180\nrow: 0 100 200 300\nrow: 1 - 400 500 # trailing\n").unwrap();
    assert_eq!(t.missing_cells, 1);
    // Bilinear: half way in tan(gamma) between rows 0 and 1 at beta 90: (200 + 400) / 2 = 300.
    assert!(close(t.r(90.0, 0.5).unwrap() / t.unit, 300.0, 1e-12));
    // Half way in beta (45) at tan 0: (100 + 200) / 2.
    assert!(close(t.r(45.0, 0.0).unwrap() / t.unit, 150.0, 1e-12));
    assert!(t.r(0.0, 1.5).is_none(), "beyond the last row");
    for bad in [
        "beta: 0 90 180\nrow: 0 1 2 3\nrow: 1 1 2 3\n",
        "unit: 1e-4\nbeta: 0 90\nrow: 0 1 2\nrow: 1 1 2\n",
        "unit: 1e-4\nbeta: 0 90 180\nrow: 0 1 2\nrow: 1 1 2 3\n",
        "unit: 1e-4\nbeta: 0 90 180\nrow: 0.5 1 2 3\nrow: 1 1 2 3\n",
        "unit: 1e-4\nq0_table: 0.07\nbeta: 0 90 180\nrow: 0 1 2 3\nrow: 1 1 2 3\n",
        "unit: 1e-4\nbeta: 0 90 180\nrow: 0 1 2 3\n",
        "unit: 1e-4\nwhat: 3\n",
    ] {
        assert!(RTable::parse(bad).is_err(), "{bad}");
    }
}

#[test]
fn edge_illuminance_ratio_from_strips() {
    let a = straight();
    let def = installation(Arrangement::SingleRight, 0.0, 120.0, 30.0);
    let layout = layout_installation(&a, &def).unwrap();
    let data = LightingData { definition: LightingDefinition::Installation(def.clone()), layout };
    let phot = isotropic(1000.0);
    let carriageway = default_carriageway_area(&data, Division::Count(4), Division::Count(6)).unwrap();
    assert_eq!((carriageway.offset_from_m, carriageway.offset_to_m), (-3.5, 3.5), "7 m about the alignment");
    assert_eq!((carriageway.station_from_m, carriageway.station_to_m), (0.0, 120.0));
    let mut s = settings();
    s.areas = vec![carriageway];
    s.eir_lane_width_m = Some(3.5);
    let (frame, _) = GridFrame::from_layout(&data.layout).unwrap();
    let r = calculate(&phot, &data, &frame, &s, None).unwrap();
    assert_eq!(r.areas.len(), 5);
    let avg = |n: &str| r.area(n).unwrap().e_h.average;
    let eir = r.eir.as_ref().unwrap();
    assert!(close(eir.left, avg("EIR outer left") / avg("EIR inner left"), 1e-12));
    assert!(close(eir.right, avg("EIR outer right") / avg("EIR inner right"), 1e-12));
    assert!(close(eir.value, eir.left.min(eir.right), 1e-12));
    // Columns stand on the right (south): the right outer strip is brighter than the left one.
    assert!(eir.right > eir.left);
    // Strips are adjacent to the edges, each one lane wide.
    let strip = r.area("EIR outer left").unwrap();
    assert!(strip.points.iter().all(|p| p.offset_m < -3.5 && p.offset_m > -7.0));
    assert!(strip.kind == AreaKind::EdgeStrip);
    // Strips wider than half the carriageway are refused.
    s.eir_lane_width_m = Some(4.0);
    assert!(calculate(&phot, &data, &frame, &s, None).is_err());
    // A single column has no derived carriageway.
    assert!(default_carriageway_area(&single(0.0), Division::Count(2), Division::Count(2)).is_err());
}

fn linear_area() -> CalcResult {
    // 4 by 3 lattice, unit spacing, value = east coordinate (so iso-lines are vertical).
    let mut points = Vec::new();
    let mut values = Vec::new();
    for i in 0..4 {
        for j in 0..3 {
            points.push(GridPoint { i_along: i, i_across: j, station_m: i as f64, offset_m: j as f64, east_m: i as f64, north_m: j as f64 });
            values.push(i as f64);
        }
    }
    let e_h = Stats::of(&values);
    CalcResult {
        areas: vec![AreaResult { name: "L".to_string(), kind: AreaKind::Other, n_along: 4, n_across: 3, points, e_h_lx: values, e_h, vertical: vec![], luminance: vec![] }],
        eir: None,
        luminaire_count: 1,
        maintenance_factor: 1.0,
        contribution_distance_m: 10.0,
        plane_level_m: 0.0,
        warnings: vec![],
    }
}

#[test]
fn contours_values_and_csv_outputs() {
    let r = linear_area();
    let lum = vec![LuminaireInstance { mark: "a".to_string(), x_m: 1.0, y_m: 1.0, z_m: 8.0, bracket_azimuth_rad: 0.0, tilt_deg: 0.0 }];
    let d = contour_drawing(&r, "L", &[1.5, 2.25], &lum).unwrap();
    // Origin: 1 m margin before the lowest plan coordinate.
    assert_eq!((d.origin_x_m, d.origin_y_m), (-1.0, -1.0));
    let lines_at = |x_m: f64| {
        d.drawing
            .primitives
            .iter()
            .filter_map(|p| match p {
                Primitive::Line { from, to, width, .. } if *width == 60.0 => Some((from, to)),
                _ => None,
            })
            .filter(|(a, b)| (a.x - (x_m + 1.0) * 1000.0).abs() < 1e-6 && (b.x - (x_m + 1.0) * 1000.0).abs() < 1e-6)
            .count()
    };
    assert_eq!(lines_at(1.5), 2, "two cells high: two segments at x = 1.5");
    assert_eq!(lines_at(2.25), 2);
    assert_eq!(d.labels.iter().filter(|l| l.text.ends_with(" lx")).count(), 2);
    assert!(d.labels.iter().any(|l| l.text == "1.5 lx") && d.labels.iter().any(|l| l.text == "2.25 lx"));
    assert!(d.drawing.extents.max.x > d.drawing.extents.min.x);
    assert!(contour_drawing(&r, "nope", &[1.0], &lum).is_none());
    // A level outside the range draws nothing.
    let none = contour_drawing(&r, "L", &[10.0], &lum).unwrap();
    assert!(none.labels.is_empty());

    let v = grid_values_drawing(&r, "L", &lum).unwrap();
    assert_eq!(v.labels.len(), 12, "a label per point");
    assert!(v.labels.iter().any(|l| l.text == "3.0"));

    let csv = grid_to_csv(&r);
    let lines: Vec<&str> = csv.lines().collect();
    assert_eq!(lines[0], "area,kind,i_along,i_across,station_m,offset_m,easting_m,northing_m,e_h_lx");
    assert_eq!(lines.len(), 13);
    assert_eq!(lines[1], "L,other,0,0,0.000,0.000,0.000,0.000,0.000");
    assert_eq!(lines[12], "L,other,3,2,3.000,2.000,3.000,2.000,3.000");

    assert_eq!(nice_levels(0.0, 10.0, 5), vec![2.0, 4.0, 6.0, 8.0]);
    assert_eq!(nice_levels(3.2, 7.9, 4), vec![4.0, 6.0]);
    assert!(nice_levels(5.0, 5.0, 3).is_empty());
}

#[test]
fn a_saddle_cell_gives_two_segments_and_a_peak_a_closed_ring() {
    // Peak in the middle of a 3 by 3 lattice: iso-line round it, 4 segments in the 4 cells.
    let mut r = linear_area();
    let a = &mut r.areas[0];
    a.n_along = 3;
    a.n_across = 3;
    a.points.truncate(9);
    a.e_h_lx = vec![0.0, 0.0, 0.0, 0.0, 10.0, 0.0, 0.0, 0.0, 0.0];
    let segs = super::outputs::contour_segments(&r.areas[0], &r.areas[0].e_h_lx, 5.0);
    assert_eq!(segs.len(), 4);
    // Saddle: a and c high, b and d low in one cell: two segments.
    let mut r = linear_area();
    let a = &mut r.areas[0];
    a.n_along = 2;
    a.n_across = 2;
    a.points.truncate(4);
    // Lattice index order is along-major: (0,0) (0,1) (1,0) (1,1) -> corners a=(0,0) b=(0,1) c=(1,1) d=(1,0).
    a.e_h_lx = vec![10.0, 0.0, 0.0, 10.0];
    let segs = super::outputs::contour_segments(&r.areas[0], &r.areas[0].e_h_lx, 5.0);
    assert_eq!(segs.len(), 2);
}

#[test]
fn polar_diagram_of_isotropic_and_lambertian_sources() {
    let iso = polar_diagram(&isotropic(500.0), &[(0.0, 180.0), (90.0, 270.0)]);
    // Outer ring at 500 cd (the smallest 1, 2, 2.5, 5 x 10^k holding 500 is 500): the curves lie on it.
    let mut radii: Vec<f64> = Vec::new();
    for p in &iso.drawing.primitives {
        if let Primitive::Line { from, width, .. } = p {
            if *width == 12.0 {
                radii.push((from.x * from.x + from.y * from.y).sqrt());
            }
        }
    }
    assert_eq!(radii.len(), 4 * 180);
    assert!(radii.iter().all(|r| (r - 1000.0).abs() < 1e-6));
    let rings = iso.drawing.primitives.iter().filter(|p| matches!(p, Primitive::Arc { .. })).count();
    assert_eq!(rings, 4);
    assert!(iso.labels.iter().any(|l| l.text == "500 cd") && iso.labels.iter().any(|l| l.text == "125 cd"));
    assert!(iso.labels.iter().any(|l| l.text == "C0-C180") && iso.labels.iter().any(|l| l.text == "C90-C270"));
    // Lambertian 1000 cd: outer ring 1000, at gamma 60 the radius is 500, nadir at the bottom.
    let l = polar_diagram(&lambertian(1000.0), &[(0.0, 180.0)]);
    let at60 = l
        .drawing
        .primitives
        .iter()
        .filter_map(|p| match p {
            Primitive::Line { from, width, .. } if *width == 12.0 && from.x > 0.0 => Some(from),
            _ => None,
        })
        .find(|p| (p.y + 250.0).abs() < 1.0 && p.x > 0.0)
        .expect("a vertex at gamma 60");
    assert!(close((at60.x * at60.x + at60.y * at60.y).sqrt(), 500.0, 1e-6));
    assert!(at60.y < 0.0, "downward is below the centre");
}

#[test]
fn stored_settings_survive_a_refresh_and_resolve_with_warnings() {
    use afrobim_kernel::Document;
    let a = straight();
    let mut doc = Document::default();
    let id = crate::add_installation(&mut doc, &a, installation(Arrangement::SingleRight, 0.0, 120.0, 30.0)).unwrap();
    let data = crate::lighting_of(&doc, id).unwrap();
    let dir = std::env::temp_dir().join(format!("afrobim_phot_{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let path = dir.join("test.ies");
    let vert: Vec<f64> = (0..=12).map(|k| k as f64 * 15.0).collect();
    let text = ies_simple(&[0.0], &vert, &[vec![1000.0; 13]]);
    std::fs::write(&path, &text).unwrap();
    let loaded = crate::photometry::load_photometry_file(&path).unwrap();
    let mut s = settings();
    s.maintenance_factor = 0.8;
    s.areas = vec![default_carriageway_area(&data, Division::Count(4), Division::Count(3)).unwrap()];
    set_photometry(&mut doc, id, path.to_str().unwrap(), &loaded, &s, None).unwrap();
    let stored = photometry_settings_of(&doc, id).unwrap().unwrap();
    assert_eq!(stored.file_sha256, loaded.sha256);
    assert_eq!(stored.settings.maintenance_factor, 0.8);
    assert!(stored.distribution.is_some(), "small: embedded");
    // The key survives a refresh of the layout and a save and load of the document.
    crate::refresh_layout(&mut doc, id, &a, None).unwrap();
    assert!(photometry_settings_of(&doc, id).unwrap().is_some());
    let again = Document::from_json(&doc.to_json()).unwrap();
    assert_eq!(photometry_settings_of(&again, id).unwrap(), photometry_settings_of(&doc, id).unwrap());
    // The lighting definition still reads.
    assert!(crate::lighting_of(&doc, id).is_ok());

    // Repeat the stored run.
    let run = run_stored_photometry(&doc, id, None, None).unwrap();
    assert!(run.warnings.iter().all(|w| !w.contains("changed") && !w.contains("cannot be read")));
    assert_eq!(run.file_name, "test.ies");
    assert!(run.result.areas[0].e_h.average > 0.0);

    // File changed: the embedded copy is used and the warning says so.
    std::fs::write(&path, text.replace("1000", "2000")).unwrap();
    let run2 = run_stored_photometry(&doc, id, None, None).unwrap();
    assert!(run2.warnings.iter().any(|w| w.contains("has changed")));
    assert!(close(run2.result.areas[0].e_h.average, run.result.areas[0].e_h.average, 1e-12));
    // File missing, embedded copy: still runs, with a warning.
    std::fs::remove_file(&path).unwrap();
    let run3 = run_stored_photometry(&doc, id, None, None).unwrap();
    assert!(run3.warnings.iter().any(|w| w.contains("cannot be read")));
    // File missing and nothing embedded: an error that says so.
    let mut no_embed = stored.clone();
    no_embed.distribution = None;
    assert!(matches!(resolve_photometry(&no_embed, None), Err(CalcError::Photometry(m)) if m.contains("no copy")));
    // A relative path resolves from the base directory.
    std::fs::write(&path, &text).unwrap();
    let mut rel = stored.clone();
    rel.file_path = "test.ies".to_string();
    assert!(resolve_photometry(&rel, Some(&dir)).unwrap().2.is_empty());
    // A large distribution is referenced, not embedded.
    let mut big = loaded.clone();
    big.photometry.candela = vec![1.0; MAX_EMBEDDED_VALUES + 1];
    big.photometry.c_deg = vec![0.0];
    big.photometry.gamma_deg = (0..=MAX_EMBEDDED_VALUES).map(|k| k as f64 / 100.0).collect();
    set_photometry(&mut doc, id, "x.ies", &big, &s, Some(("r.txt", "00"))).unwrap();
    let st = photometry_settings_of(&doc, id).unwrap().unwrap();
    assert!(st.distribution.is_none());
    assert_eq!(st.rtable_path.as_deref(), Some("r.txt"));
    clear_photometry(&mut doc, id).unwrap();
    assert!(photometry_settings_of(&doc, id).unwrap().is_none());
    assert!(matches!(photometry_settings_of(&doc, afrobim_kernel::ElementId(999)), Err(crate::LightingError::ElementNotFound(_))));
    std::fs::remove_dir_all(&dir).ok();
}

#[test]
fn computed_results_fill_the_fields_and_feed_the_class_check() {
    use crate::design::{check_lighting, CheckInputs, Outcome, PhotometricResults};
    let a = straight();
    let mut def = installation(Arrangement::SingleRight, 0.0, 120.0, 30.0);
    def.lighting_class = "C2".to_string();
    let layout = layout_installation(&a, &def).unwrap();
    let data = LightingData { definition: LightingDefinition::Installation(def), layout };
    // 12 000 cd isotropic luminaires at 10 m every 30 m give tens of lux over the carriageway.
    let phot = isotropic(12000.0);
    let mut s = settings();
    s.areas = vec![default_carriageway_area(&data, Division::Count(8), Division::Count(6)).unwrap()];
    s.contribution_distance_m = 200.0;
    s.maintenance_factor = 0.8;
    let run = run_photometry(&data, &phot, "iso.ies", &"ab".repeat(32), &s, None, None).unwrap();
    assert_eq!(run.primary_area_for_class("C2"), Some("Carriageway"));
    let mut ph = PhotometricResults { ti_percent: Some(7.0), ..Default::default() };
    let filled = run.apply_to(&mut ph, "Carriageway").unwrap();
    assert!(filled.contains(&"e_av_lx") && filled.contains(&"u_o_e") && !filled.contains(&"l_av_cd_m2"));
    assert_eq!(ph.ti_percent, Some(7.0), "an entered value that was not computed is left alone");
    assert!(ph.source.starts_with(COMPUTED_SOURCE_PREFIX) && ph.source.contains("iso.ies") && ph.source.contains("abababababab") && ph.source.contains("0.8"));
    let area = &run.result.areas[0];
    assert_eq!(ph.e_av_lx, Some(area.e_h.average));
    assert_eq!(ph.u_o_e, Some(area.e_h.uniformity_min_avg));
    assert_eq!(ph.e_h_min_lx, Some(area.e_h.minimum));
    // The class check compares the computed values and does not call them external.
    let data_for_check = data.clone();
    let c = check_lighting(&data_for_check, &CheckInputs { photometry: ph.clone(), ..Default::default() });
    let line = c.lines.iter().find(|l| l.requirement.contains("Eav") || l.requirement.contains("average illuminance")).expect("an Eav line");
    assert!(matches!(line.outcome, Outcome::Ok | Outcome::Fails), "{line:?}");
    assert!(c.lines.iter().any(|l| l.finding.contains("Computed by AfroBIM")), "the source is quoted");
    assert!(!c.lines.iter().any(|l| l.finding.contains("Computed by AfroBIM") && l.finding.contains("calculated outside this crate")));
    // Luminance-based criteria of an M class stay input needed without an r-table.
    let mut m_data = data.clone();
    if let LightingDefinition::Installation(i) = &mut m_data.definition {
        i.lighting_class = "M3".to_string();
    }
    let c = check_lighting(&m_data, &CheckInputs { photometry: ph, ..Default::default() });
    let lav = c.lines.iter().find(|l| l.requirement.contains("Lav")).expect("an Lav line");
    assert_eq!(lav.outcome, Outcome::InputNeeded);
    // An unknown primary area is refused.
    assert!(run.apply_to(&mut PhotometricResults::default(), "Nope").is_err());
    // With an r-table and one observer the luminance fields are filled too.
    let table = RTable::parse(&rtable_text("100 100 100")).unwrap();
    let mut s2 = s.clone();
    s2.observers = vec![ObserverSpec { name: "O".to_string(), position: ObserverPosition::StationOffset { station_m: -60.0, offset_m: 0.0 }, height_m: 1.5 }];
    let run2 = run_photometry(&data, &phot, "iso.ies", &"ab".repeat(32), &s2, Some(&table), None).unwrap();
    let mut ph2 = PhotometricResults::default();
    let filled = run2.apply_to(&mut ph2, "Carriageway").unwrap();
    assert!(filled.contains(&"l_av_cd_m2") && filled.contains(&"u_o"));
    assert!(ph2.l_av_cd_m2.unwrap() > 0.0 && ph2.u_o.unwrap() > 0.0 && ph2.u_o.unwrap() <= 1.0);
}

#[test]
fn the_grid_rule_the_held_documents_give_is_only_for_car_parks() {
    assert_eq!(car_park_grid_limits(true), (0.5, 1.0, 36));
    assert_eq!(car_park_grid_limits(false), (1.0, 5.0, 36));
}

#[test]
fn orientation_changes_an_asymmetric_result() {
    // Light only on the C90 side: with C90 towards the carriageway the carriageway is lit; with C270
    // towards it, it is not (the luminaire throws behind the column).
    let vert = [0.0, 90.0];
    let rows = [vec![0.0, 0.0], vec![1000.0, 1000.0], vec![0.0, 0.0], vec![0.0, 0.0]];
    let phot = parse_ies(&ies_simple(&[0.0, 90.0, 180.0, 270.0], &vert, &rows), IesOptions::default()).unwrap();
    let data = single(0.0);
    let mut s = settings();
    s.orientation = Orientation { c_toward_carriageway_deg: 90.0, counterclockwise: true };
    let near = at(&data, &phot, &s, 150.0, 1.0);
    assert!(near.e_h_lx[0] > 1.0, "street side lit: {}", near.e_h_lx[0]);
    s.orientation.c_toward_carriageway_deg = 270.0;
    let flipped = at(&data, &phot, &s, 150.0, 1.0);
    assert!(flipped.e_h_lx[0] < 1e-9, "{}", flipped.e_h_lx[0]);
}
