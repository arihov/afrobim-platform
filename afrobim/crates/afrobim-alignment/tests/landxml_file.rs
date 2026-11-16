//! Whole-file LandXML import: `parse_landxml_file` / `read_landxml_file`.
//!
//! `full_utm_north.xml` and `full_utm_south.xml` were written by hand to the LandXML 1.2 schema
//! (they validate against LandXML-1.2.xsd) with coordinates of true UTM size, so that a failure to
//! keep them exactly shows. `civil3d_sr15_export.xml` is a real Civil 3D 2023 export (see the README
//! for where it comes from); the others were not exported by Civil 3D.

use std::path::PathBuf;

use afrobim_alignment::{parse_landxml_file, read_landxml_file, LandXmlError, LandXmlGeomElement};

fn file(name: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests").join("landxml").join(name)
}

fn assert_close(got: f64, want: f64, tol: f64, what: &str) {
    assert!((got - want).abs() <= tol, "{what}: got {got}, want {want}");
}

#[test]
fn reads_the_header_units_coordinate_system_project_and_application() {
    let f = read_landxml_file(&file("full_utm_north.xml")).unwrap();
    assert_eq!(f.header.version.as_deref(), Some("1.2"));
    assert_eq!(f.header.date.as_deref(), Some("2026-11-16"));
    assert_eq!(f.header.namespace.as_deref(), Some("http://www.landxml.org/schema/LandXML-1.2"));
    assert!(f.units.declared && !f.units.imperial);
    assert_eq!(f.units.linear_unit, "meter");
    assert_eq!(f.units.area_unit.as_deref(), Some("hectare"));
    assert_eq!(f.units.angular_unit, "decimal degrees");
    assert_eq!(f.units.direction_unit, "decimal degrees");
    assert_eq!(f.units.elevation_unit, "meter");
    assert_eq!(f.units.lat_long_angular_unit, "decimal degrees");
    assert_eq!(f.units.metres_per_linear_unit, 1.0);
    assert_eq!(f.units.square_metres_per_area_unit, Some(10_000.0));

    let cs = f.coordinate_system.as_ref().unwrap();
    assert_eq!(cs.name.as_deref(), Some("WGS 84 / UTM zone 36N"));
    assert_eq!(cs.desc.as_deref(), Some("Kampala test"));
    assert_eq!(cs.epsg_code.as_deref(), Some("32636"));
    assert_eq!(cs.horizontal_datum.as_deref(), Some("WGS84"));
    assert_eq!(cs.vertical_datum.as_deref(), Some("EGM2008"));
    assert_eq!(cs.ellipsoid_name.as_deref(), Some("WGS84"));
    assert_eq!(cs.horizontal_coordinate_system_name.as_deref(), Some("UTM36N"));
    assert_eq!(cs.projected_coordinate_system_name.as_deref(), Some("WGS 84 / UTM zone 36N"));
    assert_eq!(cs.datum.as_deref(), Some("WGS84"));
    assert_eq!(cs.rotation_angle, Some(0.5));
    assert_eq!(cs.ogc_wkt_code, None);
    assert_eq!(cs.file_location, None);

    assert_eq!(f.project.as_ref().unwrap().name, "Test road");
    let app = &f.applications[0];
    assert_eq!((app.name.as_str(), app.manufacturer.as_deref(), app.version.as_deref()), ("hand-built", Some("AfroBIM"), Some("1")));
    assert_eq!(app.time_stamp.as_deref(), Some("2026-11-16T09:00:00"));
    assert!(!f.is_civil_3d());
}

