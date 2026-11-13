use std::collections::BTreeMap;
use std::path::Path;

use serde::{Deserialize, Serialize};

use crate::{BBox, Pt};

/// File extension of an AfroBIM document.
pub const FILE_EXTENSION: &str = "afrobim";
const FORMAT_NAME: &str = "afrobim";
const FORMAT_VERSION: u32 = 1;

/// Stable identity of an element within a document.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(transparent)]
pub struct ElementId(pub u64);

/// Where an element stands on site, as opposed to where its face is laid out on the design
/// sheet. Coordinates are millimetres in the site's plan system, Z up.
///
/// `rotation_deg` turns the element's own frame about +Z, counter-clockwise from site +X. For a
/// sign, local +X is its width and the face looks towards local −Y, so a sign serving traffic
/// that travels on heading θ has rotation θ − 90°.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SitePlacement {
    pub x: f64,
    pub y: f64,
    #[serde(default)]
    pub z: f64,
    #[serde(default)]
    pub rotation_deg: f64,
    /// The alignment the position was taken from, with its station and offset in metres
    /// (offset positive to the right looking up-station). Kept so the position can be
    /// recomputed when the alignment changes.
    #[serde(default)]
    pub alignment: Option<String>,
    #[serde(default)]
    pub station_m: Option<f64>,
    #[serde(default)]
    pub offset_m: Option<f64>,
}

/// A building-information component placed in model space.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Element {
    pub id: ElementId,
    /// Names the module that owns this element, e.g. `"sign"`.
    pub category: String,
    /// The mark or code the element is scheduled under, e.g. `"ADS-001"`.
    pub name: String,
    /// Lower-left corner in model space, millimetres.
    pub origin: Pt,
    /// Overall size in millimetres. Derived by the owning module; stored so a document can be
    /// picked and zoomed before any module has run.
    pub width: f64,
    pub height: f64,
    #[serde(default = "default_layer")]
    pub layer: String,
    /// The module-owned definition (for a sign: its type, speed, destinations...).
    pub data: serde_json::Value,
    /// Free-form property set: material, drawing reference, cost code and the like.
    #[serde(default)]
    pub properties: BTreeMap<String, String>,
    /// Position on site, when the element has been placed there.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub site: Option<SitePlacement>,
    /// The element this one belongs to: a post is hosted by its sign, a foundation by its post.
    /// Erasing the host does not erase its dependants; callers decide.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub host: Option<ElementId>,
}

fn default_layer() -> String {
    "0".to_string()
}

impl Element {
    pub fn bounds(&self) -> BBox {
        BBox::new(self.origin.x, self.origin.y, self.origin.x + self.width, self.origin.y + self.height)
    }

    /// Grip points: four corners, then the centre.
    pub fn grips(&self) -> [Pt; 5] {
        let b = self.bounds();
        [b.min, Pt::new(b.max.x, b.min.y), b.max, Pt::new(b.min.x, b.max.y), b.centre()]
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Layer {
    pub name: String,
    pub visible: bool,
    pub locked: bool,
}

/// How a pick or a selection box changes the selection set.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SelectMode {
    Replace,
    Add,
    Remove,
}

#[derive(Debug, thiserror::Error)]
pub enum DocumentError {
    #[error("{0}")]
    Io(#[from] std::io::Error),
    #[error("not a valid AfroBIM document: {0}")]
    Format(String),
}

#[derive(Serialize, Deserialize)]
struct FileDto {
    format: String,
    version: u32,
    units: String,
    #[serde(default)]
    standard: String,
    layers: Vec<Layer>,
    elements: Vec<Element>,
}

/// The model: elements in draw order (last is on top), layers and the selection set.
#[derive(Debug, Clone, PartialEq)]
pub struct Document {
    elements: Vec<Element>,
    layers: Vec<Layer>,
    selection: Vec<ElementId>,
    next_id: u64,
    /// The design standard the document was authored against, e.g. a rule pack id.
    pub standard: String,
}

impl Default for Document {
    fn default() -> Self {
        Self {
            elements: Vec::new(),
            layers: vec![Layer { name: "0".into(), visible: true, locked: false }],
            selection: Vec::new(),
            next_id: 1,
            standard: String::new(),
        }
    }
}

impl Document {
    /// Gap left between elements by automatic arrangement, mm.
    pub const ARRANGE_GAP: f64 = 500.0;
    /// Row width used by automatic arrangement before starting a new row, mm.
    pub const ARRANGE_ROW_WIDTH: f64 = 12000.0;

