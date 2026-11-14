//! BCF import. Reads 2.1 (and, with a warning, 2.0 and 1.x, whose markup this reader handles as
//! 2.1 and whose extra elements it reports as skipped). BCF 3.0 uses a different markup layout
//! and is refused. Unknown content is never an error: it is counted in the report.

use std::collections::{BTreeMap, HashMap};

use afrobim_ifc::element_global_id;
use afrobim_kernel::ElementId;
use roxmltree::{Document, Node};

use super::export::{is_guid, is_ifc_guid, BcfError, Sidecar, SIDECAR_FORMAT, SIDECAR_NAME};
use super::time::parse_datetime;
use super::zip::{read_zip, ZipEntry};
use crate::geom::{len, scale, V3};
use crate::issue::{Camera, ElementRef, Issue, IssueComment, Projection, Viewpoint, Visibility};

#[derive(Debug, Clone, Default, PartialEq)]
pub struct ImportOptions {
    /// The IFC project name the elements' GlobalIds were derived with
    /// (`IfcOptions::project_name`). Together with `known_ids` it maps GlobalIds in the file back
    /// to elements of this document.
    pub project_name: String,
    /// The ids of the document's elements. A component whose IfcGuid equals
    /// `element_global_id(project_name, id)` for one of these becomes a reference to that id;
    /// others are kept as foreign references (IFC GlobalId only).
    pub known_ids: Vec<ElementId>,
}

/// What an import read and what it did not.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct ImportReport {
    /// `VersionId` of `bcf.version`.
    pub version: String,
    pub topics: usize,
    pub comments: usize,
    pub viewpoints: usize,
    pub snapshots: usize,
    /// Components resolved to elements of this document / kept as foreign references.
    pub components_matched: usize,
    pub components_foreign: usize,
    /// Content that exists in the file and was not imported: elements and attributes outside the
    /// 2.1 vocabulary, parts of 2.1 the issue model has no place for (Lines, ClippingPlanes,
    /// Bitmap, Coloring, ReferenceLink, Stage, BimSnippet, DocumentReference, RelatedTopic, ...),
    /// and files that are not part of the format.
    pub skipped: Vec<String>,
    /// Problems with content that was imported (a bad date, a missing required element, ...).
    pub warnings: Vec<String>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct BcfImport {
    pub issues: Vec<Issue>,
    pub report: ImportReport,
}

struct Lookup {
    by_guid: HashMap<String, ElementId>,
    by_id: HashMap<ElementId, String>,
}

impl Lookup {
    fn new(o: &ImportOptions) -> Lookup {
        let by_id: HashMap<ElementId, String> = o.known_ids.iter().map(|id| (*id, element_global_id(&o.project_name, *id))).collect();
        Lookup { by_guid: by_id.iter().map(|(id, g)| (g.clone(), *id)).collect(), by_id }
    }
}

fn child<'a, 'i>(n: Node<'a, 'i>, name: &str) -> Option<Node<'a, 'i>> {
    n.children().find(|c| c.is_element() && c.tag_name().name() == name)
}

fn children<'a, 'i>(n: Node<'a, 'i>, name: &'a str) -> impl Iterator<Item = Node<'a, 'i>> {
    n.children().filter(move |c| c.is_element() && c.tag_name().name() == name)
}

fn text_of(n: Node, name: &str) -> Option<String> {
    child(n, name).map(|c| c.text().unwrap_or("").trim().to_string())
}

/// Counts the element children of `n` that are not in `handled` and unknown attributes not in
/// `attrs`, and records them as skipped under `ctx`.
fn tally(n: Node, ctx: &str, handled: &[&str], attrs: &[&str], report: &mut ImportReport) {
    let mut counts: BTreeMap<String, usize> = BTreeMap::new();
    for c in n.children().filter(|c| c.is_element()) {
        if !handled.contains(&c.tag_name().name()) {
            *counts.entry(c.tag_name().name().to_string()).or_default() += 1;
        }
    }
    for (name, k) in counts {
        report.skipped.push(format!("{ctx}: element {name} x{k} not imported"));
    }
    for a in n.attributes() {
        if !attrs.contains(&a.name()) {
            report.skipped.push(format!("{ctx}: attribute {} not imported", a.name()));
        }
    }
}

