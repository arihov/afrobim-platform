//! Issues: the things people discuss and assign, with comments, related elements, a location
//! and saved viewpoints. They persist with the coordination data and exchange through BCF.

use afrobim_ifc::name_uuid;
use afrobim_kernel::ElementId;
use serde::{Deserialize, Serialize};

use crate::b64;
use crate::geom::{cross, dot, len, scale, sub, V3};
use crate::group::ClashGroup;
use crate::results::Clash;

/// Common values for the free-text `status`, `issue_type` and `priority` fields. BCF 2.1 leaves
/// the values to the project (`markup.xsd` declares these as plain strings); the status list
/// follows the sample `extensions.xsd` of the 2.1 release (Open, In Progress, Closed, ReOpened).
pub mod values {
    pub const STATUS_OPEN: &str = "Open";
    pub const STATUS_IN_PROGRESS: &str = "In Progress";
    pub const STATUS_RESOLVED: &str = "Resolved";
    pub const STATUS_CLOSED: &str = "Closed";
    pub const STATUS_REOPENED: &str = "ReOpened";
    pub const TYPE_CLASH: &str = "Clash";
    pub const TYPE_ISSUE: &str = "Issue";
    pub const PRIORITY_LOW: &str = "Low";
    pub const PRIORITY_NORMAL: &str = "Normal";
    pub const PRIORITY_HIGH: &str = "High";
    pub const PRIORITY_CRITICAL: &str = "Critical";
}

/// A reference to an element: one of this document (`id`), one of a reference model or a foreign
/// BCF file (only `ifc_guid`), or both.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ElementRef {
    #[serde(default)]
    pub id: Option<ElementId>,
    /// The mark or name shown to people.
    #[serde(default)]
    pub mark: String,
    /// The IFC GlobalId, when known. For an element of this document it is derived at export time
    /// (`afrobim_ifc::element_global_id`) when left `None`.
    #[serde(default)]
    pub ifc_guid: Option<String>,
}

impl ElementRef {
    /// An element of this document.
    pub fn element(id: ElementId, mark: &str) -> ElementRef {
        ElementRef { id: Some(id), mark: mark.to_string(), ifc_guid: None }
    }
    /// An element known only by its IFC GlobalId (a reference model, or a foreign BCF file).
    pub fn foreign(ifc_guid: &str, name: &str) -> ElementRef {
        ElementRef { id: None, mark: name.to_string(), ifc_guid: Some(ifc_guid.to_string()) }
    }
}

fn unit(a: V3) -> Option<V3> {
    let l = len(a);
    (l > 1e-12 && l.is_finite()).then(|| scale(a, 1.0 / l))
}

/// One comment on an issue.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct IssueComment {
    pub guid: String,
    pub author: String,
    /// Seconds since the Unix epoch (UTC).
    pub date: i64,
    pub text: String,
    /// The viewpoint this comment refers to, if any.
    #[serde(default)]
    pub viewpoint: Option<String>,
}

/// The camera of a viewpoint. World millimetres, Z up; BCF stores metres and the exporter divides.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Camera {
    pub position: V3,
    /// Viewing direction (need not be normalised; export normalises).
    pub direction: V3,
    pub up: V3,
    pub projection: Projection,
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Projection {
    /// Vertical or horizontal field of view in degrees, as the viewer defines it. BCF 2.1 only
    /// admits 45 to 60 (`visinfo.xsd`, `FieldOfView`); other values are clamped on export.
    Perspective { fov_deg: f64 },
    /// BCF `ViewToWorldScale`: the visible size of the view, here in millimetres.
    Orthogonal { view_to_world_scale_mm: f64 },
}

impl Camera {
    /// A camera at `target + from_direction * distance_mm`, looking at `target`, with Z up (or Y
    /// up when looking straight along Z). `from_direction` points from the target to the camera.
    pub fn looking_at(target: V3, from_direction: V3, distance_mm: f64, projection: Projection) -> Camera {
        let back = unit(from_direction).unwrap_or([0.0, -1.0, 0.0]);
        let position = [target[0] + back[0] * distance_mm, target[1] + back[1] * distance_mm, target[2] + back[2] * distance_mm];
        let direction = scale(back, -1.0);
        let world_up = if back[0].abs() < 1e-9 && back[1].abs() < 1e-9 { [0.0, 1.0, 0.0] } else { [0.0, 0.0, 1.0] };
        let right = cross(direction, world_up);
        let up = unit(cross(right, direction)).unwrap_or(world_up);
        Camera { position, direction, up, projection }
    }

    /// True when the direction and up vector are usable (non-zero, not parallel).
    pub fn is_valid(&self) -> bool {
        let d = len(self.direction);
        let u = len(self.up);
        d > 1e-12 && u > 1e-12 && len(cross(self.direction, self.up)) > 1e-9 * d * u && dot(self.direction, self.direction).is_finite()
    }