#[test]
fn keeps_utm_coordinates_exactly_north_of_the_equator() {
    let f = read_landxml_file(&file("full_utm_north.xml")).unwrap();

    // Control points, with the nested group.
    assert_eq!(f.cg_points.len(), 1);
    let g = &f.cg_points[0];
    assert_eq!((g.name.as_deref(), g.desc.as_deref(), g.state.as_deref()), (Some("Control"), Some("control points"), Some("existing")));
    assert_eq!(g.points.len(), 2);
    assert_eq!(g.groups.len(), 1);
    assert_eq!(g.groups[0].name.as_deref(), Some("Benchmarks"));
    assert_eq!(g.groups[0].code.as_deref(), Some("BM"));
    let all = g.all_points();
    assert_eq!(all.len(), 3);
    let cp1 = all[0];
    assert_eq!((cp1.name.as_str(), cp1.code.as_deref(), cp1.desc.as_deref(), cp1.oid.as_deref()), ("CP1", Some("CTL"), Some("primary"), Some("1")));
    assert_eq!((cp1.northing_m, cp1.easting_m, cp1.elevation_m), (35012.3456, 450123.4567, Some(1150.1234)));
    let bm = all[2];
    assert_eq!((bm.northing_m, bm.easting_m, bm.elevation_m), (35050.0001, 450150.0001, None));
    assert_eq!(bm.point_geometry.as_deref(), Some("point"));

    // The alignment: X east, Y north, at the file's values.
    let a = &f.alignments[0];
    assert_eq!(a.name, "CL1");
    assert_eq!((a.elements[0].x, a.elements[0].y), (450000.5678, 35000.1234));
    assert_eq!(a.start_station_m, 1000.0);
    assert_eq!(a.elements.len(), 3);
    assert_close(a.length(), 100.0 + 200.0 * std::f64::consts::FRAC_PI_2 + 100.0, 1e-9, "length");

    // Existing ground profile: two runs (a gap), values untouched.
    assert_eq!(f.ground_profiles.len(), 1);
    let ps = &f.ground_profiles[0];
    assert_eq!((ps.alignment.as_str(), ps.profile.as_deref(), ps.name.as_str(), ps.state.as_deref()), ("CL1", Some("CL1 profiles"), "EG", Some("existing")));
    assert_eq!(ps.point_lists, vec![vec![[1000.0, 1150.25], [1100.0, 1151.5], [1200.0, 1153.75]], vec![[1300.0, 1154.0], [1514.159265, 1156.5]]]);
    // The design profile is still read with the alignment.
    assert_eq!(a.profile.as_ref().unwrap().name, "FG");

    // Cross-sections.
    assert_eq!(f.cross_sections.len(), 2);
    let xs = &f.cross_sections[0];
    assert_eq!((xs.alignment.as_str(), xs.station_m, xs.name.as_deref()), ("CL1", 1000.0, Some("XS1")));
    assert_eq!(xs.surfaces[0].name, "EG");
    assert_eq!(xs.surfaces[0].point_lists, vec![vec![[-20.0, 1150.5], [0.0, 1150.25], [20.0, 1150.75]]]);
    let d = &xs.design_surfaces[0];
    assert_eq!((d.name.as_deref(), d.side.as_deref(), d.material.as_deref(), d.closed_area), (Some("Pavement"), Some("both"), Some("asphalt"), Some(false)));
    assert_eq!(d.points.len(), 3);
    assert_eq!((d.points[2].offset_m, d.points[2].elevation_m), (3.5, 1150.1));
    assert_eq!(f.cross_sections[1].station_m, 1100.0);
    assert!(f.cross_sections[1].design_surfaces.is_empty());

    // Parcels: area in hectares converted, nested parcel, boundary with a curve.
    assert_eq!(f.parcels.len(), 1);
    let lot = &f.parcels[0];
    assert_eq!((lot.name.as_str(), lot.area, lot.area_m2), ("Lot 1", Some(0.25), Some(2500.0)));
    assert_eq!((lot.owner.as_deref(), lot.parcel_type.as_deref(), lot.class.as_deref(), lot.state.as_deref()), (Some("State"), Some("road"), Some("Single Lot"), Some("existing")));
    let c = lot.center.unwrap();
    assert_eq!((c.x, c.y, c.z), (450050.5, 35050.5, None));
    let geom = &lot.coord_geoms[0];
    assert_eq!(geom.elements.len(), 3);
    match &geom.elements[1] {
        LandXmlGeomElement::Curve { start, center, end, clockwise, radius_m, .. } => {
            assert_eq!((start.x, start.y), (450060.2222, 35040.1111));
            assert_eq!((center.x, center.y), (450060.2222, 35050.1111));
            assert_eq!((end.x, end.y), (450050.2222, 35050.1111));
            assert!(*clockwise);
            assert_eq!(*radius_m, Some(10.0));
        }
        other => panic!("{other:?}"),
    }
    assert_eq!(lot.parcels.len(), 1);
    assert_eq!((lot.parcels[0].name.as_str(), lot.parcels[0].area_m2), ("Lot 1A", Some(1000.0)));

    // Plan features: a line with elevations, an irregular line, a chain.
    assert_eq!(f.plan_features.len(), 2);
    let fence = &f.plan_features[0];
    assert_eq!((fence.name.as_deref(), fence.group.as_deref()), (Some("Fence"), Some("Features")));
    let els = &fence.coord_geoms[0].elements;
    match &els[0] {
        LandXmlGeomElement::Line { start, end } => {
            assert_eq!((start.x, start.y, start.z), (450001.0, 35001.0, Some(1150.0)));
            assert_eq!((end.x, end.y, end.z), (450051.0, 35001.0, Some(1151.0)));
        }
        other => panic!("{other:?}"),
    }
    match &els[1] {
        LandXmlGeomElement::IrregularLine { points, .. } => {
            assert_eq!(points.len(), 3);
            assert_eq!((points[1].x, points[1].y), (450061.0, 35006.0));
        }
        other => panic!("{other:?}"),
    }
    match &els[2] {
        LandXmlGeomElement::Chain { names, points } => {
            assert_eq!(names, &["CP1", "CP2"]);
            assert_eq!((points[1].x, points[1].y, points[1].z), (450223.4567, 35112.3456, Some(1152.5)));
        }
        other => panic!("{other:?}"),
    }
    assert_eq!(f.plan_features[1].coord_geoms[0].name.as_deref(), Some("PoleGeom"));
    // The same file read for its alignments only gives the same alignments.
    let only = afrobim_alignment::read_landxml(&file("full_utm_north.xml")).unwrap();
    assert_eq!(only.alignments, f.alignments);
}

