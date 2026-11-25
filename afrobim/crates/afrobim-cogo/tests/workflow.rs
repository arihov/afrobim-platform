// SPDX-License-Identifier: LicenseRef-AfroBIM-Proprietary
// Copyright (c) 2026 AfroBIM. All rights reserved.
use afrobim_cogo::*;

const FILE: &str = "1,500000.1234,9999999.1234,12.345,EP 3.5\n2,500010.0001,9999990.0002,12.5,TREE oak\n3,500020.5,9999980.25,13.0,CL\n4,500030,9999970,,BM\n";

#[test]
fn file_to_keys_groups_and_back() {
    let d = detect_format(FILE, 3);
    let fmt = d.best.clone().unwrap();
    assert_eq!(fmt.order, ColumnOrder::Penzd);
    let parsed = parse_points(FILE, &fmt, Some("site.csv")).unwrap();
    let mut store = PointStore::new();
    import_into(&mut store, parsed.points, DuplicatePolicy::Reject).unwrap();

    let mut keys = DescriptionKeySet::new("road");
    keys.keys.push(DescriptionKey { code: "EP $1".into(), full_description: "Edge of pavement, {$1}".into(), layer: "C-EP".into(), ..Default::default() });
    keys.keys.push(DescriptionKey { code: "TR*".into(), full_description: "Tree $*".into(), layer: "V-TREE".into(), ..Default::default() });
    let rep = keys.apply(&mut store);
    assert_eq!(rep.matched, vec![1, 2]);
    assert_eq!(store.get(1).unwrap().full_description, "Edge of pavement, {3.5}");

    let mut groups = PointGroupSet::new();
    let mut g = PointGroup::new("surface");
    g.include_all = true;
    g.surface_points = true;
    g.exclude.push(Rule::RawDescription { pattern: "TREE*".into() });
    groups.add(g).unwrap();
    let s: Vec<u32> = groups.surface_points(&store, "surface").unwrap().iter().map(|p| p.number).collect();
    assert_eq!(s, vec![1, 3]); // 2 is a tree, 4 has no elevation

    // coordinates are untouched by all of the above, and survive export and re-import to 0.0001 m
    let o = ExportOptions { order: ColumnOrder::Penzd, delimiter: Delimiter::Comma, decimals: 4, missing_elevation: Some("".into()), header: false };
    let text = export_points(store.iter(), &o, false).unwrap();
    let again = parse_points(&text, &fmt, None).unwrap();
    for (a, b) in store.iter().zip(&again.points) {
        assert!((a.northing - b.northing).abs() < 1e-4 && (a.easting - b.easting).abs() < 1e-4);
    }
    assert_eq!(store.get(1).unwrap().northing, 9999999.1234);
    assert!(store.log().is_empty());
}

#[test]
fn reserve_take_to_schedule_json() {
    let b = Boundary::polygon(&[Xy::new(500000.0, 9999900.0), Xy::new(500100.0, 9999900.0), Xy::new(500100.0, 9999950.0), Xy::new(500000.0, 9999950.0)]).unwrap();
    let mut p = Parcel::new("Plot 1", 1, b);
    p.status = ParcelStatus::ToBeAcquired;
    let cl = [Xy::new(500060.0, 9999890.0), Xy::new(500060.0, 9999960.0)];
    let edge = offset_polyline(&cl, 10.0, 4.0).unwrap(); // west edge at x = 500050 when going north
    let rows = acquisition_schedule(&[p], &edge, Side::Right, &SplitOptions { snap_tol_m: 1e-6, arc_tol_m: 1e-4 }).unwrap();
    // offset to the left of a north-going line is west; the right side of the edge is east: 50 m x 50 m taken
    assert!((rows[0].take_area_m2 - 2500.0).abs() < 1e-6, "{rows:?}");
    let j = serde_json::to_string(&rows).unwrap();
    let back: Vec<AcquisitionRow> = serde_json::from_str(&j).unwrap();
    assert_eq!(rows, back);
}
