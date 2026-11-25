// SPDX-License-Identifier: LicenseRef-AfroBIM-Proprietary
// Copyright (c) 2026 AfroBIM. All rights reserved.
//! Annotation settings stored in the document: display units, annotation scale, the named text
//! and dimension styles, and the layer properties the kernel does not model.
//!
//! The kernel [`Document`] has no settings store, so they live in one element of category
//! [`SETTINGS_CATEGORY`] on the hidden layer [`SETTINGS_LAYER`]. That makes them part of the file,
//! of undo snapshots and of copy/paste of whole documents, with no kernel change. The element
//! draws nothing and, being on a hidden layer, cannot be picked.

use std::collections::BTreeMap;

use afrobim_draw::Pt2;
use afrobim_kernel::{Document, Element, ElementId, Generated, Layer, Module};
use serde::{Deserialize, Serialize};

use crate::element;
use crate::styles::{DimStyle, HeightSpec, TextStyle};
use crate::units::DisplayUnits;

pub const SETTINGS_CATEGORY: &str = "ann-settings";
pub const SETTINGS_LAYER: &str = "AnnotationSettings";

/// What a layer looks like, beyond the visible and locked flags the kernel keeps.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct LayerProps {
    pub colour: [u8; 3],
    /// Pen width in paper millimetres; 0 means the output's default pen.
    pub lineweight_mm: f64,
}