fn parse_doc(bytes: &[u8]) -> Result<String, String> {
    let bytes = bytes.strip_prefix(&[0xEF, 0xBB, 0xBF]).unwrap_or(bytes);
    String::from_utf8(bytes.to_vec()).map_err(|_| "not UTF-8 text (UTF-16 and other encodings are not supported)".to_string())
}

fn dbl(n: Node, name: &str) -> Option<f64> {
    text_of(n, name)?.parse().ok()
}

fn read_point(n: Node, name: &str) -> Option<V3> {
    let p = child(n, name)?;
    Some([dbl(p, "X")?, dbl(p, "Y")?, dbl(p, "Z")?])
}

fn mm(p: V3) -> V3 {
    scale(p, 1000.0)
}

fn norm(d: V3) -> V3 {
    let l = len(d);
    if l > 0.0 {
        scale(d, 1.0 / l)
    } else {
        d
    }
}

/// True when the sidecar camera is what the XML says (so the XML has not been edited since):
/// positions within a micrometre, directions within 1e-9, field of view as clamped on export.
fn same_camera(side: &Camera, xml: &Camera) -> bool {
    let close = |a: V3, b: V3, tol: f64| (0..3).all(|k| (a[k] - b[k]).abs() <= tol);
    let proj = match (side.projection, xml.projection) {
        (Projection::Perspective { fov_deg: a }, Projection::Perspective { fov_deg: b }) => (a.clamp(45.0, 60.0) - b).abs() < 1e-9,
        (Projection::Orthogonal { view_to_world_scale_mm: a }, Projection::Orthogonal { view_to_world_scale_mm: b }) => (a - b).abs() <= 1e-3,
        _ => false,
    };
    proj && close(side.position, xml.position, 1e-3) && close(norm(side.direction), norm(xml.direction), 1e-9) && close(norm(side.up), norm(xml.up), 1e-9)
}

fn read_camera(vi: Node, report: &mut ImportReport, tag: &str) -> Option<Camera> {
    let (node, perspective) = match (child(vi, "PerspectiveCamera"), child(vi, "OrthogonalCamera")) {
        (Some(p), o) => {
            if o.is_some() {
                report.skipped.push(format!("{tag}: OrthogonalCamera ignored, the viewpoint also has a PerspectiveCamera"));
            }
            (p, true)
        }
        (None, Some(o)) => (o, false),
        (None, None) => return None,
    };
    let (Some(pos), Some(dir), Some(up)) = (read_point(node, "CameraViewPoint"), read_point(node, "CameraDirection"), read_point(node, "CameraUpVector")) else {
        report.warnings.push(format!("{tag}: camera is missing CameraViewPoint, CameraDirection or CameraUpVector; camera dropped"));
        return None;
    };
    tally(node, &format!("{tag}: camera"), &["CameraViewPoint", "CameraDirection", "CameraUpVector", "FieldOfView", "ViewToWorldScale"], &[], report);
    let projection = if perspective {
        Projection::Perspective { fov_deg: dbl(node, "FieldOfView").unwrap_or_else(|| {
            report.warnings.push(format!("{tag}: PerspectiveCamera without FieldOfView; 50 assumed"));
            50.0
        }) }
    } else {
        Projection::Orthogonal { view_to_world_scale_mm: dbl(node, "ViewToWorldScale").map(|v| v * 1000.0).unwrap_or_else(|| {
            report.warnings.push(format!("{tag}: OrthogonalCamera without ViewToWorldScale; 0 assumed"));
            0.0
        }) }
    };
    Some(Camera { position: mm(pos), direction: dir, up, projection })
}

struct ComponentCtx<'a> {
    lookup: &'a Lookup,
    marks: HashMap<String, ElementRef>,
}

