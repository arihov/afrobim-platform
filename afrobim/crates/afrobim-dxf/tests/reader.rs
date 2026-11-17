//! Reader tests on hand-written DXF text, and on what this crate's own writer produces.
//! Expected values are worked by hand in the comments.

use afrobim_draw::{Colour, Drawing, Extents, Primitive, Pt2};
use afrobim_dxf::reader::{self, DisplayOptions, Geom, ReadError, Shape, SnapKind, SnapOptions};
use afrobim_dxf::{DxfDocument, DxfText, TextAlign};

/// A DXF with `header` pairs, `tables` and `blocks` text and `entities` text, each already
/// written as group code / value line pairs.
fn dxf(header: &str, tables: &str, blocks: &str, entities: &str) -> String {
    format!(
        "0\nSECTION\n2\nHEADER\n{header}0\nENDSEC\n0\nSECTION\n2\nTABLES\n{tables}0\nENDSEC\n0\nSECTION\n2\nBLOCKS\n{blocks}0\nENDSEC\n0\nSECTION\n2\nENTITIES\n{entities}0\nENDSEC\n0\nEOF\n"
    )
}

fn ents(entities: &str) -> reader::Underlay {
    reader::read_bytes(dxf("", "", "", entities).as_bytes()).unwrap()
}

fn close(a: f64, b: f64) -> bool {
    (a - b).abs() < 1e-9
}

#[test]
fn line_circle_arc_point() {
    let u = ents("0\nLINE\n8\nROAD\n10\n1\n20\n2\n30\n0\n11\n4\n21\n6\n31\n0\n0\nCIRCLE\n8\nROAD\n10\n10\n20\n10\n40\n2.5\n0\nARC\n8\nEDGE\n10\n0\n20\n0\n40\n5\n50\n30\n51\n120\n0\nPOINT\n8\nEDGE\n10\n7\n20\n8\n");
    assert_eq!(u.entities.len(), 4);
    assert!(matches!(&u.entities[0].geom, Geom::Line(a, b) if a.x == 1.0 && a.y == 2.0 && b.x == 4.0 && b.y == 6.0));
    assert!(matches!(&u.entities[1].geom, Geom::Circle { r, .. } if *r == 2.5));
    assert!(matches!(&u.entities[2].geom, Geom::Arc { a0, a1, .. } if *a0 == 30.0 && *a1 == 120.0));
    assert!(matches!(&u.entities[3].geom, Geom::Point(p) if p.x == 7.0 && p.y == 8.0));
    // Layers are created on first use, with the entity's own name.
    let names: Vec<&str> = u.layers.iter().map(|l| l.name.as_str()).collect();
    assert_eq!(names, ["0", "ROAD", "EDGE"]);
    assert_eq!(u.report.read["LINE"], 1);
    assert_eq!(u.report.total_unread(), 0);
    // Bounds: the circle reaches 12.5 in x and y (centre 10 + radius 2.5).
    let (lo, hi) = u.bounds.unwrap();
    assert!(lo.x <= 1.0);
    assert!(close(hi.x, 12.5) && close(hi.y, 12.5));
}

#[test]
fn lwpolyline_bulge_becomes_an_arc_through_the_right_points() {
    // Chord (0,0)-(2,0), bulge 0.5 on the first vertex: centre (1, 0.75), radius 1.25,
    // lowest point of the arc (1, -0.5) (see the geom tests).
    let u = ents("0\nLWPOLYLINE\n8\n0\n90\n2\n70\n0\n10\n0\n20\n0\n42\n0.5\n10\n2\n20\n0\n");
    assert!(matches!(&u.entities[0].geom, Geom::Poly { pts, closed: false } if pts[0].bulge == 0.5));
    let flat = u.flatten(1e-4);
    let (_, _, pts, closed) = &flat[0];
    assert!(!closed);
    assert!(close(pts[0].x, 0.0) && close(pts[0].y, 0.0));
    assert!(close(pts.last().unwrap().x, 2.0) && close(pts.last().unwrap().y, 0.0));
    let lowest = pts.iter().map(|p| p.y).fold(f64::INFINITY, f64::min);
    assert!((lowest + 0.5).abs() < 1e-4, "{lowest}");
    // Every point lies on the circle.
    for p in pts {
        assert!(((p.x - 1.0).hypot(p.y - 0.75) - 1.25).abs() < 1e-9);
    }
}