impl Default for LayerProps {
    /// Default values: black, default pen.
    fn default() -> Self {
        LayerProps { colour: [0, 0, 0], lineweight_mm: 0.0 }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct AnnotationSettings {
    pub units: DisplayUnits,
    /// The drawing scale denominator new annotation is sized for (500 for 1:500). A default of
    /// 1 means paper sizes equal model sizes; set it before annotating.
    pub annotation_scale: f64,
    pub text_styles: Vec<TextStyle>,
    pub dim_styles: Vec<DimStyle>,
    pub current_text_style: String,
    pub current_dim_style: String,
    pub layers: BTreeMap<String, LayerProps>,
}

impl Default for AnnotationSettings {
    fn default() -> Self {
        AnnotationSettings {
            units: DisplayUnits::default(),
            annotation_scale: 1.0,
            text_styles: vec![TextStyle::default()],
            dim_styles: vec![DimStyle::default()],
            current_text_style: "Standard".into(),
            current_dim_style: "Standard".into(),
            layers: BTreeMap::new(),
        }
    }
}

impl AnnotationSettings {
    pub fn text_style(&self, name: &str) -> Option<&TextStyle> {
        self.text_styles.iter().find(|s| s.name.eq_ignore_ascii_case(name))
    }

    pub fn dim_style(&self, name: &str) -> Option<&DimStyle> {
        self.dim_styles.iter().find(|s| s.name.eq_ignore_ascii_case(name))
    }

    /// The current text style, or the first one.
    pub fn current_text(&self) -> TextStyle {
        self.text_style(&self.current_text_style).or(self.text_styles.first()).cloned().unwrap_or_default()
    }

    /// The current dimension style with its scale set to the annotation scale.
    pub fn current_dim(&self) -> DimStyle {
        let mut s = self.dim_style(&self.current_dim_style).or(self.dim_styles.first()).cloned().unwrap_or_default();
        s.scale = self.annotation_scale;
        s
    }

    /// Adds a text style, or replaces the one of the same name.
    pub fn set_text_style(&mut self, style: TextStyle) {
        match self.text_styles.iter_mut().find(|s| s.name.eq_ignore_ascii_case(&style.name)) {
            Some(s) => *s = style,
            None => self.text_styles.push(style),
        }
    }

    pub fn set_dim_style(&mut self, style: DimStyle) {
        match self.dim_styles.iter_mut().find(|s| s.name.eq_ignore_ascii_case(&style.name)) {
            Some(s) => *s = style,
            None => self.dim_styles.push(style),
        }
    }

    /// Removes a style; the last style of each kind cannot be removed. Returns whether it went.
    pub fn remove_text_style(&mut self, name: &str) -> bool {
        if self.text_styles.len() <= 1 {
            return false;
        }
        let before = self.text_styles.len();
        self.text_styles.retain(|s| !s.name.eq_ignore_ascii_case(name));
        before != self.text_styles.len()
    }

    pub fn remove_dim_style(&mut self, name: &str) -> bool {
        if self.dim_styles.len() <= 1 {
            return false;
        }
        let before = self.dim_styles.len();
        self.dim_styles.retain(|s| !s.name.eq_ignore_ascii_case(name));
        before != self.dim_styles.len()
    }
}

/// The settings element's id, if the document has one.
pub fn settings_id(doc: &Document) -> Option<ElementId> {
    doc.elements().iter().find(|e| e.category == SETTINGS_CATEGORY).map(|e| e.id)
}

/// The document's annotation settings, or the defaults when none are stored (or they cannot be read).
pub fn load(doc: &Document) -> AnnotationSettings {
    doc.elements()
        .iter()
        .find(|e| e.category == SETTINGS_CATEGORY)
        .and_then(|e| serde_json::from_value(e.data.clone()).ok())
        .unwrap_or_default()
}

/// Stores the settings in the document, creating the settings element (and its hidden layer) on
/// first use.
pub fn store(doc: &mut Document, settings: &AnnotationSettings) {
    let data = serde_json::to_value(settings).expect("settings serialise");
    if let Some(id) = settings_id(doc) {
        if let Some(e) = doc.find_mut(id) {
            e.data = data;
        }
        return;
    }
    doc.add(SETTINGS_CATEGORY, "Annotation settings", Pt2::new(0.0, 0.0), 0.0, 0.0, SETTINGS_LAYER, data);
    if let Some(l) = doc.layers_mut().iter_mut().find(|l: &&mut Layer| l.name == SETTINGS_LAYER) {
        l.visible = false;
        l.locked = true;
    }
}

/// Re-applies the document's named styles to the annotations that use them (text by style name,
/// dimensions and leaders by their style name), keeping each one's own scale, and normalises
/// them. Returns how many were changed.
pub fn restyle(doc: &mut Document) -> usize {
    let settings = load(doc);
    let ids: Vec<ElementId> = doc.elements().iter().filter(|e| element::is_annotation(e)).map(|e| e.id).collect();
    let mut changed = 0;
    for id in ids {
        let Some(e) = doc.find_mut(id) else { continue };
        let Ok(mut ann) = element::read(e) else { continue };
        let before = ann.clone();
        match &mut ann {
            element::Ann::Text(t) => {
                if let Some(s) = settings.text_style(&t.style) {
                    t.width_factor = s.width_factor;
                    t.bold = s.bold;
                    t.line_spacing = s.line_spacing;
                    if let HeightSpec::Paper { scale, .. } = t.height {
                        t.height = HeightSpec::Paper { mm: s.height_paper_mm, scale };
                    }
                }
            }
            element::Ann::Dim(d) => {
                if let Some(s) = settings.dim_style(&d.style.name) {
                    let scale = d.style.scale;
                    d.style = DimStyle { scale, ..s.clone() };
                }
            }
            element::Ann::Leader(l) => {
                if let Some(s) = settings.dim_style(&l.style.name) {
                    let scale = l.style.scale;
                    l.style = DimStyle { scale, ..s.clone() };
                }
            }
        }
        if ann != before {
            changed += 1;
            let _ = element::write(e, ann);
        }
    }
    changed
}

/// The settings element's module: it draws nothing.
#[derive(Debug, Clone, Copy, Default)]
pub struct SettingsModule;

impl Module for SettingsModule {
    fn category(&self) -> &str {
        SETTINGS_CATEGORY
    }
    fn display_name(&self) -> &str {
        "Annotation settings"
    }
    fn generate(&self, _element: &Element) -> Result<Generated, String> {
        Ok(Generated::default())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn settings_round_trip_through_the_document_file() {
        let mut doc = Document::default();
        assert_eq!(load(&doc), AnnotationSettings::default());
        let mut s = AnnotationSettings { annotation_scale: 500.0, ..Default::default() };
        s.units.length_decimals = 1;
        s.set_text_style(TextStyle { name: "Notes".into(), height_paper_mm: 3.5, ..TextStyle::default() });
        s.layers.insert("DIMS".into(), LayerProps { colour: [200, 0, 0], lineweight_mm: 0.25 });
        store(&mut doc, &s);
        store(&mut doc, &s); // second store updates, not duplicates
        assert_eq!(doc.elements().iter().filter(|e| e.category == SETTINGS_CATEGORY).count(), 1);
        let back = Document::from_json(&doc.to_json()).unwrap();
        assert_eq!(load(&back), s);
        // Hidden and locked: nothing can pick it.
        assert!(!back.layer_visible(SETTINGS_LAYER));
        assert_eq!(back.pick(Pt2::new(0.0, 0.0), 100.0), None);
    }

    #[test]
    fn styles_are_replaced_by_name_and_the_last_one_stays() {
        let mut s = AnnotationSettings::default();
        s.set_dim_style(DimStyle { name: "standard".into(), text_height_mm: 3.5, ..DimStyle::default() });
        assert_eq!(s.dim_styles.len(), 1);
        assert_eq!(s.current_dim().text_height_mm, 3.5);
        assert!(!s.remove_dim_style("Standard"));
        s.set_dim_style(DimStyle { name: "Road".into(), ..DimStyle::default() });
        assert!(s.remove_dim_style("standard"));
        assert_eq!(s.dim_styles.len(), 1);
        assert!(!s.remove_text_style("Standard"));
    }

    #[test]
    fn restyle_applies_a_changed_style_to_the_annotations_that_use_it() {
        use crate::dim::{DimData, DimKind, LinearOrient};
        use crate::geometry::pt;
        use crate::text::TextData;
        let mut doc = Document::default();
        let mut s = AnnotationSettings { annotation_scale: 100.0, ..Default::default() };
        store(&mut doc, &s);
        let d = DimData::new(DimKind::Linear { p1: pt(0.0, 0.0), p2: pt(5000.0, 0.0), line_pt: pt(0.0, 800.0), orient: LinearOrient::Horizontal }, &s.current_dim());
        let dim = element::add_dimension(&mut doc, d, "DIMS").unwrap();
        let text = element::add_text(&mut doc, TextData::new("note", pt(0.0, 2000.0), &s.current_text(), 100.0), "NOTES").unwrap();
        let height = |doc: &Document, id| doc.find(id).unwrap().height;
        let before = (height(&doc, dim), height(&doc, text));
        // Make the text taller and the dimension text taller; nothing changes until restyle runs.
        let mut ts = s.current_text();
        ts.height_paper_mm = 5.0;
        s.set_text_style(ts);
        let mut ds = s.dim_styles[0].clone();
        ds.text_height_mm = 5.0;
        ds.length_decimals = 1;
        s.set_dim_style(ds);
        store(&mut doc, &s);
        assert_eq!((height(&doc, dim), height(&doc, text)), before);
        assert_eq!(restyle(&mut doc), 2);
        assert!((doc.find(text).unwrap().height - 500.0).abs() < 1e-9); // 5 mm paper at 1:100
        let r = element::AnnotationModule::DIMENSION.generate(doc.find(dim).unwrap()).unwrap();
        assert_eq!(r.derived["text"], "5.0"); // one decimal now
        assert_eq!(restyle(&mut doc), 0); // idempotent
    }

    #[test]
    fn current_dim_takes_the_annotation_scale() {
        let s = AnnotationSettings { annotation_scale: 250.0, ..Default::default() };
        assert_eq!(s.current_dim().scale, 250.0);
        assert_eq!(s.current_dim().m(2.5), 625.0);
    }
}
