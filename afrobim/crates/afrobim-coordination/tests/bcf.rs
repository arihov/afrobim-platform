//! Issues, clash grouping and BCF 2.1 exchange.
//!
//! The schema test needs the buildingSMART/BCF-XML `v2.1` schemas (directory in `BCF_XSD_DIR`,
//! default `/home/user/buildingsmart/v21/Schemas`) and `xmllint`; it is skipped without them. The
//! foreign-file test reads the 2.1 test cases (`BCF_TESTCASES_DIR`, default
//! `/home/user/buildingsmart/bcf-xml/Test Cases/v2.1`) and is skipped when absent.

use std::path::{Path, PathBuf};
use std::process::Command;

use afrobim_coordination::bcf::zip::{read_zip, write_zip, ZipEntry};
use afrobim_coordination::geom::*;
use afrobim_coordination::*;
use afrobim_draw::Mesh3;
use afrobim_ifc::element_global_id;
use afrobim_kernel::ElementId;

const PROJECT: &str = "A104 signs";
const PNG_HEAD: [u8; 8] = [0x89, b'P', b'N', b'G', 0x0D, 0x0A, 0x1A, 0x0A];

fn options() -> BcfOptions {
    BcfOptions { project_name: PROJECT.into(), ifc_file_name: Some("model.ifc".into()), project_id: Some("proj-1".into()), project_label: "A104".into(), ..BcfOptions::default() }
}

fn import_options() -> ImportOptions {
    ImportOptions { project_name: PROJECT.into(), known_ids: (1..=20).map(ElementId).collect() }
}

fn box_body(id: u64, min: V3, max: V3) -> Body {
    Body::new(ElementId(id), &format!("M{id}"), "any", "0", None, vec![Mesh3::cuboid("Box", min, max, [128; 3])])
}

fn clashes() -> Coordination {
    let mut co = Coordination { tests: vec![ClashTest::hard("T", Selection::all(), Selection::all())], ..Coordination::default() };
    co.run(&Scene::new(vec![box_body(1, [0.0; 3], [100.0; 3]), box_body(2, [50.0; 3], [150.0; 3]), box_body(3, [120.0, 0.0, 0.0], [220.0, 100.0, 100.0])]));
    co
}

fn rich_issues() -> Vec<Issue> {
    let mut co = clashes();
    assert!(co.results.clashes.len() >= 2);
    let cid = co.results.clashes[0].id.clone();
    let guid = co.raise_issue_from_clash(&cid, "ann@example.org", 1_700_000_000).unwrap();
    {
        let i = co.issue_mut(&guid).unwrap();
        i.priority = values::PRIORITY_HIGH.into();
        i.assignee = "bob@example.org".into();
        i.due = Some(1_701_000_000);
        i.labels = vec!["Structure".into(), "Drainage".into()];
        i.description.push_str(" <needs> \"quotes\" & ampersands");
        i.add_comment("bob@example.org", 1_700_000_100, "Looking at it, line one\nline two");
        let vp = i.add_location_viewpoint([-1.0, -1.0, 0.7], 5000.0, Projection::Perspective { fov_deg: 55.0 }).unwrap();
        vp.snapshot_png = Some([PNG_HEAD.to_vec(), vec![1, 2, 3, 4, 5]].concat());
        vp.visibility = Visibility::Except(vec![ElementRef::element(ElementId(3), "M3")]);
        let vp2 = i.add_viewpoint(Some(Camera { position: [1000.0, 2000.0, 3000.0], direction: [0.0, 0.0, -1.0], up: [0.0, 1.0, 0.0], projection: Projection::Orthogonal { view_to_world_scale_mm: 12_000.0 } }));
        vp2.visibility = Visibility::Only(vec![ElementRef::element(ElementId(1), "M1"), ElementRef::foreign("2TaLqCNHvEn9_7cUVrypdX", "Pipe in the reference model")]);
        let vguid = i.viewpoints[0].guid.clone();
        i.comments[0].viewpoint = Some(vguid);
        i.set_status(values::STATUS_IN_PROGRESS, "bob@example.org", 1_700_000_200);
    }
    let free = co.new_issue("Sign face shows the wrong route", "carol", 1_700_000_300);
    free.elements.push(ElementRef::element(ElementId(7), "ADS-007"));
    free.location = Some([12_345.678, -9_876.543, 1_500.0]);
    free.add_viewpoint(None).selected.push(ElementRef::element(ElementId(7), "ADS-007"));
    co.new_issue("Plain note", "dave", 1_700_000_400);
    co.issues
}