#[test]
fn reads_a_surface_with_its_source_data() {
    let f = read_landxml_file(&file("full_utm_north.xml")).unwrap();
    let s = &f.surfaces[0];
    assert_eq!((s.name.as_str(), s.desc.as_deref(), s.state.as_deref(), s.surf_type.as_deref()), ("EG", Some("existing ground"), Some("existing"), Some("TIN")));
    assert_eq!(s.points.len(), 4);
    assert_eq!((s.points[2].id, s.points[2].northing_m, s.points[2].easting_m, s.points[2].elevation_m), (3, 35010.0001, 450010.0001, 1152.0));
    assert_eq!(s.faces.len(), 3);
    assert_eq!((s.faces[1].ids.clone(), s.faces[1].invisible), (vec![1, 3, 4], false));
    assert!(s.faces[2].invisible);

    let src = &s.source_data;
    assert_eq!(src.data_points.len(), 1);
    assert_eq!((src.data_points[0].name.as_deref(), src.data_points[0].code.as_deref()), (Some("spot heights"), Some("SPOT")));
    assert_eq!((src.data_points[0].points[1].x, src.data_points[0].points[1].y, src.data_points[0].points[1].z), (450010.2222, 35010.2222, Some(1151.0)));
    assert_eq!(src.breaklines.len(), 2);
    assert_eq!((src.breaklines[0].name.as_deref(), src.breaklines[0].brk_type.as_deref(), src.breaklines[0].state.as_deref()), (Some("ditch"), Some("standard"), Some("existing")));
    assert_eq!(src.breaklines[0].points[1].z, Some(1150.5));
    assert_eq!(src.breaklines[1].points[1].z, None);
    assert_eq!(src.contours.len(), 1);
    assert_eq!((src.contours[0].elevation_m, src.contours[0].points.len(), src.contours[0].points[1].y), (1150.0, 2, 35001.5));
    assert_eq!(src.boundaries.len(), 1);
    let b = &src.boundaries[0];
    assert_eq!((b.name.as_deref(), b.bnd_type.as_str(), b.edge_trim, b.area, b.points.len()), (Some("limit"), "outer", false, Some(100.0), 5));
}

