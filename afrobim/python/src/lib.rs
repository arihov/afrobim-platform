// SPDX-License-Identifier: LicenseRef-AfroBIM-Proprietary
// Copyright (c) 2026 AfroBIM. All rights reserved.
//! The compiled half of the `afrobim` Python package (`afrobim._native`).
//!
//! It is a thin shell over the Rust crates: the sign engine, the DXF writer and the kernel
//! document. Structured values cross the boundary as JSON text, in the same shape the crates
//! serialise (so a sign definition in Python is the dict a `.afrobim` file stores); the pure
//! Python package in `python/afrobim` turns that into dicts, adds type hints and docstrings,
//! and is what users import.

use std::path::PathBuf;

use afrobim_draw::{Colour, Drawing, Primitive, Pt2};
use afrobim_kernel::{Document, DocumentError as KernelDocumentError, ElementId};
use afrobim_signs::{SignDefinition, SignEngine, SignKind, SignLayout};
use pyo3::create_exception;
use pyo3::exceptions::{PyKeyError, PyOSError, PyValueError};
use pyo3::prelude::*;
use serde_json::{json, Value};

mod svg;

/// How the desktop app files a sign in a document (`afrobim-app/src/signs_glue.rs`).
const CATEGORY: &str = "sign";
const LAYER: &str = "Signs";

create_exception!(afrobim, SignError, PyValueError, "The sign engine rejected a rule pack, a font or a sign definition.");
create_exception!(afrobim, UnsupportedError, SignError, "The sign needs something the engine does not lay out yet.");
create_exception!(afrobim, DocumentError, PyValueError, "The text or file is not a valid AfroBIM document.");

fn sign_err(e: afrobim_signs::SignError) -> PyErr {
    match e {
        afrobim_signs::SignError::Unsupported(_) => UnsupportedError::new_err(e.to_string()),
        _ => SignError::new_err(e.to_string()),
    }
}

fn doc_err(e: KernelDocumentError) -> PyErr {
    match e {
        KernelDocumentError::Io(io) => PyOSError::new_err(io.to_string()),
        other => DocumentError::new_err(other.to_string()),
    }
}

/// The same wording the desktop app uses for an element whose definition cannot be read.
fn parse_definition(text: &str) -> PyResult<SignDefinition> {
    serde_json::from_str(text).map_err(|e| SignError::new_err(format!("The sign definition is not valid: {e}")))
}

fn to_json<T: serde::Serialize>(value: &T) -> String {
    serde_json::to_string(value).expect("engine values always serialise")
}

fn colour_name(c: Colour) -> &'static str {
    match c {
        Colour::White => "White",
        Colour::Black => "Black",
        Colour::Green => "Green",
        Colour::Blue => "Blue",
        Colour::Brown => "Brown",
        Colour::Yellow => "Yellow",
        Colour::Red => "Red",
        Colour::Grey => "Grey",
    }
}

fn pt(p: &Pt2) -> Value {
    json!([p.x, p.y])
}

/// One primitive as a flat dict: `type` names the variant, points are `[x, y]` pairs. This is
/// the shape of the engine's golden fixtures (`afrobim-signs/tests/golden/*.json`).
fn primitive_value(prim: &Primitive) -> Value {
    let colour = |c: &Option<Colour>| c.map(colour_name);
    match prim {
        Primitive::Fill { outline, colour } => json!({
            "type": "Fill",
            "outline": outline.iter().map(pt).collect::<Vec<_>>(),
            "colour": colour_name(*colour),
        }),
        Primitive::Line { from, to, width, colour: c } => json!({
            "type": "Line",
            "from": pt(from),
            "to": pt(to),
            "width": width,
            "colour": colour(c),
        }),
        Primitive::Arc { centre, radius, start_deg, end_deg, colour: c } => json!({
            "type": "Arc",
            "centre": pt(centre),
            "radius": radius,
            "start_deg": start_deg,
            "end_deg": end_deg,
            "colour": colour(c),
        }),
        Primitive::Glyphs { contours, text, ink } => json!({
            "type": "Glyphs",
            "contours": contours.iter().map(|c| c.iter().map(pt).collect::<Vec<_>>()).collect::<Vec<_>>(),
            "text": text,
            "ink": colour(ink),
        }),
        Primitive::Symbol { origin, id, size, rotation_deg } => json!({
            "type": "Symbol",
            "origin": pt(origin),
            "id": id,
            "size": size,
            "rotation_deg": rotation_deg,
        }),
    }
}