#[test]
fn closed_lwpolyline_and_old_style_polyline_with_bulge() {
    let u = ents("0\nLWPOLYLINE\n8\n0\n90\n3\n70\n1\n10\n0\n20\n0\n10\n4\n20\n0\n10\n4\n20\n3\n");
    assert!(matches!(&u.entities[0].geom, Geom::Poly { pts, closed: true } if pts.len() == 3));
    // R12 style: POLYLINE, VERTEX..., SEQEND. A semicircle (bulge 1) from (0,0) to (2,0).
    let r12 = "0\nPOLYLINE\n8\nA\n66\n1\n70\n0\n0\nVERTEX\n8\nA\n10\n0\n20\n0\n30\n0\n42\n1\n0\nVERTEX\n8\nA\n10\n2\n20\n0\n30\n0\n0\nSEQEND\n8\nA\n";
    let u = ents(r12);
    assert_eq!(u.entities.len(), 1);
    let (_, _, pts, _) = &u.flatten(1e-3)[0];
    let lowest = pts.iter().map(|p| p.y).fold(f64::INFINITY, f64::min);
    assert!((lowest + 1.0).abs() < 1e-3); // a semicircle of radius 1 dips to y = -1
    assert_eq!(u.report.read["POLYLINE"], 1);
}

#[test]
fn polyline_meshes_and_a_missing_seqend_are_handled() {
    let mesh = "0\nPOLYLINE\n8\nA\n66\n1\n70\n16\n71\n2\n72\n2\n0\nVERTEX\n8\nA\n10\n0\n20\n0\n30\n0\n70\n64\n0\nSEQEND\n8\nA\n";
    let u = ents(mesh);
    assert!(u.entities.is_empty());
    assert_eq!(u.report.unread["POLYLINE (3D mesh)"], 1);
    let u = ents("0\nPOLYLINE\n8\nA\n66\n1\n70\n0\n0\nVERTEX\n8\nA\n10\n0\n20\n0\n30\n0\n0\nVERTEX\n8\nA\n10\n1\n20\n1\n30\n0\n0\nLINE\n8\nA\n10\n0\n20\n0\n11\n1\n21\n1\n");
    assert_eq!(u.entities.len(), 2);
    assert!(u.report.warnings.iter().any(|w| w.contains("SEQEND")));
}

#[test]
fn ellipse_full_and_partial() {
    // Centre (10,20), major axis end (4,0), ratio 0.5: semi-axes 4 and 2.
    let u = ents("0\nELLIPSE\n8\n0\n10\n10\n20\n20\n30\n0\n11\n4\n21\n0\n31\n0\n40\n0.5\n41\n0\n42\n6.283185307179586\n");
    let (_, _, pts, closed) = &u.flatten(1e-3)[0];
    assert!(*closed);
    for p in pts {
        let (x, y) = ((p.x - 10.0) / 4.0, (p.y - 20.0) / 2.0);
        assert!(close(x * x + y * y, 1.0));
    }
    // Quarter from parameter 0 to pi/2: from (14,20) to (10,22).
    let q = ents("0\nELLIPSE\n8\n0\n10\n10\n20\n20\n30\n0\n11\n4\n21\n0\n31\n0\n40\n0.5\n41\n0\n42\n1.5707963267948966\n");
    let (_, _, pts, closed) = &q.flatten(1e-3)[0];
    assert!(!closed);
    assert!(close(pts[0].x, 14.0) && close(pts[0].y, 20.0));
    assert!(close(pts.last().unwrap().x, 10.0) && close(pts.last().unwrap().y, 22.0));
    // Rotated major axis (0,3): the minor axis is 0.5 * (-3, 0), so the half ellipse bulges to -X.
    let r = ents("0\nELLIPSE\n8\n0\n10\n0\n20\n0\n30\n0\n11\n0\n21\n3\n31\n0\n40\n0.5\n41\n0\n42\n3.141592653589793\n");
    let (_, _, pts, _) = &r.flatten(1e-3)[0];
    assert!(close(pts[0].x, 0.0) && close(pts[0].y, 3.0));
    assert!(pts.iter().any(|p| (p.x + 1.5).abs() < 0.01));
}