#[test]
fn summarises_counts_and_extents_per_class_and_counts_what_it_skips() {
    let f = read_landxml_file(&file("full_utm_north.xml")).unwrap();
    let s = &f.summary;
    assert_eq!((s.alignments.count, s.cg_points.count, s.parcels.count, s.plan_features.count, s.surfaces.count), (1, 3, 2, 2, 1));
    assert_eq!((s.ground_profiles, s.cross_sections), (1, 2));
    let counts: Vec<(&str, usize)> = s.counts.iter().map(|(k, v)| (k.as_str(), *v)).collect();
    assert_eq!(
        counts,
        [
            ("boundaries", 1),
            ("breaklines", 2),
            ("cg_point_groups", 2),
            ("chains", 1),
            ("contours", 1),
            ("data_point_sets", 1),
            ("irregular_lines", 1),
            ("surface_faces", 3),
            ("surface_points", 4)
        ]
    );

    let e = s.cg_points.extent.unwrap();
    assert_eq!((e.min_easting, e.max_easting, e.min_northing, e.max_northing), (450123.4567, 450223.4567, 35012.3456, 35112.3456));
    let e = s.parcels.extent.unwrap();
    assert_eq!((e.min_easting, e.max_easting, e.min_northing, e.max_northing), (450040.2222, 450060.2222, 35040.1111, 35050.5));
    let e = s.plan_features.extent.unwrap();
    assert_eq!((e.min_easting, e.max_easting, e.min_northing, e.max_northing), (450001.0, 450223.4567, 35001.0, 35112.3456));
    let e = s.surfaces.extent.unwrap();
    assert_eq!((e.min_easting, e.max_easting, e.min_northing, e.max_northing), (450000.0, 450100.0, 35000.0, 35010.2222));
    // The alignment is sampled along its elements: the arc's extent is that of its circle's quarter.
    let e = s.alignments.extent.unwrap();
    assert_close(e.min_easting, 450000.5678, 1e-9, "alignment min easting");
    assert_close(e.max_easting, 450300.5678, 1e-3, "alignment max easting");
    assert_close(e.min_northing, 35000.1234, 1e-9, "alignment min northing");
    assert_close(e.max_northing, 35300.1234, 1e-3, "alignment max northing");
    let all = s.overall_extent().unwrap();
    assert_eq!((all.min_easting, all.min_northing), (450000.0, 35000.0));
    assert_close(all.max_easting, 450300.5678, 1e-3, "overall max easting");

    // Skipped elements, counted by path and named in the warnings.
    let skipped: Vec<(&str, usize)> = s.skipped.iter().map(|(k, v)| (k.as_str(), *v)).collect();
    assert_eq!(skipped, [("LandXML/Alignments/Alignment/Feature", 1), ("LandXML/Roadways", 1)]);
    assert!(f.warnings.contains(&"1 element at LandXML/Roadways was not read.".to_string()), "{:#?}", f.warnings);
    assert!(f.warnings.contains(&"1 element at LandXML/Alignments/Alignment/Feature was not read.".to_string()));
}

