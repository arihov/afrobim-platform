// SPDX-License-Identifier: LicenseRef-AfroBIM-Proprietary
// Copyright (c) 2026 AfroBIM. All rights reserved.
//! Project information for title blocks, stored in the document as one element of category
//! `project_info`.

use afrobim_draw::{Drawing, Extents, Pt2};
use afrobim_kernel::{Document, Element, ElementId, Generated, Module, Pt};
use serde::{Deserialize, Serialize};

/// The element category that holds the project information.
pub const PROJECT_INFO_CATEGORY: &str = "project_info";
/// The layer it is placed on.
pub const PROJECT_INFO_LAYER: &str = "Sheets";

/// One line of a revision table.
#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
pub struct Revision {
    pub rev: String,
    pub date: String,
    pub description: String,
    pub by: String,
    pub checked: String,
}

/// A free field the user adds to the title block: a caption and its text.
#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
pub struct UserField {
    pub label: String,
    pub value: String,
}

/// What every title block of the project says. All text is entered by the user; nothing is
/// filled in. Any sheet can override the person and date fields (see `SheetFields`).
#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
pub struct ProjectInfo {
    pub project: String,
    pub client: String,
    pub contract_number: String,
    /// The consulting engineer (printed by the MoWT title block).
    #[serde(default)]
    pub consultant: String,
    /// The contractor (printed by the MoWT title block).
    #[serde(default)]
    pub contractor: String,
    pub drawn_by: String,
    pub checked_by: String,
    pub approved_by: String,
    /// Issue date as it is to be printed.
    pub date: String,
    pub revisions: Vec<Revision>,
    pub user_fields: Vec<UserField>,
    /// Caption inside the logo placeholder rectangle, e.g. "LOGO". Empty prints the rectangle blank.
    pub logo_text: String,
}

impl ProjectInfo {
    pub fn to_value(&self) -> serde_json::Value {
        serde_json::to_value(self).expect("project information always serialises")
    }

    pub fn from_value(v: &serde_json::Value) -> Result<ProjectInfo, String> {
        serde_json::from_value(v.clone()).map_err(|e| format!("project information cannot be read: {e}"))
    }

    /// The text of the last revision row, or empty.
    pub fn current_revision(&self) -> String {
        self.revisions.last().map(|r| r.rev.clone()).unwrap_or_default()
    }

    /// The value of a user field by caption.
    pub fn user_field(&self, label: &str) -> Option<&str> {
        self.user_fields.iter().find(|f| f.label == label).map(|f| f.value.as_str())
    }
}

/// The module for `project_info` elements. It draws nothing; it only reports.
#[derive(Debug, Clone, Copy, Default)]
pub struct ProjectInfoModule;

impl Module for ProjectInfoModule {
    fn category(&self) -> &str {
        PROJECT_INFO_CATEGORY
    }

    fn display_name(&self) -> &str {
        "Project information"
    }

    fn generate(&self, element: &Element) -> Result<Generated, String> {
        let info = ProjectInfo::from_value(&element.data)?;
        let mut derived = std::collections::BTreeMap::new();
        derived.insert("Project".to_string(), info.project.clone());
        derived.insert("Revisions".to_string(), info.revisions.len().to_string());
        Ok(Generated { drawing: Drawing { primitives: vec![], extents: Extents { min: Pt2::new(0.0, 0.0), max: Pt2::new(1.0, 1.0) } }, derived, ..Generated::default() })
    }
}

/// The id of the document's project information element, if it has one.
pub fn project_info_id(doc: &Document) -> Option<ElementId> {
    doc.elements().iter().find(|e| e.category == PROJECT_INFO_CATEGORY).map(|e| e.id)
}

/// The project information of the document; empty when none has been entered.
pub fn project_info(doc: &Document) -> ProjectInfo {
    project_info_id(doc).and_then(|id| doc.find(id)).and_then(|e| ProjectInfo::from_value(&e.data).ok()).unwrap_or_default()
}

/// Stores the project information: replaces the data of the existing element, or adds one.
pub fn set_project_info(doc: &mut Document, info: &ProjectInfo) -> ElementId {
    if let Some(id) = project_info_id(doc) {
        if let Some(e) = doc.find_mut(id) {
            e.data = info.to_value();
        }
        return id;
    }
    doc.ensure_layer(PROJECT_INFO_LAYER);
    doc.add(PROJECT_INFO_CATEGORY, "Project information", Pt::new(0.0, 0.0), 1.0, 1.0, PROJECT_INFO_LAYER, info.to_value())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn project_information_survives_a_save_and_load_of_the_document() {
        let mut doc = Document::default();
        let info = ProjectInfo {
            project: "Rural road".into(),
            client: "Client".into(),
            contract_number: "C-1".into(),
            revisions: vec![Revision { rev: "A".into(), date: "2026-01-01".into(), description: "First issue".into(), by: "AB".into(), checked: "CD".into() }],
            user_fields: vec![UserField { label: "Phase".into(), value: "Detailed design".into() }],
            ..ProjectInfo::default()
        };
        let id = set_project_info(&mut doc, &info);
        // Setting again replaces rather than adds.
        let mut changed = info.clone();
        changed.client = "Other".into();
        assert_eq!(set_project_info(&mut doc, &changed), id);
        assert_eq!(doc.elements().iter().filter(|e| e.category == PROJECT_INFO_CATEGORY).count(), 1);
        let back = Document::from_json(&doc.to_json()).unwrap();
        assert_eq!(project_info(&back), changed);
        assert_eq!(project_info(&back).current_revision(), "A");
        assert_eq!(project_info(&back).user_field("Phase"), Some("Detailed design"));
        assert_eq!(project_info(&Document::default()), ProjectInfo::default());
    }
}
