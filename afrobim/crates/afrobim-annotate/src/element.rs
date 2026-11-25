// SPDX-License-Identifier: LicenseRef-AfroBIM-Proprietary
// Copyright (c) 2026 AfroBIM. All rights reserved.
//! Annotations as document elements, and their `Module` implementations.
//!
//! One element per annotation, in one of the categories [`TEXT_CATEGORY`], [`DIM_CATEGORY`] and
//! [`LEADER_CATEGORY`]. The element's `data` is the serialised [`Ann`] in the element's *local*
//! frame: every point is relative to `element.origin`. That is what lets the kernel's
//! translate, copy and paste move an annotation correctly without knowing what it is, and
//! `Document::translate` moves its definition points with it.
//!
//! `origin`, `width` and `height` are the lower-left corner and size of everything the annotation
//! draws, text included. After you change an annotation's data call [`write`] (or [`normalize`]),
//! which recomputes them and re-bases the points so the lower-left corner is on `origin`; that is
//! the contract `Generated.drawing` needs ("the element's lower-left corner on the origin").
//!
//! Values that need the rest of the document (a station on an alignment, the mark of another
//! element) are looked up by [`refresh`], which the application calls after edits; the lookup
//! result is cached in the element so `generate` needs no document.

use std::collections::BTreeMap;

use afrobim_alignment::Alignment;
use afrobim_kernel::{Document, Element, ElementId, Generated, Module, ModuleRegistry, Note, Severity};
use serde::{Deserialize, Serialize};

use crate::dim::{station_offset_cache, DimData, DimKind};
use crate::geometry::*;
use crate::leader::{LeaderData, LeaderText};
use crate::settings::SettingsModule;
use crate::text::TextData;

pub const TEXT_CATEGORY: &str = "ann-text";
pub const DIM_CATEGORY: &str = "ann-dim";
pub const LEADER_CATEGORY: &str = "ann-leader";

/// Default layer for annotation made without one.
pub const DEFAULT_LAYER: &str = "ANNOTATION";

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum Ann {
    Text(TextData),
    Dim(DimData),
    Leader(LeaderData),
}

impl Ann {
    pub fn category(&self) -> &'static str {
        match self {
            Ann::Text(_) => TEXT_CATEGORY,
            Ann::Dim(_) => DIM_CATEGORY,
            Ann::Leader(_) => LEADER_CATEGORY,
        }
    }

    pub fn translate(&mut self, d: Pt) {
        match self {
            Ann::Text(t) => t.at = add(t.at, d),
            Ann::Dim(x) => x.translate(d),
            Ann::Leader(l) => l.translate(d),
        }
    }

    /// What it draws, in the same frame as its data.
    pub fn geometry(&self) -> Result<AnnGeometry, String> {
        match self {
            Ann::Text(t) => Ok(t.geometry()),
            Ann::Dim(d) => d.compute().map(|r| r.geometry),
            Ann::Leader(l) => l.geometry(),
        }
    }

    /// The definition points, for grips.
    pub fn grips(&self) -> Vec<Pt> {
        match self {
            Ann::Text(t) => vec![t.at],
            Ann::Dim(d) => d.points(),
            Ann::Leader(l) => l.pts.clone(),
        }
    }

    /// Moves definition point `index` (as numbered by [`grips`](Self::grips)). Returns false for
    /// a bad index.
    pub fn set_grip(&mut self, index: usize, to: Pt) -> bool {
        match self {
            Ann::Text(t) if index == 0 => {
                t.at = to;
                true
            }
            Ann::Dim(d) => match d.points_mut().into_iter().nth(index) {
                Some(p) => {
                    *p = to;
                    true
                }
                None => false,
            },
            Ann::Leader(l) if index < l.pts.len() => {
                l.pts[index] = to;
                true
            }
            _ => false,
        }
    }
}

pub fn is_annotation(e: &Element) -> bool {
    matches!(e.category.as_str(), TEXT_CATEGORY | DIM_CATEGORY | LEADER_CATEGORY)
}

/// The annotation in the element's local frame.
pub fn read(e: &Element) -> Result<Ann, String> {
    let ann: Ann = serde_json::from_value(e.data.clone()).map_err(|err| format!("'{}' is not a valid annotation: {err}", e.name))?;
    let ok = matches!((&ann, e.category.as_str()), (Ann::Text(_), TEXT_CATEGORY) | (Ann::Dim(_), DIM_CATEGORY) | (Ann::Leader(_), LEADER_CATEGORY));
    if ok {
        Ok(ann)
    } else {
        Err(format!("'{}' holds a different kind of annotation than its category says.", e.name))
    }
}