impl ComponentCtx<'_> {
    fn resolve(&self, guid: &str, report: &mut ImportReport) -> ElementRef {
        let known = self.marks.get(guid);
        let mark = known.map(|r| r.mark.clone()).unwrap_or_default();
        if let Some(id) = self.lookup.by_guid.get(guid) {
            report.components_matched += 1;
            // The GlobalId is derivable for an element of this document; keep it only if the
            // issue carried it explicitly.
            return ElementRef { id: Some(*id), mark, ifc_guid: known.and_then(|r| r.ifc_guid.clone()) };
        }
        report.components_foreign += 1;
        ElementRef { id: None, mark, ifc_guid: Some(guid.to_string()) }
    }

    fn list(&self, n: Node, tag: &str, report: &mut ImportReport) -> Vec<ElementRef> {
        let mut out = Vec::new();
        for c in children(n, "Component") {
            tally(c, &format!("{tag}: Component"), &["OriginatingSystem", "AuthoringToolId"], &["IfcGuid"], report);
            match c.attribute("IfcGuid") {
                Some(g) if is_ifc_guid(g) => out.push(self.resolve(g, report)),
                Some(g) => report.warnings.push(format!("{tag}: component IfcGuid {g:?} is not a 22 character GlobalId; left out")),
                None => report.skipped.push(format!("{tag}: component without IfcGuid left out")),
            }
        }
        out
    }
}

fn read_viewpoint(xml: &str, guid: &str, cc: &ComponentCtx, tag: &str, report: &mut ImportReport) -> Option<Viewpoint> {
    let doc = match Document::parse(xml) {
        Ok(d) => d,
        Err(e) => {
            report.warnings.push(format!("{tag}: viewpoint file is not well-formed XML ({e}); viewpoint dropped"));
            return None;
        }
    };
    let vi = doc.root_element();
    if vi.tag_name().name() != "VisualizationInfo" {
        report.warnings.push(format!("{tag}: viewpoint file root is {}, not VisualizationInfo; dropped", vi.tag_name().name()));
        return None;
    }
    tally(vi, tag, &["Components", "OrthogonalCamera", "PerspectiveCamera"], &["Guid"], report);
    let camera = read_camera(vi, report, tag);
    let mut vp = Viewpoint::new(guid.to_string(), camera);
    if let Some(c) = child(vi, "Components") {
        tally(c, &format!("{tag}: Components"), &["Selection", "Visibility"], &[], report);
        if let Some(s) = child(c, "Selection") {
            vp.selected = cc.list(s, tag, report);
        }
        if let Some(v) = child(c, "Visibility") {
            let default_visible = v.attribute("DefaultVisibility").map(|t| t == "true" || t == "1").unwrap_or(false);
            let ex = child(v, "Exceptions").map(|e| cc.list(e, tag, report)).unwrap_or_default();
            tally(v, &format!("{tag}: Visibility"), &["Exceptions"], &["DefaultVisibility"], report);
            vp.visibility = match (default_visible, ex.is_empty()) {
                (true, true) => Visibility::All,
                (true, false) => Visibility::Except(ex),
                (false, _) => Visibility::Only(ex),
            };
        }
    }
    Some(vp)
}

