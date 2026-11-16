//! Document-level project settings.
//!
//! The kernel had no store for settings that belong to the whole document rather than to one
//! element. This is a minimal one, following the pattern the sheets crate uses for
//! `ProjectInfo`: one element of category [`PROJECT_SETTINGS_CATEGORY`] whose `data` is a JSON
//! object, one key per setting. A setting is read with [`project_setting`] and written with
//! [`set_project_setting`]; the element is added the first time something is written and is
//! saved with the document like any other element.
//!
//! The element draws nothing. [`ProjectSettingsModule`] is the module for the category; a front
//! end registers it so the element does not report "no module installed".

use std::collections::BTreeMap;

use afrobim_draw::{Drawing, Extents, Pt2};
use serde_json::{Map, Value};

use crate::{Document, Element, ElementId, Generated, Module, ProjectCrs, Pt};

/// The element category that holds the project settings.
pub const PROJECT_SETTINGS_CATEGORY: &str = "project_settings";
/// The layer the settings element is placed on.
pub const PROJECT_SETTINGS_LAYER: &str = "Project";

/// The module for `project_settings` elements. It draws nothing and lists the keys it holds.
#[derive(Debug, Clone, Copy, Default)]
pub struct ProjectSettingsModule;

impl Module for ProjectSettingsModule {
    fn category(&self) -> &str {
        PROJECT_SETTINGS_CATEGORY
    }

    fn display_name(&self) -> &str {
        "Project settings"
    }

    fn generate(&self, element: &Element) -> Result<Generated, String> {
        let map = element.data.as_object().ok_or("The project settings are not a JSON object.")?;
        let mut derived = BTreeMap::new();
        derived.insert("Settings".to_string(), map.keys().cloned().collect::<Vec<_>>().join(", "));
        Ok(Generated {
            drawing: Drawing { primitives: vec![], extents: Extents { min: Pt2::new(0.0, 0.0), max: Pt2::new(1.0, 1.0) } },
            derived,
            ..Generated::default()
        })
    }
}

/// The id of the document's project settings element, if it has one.
pub fn project_settings_id(doc: &Document) -> Option<ElementId> {
    doc.elements().iter().find(|e| e.category == PROJECT_SETTINGS_CATEGORY).map(|e| e.id)
}

/// The value stored under `key`, or `None` when there is none.
pub fn project_setting(doc: &Document, key: &str) -> Option<Value> {
    let e = doc.find(project_settings_id(doc)?)?;
    e.data.as_object()?.get(key).cloned()
}

/// Stores `value` under `key`, adding the settings element if the document has none. Other keys
/// are kept. Returns the id of the settings element.
pub fn set_project_setting(doc: &mut Document, key: &str, value: Value) -> ElementId {
    if let Some(id) = project_settings_id(doc) {
        if let Some(e) = doc.find_mut(id) {
            let mut map = match e.data.take() {
                Value::Object(m) => m,
                _ => Map::new(),
            };
            map.insert(key.to_string(), value);
            e.data = Value::Object(map);
        }
        return id;
    }
    doc.ensure_layer(PROJECT_SETTINGS_LAYER);
    let mut map = Map::new();
    map.insert(key.to_string(), value);
    doc.add(PROJECT_SETTINGS_CATEGORY, "Project settings", Pt::new(0.0, 0.0), 1.0, 1.0, PROJECT_SETTINGS_LAYER, Value::Object(map))
}

/// The project setting key that holds the coordinate system.
pub const PROJECT_CRS_KEY: &str = "project_crs";

/// The coordinate system of the project, when one is set. A file written before this existed,
/// or a value that cannot be read, gives `None`: the project is then undeclared.
pub fn project_crs(doc: &Document) -> Option<ProjectCrs> {
    serde_json::from_value(project_setting(doc, PROJECT_CRS_KEY)?).ok()
}

/// Sets the coordinate system of the project. Model data is not touched: this only records what
/// the coordinates mean.
pub fn set_project_crs(doc: &mut Document, crs: &ProjectCrs) -> ElementId {
    set_project_setting(doc, PROJECT_CRS_KEY, serde_json::to_value(crs).expect("a coordinate system serialises"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn settings_are_added_replaced_and_saved_with_the_document() {
        let mut doc = Document::default();
        assert_eq!(project_setting(&doc, "a"), None);
        let id = set_project_setting(&mut doc, "a", json!(1));
        assert_eq!(set_project_setting(&mut doc, "b", json!("x")), id);
        assert_eq!(set_project_setting(&mut doc, "a", json!(2)), id);
        assert_eq!(doc.elements().iter().filter(|e| e.category == PROJECT_SETTINGS_CATEGORY).count(), 1);
        let back = Document::from_json(&doc.to_json()).unwrap();
        assert_eq!(project_setting(&back, "a"), Some(json!(2)));
        assert_eq!(project_setting(&back, "b"), Some(json!("x")));
        let g = ProjectSettingsModule.generate(back.find(id).unwrap()).unwrap();
        assert_eq!(g.derived["Settings"], "a, b");
    }

    #[test]
    fn the_project_crs_is_stored_in_the_document_and_old_files_read_as_undeclared() {
        let mut doc = Document::default();
        assert_eq!(project_crs(&doc), None);
        let crs = ProjectCrs::from_epsg(21036).unwrap();
        set_project_crs(&mut doc, &crs);
        let back = Document::from_json(&doc.to_json()).unwrap();
        assert_eq!(project_crs(&back), Some(crs));
        // A file with a settings element but no CRS key, and one with an unreadable value.
        let mut old = Document::default();
        set_project_setting(&mut old, "a", json!(1));
        assert_eq!(project_crs(&Document::from_json(&old.to_json()).unwrap()), None);
        set_project_setting(&mut old, PROJECT_CRS_KEY, json!("garbage"));
        assert_eq!(project_crs(&old), None);
    }
}
