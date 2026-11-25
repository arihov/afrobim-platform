// SPDX-License-Identifier: LicenseRef-AfroBIM-Proprietary
// Copyright (c) 2026 AfroBIM. All rights reserved.
//! The synthetic importer cases: every geometry path the importer supports, each in one product,
//! with unusual placements. Built with `synth::B`.

#![allow(dead_code)]

use super::synth::*;

pub fn faces_zero_based(faces: &[(Vec<u32>, Vec<Vec<u32>>)]) -> Vec<BrepFace> {
    faces
        .iter()
        .map(|(o, h)| (o.iter().map(|&i| i as usize - 1).collect(), h.iter().map(|r| r.iter().map(|&i| i as usize - 1).collect()).collect(), false))
        .collect()
}

pub const L_SHAPE: [[f64; 2]; 6] = [[0.0, 0.0], [1000.0, 0.0], [1000.0, 300.0], [300.0, 300.0], [300.0, 800.0], [0.0, 800.0]];

pub const PLATE: [[f64; 3]; 16] = [
    [0.0, 0.0, 0.0],
    [10000.0, 0.0, 0.0],
    [10000.0, 10000.0, 0.0],
    [0.0, 10000.0, 0.0],
    [0.0, 0.0, 1000.0],
    [10000.0, 0.0, 1000.0],
    [10000.0, 10000.0, 1000.0],
    [0.0, 10000.0, 1000.0],
    [3000.0, 3000.0, 0.0],
    [5000.0, 3000.0, 0.0],
    [5000.0, 5000.0, 0.0],
    [3000.0, 5000.0, 0.0],
    [3000.0, 3000.0, 1000.0],
    [5000.0, 3000.0, 1000.0],
    [5000.0, 5000.0, 1000.0],
    [3000.0, 5000.0, 1000.0],
];

pub fn plate_faces() -> Vec<(Vec<u32>, Vec<Vec<u32>>)> {
    vec![
        (vec![5, 6, 7, 8], vec![vec![13, 14, 15, 16]]),
        (vec![1, 4, 3, 2], vec![vec![9, 10, 11, 12]]),
        (vec![1, 2, 6, 5], vec![]),
        (vec![2, 3, 7, 6], vec![]),
        (vec![3, 4, 8, 7], vec![]),
        (vec![4, 1, 5, 8], vec![]),
        (vec![9, 13, 14, 10], vec![]),
        (vec![10, 14, 15, 11], vec![]),
        (vec![11, 15, 16, 12], vec![]),
        (vec![12, 16, 13, 9], vec![]),
    ]
}