fn bad_primitive(index: usize, what: &str) -> PyErr {
    PyValueError::new_err(format!("primitive {index}: {what}"))
}

/// The reverse of [`primitive_value`], for geometry that comes back from Python.
fn primitive_from(index: usize, v: &Value) -> PyResult<Primitive> {
    let number = |key: &str| v[key].as_f64().ok_or_else(|| bad_primitive(index, &format!("'{key}' must be a number")));
    let point = |p: &Value, key: &str| match (p[0].as_f64(), p[1].as_f64()) {
        (Some(x), Some(y)) => Ok(Pt2::new(x, y)),
        _ => Err(bad_primitive(index, &format!("'{key}' must hold [x, y] points"))),
    };
    let points = |p: &Value, key: &str| -> PyResult<Vec<Pt2>> {
        p.as_array()
            .ok_or_else(|| bad_primitive(index, &format!("'{key}' must be a list of [x, y] points")))?
            .iter()
            .map(|q| point(q, key))
            .collect()
    };
    let colour = |key: &str| -> PyResult<Option<Colour>> {
        match &v[key] {
            Value::Null => Ok(None),
            c => serde_json::from_value(c.clone()).map(Some).map_err(|_| bad_primitive(index, &format!("'{key}' is not a sign colour"))),
        }
    };
    let text = |key: &str| v[key].as_str().unwrap_or_default().to_string();
    Ok(match v["type"].as_str() {
        Some("Fill") => Primitive::Fill {
            outline: points(&v["outline"], "outline")?,
            colour: colour("colour")?.ok_or_else(|| bad_primitive(index, "a Fill needs a 'colour'"))?,
        },
        Some("Line") => Primitive::Line {
            from: point(&v["from"], "from")?,
            to: point(&v["to"], "to")?,
            width: v["width"].as_f64().unwrap_or(0.0),
            colour: colour("colour")?,
        },
        Some("Arc") => Primitive::Arc {
            centre: point(&v["centre"], "centre")?,
            radius: number("radius")?,
            start_deg: number("start_deg")?,
            end_deg: number("end_deg")?,
            colour: colour("colour")?,
        },
        Some("Glyphs") => Primitive::Glyphs {
            contours: v["contours"]
                .as_array()
                .ok_or_else(|| bad_primitive(index, "'contours' must be a list of outlines"))?
                .iter()
                .map(|c| points(c, "contours"))
                .collect::<PyResult<_>>()?,
            text: text("text"),
            ink: colour("ink")?,
        },
        Some("Symbol") => Primitive::Symbol {
            origin: point(&v["origin"], "origin")?,
            id: text("id"),
            size: number("size")?,
            rotation_deg: v["rotation_deg"].as_f64().unwrap_or(0.0),
        },
        other => return Err(bad_primitive(index, &format!("unknown type {other:?}"))),
    })
}

/// An SVG of primitives in the flat dict shape, on a canvas `width` by `height` whose lower-left
/// corner is the origin.
#[pyfunction]
fn svg_from_primitives(primitives_json: &str, width: f64, height: f64) -> PyResult<String> {
    let value: Value = serde_json::from_str(primitives_json).map_err(|e| PyValueError::new_err(e.to_string()))?;
    let list = value.as_array().ok_or_else(|| PyValueError::new_err("primitives must be a list"))?;
    let primitives = list.iter().enumerate().map(|(i, v)| primitive_from(i, v)).collect::<PyResult<Vec<_>>>()?;
    let extents = afrobim_draw::Extents { min: Pt2::new(0.0, 0.0), max: Pt2::new(width, height) };
    Ok(svg::write(&Drawing { primitives, extents }, None))
}

/// A sign definition with every field present, as the engine serialises it.
#[pyfunction]
fn normalise_definition(definition_json: &str) -> PyResult<String> {
    Ok(to_json(&parse_definition(definition_json)?))
}

/// The display sRGB of a named sign colour, as `#RRGGBB`.
#[pyfunction]
fn display_rgb(colour: &str) -> PyResult<String> {
    let c: Colour = serde_json::from_value(Value::String(colour.to_string()))
        .map_err(|_| PyValueError::new_err(format!("'{colour}' is not a sign colour")))?;
    Ok(svg::hex(c))
}

