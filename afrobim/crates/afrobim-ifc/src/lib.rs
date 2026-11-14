//! IFC 4.3 export of AfroBIM documents.
//!
//! [`write_ifc`] turns a [`Document`] and the generated drawings of its elements into an
//! IFC4X3_ADD2 STEP physical file (ISO 10303-21): a project in millimetres, one site, and every
//! element as an `IfcSign` (or an `IfcBuildingElementProxy` for a category this crate does not
//! know) with a tessellated body, colours, property sets and quantities.
//!
//! [`export_ifc_model`] takes the same input and writes the whole model: posts as `IfcMember`
//! and foundations as `IfcFooting` with swept solids, each sign and its supports as one
//! `IfcElementAssembly`, every element at its `SitePlacement` inside an `IfcRoad`, and alignments
//! as `IfcAlignment`. It also writes the elements of the road modules (markings, side drains and
//! culverts, gantries, street lighting, road restraint, pavements, corridors and ground surfaces)
//! from the solids their modules generate, with the entity, predefined type, standard property
//! sets and quantities the README tabulates. For a document with none of those it writes the same
//! bytes as [`export_ifc`], which stays as it was: signs on the upright sheet.
//!
//! The `import` module reads IFC files (IFC2X3, IFC4, IFC4X3) as a reference model for display and
//! clash checking: [`import::import_ifc`]. The writer above does not change. See the crate README
//! for the entity mapping, the placement convention, how to validate a file and what the importer
//! reads.
//!
//! ```
//! use afrobim_ifc::{write_ifc, IfcElementInput, IfcOptions};
//! use afrobim_kernel::{Document, Pt};
//!
//! let mut doc = Document::default();
//! let id = doc.add("sign", "ADS-001", Pt::new(0.0, 0.0), 1200.0, 800.0, "0", serde_json::json!({}));
//! let inputs = [IfcElementInput { element: doc.find(id).unwrap(), generated: None }];
//! let text = write_ifc(&doc, &inputs, &IfcOptions { timestamp: Some(0), ..IfcOptions::default() });
//! assert!(text.starts_with("ISO-10303-21;"));
//! ```

mod guid;
pub mod import;
mod mesh;
mod model;
mod step;

use std::collections::{BTreeMap, BTreeSet};

use afrobim_draw::{Colour, Primitive};
use afrobim_kernel::{Document, Element, ElementId, Generated};

pub use guid::{global_id, name_uuid};
pub use model::{
    export_ifc_model, write_ifc_model, ALIGNMENT_CATEGORY, ALIGNMENT_CHORD_M, CORRIDOR_CATEGORY, CULVERT_CATEGORY, DRAIN_CATEGORY, FOUNDATION_CATEGORY, GANTRY_CATEGORY, LIGHTING_CATEGORY,
    MARKING_CATEGORY, PAVEMENT_CATEGORY, POST_CATEGORY, PROP_DESIGN_BASIS, RESTRAINT_CATEGORY, SURFACE_CATEGORY,
};
pub use mesh::{tessellate_artwork, Artwork, ArtworkMesh, ARC_BAND_WIDTH_MM};
pub use step::{step_real, step_string};

use step::{list, opt_string, reals, refs, Instances};

/// The schema identifier written to FILE_SCHEMA.
pub const SCHEMA: &str = "IFC4X3_ADD2";

/// Category of the elements exported as `IfcSign`.
pub const SIGN_CATEGORY: &str = "sign";

/// Longest `IfcLabel` / `IfcIdentifier`, in characters.
const MAX_LABEL: usize = 255;

/// What to write around the model.
#[derive(Debug, Clone, PartialEq)]
pub struct IfcOptions {
    /// `IfcProject.Name`, the file name in the header, and the scope of every GlobalId: the same
    /// project name and element id always give the same GlobalId.
    pub project_name: String,
    /// FILE_NAME author.
    pub author: String,
    /// FILE_NAME organization.
    pub organization: String,
    /// FILE_NAME originating system, e.g. `"AfroBIM 0.1.0"`.
    pub application: String,
    /// Seconds since the Unix epoch for the header time stamp. `None` takes the current time;
    /// pass a fixed value to get byte-identical files.
    pub timestamp: Option<i64>,
    /// Thickness of the sign plate, mm.
    pub plate_thickness_mm: f64,
    /// Distance between the plate face and the first artwork layer, and between artwork layers, mm.
    pub artwork_offset_mm: f64,
}