#[test]
fn spline_is_evaluated_and_bad_ones_fall_back_with_a_warning() {
    // Clamped quadratic Bezier (0,0) (1,2) (2,0): C(t) = (2t, 4t(1-t)), so y = x (2 - x).
    let ok = "0\nSPLINE\n8\n0\n70\n8\n71\n2\n72\n6\n73\n3\n74\n0\n40\n0\n40\n0\n40\n0\n40\n1\n40\n1\n40\n1\n10\n0\n20\n0\n30\n0\n10\n1\n20\n2\n30\n0\n10\n2\n20\n0\n30\n0\n";
    let u = ents(ok);
    let (_, _, pts, _) = &u.flatten(1e-4)[0];
    assert!(pts.iter().any(|p| (p.x - 1.0).abs() < 1e-3 && (p.y - 1.0).abs() < 1e-3));
    for p in pts {
        assert!((p.y - p.x * (2.0 - p.x)).abs() < 1e-9);
    }
    let ex = u.expand(&Default::default());
    assert!(ex.warnings.is_empty());
    // Knots missing: the control polygon is drawn and the file says why.
    let bad = "0\nSPLINE\n8\n0\n70\n8\n71\n2\n73\n3\n10\n0\n20\n0\n30\n0\n10\n1\n20\n2\n30\n0\n10\n2\n20\n0\n30\n0\n";
    let u = ents(bad);
    let ex = u.expand(&Default::default());
    assert_eq!(ex.items.len(), 1);
    assert!(ex.warnings.iter().any(|w| w.contains("control polygon")));
}

#[test]
fn a_spline_with_only_fit_points_is_drawn_through_them_with_a_warning() {
    let fit = "0\nSPLINE\n8\n0\n70\n0\n71\n3\n74\n3\n11\n0\n21\n0\n31\n0\n11\n5\n21\n5\n31\n0\n11\n10\n21\n0\n31\n0\n";
    let u = ents(fit);
    let ex = u.expand(&Default::default());
    assert!(ex.warnings.iter().any(|w| w.contains("fit points only")));
    let (_, _, pts, _) = &u.flatten(1e-3)[0];
    assert_eq!(pts.len(), 3);
    assert!(close(pts[1].x, 5.0) && close(pts[1].y, 5.0));
}

#[test]
fn text_and_mtext() {
    let u = ents("0\nTEXT\n8\nT\n10\n5\n20\n6\n30\n0\n40\n2.5\n1\nKerb 45%%d\n50\n30\n72\n1\n73\n0\n11\n9\n21\n9\n31\n0\n0\nMTEXT\n8\nT\n10\n1\n20\n2\n30\n0\n40\n3\n41\n50\n71\n5\n3\nSecond \\\\\\\\ \n1\n{\\fArial|b0;\\H2;Bold}\\PNext line\n");
    let Geom::Text(t) = &u.entities[0].geom else { panic!() };
    // Centre alignment uses the alignment point (9, 9), not the insertion point.
    assert_eq!((t.at.x, t.at.y, t.height, t.rotation_deg), (9.0, 9.0, 2.5, 30.0));
    assert_eq!(t.text, "Kerb 45\u{b0}");
    assert_eq!((t.h, t.v), (reader::HAlign::Centre, reader::VAlign::Baseline));
    let Geom::Text(m) = &u.entities[1].geom else { panic!() };
    // The two chunks are joined (code 3 first, then 1) and formatting is stripped.
    assert!(m.text.starts_with("Second ") && m.text.ends_with("Bold\nNext line"), "{:?}", m.text);
    assert_eq!((m.h, m.v, m.box_width), (reader::HAlign::Centre, reader::VAlign::Middle, 50.0));
    assert!(u.report.mtext_stripped.contains("\\f") && u.report.mtext_stripped.contains("\\H") && u.report.mtext_stripped.contains("{}"));
    let d = u.to_drawing(&DisplayOptions { unit_scale_mm: Some(1000.0), ..Default::default() });
    assert_eq!(d.labels.len(), 2);
    assert!(close(d.labels[0].height_mm, 2500.0));
}