// ─── engine ──────────────────────────────────────────────────────────────────

#[pyclass(module = "afrobim._native", frozen)]
struct Engine {
    inner: SignEngine,
    pack: PathBuf,
    fonts: PathBuf,
}

impl Engine {
    /// Lays the sign out as the desktop app does for an element called `name`.
    fn layout_named(&self, definition: &SignDefinition, name: &str) -> PyResult<SignLayout> {
        let mut def = definition.clone();
        def.code = name.to_string();
        self.inner.layout(&def).map_err(sign_err)
    }
}

#[pymethods]
impl Engine {
    #[new]
    fn new(pack: PathBuf, fonts: PathBuf) -> PyResult<Self> {
        let inner = SignEngine::load(&pack, &fonts).map_err(sign_err)?;
        Ok(Engine { inner, pack, fonts })
    }

    #[getter]
    fn pack_id(&self) -> &str {
        self.inner.pack_id()
    }

    #[getter]
    fn pack_name(&self) -> &str {
        self.inner.pack_name()
    }

    #[getter]
    fn pack_path(&self) -> PathBuf {
        self.pack.clone()
    }

    #[getter]
    fn fonts_path(&self) -> PathBuf {
        self.fonts.clone()
    }

    fn catalog_json(&self) -> String {
        to_json(&self.inner.catalog())
    }

    fn definition_json(&self, key: &str, speed_kph: Option<u32>) -> PyResult<String> {
        let entry = self
            .inner
            .catalog_entry(key)
            .ok_or_else(|| PyKeyError::new_err(format!("no catalog entry '{key}' in rule pack {}", self.inner.pack_id())))?;
        let def = match speed_kph {
            Some(speed) => self.inner.definition_for_speed(entry, speed),
            None => self.inner.definition_for(entry),
        };
        Ok(to_json(&def))
    }

    fn layout(&self, definition_json: &str) -> PyResult<Layout> {
        let def = parse_definition(definition_json)?;
        Ok(Layout { inner: self.inner.layout(&def).map_err(sign_err)? })
    }

    fn sign_size_mm(&self, definition_json: &str) -> PyResult<Option<f64>> {
        Ok(self.inner.sign_size_mm(&parse_definition(definition_json)?))
    }

    fn size_bands_kph(&self) -> Vec<Option<i64>> {
        self.inner.sign_size_bands_kph().to_vec()
    }
}

// ─── layout ──────────────────────────────────────────────────────────────────

#[pyclass(module = "afrobim._native", frozen)]
struct Layout {
    inner: SignLayout,
}

impl Layout {
    fn in_mm(&self) -> Drawing {
        self.inner.drawing.scaled_to_origin(self.inner.sw_mm)
    }
}

#[pymethods]
impl Layout {
    #[getter]
    fn width_sw(&self) -> f64 {
        self.inner.drawing.extents.width()
    }

    #[getter]
    fn height_sw(&self) -> f64 {
        self.inner.drawing.extents.height()
    }

    #[getter]
    fn width_mm(&self) -> f64 {
        self.in_mm().extents.width()
    }

    #[getter]
    fn height_mm(&self) -> f64 {
        self.in_mm().extents.height()
    }

    #[getter]
    fn sw_mm(&self) -> f64 {
        self.inner.sw_mm
    }

    #[getter]
    fn x_height_mm(&self) -> f64 {
        self.inner.x_height_mm
    }

    #[getter]
    fn has_errors(&self) -> bool {
        self.inner.has_errors()
    }

    fn diagnostics_json(&self) -> String {
        to_json(&self.inner.diagnostics)
    }

    /// `mm` false: stroke widths, exactly as the engine laid the sign out. True: millimetres
    /// with the lower-left corner on the origin, as the desktop app draws it.
    fn primitives_json(&self, mm: bool) -> String {
        let prims = |d: &Drawing| Value::Array(d.primitives.iter().map(primitive_value).collect()).to_string();
        if mm {
            prims(&self.in_mm())
        } else {
            prims(&self.inner.drawing)
        }
    }

    /// DXF R12 in millimetres: the engine's drawing scaled by its stroke width, which is the
    /// call the C# reference writer is compared with.
    fn to_dxf(&self) -> String {
        afrobim_dxf::write(&self.inner.drawing, self.inner.sw_mm)
    }

