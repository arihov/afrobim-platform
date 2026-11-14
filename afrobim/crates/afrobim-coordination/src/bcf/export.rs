//! BCF 2.1 export. Element and attribute names are those of `markup.xsd`, `visinfo.xsd`,
//! `version.xsd` and `project.xsd` of the buildingSMART/BCF-XML release tagged `v2.1`.

use std::collections::BTreeSet;

use afrobim_ifc::{element_global_id, global_id};
use serde::{Deserialize, Serialize};

use super::time::format_datetime;
use super::xml::XmlWriter;
use super::zip::{write_zip, ZipEntry, ZipError};
use crate::geom::{len, scale, V3};
use crate::issue::{Camera, ClashLink, ElementRef, Issue, Projection, Viewpoint, Visibility};

/// What goes around the issues.
#[derive(Debug, Clone, PartialEq)]
pub struct BcfOptions {
    /// The IFC project name: the scope of every GlobalId. Pass the `IfcOptions::project_name`
    /// the IFC file was exported with, so elements resolve to the GlobalIds in that file.
    pub project_name: String,
    /// Name of the IFC file the issues refer to, written to the topic header (`Header/File`).
    /// `None` writes no header.
    pub ifc_file_name: Option<String>,
    /// Written as `Component/OriginatingSystem`.
    pub originating_system: String,
    /// `ProjectExtension/Project/@ProjectId` of `project.bcfp`. `None` writes no project file.
    pub project_id: Option<String>,
    /// `ProjectExtension/Project/Name`.
    pub project_label: String,
}

impl Default for BcfOptions {
    fn default() -> Self {
        BcfOptions { project_name: "AfroBIM project".into(), ifc_file_name: None, originating_system: ORIGINATING_SYSTEM.into(), project_id: None, project_label: String::new() }
    }
}

pub const ORIGINATING_SYSTEM: &str = "AfroBIM";

/// The AfroBIM extension file inside a topic folder. BCF 2.1 has no place for an issue's
/// location, its related elements' marks or its clash link; they go here. Other tools ignore
/// files they do not know (not verified against any tool).
pub const SIDECAR_NAME: &str = "afrobim.json";

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub(crate) struct Sidecar {
    pub format: String,
    pub version: u32,
    #[serde(default)]
    pub location: Option<V3>,
    #[serde(default)]
    pub elements: Vec<ElementRef>,
    #[serde(default)]
    pub clashes: Vec<ClashLink>,
    #[serde(default)]
    pub viewpoints: Vec<SidecarViewpoint>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub(crate) struct SidecarViewpoint {
    pub guid: String,
    #[serde(default)]
    pub camera: Option<Camera>,
    #[serde(default)]
    pub selected: Vec<ElementRef>,
    #[serde(default)]
    pub visibility: Visibility,
}

pub(crate) const SIDECAR_FORMAT: &str = "afrobim-bcf-extension";

#[derive(Debug, Clone, PartialEq)]
pub enum BcfError {
    Zip(ZipError),
    /// An issue, comment or viewpoint GUID is not `8-4-4-4-12` hex (the `Guid` type of the XSDs).
    InvalidGuid(String),
    /// Two issues share a GUID.
    DuplicateGuid(String),
    /// The input is not a BCF file we can read.
    NotBcf(String),
    Unsupported(String),
}

impl std::fmt::Display for BcfError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            BcfError::Zip(e) => write!(f, "{e}"),
            BcfError::InvalidGuid(g) => write!(f, "not a BCF GUID (8-4-4-4-12 hex): {g:?}"),
            BcfError::DuplicateGuid(g) => write!(f, "two issues share the GUID {g}"),
            BcfError::NotBcf(s) => write!(f, "not a BCF file: {s}"),
            BcfError::Unsupported(s) => write!(f, "unsupported: {s}"),
        }
    }
}

impl std::error::Error for BcfError {}

impl From<ZipError> for BcfError {
    fn from(e: ZipError) -> Self {
        BcfError::Zip(e)
    }
}

