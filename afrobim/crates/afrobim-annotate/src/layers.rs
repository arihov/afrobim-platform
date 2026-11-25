// SPDX-License-Identifier: LicenseRef-AfroBIM-Proprietary
// Copyright (c) 2026 AfroBIM. All rights reserved.
//! Layer table helpers.
//!
//! The kernel models a layer as a name with `visible` and `locked` flags and lets an element name
//! its layer. It has no colour, no line weight, no rename and no delete. This module adds those
//! on top without touching the kernel: colour and line weight live in the annotation settings
//! element ([`crate::settings`]); rename and delete rewrite the layer name of the elements.
//!
//! Rules (this crate's, stated):
//!
//! - Layer `0` cannot be renamed or deleted; neither can the hidden settings layer.
//! - Names are unique ignoring case and may not contain the characters DXF forbids in a table
//!   name: `" ; > \ ` < = : / ? *` (the set `ezdxf` 1.4.5 refuses in `ezdxf.lldxf.validator`).
//! - Deleting a layer that has elements is refused, unless the caller names a layer to move them to.
//!
//! The kernel's layer flags are not part of the document's undo snapshot (it snapshots elements
//! only), so changing visible or locked is not undone by the kernel's undo. The properties kept
//! here are inside the settings element and are.

use afrobim_kernel::{Document, Layer};

use crate::settings::{self, LayerProps, SETTINGS_LAYER};

#[derive(Debug, Clone, PartialEq, thiserror::Error)]
pub enum LayerError {
    #[error("'{0}' is not a valid layer name: it is empty or contains one of \" ; > \\ ` < = : / ? *")]
    BadName(String),
    #[error("there is already a layer called '{0}'")]
    Exists(String),
    #[error("there is no layer called '{0}'")]
    Missing(String),
    #[error("layer '{0}' is built in and cannot be renamed or deleted")]
    Protected(String),
    #[error("layer '{name}' holds {count} element(s); delete it only by moving them to another layer")]
    InUse { name: String, count: usize },
}

/// What a layer manager shows for one layer.
#[derive(Debug, Clone, PartialEq)]
pub struct LayerInfo {
    pub name: String,
    pub visible: bool,
    pub locked: bool,
    pub colour: [u8; 3],
    pub lineweight_mm: f64,
    /// Elements on the layer.
    pub elements: usize,
}

const FORBIDDEN: [char; 11] = ['"', ';', '>', '\\', '`', '<', '=', ':', '/', '?', '*'];

pub fn valid_name(name: &str) -> bool {
    let t = name.trim();
    !t.is_empty() && t == name && !name.contains(FORBIDDEN) && !name.chars().any(|c| c.is_control())
}

fn find<'a>(doc: &'a Document, name: &str) -> Option<&'a Layer> {
    doc.layers().iter().find(|l| l.name.eq_ignore_ascii_case(name))
}

fn protected(name: &str) -> bool {
    name == "0" || name == SETTINGS_LAYER
}

fn count(doc: &Document, name: &str) -> usize {
    doc.elements().iter().filter(|e| e.layer == name).count()
}

/// Every layer except the hidden settings layer, in table order.
pub fn list(doc: &Document) -> Vec<LayerInfo> {
    let s = settings::load(doc);
    doc.layers()
        .iter()
        .filter(|l| l.name != SETTINGS_LAYER)
        .map(|l| {
            let p = s.layers.get(&l.name).cloned().unwrap_or_default();
            LayerInfo { name: l.name.clone(), visible: l.visible, locked: l.locked, colour: p.colour, lineweight_mm: p.lineweight_mm, elements: count(doc, &l.name) }
        })
        .collect()
}

/// Adds a layer with the given properties. Visible and unlocked.
pub fn create(doc: &mut Document, name: &str, props: LayerProps) -> Result<(), LayerError> {
    if !valid_name(name) {
        return Err(LayerError::BadName(name.to_string()));
    }
    if find(doc, name).is_some() {
        return Err(LayerError::Exists(name.to_string()));
    }
    doc.ensure_layer(name);
    set_props(doc, name, props);
    Ok(())
}

