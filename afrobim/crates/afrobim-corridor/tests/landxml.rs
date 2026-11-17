//! LandXML surface import.
//!
//! `tests/landxml/surface.xml` and `surface_feet.xml` were built by hand to the LandXML 1.2
//! schema and validate against LandXML-1.2.xsd (`xmllint --noout --schema LandXML-1.2.xsd`).
//! **Neither was exported by Civil 3D.** Their coordinates and levels are arbitrary test numbers.

use std::path::PathBuf;

use afrobim_corridor::{parse_landxml_surfaces, parse_landxml_surfaces_with, read_landxml_surfaces, SurfaceImport, SurfaceReadOptions, SurfaceXmlError};

fn import(name: &str) -> SurfaceImport {
    read_landxml_surfaces(&PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests").join("landxml").join(name)).unwrap()
}

fn close(got: f64, want: f64, what: &str) {
    assert!((got - want).abs() <= 1e-9, "{what}: got {got}, want {want}");
}

#[test]
fn a_tin_surface_is_read_northing_first() {
    let file = import("surface.xml");
    assert_eq!(file.surfaces.len(), 1, "{:?}", file.notes);
    let eg = &file.surfaces[0];
    assert_eq!(eg.name, "EG");
    assert_eq!((eg.tin.points().len(), eg.tin.triangles().len()), (6, 4));
    assert_eq!(eg.invisible_faces, 1);

    // P 2 is "1000 5020 1201": northing 1000, easting 5020. So X = 5020 and Y = 1000.
    assert_eq!(eg.tin.points()[1], [5020.0, 1000.0, 1201.0]);
    // P 5 has no text and takes the CgPoint PEAK.
    assert_eq!(eg.tin.points()[4], [5010.0, 1010.0, 1205.0]);
    let b = eg.tin.bounds();
    assert_eq!((b.min_x, b.max_x, b.min_y, b.max_y), (5000.0, 5020.0, 1000.0, 1020.0));
    close(eg.tin.plan_area(), 400.0, "the 20 m square; the invisible triangle is not part of it");

    close(eg.tin.elevation_at(5010.0, 1010.0).unwrap(), 1205.0, "the peak");
    // Halfway from the middle of the south edge (1200.5) to the peak (1205).
    close(eg.tin.elevation_at(5010.0, 1005.0).unwrap(), 1202.75, "south triangle");
    // Halfway from the middle of the east edge (1202) to the peak.
    close(eg.tin.elevation_at(5015.0, 1010.0).unwrap(), 1203.5, "east triangle");
    // Inside the face marked i="1".
    assert_eq!(eg.tin.elevation_at(5030.0, 1010.0), None);
    // Had easting and northing been swapped this point would be on the surface.
    assert_eq!(eg.tin.elevation_at(1010.0, 5010.0), None);

    let notes = file.notes.join("\n");
    assert!(notes.contains("Surface 'EG': 1 face(s) marked invisible"), "{notes}");
    assert!(notes.contains("Surface 'EG': 1 face(s) were left out because they name a point that is not among the surface's points"), "{notes}");
    // The grid is left out by default, and read as two triangles per cell on request.
    assert!(notes.contains("Surface 'Gridded' was not imported: its surfType is 'grid'"), "{notes}");
    let text = std::fs::read_to_string(PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests").join("landxml").join("surface.xml")).unwrap();
    let with_grid = parse_landxml_surfaces_with(&text, &SurfaceReadOptions { grid_as_tin: true }).unwrap();
    assert!(with_grid.notes.join("\n").contains("Surface 'Gridded' is a grid: each four-point cell was split into two triangles"));
    let grid = &with_grid.surfaces[1];
    assert_eq!((grid.name.as_str(), grid.surf_type.as_str()), ("Gridded", "grid"));
    assert_eq!((grid.tin.points().len(), grid.tin.triangles().len()), (4, 2));
    assert_eq!(grid.tin.points()[2], [10.0, 10.0, 10.0]);
    close(grid.tin.plan_area(), 100.0, "one 10 m cell");
    assert_eq!(eg.surf_type, "TIN");
    assert_eq!((eg.description.as_deref(), eg.state.as_deref()), (Some("existing ground"), None));
    assert!(eg.source_data.is_empty());
}

#[test]
fn feet_are_converted_for_all_three_coordinates() {
    let file = import("surface_feet.xml");
    assert!(file.notes.is_empty(), "{:?}", file.notes);
    let tin = &file.surfaces[0].tin;
    // P 30 is northing 500 ft, easting 1000 ft, elevation 110 ft.
    let p = tin.points()[2];
    close(p[0], 304.8, "easting");
    close(p[1], 152.4, "northing");
    close(p[2], 33.528, "elevation");
    // The ground rises 10 ft over 500 ft of northing: 2 %, whatever the unit.
    close(tin.elevation_at(100.0, 76.2).unwrap(), 105.0 * 0.3048, "halfway up");
}

#[test]
fn files_that_are_not_surfaces() {
    assert!(matches!(parse_landxml_surfaces("<LandXML"), Err(SurfaceXmlError::Xml(_))));
    assert!(matches!(parse_landxml_surfaces("<Other/>"), Err(SurfaceXmlError::NotLandXml(n)) if n == "Other"));
    assert!(matches!(
        parse_landxml_surfaces(r#"<LandXML><Units><Metric linearUnit="furlong"/></Units></LandXML>"#),
        Err(SurfaceXmlError::Unit(u)) if u == "furlong"
    ));
    let none = parse_landxml_surfaces(r#"<LandXML><Units><Metric linearUnit="meter"/></Units></LandXML>"#).unwrap();
    assert_eq!(none.notes, vec!["The file contains no surfaces.".to_string()]);

    // No unit: metres, with a note. Bad points and faces are counted; nothing usable is left.
    let poor = parse_landxml_surfaces(
        r#"<LandXML><Surfaces><Surface name="S"><Definition surfType="TIN">
             <Pnts><P id="1">0 0 1</P><P id="2">0 10</P><P id="3">10 10 1</P><P>5 5 1</P></Pnts>
             <Faces><F>1 2 3</F><F>1 3</F><F>1 1 3</F></Faces>
           </Definition></Surface><Surface name="T"/></Surfaces></LandXML>"#,
    )
    .unwrap();
    assert!(poor.surfaces.is_empty());
    let notes = poor.notes.join("\n");
    assert!(notes.contains("metres assumed"), "{notes}");
    assert!(notes.contains("2 point(s) were left out"), "{notes}");
    assert!(notes.contains("1 face(s) were left out because they do not have exactly three point ids"), "{notes}");
    assert!(notes.contains("1 face(s) were left out because they name a point"), "{notes}");
    assert!(notes.contains("1 face(s) were left out because they have no area in plan"), "{notes}");
    assert!(notes.contains("Surface 'S' was not imported: a surface needs at least one triangle."), "{notes}");
    assert!(notes.contains("Surface 'T' was not imported: it has no Definition"), "{notes}");
}

#[test]
fn utm_sized_coordinates_and_source_data_survive_exactly() {
    // Easting about 450 000 m, northing about 9 960 000 m (south of the equator), four decimals.
    let file = import("surface_utm_source_data.xml");
    assert_eq!(file.surfaces.len(), 1, "{:?}", file.notes);
    let s = &file.surfaces[0];
    assert_eq!((s.name.as_str(), s.description.as_deref(), s.state.as_deref(), s.surf_type.as_str()), ("EG", Some("existing ground"), Some("existing"), "TIN"));
    assert_eq!(s.tin.points()[0], [450000.1234, 9960000.5678, 1150.25]);
    assert_eq!(s.tin.points()[2], [450010.1234, 9960010.5678, 1152.5]);
    assert_eq!(s.tin.triangles().len(), 2);
    let b = s.tin.bounds();
    assert_eq!((b.min_x, b.max_x, b.min_y, b.max_y), (450000.1234, 450010.1234, 9960000.5678, 9960010.5678));

    let src = &s.source_data;
    assert_eq!((src.breaklines.len(), src.contours.len(), src.boundaries.len(), src.data_points.len()), (1, 1, 1, 1));
    let bl = &src.breaklines[0];
    assert_eq!((bl.name.as_deref(), bl.brk_type.as_deref()), (Some("ridge"), Some("standard")));
    assert_eq!((bl.points[0].x, bl.points[0].y, bl.points[0].z), (450000.1234, 9960000.5678, Some(1150.25)));
    assert_eq!((src.contours[0].elevation_m, src.contours[0].points[1].x, src.contours[0].points[1].y), (1151.0, 450005.1234, 9960005.5678));
    assert_eq!((src.boundaries[0].bnd_type.as_str(), src.boundaries[0].edge_trim, src.boundaries[0].points.len()), ("outer", true, 4));
    assert_eq!((src.data_points[0].points[0].x, src.data_points[0].points[0].y, src.data_points[0].points[0].z), (450000.1234, 9960000.5678, Some(1150.25)));
    assert!(file.notes.is_empty(), "{:?}", file.notes);
}

/// The synthetic fixture with Civil 3D's structure (`afrobim-alignment/tests/landxml/civil3d_like.xml`,
/// made-up numbers): a TIN whose hull faces are invisible, a stray point, `PointFiles`, and the
/// statistics `Definition` states.
#[test]
fn a_civil_3d_style_surface_keeps_its_statistics_source_files_and_stray_point() {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("..").join("afrobim-alignment").join("tests").join("landxml").join("civil3d_like.xml");
    let file = read_landxml_surfaces(&path).unwrap();
    assert_eq!(file.surfaces.len(), 1, "{:?}", file.notes);
    let s = &file.surfaces[0];
    assert_eq!((s.name.as_str(), s.surf_type.as_str()), ("Test OGL", "TIN"));
    assert_eq!((s.tin.points().len(), s.tin.triangles().len(), s.invisible_faces), (13, 12, 2));
    // The areas Definition states are those of the visible faces: the TIN has the same plan area.
    let st = s.stated.unwrap();
    assert!((s.tin.plan_area() - st.area_2d_m2.unwrap()).abs() < 1e-6, "{} against {:?}", s.tin.plan_area(), st);
    assert_eq!((st.elev_min_m, st.elev_max_m), (Some(1183.25), Some(1204.0)));
    // The source files are named, not skipped.
    assert_eq!(s.source_data.point_files.len(), 1);
    assert_eq!(s.source_data.point_files[0].file_format, "xyz");
    // The stray point is in the TIN as written, named in the notes and in outlier_points.
    assert_eq!(s.outlier_points, [12]);
    assert_eq!(s.tin.points()[12], [312.25, 10766.5, 1183.25]);
    assert!(file.notes.iter().any(|n| n.contains("Surface 'Test OGL': 1 of 13 points lie more than 100 times the median distance")), "{:?}", file.notes);
    assert!(file.notes.iter().any(|n| n.contains("2 face(s) marked invisible")), "{:?}", file.notes);
}

/// A private LandXML file (not in this repository): skipped unless `AFROBIM_PRIVATE_LANDXML` names it.
#[test]
fn the_surfaces_of_the_private_file_are_read_whole() {
    let Ok(p) = std::env::var("AFROBIM_PRIVATE_LANDXML") else {
        println!("AFROBIM_PRIVATE_LANDXML is not set: private test skipped.");
        return;
    };
    let path = PathBuf::from(&p);
    if !path.is_file() {
        println!("AFROBIM_PRIVATE_LANDXML is set to '{p}', which is not a file: private test skipped.");
        return;
    }
    let t = std::time::Instant::now();
    let file = read_landxml_surfaces(&path).unwrap();
    println!("read_landxml_surfaces: {:?}", t.elapsed());
    let t = std::time::Instant::now();
    let text = std::fs::read_to_string(&path).unwrap();
    let again = parse_landxml_surfaces_with(&text, &SurfaceReadOptions { grid_as_tin: true }).unwrap();
    println!("parse_landxml_surfaces_with: {:?}", t.elapsed());
    assert_eq!(file.surfaces.len(), 4);
    assert_eq!(again.surfaces.len(), 4);
    let (mut points, mut triangles, mut invisible) = (0, 0, 0);
    for (s, a) in file.surfaces.iter().zip(&again.surfaces) {
        assert_eq!(s.tin, a.tin);
        assert_eq!(s.surf_type, "TIN");
        points += s.tin.points().len();
        triangles += s.tin.triangles().len();
        invisible += s.invisible_faces;
        // Every visible face is a triangle of the TIN, the invisible ones are not; the plan area is what the file states.
        let st = s.stated.expect("the file states the statistics");
        let area = s.tin.plan_area();
        println!("surface with {} points: {} triangles, {} invisible faces, plan area {area:.6} m2, stated {:.6} m2, {} outlier points, {} source file(s)", s.tin.points().len(), s.tin.triangles().len(), s.invisible_faces, st.area_2d_m2.unwrap(), s.outlier_points.len(), s.source_data.point_files.len());
        assert!((area - st.area_2d_m2.unwrap()).abs() < 1e-3, "{area} against {:?}", st);
        assert_eq!(s.source_data.point_files.len(), 1);
    }
    assert_eq!((points, triangles + invisible), (20_576, 40_915));
    assert_eq!(invisible, 581);
    assert_eq!(file.surfaces.iter().filter(|s| !s.outlier_points.is_empty()).count(), 1);
    // The invisible faces are counted in the notes, surface by surface.
    assert_eq!(file.notes.iter().filter(|n| n.contains("marked invisible")).count(), 4);
}