    fn to_svg(&self) -> String {
        svg::write(&self.in_mm(), Some((self.inner.sw_mm, self.inner.x_height_mm)))
    }
}

// ─── document ────────────────────────────────────────────────────────────────

/// The mark the desktop app gives a new sign of each insertable kind.
fn default_mark(kind: SignKind) -> &'static str {
    match kind {
        SignKind::StackType | SignKind::MapTypeCrossroads | SignKind::MapTypeRoundabout | SignKind::MapTypeTee => "ADS-001",
        SignKind::DirectionSign => "DS-001",
        SignKind::Confirmatory => "RC-001",
        SignKind::PlaceIdentification => "PL-001",
        SignKind::LanePreselection => "LP-001",
        SignKind::Diversion => "DV-001",
        _ => "SIGN-001",
    }
}

#[pyclass(module = "afrobim._native")]
struct Doc {
    inner: Document,
}

impl Doc {
    fn require(&self, id: u64) -> PyResult<&afrobim_kernel::Element> {
        self.inner.find(ElementId(id)).ok_or_else(|| PyKeyError::new_err(format!("no element with id {id}")))
    }

    fn require_mut(&mut self, id: u64) -> PyResult<&mut afrobim_kernel::Element> {
        self.inner.find_mut(ElementId(id)).ok_or_else(|| PyKeyError::new_err(format!("no element with id {id}")))
    }
}

#[pymethods]
impl Doc {
    #[new]
    fn new() -> Self {
        Doc { inner: Document::default() }
    }

    #[staticmethod]
    fn load(path: PathBuf) -> PyResult<Doc> {
        Ok(Doc { inner: Document::load(&path).map_err(doc_err)? })
    }

    #[staticmethod]
    fn from_json(text: &str) -> PyResult<Doc> {
        Ok(Doc { inner: Document::from_json(text).map_err(doc_err)? })
    }

    fn save(&self, path: PathBuf) -> PyResult<()> {
        self.inner.save(&path).map_err(doc_err)
    }

    fn to_json(&self) -> String {
        self.inner.to_json()
    }

    #[getter]
    fn standard(&self) -> &str {
        &self.inner.standard
    }

    #[setter]
    fn set_standard(&mut self, value: String) {
        self.inner.standard = value;
    }

    fn len(&self) -> usize {
        self.inner.elements().len()
    }

    fn elements_json(&self) -> String {
        to_json(&self.inner.elements())
    }

    fn element_json(&self, id: u64) -> PyResult<String> {
        Ok(to_json(self.require(id)?))
    }

    fn layers_json(&self) -> String {
        to_json(&self.inner.layers())
    }

    fn unique_name(&self, name: &str) -> String {
        self.inner.unique_name(name)
    }

    /// (min x, min y, max x, max y) of every element, mm.
    fn extents(&self) -> Option<(f64, f64, f64, f64)> {
        self.inner.extents().map(|b| (b.min.x, b.min.y, b.max.x, b.max.y))
    }

    /// Files a sign as the desktop app does: category `sign`, layer `Signs`, the element's name
    /// is the mark (made unique), its data the serialised definition, its size the laid-out
    /// drawing's extents in mm. With no position it goes where automatic arrangement puts it.
    #[pyo3(signature = (engine, definition_json, at, mark))]
    fn add_sign(&mut self, engine: PyRef<'_, Engine>, definition_json: &str, at: Option<(f64, f64)>, mark: Option<&str>) -> PyResult<u64> {
        let def = parse_definition(definition_json)?;
        let wanted = match mark.map(str::trim).filter(|m| !m.is_empty()) {
            Some(m) => m,
            None if !def.code.trim().is_empty() => def.code.trim(),
            None => default_mark(def.kind),
        };
        let name = self.inner.unique_name(wanted);
        let layout = engine.layout_named(&def, &name)?;
        let size = layout.drawing.scaled_to_origin(layout.sw_mm).extents;
        let (w, h) = (size.width(), size.height());
        let origin = match at {
            Some((x, y)) => Pt2::new(x, y),
            None => self.inner.next_free_origin(w, h),
        };
        let data = serde_json::to_value(&def).expect("a sign definition always serialises");
        let id = self.inner.add(CATEGORY, &name, origin, w, h, LAYER, data);
        if self.inner.standard.is_empty() {
            self.inner.standard = engine.inner.pack_id().to_string();
        }
        Ok(id.0)
    }