    pub fn elements(&self) -> &[Element] {
        &self.elements
    }

    pub fn layers(&self) -> &[Layer] {
        &self.layers
    }

    pub fn layers_mut(&mut self) -> &mut Vec<Layer> {
        &mut self.layers
    }

    /// Selected ids in pick order; the last one is the primary selection.
    pub fn selection(&self) -> &[ElementId] {
        &self.selection
    }

    pub fn primary(&self) -> Option<ElementId> {
        self.selection.last().copied()
    }

    pub fn is_selected(&self, id: ElementId) -> bool {
        self.selection.contains(&id)
    }

    pub fn find(&self, id: ElementId) -> Option<&Element> {
        self.elements.iter().find(|e| e.id == id)
    }

    pub fn find_mut(&mut self, id: ElementId) -> Option<&mut Element> {
        self.elements.iter_mut().find(|e| e.id == id)
    }

    /// Adds an element and returns its id. Makes sure its layer exists.
    pub fn add(&mut self, category: &str, name: &str, origin: Pt, width: f64, height: f64, layer: &str, data: serde_json::Value) -> ElementId {
        let id = ElementId(self.next_id);
        self.next_id += 1;
        self.ensure_layer(layer);
        self.elements.push(Element {
            id,
            category: category.to_string(),
            name: name.to_string(),
            origin,
            width,
            height,
            layer: layer.to_string(),
            data,
            properties: BTreeMap::new(),
            site: None,
            host: None,
        });
        id
    }

    /// Adds a copy of `source` at a new origin under a new name, keeping its definition and properties.
    pub fn add_copy(&mut self, source: &Element, name: &str, origin: Pt) -> ElementId {
        let id = self.add(&source.category, name, origin, source.width, source.height, &source.layer, source.data.clone());
        if let Some(e) = self.find_mut(id) {
            e.properties = source.properties.clone();
            e.site = source.site.clone();
            e.host = source.host;
        }
        id
    }

    pub fn remove(&mut self, id: ElementId) -> bool {
        let before = self.elements.len();
        self.elements.retain(|e| e.id != id);
        self.selection.retain(|s| *s != id);
        self.elements.len() != before
    }

    pub fn ensure_layer(&mut self, name: &str) {
        if !self.layers.iter().any(|l| l.name == name) {
            self.layers.push(Layer { name: name.to_string(), visible: true, locked: false });
        }
    }

    pub fn layer_visible(&self, name: &str) -> bool {
        self.layers.iter().find(|l| l.name == name).map(|l| l.visible).unwrap_or(true)
    }

    fn pickable(&self, e: &Element) -> bool {
        self.layers.iter().find(|l| l.name == e.layer).map(|l| l.visible && !l.locked).unwrap_or(true)
    }

    /// A name not yet used in the document: `ADS-001` → `ADS-002`, `R61` → `R61-2`.
    pub fn unique_name(&self, name: &str) -> String {
        let used = |n: &str| self.elements.iter().any(|e| e.name.eq_ignore_ascii_case(n));
        if !used(name) {
            return name.to_string();
        }
        let digits = name.chars().rev().take_while(|c| c.is_ascii_digit()).count();
        let split = name.len() - digits;
        if digits > 0 && split > 0 && name[..split].ends_with('-') {
            if let Ok(n) = name[split..].parse::<u64>() {
                for i in n + 1..n + 100_000 {
                    let c = format!("{}{:0width$}", &name[..split], i, width = digits);
                    if !used(&c) {
                        return c;
                    }
                }
            }
        }
        (2..).map(|i| format!("{name}-{i}")).find(|c| !used(c)).unwrap()
    }