/// The annotation with its points in model space (local points plus the element's origin).
pub fn read_absolute(e: &Element) -> Result<Ann, String> {
    let mut a = read(e)?;
    a.translate(e.origin);
    Ok(a)
}

/// What an element draws, in model space (its local geometry moved to its origin).
pub fn geometry_absolute(e: &Element) -> Result<AnnGeometry, String> {
    let mut g = read(e)?.geometry()?;
    g.translate(e.origin);
    Ok(g)
}

/// Stores `ann` (with points in the element's local frame) and normalises the element.
pub fn write(e: &mut Element, ann: Ann) -> Result<(), String> {
    e.data = serde_json::to_value(&ann).map_err(|err| err.to_string())?;
    normalize(e)
}

/// Recomputes `origin`, `width` and `height` from what the annotation draws and re-bases its
/// points so the drawing's lower-left corner is on the origin. Also refreshes the stored
/// measured values of a dimension. An annotation that cannot be drawn keeps its bounds and returns
/// the reason.
pub fn normalize(e: &mut Element) -> Result<(), String> {
    let mut ann = read(e)?;
    let (g, measured) = match &ann {
        Ann::Dim(d) => {
            let r = d.compute()?;
            (r.geometry, Some(r.measured))
        }
        other => (other.geometry()?, None),
    };
    if let (Ann::Dim(d), Some(m)) = (&mut ann, measured) {
        d.measured = m;
    }
    let (lo, hi) = g.bounds().unwrap_or((pt(0.0, 0.0), pt(0.0, 0.0)));
    ann.translate(mul(lo, -1.0));
    e.origin = add(e.origin, lo);
    e.width = hi.x - lo.x;
    e.height = hi.y - lo.y;
    e.data = serde_json::to_value(&ann).map_err(|err| err.to_string())?;
    Ok(())
}

/// Adds an annotation whose points are in model space. Returns its id.
pub fn add_annotation(doc: &mut Document, mut ann: Ann, layer: &str) -> Result<ElementId, String> {
    // Probe first so an undrawable annotation is refused before it is stored.
    ann.geometry()?;
    let (prefix, category) = match &ann {
        Ann::Text(_) => ("TEXT-001", TEXT_CATEGORY),
        Ann::Dim(_) => ("DIM-001", DIM_CATEGORY),
        Ann::Leader(_) => ("LDR-001", LEADER_CATEGORY),
    };
    let name = doc.unique_name(prefix);
    // Start from an origin at the first definition point so the stored points are small numbers
    // (site coordinates are large; local ones keep precision).
    let base = ann.grips().first().copied().unwrap_or(pt(0.0, 0.0));
    ann.translate(mul(base, -1.0));
    let id = doc.add(category, &name, base, 0.0, 0.0, layer, serde_json::to_value(&ann).map_err(|e| e.to_string())?);
    let e = doc.find_mut(id).expect("just added");
    normalize(e)?;
    Ok(id)
}

pub fn add_text(doc: &mut Document, t: TextData, layer: &str) -> Result<ElementId, String> {
    add_annotation(doc, Ann::Text(t), layer)
}

pub fn add_dimension(doc: &mut Document, d: DimData, layer: &str) -> Result<ElementId, String> {
    let id = add_annotation(doc, Ann::Dim(d), layer)?;
    refresh(doc);
    Ok(id)
}

pub fn add_leader(doc: &mut Document, l: LeaderData, layer: &str) -> Result<ElementId, String> {
    let id = add_annotation(doc, Ann::Leader(l), layer)?;
    refresh(doc);
    Ok(id)
}

/// Replaces the annotation of element `id` with `ann` whose points are in model space (what an
/// editing command works in), keeping the element's id, name, layer and properties.
pub fn replace_absolute(doc: &mut Document, id: ElementId, mut ann: Ann) -> Result<(), String> {
    let e = doc.find_mut(id).ok_or("That annotation no longer exists.")?;
    if ann.category() != e.category {
        return Err("The kind of annotation cannot change.".into());
    }
    ann.translate(mul(e.origin, -1.0));
    write(e, ann)?;
    refresh(doc);
    Ok(())
}

/// What [`refresh`] did.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct RefreshReport {
    pub updated: usize,
    /// Things that could not be resolved, in words.
    pub problems: Vec<String>,
}