fn tmp(label: &str) -> PathBuf {
    let d = std::env::temp_dir().join(format!("afrobim-bcf-{label}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&d);
    std::fs::create_dir_all(&d).unwrap();
    d
}

#[test]
fn export_then_import_gives_the_same_issues() {
    let issues = rich_issues();
    let out = export_bcf(&issues, &options()).unwrap();
    assert!(out.notes.is_empty(), "{:?}", out.notes);
    let back = import_bcf(&out.bytes, &import_options()).unwrap();
    assert_eq!(back.issues.len(), issues.len());
    for (a, b) in issues.iter().zip(&back.issues) {
        assert_eq!(a, b, "issue {} differs", a.title);
    }
    let r = &back.report;
    assert_eq!((r.version.as_str(), r.topics, r.comments, r.viewpoints, r.snapshots), ("2.1", 3, 1, 3, 1));
    assert!(r.warnings.is_empty(), "{:?}", r.warnings);
    // project.bcfp is not imported and is not reported as unknown either.
    assert!(r.skipped.is_empty(), "{:?}", r.skipped);
    assert_eq!(r.components_foreign, 1);
}

#[test]
fn export_is_reproducible_and_reimport_is_stable() {
    let issues = rich_issues();
    let a = export_bcf(&issues, &options()).unwrap().bytes;
    assert_eq!(a, export_bcf(&issues, &options()).unwrap().bytes);
    let back = import_bcf(&a, &import_options()).unwrap();
    assert_eq!(export_bcf(&back.issues, &options()).unwrap().bytes, a);
}

#[test]
fn components_carry_the_global_ids_the_ifc_exporter_assigns() {
    let issues = rich_issues();
    let bytes = export_bcf(&issues, &options()).unwrap().bytes;
    let entries = read_zip(&bytes).unwrap();
    let from_clash = &issues[0];
    let vp = entries.iter().find(|e| e.name == format!("{}/viewpoint.bcfv", from_clash.guid)).unwrap();
    let text = String::from_utf8(vp.data.clone()).unwrap();
    let doc = roxmltree::Document::parse(&text).unwrap();
    let guids: Vec<&str> = doc.descendants().filter(|n| n.tag_name().name() == "Component").filter_map(|n| n.attribute("IfcGuid")).collect();
    for id in [1u64, 2, 3] {
        let want = element_global_id(PROJECT, ElementId(id));
        assert!(guids.contains(&want.as_str()), "{want} missing from {guids:?}");
    }
    // The authoring-tool id is the element id.
    assert!(text.contains("<AuthoringToolId>1</AuthoringToolId>"));
    assert!(text.contains("<OriginatingSystem>AfroBIM</OriginatingSystem>"));
}

#[test]
fn camera_is_written_in_metres_with_unit_vectors() {
    let issues = rich_issues();
    let bytes = export_bcf(&issues, &options()).unwrap().bytes;
    let entries = read_zip(&bytes).unwrap();
    let vp = entries.iter().find(|e| e.name == format!("{}/viewpoint-1.bcfv", issues[0].guid)).unwrap();
    let doc = roxmltree::Document::parse(std::str::from_utf8(&vp.data).unwrap()).unwrap();
    let ortho = doc.descendants().find(|n| n.tag_name().name() == "OrthogonalCamera").unwrap();
    let pos = ortho.children().find(|n| n.tag_name().name() == "CameraViewPoint").unwrap();
    let x: f64 = pos.children().find(|n| n.tag_name().name() == "X").unwrap().text().unwrap().parse().unwrap();
    assert_eq!(x, 1.0);
    let scale: f64 = ortho.children().find(|n| n.tag_name().name() == "ViewToWorldScale").unwrap().text().unwrap().parse().unwrap();
    assert_eq!(scale, 12.0);
}

#[test]
fn field_of_view_outside_the_schema_range_is_clamped_with_a_note() {
    let mut co = Coordination::default();
    let i = co.new_issue("x", "a", 0);
    i.add_viewpoint(Some(Camera::looking_at([0.0; 3], [0.0, -1.0, 0.0], 1000.0, Projection::Perspective { fov_deg: 90.0 })));
    let out = export_bcf(&co.issues, &options()).unwrap();
    assert_eq!(out.notes.len(), 1);
    assert!(out.notes[0].contains("45 to 60"));
    let back = import_bcf(&out.bytes, &import_options()).unwrap();
    // The file says 60; the AfroBIM extension file still holds the 90 the user set, and as the
    // rest of the camera matches the file, the import keeps the exact original.
    let Some(Camera { projection: Projection::Perspective { fov_deg }, .. }) = back.issues[0].viewpoints[0].camera.clone() else { panic!() };
    assert_eq!(fov_deg, 90.0);
}

#[test]
fn invalid_input_is_an_error_not_a_panic() {
    let mut co = Coordination::default();
    co.new_issue("a", "x", 0);
    co.issues[0].guid = "not-a-guid".into();
    assert!(matches!(export_bcf(&co.issues, &options()), Err(BcfError::InvalidGuid(_))));
    let mut co = Coordination::default();
    co.new_issue("a", "x", 0);
    let dup = co.issues[0].clone();
    co.issues.push(dup);
    assert!(matches!(export_bcf(&co.issues, &options()), Err(BcfError::DuplicateGuid(_))));
    assert!(import_bcf(b"hello", &import_options()).is_err());
    assert!(import_bcf(&[], &import_options()).is_err());
}

fn foreign_markup(guid: &str, extra_topic: &str, extra_root: &str) -> String {
    format!(
        r#"<?xml version="1.0" encoding="utf-8"?>
<Markup xmlns:xsi="http://www.w3.org/2001/XMLSchema-instance" xmlns:xsd="http://www.w3.org/2001/XMLSchema">
  <Header><File IfcProject="0K$ZpXG3b8KBnwUpWdjrBX" IfcSpatialStructureElement="2TaLqCNHvEn9_7cUVrypdX" isExternal="false" Vendor="x"><Filename>a.ifc</Filename><Date>2020-01-01T00:00:00Z</Date><Reference>../a.ifc</Reference></File></Header>
  <Topic Guid="{guid}" TopicType="Issue" TopicStatus="Open" Custom="1">
    <ReferenceLink>https://example.org/1</ReferenceLink>
    <Title>Foreign topic</Title>
    <Priority>Normal</Priority>
    <Index>1</Index>
    <Labels>Architecture</Labels>
    <CreationDate>2020-05-17T10:00:00+02:00</CreationDate>
    <CreationAuthor>someone@example.org</CreationAuthor>
    <AssignedTo>other@example.org</AssignedTo>
    <Stage>Construction Start</Stage>
    <Description>Wall &amp; slab</Description>
    <RelatedTopic Guid="11111111-2222-3333-4444-555555555555"/>
    {extra_topic}
  </Topic>
  <Comment Guid="aaaaaaaa-bbbb-cccc-dddd-eeeeeeeeeeee"><Date>2020-05-18T10:00:00Z</Date><VerbalStatus>x</VerbalStatus><Status>y</Status><Author>a</Author><Comment>hello</Comment><Viewpoint Guid="99999999-2222-3333-4444-555555555555"/></Comment>
  <Viewpoints Guid="99999999-2222-3333-4444-555555555555"><Viewpoint>vp.bcfv</Viewpoint><Snapshot>snap.png</Snapshot><Index>0</Index></Viewpoints>
  {extra_root}
</Markup>"#
    )
}

const FOREIGN_VP: &str = r#"<?xml version="1.0" encoding="UTF-8"?>
<VisualizationInfo Guid="99999999-2222-3333-4444-555555555555">
  <Components>
    <ViewSetupHints SpacesVisible="false" OpeningsVisible="true"/>
    <Selection><Component IfcGuid="0K$ZpXG3b8KBnwUpWdjrBX"><OriginatingSystem>Revit</OriginatingSystem><AuthoringToolId>123</AuthoringToolId></Component></Selection>
    <Visibility DefaultVisibility="false"><Exceptions><Component IfcGuid="2TaLqCNHvEn9_7cUVrypdX"/></Exceptions></Visibility>
    <Coloring><Color Color="FF0000"><Component IfcGuid="2TaLqCNHvEn9_7cUVrypdX"/></Color></Coloring>
  </Components>
  <PerspectiveCamera>
    <CameraViewPoint><X>1.5</X><Y>2</Y><Z>3</Z></CameraViewPoint>
    <CameraDirection><X>0</X><Y>1</Y><Z>0</Z></CameraDirection>
    <CameraUpVector><X>0</X><Y>0</Y><Z>1</Z></CameraUpVector>
    <FieldOfView>60</FieldOfView>
  </PerspectiveCamera>
  <Lines><Line><StartPoint><X>0</X><Y>0</Y><Z>0</Z></StartPoint><EndPoint><X>1</X><Y>1</Y><Z>1</Z></EndPoint></Line></Lines>
  <ClippingPlanes><ClippingPlane><Location><X>0</X><Y>0</Y><Z>0</Z></Location><Direction><X>0</X><Y>0</Y><Z>1</Z></Direction></ClippingPlane></ClippingPlanes>
</VisualizationInfo>"#;

#[test]
fn a_foreign_file_is_read_and_what_is_skipped_is_reported() {
    let topic = "4d1a2b3c-aaaa-bbbb-cccc-000000000001";
    let bcfv = FOREIGN_VP.as_bytes().to_vec();
    let entries = vec![
        ZipEntry { name: "bcf.version".into(), data: br#"<?xml version="1.0"?><Version VersionId="2.1"><DetailedVersion>2.1</DetailedVersion></Version>"#.to_vec() },
        ZipEntry { name: "project.bcfp".into(), data: b"<ProjectExtension/>".to_vec() },
        ZipEntry { name: "extensions.xsd".into(), data: b"<x/>".to_vec() },
        ZipEntry { name: format!("{topic}/markup.bcf"), data: foreign_markup(topic, "<Mystery>1</Mystery>", "<Unknown/>").into_bytes() },
        ZipEntry { name: format!("{topic}/vp.bcfv"), data: bcfv },
        ZipEntry { name: format!("{topic}/snap.png"), data: [PNG_HEAD.to_vec(), vec![9; 20]].concat() },
        ZipEntry { name: format!("{topic}/notes.docx"), data: vec![1, 2, 3] },
    ];
    let bytes = write_zip(&entries).unwrap();
    let got = import_bcf(&bytes, &import_options()).unwrap();
    assert_eq!(got.issues.len(), 1);
    let i = &got.issues[0];
    assert_eq!((i.title.as_str(), i.issue_type.as_str(), i.status.as_str(), i.priority.as_str()), ("Foreign topic", "Issue", "Open", "Normal"));
    assert_eq!(i.description, "Wall & slab");
    assert_eq!(i.assignee, "other@example.org");
    assert_eq!(i.labels, vec!["Architecture"]);
    // 2020-05-17T10:00:00+02:00 = 08:00 UTC.
    assert_eq!(i.created, 1_589_702_400);
    assert_eq!(i.comments.len(), 1);
    assert_eq!(i.comments[0].viewpoint.as_deref(), Some("99999999-2222-3333-4444-555555555555"));
    let vp = &i.viewpoints[0];
    assert!(vp.snapshot_png.is_some());
    let cam = vp.camera.as_ref().unwrap();
    assert_eq!(cam.position, [1500.0, 2000.0, 3000.0]);
    assert_eq!(cam.projection, Projection::Perspective { fov_deg: 60.0 });
    assert_eq!(vp.selected, vec![ElementRef::foreign("0K$ZpXG3b8KBnwUpWdjrBX", "")]);
    assert_eq!(vp.visibility, Visibility::Only(vec![ElementRef::foreign("2TaLqCNHvEn9_7cUVrypdX", "")]));
    assert_eq!(i.elements, vp.selected);
    let skipped = got.report.skipped.join("\n");
    for needle in ["Mystery", "Unknown", "ReferenceLink", "Stage", "RelatedTopic", "Custom", "VerbalStatus", "Lines", "ClippingPlanes", "Coloring", "ViewSetupHints", "Vendor", "notes.docx", "extensions.xsd", "Reference"] {
        assert!(skipped.contains(needle), "{needle} not reported in:\n{skipped}");
    }
    assert!(got.report.warnings.is_empty(), "{:?}", got.report.warnings);
}

#[test]
fn damaged_topics_are_dropped_with_a_warning_and_the_rest_is_kept() {
    let good = "4d1a2b3c-aaaa-bbbb-cccc-000000000001";
    let bad = "4d1a2b3c-aaaa-bbbb-cccc-000000000002";
    let dtd = "4d1a2b3c-aaaa-bbbb-cccc-000000000003";
    let entries = vec![
        ZipEntry { name: "bcf.version".into(), data: br#"<Version VersionId="2.0"/>"#.to_vec() },
        ZipEntry { name: format!("{good}/markup.bcf"), data: foreign_markup(good, "", "").into_bytes() },
        ZipEntry { name: format!("{bad}/markup.bcf"), data: b"<Markup><Topic".to_vec() },
        ZipEntry { name: format!("{dtd}/markup.bcf"), data: b"<!DOCTYPE Markup [<!ENTITY a \"b\">]><Markup/>".to_vec() },
    ];
    let got = import_bcf(&write_zip(&entries).unwrap(), &import_options()).unwrap();
    assert_eq!(got.issues.len(), 1);
    assert_eq!(got.report.warnings.len(), 3, "{:?}", got.report.warnings);
    assert!(got.report.warnings.iter().any(|w| w.contains(bad)));
    // The viewpoint named in the markup is missing from this archive.
    assert!(got.report.warnings.iter().any(|w| w.contains("vp.bcfv")));
}

#[test]
fn bcf_3_is_refused_and_1_0_is_read_with_a_warning() {
    let three = write_zip(&[ZipEntry { name: "bcf.version".into(), data: br#"<Version VersionId="3.0"/>"#.to_vec() }]).unwrap();
    assert!(matches!(import_bcf(&three, &import_options()), Err(BcfError::Unsupported(_))));
    let one = write_zip(&[ZipEntry { name: "bcf.version".into(), data: br#"<Version VersionId="1.0"/>"#.to_vec() }]).unwrap();
    let got = import_bcf(&one, &import_options()).unwrap();
    assert!(got.issues.is_empty() && got.report.warnings.iter().any(|w| w.contains("1.0")));
}

#[test]
fn issues_persist_with_the_coordination_data() {
    let co = Coordination { issues: rich_issues(), ..clashes() };
    let json = co.to_json().unwrap();
    assert_eq!(Coordination::from_json(&json).unwrap(), co);
    // A document saved before issues existed still loads.
    let old = serde_json::json!({ "tests": [], "results": { "clashes": [] } }).to_string();
    assert!(Coordination::from_json(&old).unwrap().issues.is_empty());
    // The snapshot is stored as text, not as a list of numbers.
    assert!(json.contains("\"snapshot_png\": \"iVBORw0KGgo"));
}

#[test]
fn an_issue_from_a_clash_has_a_stable_guid_and_the_clash_data() {
    let mut co = clashes();
    let c = co.results.clashes[0].clone();
    let g1 = co.raise_issue_from_clash(&c.id, "ann", 10).unwrap();
    let g2 = co.raise_issue_from_clash(&c.id, "ann", 99).unwrap();
    assert_eq!(g1, g2);
    assert_eq!(co.issues.len(), 1);
    let i = co.issue(&g1).unwrap();
    assert_eq!(i.clashes[0].clash_id, c.id);
    assert_eq!(i.location, Some(c.point));
    assert_eq!(i.elements.iter().filter_map(|e| e.id).collect::<Vec<_>>(), vec![c.element_a, c.element_b]);
    assert!(co.raise_issue_from_clash("CL-nope", "a", 0).is_none());
    // Free-standing GUIDs are unique even for equal input.
    let a = co.new_issue("same", "x", 5).guid.clone();
    let b = co.new_issue("same", "x", 5).guid.clone();
    assert_ne!(a, b);
}

#[test]
fn merge_issues_replaces_by_guid() {
    let mut co = Coordination::default();
    co.issues = rich_issues();
    let mut again = co.issues.clone();
    again[0].title = "renamed".into();
    again.push(Issue::with_guid("00000000-0000-4000-8000-000000000001", "new", "x", 0));
    assert_eq!(co.merge_issues(again), (1, 3));
    assert_eq!(co.issues[0].title, "renamed");
    assert_eq!(co.issues.len(), 4);
}

#[test]
fn a_group_becomes_one_issue() {
    let co = clashes();
    let groups = group_clashes(&co.results.clashes, &GroupOptions::by_element());
    // Boxes 1-2, 1-3 (overlap in x 120..100? 1 and 3 do not overlap), 2-3 share element 2 or 3.
    let g = groups.iter().max_by_key(|g| g.clash_ids.len()).unwrap();
    let issue = Issue::from_group(g, &co.results.clashes, "ann", 0);
    assert_eq!(issue.clashes.len(), g.clash_ids.len());
    assert_eq!(issue.location, Some(g.centroid));
    let ids: Vec<u64> = issue.elements.iter().filter_map(|e| e.id).map(|i| i.0).collect();
    assert_eq!(ids.len(), g.elements.len());
    assert_eq!(issue.guid, Issue::from_group(g, &co.results.clashes, "other", 5).guid);
}

#[test]
fn zip_archives_read_with_python_and_unzip() {
    let dir = tmp("zip");
    let path = dir.join("t.bcf");
    std::fs::write(&path, export_bcf(&rich_issues(), &options()).unwrap().bytes).unwrap();
    let mut ran = false;
    if let Ok(o) = Command::new("python3").args(["-I", "-c", "import zipfile,sys; z=zipfile.ZipFile(sys.argv[1]); assert z.testzip() is None; print(len(z.namelist()), z.namelist()[0])"]).arg(&path).output() {
        if o.status.success() {
            ran = true;
            let text = String::from_utf8_lossy(&o.stdout);
            assert!(text.contains("bcf.version"), "{text}");
        } else {
            panic!("python zipfile rejected the archive: {}", String::from_utf8_lossy(&o.stderr));
        }
    }
    if let Ok(o) = Command::new("unzip").arg("-t").arg(&path).output() {
        assert!(o.status.success(), "unzip -t: {}", String::from_utf8_lossy(&o.stdout));
        ran = true;
    }
    eprintln!("external zip readers used: {ran}");
}

fn xsd_dir() -> Option<PathBuf> {
    let d = PathBuf::from(std::env::var("BCF_XSD_DIR").unwrap_or_else(|_| "/home/user/buildingsmart/v21/Schemas".into()));
    d.join("markup.xsd").exists().then_some(d)
}

fn xmllint_ok() -> bool {
    Command::new("xmllint").arg("--version").output().is_ok()
}

fn validate(xsd: &Path, file: &Path) -> Result<(), String> {
    let o = Command::new("xmllint").arg("--noout").arg("--schema").arg(xsd).arg(file).output().map_err(|e| e.to_string())?;
    if o.status.success() {
        Ok(())
    } else {
        Err(String::from_utf8_lossy(&o.stderr).to_string())
    }
}

#[test]
fn exported_xml_validates_against_the_bcf_2_1_schemas() {
    let (Some(xsd), true) = (xsd_dir(), xmllint_ok()) else {
        eprintln!("SKIPPED: xmllint or the BCF 2.1 schemas are missing");
        return;
    };
    let dir = tmp("xsd");
    let mut checked = 0;
    for (label, issues) in [("rich", rich_issues()), ("empty", Vec::new())] {
        let bytes = export_bcf(&issues, &options()).unwrap().bytes;
        for e in read_zip(&bytes).unwrap() {
            let (schema, ok) = match e.name.rsplit('/').next().unwrap() {
                "markup.bcf" => ("markup.xsd", true),
                "bcf.version" => ("version.xsd", true),
                "project.bcfp" => ("project.xsd", true),
                n if n.ends_with(".bcfv") => ("visinfo.xsd", true),
                _ => ("", false),
            };
            if !ok {
                continue;
            }
            let file = dir.join(format!("{label}-{checked}.xml"));
            std::fs::write(&file, &e.data).unwrap();
            if let Err(msg) = validate(&xsd.join(schema), &file) {
                panic!("{} fails {schema}: {msg}\n{}", e.name, String::from_utf8_lossy(&e.data));
            }
            checked += 1;
        }
    }
    eprintln!("validated {checked} XML files against the BCF 2.1 schemas with xmllint");
    assert!(checked >= 10);
}

#[test]
fn the_schema_validation_catches_a_wrong_file() {
    // Guards the test above: a broken markup must fail, so a pass means something.
    let (Some(xsd), true) = (xsd_dir(), xmllint_ok()) else {
        eprintln!("SKIPPED: xmllint or the BCF 2.1 schemas are missing");
        return;
    };
    let dir = tmp("xsd-neg");
    let file = dir.join("bad.xml");
    std::fs::write(&file, "<Markup><Topic Guid=\"x\"><Title>t</Title></Topic></Markup>").unwrap();
    assert!(validate(&xsd.join("markup.xsd"), &file).is_err());
}

#[test]
fn the_buildingsmart_2_1_test_cases_import() {
    let root = PathBuf::from(std::env::var("BCF_TESTCASES_DIR").unwrap_or_else(|_| "/home/user/buildingsmart/bcf-xml/Test Cases/v2.1".into()));
    if !root.exists() {
        eprintln!("SKIPPED: BCF 2.1 test cases not found");
        return;
    }
    let mut files = Vec::new();
    let mut stack = vec![root];
    while let Some(d) = stack.pop() {
        for e in std::fs::read_dir(&d).unwrap().flatten() {
            let p = e.path();
            if p.is_dir() {
                // The `unzipped` folders hold the same content as the archives.
                if p.file_name().and_then(|n| n.to_str()) != Some("unzipped") {
                    stack.push(p);
                }
            } else if matches!(p.extension().and_then(|x| x.to_str()), Some("bcf" | "bcfzip")) {
                files.push(p);
            }
        }
    }
    files.sort();
    assert!(!files.is_empty());
    let (mut topics, mut failed) = (0, Vec::new());
    for f in &files {
        let bytes = std::fs::read(f).unwrap();
        match import_bcf(&bytes, &ImportOptions::default()) {
            Ok(r) => {
                topics += r.issues.len();
                eprintln!("{}: {} topics, {} viewpoints, {} comments, {} skipped notes, {} warnings", f.file_name().unwrap().to_string_lossy(), r.issues.len(), r.report.viewpoints, r.report.comments, r.report.skipped.len(), r.report.warnings.len());
                // Re-export what was read and check it is a valid archive that reads back.
                let again = export_bcf(&r.issues, &options());
                match again {
                    Ok(x) => assert_eq!(import_bcf(&x.bytes, &ImportOptions { project_name: PROJECT.into(), known_ids: vec![] }).unwrap().issues.len(), r.issues.len()),
                    Err(e) => eprintln!("  not re-exportable: {e}"),
                }
            }
            Err(e) => failed.push(format!("{}: {e}", f.display())),
        }
    }
    eprintln!("{} files, {topics} topics", files.len());
    assert!(failed.is_empty(), "{failed:?}");
    assert!(topics >= files.len() / 2);
}
