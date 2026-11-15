//! The alignment export and the linear placement, without any toolkit: what is written, that the
//! default is what it always was, and the round trip through the crate's own importer.

mod common;

use std::collections::BTreeMap;

use afrobim_alignment::{CATEGORY, Pvi};
use afrobim_ifc::import::{import_ifc_bytes, ImportOptions, ReferenceModel};
use afrobim_ifc::{export_ifc_model, AlignmentExport, IfcExport, IfcOptions};

use common::linear::{linear_fixture, LinearFixture};
use common::roads::road_fixture;
use common::site::site_fixture;
use common::options;

fn export(f: &LinearFixture, mode: AlignmentExport) -> IfcExport {
    export_ifc_model(&f.document, &f.inputs(), &IfcOptions { alignment: mode, ..options() })
}

fn count(text: &str, entity: &str) -> usize {
    text.lines().filter(|l| l.split_once('=').is_some_and(|(_, rest)| rest.starts_with(&format!("{entity}(")))).count()
}

fn lines_of<'a>(text: &'a str, entity: &str) -> Vec<&'a str> {
    text.lines().filter(|l| l.split_once('=').is_some_and(|(_, rest)| rest.starts_with(&format!("{entity}(")))).collect()
}

#[test]
fn the_default_writes_a_polyline_and_absolute_placements() {
    let f = linear_fixture();
    let default = export_ifc_model(&f.document, &f.inputs(), &options());
    let simple = export(&f, AlignmentExport::Simple);
    assert_eq!(default.text, simple.text, "Simple is the default");
    assert_eq!(IfcOptions::default().alignment, AlignmentExport::Simple);
    assert_eq!(count(&simple.text, "IFCLINEARPLACEMENT"), 0);
    assert_eq!(count(&simple.text, "IFCGRADIENTCURVE") + count(&simple.text, "IFCCOMPOSITECURVE") + count(&simple.text, "IFCCURVESEGMENT"), 0);
    assert_eq!(count(&simple.text, "IFCINDEXEDPOLYCURVE"), 2);
    assert_eq!(count(&simple.text, "IFCALIGNMENTVERTICAL"), 0);
    // Whatever the document, the default text is the text of an export that does not know the option.
    let site = site_fixture();
    let a = export_ifc_model(&site.document, &site.inputs(), &options());
    let b = export_ifc_model(&site.document, &site.inputs(), &IfcOptions { alignment: AlignmentExport::Simple, ..options() });
    assert_eq!(a.text, b.text);
}