#[test]
fn solid_and_hatch_outlines() {
    // SOLID corners 0,1,2,3 are stored 0,1,3,2 around the outline: (0,0) (2,0) (0,1) (2,1) is
    // the quad (0,0) (2,0) (2,1) (0,1).
    let u = ents("0\nSOLID\n8\n0\n10\n0\n20\n0\n11\n2\n21\n0\n12\n0\n22\n1\n13\n2\n23\n1\n");
    assert!(matches!(&u.entities[0].geom, Geom::Solid(p) if p.len() == 4 && p[2].x == 2.0 && p[2].y == 1.0 && p[3].x == 0.0 && p[3].y == 1.0));
    // Polyline-path hatch: a closed 4 by 3 rectangle.
    let h = "0\nHATCH\n8\n0\n10\n0\n20\n0\n30\n0\n210\n0\n220\n0\n230\n1\n2\nSOLID\n70\n1\n71\n0\n91\n1\n92\n7\n72\n0\n73\n1\n93\n4\n10\n0\n20\n0\n10\n4\n20\n0\n10\n4\n20\n3\n10\n0\n20\n3\n97\n0\n75\n0\n76\n1\n98\n0\n";
    let u = ents(h);
    let flat = u.flatten(1e-3);
    assert_eq!(flat.len(), 1);
    assert_eq!(flat[0].2.len(), 4);
    assert!(flat[0].3);
    // Edge-path hatch: line, arc (anticlockwise, centre (4,2) radius 2 from 270 to 90 degrees),
    // line, line: a D shape whose arc bulges through (6, 2).
    let e = "0\nHATCH\n8\n0\n10\n0\n20\n0\n30\n0\n210\n0\n220\n0\n230\n1\n2\nSOLID\n70\n1\n71\n0\n91\n1\n92\n1\n93\n4\n72\n1\n10\n0\n20\n0\n11\n4\n21\n0\n72\n2\n10\n4\n20\n2\n40\n2\n50\n270\n51\n90\n73\n1\n72\n1\n10\n4\n20\n4\n11\n0\n21\n4\n72\n1\n10\n0\n20\n4\n11\n0\n21\n0\n97\n0\n75\n0\n76\n1\n98\n0\n";
    let u = ents(e);
    let (_, _, pts, closed) = &u.flatten(1e-3)[0];
    assert!(*closed);
    assert!(pts.iter().any(|p| (p.x - 6.0).abs() < 1e-3 && (p.y - 2.0).abs() < 1e-3));
}

#[test]
fn layers_header_units_and_extents() {
    let header = "9\n$ACADVER\n1\nAC1027\n9\n$INSUNITS\n70\n6\n9\n$MEASUREMENT\n70\n1\n9\n$EXTMIN\n10\n-5\n20\n-6\n30\n0\n9\n$EXTMAX\n10\n100\n20\n200\n30\n0\n";
    let tables = "0\nTABLE\n2\nLAYER\n70\n3\n0\nLAYER\n2\nCARRIAGEWAY\n70\n0\n62\n3\n6\nCONTINUOUS\n0\nLAYER\n2\nOLD\n70\n1\n62\n8\n0\nLAYER\n2\nHIDDEN\n70\n4\n62\n-5\n420\n16711680\n0\nENDTAB\n";
    let u = reader::read_bytes(dxf(header, tables, "", "0\nLINE\n8\nOLD\n10\n0\n20\n0\n11\n1\n21\n1\n0\nLINE\n8\nCARRIAGEWAY\n10\n0\n20\n0\n11\n5\n21\n5\n").as_bytes()).unwrap();
    assert_eq!(u.report.dxf_version.as_deref(), Some("AC1027"));
    assert_eq!((u.units.insunits, u.units.measurement, u.units.mm_per_unit, u.units.assumed), (Some(6), Some(1), 1000.0, false));
    assert_eq!(u.units.name, "metres");
    assert_eq!(u.unit_scale_mm, 1000.0);
    let (lo, hi) = u.header_extents.unwrap();
    assert_eq!((lo.x, lo.y, hi.x, hi.y), (-5.0, -6.0, 100.0, 200.0));
    let by = |n: &str| u.layers[u.layer_index(n).unwrap()].clone();
    assert_eq!(by("CARRIAGEWAY").aci, Some(3));
    assert!(by("old").frozen && !by("old").visible()); // case-insensitive lookup
    let h = by("HIDDEN");
    assert!(h.locked && h.off && h.rgb == Some([0xFF, 0, 0]) && !h.visible());
    // A frozen layer is not drawn unless asked for.
    let d = u.to_drawing(&DisplayOptions::default());
    assert_eq!(d.items, 1);
    let all = u.to_drawing(&DisplayOptions { show_file_hidden_layers: true, ..Default::default() });
    assert_eq!(all.items, 2);
    let hidden = u.to_drawing(&DisplayOptions { hidden_layers: vec!["carriageway".into()], ..Default::default() });
    assert_eq!(hidden.items, 0);
    // The drawing is in millimetres: the 5 x 5 m line is 5000 long in each direction.
    let ext = d.drawing.extents;
    assert!(close(ext.max.x, 5000.0) && close(ext.max.y, 5000.0));
    // Overriding the scale (the file is really in mm).
    let mm = u.to_drawing(&DisplayOptions { unit_scale_mm: Some(1.0), ..Default::default() });
    assert!(close(mm.drawing.extents.max.x, 5.0));
}

