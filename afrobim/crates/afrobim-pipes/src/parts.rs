//! Parts lists and the parts catalogue: user data, never built in.
//!
//! The crate ships no pipe sizes, materials or wall thicknesses. A catalogue is loaded from a
//! JSON or CSV file the project supplies. Each part may carry a BIM classification (IFC entity and
//! predefined type as free text), a material and a cost item code.
//!
//! IFC names: the only names this crate has seen in the repository's `afrobim-ifc` sources are
//! the entities `IFCPIPESEGMENT` (written there with predefined types `CULVERT` and `GUTTER`) and
//! `IFCDISTRIBUTIONCHAMBERELEMENT` (in its import tables). The crate fills in nothing: the two
//! fields are free text for the part's author, and are empty when not given.
//!
//! # CSV
//!
//! One header row, then one part per row. Columns (all lengths in metres, any order, unknown
//! columns ignored):
//!
//! `id, name, family, kind, shape, diameter_m, width_m, height_m, length_m, material,
//! wall_thickness_m, manning_n, ks_m, ifc_entity, ifc_predefined_type, cost_item_code`
//!
//! * `family` is `pipe` or `structure`.
//! * Pipes: `shape` is `circular` (uses `diameter_m`), `box`, `elliptical` or `egg` (use
//!   `width_m` and `height_m`). An egg needs a width profile, which CSV cannot carry; use JSON.
//! * Structures: `kind` is `manhole`, `catch_basin`, `headwall`, `outfall`, `junction` or `null`;
//!   `shape` is `circular` (`diameter_m`) or `rectangular` (`length_m`, `width_m`); empty `shape`
//!   means no inner dimensions.
//! * Roughness: `manning_n` or `ks_m` (not both).

use serde::{Deserialize, Serialize};

use crate::csv;
use crate::error::PipesError;
use crate::model::{Pipe, PipeShape, Roughness, Structure, StructureInner, StructureKind};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PartFamily {
    Pipe,
    Structure,
}

/// One catalogue part.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Part {
    pub id: String,
    pub name: String,
    pub family: PartFamily,
    /// Pipe parts: the cross-section.
    pub pipe_shape: Option<PipeShape>,
    /// Structure parts: the kind of structure.
    pub structure_kind: Option<StructureKind>,
    /// Structure parts: inner plan dimensions.
    pub structure_inner: Option<StructureInner>,
    pub material: Option<String>,
    pub wall_thickness_m: Option<f64>,
    pub roughness: Option<Roughness>,
    /// BIM classification: IFC entity name, free text (for example `IFCPIPESEGMENT`).
    pub ifc_entity: Option<String>,
    /// BIM classification: IFC predefined type, free text.
    pub ifc_predefined_type: Option<String>,
    pub cost_item_code: Option<String>,
}

/// A named list of parts.
#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
pub struct PartsCatalogue {
    pub name: String,
    pub source: Option<String>,
    pub parts: Vec<Part>,
}

impl PartsCatalogue {
    pub fn get(&self, id: &str) -> Option<&Part> {
        self.parts.iter().find(|p| p.id == id)
    }

    pub fn pipe_parts(&self) -> impl Iterator<Item = &Part> {
        self.parts.iter().filter(|p| p.family == PartFamily::Pipe)
    }

    pub fn structure_parts(&self) -> impl Iterator<Item = &Part> {
        self.parts.iter().filter(|p| p.family == PartFamily::Structure)
    }

    /// Check ids are unique and each part has what its family needs.
    pub fn validate(&self) -> Result<(), PipesError> {
        for (i, p) in self.parts.iter().enumerate() {
            if p.id.trim().is_empty() {
                return Err(PipesError::Parts(format!("part {} has no id", i + 1)));
            }
            if self.parts[..i].iter().any(|q| q.id == p.id) {
                return Err(PipesError::Parts(format!("part id '{}' is used twice", p.id)));
            }
            match p.family {
                PartFamily::Pipe => match &p.pipe_shape {
                    Some(s) if s.is_valid() => {}
                    _ => return Err(PipesError::Parts(format!("pipe part '{}' needs a valid shape with positive dimensions", p.id))),
                },
                PartFamily::Structure => {
                    if p.structure_kind.is_none() {
                        return Err(PipesError::Parts(format!("structure part '{}' needs a kind", p.id)));
                    }
                }
            }
            if let Some(w) = p.wall_thickness_m {
                if !(w.is_finite() && w >= 0.0) {
                    return Err(PipesError::Parts(format!("part '{}': wall thickness must not be negative", p.id)));
                }
            }
        }
        Ok(())
    }

