//! LandXML import.
//!
//! The files in `tests/landxml/` were built by hand to the LandXML 1.2 schema (they validate
//! against LandXML-1.2.xsd); none was exported by Civil 3D. Their coordinates were computed
//! outside this crate, by Simpson integration of the heading, and the expected joint coordinates
//! below come from that same calculation, so the test checks the reader and the geometry against
//! an independent source.

use std::f64::consts::FRAC_PI_6;
use std::path::PathBuf;

use afrobim_alignment::{parse_landxml, read_landxml, ElementKind, LandXmlError, Turn};

fn file(name: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests").join("landxml").join(name)
}

fn assert_close(got: f64, want: f64, tol: f64, what: &str) {
    assert!((got - want).abs() <= tol, "{what}: got {got}, want {want} (off by {:e})", got - want);
}

#[test]
fn reads_lines_curves_and_spirals_with_northing_first_and_a_station_start() {
    let import = read_landxml(&file("site.xml")).unwrap();
    assert!(import.notes.is_empty(), "{:?}", import.notes);
    assert_eq!(import.alignments.len(), 2);

    let a = &import.alignments[0];
    assert_eq!(a.name, "MC01");
    assert_eq!(a.start_station_m, 1000.0);
    use ElementKind::{Arc, Line, Spiral};
    let kinds: Vec<ElementKind> = a.elements.iter().map(|e| e.kind()).collect();
    assert_eq!(kinds, [Line, Spiral, Arc, Spiral, Line, Spiral, Arc, Spiral, Line]);
    let turns: Vec<Option<Turn>> = a.elements.iter().map(|e| e.turn()).collect();
    let (l, r) = (Some(Turn::Left), Some(Turn::Right));
    assert_eq!(turns, [None, l, l, l, None, r, r, r, None]);
    for (e, want) in a.elements.iter().zip([100.0, 60.0, 120.0, 60.0, 80.0, 50.0, 90.0, 50.0, 70.0]) {
        assert_close(e.length, want, 2e-6, "element length");
    }
    assert_close(a.length(), 680.0, 1e-5, "length");
    assert_close(a.end_station_m(), 1680.0, 1e-5, "end station");
    assert_eq!(a.min_radius().map(|r| (r * 1e3).round() / 1e3), Some(200.0));
    // Spiral radii: INF at the tangent end.
    assert_eq!(a.elements[1].radius_start(), None);
    assert_close(a.elements[1].radius_end().unwrap(), 300.0, 1e-9, "spiral in");
    assert_close(a.elements[3].radius_start().unwrap(), 300.0, 1e-9, "spiral out");
    assert_eq!(a.elements[3].radius_end(), None);
    assert_close(a.elements[2].radius_start().unwrap(), 300.0, 2e-6, "curve radius from Start and Center");

    // X is the easting (second number in the file), Y the northing (first).
    assert_eq!((a.elements[0].x, a.elements[0].y), (457_200.0, 31_250.0));

    // Station, easting, northing, heading at each joint and at the end: from the independent calculation.
    let expected = [
        (1000.0, 457_200.000_000, 31_250.000_000, FRAC_PI_6), // the file starts on a bearing of 30° from east
        (1100.0, 457_286.602_540, 31_300.000_000, FRAC_PI_6),
        (1160.0, 457_337.512_841, 31_331.700_828, 0.623_598_775_598),
        (1280.0, 457_418.520_527, 31_419.146_674, 1.023_598_775_598),
        (1340.0, 457_446.243_052, 31_472.328_088, 1.123_598_775_598),
        (1420.0, 457_480.838_282, 31_544.461_090, 1.123_598_775_598),
        (1470.0, 457_504.302_909, 31_588.573_913, 0.998_598_775_598),
        (1560.0, 457_568.147_096, 31_650.928_958, 0.548_598_775_598),
        (1610.0, 457_612.801_321, 31_673.346_139, 0.423_598_775_598),
        (1680.0, 457_676.614_413, 31_702.119_206, 0.423_598_775_598),
    ];
    for (station, x, y, heading) in expected {
        // Element lengths come from coordinates rounded to a micron, so stations drift by microns.
        let p = a.point_at(f64::min(station, a.end_station_m())).unwrap();
        assert_close(p.x, x, 2e-5, &format!("easting at {station}"));
        assert_close(p.y, y, 2e-5, &format!("northing at {station}"));
        assert_close(p.heading_rad, heading, 1e-6, &format!("heading at {station}"));
    }
    // Smooth all the way: the file's points are good to a micron.
    assert!(a.discontinuities(1e-5, 1e-6).is_empty(), "{:?}", a.joints());
    // Mid-spiral and mid-curve points invert.
    for station in [1130.0, 1220.0, 1445.0, 1515.0, 1585.0] {
        let (x, y) = a.point_at_offset(station, -6.0).unwrap();
        let (s, d) = a.station_offset(x, y).unwrap();
        assert_close(s, station, 1e-7, "station");
        assert_close(d, -6.0, 1e-7, "offset");
    }

    // A curve, a spiral between two radii, a tighter curve.
    let c = &import.alignments[1];
    assert_eq!(c.name, "Compound");
    assert_eq!(c.start_station_m, 0.0);
    assert_eq!(c.elements.iter().map(|e| e.kind()).collect::<Vec<_>>(), [Arc, Spiral, Arc]);
    assert_close(c.elements[1].radius_start().unwrap(), 400.0, 1e-9, "radiusStart");
    assert_close(c.elements[1].radius_end().unwrap(), 150.0, 1e-9, "radiusEnd");
    let end = c.point_at(c.end_station_m()).unwrap();
    assert_close(end.x, 919.923_630, 2e-5, "end easting");
    assert_close(end.y, 2_156.090_862, 2e-5, "end northing");
    assert_close(end.heading_rad, 2.528_662_585_328, 1e-6, "end heading");
    assert!(c.discontinuities(1e-5, 1e-6).is_empty(), "{:?}", c.joints());
}

