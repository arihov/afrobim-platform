//! LandXML surface import.
//!
//! `tests/landxml/surface.xml` and `surface_feet.xml` were built by hand to the LandXML 1.2
//! schema and validate against LandXML-1.2.xsd (`xmllint --noout --schema LandXML-1.2.xsd`).
//! **Neither was exported by Civil 3D.** Their coordinates and levels are arbitrary test numbers.

use std::path::PathBuf;

use afrobim_corridor::{parse_landxml_surfaces, read_landxml_surfaces, SurfaceImport, SurfaceXmlError};

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
    assert!(notes.contains("Surface 'Gridded' was not imported: its surfType is 'grid'"), "{notes}");
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