#[test]
fn keeps_utm_coordinates_exactly_south_of_the_equator() {
    let f = read_landxml_file(&file("full_utm_south.xml")).unwrap();
    assert_eq!(f.coordinate_system.as_ref().unwrap().epsg_code.as_deref(), Some("32736"));
    let p = &f.cg_points[0].points[0];
    assert_eq!((p.northing_m, p.easting_m, p.elevation_m), (9960012.3456, 450123.4567, Some(1150.1234)));
    let a = &f.alignments[0];
    assert_eq!((a.elements[0].x, a.elements[0].y), (450123.4567, 9960012.3456));
    let s = &f.surfaces[0];
    assert_eq!((s.points[0].northing_m, s.points[0].easting_m), (9960000.0001, 450000.0001));
    let e = f.summary.surfaces.extent.unwrap();
    assert_eq!((e.min_northing, e.max_northing, e.min_easting, e.max_easting), (9960000.0001, 9960010.0001, 450000.0001, 450010.0001));
    let e = f.summary.overall_extent().unwrap();
    assert_eq!(e.min_northing, 9960000.0001);
    assert_close(e.max_northing, 9960112.3456, 1e-9, "max northing");
    // No Feature, no unknown element, and a file of this kind has nothing to say about "no alignments".
    assert!(f.summary.skipped.is_empty());
    assert!(f.warnings.iter().all(|w| !w.contains("no alignments")), "{:#?}", f.warnings);
}

#[test]
fn reads_a_real_civil_3d_export_in_us_survey_feet_at_its_true_coordinates() {
    let f = read_landxml_file(&file("civil3d_sr15_export.xml")).unwrap();
    assert!(f.is_civil_3d());
    let app = &f.applications[0];
    assert_eq!((app.name.as_str(), app.manufacturer.as_deref(), app.version.as_deref()), ("Autodesk Civil 3D", Some("Autodesk, Inc."), Some("2023")));
    assert!(f.units.imperial);
    assert_eq!(f.units.linear_unit, "USSurveyFoot");
    assert_eq!(f.units.area_unit.as_deref(), Some("squareFoot"));
    assert_eq!(f.units.angular_unit, "decimal degrees");
    let ft = 1200.0 / 3937.0;
    assert_eq!(f.units.metres_per_linear_unit, ft);

    let cs = f.coordinate_system.as_ref().unwrap();
    assert_eq!(cs.epsg_code.as_deref(), Some("2239"));
    assert_eq!(cs.horizontal_datum.as_deref(), Some("NAD83"));
    assert_eq!(cs.horizontal_coordinate_system_name.as_deref(), Some("GA83-EF"));
    assert_eq!(cs.desc.as_deref(), Some("NAD83 Georgia State Planes, East Zone, US Foot"));
    assert!(cs.ogc_wkt_code.as_deref().unwrap().starts_with("PROJCS[\"GA83-EF\""));

    // The first Line of the alignment: "northing easting", in feet, converted by the unit only.
    let a = &f.alignments[0];
    assert_eq!(a.name, "DE1_ALIGN");
    assert_eq!((a.elements[0].x, a.elements[0].y), (462742.720198000025 * ft, 1058088.529542000033 * ft));
    assert_eq!(a.start_station_m, 10000.0 * ft);
    assert_close(a.length(), 23470.158551798326 * ft, 0.05, "length");

    // Existing ground profile and the design profile.
    assert_eq!(f.ground_profiles.len(), 1);
    let ps = &f.ground_profiles[0];
    assert_eq!((ps.alignment.as_str(), ps.name.as_str(), ps.state.as_deref()), ("DE1_ALIGN", "Surface - GDOT - Surface (1)", Some("existing")));
    assert_eq!(ps.point_lists.len(), 1);
    assert_eq!(ps.point_lists[0][0], [10000.0 * ft, 0.0]);
    assert!(ps.point_lists[0].len() > 10);
    assert_eq!(a.profile.as_ref().unwrap().name, "DE1_PROF");

    // Plan features with elevations.
    assert_eq!(f.plan_features.len(), 14);
    assert_eq!(f.plan_features[0].group.as_deref(), Some("Siteless FeatureLines"));
    match &f.plan_features[0].coord_geoms[0].elements[0] {
        LandXmlGeomElement::Line { start, .. } => {
            assert_eq!((start.x, start.y, start.z), (463473.435035537463 * ft, 1062815.533963634633 * ft, Some(504.323294635862 * ft)));
        }
        other => panic!("{other:?}"),
    }
    // Civil 3D's Feature/Property elements are skipped and counted, not an error.
    assert!(f.summary.skipped.keys().any(|k| k.ends_with("/Feature")), "{:?}", f.summary.skipped);
    assert!(f.summary.alignments.extent.is_some() && f.summary.plan_features.extent.is_some());
    let e = f.summary.plan_features.extent.unwrap();
    assert!(e.min_easting > 450_000.0 * ft && e.max_northing > 1_000_000.0 * ft);
}