#[test]
fn the_geometry_mode_writes_the_layouts_and_the_curves_of_the_documentation() {
    let f = linear_fixture();
    let e = export(&f, AlignmentExport::Geometry);
    let t = &e.text;
    assert_eq!(count(t, "IFCALIGNMENT"), 2);
    assert_eq!(count(t, "IFCALIGNMENTHORIZONTAL"), 2);
    assert_eq!(count(t, "IFCALIGNMENTVERTICAL"), 1, "only the alignment with a profile has a vertical layout");
    assert_eq!(count(t, "IFCCOMPOSITECURVE"), 2);
    assert_eq!(count(t, "IFCGRADIENTCURVE"), 1);
    assert_eq!(count(t, "IFCINDEXEDPOLYCURVE"), 0, "the polyline is replaced by the curves");
    // A curve segment per layout segment, and the zero-length one: 11 + 1 horizontal and 8 + 1 vertical for LN01, 5 + 1 for LN02.
    assert_eq!(count(t, "IFCALIGNMENTSEGMENT"), 12 + 9 + 6);
    assert_eq!(count(t, "IFCCURVESEGMENT"), 12 + 9 + 6);
    assert_eq!(count(t, "IFCALIGNMENTHORIZONTALSEGMENT"), 12 + 6);
    assert_eq!(count(t, "IFCALIGNMENTVERTICALSEGMENT"), 9);
    // Parent curves: lines, circles, clothoids, polynomials.
    assert_eq!(count(t, "IFCCLOTHOID"), 5 + 2, "every spiral");
    assert!(count(t, "IFCPOLYNOMIALCURVE") >= 3);
    // The last curve segment of each curve is a zero-length, discontinuous one; no other is discontinuous.
    let discontinuous = lines_of(t, "IFCCURVESEGMENT").into_iter().filter(|l| l.contains(".DISCONTINUOUS.")).count();
    assert_eq!(discontinuous, 3, "one per curve");
    for l in lines_of(t, "IFCCURVESEGMENT").into_iter().filter(|l| l.contains(".DISCONTINUOUS.")) {
        assert!(l.contains("IFCLENGTHMEASURE(0.),IFCLENGTHMEASURE(0.)"), "{l}");
    }
    // Representations: 'Axis' / 'Curve3D' with a 'FootPrint' / 'Curve2D' for LN01, 'Axis' / 'Curve2D' for LN02, a segment representation each.
    assert_eq!(count(t, "IFCGEOMETRICREPRESENTATIONSUBCONTEXT"), 3, "Body, Axis and FootPrint");
    assert_eq!(lines_of(t, "IFCSHAPEREPRESENTATION").iter().filter(|l| l.contains("'Axis','Curve3D'")).count(), 1);
    assert_eq!(lines_of(t, "IFCSHAPEREPRESENTATION").iter().filter(|l| l.contains("'FootPrint','Curve2D'")).count(), 1);
    assert_eq!(lines_of(t, "IFCSHAPEREPRESENTATION").iter().filter(|l| l.contains("'Axis','Curve2D'")).count(), 1);
    assert_eq!(lines_of(t, "IFCSHAPEREPRESENTATION").iter().filter(|l| l.contains("'Axis','Segment'")).count(), 12 + 9 + 6);
    // Stationing referents: a start station each and one per equation (4 on LN01, 2 on LN02), and 8 superelevation events.
    assert_eq!(lines_of(t, "IFCREFERENT").iter().filter(|l| l.contains(".STATION.")).count(), 2 + 4 + 2);
    assert_eq!(lines_of(t, "IFCREFERENT").iter().filter(|l| l.contains(".SUPERELEVATIONEVENT.")).count(), 8);
    assert_eq!(count(t, "IFCALIGNMENTCANT") + count(t, "IFCALIGNMENTCANTSEGMENT") + count(t, "IFCSEGMENTEDREFERENCECURVE"), 0, "rail cant is not road superelevation");
    assert_eq!(count(t, "IFCRELREFERENCEDINSPATIALSTRUCTURE"), 1);
    // Elements keep their absolute placements: the only linear placements are the referents'.
    assert_eq!(count(t, "IFCLINEARPLACEMENT"), 2 + 4 + 2 + 8);
    assert!(e.notes.iter().all(|n| !n.contains("placed by IfcLinearPlacement")));
    assert!(e.notes.iter().any(|n| n.contains("8 superelevation event(s)")));
}

#[test]
fn the_linear_mode_places_the_elements_that_have_a_station_and_keeps_the_others() {
    let f = linear_fixture();
    let e = export(&f, AlignmentExport::Linear);
    let t = &e.text;
    let referents = 2 + 4 + 2 + 8;
    // ADS-201 and REG-202 (assemblies), MKR-203, LOOSE-P2, MK-204, WRN-205 on LN01, REG-209 on LN02.
    assert_eq!(count(t, "IFCLINEARPLACEMENT"), referents + 7);
    assert!(e.notes.iter().any(|n| n.contains("7 element(s) are placed by IfcLinearPlacement")), "{:?}", e.notes);
    assert!(e.notes.iter().any(|n| n.contains("station 9+000.000 is not on alignment 'LN01'")), "{:?}", e.notes);
    // No Axis and no RefDirection: the axes are those of the curve.
    for l in lines_of(t, "IFCAXIS2PLACEMENTLINEAR") {
        assert!(l.ends_with(",$,$);"), "{l}");
    }
    // Every product's linear placement is relative to the alignment's own placement and carries the fallback.
    for l in lines_of(t, "IFCLINEARPLACEMENT") {
        let args: Vec<&str> = l.split_once('(').unwrap().1.trim_end_matches(");").split(',').collect();
        assert_eq!(args.len(), 3, "{l}");
        assert!(args.iter().all(|a| a.starts_with('#')), "{l}");
    }
    // The basis curves are the alignments' own.
    let gradient: Vec<&str> = lines_of(t, "IFCGRADIENTCURVE");
    assert_eq!(gradient.len(), 1);
    let gradient_id = gradient[0].split('=').next().unwrap();
    let on_gradient = lines_of(t, "IFCPOINTBYDISTANCEEXPRESSION").iter().filter(|l| l.ends_with(&format!(",{gradient_id});"))).count();
    // Seven referents of LN01 (the start, the equations and the superelevation events) are placed
    // on the gradient curve where it covers them, the rest and LN02's on the 2D curve.
    assert!(on_gradient >= 6 + 8, "{on_gradient}");
    // The absolute ones: REG-206 (no station), REG-207 (a station beyond the end), REG-208 (no placement) stay local placements.
    for name in ["REG-206", "REG-207", "REG-208"] {
        let id = f.document.elements().iter().find(|e| e.name == name).unwrap().id;
        let guid = afrobim_ifc::element_global_id(&options().project_name, id);
        let line = t.lines().find(|l| l.contains(&format!("'{guid}'")) && l.contains("IFCSIGN(")).unwrap();
        let placement_id = line.split(',').nth(5).unwrap();
        let placement = t.lines().find(|l| l.starts_with(&format!("{placement_id}="))).unwrap();
        assert!(placement.contains("IFCLOCALPLACEMENT"), "{name}: {placement}");
    }
}