    /// The same camera moved by `delta` (for example to put a clash viewpoint beside the point).
    pub fn translated(&self, delta: V3) -> Camera {
        Camera { position: [self.position[0] + delta[0], self.position[1] + delta[1], self.position[2] + delta[2]], ..self.clone() }
    }

    /// Distance from the camera to a point along the viewing direction, mm.
    pub fn depth_of(&self, p: V3) -> f64 {
        match unit(self.direction) {
            Some(d) => dot(sub(p, self.position), d),
            None => 0.0,
        }
    }
}

/// Which elements a viewpoint shows.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Visibility {
    /// Everything visible.
    #[default]
    All,
    /// Only these are visible (BCF: `DefaultVisibility` false, the list as exceptions).
    Only(Vec<ElementRef>),
    /// Everything but these (BCF: `DefaultVisibility` true, the list as exceptions).
    Except(Vec<ElementRef>),
}

/// A saved view: camera, selection, visibility and an optional snapshot.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Viewpoint {
    pub guid: String,
    #[serde(default)]
    pub camera: Option<Camera>,
    #[serde(default)]
    pub selected: Vec<ElementRef>,
    #[serde(default)]
    pub visibility: Visibility,
    /// PNG bytes supplied by the caller (the library renders nothing). Persisted as base64.
    #[serde(default, with = "b64::opt_bytes")]
    pub snapshot_png: Option<Vec<u8>>,
}

impl Viewpoint {
    pub fn new(guid: String, camera: Option<Camera>) -> Viewpoint {
        Viewpoint { guid, camera, selected: Vec::new(), visibility: Visibility::All, snapshot_png: None }
    }
}

/// The clash an issue was raised from (or one of the clashes of a group).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ClashLink {
    pub clash_id: String,
    pub test: String,
    pub element_a: ElementId,
    pub element_b: ElementId,
    pub mark_a: String,
    pub mark_b: String,
    /// World millimetres.
    pub point: V3,
}