/// An exported archive and what the exporter had to leave out or change.
#[derive(Debug, Clone, PartialEq)]
pub struct BcfExport {
    /// The `.bcf` / `.bcfzip` file.
    pub bytes: Vec<u8>,
    pub notes: Vec<String>,
}

pub(crate) fn is_guid(s: &str) -> bool {
    let b = s.as_bytes();
    b.len() == 36 && b.iter().enumerate().all(|(i, c)| if matches!(i, 8 | 13 | 18 | 23) { *c == b'-' } else { c.is_ascii_hexdigit() })
}

/// `IfcGuid` of the XSDs: 22 characters of `[0-9,A-Z,a-z,_$]` (the pattern as written also lets a
/// comma through, which no IFC GlobalId contains).
pub(crate) fn is_ifc_guid(s: &str) -> bool {
    s.chars().count() == 22 && s.chars().all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '$')
}

fn num(v: f64) -> String {
    // Rust's Display prints the shortest text that reads back to the same f64, without an
    // exponent, which is valid xs:double.
    if v == 0.0 {
        "0".into()
    } else {
        format!("{v}")
    }
}

fn m(v: f64) -> String {
    num(v / 1000.0)
}

fn point(w: &mut XmlWriter, name: &'static str, p: V3) {
    w.open(name, &[]);
    w.text("X", &m(p[0]));
    w.text("Y", &m(p[1]));
    w.text("Z", &m(p[2]));
    w.close();
}

fn direction(w: &mut XmlWriter, name: &'static str, d: V3) {
    let l = len(d);
    let u = if l > 0.0 { scale(d, 1.0 / l) } else { d };
    w.open(name, &[]);
    w.text("X", &num(u[0]));
    w.text("Y", &num(u[1]));
    w.text("Z", &num(u[2]));
    w.close();
}

struct Ctx<'a> {
    options: &'a BcfOptions,
    notes: Vec<String>,
}

impl Ctx<'_> {
    fn guid_of(&mut self, r: &ElementRef, issue: &str) -> Option<String> {
        let g = match (&r.ifc_guid, r.id) {
            (Some(g), _) => g.clone(),
            (None, Some(id)) => element_global_id(&self.options.project_name, id),
            (None, None) => {
                self.notes.push(format!("{issue}: element {:?} has neither an id nor an IFC GlobalId; left out", r.mark));
                return None;
            }
        };
        if !is_ifc_guid(&g) {
            self.notes.push(format!("{issue}: {g:?} is not an IFC GlobalId (22 characters); left out"));
            return None;
        }
        Some(g)
    }

    fn components(&mut self, w: &mut XmlWriter, name: &'static str, refs: &[ElementRef], issue: &str) {
        let mut seen = BTreeSet::new();
        let mut list = Vec::new();
        for r in refs {
            if let Some(g) = self.guid_of(r, issue) {
                if seen.insert(g.clone()) {
                    list.push((g, r.id));
                }
            }
        }
        if list.is_empty() {
            return;
        }
        w.open(name, &[]);
        for (g, id) in list {
            w.open("Component", &[("IfcGuid", &g)]);
            w.text("OriginatingSystem", &self.options.originating_system);
            if let Some(id) = id {
                w.text("AuthoringToolId", &id.0.to_string());
            }
            w.close();
        }
        w.close();
    }

    fn viewpoint_xml(&mut self, v: &Viewpoint, issue: &str) -> String {
        let mut w = XmlWriter::new();
        w.open("VisualizationInfo", &[("Guid", &v.guid)]);
        let has_components = !v.selected.is_empty() || !matches!(v.visibility, Visibility::All);
        if has_components {
            w.open("Components", &[]);
            self.components(&mut w, "Selection", &v.selected, issue);
            // `Visibility` is required inside `Components` (minOccurs 1).
            match &v.visibility {
                Visibility::All => w.empty("Visibility", &[("DefaultVisibility", "true")]),
                Visibility::Only(list) | Visibility::Except(list) => {
                    let default = matches!(v.visibility, Visibility::Except(_));
                    w.open("Visibility", &[("DefaultVisibility", if default { "true" } else { "false" })]);
                    self.components(&mut w, "Exceptions", list, issue);
                    w.close();
                }
            }
            w.close();
        }
        if let Some(c) = &v.camera {
            if !c.is_valid() {
                self.notes.push(format!("{issue}: viewpoint {} has a degenerate camera direction or up vector; camera left out", v.guid));
            } else {
                match c.projection {
                    Projection::Orthogonal { view_to_world_scale_mm } => {
                        w.open("OrthogonalCamera", &[]);
                        point(&mut w, "CameraViewPoint", c.position);
                        direction(&mut w, "CameraDirection", c.direction);
                        direction(&mut w, "CameraUpVector", c.up);
                        w.text("ViewToWorldScale", &m(view_to_world_scale_mm));
                        w.close();
                    }
                    Projection::Perspective { fov_deg } => {
                        let clamped = fov_deg.clamp(45.0, 60.0);
                        if clamped != fov_deg {
                            self.notes.push(format!("{issue}: field of view {fov_deg} degrees is outside the 45 to 60 BCF 2.1 allows; written as {clamped}"));
                        }
                        w.open("PerspectiveCamera", &[]);
                        point(&mut w, "CameraViewPoint", c.position);
                        direction(&mut w, "CameraDirection", c.direction);
                        direction(&mut w, "CameraUpVector", c.up);
                        w.text("FieldOfView", &num(clamped));
                        w.close();
                    }
                }
            }
        }
        w.close();
        w.finish()
    }
}