    pub fn from_json(text: &str) -> Result<PartsCatalogue, PipesError> {
        let c: PartsCatalogue = serde_json::from_str(text).map_err(|e| PipesError::Parts(e.to_string()))?;
        c.validate()?;
        Ok(c)
    }

    pub fn to_json(&self) -> String {
        serde_json::to_string_pretty(self).expect("a catalogue always serialises")
    }

    /// Read a catalogue from CSV text, see the module documentation for the columns.
    pub fn from_csv(name: &str, text: &str) -> Result<PartsCatalogue, PipesError> {
        let rows = csv::parse(text).map_err(PipesError::Parts)?;
        let Some(header) = rows.first() else {
            return Err(PipesError::Parts("the file is empty".to_string()));
        };
        let header: Vec<String> = header.iter().map(|h| h.trim().to_ascii_lowercase()).collect();
        let col = |n: &str| header.iter().position(|h| h == n);
        for need in ["id", "family"] {
            if col(need).is_none() {
                return Err(PipesError::Parts(format!("the header has no '{need}' column")));
            }
        }
        let mut parts = Vec::new();
        for (ri, r) in rows.iter().enumerate().skip(1) {
            let line = ri + 1;
            let get = |n: &str| col(n).and_then(|c| r.get(c)).map(|s| s.trim()).filter(|s| !s.is_empty());
            let num = |n: &str| -> Result<Option<f64>, PipesError> {
                match get(n) {
                    None => Ok(None),
                    Some(s) => s.parse::<f64>().map(Some).map_err(|_| PipesError::Parts(format!("line {line}: '{s}' in column {n} is not a number"))),
                }
            };
            let id = get("id").ok_or_else(|| PipesError::Parts(format!("line {line}: no id")))?.to_string();
            let family = match get("family").map(|s| s.to_ascii_lowercase()).as_deref() {
                Some("pipe") => PartFamily::Pipe,
                Some("structure") => PartFamily::Structure,
                other => return Err(PipesError::Parts(format!("line {line}: family must be pipe or structure, got {other:?}"))),
            };
            let (d, w, h, l) = (num("diameter_m")?, num("width_m")?, num("height_m")?, num("length_m")?);
            let shape = get("shape").map(|s| s.to_ascii_lowercase());
            let need = |v: Option<f64>, what: &str| v.ok_or_else(|| PipesError::Parts(format!("line {line}: part '{id}' needs {what}")));
            let (mut pipe_shape, mut structure_kind, mut structure_inner) = (None, None, None);
            match family {
                PartFamily::Pipe => {
                    pipe_shape = Some(match shape.as_deref() {
                        Some("circular") => PipeShape::Circular { diameter_m: need(d, "diameter_m")? },
                        Some("box") => PipeShape::Box { width_m: need(w, "width_m")?, height_m: need(h, "height_m")? },
                        Some("elliptical") => PipeShape::Elliptical { width_m: need(w, "width_m")?, height_m: need(h, "height_m")? },
                        Some("egg") => PipeShape::Egg { width_m: need(w, "width_m")?, height_m: need(h, "height_m")?, profile: Vec::new() },
                        other => return Err(PipesError::Parts(format!("line {line}: pipe shape must be circular, box, elliptical or egg, got {other:?}"))),
                    });
                }
                PartFamily::Structure => {
                    let k = get("kind").ok_or_else(|| PipesError::Parts(format!("line {line}: structure '{id}' needs a kind")))?;
                    structure_kind = Some(StructureKind::from_key(k).ok_or_else(|| PipesError::Parts(format!("line {line}: unknown structure kind '{k}'")))?);
                    structure_inner = match shape.as_deref() {
                        None => None,
                        Some("circular") => Some(StructureInner::Circular { diameter_m: need(d, "diameter_m")? }),
                        Some("rectangular") => Some(StructureInner::Rectangular { length_m: need(l, "length_m")?, width_m: need(w, "width_m")? }),
                        other => return Err(PipesError::Parts(format!("line {line}: structure shape must be circular or rectangular, got {other:?}"))),
                    };
                }
            }
            let roughness = match (num("manning_n")?, num("ks_m")?) {
                (Some(_), Some(_)) => return Err(PipesError::Parts(format!("line {line}: give manning_n or ks_m, not both"))),
                (Some(n), None) => Some(Roughness::Manning { n }),
                (None, Some(ks_m)) => Some(Roughness::ColebrookWhite { ks_m }),
                (None, None) => None,
            };
            parts.push(Part {
                name: get("name").unwrap_or(&id).to_string(),
                id,
                family,
                pipe_shape,
                structure_kind,
                structure_inner,
                material: get("material").map(str::to_string),
                wall_thickness_m: num("wall_thickness_m")?,
                roughness,
                ifc_entity: get("ifc_entity").map(str::to_string),
                ifc_predefined_type: get("ifc_predefined_type").map(str::to_string),
                cost_item_code: get("cost_item_code").map(str::to_string),
            });
        }
        let c = PartsCatalogue { name: name.to_string(), source: None, parts };
        c.validate()?;
        Ok(c)
    }