impl ClashLink {
    pub fn of(c: &Clash) -> ClashLink {
        ClashLink {
            clash_id: c.id.clone(),
            test: c.test.clone(),
            element_a: c.element_a,
            element_b: c.element_b,
            mark_a: c.mark_a.clone(),
            mark_b: c.mark_b.clone(),
            point: c.point,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Issue {
    /// Stable identity, the BCF topic GUID: lower case `8-4-4-4-12` hex.
    pub guid: String,
    pub title: String,
    #[serde(default)]
    pub description: String,
    /// BCF `TopicType`, free text (see [`values`]).
    #[serde(default)]
    pub issue_type: String,
    /// BCF `TopicStatus`, free text.
    #[serde(default)]
    pub status: String,
    /// BCF `Priority`, free text.
    #[serde(default)]
    pub priority: String,
    #[serde(default)]
    pub assignee: String,
    pub author: String,
    /// Seconds since the Unix epoch (UTC).
    pub created: i64,
    #[serde(default)]
    pub modified: Option<i64>,
    #[serde(default)]
    pub modified_author: String,
    #[serde(default)]
    pub due: Option<i64>,
    #[serde(default)]
    pub labels: Vec<String>,
    #[serde(default)]
    pub comments: Vec<IssueComment>,
    /// The elements the issue is about.
    #[serde(default)]
    pub elements: Vec<ElementRef>,
    /// Where the issue is, world millimetres.
    #[serde(default)]
    pub location: Option<V3>,
    #[serde(default)]
    pub viewpoints: Vec<Viewpoint>,
    /// The clashes this issue covers: one for an issue from a clash, several for a group.
    #[serde(default)]
    pub clashes: Vec<ClashLink>,
}

impl Issue {
    /// A free-standing issue with an explicit GUID.
    pub fn with_guid(guid: &str, title: &str, author: &str, created: i64) -> Issue {
        Issue {
            guid: guid.to_string(),
            title: title.to_string(),
            description: String::new(),
            issue_type: values::TYPE_ISSUE.to_string(),
            status: values::STATUS_OPEN.to_string(),
            priority: values::PRIORITY_NORMAL.to_string(),
            assignee: String::new(),
            author: author.to_string(),
            created,
            modified: None,
            modified_author: String::new(),
            due: None,
            labels: Vec::new(),
            comments: Vec::new(),
            elements: Vec::new(),
            location: None,
            viewpoints: Vec::new(),
            clashes: Vec::new(),
        }
    }

    /// An issue raised from one clash. The GUID derives from the clash id, so raising an issue
    /// from the same clash again gives the same GUID (and a re-run of a test keeps the link).
    pub fn from_clash(c: &Clash, author: &str, created: i64) -> Issue {
        let mut i = Issue::with_guid(&name_uuid(&format!("issue/clash/{}", c.id)), &format!("{}: {} / {}", c.test, c.mark_a, c.mark_b), author, created);
        i.issue_type = values::TYPE_CLASH.to_string();
        i.description = clash_text(c);
        i.elements = vec![ElementRef::element(c.element_a, &c.mark_a), ElementRef::element(c.element_b, &c.mark_b)];
        i.location = Some(c.point);
        i.clashes = vec![ClashLink::of(c)];
        i
    }

    /// An issue covering a group of clashes (see [`crate::group_clashes`]). The GUID derives from
    /// the group id. The location is the group's centroid.
    pub fn from_group(g: &ClashGroup, clashes: &[Clash], author: &str, created: i64) -> Issue {
        let members: Vec<&Clash> = g.clash_ids.iter().filter_map(|id| clashes.iter().find(|c| &c.id == id)).collect();
        let title = match members.as_slice() {
            [] => format!("Clash group {}", g.id),
            [one] => format!("{}: {} / {}", one.test, one.mark_a, one.mark_b),
            many => format!("{} clashes around {} / {} and others", many.len(), many[0].mark_a, many[0].mark_b),
        };
        let mut i = Issue::with_guid(&name_uuid(&format!("issue/group/{}", g.id)), &title, author, created);
        i.issue_type = values::TYPE_CLASH.to_string();
        i.description = members.iter().map(|c| clash_text(c)).collect::<Vec<_>>().join("\n");
        let mut seen = std::collections::BTreeSet::new();
        for c in &members {
            for (id, mark) in [(c.element_a, &c.mark_a), (c.element_b, &c.mark_b)] {
                if seen.insert(id) {
                    i.elements.push(ElementRef::element(id, mark));
                }
            }
        }
        i.location = Some(g.centroid);
        i.clashes = members.iter().map(|c| ClashLink::of(c)).collect();
        i
    }

    /// Adds a comment (GUID derived from the issue GUID and the comment count and text) and
    /// stamps the modification.
    pub fn add_comment(&mut self, author: &str, date: i64, text: &str) -> &IssueComment {
        let guid = name_uuid(&format!("comment/{}/{}/{}/{}", self.guid, self.comments.len(), date, text));
        self.comments.push(IssueComment { guid, author: author.to_string(), date, text: text.to_string(), viewpoint: None });
        self.modified = Some(date);
        self.modified_author = author.to_string();
        self.comments.last().unwrap()
    }

    /// Adds a viewpoint with a GUID derived from the issue GUID and the viewpoint count.
    pub fn add_viewpoint(&mut self, camera: Option<Camera>) -> &mut Viewpoint {
        let guid = name_uuid(&format!("viewpoint/{}/{}", self.guid, self.viewpoints.len()));
        self.viewpoints.push(Viewpoint::new(guid, camera));
        self.viewpoints.last_mut().unwrap()
    }

    /// Changes the status and stamps the modification.
    pub fn set_status(&mut self, status: &str, by: &str, date: i64) {
        self.status = status.to_string();
        self.modified = Some(date);
        self.modified_author = by.to_string();
    }

    /// Adds the viewpoint a clash deserves: a camera `distance_mm` from the issue location,
    /// looking at it from `from_direction`, with the issue's elements selected. Does nothing
    /// without a location. The distance is the caller's choice.
    pub fn add_location_viewpoint(&mut self, from_direction: V3, distance_mm: f64, projection: Projection) -> Option<&mut Viewpoint> {
        let at = self.location?;
        let selected = self.elements.clone();
        let vp = self.add_viewpoint(Some(Camera::looking_at(at, from_direction, distance_mm, projection)));
        vp.selected = selected;
        Some(vp)
    }
}

fn clash_text(c: &Clash) -> String {
    let measure = match (c.depth_mm, c.distance_mm) {
        (Some(d), _) if c.kind == crate::ClashKind::Hard => format!("penetration about {d:.1} mm"),
        (_, Some(d)) => format!("distance {d:.1} mm"),
        _ => String::new(),
    };
    format!(
        "{} clash between {} ({}) and {} ({}), {}at ({:.0}, {:.0}, {:.0}) mm [clash {}]",
        c.kind.label(),
        c.mark_a,
        c.category_a,
        c.mark_b,
        c.category_b,
        if measure.is_empty() { String::new() } else { format!("{measure}, ") },
        c.point[0],
        c.point[1],
        c.point[2],
        c.id
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn looking_at_aims_at_the_target_with_a_usable_up() {
        let c = Camera::looking_at([1000.0, 2000.0, 500.0], [0.0, -1.0, 0.5], 3000.0, Projection::Perspective { fov_deg: 50.0 });
        assert!(c.is_valid());
        assert!((c.depth_of([1000.0, 2000.0, 500.0]) - 3000.0).abs() < 1e-6);
        assert!(dot(unit(c.direction).unwrap(), unit(c.up).unwrap()).abs() < 1e-9);
        assert!(c.up[2] > 0.0);
        let down = Camera::looking_at([0.0; 3], [0.0, 0.0, 1.0], 1000.0, Projection::Perspective { fov_deg: 50.0 });
        assert!(down.is_valid());
    }
}