#[test]
fn missing_units_are_assumed_and_flagged() {
    let u = ents("0\nLINE\n8\n0\n10\n0\n20\n0\n11\n1\n21\n1\n");
    assert!(u.units.assumed && u.units.mm_per_unit == 1000.0);
    let u = reader::read_bytes(dxf("9\n$INSUNITS\n70\n0\n9\n$MEASUREMENT\n70\n0\n", "", "", "").as_bytes()).unwrap();
    assert!(u.units.assumed && (u.units.mm_per_unit - 25.4).abs() < 1e-12);
}

const BLOCKS: &str = "\
0\nBLOCK\n8\n0\n2\nBOX\n70\n0\n10\n1\n20\n1\n30\n0\n\
0\nLINE\n8\n0\n10\n1\n20\n1\n11\n3\n21\n1\n\
0\nLINE\n8\nSIDE\n62\n0\n10\n3\n20\n1\n11\n3\n21\n2\n\
0\nENDBLK\n8\n0\n\
0\nBLOCK\n8\n0\n2\nOUTER\n70\n0\n10\n0\n20\n0\n30\n0\n\
0\nINSERT\n8\n0\n2\nBOX\n10\n10\n20\n0\n\
0\nENDBLK\n8\n0\n\
0\nBLOCK\n8\n0\n2\nLOOP\n70\n0\n10\n0\n20\n0\n30\n0\n\
0\nINSERT\n8\n0\n2\nLOOP\n10\n1\n20\n0\n\
0\nENDBLK\n8\n0\n";

fn with_blocks(entities: &str) -> reader::Underlay {
    reader::read_bytes(dxf("", "", BLOCKS, entities).as_bytes()).unwrap()
}

#[test]
fn insert_applies_base_point_scale_and_rotation() {
    // BOX has base (1,1); its first line runs (1,1)-(3,1). Insert at (100, 50), scale 2, rotation 90:
    // from the base the line is (0,0)-(2,0), scaled (0,0)-(4,0), rotated (0,0)-(0,4), placed at (100,50).
    let u = with_blocks("0\nINSERT\n8\nPLAN\n2\nBOX\n10\n100\n20\n50\n41\n2\n42\n2\n50\n90\n62\n1\n");
    let flat = u.flatten(1e-3);
    assert_eq!(flat.len(), 2);
    let (_, layer, pts, _) = &flat[0];
    assert_eq!(layer, "PLAN"); // layer 0 inside the block takes the insert's layer
    assert!(close(pts[0].x, 100.0) && close(pts[0].y, 50.0) && close(pts[1].x, 100.0) && close(pts[1].y, 54.0), "{pts:?}");
    // The second line (3,1)-(3,2) is (2,0)-(2,1) from the base:
    // (2,0) -> (4,0) -> (0,4) -> (100,54); (2,1) -> (4,2) -> (-2,4) -> (98,54).
    let (_, layer2, p2, _) = &flat[1];
    assert_eq!(layer2, "SIDE"); // a named layer keeps its name
    assert!(close(p2[0].x, 100.0) && close(p2[0].y, 54.0) && close(p2[1].x, 98.0) && close(p2[1].y, 54.0), "{p2:?}");
    // By-block colour (62 = 0 on the second line) takes the insert's colour (ACI 1 = red).
    let ex = u.expand(&Default::default());
    assert_eq!(ex.items[1].colour.aci, Some(1));
    assert_eq!(ex.items[1].colour.rgb, [255, 0, 0]);
}