fn viewpoint_file(i: usize) -> String {
    if i == 0 {
        "viewpoint.bcfv".into()
    } else {
        format!("viewpoint-{i}.bcfv")
    }
}

fn snapshot_file(i: usize) -> String {
    if i == 0 {
        "snapshot.png".into()
    } else {
        format!("snapshot-{i}.png")
    }
}

/// Writes `issues` as a BCF 2.1 archive (`bcf.version`, optional `project.bcfp`, and per issue a
/// folder named by its GUID with `markup.bcf`, `viewpoint*.bcfv`, `snapshot*.png` and the
/// AfroBIM extension file [`SIDECAR_NAME`]). Directory entries are written and paths use `/`, as
/// the 2.1 documentation asks.
pub fn export_bcf(issues: &[Issue], options: &BcfOptions) -> Result<BcfExport, BcfError> {
    let mut ctx = Ctx { options, notes: Vec::new() };
    let mut entries: Vec<ZipEntry> = Vec::new();
    let mut seen = BTreeSet::new();
    // Exactly the bytes of the documentation's `bcf.version` sample, with the namespace-free
    // `version.xsd` structure.
    let mut vw = XmlWriter::new();
    vw.open("Version", &[("VersionId", "2.1")]);
    vw.text("DetailedVersion", "2.1");
    vw.close();
    entries.push(ZipEntry { name: "bcf.version".into(), data: vw.finish().into_bytes() });
    if let Some(pid) = &options.project_id {
        let mut pw = XmlWriter::new();
        pw.open("ProjectExtension", &[]);
        pw.open("Project", &[("ProjectId", pid)]);
        if !options.project_label.is_empty() {
            pw.text("Name", &options.project_label);
        }
        pw.close();
        pw.text("ExtensionSchema", "");
        pw.close();
        entries.push(ZipEntry { name: "project.bcfp".into(), data: pw.finish().into_bytes() });
    }
    for (index, issue) in issues.iter().enumerate() {
        if !is_guid(&issue.guid) {
            return Err(BcfError::InvalidGuid(issue.guid.clone()));
        }
        if !seen.insert(issue.guid.to_lowercase()) {
            return Err(BcfError::DuplicateGuid(issue.guid.clone()));
        }
        for v in &issue.viewpoints {
            if !is_guid(&v.guid) {
                return Err(BcfError::InvalidGuid(v.guid.clone()));
            }
        }
        for c in &issue.comments {
            if !is_guid(&c.guid) {
                return Err(BcfError::InvalidGuid(c.guid.clone()));
            }
        }
        let tag = format!("issue {} ({})", issue.guid, issue.title);
        let dir = format!("{}/", issue.guid);
        entries.push(ZipEntry { name: dir.clone(), data: Vec::new() });

        let mut w = XmlWriter::new();
        w.open("Markup", &[]);
        if let Some(file) = &options.ifc_file_name {
            w.open("Header", &[]);
            let project_guid = global_id(&format!("{}/project", options.project_name));
            w.open("File", &[("IfcProject", &project_guid), ("isExternal", "true")]);
            w.text("Filename", file);
            w.close();
            w.close();
        }
        let mut attrs: Vec<(&str, &str)> = vec![("Guid", &issue.guid)];
        if !issue.issue_type.is_empty() {
            attrs.push(("TopicType", &issue.issue_type));
        }
        if !issue.status.is_empty() {
            attrs.push(("TopicStatus", &issue.status));
        }
        w.open("Topic", &attrs);
        w.text("Title", &issue.title);
        if !issue.priority.is_empty() {
            w.text("Priority", &issue.priority);
        }
        w.text("Index", &index.to_string());
        for l in &issue.labels {
            w.text("Labels", l);
        }
        w.text("CreationDate", &format_datetime(issue.created));
        w.text("CreationAuthor", &issue.author);
        if let Some(t) = issue.modified {
            w.text("ModifiedDate", &format_datetime(t));
            if !issue.modified_author.is_empty() {
                w.text("ModifiedAuthor", &issue.modified_author);
            }
        }
        if let Some(t) = issue.due {
            w.text("DueDate", &format_datetime(t));
        }
        if !issue.assignee.is_empty() {
            w.text("AssignedTo", &issue.assignee);
        }
        if !issue.description.is_empty() {
            w.text("Description", &issue.description);
        }
        w.close();
        for c in &issue.comments {
            w.open("Comment", &[("Guid", &c.guid)]);
            w.text("Date", &format_datetime(c.date));
            w.text("Author", &c.author);
            w.text("Comment", &c.text);
            if let Some(vp) = c.viewpoint.as_ref().filter(|g| issue.viewpoints.iter().any(|v| &v.guid == *g)) {
                w.empty("Viewpoint", &[("Guid", vp)]);
            }
            w.close();
        }
        for (i, v) in issue.viewpoints.iter().enumerate() {
            let png = v.snapshot_png.as_ref().filter(|b| !b.is_empty());
            w.open("Viewpoints", &[("Guid", &v.guid)]);
            w.text("Viewpoint", &viewpoint_file(i));
            if png.is_some() {
                w.text("Snapshot", &snapshot_file(i));
            }
            w.text("Index", &i.to_string());
            w.close();
            let xml = ctx.viewpoint_xml(v, &tag);
            entries.push(ZipEntry { name: format!("{dir}{}", viewpoint_file(i)), data: xml.into_bytes() });
            if let Some(b) = png {
                entries.push(ZipEntry { name: format!("{dir}{}", snapshot_file(i)), data: b.clone() });
            }
        }
        w.close();
        entries.push(ZipEntry { name: format!("{dir}markup.bcf"), data: w.finish().into_bytes() });

        let side = Sidecar {
            format: SIDECAR_FORMAT.into(),
            version: 1,
            location: issue.location,
            elements: issue.elements.clone(),
            clashes: issue.clashes.clone(),
            viewpoints: issue
                .viewpoints
                .iter()
                .map(|v| SidecarViewpoint { guid: v.guid.clone(), camera: v.camera.clone(), selected: v.selected.clone(), visibility: v.visibility.clone() })
                .collect(),
        };
        let json = serde_json::to_vec_pretty(&side).map_err(|e| BcfError::Unsupported(e.to_string()))?;
        entries.push(ZipEntry { name: format!("{dir}{SIDECAR_NAME}"), data: json });
    }
    let bytes = write_zip(&entries)?;
    Ok(BcfExport { bytes, notes: ctx.notes })
}