    /// Write the catalogue as CSV (an egg's width profile is not written; use JSON).
    pub fn to_csv(&self) -> String {
        let mut out = csv::row(
            &["id", "name", "family", "kind", "shape", "diameter_m", "width_m", "height_m", "length_m", "material", "wall_thickness_m", "manning_n", "ks_m", "ifc_entity", "ifc_predefined_type", "cost_item_code"].map(String::from),
        );
        let f = |v: Option<f64>| v.map(|x| x.to_string()).unwrap_or_default();
        for p in &self.parts {
            let (mut shape, mut d, mut w, mut h, mut l) = (String::new(), None, None, None, None);
            match (&p.pipe_shape, &p.structure_inner) {
                (Some(PipeShape::Circular { diameter_m }), _) => {
                    shape = "circular".into();
                    d = Some(*diameter_m);
                }
                (Some(PipeShape::Box { width_m, height_m }), _) => {
                    shape = "box".into();
                    (w, h) = (Some(*width_m), Some(*height_m));
                }
                (Some(PipeShape::Elliptical { width_m, height_m }), _) => {
                    shape = "elliptical".into();
                    (w, h) = (Some(*width_m), Some(*height_m));
                }
                (Some(PipeShape::Egg { width_m, height_m, .. }), _) => {
                    shape = "egg".into();
                    (w, h) = (Some(*width_m), Some(*height_m));
                }
                (None, Some(StructureInner::Circular { diameter_m })) => {
                    shape = "circular".into();
                    d = Some(*diameter_m);
                }
                (None, Some(StructureInner::Rectangular { length_m, width_m })) => {
                    shape = "rectangular".into();
                    (l, w) = (Some(*length_m), Some(*width_m));
                }
                (None, None) => {}
            }
            let (n, ks) = match p.roughness {
                Some(Roughness::Manning { n }) => (Some(n), None),
                Some(Roughness::ColebrookWhite { ks_m }) => (None, Some(ks_m)),
                None => (None, None),
            };
            out.push_str(&csv::row(&[
                p.id.clone(),
                p.name.clone(),
                if p.family == PartFamily::Pipe { "pipe" } else { "structure" }.to_string(),
                p.structure_kind.map(|k| k.key().to_string()).unwrap_or_default(),
                shape,
                f(d),
                f(w),
                f(h),
                f(l),
                p.material.clone().unwrap_or_default(),
                f(p.wall_thickness_m),
                f(n),
                f(ks),
                p.ifc_entity.clone().unwrap_or_default(),
                p.ifc_predefined_type.clone().unwrap_or_default(),
                p.cost_item_code.clone().unwrap_or_default(),
            ]));
        }
        out
    }
}

impl Part {
    /// Apply a pipe part to a pipe: shape, material, wall thickness, roughness and the part
    /// reference. Inverts, name, flow and the curve are left as they are.
    pub fn apply_to_pipe(&self, pipe: &mut Pipe) -> Result<(), PipesError> {
        let shape = match (&self.family, &self.pipe_shape) {
            (PartFamily::Pipe, Some(s)) => s.clone(),
            _ => return Err(PipesError::Parts(format!("part '{}' is not a pipe part", self.id))),
        };
        pipe.shape = shape;
        pipe.material = self.material.clone();
        pipe.wall_thickness_m = self.wall_thickness_m;
        if self.roughness.is_some() {
            pipe.roughness = self.roughness;
        }
        pipe.part = Some(self.id.clone());
        Ok(())
    }

    /// Apply a structure part: kind, inner dimensions, wall thickness and the part reference.
    pub fn apply_to_structure(&self, s: &mut Structure) -> Result<(), PipesError> {
        let kind = match (&self.family, self.structure_kind) {
            (PartFamily::Structure, Some(k)) => k,
            _ => return Err(PipesError::Parts(format!("part '{}' is not a structure part", self.id))),
        };
        s.kind = kind;
        s.inner = self.structure_inner;
        s.wall_thickness_m = self.wall_thickness_m;
        s.part = Some(self.id.clone());
        Ok(())
    }
}