impl Default for IfcOptions {
    fn default() -> Self {
        Self {
            project_name: "AfroBIM project".to_string(),
            author: String::new(),
            organization: String::new(),
            application: "AfroBIM".to_string(),
            timestamp: None,
            plate_thickness_mm: 3.0,
            artwork_offset_mm: 0.1,
        }
    }
}

/// One element to export, with the drawing its module generated for it.
#[derive(Debug, Clone, Copy)]
pub struct IfcElementInput<'a> {
    pub element: &'a Element,
    /// `None` exports the plate without artwork and without the `AfroBIM_Derived` property set.
    /// [`export_ifc_model`] also reads `Generated::solids` from here: the colours of a support's
    /// solids, and the solids themselves when the support's definition cannot be read.
    pub generated: Option<&'a Generated>,
}

/// An exported file and what the exporter left out or changed.
#[derive(Debug, Clone, PartialEq)]
pub struct IfcExport {
    /// The STEP physical file.
    pub text: String,
    /// Human-readable remarks: skipped symbol placeholders, arcs written as bands, elements
    /// without geometry, repeated element ids.
    pub notes: Vec<String>,
}

/// The GlobalId [`write_ifc`] gives an element: stable for a project name and element id.
pub fn element_global_id(project_name: &str, id: ElementId) -> String {
    global_id(&format!("{project_name}/element/{}", id.0))
}

/// Writes the IFC file. Same as [`export_ifc`] without the notes.
pub fn write_ifc(document: &Document, elements: &[IfcElementInput], options: &IfcOptions) -> String {
    export_ifc(document, elements, options).text
}

/// Writes the IFC file for `elements` (normally every element of `document`, each with its
/// generated drawing) and reports what was left out.
///
/// The output depends only on the arguments: with a fixed `options.timestamp` the same input
/// gives byte-identical text.
pub fn export_ifc(document: &Document, elements: &[IfcElementInput], options: &IfcOptions) -> IfcExport {
    let mut w = Writer::new(options, Plan::default());
    let mut notes = Vec::new();
    let mut seen = BTreeSet::new();
    let mut products = Vec::new();
    for input in elements {
        if !seen.insert(input.element.id) {
            notes.push(format!("{}: element id {} given more than once; exported once", input.element.name, input.element.id.0));
            continue;
        }
        products.push(w.element(document, input, &mut notes));
    }
    if !products.is_empty() {
        w.contain(&products);
    }
    IfcExport { text: w.finish(), notes }
}