#[test]
fn converts_units_and_says_what_it_left_out() {
    let import = read_landxml(&file("unsupported.xml")).unwrap();
    let names: Vec<&str> = import.alignments.iter().map(|a| a.name.as_str()).collect();
    assert_eq!(names, ["Feet", "By reference", "Kinked"]);
    let note = |needle: &str| import.notes.iter().find(|n| n.contains(needle)).unwrap_or_else(|| panic!("no note about {needle} in {:#?}", import.notes));

    // US survey feet to metres.
    let ft = 1200.0 / 3937.0;
    let a = &import.alignments[0];
    assert_close(a.start_station_m, 1000.0 * ft, 1e-9, "staStart");
    assert_close(a.length(), 750.0 * ft, 1e-5, "length");
    assert_close(a.elements[1].radius_start().unwrap(), 500.0 * ft, 1e-5, "radius");
    assert_eq!(a.elements[1].turn(), Some(Turn::Right));
    assert_close(a.elements[0].x, 1000.0 * ft, 1e-9, "easting");
    assert_close(a.elements[0].y, 5000.0 * ft, 1e-9, "northing");
    let end = a.point_at(a.end_station_m()).unwrap();
    assert_close(end.x, 425.093_634, 1e-5, "end easting");
    assert_close(end.y, 1_335.994_234, 1e-5, "end northing");
    assert!(note("station equations").starts_with("Alignment 'Feet'"));

    // Points by pntRef; and a stated length that disagrees with them.
    let b = &import.alignments[1];
    assert_close(b.length(), 400.0 * ft, 1e-9, "length from the referenced points");
    assert_close(b.elements[0].heading_rad, 0.0, 1e-12, "due east");
    assert!(note("Alignment 'By reference': element 1 (Line)").contains("the points are used"));
    assert!(note("Alignment 'By reference': length=").contains("add up to"));

    // A corner is imported and reported.
    assert_eq!(import.alignments[2].elements.len(), 2);
    assert!(note("Alignment 'Kinked'").contains("turns 90.0000°"), "{}", note("Alignment 'Kinked'"));

    // Left out, each with its reason.
    assert_eq!(note("'Bloss'"), "Alignment 'Bloss' was not imported: element 2 is a spiral of type 'bloss'; only clothoid spirals are supported");
    assert_eq!(note("'Irregular'"), "Alignment 'Irregular' was not imported: element 1 is a IrregularLine, which is not supported");
    assert_eq!(note("'Chained'"), "Alignment 'Chained' was not imported: element 1 is a Chain, which is not supported");
    assert!(note("'Dangling'").contains("refers to point 'P9', which is not in the file"));
    assert_eq!(import.notes.len(), 8, "{:#?}", import.notes);
}