#[test]
fn minsert_array_and_nested_blocks() {
    // 3 columns x 2 rows, spacing 10 and 5, rotated 90 degrees: the grid turns, the spacing is not scaled.
    let u = with_blocks("0\nINSERT\n8\n0\n2\nBOX\n10\n0\n20\n0\n50\n90\n70\n3\n71\n2\n44\n10\n45\n5\n");
    let flat = u.flatten(1e-3);
    assert_eq!(flat.len(), 12); // 6 copies x 2 lines
    // The copy at column 2, row 1: offset (20, 5) turned 90 degrees = (-5, 20).
    assert!(flat.iter().any(|(_, _, p, _)| (p[0].x + 5.0).abs() < 1e-9 && (p[0].y - 20.0).abs() < 1e-9));
    // OUTER inserts BOX at (10,0): BOX line (1,1)-(3,1) with base (1,1) lands at (10,0)-(12,0); then OUTER at (100,100).
    let u = with_blocks("0\nINSERT\n8\n0\n2\nOUTER\n10\n100\n20\n100\n");
    let flat = u.flatten(1e-3);
    assert_eq!(flat.len(), 2);
    assert!(close(flat[0].2[0].x, 110.0) && close(flat[0].2[0].y, 100.0) && close(flat[0].2[1].x, 112.0));
}

#[test]
fn recursive_blocks_stop_at_the_depth_limit_and_missing_blocks_warn() {
    let u = with_blocks("0\nINSERT\n8\n0\n2\nLOOP\n10\n0\n20\n0\n0\nINSERT\n8\n0\n2\nGONE\n10\n0\n20\n0\n");
    let ex = u.expand(&reader::ExpandOptions { max_depth: 5, ..Default::default() });
    assert!(ex.warnings.iter().any(|w| w.contains("nested deeper than 5")));
    assert!(ex.warnings.iter().any(|w| w.contains("'GONE'")));
    assert!(ex.items.is_empty());
}

#[test]
fn mirrored_extrusion_flips_x_and_arc_direction() {
    // ARC centre (3,4) radius 2 from 0 to 90 degrees in an OCS with normal (0,0,-1): mirrored in x,
    // so it is the arc centred at (-3,4) from 90 to 180.
    let u = ents("0\nARC\n8\n0\n10\n3\n20\n4\n40\n2\n50\n0\n51\n90\n210\n0\n220\n0\n230\n-1\n");
    let Geom::Arc { c, a0, a1, .. } = &u.entities[0].geom else { panic!() };
    assert!(close(c.x, -3.0) && close(c.y, 4.0) && close(*a0, 90.0) && close(*a1, 180.0));
    // An oblique extrusion is counted, not guessed.
    let o = ents("0\nCIRCLE\n8\n0\n10\n0\n20\n0\n40\n1\n210\n1\n220\n0\n230\n0\n");
    assert_eq!(o.report.unread["CIRCLE (oblique extrusion)"], 1);
}

#[test]
fn everything_not_read_is_counted_by_type() {
    let u = ents("0\nLEADER\n8\n0\n0\nIMAGE\n8\n0\n0\nIMAGE\n8\n0\n0\nVIEWPORT\n8\n0\n0\nLINE\n8\n0\n10\n0\n20\n0\n11\n1\n21\n1\n0\nLINE\n8\n0\n67\n1\n10\n0\n20\n0\n11\n1\n21\n1\n0\nLINE\n8\n0\n60\n1\n10\n0\n20\n0\n11\n1\n21\n1\n");
    assert_eq!(u.report.unread["LEADER"], 1);
    assert_eq!(u.report.unread["IMAGE"], 2);
    assert_eq!(u.report.unread["VIEWPORT"], 1);
    assert_eq!(u.report.unread["LINE (invisible)"], 1);
    assert_eq!(u.report.paper_space_skipped, 1);
    assert_eq!(u.report.read["LINE"], 1);
    assert_eq!(u.report.total_unread(), 5);
}