/// Looks up what annotations take from the rest of the document: the mark of the element a
/// leader refers to, and the station and offset of a station-offset dimension on its alignment.
/// Call it after anything that can change them (an element renamed or erased, an alignment
/// edited or a dimension point moved). Idempotent.
pub fn refresh(doc: &mut Document) -> RefreshReport {
    let mut report = RefreshReport::default();
    let marks: BTreeMap<u64, String> = doc.elements().iter().map(|e| (e.id.0, e.name.clone())).collect();
    let alignments: BTreeMap<String, Alignment> = doc
        .elements()
        .iter()
        .filter(|e| e.category == afrobim_alignment::CATEGORY)
        .filter_map(|e| serde_json::from_value::<Alignment>(e.data.clone()).ok().map(|a| (e.name.clone(), a)))
        .collect();
    let ids: Vec<ElementId> = doc.elements().iter().filter(|e| matches!(e.category.as_str(), DIM_CATEGORY | LEADER_CATEGORY)).map(|e| e.id).collect();
    for id in ids {
        let Some(e) = doc.find_mut(id) else { continue };
        let Ok(mut ann) = read(e) else { continue };
        let before = ann.clone();
        match &mut ann {
            Ann::Leader(l) => {
                if let LeaderText::Mark { element, cached } = &mut l.text {
                    match marks.get(element) {
                        Some(name) => *cached = name.clone(),
                        None => {
                            *cached = "?".into();
                            report.problems.push(format!("{} refers to element {element}, which no longer exists.", e.name));
                        }
                    }
                }
            }
            Ann::Dim(d) => {
                if let DimKind::StationOffset { point, alignment, .. } = &d.kind {
                    let abs = add(*point, e.origin);
                    d.cache = match alignments.get(alignment) {
                        Some(a) => station_offset_cache(a, abs),
                        None => crate::dim::DimCache { problem: Some(format!("there is no alignment called '{alignment}'")), ..Default::default() },
                    };
                    if let Some(p) = &d.cache.problem {
                        report.problems.push(format!("{}: {p}.", e.name));
                    }
                }
            }
            Ann::Text(_) => {}
        }
        if ann != before {
            let _ = write(e, ann);
            report.updated += 1;
        }
    }
    report
}

/// The kernel module for one annotation category.
#[derive(Debug, Clone, Copy)]
pub struct AnnotationModule {
    category: &'static str,
    name: &'static str,
}

impl AnnotationModule {
    pub const TEXT: AnnotationModule = AnnotationModule { category: TEXT_CATEGORY, name: "Text" };
    pub const DIMENSION: AnnotationModule = AnnotationModule { category: DIM_CATEGORY, name: "Dimensions" };
    pub const LEADER: AnnotationModule = AnnotationModule { category: LEADER_CATEGORY, name: "Leaders" };
}

impl Module for AnnotationModule {
    fn category(&self) -> &str {
        self.category
    }

    fn display_name(&self) -> &str {
        self.name
    }

    /// The drawing is the annotation's strokes, arrow heads and single-stroke lettering (a preview
    /// of the text, because the draw crate has no text primitive); use [`crate::output`] for the
    /// real text in DXF and on sheets. `derived` carries `kind`, `text` and `measured`.
    fn generate(&self, element: &Element) -> Result<Generated, String> {
        let ann = read(element)?;
        let mut derived = BTreeMap::new();
        let mut notes = Vec::new();
        let geometry = match &ann {
            Ann::Text(t) => {
                derived.insert("kind".into(), "text".into());
                derived.insert("text".into(), t.text.clone());
                t.geometry()
            }
            Ann::Dim(d) => {
                let r = d.compute()?;
                derived.insert("kind".into(), dim_kind_name(&d.kind).into());
                derived.insert("text".into(), r.texts.join(" | "));
                derived.insert("measured".into(), r.measured.iter().map(|m| format!("{m:.6}")).collect::<Vec<_>>().join(" | "));
                r.geometry
            }
            Ann::Leader(l) => {
                derived.insert("kind".into(), "leader".into());
                derived.insert("text".into(), l.display_text());
                l.geometry()?
            }
        };
        for n in &geometry.notes {
            notes.push(Note { severity: Severity::Warning, code: "annotation.layout".into(), message: n.clone() });
        }
        Ok(Generated { drawing: geometry.to_drawing(true), notes, derived, solids: Vec::new() })
    }
}