    /// The union of every element's bounds, or None when the document is empty.
    pub fn extents(&self) -> Option<BBox> {
        let mut it = self.elements.iter().map(|e| e.bounds());
        let first = it.next()?;
        Some(it.fold(first, |a, b| a.union(&b)))
    }

    /// The topmost pickable element whose bounds, grown by the pickbox half-size, contain the point.
    pub fn pick(&self, world: Pt, pick_radius: f64) -> Option<ElementId> {
        self.elements
            .iter()
            .rev()
            .find(|e| self.pickable(e) && e.bounds().inflate(pick_radius).contains_point(world))
            .map(|e| e.id)
    }

    /// Elements caught by a selection box: a window takes only those wholly inside; a crossing
    /// also takes those the box touches.
    pub fn in_box(&self, area: &BBox, crossing: bool) -> Vec<ElementId> {
        self.elements
            .iter()
            .filter(|e| self.pickable(e))
            .filter(|e| if crossing { area.intersects(&e.bounds()) } else { area.contains_box(&e.bounds()) })
            .map(|e| e.id)
            .collect()
    }

    pub fn select(&mut self, ids: &[ElementId], mode: SelectMode) {
        let mut list: Vec<ElementId> = Vec::new();
        for id in ids {
            if self.find(*id).is_some() && !list.contains(id) {
                list.push(*id);
            }
        }
        match mode {
            SelectMode::Replace => self.selection = list,
            SelectMode::Add => {
                for id in list {
                    self.selection.retain(|s| *s != id); // re-picking makes it the primary
                    self.selection.push(id);
                }
            }
            SelectMode::Remove => self.selection.retain(|s| !list.contains(s)),
        }
    }

    pub fn select_all(&mut self) {
        let ids: Vec<ElementId> = self.elements.iter().filter(|e| self.pickable(e)).map(|e| e.id).collect();
        self.selection = ids;
    }

    pub fn clear_selection(&mut self) {
        self.selection.clear();
    }

    pub fn translate(&mut self, ids: &[ElementId], dx: f64, dy: f64) {
        for e in self.elements.iter_mut().filter(|e| ids.contains(&e.id)) {
            e.origin = Pt::new(e.origin.x + dx, e.origin.y + dy);
        }
    }

    /// Where automatic arrangement puts the next element: left to right along a row, starting a
    /// new row below once the row is `ARRANGE_ROW_WIDTH` wide. Rows grow downwards so the first
    /// element stays at the origin.
    pub fn next_free_origin(&self, width: f64, _height: f64) -> Pt {
        let Some(last) = self.elements.last().map(|e| e.bounds()) else {
            return Pt::new(0.0, 0.0);
        };
        let x = last.max.x + Self::ARRANGE_GAP;
        if x + width <= Self::ARRANGE_ROW_WIDTH {
            return Pt::new(x, last.min.y);
        }
        let bottom = self.elements.iter().map(|e| e.origin.y).fold(f64::INFINITY, f64::min);
        Pt::new(0.0, bottom - Self::ARRANGE_GAP - _height)
    }

    /// Lays every element out again in rows, in document order.
    pub fn arrange(&mut self) {
        let mut plan = Document::default();
        for i in 0..self.elements.len() {
            let (w, h) = (self.elements[i].width, self.elements[i].height);
            let at = plan.next_free_origin(w, h);
            plan.add("", "", at, w, h, "0", serde_json::Value::Null);
            self.elements[i].origin = at;
        }
    }

    /// The elements only: what undo snapshots and what a file stores.
    pub fn snapshot(&self) -> Vec<Element> {
        self.elements.clone()
    }

    /// Replaces the elements (undo, redo). Selection is cleared; ids are preserved.
    pub fn restore(&mut self, elements: Vec<Element>) {
        self.next_id = self.next_id.max(elements.iter().map(|e| e.id.0 + 1).max().unwrap_or(1));
        self.elements = elements;
        self.selection.clear();
    }