fn world_bounds(model: &ReferenceModel) -> BTreeMap<String, ([f64; 3], [f64; 3])> {
    model.products.iter().filter_map(|p| p.bounds.map(|b| (p.global_id.clone(), b))).collect()
}

fn read(text: &str) -> ReferenceModel {
    import_ifc_bytes(text.as_bytes().to_vec(), &ImportOptions::default()).expect("the export imports")
}

fn worst_difference(a: &BTreeMap<String, ([f64; 3], [f64; 3])>, b: &BTreeMap<String, ([f64; 3], [f64; 3])>) -> f64 {
    assert_eq!(a.keys().collect::<Vec<_>>(), b.keys().collect::<Vec<_>>(), "the same products have geometry");
    let mut worst: f64 = 0.0;
    for (k, (lo, hi)) in a {
        let (lo2, hi2) = b[k];
        for i in 0..3 {
            worst = worst.max((lo[i] - lo2[i]).abs()).max((hi[i] - hi2[i]).abs());
        }
    }
    worst
}

#[test]
fn a_round_trip_gives_the_same_world_positions() {
    let f = linear_fixture();
    let absolute = read(&export(&f, AlignmentExport::Geometry).text);
    let linear_text = export(&f, AlignmentExport::Linear).text;
    let linear = read(&linear_text);
    let placements = count(&linear_text, "IFCLINEARPLACEMENT");
    assert_eq!(linear.report.linear_placements_resolved, placements, "every linear placement is evaluated on its curve");
    assert_eq!(linear.report.linear_placements_fallback, 0);
    assert!(linear.report.placements_unresolved.is_empty(), "{:?}", linear.report.placements_unresolved);
    assert_eq!(absolute.report.linear_placements_resolved, count(&export(&f, AlignmentExport::Geometry).text, "IFCLINEARPLACEMENT"), "the referents of the Geometry mode are evaluated too");
    let (a, l) = (world_bounds(&absolute), world_bounds(&linear));
    assert!(a.len() >= 12, "{} products with geometry", a.len());
    let worst = worst_difference(&a, &l);
    println!("round trip: {} products with geometry, worst difference {worst:.2e} mm, {placements} linear placements evaluated", a.len());
    assert!(worst < 1e-4, "{worst}");
}

#[test]
fn a_curve_the_importer_does_not_read_falls_back_to_the_cartesian_position() {
    let f = linear_fixture();
    let absolute = read(&export(&f, AlignmentExport::Geometry).text);
    // The clothoids become something the importer does not know: every placement on a curve that
    // has one is no longer evaluated and takes its fallback, which must be the frame of the curve.
    let text = export(&f, AlignmentExport::Linear).text.replace("=IFCCLOTHOID(", "=IFCSINESPIRAL(");
    let linear = read(&text);
    assert!(linear.report.linear_placements_fallback > 0);
    assert_eq!(linear.report.linear_placements_resolved, 0, "every curve has a clothoid");
    assert!(linear.report.warnings.iter().any(|w| w.contains("CartesianPosition used")));
    let worst = worst_difference(&world_bounds(&absolute), &world_bounds(&linear));
    println!("fallback: worst difference {worst:.2e} mm");
    assert!(worst < 1e-4, "{worst}");
}