/// `odd` adds the products IfcOpenShell is not asked about: bounding box only, an unread item, no
/// representation, an unresolvable placement.
pub fn cases(odd: bool) -> String {
    let mut b = B::new("IFC4", "IFCSIUNIT(*,.LENGTHUNIT.,.MILLI.,.METRE.)");
    let c30 = 30f64.to_radians();
    let s = std::f64::consts::FRAC_1_SQRT_2;

    // A: slanted extrusion, rotated profile and solid frames, rotated product.
    let pl = b.local_placement(None, [10000.0, 5000.0, 2000.0], Some([c30.cos(), c30.sin(), 0.0]));
    let prof = b.rect(2000.0, 1000.0, [500.0, 0.0], Some([1.0, 1.0]));
    let pos = b.axis3([100.0, 200.0, 300.0], Some([0.0, 0.0, 1.0]), Some([0.0, 1.0, 0.0]));
    let ex = b.extrusion(prof, Some(pos), [0.2, 0.0, 1.0], 3000.0);
    let sh = b.shape("Body", "SweptSolid", &[ex]);
    let a = b.proxy("A-slanted", pl, Some(sh));
    let p1 = b.add("IFCPROPERTYSINGLEVALUE('Label',$,IFCLABEL('abc'),$)".to_string());
    let p2 = b.add("IFCPROPERTYSINGLEVALUE('Len',$,IFCLENGTHMEASURE(12.5),$)".to_string());
    let p3 = b.add("IFCPROPERTYSINGLEVALUE('Flag',$,IFCBOOLEAN(.T.),$)".to_string());
    let p4 = b.add("IFCPROPERTYENUMERATEDVALUE('Choice',$,(IFCLABEL('x'),IFCLABEL('y')),$)".to_string());
    let p5 = b.add("IFCPROPERTYBOUNDEDVALUE('Range',$,IFCLENGTHMEASURE(2.),IFCLENGTHMEASURE(1.),$,$)".to_string());
    let ps = b.add(format!("IFCPROPERTYSET('{}',$,'Pset_Test',$,(#{p1},#{p2},#{p3},#{p4},#{p5}))", guid(500)));
    let q = b.add("IFCQUANTITYLENGTH('Length',$,$,2500.,$)".to_string());
    let qs = b.add(format!("IFCELEMENTQUANTITY('{}',$,'Qto_Test',$,$,(#{q}))", guid(501)));
    b.add(format!("IFCRELDEFINESBYPROPERTIES('{}',$,$,$,(#{a}),#{ps})", guid(502)));
    b.add(format!("IFCRELDEFINESBYPROPERTIES('{}',$,$,$,(#{a}),#{qs})", guid(503)));

    // B: mapped item, origin offset, rotated operator and scale 2; an L profile so orientation matters.
    let prof = b.closed_profile(&L_SHAPE);
    let ex = b.extrusion(prof, None, [0.0, 0.0, 1.0], 400.0);
    let origin = b.axis3([1000.0, 0.0, 0.0], Some([0.0, 0.0, 1.0]), Some([1.0, 0.0, 0.0]));
    let op = b.operator(Some([0.0, 1.0, 0.0]), Some([-1.0, 0.0, 0.0]), Some([0.0, 0.0, 1.0]), [5000.0, 0.0, 0.0], Some(2.0));
    let mi = b.mapped(origin, &[ex], op);
    let pl = b.local_placement(None, [0.0, 20000.0, 0.0], None);
    let sh = b.shape("Body", "MappedRepresentation", &[mi]);
    b.proxy("B-mapped", pl, Some(sh));

    // B2: a rotated origin frame and an operator with defaults.
    let origin = b.axis3([0.0, 0.0, 0.0], Some([0.0, 0.0, 1.0]), Some([s, s, 0.0]));
    let op = b.operator(None, None, None, [-3000.0, 1000.0, 500.0], None);
    let prof = b.closed_profile(&L_SHAPE);
    let ex = b.extrusion(prof, None, [0.0, 0.0, 1.0], 400.0);
    let mi = b.mapped(origin, &[ex], op);
    let pl = b.local_placement(None, [0.0, 30000.0, 0.0], None);
    let sh = b.shape("Body", "MappedRepresentation", &[mi]);
    b.proxy("B2-mapped-origin", pl, Some(sh));

    // C: tetrahedron as a triangulated set, and the same through PnIndex.
    let tet = [[0.0, 0.0, 0.0], [1000.0, 0.0, 0.0], [0.0, 1000.0, 0.0], [0.0, 0.0, 1000.0]];
    let tris = [[1, 3, 2], [1, 2, 4], [1, 4, 3], [2, 3, 4]];
    let ts = b.triangulated(&tet, &tris, None);
    let mut shifted = vec![[9e9, 9e9, 9e9]];
    shifted.extend_from_slice(&tet);
    let ts2 = b.triangulated(&shifted, &tris, Some(&[2, 3, 4, 5]));
    let pl = b.local_placement(None, [0.0, 40000.0, 0.0], None);
    let sh = b.shape("Body", "Tessellation", &[ts]);
    b.proxy("C-triangulated", pl, Some(sh));
    let pl = b.local_placement(None, [0.0, 41000.0, 0.0], None);
    let sh = b.shape("Body", "Tessellation", &[ts2]);
    b.proxy("C2-triangulated-pnindex", pl, Some(sh));

    // D: polygonal face set, a plate with a square hole.
    let ps = b.polygonal(&PLATE, &plate_faces());
    let pl = b.local_placement(None, [0.0, 50000.0, 0.0], None);
    let sh = b.shape("Body", "Tessellation", &[ps]);
    b.proxy("D-polygonal-void", pl, Some(sh));

    // E: faceted brep, a cuboid with one face written reversed with .F. orientation.
    let pts = cuboid(1000.0, 2000.0, 3000.0);
    let faces: Vec<BrepFace> = CUBOID_FACES.iter().enumerate().map(|(i, f)| (f.to_vec(), vec![], i == 1)).collect();
    let br = b.brep(&pts, &faces);
    let pl = b.local_placement(None, [0.0, 60000.0, 0.0], None);
    let sh = b.shape("Body", "Brep", &[br]);
    b.proxy("E-brep-reversed-bound", pl, Some(sh));

    // E2: faceted brep with a hole (inner bound) in the top and bottom faces.
    let faces = faces_zero_based(&plate_faces());
    let br = b.brep(&PLATE, &faces);
    let pl = b.local_placement(None, [0.0, 70000.0, 0.0], None);
    let sh = b.shape("Body", "Brep", &[br]);
    b.proxy("E2-brep-hole", pl, Some(sh));

    // F: circle extrusion on a product turned 45 degrees; profile with a void.
    let c = b.circle(500.0);
    let ex = b.extrusion(c, None, [0.0, 0.0, 1.0], 2000.0);
    let pl = b.local_placement(None, [0.0, 80000.0, 0.0], Some([s, s, 0.0]));
    let sh = b.shape("Body", "SweptSolid", &[ex]);
    b.proxy("F-circle", pl, Some(sh));
    let p = b.profile_with_voids(&[[0.0, 0.0], [4000.0, 0.0], [4000.0, 4000.0], [0.0, 4000.0]], &[[1000.0, 1000.0], [2000.0, 1000.0], [2000.0, 2000.0], [1000.0, 2000.0]]);
    let ex = b.extrusion(p, None, [0.0, 0.0, 1.0], 1000.0);
    let pl = b.local_placement(None, [0.0, 90000.0, 0.0], None);
    let sh = b.shape("Body", "SweptSolid", &[ex]);
    b.proxy("F2-profile-with-void", pl, Some(sh));

    // G: placement chain, site -> building -> storey -> product, each rotated or lifted.
    let site = b.local_placement(None, [1000.0, 2000.0, 0.0], Some([0.0, 1.0, 0.0]));
    let building = b.local_placement(Some(site), [100.0, 0.0, 0.0], Some([s, s, 0.0]));
    let storey = b.local_placement(Some(building), [0.0, 0.0, 3000.0], None);
    let pl = b.local_placement(Some(storey), [500.0, 500.0, 0.0], None);
    let prof = b.rect(1000.0, 1000.0, [0.0, 0.0], None);
    let ex = b.extrusion(prof, None, [0.0, 0.0, 1.0], 1000.0);
    let sh = b.shape("Body", "SweptSolid", &[ex]);
    b.proxy("G-chain", pl, Some(sh));

    if odd {
        odd_cases(&mut b);
    }
    // L: an Axis representation next to a body.
    let pl = b.local_placement(None, [0.0, 130000.0, 0.0], None);
    let axis = b.polyline(&[[0.0, 0.0], [1000.0, 0.0]]);
    let prof = b.rect(1000.0, 1000.0, [0.0, 0.0], None);
    let ex = b.extrusion(prof, None, [0.0, 0.0, 1.0], 1000.0);
    let sh = b.shape_multi(&[("Axis", "Curve2D", vec![axis]), ("Body", "SweptSolid", vec![ex])]);
    b.proxy("L-axis-and-body", pl, Some(sh));
    b.finish()
}