/// Reads a BCF archive.
pub fn import_bcf(bytes: &[u8], options: &ImportOptions) -> Result<BcfImport, BcfError> {
    let entries = read_zip(bytes)?;
    let by_name: HashMap<&str, &ZipEntry> = entries.iter().map(|e| (e.name.as_str(), e)).collect();
    let mut report = ImportReport::default();
    // Version.
    match by_name.get("bcf.version") {
        None => {
            report.warnings.push("no bcf.version; read as 2.1".into());
            report.version = "2.1".into();
        }
        Some(e) => {
            let text = parse_doc(&e.data).map_err(|m| BcfError::NotBcf(format!("bcf.version: {m}")))?;
            let doc = Document::parse(&text).map_err(|x| BcfError::NotBcf(format!("bcf.version: {x}")))?;
            let root = doc.root_element();
            let v = root.attribute("VersionId").map(str::to_string).or_else(|| text_of(root, "DetailedVersion")).unwrap_or_default();
            report.version = v.clone();
            if v.starts_with('3') {
                return Err(BcfError::Unsupported(format!("BCF {v}: only BCF 2.1 (and 2.0) markup is implemented")));
            }
            if v.starts_with('1') {
                report.warnings.push(format!("BCF {v}: read with the 2.1 rules; Comment Status and VerbalStatus are ignored"));
            } else if !v.starts_with("2.") {
                report.warnings.push(format!("unknown BCF version {v:?}; read as 2.1"));
            }
        }
    }
    let lookup = Lookup::new(options);
    // Topic folders: `<dir>/markup.bcf`.
    let mut folders: Vec<String> = Vec::new();
    for e in &entries {
        if let Some((dir, file)) = e.name.split_once('/') {
            if file.eq_ignore_ascii_case("markup.bcf") && !dir.is_empty() {
                folders.push(dir.to_string());
            }
        }
    }
    // Files outside the format.
    let mut stray: Vec<String> = Vec::new();
    for e in &entries {
        if e.name.ends_with('/') {
            continue;
        }
        match e.name.split_once('/') {
            None => {
                if !["bcf.version", "project.bcfp"].contains(&e.name.as_str()) {
                    stray.push(e.name.clone());
                }
            }
            Some((dir, file)) => {
                let lower = file.to_ascii_lowercase();
                let known = lower == "markup.bcf" || lower.ends_with(".bcfv") || lower.ends_with(".png") || lower.ends_with(".jpg") || file == SIDECAR_NAME;
                if !known || !folders.iter().any(|f| f == dir) {
                    stray.push(e.name.clone());
                }
            }
        }
    }
    for s in stray.iter().take(20) {
        report.skipped.push(format!("file {s} is not part of the topic format; ignored"));
    }
    if stray.len() > 20 {
        report.skipped.push(format!("and {} more files ignored", stray.len() - 20));
    }

    let mut issues: Vec<(Issue, Option<i64>)> = Vec::new();
    for dir in &folders {
        let wanted = format!("{dir}/markup.bcf");
        let Some(markup) = entries.iter().find(|e| e.name.eq_ignore_ascii_case(&wanted)) else { continue };
        if let Some((issue, index)) = read_topic(dir, markup, &by_name, &lookup, &mut report) {
            if issues.iter().any(|(i, _): &(Issue, Option<i64>)| i.guid == issue.guid) {
                report.warnings.push(format!("{dir}: topic GUID {} appears twice; second one dropped", issue.guid));
            } else {
                issues.push((issue, index));
            }
        }
    }
    // Topic order: by the Index element where present (stable, those without last), else archive order.
    issues.sort_by_key(|(_, index)| index.unwrap_or(i64::MAX));
    let issues: Vec<Issue> = issues.into_iter().map(|(i, _)| i).collect();
    report.topics = issues.len();
    report.comments = issues.iter().map(|i| i.comments.len()).sum();
    report.viewpoints = issues.iter().map(|i| i.viewpoints.len()).sum();
    report.snapshots = issues.iter().flat_map(|i| &i.viewpoints).filter(|v| v.snapshot_png.is_some()).count();
    Ok(BcfImport { issues, report })
}