    /// Replaces a sign's definition and refreshes its stored size.
    fn update_sign(&mut self, engine: PyRef<'_, Engine>, id: u64, definition_json: &str) -> PyResult<()> {
        let def = parse_definition(definition_json)?;
        let element = self.require_mut(id)?;
        if element.category != CATEGORY {
            return Err(PyValueError::new_err(format!("element {id} is a '{}', not a sign", element.category)));
        }
        let layout = engine.layout_named(&def, &element.name)?;
        let size = layout.drawing.scaled_to_origin(layout.sw_mm).extents;
        element.width = size.width();
        element.height = size.height();
        element.data = serde_json::to_value(&def).expect("a sign definition always serialises");
        Ok(())
    }

    /// Lays out the sign stored in an element.
    fn layout(&self, engine: PyRef<'_, Engine>, id: u64) -> PyResult<Layout> {
        let element = self.require(id)?;
        if element.category != CATEGORY {
            return Err(PyValueError::new_err(format!("element {id} is a '{}', not a sign", element.category)));
        }
        let def: SignDefinition = serde_json::from_value(element.data.clone())
            .map_err(|e| SignError::new_err(format!("{}: The sign definition is not valid: {e}", element.name)))?;
        Ok(Layout { inner: engine.layout_named(&def, &element.name)? })
    }

    fn rename(&mut self, id: u64, name: &str) -> PyResult<()> {
        self.require_mut(id)?.name = name.to_string();
        Ok(())
    }

    fn remove(&mut self, id: u64) -> bool {
        self.inner.remove(ElementId(id))
    }

    fn translate(&mut self, id: u64, dx: f64, dy: f64) -> PyResult<()> {
        self.require(id)?;
        self.inner.translate(&[ElementId(id)], dx, dy);
        Ok(())
    }

    fn move_to(&mut self, id: u64, x: f64, y: f64) -> PyResult<()> {
        self.require_mut(id)?.origin = Pt2::new(x, y);
        Ok(())
    }

    fn arrange(&mut self) {
        self.inner.arrange();
    }

    fn properties_json(&self, id: u64) -> PyResult<String> {
        Ok(to_json(&self.require(id)?.properties))
    }

    /// Sets one property; `None` removes it.
    fn set_property(&mut self, id: u64, key: &str, value: Option<String>) -> PyResult<()> {
        let props = &mut self.require_mut(id)?.properties;
        match value {
            Some(v) => props.insert(key.to_string(), v),
            None => props.remove(key),
        };
        Ok(())
    }

    /// Every sign at its position, in millimetres, as one DXF: what the desktop app's export
    /// writes with nothing selected. Elements of other categories are left out.
    fn export_dxf(&self, engine: PyRef<'_, Engine>) -> PyResult<String> {
        let mut dxf = afrobim_dxf::DxfDocument::new();
        for e in self.inner.elements().iter().filter(|e| e.category == CATEGORY) {
            let def: SignDefinition = serde_json::from_value(e.data.clone())
                .map_err(|err| SignError::new_err(format!("{}: The sign definition is not valid: {err}", e.name)))?;
            let layout = engine.layout_named(&def, &e.name)?;
            dxf.add(&layout.drawing.scaled_to_origin(layout.sw_mm), 1.0, e.origin);
        }
        Ok(dxf.finish())
    }
}

#[pymodule]
fn _native(m: &Bound<'_, PyModule>) -> PyResult<()> {
    m.add_class::<Engine>()?;
    m.add_class::<Layout>()?;
    m.add_class::<Doc>()?;
    m.add_function(wrap_pyfunction!(normalise_definition, m)?)?;
    m.add_function(wrap_pyfunction!(display_rgb, m)?)?;
    m.add_function(wrap_pyfunction!(svg_from_primitives, m)?)?;
    m.add("SignError", m.py().get_type::<SignError>())?;
    m.add("UnsupportedError", m.py().get_type::<UnsupportedError>())?;
    m.add("DocumentError", m.py().get_type::<DocumentError>())?;
    m.add("FILE_EXTENSION", afrobim_kernel::FILE_EXTENSION)?;
    Ok(())
}