pub fn dim_kind_name(k: &DimKind) -> &'static str {
    match k {
        DimKind::Linear { .. } => "linear",
        DimKind::Chain { .. } => "chain",
        DimKind::Angular { .. } | DimKind::AngularLines { .. } => "angular",
        DimKind::Radius { .. } => "radius",
        DimKind::Diameter { .. } => "diameter",
        DimKind::ArcLength { .. } => "arc length",
        DimKind::StationOffset { .. } => "station and offset",
        DimKind::Level { .. } => "level",
    }
}

/// Registers the text, dimension, leader and settings modules.
pub fn register(modules: &mut ModuleRegistry) {
    modules.register(Box::new(AnnotationModule::TEXT));
    modules.register(Box::new(AnnotationModule::DIMENSION));
    modules.register(Box::new(AnnotationModule::LEADER));
    modules.register(Box::new(SettingsModule));
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::dim::LinearOrient;
    use crate::styles::{DimStyle, TextStyle};
    use afrobim_kernel::Pt;

    fn dim_style() -> DimStyle {
        DimStyle { text_height_mm: 10.0, arrow_size_mm: 10.0, text_gap_mm: 2.0, ..DimStyle::default() }
    }

    #[test]
    fn a_dimension_element_draws_selects_moves_copies_and_saves() {
        let mut doc = Document::default();
        let mut reg = ModuleRegistry::default();
        register(&mut reg);
        let d = DimData::new(
            DimKind::Linear { p1: pt(1_000_000.0, 2_000_000.0), p2: pt(1_005_000.0, 2_000_000.0), line_pt: pt(1_000_000.0, 2_000_800.0), orient: LinearOrient::Horizontal },
            &dim_style(),
        );
        let id = add_dimension(&mut doc, d, "DIMS").unwrap();
        let e = doc.find(id).unwrap().clone();
        // The bounds are the lower-left of everything drawn (the measured points: x from 1,000,000).
        assert_eq!(e.origin.x, 1_000_000.0);
        assert_eq!(e.origin.y, 2_000_001.0); // the extension lines start one gap (1 mm, the style default) above the points
        assert!((e.width - 5000.0).abs() < 1e-6);
        // Generated drawing is local: its extents start at the origin.
        let g = reg.generate(&e).unwrap();
        assert!(g.drawing.extents.min.x.abs() < 1e-6 && g.drawing.extents.min.y.abs() < 1e-6);
        assert_eq!(g.derived["kind"], "linear");
        assert_eq!(g.derived["text"], "5.000");
        // Selectable: pick inside the bounds.
        assert_eq!(doc.pick(Pt::new(1_002_500.0, 2_000_400.0), 0.0), Some(id));
        // Move: the origin moves, the definition points go with it, the measured value is unchanged.
        doc.translate(&[id], 100.0, 50.0);
        let moved = read_absolute(doc.find(id).unwrap()).unwrap();
        let Ann::Dim(m) = &moved else { panic!() };
        assert_eq!(m.points()[0], pt(1_000_100.0, 2_000_050.0));
        assert_eq!(m.compute().unwrap().measured, vec![5000.0]);
        // Copy keeps the data.
        let src = doc.find(id).unwrap().clone();
        let copy = doc.add_copy(&src, "DIM-002", Pt::new(0.0, 0.0));
        let c = read_absolute(doc.find(copy).unwrap()).unwrap();
        let Ann::Dim(c) = c else { panic!() };
        assert_eq!(c.points()[0], pt(0.0, -1.0)); // the first point sits 1 below the lower-left of the drawing
        // Save and reload.
        let back = Document::from_json(&doc.to_json()).unwrap();
        assert_eq!(back.elements(), doc.elements());
        assert!(reg.generate(back.find(id).unwrap()).is_ok());
        // Erase.
        assert!(doc.remove(id));
    }

    #[test]
    fn editing_a_grip_updates_the_measured_value_and_the_bounds() {
        let mut doc = Document::default();
        let d = DimData::new(DimKind::Linear { p1: pt(0.0, 0.0), p2: pt(1000.0, 0.0), line_pt: pt(0.0, 200.0), orient: LinearOrient::Horizontal }, &dim_style());
        let id = add_dimension(&mut doc, d, "DIMS").unwrap();
        let mut ann = read_absolute(doc.find(id).unwrap()).unwrap();
        assert!(ann.set_grip(1, pt(1800.0, 0.0)));
        assert!(!ann.set_grip(9, pt(0.0, 0.0)));
        replace_absolute(&mut doc, id, ann).unwrap();
        let e = doc.find(id).unwrap();
        let Ann::Dim(d) = read(e).unwrap() else { panic!() };
        assert_eq!(d.measured, vec![1800.0]);
        assert!((e.width - 1800.0).abs() < 1e-9);
    }

    #[test]
    fn text_element_uses_the_style_at_the_annotation_scale() {
        let mut doc = Document::default();
        let t = TextData::new("Existing kerb", pt(5000.0, 6000.0), &TextStyle::default(), 500.0);
        let id = add_text(&mut doc, t, DEFAULT_LAYER).unwrap();
        let e = doc.find(id).unwrap();
        // 2.5 mm paper at 1:500 is 1250 mm capitals.
        assert!((e.height - 1250.0).abs() < 1e-9);
        assert_eq!(e.origin, pt(5000.0, 6000.0));
        let g = AnnotationModule::TEXT.generate(e).unwrap();
        assert_eq!(g.derived["text"], "Existing kerb");
        assert!(!g.drawing.primitives.is_empty()); // the stroke preview
    }

    #[test]
    fn leader_follows_the_mark_of_its_element() {
        let mut doc = Document::default();
        let sign = doc.add("sign", "ADS-001", Pt::new(0.0, 0.0), 100.0, 100.0, "Signs", serde_json::Value::Null);
        let l = LeaderData::new(vec![pt(50.0, 50.0), pt(500.0, 400.0)], LeaderText::Mark { element: sign.0, cached: String::new() }, &dim_style());
        let id = add_leader(&mut doc, l, "ANNOTATION").unwrap();
        let text = |doc: &Document| match read(doc.find(id).unwrap()).unwrap() {
            Ann::Leader(l) => l.display_text(),
            _ => unreachable!(),
        };
        assert_eq!(text(&doc), "ADS-001");
        doc.find_mut(sign).unwrap().name = "ADS-002".into();
        let r = refresh(&mut doc);
        assert_eq!(r.updated, 1);
        assert_eq!(text(&doc), "ADS-002");
        assert_eq!(refresh(&mut doc).updated, 0); // idempotent
        doc.remove(sign);
        let r = refresh(&mut doc);
        assert_eq!(text(&doc), "?");
        assert_eq!(r.problems.len(), 1);
    }

    #[test]
    fn station_offset_dimension_reads_the_alignment_in_the_document() {
        let mut doc = Document::default();
        let a = afrobim_alignment::AlignmentBuilder::new("A1", 0.0, 0.0, 0.0, 0.0).line(2000.0).build().unwrap();
        doc.add(afrobim_alignment::CATEGORY, "A1", Pt::new(0.0, 0.0), 1.0, 1.0, "Alignments", serde_json::to_value(&a).unwrap());
        let d = DimData::new(DimKind::StationOffset { point: pt(1_250_500.0, -4500.0), alignment: "A1".into(), text_pt: pt(1_251_000.0, -2000.0) }, &dim_style());
        let id = add_dimension(&mut doc, d, "ANNOTATION").unwrap();
        let e = doc.find(id).unwrap();
        let g = AnnotationModule::DIMENSION.generate(e).unwrap();
        assert_eq!(g.derived["text"], "STA 1+250.500 | OFF 4.500 R");
        // Edit the alignment name away: the dimension reports the problem instead of a stale value.
        let ids: Vec<_> = doc.elements().iter().filter(|e| e.category == afrobim_alignment::CATEGORY).map(|e| e.id).collect();
        doc.remove(ids[0]);
        let r = refresh(&mut doc);
        assert!(r.problems[0].contains("no alignment called 'A1'"));
        let g = AnnotationModule::DIMENSION.generate(doc.find(id).unwrap()).unwrap();
        assert!(g.derived["text"].contains('?'));
    }

    #[test]
    fn an_undrawable_dimension_is_refused_and_a_wrong_category_is_reported() {
        let mut doc = Document::default();
        let bad = DimData::new(DimKind::Linear { p1: pt(0.0, 0.0), p2: pt(0.0, 500.0), line_pt: pt(0.0, 0.0), orient: LinearOrient::Horizontal }, &dim_style());
        assert!(add_dimension(&mut doc, bad, "X").is_err());
        assert!(doc.elements().is_empty());
        let t = TextData::new("x", pt(0.0, 0.0), &TextStyle::default(), 1.0);
        let id = add_text(&mut doc, t, "X").unwrap();
        doc.find_mut(id).unwrap().category = DIM_CATEGORY.into();
        assert!(read(doc.find(id).unwrap()).unwrap_err().contains("different kind"));
    }
}
