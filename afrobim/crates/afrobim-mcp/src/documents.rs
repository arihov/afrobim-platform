// SPDX-License-Identifier: LicenseRef-AfroBIM-Proprietary
// Copyright (c) 2026 AfroBIM. All rights reserved.
//! The document tools: build and export an `.afrobim` model the desktop app can open.
//!
//! These have no C# counterpart. They are stateless: every call loads the file it is given,
//! works on it and (for the tools that change it) saves it again, so the file on disk is always
//! the whole truth and the desktop app, a script and the assistant can take turns on it.
//!
//! A sign is stored exactly as the desktop app stores one: an element of category `sign` on
//! layer `Signs`, its mark as the element name, its lower-left corner in model millimetres, its
//! finished size, and its `SignDefinition` as the element data.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use afrobim_draw::Pt2;
use afrobim_kernel::{Document, Element, ElementId};
use afrobim_signs::{Severity, SignDefinition, SignEngine, SignLayout};
use serde::Serialize;
use serde_json::{Map, Value};

use crate::engine::{self, round1};
use crate::paths::Access;
use crate::tools::{
    blank_str, check_speed, count, family_token, status_of, DiagnosticInfo, ToolError, Tools,
};
use crate::{paths, schedule};

/// The element category and layer the desktop app's traffic-signs module uses
/// (`afrobim-app/src/signs_glue.rs`).
pub const SIGN_CATEGORY: &str = "sign";
pub const SIGN_LAYER: &str = "Signs";
const EXTENSION: &str = afrobim_kernel::FILE_EXTENSION;