/// A typed `IfcPropertySingleValue` value.
enum Value {
    /// `IfcLabel`, or `IfcText` when longer than a label may be.
    Text(String),
    Identifier(String),
    Integer(i64),
    Real(f64),
    Boolean(bool),
    /// `IfcLengthMeasure`, in the file's length unit (mm).
    Length(f64),
    /// A measure of another named type, e.g. `IFCPOSITIVELENGTHMEASURE` or `IFCNONNEGATIVELENGTHMEASURE`,
    /// as a standard property set asks for it.
    Measure(&'static str, f64),
}

impl Value {
    fn encode(&self) -> String {
        match self {
            Value::Text(s) if s.chars().count() > MAX_LABEL => format!("IFCTEXT({})", step_string(s)),
            Value::Text(s) => format!("IFCLABEL({})", step_string(s)),
            Value::Identifier(s) => format!("IFCIDENTIFIER({})", step_string(&clip(s))),
            Value::Integer(i) => format!("IFCINTEGER({i})"),
            Value::Real(r) => format!("IFCREAL({})", step_real(*r)),
            Value::Boolean(b) => format!("IFCBOOLEAN({})", if *b { ".T." } else { ".F." }),
            Value::Length(l) => format!("IFCLENGTHMEASURE({})", step_real(*l)),
            Value::Measure(name, v) => format!("{name}({})", step_real(*v)),
        }
    }
}

/// At most [`MAX_LABEL`] characters.
fn clip(text: &str) -> String {
    text.chars().take(MAX_LABEL).collect()
}

/// A JSON scalar as text; `None` for null, arrays and objects.
fn scalar(value: Option<&serde_json::Value>) -> Option<String> {
    match value? {
        serde_json::Value::String(s) if !s.is_empty() => Some(s.clone()),
        serde_json::Value::Number(n) => Some(n.to_string()),
        serde_json::Value::Bool(b) => Some(b.to_string()),
        _ => None,
    }
}

/// What the file needs beyond the plain sign export. The default adds nothing, which keeps
/// [`export_ifc`] writing exactly what it always wrote.
#[derive(Debug, Clone, Copy, Default)]
struct Plan {
    /// An `IfcRoad` under the site, which then contains the elements.
    road: bool,
    /// A cubic metre in the unit assignment, for volume quantities.
    volume_unit: bool,
}

struct Writer<'a> {
    options: &'a IfcOptions,
    data: Instances,
    project: u32,
    context: u32,
    body_context: u32,
    /// The +Z `IfcDirection`, shared.
    up: u32,
    /// The spatial element that contains the elements (the site, or the road when there is one)
    /// and its placement.
    container: u32,
    container_placement: u32,
    /// The `IfcSite`, which is the container only while there is no road, and its placement.
    site: u32,
    site_placement: u32,
    /// `IfcSurfaceStyle` per style name, created on first use.
    styles: BTreeMap<String, u32>,
    thickness: f64,
    offset: f64,
}

impl<'a> Writer<'a> {
    /// Starts the file: units, contexts, project, site.
    fn new(options: &'a IfcOptions, plan: Plan) -> Self {
        let positive = |v: f64, fallback: f64| if v.is_finite() && v > 0.0 { v } else { fallback };
        let mut w = Writer {
            options,
            data: Instances::default(),
            project: 0,
            context: 0,
            body_context: 0,
            up: 0,
            container: 0,
            container_placement: 0,
            site: 0,
            site_placement: 0,
            styles: BTreeMap::new(),
            thickness: positive(options.plate_thickness_mm, 3.0),
            offset: positive(options.artwork_offset_mm, 0.1),
        };
        let d = &mut w.data;
        let origin = d.add("IFCCARTESIANPOINT", "(0.,0.,0.)");
        let z = d.add("IFCDIRECTION", "(0.,0.,1.)");
        let x = d.add("IFCDIRECTION", "(1.,0.,0.)");
        let world = d.add("IFCAXIS2PLACEMENT3D", &format!("#{origin},#{z},#{x}"));
        let context = d.add("IFCGEOMETRICREPRESENTATIONCONTEXT", &format!("$,'Model',3,1.E-5,#{world},$"));
        let body = d.add("IFCGEOMETRICREPRESENTATIONSUBCONTEXT", &format!("'Body','Model',*,*,*,*,#{context},$,.MODEL_VIEW.,$"));
        let length = d.add("IFCSIUNIT", "*,.LENGTHUNIT.,.MILLI.,.METRE.");
        let area = d.add("IFCSIUNIT", "*,.AREAUNIT.,$,.SQUARE_METRE.");
        let mut unit_ids = vec![length, area];
        if plan.volume_unit {
            unit_ids.push(d.add("IFCSIUNIT", "*,.VOLUMEUNIT.,$,.CUBIC_METRE."));
        }
        unit_ids.push(d.add("IFCSIUNIT", "*,.PLANEANGLEUNIT.,$,.RADIAN."));
        let units = d.add("IFCUNITASSIGNMENT", &refs(&unit_ids));
        let project_guid = step_string(&global_id(&format!("{}/project", options.project_name)));
        let project = d.add("IFCPROJECT", &format!("{project_guid},$,{},$,$,$,$,(#{context}),#{units}", step_string(&clip(&options.project_name))));
        let site_placement = d.add("IFCLOCALPLACEMENT", &format!("$,#{world}"));
        let site_guid = step_string(&global_id(&format!("{}/site", options.project_name)));
        let site = d.add("IFCSITE", &format!("{site_guid},$,'Site',$,$,#{site_placement},$,$,.ELEMENT.,$,$,$,$,$"));
        let rel_guid = step_string(&global_id(&format!("{}/rel-aggregates", options.project_name)));
        d.add("IFCRELAGGREGATES", &format!("{rel_guid},$,$,$,#{project},(#{site})"));
        w.container = site;
        w.container_placement = site_placement;
        w.site = site;
        w.site_placement = site_placement;
        if plan.road {
            // IfcRoad is the IfcFacility for a road; the spatial composition table of IFC 4.3
            // puts it under the site.
            let road_placement = d.add("IFCLOCALPLACEMENT", &format!("#{site_placement},#{world}"));
            let road_guid = step_string(&global_id(&format!("{}/road", options.project_name)));
            let road = d.add("IFCROAD", &format!("{road_guid},$,'Road',$,$,#{road_placement},$,$,.ELEMENT.,$"));
            let rel_guid = step_string(&global_id(&format!("{}/rel-site-road", options.project_name)));
            d.add("IFCRELAGGREGATES", &format!("{rel_guid},$,$,$,#{site},(#{road})"));
            w.container = road;
            w.container_placement = road_placement;
        }
        w.project = project;
        w.context = context;
        w.body_context = body;
        w.up = z;
        w
    }