/// Renames a layer, moving its elements and its properties with it.
pub fn rename(doc: &mut Document, old: &str, new: &str) -> Result<(), LayerError> {
    let Some(layer) = find(doc, old) else { return Err(LayerError::Missing(old.to_string())) };
    let old = layer.name.clone();
    if protected(&old) {
        return Err(LayerError::Protected(old));
    }
    if !valid_name(new) {
        return Err(LayerError::BadName(new.to_string()));
    }
    if find(doc, new).is_some_and(|l| !l.name.eq_ignore_ascii_case(&old) || l.name == new) {
        return Err(LayerError::Exists(new.to_string()));
    }
    if let Some(l) = doc.layers_mut().iter_mut().find(|l| l.name == old) {
        l.name = new.to_string();
    }
    let ids: Vec<_> = doc.elements().iter().filter(|e| e.layer == old).map(|e| e.id).collect();
    for id in ids {
        if let Some(e) = doc.find_mut(id) {
            e.layer = new.to_string();
        }
    }
    let mut s = settings::load(doc);
    if let Some(p) = s.layers.remove(&old) {
        s.layers.insert(new.to_string(), p);
        settings::store(doc, &s);
    }
    Ok(())
}

/// What happens to the elements of a layer being deleted.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum OnDelete {
    /// Refuse when the layer has any element.
    RefuseUnlessEmpty,
    /// Move the elements to this layer (which must exist) first.
    MoveTo(String),
}

/// Deletes a layer. Returns how many elements were moved.
pub fn delete(doc: &mut Document, name: &str, policy: &OnDelete) -> Result<usize, LayerError> {
    let Some(layer) = find(doc, name) else { return Err(LayerError::Missing(name.to_string())) };
    let name = layer.name.clone();
    if protected(&name) {
        return Err(LayerError::Protected(name));
    }
    let n = count(doc, &name);
    let mut moved = 0;
    if n > 0 {
        match policy {
            OnDelete::RefuseUnlessEmpty => return Err(LayerError::InUse { name, count: n }),
            OnDelete::MoveTo(target) => {
                let Some(t) = find(doc, target) else { return Err(LayerError::Missing(target.clone())) };
                let target = t.name.clone();
                if target == name {
                    return Err(LayerError::InUse { name, count: n });
                }
                let ids: Vec<_> = doc.elements().iter().filter(|e| e.layer == name).map(|e| e.id).collect();
                for id in ids {
                    if let Some(e) = doc.find_mut(id) {
                        e.layer = target.clone();
                        moved += 1;
                    }
                }
            }
        }
    }
    doc.layers_mut().retain(|l| l.name != name);
    let mut s = settings::load(doc);
    if s.layers.remove(&name).is_some() {
        settings::store(doc, &s);
    }
    Ok(moved)
}

pub fn set_props(doc: &mut Document, name: &str, props: LayerProps) {
    let mut s = settings::load(doc);
    s.layers.insert(name.to_string(), props);
    settings::store(doc, &s);
}

pub fn props(doc: &Document, name: &str) -> LayerProps {
    settings::load(doc).layers.get(name).cloned().unwrap_or_default()
}

pub fn set_colour(doc: &mut Document, name: &str, colour: [u8; 3]) -> Result<(), LayerError> {
    let mut p = props_checked(doc, name)?;
    p.colour = colour;
    set_props(doc, name, p);
    Ok(())
}

pub fn set_lineweight(doc: &mut Document, name: &str, lineweight_mm: f64) -> Result<(), LayerError> {
    let mut p = props_checked(doc, name)?;
    p.lineweight_mm = lineweight_mm.max(0.0);
    set_props(doc, name, p);
    Ok(())
}

fn props_checked(doc: &Document, name: &str) -> Result<LayerProps, LayerError> {
    if find(doc, name).is_none() {
        return Err(LayerError::Missing(name.to_string()));
    }
    Ok(props(doc, name))
}

pub fn set_visible(doc: &mut Document, name: &str, visible: bool) -> Result<(), LayerError> {
    match doc.layers_mut().iter_mut().find(|l| l.name.eq_ignore_ascii_case(name)) {
        Some(l) => {
            l.visible = visible;
            Ok(())
        }
        None => Err(LayerError::Missing(name.to_string())),
    }
}