// ─── result shapes ───────────────────────────────────────────────────────────

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct DocumentNewResult {
    pub path: String,
    pub standard: String,
    pub elements: usize,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct AddSignResult {
    pub path: String,
    pub id: u64,
    pub mark: String,
    pub kind: String,
    pub catalog_key: Option<String>,
    pub x_mm: f64,
    pub y_mm: f64,
    pub width_mm: f64,
    pub height_mm: f64,
    pub sw_mm: f64,
    pub x_height_mm: f64,
    pub has_errors: bool,
    pub diagnostics: Vec<DiagnosticInfo>,
    /// Elements in the document after the sign was added.
    pub elements: usize,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct ElementInfo {
    pub id: u64,
    pub mark: String,
    pub category: String,
    pub layer: String,
    pub x_mm: f64,
    pub y_mm: f64,
    pub width_mm: f64,
    pub height_mm: f64,
    pub properties: BTreeMap<String, String>,
    /// The module-owned definition: for a sign, its `SignDefinition`.
    pub definition: Value,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct ExtentsInfo {
    pub min_x_mm: f64,
    pub min_y_mm: f64,
    pub max_x_mm: f64,
    pub max_y_mm: f64,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct DocumentListResult {
    pub path: String,
    pub standard: String,
    pub count: usize,
    pub extents: Option<ExtentsInfo>,
    pub layers: Vec<String>,
    pub elements: Vec<ElementInfo>,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct RemoveElementResult {
    pub path: String,
    pub id: u64,
    pub mark: String,
    /// Elements left in the document.
    pub elements: usize,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct ElementCheck {
    pub id: u64,
    pub mark: String,
    /// `ok`, `warnings` or `errors`.
    pub status: &'static str,
    pub width_mm: f64,
    pub height_mm: f64,
    pub errors: Vec<DiagnosticInfo>,
    pub warnings: Vec<DiagnosticInfo>,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct DocumentSaveResult {
    pub path: String,
    pub saved_to: String,
    pub pack: String,
    pub elements: usize,
    pub signs_checked: usize,
    pub error_count: usize,
    pub warning_count: usize,
    pub results: Vec<ElementCheck>,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct ExportedElement {
    pub id: u64,
    pub mark: String,
    pub x_mm: f64,
    pub y_mm: f64,
    pub width_mm: f64,
    pub height_mm: f64,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct SkippedElement {
    pub id: u64,
    pub mark: String,
    pub reason: String,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct DocumentExportResult {
    pub path: String,
    pub out_path: String,
    pub pack: String,
    pub written: Vec<ExportedElement>,
    pub skipped: Vec<SkippedElement>,
    pub error_count: usize,
    pub warning_count: usize,
}

/// The arguments of `document_add_sign`.
#[derive(Debug, Clone, Default)]
pub struct AddSign<'a> {
    pub path: &'a str,
    pub pack: Option<&'a str>,
    pub key: Option<&'a str>,
    pub schedule_row: Option<&'a str>,
    pub definition: Option<&'a Value>,
    pub speed_kph: Option<i64>,
    pub mark: Option<&'a str>,
    pub x_mm: Option<f64>,
    pub y_mm: Option<f64>,
    pub properties: Option<&'a Map<String, Value>>,
}

// ─── helpers ─────────────────────────────────────────────────────────────────

fn shown(path: &Path) -> String {
    path.to_string_lossy().into_owned()
}

fn load(path: &Path) -> Result<Document, ToolError> {
    if !path.is_file() {
        return Err(ToolError::new(format!(
            "document not found: {} (create it with document_new)",
            path.display()
        )));
    }
    Document::load(path)
        .map_err(|e| ToolError::new(format!("could not open {}: {e}", path.display())))
}

fn save(doc: &Document, path: &Path) -> Result<(), ToolError> {
    paths::prepare_parent(path)?;
    doc.save(path)
        .map_err(|e| ToolError::new(format!("could not save {}: {e}", path.display())))
}

/// The sign an element holds, under the element's mark (as the desktop app reads it).
fn sign_of(element: &Element) -> Result<SignDefinition, String> {
    let mut sign: SignDefinition = serde_json::from_value(element.data.clone())
        .map_err(|e| format!("The sign definition is not valid: {e}"))?;
    sign.code = element.name.clone();
    Ok(sign)
}

fn is_sign(element: &Element) -> bool {
    element.category == SIGN_CATEGORY
}

/// A definition argument read as a sign. `code` may be left out: the mark replaces it anyway.
fn definition_from(value: &Value) -> Result<SignDefinition, ToolError> {
    let Value::Object(map) = value else {
        return Err(ToolError::new(
            "'definition' must be a JSON object in the SignDefinition shape",
        ));
    };
    let mut map = map.clone();
    map.entry("code")
        .or_insert_with(|| Value::String(String::new()));
    serde_json::from_value(Value::Object(map))
        .map_err(|e| ToolError::new(format!("'definition' is not a valid sign definition: {e}")))
}

fn properties_from(map: &Map<String, Value>) -> Result<BTreeMap<String, String>, ToolError> {
    map.iter()
        .map(|(k, v)| match v {
            Value::String(s) => Ok((k.clone(), s.clone())),
            Value::Number(_) | Value::Bool(_) => Ok((k.clone(), v.to_string())),
            _ => Err(ToolError::new(format!(
                "property '{k}' must be a string, a number or true/false"
            ))),
        })
        .collect()
}

impl Tools {
    fn document_path(&self, path: &str, access: Access) -> Result<PathBuf, ToolError> {
        paths::resolve_file(path, &self.engine.settings.guard, access, EXTENSION, "path")
    }

    /// The sign engine for a document: the pack named in the call, else the document's own
    /// standard, else the default pack.
    fn engine_for(
        &mut self,
        pack: Option<&str>,
        doc: &Document,
    ) -> Result<std::rc::Rc<SignEngine>, ToolError> {
        let id = blank_str(pack).or_else(|| blank_str(Some(doc.standard.as_str())));
        self.engine.sign_engine(id)
    }

    pub fn document_new(
        &mut self,
        path: &str,
        pack: Option<&str>,
        overwrite: bool,
    ) -> Result<DocumentNewResult, ToolError> {
        let file = self.document_path(path, Access::Write)?;
        if file.exists() && !overwrite {
            return Err(ToolError::new(format!(
                "{} already exists; pass overwrite: true to replace it with an empty document",
                file.display()
            )));
        }
        let standard = self.engine.load_pack(pack)?.pack.id.clone();
        let mut doc = Document::default();
        doc.standard = standard.clone();
        save(&doc, &file)?;
        Ok(DocumentNewResult {
            path: shown(&file),
            standard,
            elements: 0,
        })
    }

    pub fn document_add_sign(&mut self, a: &AddSign) -> Result<AddSignResult, ToolError> {
        let (key, row) = (blank_str(a.key), blank_str(a.schedule_row));
        if [key.is_some(), row.is_some(), a.definition.is_some()]
            .iter()
            .filter(|given| **given)
            .count()
            != 1
        {
            return Err(ToolError::new(
                "give exactly one of 'key', 'schedule_row' or 'definition'",
            ));
        }
        let speed = check_speed(a.speed_kph)?;
        let origin = match (a.x_mm, a.y_mm) {
            (Some(x), Some(y)) if x.is_finite() && y.is_finite() => Some(Pt2::new(x, y)),
            (None, None) => None,
            _ => return Err(ToolError::new("give both x_mm and y_mm (finite millimetres), or neither to place the sign automatically")),
        };
        let properties = a
            .properties
            .map(properties_from)
            .transpose()?
            .unwrap_or_default();

        let file = self.document_path(a.path, Access::Write)?;
        let mut doc = load(&file)?;
        let engine = self.engine_for(a.pack, &doc)?;
        let mut sign = match (key, row, a.definition) {
            (Some(key), _, _) => engine::from_catalog(&engine, key, speed)?,
            (_, Some(row), _) => schedule::parse_schedule_row(row)?,
            (_, _, Some(definition)) => definition_from(definition)?,
            _ => unreachable!("exactly one source is given"),
        };
        let wanted = blank_str(a.mark)
            .or_else(|| blank_str(Some(sign.code.as_str())))
            .unwrap_or("SIGN-001")
            .trim()
            .to_string();
        let mark = doc.unique_name(&wanted);
        sign.code = mark.clone();

        let layout = engine::run(&engine, &sign)?;
        let (w, h) = engine::size_mm(&layout);
        let at = origin.unwrap_or_else(|| doc.next_free_origin(w, h));
        let data = serde_json::to_value(&sign)
            .map_err(|e| ToolError::new(format!("could not store the sign definition: {e}")))?;
        let id = doc.add(SIGN_CATEGORY, &mark, at, w, h, SIGN_LAYER, data);
        if let Some(element) = doc.find_mut(id) {
            element.properties = properties;
        }
        if doc.standard.trim().is_empty() {
            doc.standard = engine.pack_id().to_string();
        }
        save(&doc, &file)?;

        Ok(AddSignResult {
            path: shown(&file),
            id: id.0,
            mark,
            kind: family_token(sign.kind).to_string(),
            catalog_key: sign.catalog_key.clone(),
            x_mm: at.x,
            y_mm: at.y,
            width_mm: round1(w),
            height_mm: round1(h),
            sw_mm: layout.sw_mm,
            x_height_mm: layout.x_height_mm,
            has_errors: layout.has_errors(),
            diagnostics: DiagnosticInfo::all(&layout.diagnostics),
            elements: doc.elements().len(),
        })
    }

    pub fn document_list(&mut self, path: &str) -> Result<DocumentListResult, ToolError> {
        let file = self.document_path(path, Access::Read)?;
        let doc = load(&file)?;
        Ok(DocumentListResult {
            path: shown(&file),
            standard: doc.standard.clone(),
            count: doc.elements().len(),
            extents: doc.extents().map(|b| ExtentsInfo {
                min_x_mm: b.min.x,
                min_y_mm: b.min.y,
                max_x_mm: b.max.x,
                max_y_mm: b.max.y,
            }),
            layers: doc.layers().iter().map(|l| l.name.clone()).collect(),
            elements: doc
                .elements()
                .iter()
                .map(|e| ElementInfo {
                    id: e.id.0,
                    mark: e.name.clone(),
                    category: e.category.clone(),
                    layer: e.layer.clone(),
                    x_mm: e.origin.x,
                    y_mm: e.origin.y,
                    width_mm: round1(e.width),
                    height_mm: round1(e.height),
                    properties: e.properties.clone(),
                    definition: e.data.clone(),
                })
                .collect(),
        })
    }

    pub fn document_remove_element(
        &mut self,
        path: &str,
        id: Option<u64>,
        mark: Option<&str>,
    ) -> Result<RemoveElementResult, ToolError> {
        let mark = blank_str(mark).map(str::trim);
        if id.is_some() == mark.is_some() {
            return Err(ToolError::new("give exactly one of 'id' or 'mark'"));
        }
        let file = self.document_path(path, Access::Write)?;
        let mut doc = load(&file)?;
        let matches: Vec<(ElementId, String)> = doc
            .elements()
            .iter()
            .filter(|e| {
                id.is_some_and(|id| e.id.0 == id)
                    || mark.is_some_and(|m| e.name.eq_ignore_ascii_case(m))
            })
            .map(|e| (e.id, e.name.clone()))
            .collect();
        let (found, name) = match matches.as_slice() {
            [one] => one.clone(),
            [] => {
                let wanted = id
                    .map(|id| format!("id {id}"))
                    .or_else(|| mark.map(|m| format!("mark '{m}'")))
                    .unwrap_or_default();
                return Err(ToolError::new(format!(
                    "no element with {wanted} in {}",
                    file.display()
                )));
            }
            many => {
                let ids: Vec<String> = many.iter().map(|(id, _)| id.0.to_string()).collect();
                return Err(ToolError::new(format!(
                    "{} elements carry that mark (ids {}); remove one by 'id'",
                    many.len(),
                    ids.join(", ")
                )));
            }
        };
        doc.remove(found);
        save(&doc, &file)?;
        Ok(RemoveElementResult {
            path: shown(&file),
            id: found.0,
            mark: name,
            elements: doc.elements().len(),
        })
    }

    pub fn document_save(
        &mut self,
        path: &str,
        save_as: Option<&str>,
        pack: Option<&str>,
    ) -> Result<DocumentSaveResult, ToolError> {
        let save_as = blank_str(save_as);
        // Saving in place writes the document; a copy only reads it.
        let file = self.document_path(
            path,
            if save_as.is_some() {
                Access::Read
            } else {
                Access::Write
            },
        )?;
        let target = match save_as {
            Some(p) => paths::resolve_file(
                p,
                &self.engine.settings.guard,
                Access::Write,
                EXTENSION,
                "save_as",
            )?,
            None => file.clone(),
        };
        let mut doc = load(&file)?;
        let engine = self.engine_for(pack, &doc)?;

        let mut results = Vec::new();
        let ids: Vec<ElementId> = doc
            .elements()
            .iter()
            .filter(|e| is_sign(e))
            .map(|e| e.id)
            .collect();
        for id in ids {
            let Some(element) = doc.find_mut(id) else {
                continue;
            };
            let laid_out =
                sign_of(element).and_then(|sign| engine.layout(&sign).map_err(|e| e.to_string()));
            let (errors, warnings) = match &laid_out {
                Ok(layout) => {
                    // Bring the stored size up to date, as the desktop app does when it opens the file.
                    let (w, h) = engine::size_mm(layout);
                    if w > 0.0 && h > 0.0 {
                        (element.width, element.height) = (w, h);
                    }
                    (
                        DiagnosticInfo::of(&layout.diagnostics, Severity::Error),
                        DiagnosticInfo::of(&layout.diagnostics, Severity::Warning),
                    )
                }
                Err(message) => (
                    vec![DiagnosticInfo::error("LAYOUT_FAILED", message.clone())],
                    Vec::new(),
                ),
            };
            results.push(ElementCheck {
                id: id.0,
                mark: element.name.clone(),
                status: status_of(errors.len(), warnings.len()),
                width_mm: round1(element.width),
                height_mm: round1(element.height),
                errors,
                warnings,
            });
        }
        if doc.standard.trim().is_empty() {
            doc.standard = engine.pack_id().to_string();
        }
        save(&doc, &target)?;
        Ok(DocumentSaveResult {
            path: shown(&file),
            saved_to: shown(&target),
            pack: engine.pack_id().to_string(),
            elements: doc.elements().len(),
            signs_checked: results.len(),
            error_count: results.iter().map(|r| r.errors.len()).sum(),
            warning_count: results.iter().map(|r| r.warnings.len()).sum(),
            results,
        })
    }

    pub fn document_export_dxf(
        &mut self,
        path: &str,
        out_path: &str,
        marks: Option<&[String]>,
        pack: Option<&str>,
    ) -> Result<DocumentExportResult, ToolError> {
        let file = self.document_path(path, Access::Read)?;
        let out = paths::resolve_file(
            out_path,
            &self.engine.settings.guard,
            Access::Write,
            "dxf",
            "out_path",
        )?;
        let doc = load(&file)?;
        let marks: Option<Vec<&str>> = marks
            .filter(|m| !m.is_empty())
            .map(|m| m.iter().map(|s| s.trim()).collect());
        if let Some(missing) = marks.iter().flatten().find(|m| {
            !doc.elements()
                .iter()
                .any(|e| e.name.eq_ignore_ascii_case(m))
        }) {
            return Err(ToolError::new(format!(
                "no element with mark '{missing}' in {}",
                file.display()
            )));
        }
        let engine = self.engine_for(pack, &doc)?;

        let mut dxf = afrobim_dxf::DxfDocument::new();
        let (mut written, mut skipped) = (Vec::new(), Vec::new());
        let (mut errors, mut warnings) = (0, 0);
        for e in doc.elements() {
            if marks
                .as_ref()
                .is_some_and(|m| !m.iter().any(|m| e.name.eq_ignore_ascii_case(m)))
            {
                continue;
            }
            let laid_out: Result<SignLayout, String> = if is_sign(e) {
                sign_of(e).and_then(|sign| engine.layout(&sign).map_err(|err| err.to_string()))
            } else {
                Err(format!(
                    "category '{}' has no geometry in this server",
                    e.category
                ))
            };
            match laid_out {
                Ok(layout) => {
                    // The model is in millimetres: each sign at its own place, as File → Export DXF.
                    let drawing = layout.drawing.scaled_to_origin(layout.sw_mm);
                    dxf.add(&drawing, 1.0, e.origin);
                    errors += count(&layout.diagnostics, Severity::Error);
                    warnings += count(&layout.diagnostics, Severity::Warning);
                    written.push(ExportedElement {
                        id: e.id.0,
                        mark: e.name.clone(),
                        x_mm: e.origin.x,
                        y_mm: e.origin.y,
                        width_mm: round1(drawing.extents.width()),
                        height_mm: round1(drawing.extents.height()),
                    });
                }
                Err(reason) => skipped.push(SkippedElement {
                    id: e.id.0,
                    mark: e.name.clone(),
                    reason,
                }),
            }
        }
        if written.is_empty() {
            let why = skipped
                .first()
                .map(|s| format!(" ({}: {})", s.mark, s.reason))
                .unwrap_or_default();
            return Err(ToolError::new(format!(
                "nothing to export from {}{why}",
                file.display()
            )));
        }
        paths::prepare_parent(&out)?;
        std::fs::write(&out, dxf.finish())
            .map_err(|e| ToolError::new(format!("could not write {}: {e}", out.display())))?;
        Ok(DocumentExportResult {
            path: shown(&file),
            out_path: shown(&out),
            pack: engine.pack_id().to_string(),
            error_count: errors + skipped.len(),
            warning_count: warnings,
            written,
            skipped,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testing::{self, TempDir};
    use serde_json::json;

    fn tools() -> Tools {
        Tools::new(testing::settings())
    }

    fn new_doc(t: &mut Tools, tmp: &TempDir, name: &str) -> String {
        let path = tmp.path().join(name).to_string_lossy().into_owned();
        t.document_new(&path, None, false).unwrap();
        path
    }

    fn add_key<'a>(path: &'a str, key: &'a str) -> AddSign<'a> {
        AddSign {
            path,
            key: Some(key),
            ..AddSign::default()
        }
    }

    #[test]
    fn document_new_writes_an_empty_document_the_kernel_reads() {
        let tmp = TempDir::new("doc-new");
        let mut t = tools();
        let path = tmp.path().join("sub").join("model.afrobim"); // native separators on every system
        let r = t.document_new(path.to_str().unwrap(), None, false).unwrap();
        assert_eq!((r.standard.as_str(), r.elements), ("UK-TSM-2019", 0));
        assert_eq!(r.path, path.to_string_lossy());
        let doc = Document::load(&path).unwrap();
        assert!(doc.elements().is_empty());
        assert_eq!(doc.standard, "UK-TSM-2019");
    }

    #[test]
    fn every_tool_write_goes_through_a_temporary_file_and_leaves_only_the_document() {
        let tmp = TempDir::new("doc-atomic");
        let mut t = tools();
        let path = new_doc(&mut t, &tmp, "m.afrobim");
        t.document_add_sign(&add_key(&path, "UK04.1")).unwrap();
        t.document_save(&path, None, None).unwrap();
        t.document_new(&path, None, true).unwrap();
        let mut names: Vec<String> = std::fs::read_dir(tmp.path())
            .unwrap()
            .map(|e| e.unwrap().file_name().to_string_lossy().into_owned())
            .collect();
        names.sort();
        assert_eq!(names, vec!["m.afrobim"], "no temporary or recovery file is left");
        assert!(Document::load(Path::new(&path)).unwrap().elements().is_empty());
    }

    #[test]
    fn a_save_that_cannot_be_written_leaves_the_existing_file_whole() {
        let tmp = TempDir::new("doc-atomic-fail");
        let mut t = tools();
        let path = new_doc(&mut t, &tmp, "m.afrobim");
        t.document_add_sign(&add_key(&path, "UK04.1")).unwrap();
        // Saving "as" a name below the document (a file, not a folder) fails; the document stays.
        let below = format!("{path}/inner.afrobim");
        assert!(t.document_save(&path, Some(&below), None).is_err());
        assert_eq!(t.document_list(&path).unwrap().count, 1);
    }

    #[test]
    fn document_new_refuses_to_replace_a_file_unless_told_to() {
        let tmp = TempDir::new("doc-overwrite");
        let mut t = tools();
        let path = new_doc(&mut t, &tmp, "m.afrobim");
        t.document_add_sign(&add_key(&path, "UK04.1")).unwrap();
        assert!(t
            .document_new(&path, None, false)
            .unwrap_err()
            .0
            .contains("already exists"));
        assert_eq!(t.document_list(&path).unwrap().count, 1);
        t.document_new(&path, None, true).unwrap();
        assert_eq!(t.document_list(&path).unwrap().count, 0);
    }

    #[test]
    fn document_paths_must_be_afrobim_files_and_packs_must_exist() {
        let tmp = TempDir::new("doc-paths");
        let mut t = tools();
        let txt = tmp.path().join("model.txt");
        assert!(t
            .document_new(txt.to_str().unwrap(), None, false)
            .unwrap_err()
            .0
            .contains(".afrobim"));
        assert!(t.document_new("", None, false).is_err());
        let path = tmp.path().join("m.afrobim");
        assert!(t
            .document_new(path.to_str().unwrap(), Some("NOPE"), false)
            .unwrap_err()
            .0
            .contains("unknown pack"));
        assert!(!path.exists());
        assert!(t
            .document_list(path.to_str().unwrap())
            .unwrap_err()
            .0
            .contains("document not found"));
        std::fs::write(&path, "not json").unwrap();
        assert!(t
            .document_list(path.to_str().unwrap())
            .unwrap_err()
            .0
            .contains("not a valid AfroBIM document"));
    }

    #[test]
    fn a_catalog_sign_is_stored_as_the_desktop_app_stores_it() {
        let tmp = TempDir::new("doc-add");
        let mut t = tools();
        let path = new_doc(&mut t, &tmp, "m.afrobim");
        let r = t.document_add_sign(&add_key(&path, "UK04.1")).unwrap();
        assert_eq!(
            (r.id, r.mark.as_str(), r.kind.as_str(), r.elements),
            (1, "UK04.1", "stack", 1)
        );
        assert_eq!((r.x_mm, r.y_mm), (0.0, 0.0));
        // The same figures layout_sign (and the C# server) reports for the key.
        assert_eq!(
            (r.width_mm, r.height_mm, r.sw_mm, r.x_height_mm),
            (2111.9, 1125.0, 31.25, 125.0)
        );
        assert!(!r.has_errors);

        let doc = Document::load(Path::new(&path)).unwrap();
        let e = &doc.elements()[0];
        assert_eq!(
            (e.category.as_str(), e.layer.as_str(), e.name.as_str()),
            (SIGN_CATEGORY, SIGN_LAYER, "UK04.1")
        );
        assert!(
            (e.width - 2111.875).abs() < 1e-6 && (e.height - 1125.0).abs() < 1e-6,
            "{} x {}",
            e.width,
            e.height
        );
        let stored: SignDefinition = serde_json::from_value(e.data.clone()).unwrap();
        assert_eq!(
            (stored.code.as_str(), stored.catalog_key.as_deref()),
            ("UK04.1", Some("UK04.1"))
        );
        assert!(doc.layers().iter().any(|l| l.name == SIGN_LAYER));
    }

    #[test]
    fn signs_are_placed_in_a_row_given_unique_marks_and_keep_their_properties() {
        let tmp = TempDir::new("doc-row");
        let mut t = tools();
        let path = new_doc(&mut t, &tmp, "m.afrobim");
        let first = t
            .document_add_sign(&AddSign {
                mark: Some("ADS-001"),
                ..add_key(&path, "UK04.1")
            })
            .unwrap();
        let props = json!({"Material": "Class RA2", "Posts": 2, "Lit": false});
        let second = t
            .document_add_sign(&AddSign {
                mark: Some("ADS-001"),
                schedule_row: Some("X,stack,64,primary,green,Lampton:Ahead:0;Slough:Left:0"),
                properties: props.as_object(),
                ..AddSign {
                    path: &path,
                    ..AddSign::default()
                }
            })
            .unwrap();
        assert_eq!(
            (first.mark.as_str(), second.mark.as_str()),
            ("ADS-001", "ADS-002")
        );
        assert_eq!(
            (second.x_mm, second.y_mm),
            (2111.875 + Document::ARRANGE_GAP, 0.0)
        );
        let placed = t
            .document_add_sign(&AddSign {
                x_mm: Some(-500.0),
                y_mm: Some(4000.0),
                ..add_key(&path, "UK06.2")
            })
            .unwrap();
        assert_eq!(
            (placed.x_mm, placed.y_mm, placed.mark.as_str()),
            (-500.0, 4000.0, "UK06.2")
        );

        let list = t.document_list(&path).unwrap();
        assert_eq!((list.count, list.standard.as_str()), (3, "UK-TSM-2019"));
        assert_eq!(
            list.elements[1].properties,
            BTreeMap::from([
                ("Lit".into(), "false".into()),
                ("Material".into(), "Class RA2".into()),
                ("Posts".into(), "2".into())
            ])
        );
        assert_eq!(list.elements[1].definition["code"], "ADS-002");
        assert_eq!(
            list.elements[1].definition["destinations"][1]["name"],
            "Slough"
        );
        let ext = list.extents.unwrap();
        assert_eq!((ext.min_x_mm, ext.max_y_mm), (-500.0, 4600.0));
        assert!(list.layers.contains(&SIGN_LAYER.to_string()));
    }

    #[test]
    fn a_sign_can_be_added_from_a_definition_object() {
        let tmp = TempDir::new("doc-def");
        let mut t = tools();
        let path = new_doc(&mut t, &tmp, "m.afrobim");
        let definition = json!({
            "kind": "StackType", "speed_kph": 60, "background": "Green",
            "destinations": [{"name": "Kasese", "direction": "Ahead"}, {"name": "Masheruka", "direction": "Right"}]
        });
        let r = t
            .document_add_sign(&AddSign {
                definition: Some(&definition),
                mark: Some("ADS-010"),
                ..AddSign {
                    path: &path,
                    ..AddSign::default()
                }
            })
            .unwrap();
        assert_eq!((r.mark.as_str(), r.kind.as_str()), ("ADS-010", "stack"));
        assert!(r.width_mm > 0.0);
        // Without a mark or a code the sign still gets a name.
        let r = t
            .document_add_sign(&AddSign {
                definition: Some(&definition),
                ..AddSign {
                    path: &path,
                    ..AddSign::default()
                }
            })
            .unwrap();
        assert_eq!(r.mark, "SIGN-001");

        let bad = json!({"kind": "NotAKind", "speed_kph": 60, "background": "Green"});
        let e = t
            .document_add_sign(&AddSign {
                definition: Some(&bad),
                ..AddSign {
                    path: &path,
                    ..AddSign::default()
                }
            })
            .unwrap_err();
        assert!(e.0.contains("not a valid sign definition"), "{e}");
        let e = t
            .document_add_sign(&AddSign {
                definition: Some(&json!("x")),
                ..AddSign {
                    path: &path,
                    ..AddSign::default()
                }
            })
            .unwrap_err();
        assert!(e.0.contains("must be a JSON object"));
        assert_eq!(t.document_list(&path).unwrap().count, 2);
    }

    #[test]
    fn a_sign_with_errors_is_added_and_its_diagnostics_returned() {
        let tmp = TempDir::new("doc-diag");
        let mut t = tools();
        let path = new_doc(&mut t, &tmp, "m.afrobim");
        let dests: Vec<String> = (1..=7).map(|i| format!("Town{i}:Ahead:{i}")).collect();
        let row = format!("ADS-7,stack,64,primary,green,{}", dests.join(";"));
        let r = t
            .document_add_sign(&AddSign {
                schedule_row: Some(&row),
                ..AddSign {
                    path: &path,
                    ..AddSign::default()
                }
            })
            .unwrap();
        assert!(r.has_errors);
        assert!(r
            .diagnostics
            .iter()
            .any(|d| d.code == "MAX_DESTINATIONS" && d.clause.is_some()));
        assert_eq!(r.elements, 1);
    }

    #[test]
    fn document_add_sign_refuses_bad_arguments_and_leaves_the_file_alone() {
        let tmp = TempDir::new("doc-bad");
        let mut t = tools();
        let path = new_doc(&mut t, &tmp, "m.afrobim");
        let before = std::fs::read_to_string(&path).unwrap();
        let base = AddSign {
            path: &path,
            ..AddSign::default()
        };
        let msg = |t: &mut Tools, a: AddSign| t.document_add_sign(&a).unwrap_err().0;

        assert!(msg(&mut t, base.clone()).contains("exactly one of"));
        assert!(msg(
            &mut t,
            AddSign {
                schedule_row: Some("A,stack,64,primary,green,X:Ahead:0"),
                ..add_key(&path, "UK04.1")
            }
        )
        .contains("exactly one of"));
        assert!(msg(&mut t, add_key(&path, "NOPE")).contains("not found"));
        assert!(msg(
            &mut t,
            AddSign {
                x_mm: Some(1.0),
                ..add_key(&path, "UK04.1")
            }
        )
        .contains("both x_mm and y_mm"));
        assert!(msg(
            &mut t,
            AddSign {
                x_mm: Some(f64::NAN),
                y_mm: Some(0.0),
                ..add_key(&path, "UK04.1")
            }
        )
        .contains("both x_mm and y_mm"));
        assert!(msg(
            &mut t,
            AddSign {
                speed_kph: Some(500),
                ..add_key(&path, "UK04.1")
            }
        )
        .contains("speed_kph"));
        assert!(msg(
            &mut t,
            AddSign {
                pack: Some("NOPE"),
                ..add_key(&path, "UK04.1")
            }
        )
        .contains("unknown pack"));
        let props = json!({"Posts": [1, 2]});
        assert!(msg(
            &mut t,
            AddSign {
                properties: props.as_object(),
                ..add_key(&path, "UK04.1")
            }
        )
        .contains("property 'Posts'"));
        let missing = tmp.path().join("absent.afrobim");
        assert!(msg(&mut t, add_key(missing.to_str().unwrap(), "UK04.1")).contains("document_new"));
        assert_eq!(std::fs::read_to_string(&path).unwrap(), before);
    }

    #[test]
    fn elements_are_removed_by_id_or_mark() {
        let tmp = TempDir::new("doc-remove");
        let mut t = tools();
        let path = new_doc(&mut t, &tmp, "m.afrobim");
        for key in ["UK04.1", "UK06.2", "UK07.1"] {
            t.document_add_sign(&add_key(&path, key)).unwrap();
        }
        let r = t
            .document_remove_element(&path, None, Some("uk06.2"))
            .unwrap();
        assert_eq!((r.id, r.mark.as_str(), r.elements), (2, "UK06.2", 2));
        let r = t.document_remove_element(&path, Some(1), None).unwrap();
        assert_eq!((r.mark.as_str(), r.elements), ("UK04.1", 1));
        assert!(t
            .document_remove_element(&path, Some(1), None)
            .unwrap_err()
            .0
            .contains("no element with id 1"));
        assert!(t
            .document_remove_element(&path, None, Some("ZZ"))
            .unwrap_err()
            .0
            .contains("no element with mark 'ZZ'"));
        assert!(t
            .document_remove_element(&path, None, None)
            .unwrap_err()
            .0
            .contains("exactly one"));
        assert!(t
            .document_remove_element(&path, Some(3), Some("UK07.1"))
            .unwrap_err()
            .0
            .contains("exactly one"));
        // Ids are not reused, so a later sign cannot be mistaken for a removed one.
        assert_eq!(
            t.document_add_sign(&add_key(&path, "UK04.1")).unwrap().id,
            4
        );
    }

    #[test]
    fn document_save_checks_every_sign_refreshes_sizes_and_can_save_a_copy() {
        let tmp = TempDir::new("doc-save");
        let mut t = tools();
        let path = new_doc(&mut t, &tmp, "m.afrobim");
        t.document_add_sign(&add_key(&path, "UK04.1")).unwrap();
        // A hand-edited file: stale size, one sign that cannot be read, one foreign element.
        let mut doc = Document::load(Path::new(&path)).unwrap();
        let id = doc.elements()[0].id;
        doc.find_mut(id).unwrap().width = 1.0;
        doc.add(
            SIGN_CATEGORY,
            "BROKEN",
            Pt2::new(0.0, -2000.0),
            10.0,
            10.0,
            SIGN_LAYER,
            json!({"kind": "Nope"}),
        );
        doc.add(
            "marking",
            "RM-1",
            Pt2::new(0.0, -4000.0),
            10.0,
            10.0,
            "0",
            json!({}),
        );
        doc.standard = String::new();
        doc.save(Path::new(&path)).unwrap();

        let copy = tmp.path().join("copy.afrobim");
        let r = t
            .document_save(&path, Some(copy.to_str().unwrap()), None)
            .unwrap();
        assert_eq!(
            (r.elements, r.signs_checked, r.error_count, r.warning_count),
            (3, 2, 1, 0)
        );
        assert_eq!(r.saved_to, copy.to_string_lossy());
        assert_eq!((r.results[0].status, r.results[0].width_mm), ("ok", 2111.9));
        assert_eq!(
            (r.results[1].status, r.results[1].errors[0].code.as_str()),
            ("errors", "LAYOUT_FAILED")
        );

        let saved = Document::load(&copy).unwrap();
        assert!((saved.elements()[0].width - 2111.875).abs() < 1e-6);
        assert_eq!(saved.standard, "UK-TSM-2019");
        assert_eq!(saved.elements().len(), 3);
        // The original is untouched by a save under another name.
        assert_eq!(
            Document::load(Path::new(&path)).unwrap().elements()[0].width,
            1.0
        );
        t.document_save(&path, None, None).unwrap();
        assert!(
            (Document::load(Path::new(&path)).unwrap().elements()[0].width - 2111.875).abs() < 1e-6
        );
        assert!(t
            .document_save(&path, Some("copy.txt"), None)
            .unwrap_err()
            .0
            .contains("save_as must name a .afrobim file"));
    }

    #[test]
    fn document_export_dxf_writes_every_sign_at_its_model_position() {
        let tmp = TempDir::new("doc-dxf");
        let mut t = tools();
        let path = new_doc(&mut t, &tmp, "m.afrobim");
        t.document_add_sign(&add_key(&path, "UK04.1")).unwrap();
        t.document_add_sign(&AddSign {
            x_mm: Some(5000.0),
            y_mm: Some(1000.0),
            ..add_key(&path, "UK06.2")
        })
        .unwrap();
        let out = tmp.path().join("dxf/signs.dxf");

        let r = t
            .document_export_dxf(&path, out.to_str().unwrap(), None, None)
            .unwrap();
        assert_eq!((r.written.len(), r.skipped.len(), r.error_count), (2, 0, 0));
        assert_eq!(
            (
                r.written[1].x_mm,
                r.written[1].width_mm,
                r.written[1].height_mm
            ),
            (5000.0, 1518.5, 600.0)
        );
        let dxf = std::fs::read_to_string(&out).unwrap();
        assert!(dxf.contains("AC1009") && dxf.ends_with("0\nEOF\n"));

        // One sign alone at the origin is the same drawing export_dxf writes for it.
        let single = tmp.path().join("single.dxf");
        let marks = ["uk04.1".to_string()];
        let one = t
            .document_export_dxf(&path, single.to_str().unwrap(), Some(&marks), None)
            .unwrap();
        assert_eq!(one.written.len(), 1);
        let per_sign = t
            .export_dxf(
                tmp.path().join("per-sign").to_str().unwrap(),
                None,
                Some(&["UK04.1".to_string()]),
                None,
            )
            .unwrap();
        let (a, b) = (
            std::fs::read_to_string(&single).unwrap(),
            std::fs::read_to_string(&per_sign.files[0].path).unwrap(),
        );
        assert_eq!(a.matches("POLYLINE").count(), b.matches("POLYLINE").count());
        assert_eq!(a.matches("VERTEX").count(), b.matches("VERTEX").count());
        // Both signs together hold the entities of each.
        assert!(dxf.matches("POLYLINE").count() > a.matches("POLYLINE").count());
    }

    #[test]
    fn document_export_dxf_reports_what_it_left_out_and_refuses_bad_arguments() {
        let tmp = TempDir::new("doc-dxf-bad");
        let mut t = tools();
        let path = new_doc(&mut t, &tmp, "m.afrobim");
        let out = tmp.path().join("o.dxf");
        let out = out.to_str().unwrap();
        assert!(t
            .document_export_dxf(&path, out, None, None)
            .unwrap_err()
            .0
            .contains("nothing to export"));
        assert!(!Path::new(out).exists());

        t.document_add_sign(&add_key(&path, "UK04.1")).unwrap();
        let mut doc = Document::load(Path::new(&path)).unwrap();
        doc.add(
            "marking",
            "RM-1",
            Pt2::new(0.0, -4000.0),
            10.0,
            10.0,
            "0",
            json!({}),
        );
        doc.save(Path::new(&path)).unwrap();
        let r = t.document_export_dxf(&path, out, None, None).unwrap();
        assert_eq!((r.written.len(), r.skipped.len(), r.error_count), (1, 1, 1));
        assert!(r.skipped[0].reason.contains("marking"));

        let marks = ["NOPE".to_string()];
        assert!(t
            .document_export_dxf(&path, out, Some(&marks), None)
            .unwrap_err()
            .0
            .contains("no element with mark 'NOPE'"));
        assert!(t
            .document_export_dxf(&path, "o.txt", None, None)
            .unwrap_err()
            .0
            .contains("out_path must name a .dxf file"));
    }
}