    fn guid(&self, role: &str) -> String {
        global_id(&format!("{}/{role}", self.options.project_name))
    }

    /// One `IfcRelContainedInSpatialStructure` from the container to `products`.
    fn contain(&mut self, products: &[u32]) {
        let args = format!("{},$,$,$,{},#{}", step_string(&self.guid("rel-contained")), refs(products), self.container);
        self.data.add("IFCRELCONTAINEDINSPATIALSTRUCTURE", &args);
    }

    /// The shared surface style of a colour.
    fn style(&mut self, name: &str, rgb: [u8; 3]) -> u32 {
        self.style_keyed(name, name, rgb)
    }

    /// A shared surface style called `name`, one per `key`.
    fn style_keyed(&mut self, key: &str, name: &str, rgb: [u8; 3]) -> u32 {
        if let Some(id) = self.styles.get(key) {
            return *id;
        }
        let channels: Vec<f64> = rgb.iter().map(|c| f64::from(*c) / 255.0).collect();
        let colour = self.data.add("IFCCOLOURRGB", &format!("$,{},{},{}", step_real(channels[0]), step_real(channels[1]), step_real(channels[2])));
        let shading = self.data.add("IFCSURFACESTYLESHADING", &format!("#{colour},$"));
        let style = self.data.add("IFCSURFACESTYLE", &format!("{},.BOTH.,(#{shading})", step_string(name)));
        self.styles.insert(key.to_string(), style);
        style
    }

    /// A styled `IfcTriangulatedFaceSet`; returns the face set.
    fn face_set(&mut self, points: &[[f64; 3]], triangles: &[[u32; 3]], closed: bool, style: u32) -> u32 {
        let coords = self.data.add("IFCCARTESIANPOINTLIST3D", &format!("{},$", list(points.iter().map(|p| reals(p)))));
        let index = list(triangles.iter().map(|t| format!("({},{},{})", t[0] + 1, t[1] + 1, t[2] + 1)));
        let set = self.data.add("IFCTRIANGULATEDFACESET", &format!("#{coords},$,{},{index},$", if closed { ".T." } else { ".F." }));
        self.data.add("IFCSTYLEDITEM", &format!("#{set},(#{style}),$"));
        set
    }