    pub fn to_json(&self) -> String {
        let dto = FileDto {
            format: FORMAT_NAME.into(),
            version: FORMAT_VERSION,
            units: "mm".into(),
            standard: self.standard.clone(),
            layers: self.layers.clone(),
            elements: self.elements.clone(),
        };
        serde_json::to_string_pretty(&dto).expect("a document always serialises")
    }

    pub fn from_json(text: &str) -> Result<Document, DocumentError> {
        let dto: FileDto = serde_json::from_str(text).map_err(|e| DocumentError::Format(e.to_string()))?;
        if dto.format != FORMAT_NAME {
            return Err(DocumentError::Format(format!("format is '{}', expected '{FORMAT_NAME}'", dto.format)));
        }
        if dto.version > FORMAT_VERSION {
            return Err(DocumentError::Format(format!("file version {} is newer than this program reads ({FORMAT_VERSION})", dto.version)));
        }
        if dto.units != "mm" {
            return Err(DocumentError::Format(format!("units '{}' are not supported; expected mm", dto.units)));
        }
        let mut doc = Document { layers: dto.layers, standard: dto.standard, ..Document::default() };
        if doc.layers.is_empty() {
            doc.layers.push(Layer { name: "0".into(), visible: true, locked: false });
        }
        for e in &dto.elements {
            doc.ensure_layer(&e.layer);
        }
        doc.restore(dto.elements);
        Ok(doc)
    }

    pub fn save(&self, path: &Path) -> Result<(), DocumentError> {
        Ok(std::fs::write(path, self.to_json())?)
    }