#[test]
fn dimension_is_drawn_through_its_block() {
    let blocks = "0\nBLOCK\n8\n0\n2\n*D1\n70\n1\n10\n0\n20\n0\n30\n0\n0\nLINE\n8\nDIMS\n10\n0\n20\n0\n11\n10\n21\n0\n0\nENDBLK\n8\n0\n";
    let u = reader::read_bytes(dxf("", "", blocks, "0\nDIMENSION\n8\nDIMS\n2\n*D1\n10\n5\n20\n5\n70\n1\n").as_bytes()).unwrap();
    let flat = u.flatten(1e-3);
    assert_eq!(flat.len(), 1);
    assert!(close(flat[0].2[1].x, 10.0));
    assert_eq!(u.report.read["DIMENSION"], 1);
}

#[test]
fn binary_dxf_and_dwg_are_refused_with_instructions() {
    let e = reader::read_bytes(b"AutoCAD Binary DXF\r\n\x1a\0\x00").unwrap_err();
    assert!(matches!(e, ReadError::Binary));
    assert!(e.to_string().contains("ASCII"));
    let e = reader::read_bytes(b"AC1027\0\0\0\0\0\0\0\0\0\0\0\0\0").unwrap_err();
    assert!(matches!(e, ReadError::Dwg(_)));
    let m = e.to_string();
    assert!(m.contains("DWG") && m.contains("convert it to DXF") && m.contains("ODA File Converter") && m.contains("release R2013"), "{m}");
    assert!(matches!(reader::read_bytes(b"hello world\nthis is not a dxf\n").unwrap_err(), ReadError::NotDxf(_)));
}

#[test]
fn round_trip_of_what_this_crates_writer_produces() {
    let mut w = DxfDocument::new();
    w.add_line("ROADS", 3, Pt2::new(0.0, 0.0), Pt2::new(1500.0, 250.0));
    w.add_polyline("ROADS", 3, &[Pt2::new(0.0, 0.0), Pt2::new(1000.0, 0.0), Pt2::new(1000.0, 500.0)], true, 0.0);
    w.add_arc("ARCS", 1, Pt2::new(100.0, 100.0), 50.0, 10.0, 80.0);
    w.add_solid("FILL", 2, &[Pt2::new(0.0, 0.0), Pt2::new(10.0, 0.0), Pt2::new(10.0, 10.0), Pt2::new(0.0, 10.0)]);
    w.add_text(&DxfText { align: TextAlign::Centre, rotation_deg: 45.0, ..DxfText::new("STA 1+250 \u{b0}", Pt2::new(7.0, 8.0), 2.5, "NOTES") });
    // A drawing through the sign writer's own path: a band, an arc and a fill.
    let d = Drawing {
        primitives: vec![
            Primitive::Line { from: Pt2::new(0.0, 0.0), to: Pt2::new(100.0, 0.0), width: 10.0, colour: Some(Colour::Green) },
            Primitive::Arc { centre: Pt2::new(50.0, 50.0), radius: 20.0, start_deg: 0.0, end_deg: 180.0, colour: Some(Colour::Red) },
            Primitive::Fill { outline: vec![Pt2::new(0.0, 0.0), Pt2::new(5.0, 0.0), Pt2::new(5.0, 5.0)], colour: Colour::Blue },
        ],
        extents: Extents::default(),
    };
    w.add(&d, 1.0, Pt2::new(2000.0, 0.0));
    let text = w.finish();
    let u = reader::read_bytes(text.as_bytes()).unwrap();
    assert_eq!(u.units.insunits, Some(4)); // the writer declares millimetres
    assert_eq!(u.unit_scale_mm, 1.0);
    assert_eq!(u.report.dxf_version.as_deref(), Some("AC1009"));
    assert_eq!(u.report.total_unread(), 0, "{:?}", u.report.unread);
    let layers: Vec<&str> = u.layers.iter().map(|l| l.name.as_str()).collect();
    for l in ["ROADS", "ARCS", "NOTES", "SIGN-GREEN", "SIGN-RED", "SIGN-BLUE"] {
        assert!(layers.contains(&l), "{l} in {layers:?}");
    }
    let flat = u.flatten(1e-3);
    let find = |kind: &str, layer: &str| flat.iter().find(|f| f.0 == kind && f.1 == layer).unwrap().clone();
    let line = find("LINE", "ROADS");
    assert!(close(line.2[1].x, 1500.0) && close(line.2[1].y, 250.0));
    let poly = find("POLYLINE", "ROADS");
    assert!(poly.3 && poly.2.len() == 3 && close(poly.2[2].y, 500.0));
    let arc = find("ARC", "ARCS");
    assert!(close(arc.2[0].x, 100.0 + 50.0 * 10f64.to_radians().cos()));
    // The sign band is a closed rectangle written as a polyline (width 10 about the centre line).
    let band = find("POLYLINE", "SIGN-GREEN");
    let ys: Vec<f64> = band.2.iter().map(|p| p.y).collect();
    assert!(ys.iter().any(|y| close(*y, 5.0)) && ys.iter().any(|y| close(*y, -5.0)));
    // The text reads back as plain text at its centre anchor, with the unicode escape decoded.
    let t = u.entities.iter().find_map(|e| if let Geom::Text(t) = &e.geom { Some(t.clone()) } else { None }).unwrap();
    assert_eq!(t.text, "STA 1+250 \u{b0}");
    assert_eq!((t.at.x, t.at.y, t.height, t.rotation_deg, t.h), (7.0, 8.0, 2.5, 45.0, reader::HAlign::Centre));
    // The SOLID the writer wrote (its 3rd and 4th points swapped in the file) reads back in outline order.
    let solid = find("SOLID", "FILL");
    assert!(close(solid.2[2].x, 10.0) && close(solid.2[2].y, 10.0) && close(solid.2[3].x, 0.0) && close(solid.2[3].y, 10.0), "{:?}", solid.2);
}