    /// The Body representation of an element: the plate and, in front of it, the artwork.
    fn shape(&mut self, input: &IfcElementInput, notes: &mut Vec<String>) -> Option<u32> {
        let e = input.element;
        if !(e.width.is_finite() && e.height.is_finite() && e.width > 0.0 && e.height > 0.0) {
            notes.push(format!("{}: no geometry exported, size {} x {} mm is not positive", e.name, e.width, e.height));
            return None;
        }
        let plate_style = self.style("Plate", Colour::Grey.display_rgb());
        let mut items = vec![self.face_set(&plate_points(e.width, self.thickness, e.height), &PLATE_TRIANGLES, true, plate_style)];
        if let Some(generated) = input.generated {
            let artwork = tessellate_artwork(&generated.drawing);
            for mesh in &artwork.meshes {
                // Sheet X is local X, sheet Y is local Z; the face looks towards -Y.
                let y = -self.offset * f64::from(mesh.layer + 1);
                let points: Vec<[f64; 3]> = mesh.vertices.iter().map(|v| [v[0], y, v[1]]).collect();
                let style = self.style(&format!("{:?}", mesh.colour), mesh.colour.display_rgb());
                items.push(self.face_set(&points, &mesh.triangles, false, style));
            }
            let arcs = generated.drawing.primitives.iter().filter(|p| matches!(p, Primitive::Arc { .. })).count();
            if arcs > 0 {
                notes.push(format!("{}: {arcs} arc centreline(s) exported as bands {ARC_BAND_WIDTH_MM} mm wide", e.name));
            }
            if artwork.skipped_symbols > 0 {
                notes.push(format!("{}: {} symbol placeholder(s) not exported", e.name, artwork.skipped_symbols));
            }
            if artwork.skipped_degenerate > 0 {
                notes.push(format!("{}: {} degenerate primitive(s) not exported", e.name, artwork.skipped_degenerate));
            }
        }
        let representation = self.data.add("IFCSHAPEREPRESENTATION", &format!("#{},'Body','Tessellation',{}", self.body_context, refs(&items)));
        Some(self.data.add("IFCPRODUCTDEFINITIONSHAPE", &format!("$,$,(#{representation})")))
    }

    /// A property set and its relationship to `product`. Nothing is written for an empty set.
    fn property_set(&mut self, product: u32, element: ElementId, name: &str, properties: &[(String, Value)]) {
        self.property_set_of(product, &format!("element/{}", element.0), name, properties);
    }

    /// The same for any object; `owner` scopes the GlobalIds of the set and its relationship.
    fn property_set_of(&mut self, product: u32, owner: &str, name: &str, properties: &[(String, Value)]) {
        let ids: Vec<u32> = properties
            .iter()
            .filter(|(key, _)| !key.is_empty())
            .map(|(key, value)| self.data.add("IFCPROPERTYSINGLEVALUE", &format!("{},$,{},$", step_string(&clip(key)), value.encode())))
            .collect();
        if ids.is_empty() {
            return;
        }
        let guid = step_string(&self.guid(&format!("{owner}/pset/{name}")));
        let set = self.data.add("IFCPROPERTYSET", &format!("{guid},$,{},$,{}", step_string(name), refs(&ids)));
        self.define_of(product, owner, name, set);
    }

    /// An element quantity set and its relationship to `product`.
    fn quantity_set(&mut self, product: u32, element: ElementId, name: &str, lengths: &[(&str, f64)], area_m2: Option<f64>) {
        let mut ids: Vec<u32> =
            lengths.iter().map(|(n, v)| self.data.add("IFCQUANTITYLENGTH", &format!("{},$,$,{},$", step_string(n), step_real(*v)))).collect();
        if let Some(area) = area_m2 {
            ids.push(self.data.add("IFCQUANTITYAREA", &format!("'GrossArea',$,$,{},$", step_real(area))));
        }
        let guid = step_string(&self.guid(&format!("element/{}/qto/{name}", element.0)));
        let set = self.data.add("IFCELEMENTQUANTITY", &format!("{guid},$,{},$,$,{}", step_string(name), refs(&ids)));
        self.define(product, element, name, set);
    }

    fn define(&mut self, product: u32, element: ElementId, name: &str, definition: u32) {
        self.define_of(product, &format!("element/{}", element.0), name, definition);
    }

    fn define_of(&mut self, product: u32, owner: &str, name: &str, definition: u32) {
        let guid = step_string(&self.guid(&format!("{owner}/rel/{name}")));
        self.data.add("IFCRELDEFINESBYPROPERTIES", &format!("{guid},$,$,$,(#{product}),#{definition}"));
    }

    /// One element stood upright on the sheet: placement, then [`Writer::product`].
    fn element(&mut self, document: &Document, input: &IfcElementInput, notes: &mut Vec<String>) -> u32 {
        let e = input.element;
        // The sheet is stood upright: sheet X is world X, sheet Y is world Z.
        let location = self.data.add("IFCCARTESIANPOINT", &reals(&[e.origin.x, 0.0, e.origin.y]));
        let axes = self.data.add("IFCAXIS2PLACEMENT3D", &format!("#{location},$,$"));
        let placement = self.data.add("IFCLOCALPLACEMENT", &format!("#{},#{axes}", self.container_placement));
        self.product(document, input, placement, notes)
    }