pub fn set_locked(doc: &mut Document, name: &str, locked: bool) -> Result<(), LayerError> {
    match doc.layers_mut().iter_mut().find(|l| l.name.eq_ignore_ascii_case(name)) {
        Some(l) => {
            l.locked = locked;
            Ok(())
        }
        None => Err(LayerError::Missing(name.to_string())),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use afrobim_draw::Pt2;

    fn doc() -> Document {
        let mut d = Document::default();
        d.add("sign", "A", Pt2::new(0.0, 0.0), 1.0, 1.0, "Signs", serde_json::Value::Null);
        d.add("sign", "B", Pt2::new(5.0, 0.0), 1.0, 1.0, "Signs", serde_json::Value::Null);
        d
    }

    #[test]
    fn create_validates_and_refuses_duplicates() {
        let mut d = doc();
        create(&mut d, "DIMS", LayerProps { colour: [255, 0, 0], lineweight_mm: 0.25 }).unwrap();
        assert_eq!(props(&d, "DIMS").lineweight_mm, 0.25);
        assert_eq!(create(&mut d, "dims", LayerProps::default()), Err(LayerError::Exists("dims".into())));
        for bad in ["", " x", "a/b", "a:b", "a*", "a\"b", "q?"] {
            assert!(matches!(create(&mut d, bad, LayerProps::default()), Err(LayerError::BadName(_))), "{bad}");
        }
        assert!(valid_name("ROAD-EDGE_1 (new)"));
    }

    #[test]
    fn rename_moves_elements_and_properties() {
        let mut d = doc();
        set_colour(&mut d, "Signs", [0, 0, 255]).unwrap();
        rename(&mut d, "Signs", "SIGNS-NEW").unwrap();
        assert!(d.elements().iter().filter(|e| e.category == "sign").all(|e| e.layer == "SIGNS-NEW"));
        assert!(find(&d, "Signs").is_none());
        assert_eq!(props(&d, "SIGNS-NEW").colour, [0, 0, 255]);
        assert_eq!(rename(&mut d, "0", "Zero"), Err(LayerError::Protected("0".into())));
        assert_eq!(rename(&mut d, "nope", "x"), Err(LayerError::Missing("nope".into())));
        create(&mut d, "Other", LayerProps::default()).unwrap();
        assert_eq!(rename(&mut d, "Other", "signs-new"), Err(LayerError::Exists("signs-new".into())));
        // Changing only the case of a layer's own name is allowed.
        rename(&mut d, "Other", "OTHER").unwrap();
    }

    #[test]
    fn delete_refuses_unless_empty_or_moves_the_elements() {
        let mut d = doc();
        assert_eq!(delete(&mut d, "Signs", &OnDelete::RefuseUnlessEmpty), Err(LayerError::InUse { name: "Signs".into(), count: 2 }));
        assert_eq!(delete(&mut d, "Signs", &OnDelete::MoveTo("Nowhere".into())), Err(LayerError::Missing("Nowhere".into())));
        assert_eq!(delete(&mut d, "Signs", &OnDelete::MoveTo("Signs".into())), Err(LayerError::InUse { name: "Signs".into(), count: 2 }));
        assert_eq!(delete(&mut d, "Signs", &OnDelete::MoveTo("0".into())), Ok(2));
        assert!(d.elements().iter().filter(|e| e.category == "sign").all(|e| e.layer == "0"));
        assert!(find(&d, "Signs").is_none());
        create(&mut d, "Empty", LayerProps::default()).unwrap();
        assert_eq!(delete(&mut d, "Empty", &OnDelete::RefuseUnlessEmpty), Ok(0));
        assert_eq!(delete(&mut d, "0", &OnDelete::RefuseUnlessEmpty), Err(LayerError::Protected("0".into())));
    }

    #[test]
    fn visibility_lock_and_listing_hide_the_settings_layer() {
        let mut d = doc();
        set_visible(&mut d, "Signs", false).unwrap();
        set_locked(&mut d, "Signs", true).unwrap();
        let l = list(&d);
        assert!(l.iter().all(|i| i.name != SETTINGS_LAYER));
        let s = l.iter().find(|i| i.name == "Signs").unwrap();
        assert!(!s.visible && s.locked && s.elements == 2);
        settings::store(&mut d, &settings::AnnotationSettings::default());
        assert_eq!(set_visible(&mut d, "zzz", true), Err(LayerError::Missing("zzz".into())));
        assert_eq!(rename(&mut d, SETTINGS_LAYER, "x"), Err(LayerError::Protected(SETTINGS_LAYER.into())));
    }
}