fn odd_cases(b: &mut B) {
    // H: only a bounding box; I: an item the importer does not read; J: no representation;
    // K: a placement it cannot resolve; L: an Axis representation next to a body.
    let corner = b.pt3(100.0, 200.0, 300.0);
    let bb = b.add(format!("IFCBOUNDINGBOX(#{corner},1000.,2000.,3000.)"));
    let sh = b.shape("Box", "BoundingBox", &[bb]);
    let pl = b.local_placement(None, [0.0, 100000.0, 0.0], None);
    b.proxy("H-box-only", pl, Some(sh));
    let p1 = b.rect(1000.0, 1000.0, [0.0, 0.0], None);
    let e1 = b.extrusion(p1, None, [0.0, 0.0, 1.0], 1000.0);
    let e2 = b.extrusion(p1, None, [0.0, 0.0, 1.0], 500.0);
    let cl = b.add(format!("IFCBOOLEANCLIPPINGRESULT(.DIFFERENCE.,#{e1},#{e2})"));
    let sh = b.shape("Body", "Clipping", &[cl]);
    let pl = b.local_placement(None, [0.0, 110000.0, 0.0], None);
    b.proxy("I-unread-item", pl, Some(sh));
    let pl = b.local_placement(None, [0.0, 120000.0, 0.0], None);
    b.proxy("J-no-representation", pl, None);
    let grid = b.add("IFCGRIDPLACEMENT($,$)".to_string());
    let prof = b.rect(1000.0, 1000.0, [0.0, 0.0], None);
    let ex = b.extrusion(prof, None, [0.0, 0.0, 1.0], 1000.0);
    let sh = b.shape("Body", "SweptSolid", &[ex]);
    b.proxy("K-unresolved-placement", grid, Some(sh));
}