    /// A sign (or a proxy for a category without a mapping) at `placement`: shape, the product
    /// itself, property sets and quantities.
    fn product(&mut self, document: &Document, input: &IfcElementInput, placement: u32, notes: &mut Vec<String>) -> u32 {
        let e = input.element;
        let is_sign = e.category == SIGN_CATEGORY;
        let kind = scalar(e.data.get("kind"));
        let catalog_key = scalar(e.data.get("catalog_key"));
        let speed = e.data.get("speed_kph").filter(|v| v.is_number() || v.is_string());

        let shape = self.shape(input, notes);

        let mut description = Vec::new();
        if let Some(k) = &kind {
            description.push(format!("kind {k}"));
        }
        if let Some(c) = &catalog_key {
            description.push(format!("catalog key {c}"));
        }
        if let Some(s) = scalar(speed) {
            description.push(format!("design speed {s} km/h"));
        }
        let description = description.join(", ");
        // A sign's ObjectType is its kind; a proxy is USERDEFINED, which makes ObjectType
        // mandatory, and carries its AfroBIM category there.
        let object_type = if is_sign {
            kind.as_deref().map(clip)
        } else if e.category.is_empty() {
            Some("unknown".to_string())
        } else {
            Some(clip(&e.category))
        };
        let common = format!(
            "{},$,{},{},{},#{placement},{},{}",
            step_string(&element_global_id(&self.options.project_name, e.id)),
            step_string(&clip(&e.name)),
            opt_string(Some(&description)),
            opt_string(object_type.as_deref()),
            shape.map_or("$".to_string(), |s| format!("#{s}")),
            step_string(&e.id.0.to_string()),
        );
        let product = if is_sign {
            self.data.add("IFCSIGN", &format!("{common},.PICTORAL."))
        } else {
            notes.push(format!("{}: category '{}' has no IFC mapping; exported as IfcBuildingElementProxy", e.name, e.category));
            self.data.add("IFCBUILDINGELEMENTPROXY", &format!("{common},.USERDEFINED."))
        };

        let mut identity = vec![
            ("Mark".to_string(), Value::Text(e.name.clone())),
            ("Category".to_string(), Value::Text(e.category.clone())),
            ("Layer".to_string(), Value::Text(e.layer.clone())),
        ];
        if !document.standard.is_empty() {
            identity.push(("Standard".to_string(), Value::Text(document.standard.clone())));
        }
        if let Some(k) = &kind {
            identity.push(("Kind".to_string(), Value::Text(k.clone())));
        }
        if let Some(c) = &catalog_key {
            identity.push(("CatalogKey".to_string(), Value::Text(c.clone())));
        }
        if let Some(s) = speed {
            let value = match (s.as_i64(), s.as_f64()) {
                (Some(i), _) => Value::Integer(i),
                (None, Some(r)) => Value::Real(r),
                _ => Value::Text(scalar(Some(s)).unwrap_or_default()),
            };
            identity.push(("DesignSpeedKph".to_string(), value));
        }
        self.property_set(product, e.id, if is_sign { "AfroBIM_Sign" } else { "AfroBIM_Element" }, &identity);

        if is_sign {
            // Pset_SignCommon (IFC 4.3): Reference, IsExternal, Category, TactileMarking. Only the
            // two this crate has a value for are written; a road sign stands outdoors.
            let mut common = Vec::new();
            if let Some(c) = &catalog_key {
                common.push(("Reference".to_string(), Value::Identifier(c.clone())));
            }
            common.push(("IsExternal".to_string(), Value::Boolean(true)));
            self.property_set(product, e.id, "Pset_SignCommon", &common);
        }

        let text = |map: &BTreeMap<String, String>| map.iter().map(|(k, v)| (k.clone(), Value::Text(v.clone()))).collect::<Vec<_>>();
        self.property_set(product, e.id, "AfroBIM_Properties", &text(&e.properties));
        if let Some(generated) = input.generated {
            self.property_set(product, e.id, "AfroBIM_Derived", &text(&generated.derived));
        }

        if shape.is_some() {
            if is_sign {
                // Qto_SignBaseQuantities (IFC 4.3): Height, Width, Thickness, Weight.
                let lengths = [("Height", e.height), ("Width", e.width), ("Thickness", self.thickness)];
                self.quantity_set(product, e.id, "Qto_SignBaseQuantities", &lengths, None);
            }
            // The standard set has no area, so the face area goes in a set of our own (m²).
            let lengths = [("Width", e.width), ("Height", e.height)];
            self.quantity_set(product, e.id, "AfroBIM_BaseQuantities", &lengths, Some(e.width * e.height / 1e6));
        }
        product
    }