#[test]
fn unknown_and_unreadable_content_is_warned_about_never_an_error() {
    let xml = r#"<LandXML version="1.2"><Units><Metric linearUnit="meter" areaUnit="squareMeter"/></Units>
        <Mystery><Inner/></Mystery><Mystery/>
        <CgPoints><CgPoint name="OK">10.0 20.0 30.0</CgPoint><CgPoint name="BAD">ten twenty</CgPoint><Extra/></CgPoints>
        <Surfaces><Surface name="S"><Definition surfType="grid"><Pnts><P id="1">1 2 3</P><P>1 2 3</P></Pnts><Faces><F>1 1 1 1</F><F>1 2</F></Faces></Definition></Surface></Surfaces>
        <Parcels><Parcel name="P" area="12"><CoordGeom><Line><Start>1 2</Start></Line></CoordGeom></Parcel></Parcels>
        </LandXML>"#;
    let f = parse_landxml_file(xml).unwrap();
    assert_eq!(f.cg_points[0].points.len(), 1);
    assert_eq!(f.surfaces[0].surf_type.as_deref(), Some("grid"));
    assert_eq!((f.surfaces[0].points.len(), f.surfaces[0].faces.len()), (1, 1));
    assert_eq!(f.surfaces[0].faces[0].ids.len(), 4);
    assert!(f.parcels[0].coord_geoms[0].elements.is_empty());
    assert_eq!(f.parcels[0].area_m2, Some(12.0));
    let w = f.warnings.join("\n");
    assert!(w.contains("2 elements at LandXML/Mystery were not read."), "{w}");
    assert!(w.contains("1 element at LandXML/CgPoints/Extra was not read."), "{w}");
    assert!(w.contains("CgPoint 'BAD' has no readable coordinates"), "{w}");
    assert!(w.contains("1 point(s) were left out"), "{w}");
    assert!(w.contains("1 face(s) were left out"), "{w}");
    assert!(w.contains("CoordGeom, element 1 (Line): it has no <End>."), "{w}");
    assert_eq!(f.summary.skipped.get("LandXML/Mystery"), Some(&2));
}

#[test]
fn elevation_unit_that_differs_from_the_linear_unit_is_reported_and_not_applied() {
    let xml = r#"<LandXML><Units><Metric linearUnit="meter" areaUnit="squareMeter" elevationUnit="feet"/></Units>
        <CgPoints><CgPoint name="A">1.0 2.0 100.0</CgPoint></CgPoints></LandXML>"#;
    let f = parse_landxml_file(xml).unwrap();
    assert_eq!(f.units.elevation_unit, "feet");
    assert_eq!(f.cg_points[0].points[0].elevation_m, Some(100.0));
    assert!(f.warnings.iter().any(|w| w.contains("elevationUnit is 'feet'")), "{:?}", f.warnings);
}

#[test]
fn rejects_what_is_not_landxml() {
    assert!(matches!(parse_landxml_file("<nope"), Err(LandXmlError::Xml(_))));
    assert!(matches!(parse_landxml_file("<Drawing/>"), Err(LandXmlError::NotLandXml(n)) if n == "Drawing"));
    assert!(matches!(parse_landxml_file("<LandXML><Units><Imperial linearUnit=\"furlong\"/></Units></LandXML>"), Err(LandXmlError::Unit(_))));
    assert!(matches!(read_landxml_file(&file("missing.xml")), Err(LandXmlError::Io(_))));
}