fn read_topic(dir: &str, markup: &ZipEntry, files: &HashMap<&str, &ZipEntry>, lookup: &Lookup, report: &mut ImportReport) -> Option<(Issue, Option<i64>)> {
    let text = match parse_doc(&markup.data) {
        Ok(t) => t,
        Err(m) => {
            report.warnings.push(format!("{dir}/markup.bcf: {m}; topic dropped"));
            return None;
        }
    };
    let doc = match Document::parse(&text) {
        Ok(d) => d,
        Err(e) => {
            report.warnings.push(format!("{dir}/markup.bcf: not well-formed XML or uses a DTD ({e}); topic dropped"));
            return None;
        }
    };
    let root = doc.root_element();
    if root.tag_name().name() != "Markup" {
        report.warnings.push(format!("{dir}/markup.bcf: root element is {}, not Markup; topic dropped", root.tag_name().name()));
        return None;
    }
    let tag = format!("topic {dir}");
    tally(root, &tag, &["Header", "Topic", "Comment", "Viewpoints"], &[], report);
    let Some(topic) = child(root, "Topic") else {
        report.warnings.push(format!("{dir}/markup.bcf: no Topic element; topic dropped"));
        return None;
    };
    let guid = topic.attribute("Guid").unwrap_or(dir).to_string();
    if topic.attribute("Guid").is_none() {
        report.warnings.push(format!("{tag}: Topic has no Guid attribute; folder name used"));
    } else if !is_guid(&guid) {
        report.warnings.push(format!("{tag}: Topic Guid {guid:?} is not 8-4-4-4-12 hex"));
    }
    if !dir.eq_ignore_ascii_case(&guid) {
        report.warnings.push(format!("{tag}: folder name differs from the Topic Guid {guid}"));
    }
    tally(
        topic,
        &tag,
        &["Title", "Priority", "Index", "Labels", "CreationDate", "CreationAuthor", "ModifiedDate", "ModifiedAuthor", "DueDate", "AssignedTo", "Description"],
        &["Guid", "TopicType", "TopicStatus"],
        report,
    );
    let date = |name: &str, required: bool, report: &mut ImportReport| -> Option<i64> {
        match text_of(topic, name) {
            Some(t) => match parse_datetime(&t) {
                Some(v) => Some(v),
                None => {
                    report.warnings.push(format!("{tag}: {name} {t:?} is not an xs:dateTime"));
                    None
                }
            },
            None => {
                if required {
                    report.warnings.push(format!("{tag}: required {name} is missing"));
                }
                None
            }
        }
    };
    let title = text_of(topic, "Title").unwrap_or_else(|| {
        report.warnings.push(format!("{tag}: required Title is missing"));
        String::new()
    });
    let created = date("CreationDate", true, report).unwrap_or(0);
    let author = text_of(topic, "CreationAuthor").unwrap_or_else(|| {
        report.warnings.push(format!("{tag}: required CreationAuthor is missing"));
        String::new()
    });
    let mut issue = Issue::with_guid(&guid, &title, &author, created);
    issue.issue_type = topic.attribute("TopicType").unwrap_or("").to_string();
    issue.status = topic.attribute("TopicStatus").unwrap_or("").to_string();
    issue.priority = text_of(topic, "Priority").unwrap_or_default();
    issue.assignee = text_of(topic, "AssignedTo").unwrap_or_default();
    issue.description = child(topic, "Description").map(|c| c.text().unwrap_or("").to_string()).unwrap_or_default();
    issue.modified = date("ModifiedDate", false, report);
    issue.modified_author = text_of(topic, "ModifiedAuthor").unwrap_or_default();
    issue.due = date("DueDate", false, report);
    issue.labels = children(topic, "Labels").map(|c| c.text().unwrap_or("").trim().to_string()).collect();
    for h in children(root, "Header") {
        for f in children(h, "File") {
            tally(f, &format!("{tag}: Header/File"), &["Filename", "Date", "Reference"], &["IfcProject", "IfcSpatialStructureElement", "isExternal"], report);
        }
    }

    // The AfroBIM extension, trusted only when it parses and is ours.
    let side: Option<Sidecar> = files
        .get(format!("{dir}/{SIDECAR_NAME}").as_str())
        .and_then(|e| serde_json::from_slice::<Sidecar>(&e.data).ok())
        .filter(|s| s.format == SIDECAR_FORMAT);
    let mut marks: HashMap<String, ElementRef> = HashMap::new();
    if let Some(s) = &side {
        let listed = s.viewpoints.iter().flat_map(|v| {
            let vis: &[ElementRef] = match &v.visibility {
                Visibility::All => &[],
                Visibility::Only(l) | Visibility::Except(l) => l,
            };
            v.selected.iter().chain(vis.iter())
        });
        for r in s.elements.iter().chain(listed) {
            if let Some(g) = r.id.and_then(|id| lookup.by_id.get(&id)) {
                marks.insert(g.clone(), r.clone());
            }
            if let Some(g) = &r.ifc_guid {
                marks.insert(g.clone(), r.clone());
            }
        }
    }
    let cc = ComponentCtx { lookup, marks };

    for c in children(root, "Comment") {
        tally(c, &format!("{tag}: Comment"), &["Date", "Author", "Comment", "Viewpoint"], &["Guid"], report);
        let cguid = c.attribute("Guid").unwrap_or("").to_string();
        let when = match text_of(c, "Date") {
            Some(t) => parse_datetime(&t).unwrap_or_else(|| {
                report.warnings.push(format!("{tag}: comment Date {t:?} is not an xs:dateTime"));
                0
            }),
            None => {
                report.warnings.push(format!("{tag}: comment without Date"));
                0
            }
        };
        issue.comments.push(IssueComment {
            guid: cguid,
            author: text_of(c, "Author").unwrap_or_default(),
            date: when,
            text: child(c, "Comment").map(|n| n.text().unwrap_or("").to_string()).unwrap_or_default(),
            viewpoint: child(c, "Viewpoint").and_then(|v| v.attribute("Guid")).map(str::to_string),
        });
    }

    // Viewpoints named by the markup; a BCF 1.0 style file with only `viewpoint.bcfv` is read too.
    // (viewpoint guid, viewpoint file, snapshot file, index)
    #[allow(clippy::type_complexity)]
    let mut vp_entries: Vec<(String, Option<String>, Option<String>, Option<i64>)> = Vec::new();
    for v in children(root, "Viewpoints") {
        tally(v, &format!("{tag}: Viewpoints"), &["Viewpoint", "Snapshot", "Index"], &["Guid"], report);
        let g = v.attribute("Guid").unwrap_or("").to_string();
        vp_entries.push((g, text_of(v, "Viewpoint").filter(|t| !t.is_empty()), text_of(v, "Snapshot").filter(|t| !t.is_empty()), text_of(v, "Index").and_then(|t| t.parse().ok())));
    }
    if vp_entries.is_empty() {
        if let Some(e) = files.get(format!("{dir}/viewpoint.bcfv").as_str()) {
            let g = parse_doc(&e.data).ok().and_then(|t| Document::parse(&t).ok().and_then(|d| d.root_element().attribute("Guid").map(str::to_string))).unwrap_or_default();
            vp_entries.push((g, Some("viewpoint.bcfv".into()), files.get(format!("{dir}/snapshot.png").as_str()).map(|_| "snapshot.png".to_string()), None));
        }
    }
    vp_entries.sort_by_key(|e| e.3.unwrap_or(i64::MAX));
    for (g, vfile, sfile, _) in vp_entries {
        let mut vp = match vfile {
            Some(f) => match files.get(format!("{dir}/{f}").as_str()) {
                Some(e) => match parse_doc(&e.data) {
                    Ok(t) => {
                        let guid = if g.is_empty() { Document::parse(&t).ok().and_then(|d| d.root_element().attribute("Guid").map(str::to_string)).unwrap_or_default() } else { g.clone() };
                        read_viewpoint(&t, &guid, &cc, &format!("{tag}, viewpoint {f}"), report)
                    }
                    Err(m) => {
                        report.warnings.push(format!("{tag}: {f}: {m}; viewpoint dropped"));
                        None
                    }
                },
                None => {
                    report.warnings.push(format!("{tag}: viewpoint file {f} named in the markup is not in the archive"));
                    None
                }
            },
            None => Some(Viewpoint::new(g.clone(), None)),
        };
        if let (Some(vp), Some(sf)) = (vp.as_mut(), sfile) {
            match files.get(format!("{dir}/{sf}").as_str()) {
                Some(e) if e.data.starts_with(&[0x89, b'P', b'N', b'G', 0x0D, 0x0A, 0x1A, 0x0A]) => vp.snapshot_png = Some(e.data.clone()),
                Some(_) => report.skipped.push(format!("{tag}: snapshot {sf} is not a PNG; ignored")),
                None => report.warnings.push(format!("{tag}: snapshot file {sf} named in the markup is not in the archive")),
            }
        }
        if let Some(mut vp) = vp {
            // Where the sidecar still matches the XML, take its exact values.
            if let Some(sv) = side.as_ref().and_then(|s| s.viewpoints.iter().find(|v| v.guid == vp.guid)) {
                match (&sv.camera, &vp.camera) {
                    (Some(sc), Some(xc)) if same_camera(sc, xc) => vp.camera = Some(sc.clone()),
                    _ => {}
                }
            }
            issue.viewpoints.push(vp);
        }
    }

    match &side {
        Some(s) => {
            issue.location = s.location;
            issue.elements = s.elements.clone();
            issue.clashes = s.clashes.clone();
        }
        None => {
            // A foreign file: the related elements are the ones its viewpoints select.
            let mut seen = std::collections::BTreeSet::new();
            for v in &issue.viewpoints {
                for r in &v.selected {
                    if seen.insert((r.id, r.ifc_guid.clone())) {
                        issue.elements.push(r.clone());
                    }
                }
            }
        }
    }
    let index = text_of(topic, "Index").and_then(|t| t.parse::<i64>().ok());
    Some((issue, index))
}