    fn finish(self) -> String {
        let o = self.options;
        let seconds = o.timestamp.unwrap_or_else(|| {
            std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map_or(0, |d| d.as_secs() as i64)
        });
        let mut out = String::new();
        out.push_str("ISO-10303-21;\nHEADER;\n");
        out.push_str("FILE_DESCRIPTION(('ViewDefinition [ReferenceView]'),'2;1');\n");
        out.push_str(&format!(
            "FILE_NAME({},'{}',({}),({}),{},{},'');\n",
            step_string(&format!("{}.ifc", o.project_name)),
            step::iso_timestamp(seconds),
            step_string(&o.author),
            step_string(&o.organization),
            step_string(concat!("afrobim-ifc ", env!("CARGO_PKG_VERSION"))),
            step_string(&o.application),
        ));
        out.push_str(&format!("FILE_SCHEMA(('{SCHEMA}'));\nENDSEC;\nDATA;\n"));
        for line in self.data.lines() {
            out.push_str(line);
            out.push('\n');
        }
        out.push_str("ENDSEC;\nEND-ISO-10303-21;\n");
        out
    }
}

/// Corners of the plate in element coordinates: X along the face, Y into the plate, Z up. The
/// front face lies in the plane Y = 0 and the plate extends to Y = `thickness`.
fn plate_points(width: f64, thickness: f64, height: f64) -> [[f64; 3]; 8] {
    [
        [0.0, 0.0, 0.0],
        [width, 0.0, 0.0],
        [width, 0.0, height],
        [0.0, 0.0, height],
        [0.0, thickness, 0.0],
        [width, thickness, 0.0],
        [width, thickness, height],
        [0.0, thickness, height],
    ]
}

/// The plate as a closed shell, every triangle anticlockwise seen from outside.
const PLATE_TRIANGLES: [[u32; 3]; 12] = [
    [0, 1, 2],
    [0, 2, 3], // front, -Y
    [4, 6, 5],
    [4, 7, 6], // back, +Y
    [0, 4, 5],
    [0, 5, 1], // bottom, -Z
    [3, 2, 6],
    [3, 6, 7], // top, +Z
    [0, 3, 7],
    [0, 7, 4], // left, -X
    [1, 5, 6],
    [1, 6, 2], // right, +X
];

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_plate_is_a_closed_outward_facing_box() {
        let p = plate_points(1200.0, 3.0, 800.0);
        // Signed volume by the divergence theorem: positive when the normals point outwards.
        let volume: f64 = PLATE_TRIANGLES
            .iter()
            .map(|t| {
                let (a, b, c) = (p[t[0] as usize], p[t[1] as usize], p[t[2] as usize]);
                (a[0] * (b[1] * c[2] - b[2] * c[1]) - a[1] * (b[0] * c[2] - b[2] * c[0]) + a[2] * (b[0] * c[1] - b[1] * c[0])) / 6.0
            })
            .sum();
        assert!((volume - 1200.0 * 3.0 * 800.0).abs() < 1e-6, "{volume}");
        // Closed: every directed edge is matched by its reverse.
        let mut edges = BTreeSet::new();
        for t in PLATE_TRIANGLES {
            for k in 0..3 {
                assert!(edges.insert((t[k], t[(k + 1) % 3])));
            }
        }
        assert!(edges.iter().all(|(a, b)| edges.contains(&(*b, *a))));
    }
}