    pub fn load(path: &Path) -> Result<Document, DocumentError> {
        Document::from_json(&std::fs::read_to_string(path)?)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn three() -> Document {
        let mut d = Document::default();
        d.add("sign", "A-001", Pt::new(0.0, 0.0), 1000.0, 500.0, "Signs", json!({}));
        d.add("sign", "A-002", Pt::new(1500.0, 0.0), 1000.0, 500.0, "Signs", json!({}));
        d.add("sign", "R61", Pt::new(800.0, 200.0), 400.0, 400.0, "Signs", json!({})); // overlaps 1, on top
        d
    }

    fn ids(v: &[u64]) -> Vec<ElementId> {
        v.iter().map(|i| ElementId(*i)).collect()
    }

    #[test]
    fn pick_returns_the_topmost_and_honours_the_pickbox() {
        let d = three();
        assert_eq!(d.pick(Pt::new(900.0, 300.0), 0.0), Some(ElementId(3)));
        assert_eq!(d.pick(Pt::new(100.0, 100.0), 0.0), Some(ElementId(1)));
        assert_eq!(d.pick(Pt::new(1250.0, 100.0), 0.0), None);
        assert_eq!(d.pick(Pt::new(1010.0, 100.0), 15.0), Some(ElementId(1)));
        assert_eq!(d.pick(Pt::new(1010.0, 100.0), 5.0), None);
    }

    #[test]
    fn window_takes_enclosed_crossing_takes_touched() {
        let d = three();
        assert_eq!(d.in_box(&BBox::new(-10.0, -10.0, 1300.0, 700.0), false), ids(&[1, 3]));
        let cut = BBox::new(900.0, 100.0, 1600.0, 300.0);
        assert!(d.in_box(&cut, false).is_empty());
        assert_eq!(d.in_box(&cut, true), ids(&[1, 2, 3]));
    }

    #[test]
    fn hidden_and_locked_layers_cannot_be_picked() {
        let mut d = three();
        d.layers_mut().iter_mut().find(|l| l.name == "Signs").unwrap().locked = true;
        assert_eq!(d.pick(Pt::new(100.0, 100.0), 0.0), None);
        d.select_all();
        assert!(d.selection().is_empty());
    }

    #[test]
    fn selection_modes_and_primary() {
        let mut d = three();
        d.select(&ids(&[1]), SelectMode::Replace);
        d.select(&ids(&[2]), SelectMode::Add);
        assert_eq!(d.selection(), ids(&[1, 2]));
        d.select(&ids(&[1]), SelectMode::Add);
        assert_eq!(d.primary(), Some(ElementId(1)));
        d.select(&ids(&[2]), SelectMode::Remove);
        assert_eq!(d.selection(), ids(&[1]));
        d.select(&ids(&[99]), SelectMode::Replace);
        assert!(d.selection().is_empty());
    }

    #[test]
    fn translate_remove_extents() {
        let mut d = three();
        assert_eq!(d.extents(), Some(BBox::new(0.0, 0.0, 2500.0, 600.0)));
        d.translate(&ids(&[1, 2]), 100.0, -50.0);
        assert_eq!(d.find(ElementId(1)).unwrap().origin, Pt::new(100.0, -50.0));
        assert_eq!(d.find(ElementId(3)).unwrap().origin, Pt::new(800.0, 200.0));
        d.select(&ids(&[1, 2]), SelectMode::Replace);
        assert!(d.remove(ElementId(1)));
        assert_eq!(d.selection(), ids(&[2]));
        assert!(!d.remove(ElementId(1)));
    }

    #[test]
    fn unique_names() {
        let d = three();
        assert_eq!(d.unique_name("A-001"), "A-003");
        assert_eq!(d.unique_name("R61"), "R61-2");
        assert_eq!(d.unique_name("W1"), "W1");
    }

    #[test]
    fn auto_arrange_fills_a_row_then_wraps_below() {
        let mut d = Document::default();
        assert_eq!(d.next_free_origin(3000.0, 1000.0), Pt::new(0.0, 0.0));
        for _ in 0..3 {
            let at = d.next_free_origin(3000.0, 1000.0);
            d.add("sign", "x", at, 3000.0, 1000.0, "0", json!({}));
        }
        assert_eq!(d.elements()[1].origin, Pt::new(3500.0, 0.0));
        assert_eq!(d.next_free_origin(3000.0, 800.0), Pt::new(0.0, -1300.0));
    }

    #[test]
    fn file_round_trip_keeps_ids_data_and_properties() {
        let mut d = three();
        d.standard = "UK-TSM-2019".into();
        d.find_mut(ElementId(2)).unwrap().properties.insert("Material".into(), "Class RA2".into());
        d.find_mut(ElementId(2)).unwrap().data = json!({"kind": "StackType", "speed_kph": 60});
        let back = Document::from_json(&d.to_json()).unwrap();
        assert_eq!(back.elements(), d.elements());
        assert_eq!(back.standard, "UK-TSM-2019");
        assert!(back.layers().iter().any(|l| l.name == "Signs"));
        // New ids continue after the highest stored one.
        let mut back = back;
        assert_eq!(back.add("sign", "n", Pt::default(), 1.0, 1.0, "0", json!({})), ElementId(4));
    }

    #[test]
    fn site_placement_and_host_round_trip_and_stay_out_of_old_style_files() {
        let mut d = three();
        // Untouched elements serialise exactly as before these fields existed.
        assert!(!d.to_json().contains("\"site\"") && !d.to_json().contains("\"host\""));
        let e = d.find_mut(ElementId(2)).unwrap();
        e.site = Some(SitePlacement { x: 1.0e6, y: -2.5e5, z: 0.0, rotation_deg: 37.5, alignment: Some("A1".into()), station_m: Some(1250.0), offset_m: Some(-4.5) });
        e.host = Some(ElementId(1));
        let back = Document::from_json(&d.to_json()).unwrap();
        assert_eq!(back.elements(), d.elements());
        assert_eq!(back.find(ElementId(2)).unwrap().host, Some(ElementId(1)));
    }

    #[test]
    fn rejects_foreign_and_newer_files() {
        assert!(Document::from_json("{}").is_err());
        assert!(Document::from_json(r#"{"format":"x","version":1,"units":"mm","layers":[],"elements":[]}"#).is_err());
        assert!(Document::from_json(r#"{"format":"afrobim","version":99,"units":"mm","layers":[],"elements":[]}"#).is_err());
    }
}