#[test]
fn snap_points_include_ends_mids_centres_and_crossings() {
    let u = ents("0\nLINE\n8\n0\n10\n0\n20\n0\n11\n10\n21\n10\n0\nLINE\n8\n0\n10\n0\n20\n10\n11\n10\n21\n0\n0\nCIRCLE\n8\n0\n10\n20\n20\n20\n40\n3\n");
    let d = DisplayOptions { unit_scale_mm: Some(1.0), ..Default::default() };
    let s = u.snap_points(&d, &SnapOptions::default());
    let has = |k: SnapKind, x: f64, y: f64| s.points.iter().any(|p| p.kind == k && close(p.p.x, x) && close(p.p.y, y));
    assert!(has(SnapKind::End, 0.0, 0.0) && has(SnapKind::End, 10.0, 0.0));
    assert!(has(SnapKind::Mid, 5.0, 5.0));
    assert!(has(SnapKind::Centre, 20.0, 20.0));
    assert!(has(SnapKind::Quadrant, 23.0, 20.0));
    // The two diagonals cross at (5,5): one intersection, not two.
    assert_eq!(s.points.iter().filter(|p| p.kind == SnapKind::Intersection).count(), 1);
    assert!(has(SnapKind::Intersection, 5.0, 5.0));
    assert!(!s.truncated_intersections);
    // The budgets are honoured and reported.
    let tiny = u.snap_points(&d, &SnapOptions { max_pair_tests: 0, ..Default::default() });
    assert!(tiny.truncated_intersections && tiny.points.iter().all(|p| p.kind != SnapKind::Intersection));
    let cap = u.snap_points(&d, &SnapOptions { max_points: 3, ..Default::default() });
    assert!(cap.truncated_points && cap.points.iter().filter(|p| p.kind != SnapKind::Intersection).count() == 3);
}

#[test]
fn colours_in_the_drawing_map_to_named_colours() {
    let u = ents("0\nLINE\n8\n0\n62\n1\n10\n0\n20\n0\n11\n1\n21\n0\n0\nLINE\n8\n0\n62\n7\n10\n0\n20\n1\n11\n1\n21\n1\n");
    let d = u.to_drawing(&DisplayOptions { unit_scale_mm: Some(1.0), ..Default::default() });
    let cols: Vec<Option<Colour>> = d.drawing.primitives.iter().map(|p| if let Primitive::Line { colour, .. } = p { *colour } else { None }).collect();
    assert_eq!(cols, vec![Some(Colour::Red), None]); // ACI 1 is red; ACI 7 (white) is the front end's ink
    assert!(matches!(u.expand(&Default::default()).items[0].shape, Shape::Line(..)));
}