#[test]
fn every_kind_of_road_module_geometry_survives_the_round_trip() {
    let mut f = road_fixture();
    let ids: Vec<_> = f.document.elements().iter().filter(|e| e.site.is_some() && e.category != CATEGORY && e.category != "surface").map(|e| e.id).collect();
    for id in ids {
        let s = f.document.find_mut(id).unwrap().site.as_mut().unwrap();
        s.alignment = Some("MC10".into());
        s.station_m = Some(10.0);
        s.offset_m = Some(0.0);
    }
    let go = |mode| export_ifc_model(&f.document, &f.inputs(), &IfcOptions { alignment: mode, ..options() }).text;
    let (absolute, linear_text) = (go(AlignmentExport::Geometry), go(AlignmentExport::Linear));
    assert!(count(&linear_text, "IFCLINEARPLACEMENT") >= 18);
    let linear = read(&linear_text);
    assert_eq!(linear.report.linear_placements_fallback, 0);
    let worst = worst_difference(&world_bounds(&read(&absolute)), &world_bounds(&linear));
    println!("road modules: worst difference {worst:.2e} mm");
    assert!(worst < 1e-4, "{worst}");
}

#[test]
fn an_alignment_whose_profile_covers_only_part_of_it_is_placed_on_the_2d_curve_outside() {
    let mut f = linear_fixture();
    // Profile from 2+100 to 2+500 only.
    let id = f.document.elements().iter().find(|e| e.name == "LN01").unwrap().id;
    let mut a = f.alignment.clone();
    a.profile = Some(afrobim_alignment::Profile::new("FG", vec![Pvi::point(2100.0, 100.0), Pvi::parabola(2300.0, 104.0, 100.0), Pvi::point(2500.0, 101.0)]).unwrap());
    f.document.find_mut(id).unwrap().data = serde_json::to_value(&a).unwrap();
    let e = export(&f, AlignmentExport::Linear);
    assert!(e.notes.iter().any(|n| n.contains("the profile covers stations 2+100.000 to 2+500.000")), "{:?}", e.notes);
    let linear = read(&e.text);
    let absolute = read(&export(&f, AlignmentExport::Geometry).text);
    assert_eq!(linear.report.linear_placements_fallback, 0);
    let worst = worst_difference(&world_bounds(&absolute), &world_bounds(&linear));
    println!("partial profile: worst difference {worst:.2e} mm");
    assert!(worst < 1e-4, "{worst}");
}

#[test]
fn the_export_is_deterministic_in_every_mode() {
    let f = linear_fixture();
    for mode in [AlignmentExport::Simple, AlignmentExport::Geometry, AlignmentExport::Linear] {
        assert_eq!(export(&f, mode).text, export(&f, mode).text);
    }
}

fn fnv1a(bytes: &[u8]) -> u64 {
    bytes.iter().fold(0xcbf2_9ce4_8422_2325, |h, b| (h ^ u64::from(*b)).wrapping_mul(0x0100_0000_01b3))
}

/// The files the exporter wrote for these documents before the option existed (commit 0f2351a,
/// parent of the one that adds it), as length and FNV-1a 64 of the text. `IfcOptions::default()`
/// must still write them byte for byte.
///
/// One exception, in `roads`: the two earthworks totals (`TotalCut`, `TotalFill`) are now written
/// to thirteen significant digits (`step_measure`), because their last bit differed between
/// platforms and this test failed on arm64 macOS. The file is otherwise the one 0f2351a wrote,
/// which was 291 669 bytes with hash 0xb0c4_eb33_62ce_e533 on x86-64 Linux.
#[test]
fn the_default_export_is_byte_for_byte_what_the_previous_version_wrote() {
    let sheet = common::fixture();
    let site = site_fixture();
    let roads = road_fixture();
    let got = [
        ("sheet", afrobim_ifc::export_ifc(&sheet.document, &sheet.inputs(), &options()).text),
        ("sheet-model", export_ifc_model(&sheet.document, &sheet.inputs(), &options()).text),
        ("site", export_ifc_model(&site.document, &site.inputs(), &options()).text),
        ("roads", export_ifc_model(&roads.document, &roads.inputs(), &options()).text),
    ];
    let want = [("sheet", 14737, 0x8d58_2f06_6048_a17c_u64), ("sheet-model", 13882, 0xeed6_fb76_5b14_1a36), ("site", 86004, 0x5f29_0b57_d9e7_5d36), ("roads", 291_661, 0xa9b5_f2f4_4ce4_c3ea)];
    for ((name, text), (wname, len, hash)) in got.iter().zip(want) {
        assert_eq!(*name, wname);
        assert_eq!((text.len(), fnv1a(text.as_bytes())), (len, hash), "{name} differs from what the previous version wrote");
    }
}