#[test]
fn rejects_what_is_not_landxml_and_copes_with_sparse_files() {
    assert!(matches!(parse_landxml("this is not xml"), Err(LandXmlError::Xml(_))));
    assert!(matches!(parse_landxml("<Drawing/>"), Err(LandXmlError::NotLandXml(n)) if n == "Drawing"));
    assert!(matches!(
        parse_landxml(r#"<LandXML><Units><Metric linearUnit="furlong"/></Units></LandXML>"#),
        Err(LandXmlError::Unit(u)) if u == "furlong"
    ));
    assert!(matches!(read_landxml(&file("missing.xml")), Err(LandXmlError::Io(_))));

    let empty = parse_landxml("<LandXML><Units><Metric linearUnit=\"meter\"/></Units></LandXML>").unwrap();
    assert!(empty.alignments.is_empty());
    assert_eq!(empty.notes, ["The file contains no alignments."]);

    // No namespace, no units, no name, no staStart, millimetre-free: still a line, with notes.
    let sparse = parse_landxml(
        r#"<LandXML><Alignments><Alignment><CoordGeom>
             <Line><Start>10 20</Start><End>10 120 5.5</End></Line>
             <Curve rot="ccw"><Start>10 120</Start><Center>60 120</Center><End>60 170</End></Curve>
             <Spiral length="30" radiusEnd="INF" radiusStart="50" rot="up" spiType="clothoid"><Start>60 170</Start><PI>70 170</PI><End>90 172</End></Spiral>
           </CoordGeom></Alignment>
           <Alignment name="ok"><CoordGeom>
             <Line><Start>10 20</Start><End>10 120</End></Line>
             <Curve rot="ccw"><Start>10 120</Start><Center>60 120</Center><End>60 170</End></Curve>
           </CoordGeom></Alignment>
           <Alignment name="hollow" staStart="5"/></Alignments></LandXML>"#,
    )
    .unwrap();
    assert_eq!(sparse.alignments.len(), 1);
    let a = &sparse.alignments[0];
    assert_eq!((a.name.as_str(), a.start_station_m), ("ok", 0.0));
    // The line runs east from E 20 to E 120 at N 10; the curve turns left through a quarter circle of radius 50.
    assert_close(a.elements[0].heading_rad, 0.0, 1e-15, "heading east");
    assert_close(a.elements[1].length, 25.0 * std::f64::consts::PI, 1e-12, "quarter circle");
    let end = a.point_at(a.end_station_m()).unwrap();
    assert_close(end.x, 170.0, 1e-12, "end easting");
    assert_close(end.y, 60.0, 1e-12, "end northing");
    let all = sparse.notes.join("\n");
    assert!(all.contains("does not state its linear unit"), "{all}");
    assert!(all.contains("An alignment has no name; called 'Alignment 1'."), "{all}");
    assert!(all.contains("Alignment 'Alignment 1' was not imported: Spiral@rot is 'up', expected cw or ccw"), "{all}");
    assert!(all.contains("Alignment 'ok' has no staStart"), "{all}");
    assert!(all.contains("Alignment 'hollow' was not imported: it has no CoordGeom"), "{all}");
}
