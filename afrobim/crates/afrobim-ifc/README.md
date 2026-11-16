# afrobim-ifc

IFC 4.3 export of AfroBIM documents, and (module `import`) IFC import as a read-only reference model;
see "Import as a reference model" at the end. The exporter below is unchanged by the importer.

The crate writes an **IFC4X3_ADD2** STEP physical file (ISO 10303-21) itself, in pure Rust, and
does not link an IFC toolkit. It has two entry points that take the same input:

| Entry point | Writes |
| --- | --- |
| `export_ifc` | The signs, stood upright as they are laid out on the design sheet. Unchanged since the first version. |
| `export_ifc_model` | The model: signs with their posts and foundations as assemblies, every element where it stands on site, the alignments, and the elements of the road modules (markings, drainage, gantries, lighting, restraint, pavement, corridor, ground). |

For a document without supports, site placements, alignments and road-module elements the two
give byte-identical files. `export_ifc` ignores `Element::site` and `Element::host` and writes a post or a foundation
as an `IfcBuildingElementProxy`; it is kept so that existing callers compile and behave as before.

```rust
use afrobim_ifc::{export_ifc_model, IfcElementInput, IfcOptions};

let inputs: Vec<IfcElementInput> = document
    .elements()
    .iter()
    .map(|element| IfcElementInput { element, generated: generated_for(element) }) // Module::generate
    .collect();
let options = IfcOptions { project_name: "A104 signs".into(), ..IfcOptions::default() };
let export = export_ifc_model(&document, &inputs, &options);
std::fs::write("a104-signs.ifc", &export.text)?;
for note in &export.notes {
    eprintln!("{note}"); // what was skipped or approximated
}
```

## API

| Item | Purpose |
| --- | --- |
| `IfcOptions` | Project name, header author / organization / application, time stamp (`None` = now), plate thickness (3 mm), artwork offset (0.1 mm), and `alignment`: what is written for alignments and how what stands on them is placed. |
| `AlignmentExport` | `Simple` (the default: a polyline, absolute placements, byte for byte what earlier versions wrote), `Geometry` (layouts, curves, referents) or `Linear` (and `IfcLinearPlacement` for elements with a station). See "Alignments, complete". |
| `IfcElementInput` | An element and, optionally, the `Generated` its module produced for it (drawing, derived values, solids). |
| `export_ifc_model(document, elements, options) -> IfcExport` | The model file (`text`) plus `notes`. `write_ifc_model` returns the text only. |
| `export_ifc(document, elements, options) -> IfcExport` | The sign-sheet file plus `notes`. `write_ifc` returns the text only. |
| `element_global_id(project_name, id)` / `global_id(name)` | The deterministic GlobalIds the exporter uses. |
| `POST_CATEGORY`, `FOUNDATION_CATEGORY`, `ALIGNMENT_CATEGORY`, `SIGN_CATEGORY` | The element categories with a mapping. |
| `MARKING_CATEGORY`, `DRAIN_CATEGORY`, `CULVERT_CATEGORY`, `GANTRY_CATEGORY`, `LIGHTING_CATEGORY`, `RESTRAINT_CATEGORY`, `PAVEMENT_CATEGORY`, `CORRIDOR_CATEGORY`, `SURFACE_CATEGORY` | The road-module categories with a mapping, re-exported from their crates. |
| `PROP_DESIGN_BASIS` | The element property (`"design_basis"`) copied to `DesignBasis`. |
| `ALIGNMENT_CHORD_M` | Longest chord of an alignment's Axis polyline (1 m). |
| `tessellate_artwork(drawing) -> Artwork` | The triangulation on its own: one `ArtworkMesh` per colour and paint layer. |
| `step_string`, `step_real` | STEP literal encoding. |

With a fixed `IfcOptions::timestamp` the same input gives byte-identical text.

`export_ifc_model` needs nothing the application does not already hand to `export_ifc`: the
elements of the document (it follows `Element::host` and reads `Element::site`) and, per element,
the `Generated` of its module. The post and foundation definitions are read from `Element::data`
with `afrobim-supports`, and alignments with `afrobim-alignment`; this crate depends on both. The
road modules' definitions are read with `afrobim-markings`, `-drainage`, `-gantry`, `-lighting`,
`-restraint`, `-pavement` and `-corridor`, on which it also depends (see "The road modules").
The signature of `export_ifc_model` and the structs it takes are unchanged: the application's
existing call exports the road modules as soon as it hands over their elements and, where it has
them, what their modules generated. When `IfcElementInput::generated` is `None` for a road
element, the exporter runs that element's module itself; a corridor is always built from the
document (`afrobim_corridor::corridor_of`), because its module makes no solids.

## What both entry points export

| AfroBIM | IFC 4.3 |
| --- | --- |
| The export | `IfcProject` (name from the options), units `IfcSIUnit` millimetre, square metre, radian |
| Model space | `IfcGeometricRepresentationContext` "Model", 3D, with a "Body" `IfcGeometricRepresentationSubContext` (`MODEL_VIEW`) |
| (implicit) | One `IfcSite` "Site", aggregated by the project with `IfcRelAggregates` |
| Element, category `sign` | `IfcSign`, `PredefinedType` `PICTORAL` |
| Element, a category without a mapping | `IfcBuildingElementProxy`, `PredefinedType` `USERDEFINED`, `ObjectType` = the category |
| `Element::id` | `Tag`, and (with the project name) the `GlobalId` |
| `Element::name` (the mark) | `Name` |
| `data.kind` | `ObjectType` (signs), `AfroBIM_Sign.Kind` |
| `data.kind`, `data.catalog_key`, `data.speed_kph` | `Description`, e.g. "kind StackType, catalog key GFS-B1, design speed 80 km/h" |
| Membership of the document | One `IfcRelContainedInSpatialStructure` to every element that is not part of an assembly |
| `Element::width` × `height`, plate thickness | The plate: a closed `IfcTriangulatedFaceSet` box (12 triangles), grey |
| `Generated::drawing` | The artwork: one open `IfcTriangulatedFaceSet` per colour and paint layer |
| `Colour` | `IfcStyledItem` → `IfcSurfaceStyle` (`BOTH`) → `IfcSurfaceStyleShading` → `IfcColourRgb`, one style per colour per file, from `Colour::display_rgb()` |
| Mark, category, layer, `Document::standard`, kind, catalog key, design speed | `IfcPropertySet` `AfroBIM_Sign` (`AfroBIM_Element` for a proxy): `Mark`, `Category`, `Layer`, `Standard`, `Kind`, `CatalogKey`, `DesignSpeedKph` |
| `data.catalog_key`; a road sign stands outdoors | `Pset_SignCommon`: `Reference`, `IsExternal` = true (signs only) |
| `Element::properties` | `IfcPropertySet` `AfroBIM_Properties`, each value an `IfcLabel` (an `IfcText` beyond 255 characters) |
| `Generated::derived` | `IfcPropertySet` `AfroBIM_Derived` |
| Width, height, plate thickness | `IfcElementQuantity` `Qto_SignBaseQuantities`: `Width`, `Height`, `Thickness` (signs only) |
| Width, height, face area | `IfcElementQuantity` `AfroBIM_BaseQuantities`: `Width`, `Height` (mm), `GrossArea` (m²) |

The sign's `data` is treated as opaque JSON: only `kind`, `catalog_key` and `speed_kph` are
read, and only when they are scalars. Empty property sets are not written. There is no
`IfcOwnerHistory`: it is optional in IFC 4.3, and the author, organization and application go in
the file header instead.

A sign's Body has `RepresentationType` `Tessellation`, so every item in it is a tessellated face
set; that is why the plate is a triangulated box and not an `IfcExtrudedAreaSolid`.

### Artwork

- `Fill` outlines are triangulated with the non-zero rule and `Glyphs` contours with the even-odd
  rule, exactly as the canvas draws them (`lyon_tessellation`). `Line` bands keep their square ends.
- `Arc` is a centreline without a width; it is exported as a band 1 mm wide
  (`ARC_BAND_WIDTH_MM`) in chords of at most 6°. `Symbol` placeholders have no geometry and are
  skipped. Both are reported in `IfcExport::notes`.
- A drawing is a painter's list: later primitives cover earlier ones. In 3D, coplanar faces of
  different colours would flicker, so paint order becomes distance from the plate. A primitive is on
  layer 0 unless its bounding box overlaps an earlier primitive of another colour, in which case
  it sits one layer in front of the highest such primitive. Layer *n* lies
  `artwork_offset_mm × (n + 1)` in front of the plate face (0.1 mm, 0.2 mm, ...). A typical sign
  needs two or three layers: background, then border and legend, then anything on a patch.
- Artwork is not clipped to the plate. A drawing that is larger than the element's width × height
  is exported as drawn.

### GlobalIds

A GlobalId is the IFC compression of an RFC 4122 version 5 (SHA-1, name-based) UUID of
`"<project name>/<role>"` in a fixed AfroBIM namespace: `"<project>/project"`, `"<project>/site"`,
`"<project>/element/<id>"`, `"<project>/element/<id>/pset/<set name>"`,
`"<project>/element/<sign id>/assembly"` and so on. The same project name and element id
therefore always give the same GlobalId, and re-exporting a document keeps the identity of its
objects. A kernel `Document` has no identifier of its own, so the project name is the scope:
renaming the project changes every GlobalId, and two different documents exported under one
project name share GlobalIds for elements with the same id.

## What `export_ifc_model` adds

| AfroBIM | IFC 4.3 |
| --- | --- |
| Element, category `post` | `IfcMember`, `PredefinedType` `POST`, Body `SweptSolid` |
| Element, category `foundation` | `IfcFooting`, `PredefinedType` `PAD_FOOTING`, Body `SweptSolid` |
| A sign and the supports whose chain of `host`s leads to it | `IfcElementAssembly`, `PredefinedType` `SIGNALASSEMBLY`, named after the sign, no representation of its own; one `IfcRelAggregates` to the `IfcSign`, the `IfcMember`s and the `IfcFooting`s |
| `Element::site` | `IfcLocalPlacement` at that position and rotation (see below); `IfcPropertySet` `AfroBIM_SitePlacement` |
| Anything placed on site, or an alignment | One `IfcRoad` "Road" aggregated by the site; it, not the site, then contains the elements |
| Element, category `alignment` | `IfcAlignment` aggregated by the project, with its horizontal layout and a 2D Axis polyline (see below) |
| `PostDefinition::material`, `FoundationDefinition::concrete` | `IfcMaterial` with that text as its `Name`, one per distinct text, through `IfcRelAssociatesMaterial` |
| Any support | A cubic metre in the unit assignment, for the volume quantities |

### Entity choices

These were checked against the IFC 4.3 documentation sources (the markdown of
`buildingSMART/IFC4.3.x-development`, `docs/schemas/...`), the enumerations of the
IFC4X3_ADD2 schema as IfcOpenShell 0.9.0 ships it, and the buildingSMART validation rules
(`buildingSMART/ifc-gherkin-rules`) named below.

- **Post → `IfcMember.POST`**, not `IfcColumn`. `IfcMemberTypeEnum.POST` is "a linear (usually
  vertical) member used to support something or to mark a point", and `IfcMember` "is not
  required to be load bearing"; `IfcColumn` is a member "that transmits, through compression, the
  weight of the structure above", and its predefined types (`COLUMN`, `PIERSTEM`, `PILASTER`,
  `STANDCOLUMN`) are building and bridge parts.
- **Foundation → `IfcFooting.PAD_FOOTING`**: "an element that transfers the load of a single
  column (possibly two) to the ground". `CAISSON_FOUNDATION` is for underwater construction and
  deprecated; `STRIP_FOOTING`, `FOOTING_BEAM` and `PILE_CAP` are other things. Both shapes
  (block and cylinder) use it. `IfcFooting` is defined as a shallow foundation; whether a deep
  planted cylinder would be better called an `IfcDeepFoundation` is a judgement this exporter
  does not make.
- **Sign with supports → `IfcElementAssembly.SIGNALASSEMBLY` with `IfcRelAggregates`**, not
  `IfcRelConnectsElements`. `IfcSign` itself says a sign can "be aggregated within a Signal
  Assembly which can include multiple sign occurrences and the associated supporting structural
  elements", and `SIGNALASSEMBLY` is "an assembly to physically aggregate together one or more
  signal instances (and also sign instances) including any supporting structural elements such
  as a simple pole". Aggregation is what a viewer shows as one object in its tree;
  `IfcRelConnectsElements` states a physical connection between two elements and groups nothing.
  As `IfcElementAssembly` requires (and bSI rule SPS007 checks), the parts are *not* also
  contained in the spatial structure: only the assembly is. Each part is placed relative to the
  assembly's placement (bSI rule OJP001). No connection relationships (post to footing, sign to
  post) are written.
- **Container → `IfcRoad`** under the `IfcSite` (`IfcRoad, IfcSite` in the bSI spatial
  composition table). It is written only once something is placed on site or an alignment is
  exported, so that a plain sign sheet keeps the file it always had. There are no `IfcRoadPart`s.
- **Alignment → `IfcAlignment`** related to the `IfcProject` by `IfcRelAggregates` and never
  contained in a spatial element ("Alignment Aggregation To Project"; bSI rule ALB004).

### Geometry of posts and foundations

Solids are swept, from the definition in `Element::data`, not from the display meshes:

| Definition | Profile (`ProfileType` `AREA`), extruded along +Z |
| --- | --- |
| `Section::Chs` | `IfcCircleHollowProfileDef` (radius, wall) |
| `Section::Rhs` | `IfcRectangleHollowProfileDef` (X, Y, wall, inner and outer fillet radius when not zero) |
| `Section::ISection` | `IfcIShapeProfileDef` (overall width = flange width, overall depth, web, flange) |
| `Section::Channel` | `IfcUShapeProfileDef` (depth, flange width, web, flange), web on the low-X side |
| `Section::SolidRound`, `FoundationShape::Cylinder` | `IfcCircleProfileDef` |
| `Section::SolidRect`, `FoundationShape::Block` | `IfcRectangleProfileDef` |

The frame is the one `afrobim-supports` documents: +X along the sign's width, +Y behind the face,
+Z up, **z = 0 at ground level**, the plan bounding box with its corner on the origin. A post is
one `IfcExtrudedAreaSolid` from z = −embedment over its full length. A foundation is one from
z = −(top below ground + depth) over its depth, plus a second one of the same outline under it
for the blinding. So, unlike the 3D view, a hollow section is hollow and a round one is round.

- The **socket** of a foundation is not cut out of the solid (a note says so).
  `Qto_FootingBaseQuantities.NetVolume` deducts it; `GrossVolume` does not.
- A support whose definition cannot be read (or has errors) falls back to the solids its module
  generated: one `IfcTriangulatedFaceSet` per `Mesh3` (`Closed` only when every edge is matched),
  Body `Tessellation`, with a note. With no solids either, the product is written without a shape.
- Colours are those of the module's solids (`Mesh3::colour`); they are screen colours.

### Property sets and quantities of supports

| Set | Content |
| --- | --- |
| `AfroBIM_Post` | `Mark`, `Category`, `Layer`, `SignMark`, `Section` (the entered dimensions as text), `Length`, `Embedment`, `AboveGround` (mm), `Material`, `Finish`, `PlaceholderDimensions`, `DesignBasis` |
| `AfroBIM_Foundation` | `Mark`, `Category`, `Layer`, `SignMark`, `HostMark` (the post), `Shape`, `Length` / `Width` or `Diameter`, `Depth`, `TopBelowGround`, `FormationDepth`, `BlindingThickness`, `Socket`, `Concrete`, `PlaceholderDimensions`, `DesignBasis` |
| `AfroBIM_SignAssembly` | `Mark`, `Posts`, `Foundations`, `MountingHeight`, and `DesignBasis` when every support states the same one |
| `AfroBIM_SitePlacement` | `X`, `Y`, `Z` (mm), `RotationDeg`, and from the sign's placement `Alignment`, `StationM`, `OffsetM`; on the assembly, the sign and every placed support |
| `AfroBIM_Properties`, `AfroBIM_Derived` | As for signs: the element's properties and the module's derived values, complete |
| `Pset_MemberCommon` | `IsExternal` = true (posts) |
| `Qto_MemberBaseQuantities` | `Length` (mm), `CrossSectionArea`, `OuterSurfaceArea` (m²), `NetVolume` (m³) |
| `Qto_FootingBaseQuantities` | `Length`, `Width` (blocks), `Height`, `CrossSectionArea`, `GrossVolume`, `NetVolume`; the concrete body, without the blinding |

`Pset_MemberCommon`, `Pset_SignCommon`, `Pset_Stationing` and the three `Qto_` sets are standard
IFC 4.3 sets; their names, applicable entities and property names were checked against the
property-set templates IfcOpenShell ships. Everything else is `AfroBIM_`.

`DesignBasis` is the element property `design_basis` copied unchanged. Materials are the free
text of the definition and nothing more: an `IfcMaterial` has that text as its name and no
category, grade or description, and none is written when the text is empty. A value is only
written when the model holds it; `PlaceholderDimensions` appears (as true) on a support whose
dimensions are still the placeholders, with a note.

Quantities are the geometry of the definition: section area × length for a post (the material,
not the outer cylinder), outer perimeter × length for its surface, plan area × depth for a
foundation. No mass is written.

### Placement

The convention is the 3D view's (`afrobim-app/src/view3d.rs`) and `afrobim-supports`':

- A `SitePlacement` is a point in the site's plan system (mm, Z up) and a turn `rotation_deg`
  about +Z, counter-clockwise from +X. Local (u along the width, v into the depth, w up) is at
  `(x + u·cos − v·sin, y + u·sin + v·cos, z + w)`. A sign's face lies in its local XZ plane and
  looks towards −Y, so a sign for traffic on heading θ has rotation θ − 90°.
- A **sign's** placement is the point on the ground under its lower-left corner. The `IfcSign`
  stands one mounting height above it (the mounting height recorded on its first post; 0 for a
  sign without posts). The **assembly's** placement is that ground point with the sign's rotation.
- A **support's** placement is its own `SitePlacement`: the corner of its plan bounding box at
  ground level. A support without one is put under its sign from the position along the sign
  and the setback recorded on its post, as `afrobim_supports::sync_support_sites` would.
- In the file an element is placed relative to the road (or the site), and a part of an assembly
  relative to the assembly. Rotations are about +Z only (`Axis` (0, 0, 1), `RefDirection`
  (cos, sin, 0)), written to 12 decimals; lengths to a micro-millimetre.
- An element **without** a site placement keeps the upright-sheet position below, also when
  other elements are on site (a note counts them). On the sheet a sign's supports stand under
  it: the ground is one mounting height below the sign's lower edge, in the plane Y = 0.
  (One difference from the 3D view: a shared foundation is centred from its own recorded
  position, as `sync_support_sites` does, where the view uses its post's.)
- Site coordinates are written as they are. There is no `IfcMapConversion` or coordinate
  reference system, because the document has none.

### Alignments (the default, `AlignmentExport::Simple`)

By default an alignment is exported as business logic plus a simple curve (`AlignmentExport::Geometry`
and `AlignmentExport::Linear` write the complete alignment: see "Alignments, complete" below):

- `IfcAlignment` nests (`IfcRelNests`) one `IfcAlignmentHorizontal`, which nests one
  `IfcAlignmentSegment` per element, each with an `IfcAlignmentHorizontalSegment`: start point
  (mm), start direction (radians, counter-clockwise from +X), start and end radius (mm; 0 is a
  straight, positive turns left), length, and `LINE`, `CIRCULARARC` or `CLOTHOID`. The layout
  ends with a segment of length 0 at the end point and direction (bSI rule ALB015).
- Its representation is `Axis` / `Curve2D`: an `IfcIndexedPolyCurve` through every element
  joint, with curves and spirals in equal chords of at most 1 m (`ALIGNMENT_CHORD_M`). A 2D
  polyline is one of the shape representations `IfcAlignment` supports (bSI rule ALS005); the
  exact geometry is in the segments. **No `IfcCompositeCurve` / `IfcCurveSegment` geometry is
  written.**
- One `IfcReferent` (`STATION`) at the start, nested by the alignment, carries the start station
  in `Pset_Stationing.Station` (mm). `AfroBIM_Alignment` has `StartStationM`, `EndStationM`,
  `LengthM`, `Elements`.
- `IfcRelPositions` relates the alignment to the assemblies and signs whose site placement names
  it. Their placements stay `IfcLocalPlacement`s; the station and offset are in
  `AfroBIM_SitePlacement`. No `IfcLinearPlacement` is written.
- An alignment that cannot be read, has no elements, has a non-finite or non-positive number, or
  has a spiral whose curvature changes sign is **not exported at all**, with a note saying why.
  Joints that are not continuous are exported as given, with a note.
- In this mode only the horizontal geometry is exported: no `IfcAlignmentVertical`, and no cant
  (`Geometry` and `Linear` write the vertical layout; cant is never written, see below).

### Alignments, complete (`AlignmentExport::Geometry` and `AlignmentExport::Linear`)

`IfcOptions::alignment` selects what is written for an alignment and for what stands on it. The
default, `AlignmentExport::Simple`, is the behaviour described above and writes the files earlier
versions wrote, byte for byte (`tests/linear.rs` holds the length and a checksum of four such
files, taken from the previous commit). To switch the new behaviour on:

```rust
let options = IfcOptions {
    project_name: "A104 signs".into(),
    alignment: AlignmentExport::Linear, // or AlignmentExport::Geometry
    ..IfcOptions::default()
};
```

* `Geometry`: the horizontal and vertical layouts as business logic and as curves, the stationing
  and superelevation referents. Elements keep absolute placements.
* `Linear`: as `Geometry`, and every element whose site placement names an exported alignment
  with a station **and** an offset is placed by an `IfcLinearPlacement`. Elements without a
  station, with a station off the alignment, or with an alignment that was not exported keep the
  absolute placement (each case is a note).

Sources, besides those of "What was checked, and where": the IFC4.3.x-development documentation
at commit `c6928569550b24373bf4c6b4beb14aa303e2b468` (14 November 2026; paths below are under
`docs/`), the IFC4X3_ADD2 schema and IFC4X3 property templates of **IfcOpenShell 0.9.0.post1**, its
`ifcopenshell.api.alignment`, the buildingSMART sample `linear-placement-of-signal.ifc`
(IFC4.3.x-sample-models, commit `ef22440ec501551ccc9689309560bbc7e26aa9d7`,
`models/alignment-geometries-and-linear-positioning/`), and the alignment rules (features `ALB`,
`ALS`) of `ifc-gherkin-rules` at commit `893f8275049b8960c3ce0c7379b4b57aefeaa8f9`, which are the
rules the buildingSMART validation service runs.

#### What is written, and the rule each part follows

| Written | Rule, and where it is stated |
| --- | --- |
| `IfcAlignment` aggregated by the project, in no spatial element; referenced by the road with `IfcRelReferencedInSpatialStructure` | `schemas/core/IfcProductExtension/Entities/IfcAlignment.md` ("Alignment Aggregation To Project"); `templates/Object Connectivity/Alignment Spatial Reference` |
| `IfcAlignment.ObjectPlacement` an `IfcLocalPlacement` at the file's origin | rule ALB030 |
| One `IfcRelNests` from the alignment to `IfcAlignmentHorizontal` and, with a profile, `IfcAlignmentVertical`, in that order | `templates/Object Composition/Nesting/Alignment Layouts/` (both pages); rules ALB003, ALB031 |
| One `IfcRelNests` from each layout to its `IfcAlignmentSegment`s, one per element, `DesignParameters` an `IfcAlignmentHorizontalSegment` or `IfcAlignmentVerticalSegment`, and a **last segment of length 0** | the same pages ("A zero-length segment shall be added, at the end of the list of segments"); rules ALB002, ALB015 |
| Each `IfcAlignmentSegment` has `Representation` 'Axis' / 'Segment' holding its `IfcCurveSegment` (the one the curve also holds), and `ObjectPlacement` the alignment's placement (as `ifcopenshell.api.alignment` and the sample do; the template names only the representation) | `templates/Product Shape/Product Geometric Representation/Alignment Geometry/Alignment Geometry - Segments`; rules ALS004, ALB023 |
| Horizontal only: one representation 'Axis' / 'Curve2D' of an `IfcCompositeCurve`. Horizontal and vertical: 'Axis' / 'Curve3D' of an `IfcGradientCurve` whose `BaseCurve` is the `IfcCompositeCurve`, and 'FootPrint' / 'Curve2D' of that composite curve | `.../Alignment Geometry - Horizontal`, `... - Horizontal and Vertical`; rules ALS005, ALB021 |
| Sub-contexts 'Axis' and 'FootPrint' of the 'Model' context, `MODEL_VIEW` | `schemas/resource/IfcRepresentationResource/Entities/IfcGeometricRepresentationSubContext.md` ("The applicable values for the inherited ContextIdentifier attribute shall be identical to the RepresentationIdentifier") |
| Every curve's `Segments` are `IfcCurveSegment`s, as many as the layout has segments, **the last of length 0 and `DISCONTINUOUS`, no other `DISCONTINUOUS`** | `schemas/resource/IfcGeometryResource/Entities/IfcCompositeCurve.md` ("Segments must be a LIST[2:?] where the last segment is a zero-length segment", `CurveContinuous`); rules ALS015, ALB022 |
| `Transition` between segments worked out from the joint: `CONTSAMEGRADIENTSAMECURVATURE`, `CONTSAMEGRADIENT`, `CONTINUOUS`, or `DISCONTINUOUS` for a gap over 1 mm | `schemas/resource/IfcGeometryResource/Types/IfcTransitionCode.md` |
| `SegmentStart` and `SegmentLength` typed `IfcLengthMeasure` | `IfcCurveSegment.md` (informal proposition 1); rule ALS012 |
| Start station referent first, one referent per station equation, `Pset_Stationing` on each | `schemas/core/IfcProductExtension/Entities/IfcReferent.md` ("the first nested referent is the starting station"), `Types/IfcReferentTypeEnum.md` (`STATION`: "establishing the station origin or recording a station equation"), `schemas/shared/IfcSharedInfrastructureElements/PropertySets/Pset_Stationing.md` and the IFC4X3 template (`Station`, `IncomingStation`, `HasIncreasingStation`, all as in the template's value types); rule ALB010 |
| One `IfcReferent` `SUPERELEVATIONEVENT` per stated cross-fall and side, `Pset_Superelevation` | `Types/IfcReferentTypeEnum.md`, `schemas/domain/IfcRoadDomain/PropertySets/Pset_Superelevation.md` and its template (`Side`, `Superelevation`, `TransitionSuperelevation`, with `PEnum_SideType` and `PEnum_TransitionSuperelevationType`) |
| `IfcRelPositions` from the alignment to what is placed on it, as before | `schemas/core/IfcProductExtension/Entities/IfcRelPositions.md` ("does not affect the geometric representation of a product nor its geometrical placement") |

#### The curve segments

Everything is in millimetres and radians; the curve parameter is the length along the
curve in millimetres from its start (the distance along).

* **Line**: `IfcLine` through (0, 0) along +X, `SegmentStart` 0, `SegmentLength` the length;
  `IfcAxis2Placement2D` at the element's start point with its X axis along the start heading
  (`templates/Partial Templates/Geometry/Curve Segment Geometry/Linear Segment`).
* **Arc**: `IfcCircle` of the radius at the origin, `SegmentStart` 0, `SegmentLength` **negative
  for a right-hand arc** ("The sign of this value defines the sense agreement", `IfcCurveSegment.md`);
  `.../Arc Segment`.
* **Spiral**: `IfcClothoid`. `IfcClothoid.md` gives the curvature `kappa = A s / |A|^3` at the
  arc length `s` from the inflection point, and `Clothoid Transition Segment` the constant
  `A = sqrt(L R)`. For a spiral from curvature k0 to k1 over L, A = sign(k1 - k0) sqrt(L / |k1 - k0|)
  and the segment starts at s0 = k0 L / (k1 - k0) on the parent: 0 for a spiral from a tangent,
  -L for a spiral back to it (as in the buildingSMART sample: `A = 200`, start 0 and `A = -200`,
  start -40 for a left spiral in and a left spiral out of R = 1000 m, L = 40 m, and the same
  with the signs the other way for a right-hand curve). A compound spiral (both ends curved)
  has a start that is neither 0 nor -L. A spiral that changes hand is still refused.
* **Vertical**: in (distance along, level) coordinates, `IfcAxis2Placement2D` at the start
  point with its X axis along the start tangent. A straight is an `IfcLine`; a parabola an
  `IfcPolynomialCurve` with `CoefficientsX` (0, 1) and `CoefficientsY` (0, g0, c), c = (g1 - g0) /
  (2 H) (`Parabolic Transition Segment`; the documented template populates only the quadratic term,
  which is exact only when the start gradient is 0, so the linear term carries the start
  gradient as IfcOpenShell's mapping does); a circular curve an `IfcCircle` placed so that
  its start point is the origin, with `SegmentStart` R times the start angle and `SegmentLength`
  R times the angle swept, negative for a crest (as in the sample, `Arc Segment`). **`SegmentLength`
  of a vertical segment is the length along the curve, not the horizontal length**: IfcOpenShell
  reads it so (a 200 m parabola given its horizontal length evaluates 0.7 mm low after the
  curve), and the business logic carries the horizontal length (`HorizontalLength`).
* **Business logic**, `IfcAlignmentHorizontalSegment`: start point, `StartDirection` in
  (-pi, pi] (`IfcAlignmentHorizontalSegment.md`: "Values larger then a full circle shall not be
  used"), start and end radius (0 is infinite, positive turns left, negative right), length,
  `LINE`, `CIRCULARARC` or `CLOTHOID`. `IfcAlignmentVerticalSegment`: `StartDistAlong` (distance
  along the horizontal alignment from its start, so a station minus the start station),
  `HorizontalLength`, `StartHeight`, start and end gradient (ratios), `RadiusOfCurvature` and
  `CONSTANTGRADIENT`, `PARABOLICARC` or `CIRCULARARC` (`IfcAlignmentVerticalSegment.md`,
  `Types/IfcAlignmentVerticalSegmentTypeEnum.md`). The radius of a parabola is
  `HorizontalLength / (EndGradient - StartGradient)`, positive for a sag (rule ALB012, tolerance
  1e-3), worked out from the numbers as written; it is empty on a straight.
* **The profile** is cut at the ends of its vertical curves: straight, curve, straight, ...
  A symmetric parabola is one `PARABOLICARC`; an asymmetric one (`AsymmetricParabola`) is two,
  meeting at the PVI with the common gradient ("Unsymmetric parabolas can be created by defining two
  symmetric parabolas segments"); a `Circular` curve is a `CIRCULARARC`. A profile longer than the
  alignment is cut at its ends. A profile shorter than the alignment gives a gradient curve that
  covers only part of the alignment (a note says which part); an element outside it is placed on
  the 2D curve.
* **Station equations**: one `STATION` referent per equation, at its distance along, with `Station`
  the station ahead (what a referent's station is), `IncomingStation` the station arriving, and
  `HasIncreasingStation` false from a decreasing equation until an increasing one sets it true.
  The displayed station of every point can be recovered from these alone (a test does it, by
  its own reading of the referents, for gaps, overlaps and a decreasing run).

#### Superelevation: `IfcAlignmentCant` is not used

`IfcAlignmentCant` and `IfcAlignmentCantSegment` describe rail cant: `RailHeadDistance`, and the
height of the left and right rail relative to the vertical alignment (`IfcAlignmentCant.md`:
"Cant as a railway specificity", with the geometry on an `IfcSegmentedReferenceCurve`). A road's
stored cross-fall is a slope in percent on each side of the centreline at stated stations, with
no rail head distance, possibly stated for one side only and with gaps; mapping it to a cant with an
invented distance would state a value the document does not have. The documentation has a road
concept for it: `IfcReferent` `SUPERELEVATIONEVENT` ("a kind of event that specifies the
superelevation (cross slope) at a specific location along a road alignment, and the type of
transition from the previous location", placed on the axis curve) with `Pset_Superelevation`. That is
what is written, nested by the alignment in its own `IfcRelNests`, after the stationing referents.

* One referent per stated station and side. When both sides give the same value it is one referent
  with `Side` `BOTH`. A side with no stated value has none.
* `Superelevation` (an `IfcRatioMeasure`) has the sign of the documentation: "Positive superelevation
  is used for curves to the right and negative for curves to the left. This is consistent with
  IfcProfileDef coordinate system" (`properties/s/Superelevation.md`), whose X axis points to the
  left of the curve (`IfcOpenCrossProfileDef.md`). So the value is the slope in that frame: a left side
  rising away from the centre is positive, a right side rising away from the centre is negative, and
  a surface falling towards the right is positive on both sides. **That is this crate's reading of
  two pages; the sign has no worked example in the documentation.** The stored values, in the
  document's own convention (percent, positive rising away from the centreline), are written beside
  it in `AfroBIM_Superelevation` (`LeftCrossFallPct`, `RightCrossFallPct`, `StationM`, `Section`,
  `CriticalStation`).
* `TransitionSuperelevation` `LINEAR` is written on every event except the first of a stored
  section, which has nothing before it to transition from; between sections the document says
  nothing is known.
* The documentation proposes containing such an event in a lateral breakdown element of the road
  (a lane). This crate writes no `IfcRoadPart`, so the event is nested by the alignment only.

#### Linear placement

The documentation (`schemas/resource/IfcGeometricConstraintResource/Entities/IfcLinearPlacement.md`,
`IfcAxis2PlacementLinear.md`, `IfcPointByDistanceExpression.md`, and `templates/Product Shape/Product
Placement/Product Linear Placement`):
`IfcProduct.ObjectPlacement` an `IfcLinearPlacement`, `PlacementRelTo` the placement of the
alignment, `RelativePlacement` an `IfcAxis2PlacementLinear` whose `Location` is an
`IfcPointByDistanceExpression` (`DistanceAlong`, `OffsetLateral` positive **to the left**,
`OffsetVertical` perpendicular to the tangent in the vertical plane, `OffsetLongitudinal`, and the
`BasisCurve`). The same shape is in the buildingSMART sample.

* **Basis curve.** The `IfcGradientCurve` where the profile covers the station, otherwise the
  `IfcCompositeCurve` (whose level is 0, so the vertical offset is then the level).
* **Offsets.** The distance along is the station minus the start station (internal stations, so
  station equations do not enter). The offsets are the components of the element's position along
  the three axes of the curve at that point (tangent, left, normal), so the placement reaches the
  position exactly. The recorded station and offset are used when they reproduce the position to
  a micrometre; otherwise the nearest point of the alignment decides (a test has an element that
  says 2+500 and stands at 2+560). The lateral offset is minus the recorded offset (the document's
  offsets are positive to the right). On a gradient the vertical offset is measured along the
  normal and a longitudinal offset of (level difference x sin(gradient)) restores the point;
  both are zero for an element at the profile's level.
* **Orientation, and why the geometry is turned.** Axis and RefDirection are **left out**. The
  documentation says they are relative to the curve ("maintaining the relationship to the tangent
  of the curve"); IfcOpenShell 0.9.0.post1 reads them as absolute directions (an `Axis` (0, 0, 1)
  and `RefDirection` (1, 0, 0) gave a world X axis on a curve at 30 degrees). With both left out the
  documentation and IfcOpenShell agree: X along the tangent (in 3D on a gradient curve), Y to the
  left, Z perpendicular to the tangent in the vertical plane, upwards. The element's own frame
  (its rotation about +Z, which for a sign is the tangent minus 90 degrees) is then not the
  placement's, so **the element's geometry is written in the placement's frame**: the turn Q from the
  curve frame to the element's old frame is applied to the points of its tessellations, to the
  position and axes of its extruded solids, and, for what is aggregated into it (the parts of a
  sign assembly, a gantry, a lighting column, a barrier run, a pavement), to the placement of each part,
  which is written as an `IfcLocalPlacement` relative to the whole with Q composed with its own pose.
  Everything lands where it did; nothing else about the parts changes.
* **`CartesianPosition`**: `IfcLinearPlacement.md` documents it as the "optional fallback for the
  `RelativePlacement` attribute, which may be used by importing applications that do not support
  linear placement", and it is written on every linear placement. It stands for the
  `RelativePlacement`: it is the frame of the curve at the point (origin the element's position,
  X the tangent, Z the normal), which is the frame the geometry is written in, so a tool that
  takes the fallback instead of evaluating the curve puts the element in the same place (a test does
  exactly that with IfcOpenShell). It is **not** the old absolute frame.
* **Referents** (stationing and superelevation) are placed the same way, with no offsets, on the
  2D curve for stations and on the gradient curve where it covers a superelevation event.
* **What stays absolute**: elements without an alignment, a station or an offset in the site
  placement, an alignment that was not exported (it could not be read), and a station off the
  alignment. `IfcRelPositions` is written as before in `Geometry` and `Linear`.

#### The importer

`import_ifc` resolves `IfcLinearPlacement` (`src/import/reader/linear.rs`): the basis curve an
`IfcCompositeCurve` of `IfcLine`, `IfcCircle` and `IfcClothoid` segments, or an `IfcGradientCurve`
on one whose segments are `IfcLine`, `IfcPolynomialCurve` (a polynomial in the distance along
from the segment's start) and `IfcCircle`, a distance along typed `IfcLengthMeasure`, and no `Axis`
or `RefDirection`; the frame is the one described above. Horizontal segments are evaluated with
`afrobim_alignment::AlignmentElement` (Gauss-Legendre quadrature for the clothoid). When a placement
cannot be evaluated (another curve, `Axis` or `RefDirection` given, a parameter value, a distance
beyond the curve) its `CartesianPosition` is used if there is one (`report.linear_placements_fallback`
and a warning), otherwise the product is `PlacementUnresolved` as before. `report.linear_placements_resolved`
counts the placements evaluated.

#### Verification (IfcOpenShell 0.9.0.post1)

`tests/check_alignment_ifc.py`, run by `tests/ifcopenshell_linear.rs`, on two documents: the
model of `tests/common/linear.rs` (two alignments: LN01 of 11 elements with spirals both ways, a
compound spiral, a profile of every vertical curve kind, a gap, an overlap and a decreasing run in
the stationing, eight superelevation events; LN02 horizontal only with a gap and an overlap; seven elements
placed linearly, three kept absolute) and the road-module model of `tests/common/roads.rs` with
every root element given a station (18 linear placements: markings, drain, culverts, gantries,
lighting columns, barrier run, pavement, corridor).

* Schema and EXPRESS rules: **0 issues** on all four files (the two models, each exported with `Linear`
  and with `Geometry`).
* The service's alignment rules (ALB003, ALB010, ALB012, ALB015, ALB021 to ALB023, ALB030, ALB031,
  ALS004, ALS005, ALS012, ALS015): re-implemented in the script from the text of the
  feature files, **not run by the service** (see below), all hold. ALS016 and ALS017
  (continuity, "a warning"): IfcOpenShell's kernel puts the end of each segment within
  **0.33 micrometre** and 1.3e-10 in tangent of the start of the next; the contexts' precision is 1e-5 mm, so a
  checker evaluating clothoids as coarsely as IfcOpenShell can warn.
* Curve evaluation by the kernel (`function_item_evaluator` on the written `IfcCompositeCurve` and
  `IfcGradientCurve`) at 162 plan points (every element at ten fractions of its length, so
  every spiral at several points) and 148 vertical points (every 5 m and in the middle and
  at both ends of every curve) against `afrobim-alignment`'s own coordinates and levels: plan **0.33
  micrometre at worst** (line 0.21, spiral 0.31, arc 0.33), direction **7.5e-13 rad**, level
  **1.6e-5 mm**. The same curves built by IfcOpenShell's own mapping of each `IfcAlignmentSegment` to a
  curve segment differ from the written ones by 0.33 micrometre and 1.6e-5 mm.
* Displayed stations, 66 of them, taken at and either side of every equation: all recovered from the
  referents alone by the script's reading. `ifcopenshell.api.alignment.distance_along_from_station`
  agrees on every station of the alignment without a decreasing run, except that
  it extrapolates a station that is only in an earlier region of an overlap beyond the end of the
  alignment (it does not know where the alignment ends), and returns `None` for stations before a
  decreasing run when the stationing has one (its search takes the last referent whose test holds).
  The data is the same; that function's limit is recorded in `figures["displayed_stations"]`.
* Linear placements: `ifcopenshell.util.placement` (the kernel evaluates `IfcAxis2PlacementLinear`)
  resolves the world position of every product in the `Linear` file; against the `Geometry`
  file the worst difference is **6.9e-6 mm** on the first model and **9.7e-7 mm** on the road
  model, the world rotations of the 15 and 37 products that keep their frame **7.6e-11**
  and **2.9e-13**; the bounding box of every shaped product (41 and 46) by the geometry
  iterator **6.8e-6 mm** and **1.3e-6 mm**, equal triangle counts; and the same boxes after every linear
  placement is replaced by a local placement at its `CartesianPosition` (the fallback)
  **4.8e-7 mm** and **1.0e-6 mm**. A separate rebuilding of each placement from the kernel's
  curve evaluation and this script's own reading of the three offsets differs by the same
  figures. "World placement" of a linearly placed product is the frame of the curve (the
  geometry is turned to compensate), so the rotations are compared where the frame is kept
  and the geometry where it is not.
* Round trip through this crate's importer (`tests/linear.rs`, no toolkit): export with `Linear`,
  import, and the world bounds of every product with geometry against the same export with
  `Geometry`: **9.8e-7 mm** on the first model, **1.3e-6 mm** on the road model, every placement evaluated and none by
  the fallback; with the clothoids made unreadable (all placements by the fallback), 4.8e-7 mm.
* The importer on a file this crate did not write: the buildingSMART sample
  `linear-placement-of-signal.ifc` (25 linear placements on lines, clothoids, circles and a
  gradient curve of circular vertical curves; `tests/import_linear.rs`, which needs the file
  and prints why it is skipped when it has none): all 25 evaluated, none by the fallback, and
  the world bounding boxes of its 25 shaped products differ from IfcOpenShell's by
  **3.6e-3 mm** at worst (the sample's coordinates are of the order of 4.5e6 m).

#### Limits

* No `IfcAlignmentCant`, `IfcSegmentedReferenceCurve` or `IfcOffsetCurveByDistances`, no child
  alignments sharing one horizontal layout, no Helmert, Bloss, cosine, sine, cubic or Viennese
  curves (the alignment crate has only straights, arcs and clothoids), no vertical `CLOTHOID`.
* The sign of `Pset_Superelevation.Superelevation` is a reading, not a documented example.
* `SegmentLength` of a vertical segment as the curve length, and a polynomial parent with a linear
  term, follow IfcOpenShell's evaluation; the documentation's `IfcCurveSegment` page does not
  define the parametrisation of every parent curve ("until implementer agreements have resulted in a
  defined parametric space ... SegmentStart and SegmentLength shall be of type IfcLengthMeasure"). A
  tool that reads them differently will place the vertical curves differently.
* Axis and RefDirection are never written on an `IfcAxis2PlacementLinear`, so an element's
  rotation relative to the road is in its geometry and in the frames of its parts, not in the
  placement.
* An element placed linearly has the frame of the curve as its placement frame; a tool that reads
  the placement of a product (a sign assembly, a post) sees a frame tilted by the gradient on a
  gradient curve.
* Referents are not nested by lanes or `IfcRoadPart`s.
* An element with a station and offset that does not stand where the station says is placed where
  it stands (the alignment's nearest point), with the station it has; nothing records that they
  disagreed.
* The buildingSMART validation service **was not run**: it asks for a signed-in account before it
  takes a file (its page says "Sign in to start validating"). The rules above were read and
  re-implemented, not executed by its harness (`behave`), which was not installed here.
* No file has been opened in a commercial BIM viewer, or in any viewer.

## The road modules

`export_ifc_model` writes the elements of nine more categories. Every one is modelled by its
module in site coordinates (X east, Y north, Z up) and drawn by the 3D view from the `Mesh3` solids
the module generates, in the frame of the element's `SitePlacement` (a corridor in the frame of its
built surfaces; `afrobim-app/src/view3d.rs`, `frame_of`). The exporter writes the same solids, as
`IfcTriangulatedFaceSet`s (`Closed` when every edge of the mesh is matched, as a closed solid is;
open for the corridor and the ground), at the same placement, so the geometry sits where the 3D
view draws it. Coincident vertices are merged on the way out (a corridor is built from separate
quads), which changes no triangle.

### What was checked, and where

The IFC 4.3 documentation site (`ifc43-docs.standards.buildingsmart.org`, and
`standards.buildingsmart.org`, which it redirects to) answered every request from the build
environment with a Cloudflare challenge (HTTP 403), so its pages could not be fetched. The entities
and enumerators were checked instead against two things, fetched on 14 November 2026:

1. The markdown source of the documentation, `buildingSMART/IFC4.3.x-development` on GitHub
   (`docs/schemas/...`), master at commit `eb77fff73a258dc3220b43d985284c2526b6f91f` (13 November
   2026). Each page is named in the table below. That repository is the development line, so
   nothing was taken from it alone:
2. The **IFC4X3_ADD2** EXPRESS schema and the property-set templates that IfcOpenShell 0.9.0.post1
   ships (`ifcopenshell.schema_by_name("IFC4X3_ADD2")`, `ifcopenshell.util.pset.get_template("IFC4X3")`).
   Every enumerator, every `Pset_` / `Qto_` name, the entities each set applies to, every
   property name and its value type were read from there, and the test script
   `tests/check_roads_ifc.py` checks every standard set of the exported file against the same
   templates on each run (name, applicable class and predefined type, property names, value types).

No entity name, enumerator or property-set name here is from memory. Where the documentation had
no specific entity, the table says so.

### Mapping

| AfroBIM category | IFC 4.3 entity | PredefinedType | Pages checked (`docs/schemas/...`) |
| --- | --- | --- | --- |
| `marking` (road marking), carried by a pavement | `IfcSurfaceFeature` adhering to the `IfcPavement` by `IfcRelAdheresToElement`; contained nowhere | `LINEMARKING` ("2D lines painted on pavement surfaces to form boundaries, centrelines and edge markings") | `domain/IfcStructuralElementsDomain/Entities/IfcSurfaceFeature.md`, `.../Types/IfcSurfaceFeatureTypeEnum.md`, `core/IfcProductExtension/Entities/IfcRelAdheresToElement.md` |
| `marking`, no pavement carries it | **No specific entity fits**: `IfcBuildingElementProxy`, contained in the road, `ObjectType` `marking` | `USERDEFINED` | `shared/IfcSharedBldgElements/Entities/IfcBuildingElementProxy.md`; see "Markings" |
| `marking_ext` (second marking set), carried by a pavement | `IfcSurfaceFeature` adhering to the `IfcPavement` by `IfcRelAdheresToElement`; contained nowhere | by the kind of catalogue entry, see "Second marking set" | the three pages of the first row |
| `marking_ext`, no pavement carries it | **No specific entity fits**: `IfcBuildingElementProxy`, contained in the road, `ObjectType` `marking` | `USERDEFINED` | see "Markings" |
| `drain` (side drain, an open channel) | `IfcPipeSegment` | `GUTTER` ("a continuous open-channel segment of pipe") | `domain/IfcHvacDomain/Entities/IfcPipeSegment.md`, `.../Types/IfcPipeSegmentTypeEnum.md` |
| `culvert` (pipe culvert), one per barrel | `IfcPipeSegment`; several barrels are one `IfcGroup` | `CULVERT` ("a covered channel or large pipe that forms a watercourse below ground level, usually under a road or railway") | the same two pages; `core/IfcKernel/Entities/IfcGroup.md` |
| `gantry` | `IfcElementAssembly` aggregating legs, boom, foundations and the hung signs | `SIGNALASSEMBLY` ("an assembly to physically aggregate together one or more signal instances (and also sign instances) including any supporting structural elements such as a simple pole or a rigid frame gantry") | `core/IfcProductExtension/Types/IfcElementAssemblyTypeEnum.md` |
| gantry leg | `IfcColumn` | `COLUMN` ("a usually vertical member that may be load bearing") | `shared/IfcSharedBldgElements/Entities/IfcColumn.md`, `.../Types/IfcColumnTypeEnum.md` |
| gantry boom | `IfcBeam` | `BEAM` ("a standard beam usually used horizontally") | `shared/IfcSharedBldgElements/Types/IfcBeamTypeEnum.md` |
| gantry foundation | `IfcFooting` | `PAD_FOOTING` (as for sign supports) | see "Entity choices" above |
| `lighting`, one per column | `IfcElementAssembly` | `MAST` ("a vertical structure for the support or mounting of other equipment such as lights") | `core/IfcProductExtension/Types/IfcElementAssemblyTypeEnum.md` |
| lighting pole | `IfcMember` | `POST` ("a linear (usually vertical) member used to support something") | `shared/IfcSharedBldgElements/Types/IfcMemberTypeEnum.md` |
| lighting bracket | `IfcMember` | `NOTDEFINED` ("undefined linear element"; no enumerator names a bracket arm) | the same page |
| luminaire | `IfcLightFixture` | `NOTDEFINED` (`POINTSOURCE` and `DIRECTIONSOURCE` describe the light emission, which the module does not model) | `domain/IfcElectricalDomain/Entities/IfcLightFixture.md`, `.../Types/IfcLightFixtureTypeEnum.md` |
| lighting installation of several columns | `IfcGroup` of the column assemblies | none | `core/IfcKernel/Entities/IfcGroup.md` |
| `restraint` (barrier run) | `IfcRailing` | `GUARDRAIL` ("... or to provide restraint to an errant road vehicle, installed on the central reserve of or alongside a road") | `shared/IfcSharedBldgElements/Entities/IfcRailing.md`, `.../Types/IfcRailingTypeEnum.md`, `domain/IfcRoadDomain/PropertySets/Pset_RoadGuardElement.md` |
| terminal, transition of a run | `IfcRailing`, aggregated by the run, with `Pset_RoadGuardElement` `IsTerminal` / `IsTransition` | `GUARDRAIL` | the same pages |
| `pavement` | `IfcPavement` | `FLEXIBLE` when the top layer is hot mix asphalt or surface dressing ("pavement with a bituminous surfacing"), else `NOTDEFINED` | `shared/IfcSharedInfrastructureElements/Entities/IfcPavement.md`, `.../Types/IfcPavementTypeEnum.md` |
| pavement layer (slab) | `IfcCourse`, aggregated by the pavement | `PAVEMENT` ("a layer within a pavement structure that forms a paved area or road") | `.../Entities/IfcCourse.md`, `.../Types/IfcCourseTypeEnum.md` |
| `corridor` | **No specific entity fits**: `IfcBuildingElementProxy`, `ObjectType` `corridor`, three open surfaces (finished surface, cut slopes, fill slopes) | `USERDEFINED` | see "Corridors" |
| `surface` (ground TIN) | `IfcGeographicElement`, contained in the site | `TERRAIN` | `core/IfcProductExtension/Entities/IfcGeographicElement.md`, `.../Types/IfcGeographicElementTypeEnum.md` |

### Second marking set

The elements of category `marking_ext` (stop and give way lines, the give way triangle, lane and
deflection arrows, hatching, chevrons and island markings, yellow lines, zig-zag lines, pedestrian
crossings, worded-marking blocks, road studs, the UK and Uganda types) are written as the first set
is: an `IfcSurfaceFeature` adhering to the pavement that carries it, otherwise the proxy. A
pavement carries a marking when it is on the same alignment, covers the marking's stations, and
covers its offset (for a line across the road, both its offsets; for a point item, its centre). The
`IfcSurfaceFeatureTypeEnum` enumerators are, in the IFC4X3_ADD2 schema of IfcOpenShell 0.9.0.post1:
`DEFECT`, `HATCHMARKING`, `LINEMARKING`, `MARK`, `NONSKIDSURFACING`, `PAVEMENTSURFACEMARKING`,
`RUMBLESTRIP`, `SYMBOLMARKING`, `TAG`, `TRANSVERSERUMBLESTRIP`, `TREATMENT`, `USERDEFINED`,
`NOTDEFINED`. Their documentation text was read in the IfcOpenShell package
(`express/DocEnumeration.csv`) and in `docs/schemas/domain/IfcStructuralElementsDomain/Types/IfcSurfaceFeatureTypeEnum.md`
of `buildingSMART/IFC4.3.x-development` (fetched on 15 November 2026; the two agree). The choice is
made by `ext_feature_type`, from the kind of the catalogue entry:

| Catalogue kind (entries) | PredefinedType | The documentation the choice rests on |
| --- | --- | --- |
| Lines (1017, 1018.1, M01 to M04, M11, M12, M21 to M23), transverse lines (1001, 1002.1, 1003.1, 1003A, M31, M32), zig-zag line (M05), crossing of two broken lines (M34), parking bay (M71, catalogued only) | `LINEMARKING` | "2D lines painted on pavement surfaces to form boundaries, centrelines and edge markings" |
| Triangle (1023A, M61), arrows (1038, 1014, M41 to M45), worded markings (1022, 1024, M62, M63; M63 is catalogued only) | `SYMBOLMARKING` | "Surface markings that convey information in the form of symbols and shapes such as arrows, text or pictorial symbols" |
| Hatching, chevrons and island markings (1040, 1041, 1042, M51 to M53; M51 and M52 are catalogued only) | `HATCHMARKING` | "surface markings defined by enclosed 2d shape with defined hatch fillings". Chevrons and the island marking are read as hatch fillings of an enclosed area, which is an interpretation: no enumerator names chevrons |
| Crossing of stripes (M33) and the hump marking (M35, catalogued only) | `PAVEMENTSURFACEMARKING` | "Painted or chemical lines or symbols on the surface of pavements"; the stripes are neither a boundary line nor a symbol |
| Road studs (`STUD`) | `USERDEFINED`, `ObjectType` `ROAD_STUD` | No enumerator names a raised stud fixed to the pavement. Every enumerator of the schema whose name contains `STUD` was listed: `IfcMemberTypeEnum.STUD`, `IfcReinforcingBarRoleEnum.STUD`, `IfcReinforcingBarTypeEnum.STUD` and `IfcMechanicalFastenerTypeEnum.STUDSHEARCONNECTOR`, all structural, none for a road stud. `RUMBLESTRIP` and `TRANSVERSERUMBLESTRIP` are raised textured strips, not studs, and a transverse rumble strip is not a marking at all. The schema rule `HasObjectType` (an `ObjectType` for `USERDEFINED`) is met. This is a judgement, not a documented mapping |

Nothing is mapped to `RUMBLESTRIP`, `TRANSVERSERUMBLESTRIP`, `MARK` ("a point, line, cross, or other
mark, applied for example for easier adjustment of elements during assembly", a manufacturing
mark), `TAG`, `TREATMENT` or `NONSKIDSURFACING`.

Property sets. `Pset_RoadMarkingCommon` applies, in the IFC4X3 template, to
`IfcSurfaceFeature/HATCHMARKING`, `/LINEMARKING`, `/PAVEMENTSURFACEMARKING` and `/SYMBOLMARKING`;
it is written (`DiagramNumber`, `MaterialColour`, `MaterialType` when a material is entered) for
every adhering marking of those types, and not for a stud or a proxy. `Pset_MarkingLinesCommon`
applies to `LINEMARKING` only (template: `DashedLine` IfcBoolean, `DashedLinePattern` IfcLabel,
`NominalWidth` IfcNonNegativeLengthMeasure, read from the IFC4X3 templates bundled with IfcOpenShell
0.9.0.post1) and is written for an adhering `LINEMARKING` only where the dash pattern holds for the
whole element: a longitudinal line, or a double line whose two lines have the same pattern (with the
pattern of the urban or rural setting the element states), and a stop or give way line. The pattern
is the continuous line (`DashedLine` false, pattern `continuous`) or the one mark and gap module
(`DashedLine` true, pattern as `3000 mm mark, 6000 mm gap`, as the first set writes it), and
`NominalWidth` is every line plus the gap between lines (rows and the gap between rows for a transverse
line), in the file's unit. It is **left out**, with a note in the export notes, for the zig-zag line
(its marks follow the zig-zag), a crossing with side lines (the mark lengths are chosen within a
range), a double line of two different patterns, a line whose width or setting is not set, and a
type with no layout. `Qto_SurfaceFeatureBaseQuantities` (`Area` painted, `Length` painted)
is written for an adhering marking, with `Area` only where the module reports a painted area (not
for a placeholder block, not for studs) and `Length` only when it is above zero. The element's own
`AfroBIM_RoadMarkingExt` carries the type identifier, basis, diagram, TSRGD reference (UK types),
colour, material, speed and the caller's speed note, any UK fallback reason, the source citation and
the stations; `AfroBIM_Quantities` carries overall and painted length, painted area, the number of
marks, part marks and, for a placeholder, the area of its block.

**Placeholder blocks.** A worded marking, the lane and deflection arrows other than M41, and the
Uganda arrows M42 to M45 are drawn as the bounding block of the marking, because the source does not
dimension the outline. The export writes the block, sets `PlaceholderBlock` true and `ShapeStatus` to
"Placeholder block: the bounding block of the marking, not its true shape" with the reason from the
catalogue, and ends the `Description` with "(placeholder block, not the true shape)". An entry that
is catalogue only (M35, M51, M52, M63, M71) has no geometry and takes the type of its family; the element is still written, without
a shape, and a note says so. Every other marking has `PlaceholderBlock` false.

Validated with IfcOpenShell 0.9.0.post1 by `tests/ifcopenshell_ext_markings.rs` and
`tests/check_ext_markings_ifc.py` on a document with a pavement and 20 markings (every kind in the
table above, UK and Uganda) plus one the pavement does not carry: **0 schema and EXPRESS rule
issues**, the standard sets agree with the IFC4X3 templates (88 properties), the geometry iterator
built a shape for each of the 23 products with a representation, and each marking has the class,
PredefinedType, `ObjectType`, adhesion and placeholder properties the test states. The same
document without its pavement (every marking a proxy) also has 0 issues. The limits of the
mapping are those of the first set: a marking is attached to a pavement only when one of the
document covers it, and the offsets tested are the marking's reference offsets, not its painted
extent.

Choices the documentation settled, and what it ruled out:

- **Markings.** `IfcSurfaceFeature` is "a modification at (onto, or into) of the surface of an
  element". The EXPRESS schema, which the IfcOpenShell validator enforces, makes its inverse
  `AdheresToElement` mandatory (one or more `IfcRelAdheresToElement`) and has a WHERE rule
  that `ContainedInStructure` is empty ("Surface features shall have no spatial containment
  relationship", `IfcSurfaceFeature.md`; the same words are in `IfcRelAdheresToElement.md`).
  A first version that contained an unattached marking in the road failed both rules. So a marking is an
  `IfcSurfaceFeature` only when a pavement of the document carries it: same alignment, the
  marking's stations inside the pavement's, its offset inside the offsets the pavement covers
  (first strip's left edge to last strip's right edge). Otherwise it is an
  `IfcBuildingElementProxy`, contained in the road, and a note says so; it then has no
  `Pset_RoadMarkingCommon`, `Pset_MarkingLinesCommon` or `Qto_SurfaceFeatureBaseQuantities`, which
  apply to `IfcSurfaceFeature` only.
- **`IfcCivilElement` is not used.** `IfcCivilElement.md`: "IFC4.3.0.0 DEPRECATION This entity is
  deprecated. Usage of a generic element for civil engineering works is no longer desirable".
  `IfcBuildingElementProxy` is the documented generic.
- **Corridors.** `IfcEarthworksCut` is an `IfcFeatureElementSubtraction`: "the resulting void",
  related to the element it voids by `IfcRelVoidsElement` and placed relative to it
  (`IfcFeatureElementSubtraction.md`); a corridor's cut slopes are a surface, and there is no
  element to void. `IfcEarthworksFill` (`SLOPEFILL`, "side slope (batter) fill") names a body, and
  the module's fill slopes are a surface. The corridor is therefore the generic proxy, with the
  surfaces as they are built.
- **Terrain is contained in the site.** `IfcGeographicElement.md`, "Spatial Containment":
  `IfcSite` ("assignment to a site"). The road is a facility; the ground is not part of it.
  Everything else is contained in the existing `IfcRoad`; there are no `IfcRoadPart`s.
- **Lighting pole is `POST`, not `STANDCOLUMN`.** `IfcColumnTypeEnum.STANDCOLUMN` is "a column
  transmitting vertical loads from a superstructure to an arch below it". A pole supports a
  luminaire, which is what `IfcMemberTypeEnum.POST` says.
- **The gantry is `SIGNALASSEMBLY`, not `RIGID_FRAME`.** `RIGID_FRAME` ("with moment-resisting
  joints, such as gantry") would claim a structural form the module does not model ("schematic
  envelopes of entered dimensions"); `SIGNALASSEMBLY` names a rigid frame gantry too and is the
  documented place for sign instances, which are aggregated as parts, as `IfcSign.md` says.
- **Pavement type.** `FLEXIBLE` is claimed only for a top layer that is bituminous in the model's
  own material list (`HotMixAsphalt`, `SurfaceDressing`). The module has no concrete material, so
  `RIGID` is never written; anything else is `NOTDEFINED`.

### Containment and relationships

- Contained in the `IfcRoad` (one `IfcRelContainedInSpatialStructure`): every root product: the
  drain, each culvert barrel, each gantry assembly, each lighting-column assembly, each barrier
  run, each pavement, each corridor, and the markings no pavement carries. Contained in the
  `IfcSite`: the ground surfaces. Not contained: parts of assemblies and of runs and pavements,
  and markings that adhere (bSI rule SPS007 and the entity documents: an element is contained or
  an aggregated part, never both).
- `IfcRelAggregates`: gantry to its legs, boom, foundations and signs; column assembly to its
  pole, brackets and luminaires; run to its terminals and transitions; pavement to its courses.
  Parts are placed relative to their whole (bSI rule OJP001).
- `IfcRelAdheresToElement`: pavement to the markings it carries.
- `IfcGroup` and `IfcRelAssignsToGroup`: one element that is several products: a lighting
  installation of several columns, a culvert of several barrels. The group has the element's
  GlobalId, name and properties and no position.
- `IfcRelPositions`: the alignment to every root whose site placement names it (and to every
  lighting column, which names the alignment its layout was computed on).
- **Placement is absolute by default.** Each root has an `IfcLocalPlacement` relative to the road at its
  `SitePlacement`; station and offset are properties (`AfroBIM_SitePlacement`: `Alignment`,
  `StationM`, `OffsetM`; and the range in the element's own set). With
  `IfcOptions::alignment = AlignmentExport::Linear` a root whose site placement names an exported
  alignment, a station and an offset is placed by an `IfcLinearPlacement` on the alignment's curve
  (see "Alignments, complete"); the properties are still written.

### Per category

All lengths are millimetres in the file, areas m², volumes m³, as in the rest of the crate. Every
element also has `AfroBIM_Properties` (its own properties), `AfroBIM_Derived` (what the module
derived, complete), `AfroBIM_Notes` (every note the module attached: severity and message, named
by the note's code) and `AfroBIM_SitePlacement`. Each set below named `AfroBIM_...` also carries
`Mark`, `Category` and `Layer`, the alignment, stations (a range as `StartStationM`,
`EndStationM`, a single station as `StationM`) and offset where the element has them, and
`DesignBasis` where there is one (the element property `design_basis` copied unchanged; for a
marking, when that is absent, the edition of the Traffic Signs Manual the module checks against,
`afrobim_markings::EDITION`). The other modules record no design basis in the element, so none is
written for them.

| Element | Standard sets (names, applicable entity and property names checked) | `AfroBIM_` sets |
| --- | --- | --- |
| Marking | `Pset_RoadMarkingCommon` (`DiagramNumber`, `MaterialColour`, `MaterialType` when the material is entered), `Pset_MarkingLinesCommon` (`DashedLine`, `DashedLinePattern`, `NominalWidth`, the overall width of a double line), `Qto_SurfaceFeatureBaseQuantities` (`Area` painted, `Length` painted) | `AfroBIM_RoadMarking` (line type, diagram, TSRGD reference, pattern, width, colour, speed limit, material, source citation), `AfroBIM_Quantities` (overall and painted length, painted area, marks, part marks) |
| Second-set marking | `Pset_RoadMarkingCommon` (`DiagramNumber`, `MaterialColour`, `MaterialType`), on every adhering marking but a stud; `Pset_MarkingLinesCommon` (`DashedLine`, `DashedLinePattern`, `NominalWidth`) on an adhering line marking with one dash pattern (see "Second marking set"); `Qto_SurfaceFeatureBaseQuantities` (`Area` where painted, `Length`) | `AfroBIM_RoadMarkingExt` (see "Second marking set"), `AfroBIM_Quantities` |
| Side drain | `Pset_PipeSegmentTypeGutter` (`IsCovered` false), `Pset_PipeSegmentOccurrence` (`Gradient`), `Qto_PipeSegmentBaseQuantities` (`Length` of the drain's own centreline) | `AfroBIM_SideDrain` (section, lining, gradient, Manning's n, placeholder flag), `AfroBIM_Quantities` (excavation, lining and areas) |
| Culvert barrel | `Pset_PipeSegmentTypeCommon` (`InnerDiameter`, `OuterDiameter`, `Length`), `Pset_PipeSegmentTypeCulvert` (`InternalWidth`, `ClearDepth`, both the diameter of a circular barrel), `Pset_PipeSegmentOccurrence` (`Gradient`, fall over plan length), `Qto_PipeSegmentBaseQuantities` (`Length` of one barrel); `IfcMaterial` when the material text is entered | `AfroBIM_PipeCulvert` (barrels, diameter, wall, inverts, skew, road level, material), `AfroBIM_Quantities` |
| Gantry | `Pset_ElementAssemblyCommon` (`Reference`); legs `Qto_ColumnBaseQuantities` (`Length`), boom `Qto_BeamBaseQuantities` (`Length`), foundations `Qto_FootingBaseQuantities` (`Length`, `Width`, `Height`, `GrossVolume`) | `AfroBIM_Gantry`, `AfroBIM_GantryMember` (envelope, offset, levels, mass per metre where entered), `AfroBIM_GantryMount` on each hung sign (lane or offset, bottom above the boom, traffic served), `AfroBIM_Quantities` (span or outreach, boom, signs, sign area, foundation concrete); steel mass only where every member has a mass per metre |
| Lighting column | `Pset_ElementAssemblyCommon` (`Reference`); pole `Pset_MemberCommon` (`IsExternal`); luminaire `Pset_LightFixtureTypeCommon` (`TotalWattage`, `Reference`) | `AfroBIM_LightingColumn` (side, station, offset, level, bearing, mounting height, outreach, tilt, lighting class), `AfroBIM_LightingPart` (luminaire: product, lumen output, power, photometric file, intensity class, CCT, CRI, LED, risk group, mass, windage), `AfroBIM_LightingInstallation` on the group (counts, connected load) |
| Barrier run | `Pset_RoadGuardElement` (`IsTerminal` and `IsTransition` false), `Pset_RailingCommon` (`IsExternal`), `Qto_RailingBaseQuantities` (`Length` along the barrier) | `AfroBIM_BarrierRun` (classes, system, post spacing, beam height, end treatments, set-back), `AfroBIM_Quantities`; `AfroBIM_BarrierEnd` on a terminal or transition |
| Terminal, transition | `Pset_RoadGuardElement` (`IsTerminal` or `IsTransition`, `TerminalType` = the performance class) | `AfroBIM_BarrierEnd` |
| Pavement | `Pset_PavementCommon` (`Reference`, `NominalLength`, `NominalWidth` of the carriageway, `NominalThickness` of its layers), `Qto_PavementBaseQuantities` (`Length`, `NetVolume` of all slabs) | `AfroBIM_Pavement`, `AfroBIM_Quantities` (carriageway area, total volume, slabs) |
| Course | `Pset_CourseCommon` (`NominalLength`, `NominalWidth`, `NominalThickness`), `Qto_CourseBaseQuantities` (`Length` of the strip's own centre line, `Width`, `Thickness`, `Volume` compacted); `IfcMaterial` named by the layer's material | `AfroBIM_PavementLayer`, `AfroBIM_Quantities` (plan area) |
| Corridor | none (the proxy has no standard set that fits) | `AfroBIM_Corridor` (alignment, ground surface, definition, regions, interval, stations), `AfroBIM_Quantities` (total cut, total fill, unmeasured length, sections, sections without areas: the module's earthworks by average end area) |
| Ground | none | `AfroBIM_Surface` (source) |

The quantities are the modules' own, computed from the definitions (`MarkingQuantities`,
`DrainQuantities`, `CulvertQuantities`, `GantryQuantities`, `RunQuantities`,
`measure_definition`, the corridor's `CorridorSummary`), never from the display solids. Where the
solid is an exact prism the two agree and the test script compares them: the painted area times
the display thickness against the kernel's volume of a marking (within 0.1 %), a gantry
foundation's `GrossVolume` against its mesh (1e-6), a pavement course's `Volume` against its slab
(7e-6 on the test document).

### Geometry notes

- **Display shapes stay display shapes.** The module solids are schematics, and are exported as
  such: lighting columns (a 200 mm display diameter, a square bracket, a luminaire block), the
  barrier (a post prism and a beam strip at the entered height and spacing), the 5 mm of a
  marking, the 50 mm skin of an unlined drain, gantry members as the boxes and cylinders of their
  entered envelopes. None of these dimensions is in a quantity or a standard set.
- **Vertical position is the module's.** Markings, lighting and restraint carry the profile
  levels; a pavement and a side drain follow the alignment's profile level (held at the end
  levels beyond the profile's ends, and at zero where the alignment has no profile); culverts sit at the entered inverts; the
  corridor and the ground carry elevations. Exported where the 3D view draws them, they therefore
  do not all stand at the profile.
- **Terminals and transitions have no geometry**: the module draws none. They are products with a
  position at the end of the barrier line and properties, and a note says so.
- **The ground is the surface the module shows**: the whole TIN up to the module's display cap
  (20 000 triangles), above which it is the regular grid the module resamples it to, and a note
  says so. Calculations in the module use the whole TIN.
- **A culvert barrel and a gantry member are separate products.** A barrel has its own pipe
  segment; a twin-barrel culvert is a group of two.
- **File size.** The test document (every module, 1.9 thousand instances) is about 280 KiB. Size
  follows the modules' meshes: a long pavement, corridor or lighting installation has many
  triangles, and a survey ground has up to 20 000.

## The sheet convention

Where nothing is placed on site, the design sheet is stood upright:

| Sheet | World |
| --- | --- |
| X | X |
| Y (up the sheet) | Z (up) |
| The face of the sheet | The plane Y = 0, facing −Y |
| Plate thickness | Extruded towards +Y, so the plate occupies 0 ≤ Y ≤ thickness |
| Artwork | In front of the face, at Y = −0.1 mm, −0.2 mm, ... |

An element with origin (x, y) on the sheet gets `IfcLocalPlacement` (x, 0, y) with unrotated
axes; its geometry is given in those axes (X along the face, Y into the plate, Z up) from its
lower-left front corner. All signs therefore stand in one vertical plane, arranged as on the
sheet. `export_ifc` always does this; `export_ifc_model` does it for the elements without a site
placement.

## Georeferencing (`IfcOptions::georeference`)

With `georeference: None` (the default) nothing changes: no coordinate reference is written and
placements are the site numbers. With `Some(IfcGeoreference)` and at least one element or alignment
with a position on the site, `export_ifc_model` writes

* `IfcProjectedCRS(Name, Description, GeodeticDatum, VerticalDatum, MapProjection, MapZone, MapUnit)`
  with the name as the EPSG code (`EPSG:32736`), a description, datums as the project states them,
  `UTM` and the zone where the application knows the system, and a metre as the map unit;
* `IfcMapConversion(SourceCRS = the geometric representation context, TargetCRS, Eastings, Northings,
  OrthogonalHeight, 1., 0., 0.001)`: no rotation, and a scale of 0.001, the factor from the file's
  millimetres to the map's metres.

**Local origin rule.** Geometry is written near an origin the exporter chooses, so viewers that keep
positions in 32-bit floats do not lose accuracy at 9 960 000 000 mm. The origin is `origin` when
given (metres); otherwise the least easting and least northing of every placement with a site
position and every alignment element, rounded down to a whole kilometre, at height 0. All site
placements (also the fallback position of a linear placement) and all alignment geometry are written
minus that origin, `IfcMapConversion` carries it, and levels are not shifted. Nothing is rotated or
scaled. A model with nothing on the site (a sheet of signs) gets no map conversion.

The IFC 4.3 attribute order was checked against the IFC4X3_ADD2 schema in IfcOpenShell and the
buildingSMART sample files (`bsi-4x3-Infra-Road.ifc`). The online documentation site refused the
request (403) and was not used. Tested (`tests/georef.rs`): the entities and values; every
cartesian point stays within a few kilometres; and IfcOpenShell reads the file back and, with the map
conversion added, every product is where it is in the file written without a georeference (to a
thousandth of a millimetre, `AlignmentExport::Simple`; the `Linear` mode is checked for the entities
and the size of the numbers, not read back).

**Import.** `MapConversion` now also carries the target `IfcProjectedCRS` attributes (`crs_name`,
`crs_description`, `geodetic_datum`, `vertical_datum`, `map_projection`, `map_zone`). The application
applies the map conversion by default and checks the model against the project coordinate system.

## What is not exported

- By default (`AlignmentExport::Simple`): linear placement, alignment curve geometry, vertical
  profiles. Never: `IfcAlignmentCant` and `IfcSegmentedReferenceCurve` (rail cant is not road
  superelevation; superelevation is written as `SUPERELEVATIONEVENT` referents in the other modes),
  `IfcOffsetCurveByDistances`, child alignments, transition curves other than the clothoid.
- Georeferencing: no map conversion or coordinate reference system; no `IfcRoadPart` breakdown.
- Fixings, clamps, base plates, reinforcement, the socket as a void, excavation and backfill.
- Material grades, categories, layers or profile sets (`IfcMaterialProfileSet`); sheeting classes
  and retro-reflectivity; mass. Colours are display colours for viewing, not specification values.
- Structural design: nothing about loads, checks or adequacy beyond the `DesignBasis` text.
- Type objects (`IfcSignType`, `IfcMemberType`), classifications, layers as
  `IfcPresentationLayerAssignment` (the layer name is a property), owner history, and the notes
  of `Generated`.
- `Pset_SignCommon.Category` and `TactileMarking`, `Qto_SignBaseQuantities.Weight`, the other
  properties of `Pset_MemberCommon`, `Pset_FootingCommon`: the document has no values for them.
- For the road modules: type objects (`IfcPipeSegmentType`, `IfcRailingType`, ...), classification
  references (the classification is the entity and predefined type of the mapping table; no
  `IfcClassificationReference` is written), `IfcRoadPart`s, materials beyond the free text of a
  pavement layer or a culvert, mass and weight quantities (masses are properties), photometry,
  structural or hydraulic design results (they are checks in the modules, not stored in the
  element), geometry for terminals and transitions, the markings' other types (arrows, hatching,
  studs: the markings module does longitudinal lines only), the survey ground beyond the display
  cap.
- Import: the crate cannot read IFC.

## Validating a file

The check is done with [IfcOpenShell](https://ifcopenshell.org), an independent IFC toolkit, as a
test-time tool only. The Rust crate and its Rust tests do not depend on Python.

```sh
pip install ifcopenshell pytest      # pytest is what IfcOpenShell's EXPRESS rule checker needs
python3 crates/afrobim-ifc/tests/check_ifc.py sheet.ifc          # a file from export_ifc
python3 crates/afrobim-ifc/tests/check_model_ifc.py model.ifc    # a file from export_ifc_model
python3 crates/afrobim-ifc/tests/check_roads_ifc.py roads.ifc    # a model with road-module elements
```

Both scripts open the file and run `ifcopenshell.validate.validate` (attribute types and
cardinalities of IFC4X3_ADD2 and, with pytest installed, the EXPRESS WHERE rules). Given a second
argument, a JSON description of what the file should contain, they also compare every element
with it; the Rust integration tests write that JSON from hand-built documents.

`check_ifc.py` checks the units, contexts, spatial structure, GlobalId uniqueness and that every
element has a coloured, tessellated Body; with expectations, also class, names, placement,
property sets, quantities, the plate's bounding box and volume, the plane, area, facing and
colour of each artwork face set, and the bounding box IfcOpenShell's geometry kernel computes.

`check_model_ifc.py` checks, on any model export: units; project > site > road; that every
element is either contained or an aggregated part, never both, and that parts are placed
relative to their assembly; predefined types; that only standard sets use `Pset_` / `Qto_`;
that IfcOpenShell's geometry kernel builds every post and foundation to the volume its
parameters give; and for every alignment that it is aggregated by the project, not contained,
ends in a zero-length segment, and is continuous (each segment's end, integrated numerically
from its start, direction, radii and length, is the next segment's start). With expectations it
also compares, for every product: class, the world position and rotation read back through the
placement chain (limit 0.001 mm), the volume from the swept-solid parameters and from the
kernel's mesh (limit 0.1 %), the kernel's bounding box and centroid against the 3D view's
solids moved to the same place (limit 1 mm, which is what a 48-sided prism differs from a
circle), the material, and the property sets; and for the alignment, every segment against the
alignment's element, the stationing, what it positions, and the Axis polyline against the
`IfcCompositeCurve` that `ifcopenshell.api.alignment` derives from the exported segments,
evaluated by IfcOpenShell's kernel (limit 0.01 mm).

`check_roads_ifc.py` checks, on any export that holds road-module elements: schema and EXPRESS
rules; project > site > road; that every element is a root contained in the road (terrain: in the
site), an aggregated part placed relative to its whole, or a surface feature that adheres to an
element and is contained nowhere; that every class and predefined type is in the mapping table;
that every `Pset_` / `Qto_` set exists in the IFC4X3 templates, applies to the class and
predefined type it is on, uses only the template's property names and the template's value types
(and `AfroBIM_` for the rest); and that IfcOpenShell's geometry iterator builds a shape for every
product with a representation. With expectations it also compares the kernel's world bounding box
and triangle count of every element with the module's solids moved to where the 3D view draws
them (limit 0.05 mm), placements read back through the chain (0.001 mm), the relationships
(adhesion, groups, aggregation, positioning), entity counts, and the quantities against kernel
volumes where they can be compared.

`check_alignment_ifc.py linear.ifc absolute.ifc expectations.json` checks the complete alignment
export (`AlignmentExport::Linear` against `Geometry`) as described under "Alignments, complete":
validation, the structure the documentation and the service's alignment rules ask for, the curves
evaluated by the kernel against the alignment crate, displayed stations recovered from the
referents, the world placement and bounding box of every product in both files, and the
`CartesianPosition` fallback. `check_import_linear.py` writes the kernel's world bounding boxes of a file as
JSON for `tests/import_linear.rs`.

`cargo test -p afrobim-ifc` runs all of them: `tests/ifcopenshell.rs`, `tests/ifcopenshell_model.rs`,
`tests/ifcopenshell_roads.rs` and `tests/ifcopenshell_linear.rs` export their fixtures to `target/tmp`, write the expectations and run the scripts. When `python3`
or `ifcopenshell` is missing these tests print `SKIPPED: IfcOpenShell validation not run ...` and
pass; set `AFROBIM_REQUIRE_IFCOPENSHELL=1` to make that a failure instead. `tests/export.rs` and
`tests/model.rs` hold the checks that need no toolkit, and so does `tests/roads.rs`, which builds a
document with an element of every module through the modules' own APIs
(`tests/common/roads.rs`) and checks entities and counts, containment, adhesion, placements,
geometry against the modules' solids, standard sets and quantities against the modules' own
numbers, positioning, determinism (with and without `generated`), and elements whose data cannot
be read; the others: determinism, byte-identity of the plain
export, GlobalIds, string escaping, instance ids and references, entity counts, units,
placements, relationships, swept-solid parameters, property sets, quantities, the alignment's
segments, and the fallbacks.

### What was validated, and what was not

**Road modules.** Validated with IfcOpenShell 0.9.0.post1 on the document of
`tests/common/roads.rs`, built through the modules' APIs on one alignment (lines, a right-hand
arc, a profile): three markings (two carried by the pavement, one beyond it), a side drain, a
single and a twin-barrel culvert, a portal and a cantilever gantry with a sign on the portal, an
installation of staggered lighting columns and a single column with two luminaires, a barrier
run with a terminal and a transition, a two-slab pavement with a surface-dressing layer of no
thickness, a ground TIN and a corridor on it (with cut and fill): `ifcopenshell.validate`
with schema and EXPRESS rules, **0 issues**; 116 properties of standard sets checked against the
IFC4X3 templates; 41 shapes built by the geometry iterator for the 41 products with a
representation; bounding boxes against the modules' solids to 7e-7 mm and triangle counts equal;
placements to 5e-7 mm; pavement course volumes against their slabs to 7e-6; 939 checks in all.
The same document without its pavement and corridor (every marking then a proxy) also validates.
Validation found one real defect on the way: the first version contained an unattached marking
in the road, which the schema forbids (see "Mapping"); that case is now a proxy.

**Signs, supports and alignments.**
Validated with IfcOpenShell 0.9.0 on the fixture of `tests/common/site.rs` (five signs with
supports of every section and shape, placed and unplaced, one sign alone, one loose post, one
unknown category, one alignment of lines, arcs and clothoids turning both ways) and on the same
model with nothing placed on site: schema and EXPRESS rules with no issue; every placement to
3.3e-7 mm; kernel volumes to 2e-5; the cylinder D600 × 1200 and the block 800 × 800 × 1000 by
parameters exactly and by kernel mesh to 1.7e-5 and 1e-15; the Axis polyline to 0.0002 mm of
IfcOpenShell's composite curve.

Not validated:

- The road modules were exercised on the hand-built document above and not on a real design
  (no corridor from a surveyed ground and a LandXML alignment through the toolkit; the larger the
  document, the larger the file).
- No file has been opened in a **commercial BIM viewer**, and none in any viewer: what a viewer
  does with a proxy, a group, a surface feature that adheres to a pavement, or the open surfaces
  of a corridor was not looked at.
- The **buildingSMART Validation Service** has not been run. Its rules were read and followed
  where named above (SPS007, OJP001, ALB004, ALB015, ALS005, the spatial composition table), but
  IfcOpenShell's validator checks the schema, not those rules, and not any model view definition.
  Rule ALB010 (industry practice: an alignment nests a referent) is met with one start-station
  referent, whose placement is an `IfcLocalPlacement` in the default mode and an
  `IfcLinearPlacement` in `Geometry` and `Linear`. The alignment rules of the service were
  re-implemented and checked for the complete modes (see "Alignments, complete"); the service
  itself needs a signed-in account and was not run.
- No **viewer** has opened these files. That an assembly "shows as one object" follows from the
  aggregation relationship; it was not looked at.
- Alignments read from **LandXML**, and alignments with discontinuous joints, were not exported
  in any test against the toolkit.
- `IfcFooting` for a deep planted foundation, and `IfcRelPositions` without a linear placement,
  are choices that conform to the schema; whether a given receiving tool expects something else
  is unknown.

## Import as a reference model

`afrobim_ifc::import` reads an IFC STEP file for **display and clash checking only**. Nothing becomes
an editable AfroBIM element: the result is a [`ReferenceModel`] of products with triangle meshes in
world millimetres, their spatial structure, names, GlobalIds, entity and predefined types,
property sets as text, and a report of what was and was not read. The export code above is
untouched, and no export test changed.

**Dependency direction.** `afrobim-coordination` depends on this crate (for `element_global_id`);
this crate does not depend on it. The importer therefore returns plain `Mesh3` meshes and ids, and
the app builds clash `Body` values from them (example below), which keeps the graph acyclic.

```rust
use afrobim_ifc::import::{import_ifc, ImportOptions};
use afrobim_coordination::{Body, ClashTest, Coordination, Scene, Selection};
use afrobim_kernel::ElementId;

let model = import_ifc(std::path::Path::new("structure.ifc"), &ImportOptions::default())?;
println!("{} products, {} with geometry, {} not read", model.report.products,
         model.report.products_read + model.report.products_partial, model.report.products_not_read);
for (item, count) in &model.report.items_not_read {
    eprintln!("geometry not read: {item} x{count}");
}
// Clash bodies: ids above the document's own, category "reference", layer = the IFC entity.
let base = 1_000_000;
let bodies: Vec<Body> = model
    .clashable()
    .into_iter()
    .map(|i| {
        let p = &model.products[i];
        Body::new(ElementId(base + i as u64), &p.name, "reference", &p.entity, None, p.meshes.clone())
    })
    .collect();
// model.tree / model.roots hold the spatial structure; p.properties the property sets;
// model.find_global_id(guid) finds a product by GlobalId (for BCF components).
```

`cargo run -p afrobim-ifc --example import_stats -- model.ifc` prints the report for one file.

### Sources, schemas and attribute positions

- **Format:** ISO 10303-21 clear text encoding, read by `import::StepFile`: header (`FILE_SCHEMA`,
  `FILE_NAME`), `#id=ENTITY(...)` instances, `$ * ` null and derived, integers, reals, strings with
  `''` and the `\X\`, `\X2\`, `\X4\`, `\S\`, `\P?\` directives, enumerations, references, lists,
  typed values, binaries, comments, and complex instances (counted, not read). The instance table
  keeps the file buffer plus 16 bytes per instance and parses attribute lists on demand.
  The ISO text itself was not consulted (it is paywalled). The string escapes were checked once
  against IfcOpenShell 0.9.0.post1's reading of the same strings (the expectations in
  `stepfile.rs` are its results): equal for `''`, `\\`, `\X\`, `\X2\`, `\X4\` and `\S\`, except
  that a UTF-16 surrogate pair in `\X2\` reads as an empty string in IfcOpenShell and as the
  character here. The grammar is exercised by every sample file below.
- **Schemas supported:** IFC2X3 (and TC1), IFC4 (ADD1, ADD2, ADD2 TC1), IFC4X3 (ADD1, ADD2, TC1).
  IFC4X1 and IFC4X2 are read with the IFC4 layout and IFC4X3 drafts with the IFC4X3_ADD2 layout,
  each with a warning; any other schema name is refused.
- **Attribute positions are not written from memory.** `src/import/tables.rs` is generated by
  `tests/gen_import_tables.py` from the EXPRESS schemas compiled into **IfcOpenShell 0.9.0.post1**
  (`ifcopenshell_wrapper.schema_by_name("IFC2X3" | "IFC4" | "IFC4X3_ADD2")`; that install ships no
  `.exp` files, the compiled schemas are what it parses and validates with). For every entity the
  importer reads it holds the attribute names in schema order, and for all `IfcProduct` subtypes of
  each schema the position of `PredefinedType` (IFC2X3 `IfcWall` has none; IFC4 and IFC4X3 `IfcWall`
  have it at 8), whether the entity is a spatial structure element
  (`IfcSpatialStructureElement`, `IfcSpatialElement` in IFC4X3) and whether it is an opening. The
  reader looks attributes up by name, so a layout difference between schemas is the table's business.
  The test `the_attribute_tables_match_the_installed_ifcopenshell_schemas` regenerates and compares.
  Differences the tables capture: `IfcCartesianPointList3D.TagList` (IFC4X3), the triangulated and
  polygonal face sets (IFC4 and later only), `IfcMapConversion` (IFC4 and later), `IfcProperty`
  attribute 1 (`Description` vs `Specification`).
- **Mapped items.** The IFC documentation text shipped with IfcOpenShell (`express/DocEntity.csv`,
  `IfcRepresentationMap`) says the `MappingOrigin` "defines the coordinate system in which the
  `MappedRepresentation` is defined", and IfcOpenShell places a point of the mapped representation
  by the origin first and then by the target operator. The importer does the same (`target * origin`).
  ISO 10303-43 can be read as `target * inverse(origin)`; the two agree for the usual identity
  origin, and the choice follows IfcOpenShell, which is what the verification compares against.

### What is read

| What | How |
| --- | --- |
| Products | Every instance of an `IfcProduct` subtype of the file's schema: step id, GlobalId, name, entity, `PredefinedType` (when the entity has it and it is set), `ObjectType`, spatial and opening flags. |
| Spatial tree | `IfcRelAggregates` (any object to its parts) and `IfcRelContainedInSpatialStructure`; `tree`, `roots` and per product `container` and `parent`. |
| Properties | `IfcRelDefinesByProperties` to `IfcPropertySet` (single, enumerated and list values) and `IfcElementQuantity` (the `IfcQuantity*` values), as set name, name, value text and IFC value type. Other property kinds (bounded values, tables, complex properties, ...) are counted in `properties_not_read`. Type objects' property sets (`IfcRelDefinesByType`) are not followed. |
| Units | `IfcProject.UnitsInContext` to the `LENGTHUNIT`: an `IfcSIUnit` metre with any SI prefix, or an `IfcConversionBasedUnit` through its `IfcMeasureWithUnit` (inch, foot, ...). Everything is converted to mm at the end. No length unit: metres assumed, with a warning. |
| Placement | `IfcLocalPlacement` chains with `IfcAxis2Placement3D` (axis, reference direction orthogonalised, defaults, a parallel pair repaired) resolved iteratively (cycles and chains over 4096 are errors). `IfcLinearPlacement` is evaluated on its basis curve (`IfcCompositeCurve` of lines, circles and clothoids, `IfcGradientCurve` with lines, parabolas and circles; see "Alignments, complete"), or taken from its `CartesianPosition` when the curve is not understood (`report.linear_placements_resolved` and `_fallback` count them). Anything else (`IfcGridPlacement`, a linear placement on another curve without a fallback) is **not resolved**: the product gets `PlacementUnresolved`, no geometry, and `report.placements_unresolved` counts it. The site's placement is just part of the chain. |
| Map conversion | `IfcMapConversion` (and `IfcProjectedCRS.MapUnit`) is read into `map_conversion`. With `ImportOptions::apply_map_conversion` the world coordinates are `E + scale * R(x, y)`, `N + ...`, `H + scale * z`, with `R` the rotation of `(XAxisAbscissa, XAxisOrdinate)`. Default off: the coordinates are then the engineering coordinates, as IfcOpenShell's iterator returns them. The map applied result was compared with IfcOpenShell's `util.geolocation` helmert transformation on the three Bonsai `geolocation-mapconversion*.ifc` files (map unit equal to the project unit). |
| Geometry | `IfcTriangulatedFaceSet`, `IfcPolygonalFaceSet` (with `IfcIndexedPolygonalFaceWithVoids`; `PnIndex`), `IfcFacetedBrep` (faces with inner bounds, bound orientation), `IfcShellBasedSurfaceModel`, `IfcFaceBasedSurfaceModel`, `IfcExtrudedAreaSolid` over `IfcRectangleProfileDef`, `IfcCircleProfileDef`, `IfcArbitraryClosedProfileDef` and `IfcArbitraryProfileDefWithVoids` whose curves are `IfcPolyline` or straight-segment `IfcIndexedPolyCurve`, and `IfcMappedItem` through `IfcCartesianTransformationOperator3D` (and non uniform), recursively. |

Which representations are read: the product's `Body` representation (identifier `Body`, any case),
or if it has none, representations of a body type (`Tessellation`, `Brep`, `SweptSolid`,
`MappedRepresentation`, `SolidModel`, `SurfaceModel`, `Clipping`, ...). `Axis`, `FootPrint`, `Box`
and curve representations are not body geometry. If no body item can be read and the product has
a `Box` representation with an `IfcBoundingBox`, a box of that size stands in
(`BoundingBoxPlaceholder`); the option `bounding_box_placeholders` switches this off.

**Mesh conventions.** One `Mesh3` per body item, vertices in world mm (unused point-list vertices
dropped), triangles counter-clockwise seen from outside for solids; a transform with a negative
determinant (a mirror) flips the winding back; an extrusion whose direction runs against the
profile normal is turned right side out. Circles are inscribed polygons of
`circle_segments` vertices (default 32, first vertex on the profile's +X axis); a circle's box
therefore misses the true one by up to `r (1 - cos(pi / n))`, 0.48 % of `r` for 32. Faces are
triangulated by ear clipping with holes bridged along the shortest segment that crosses no edge
and leaves both ends inside the polygon; a face whose triangle areas do not add up to outline
minus holes (self-intersecting loops) is counted in `faces_inconsistent` and warned about.
Colours are not read (every mesh is grey).

### Coverage and what is not read

Everything else is counted per entity type in `report.items_not_read` ("geometry not read"), the
product stays in the list with `NotRead` or `Partial` and `unread_items`. Not read: boolean results
(`IfcBooleanClippingResult`, `IfcBooleanResult`), `IfcRevolvedAreaSolid`, `IfcSweptDiskSolid`,
`IfcSurfaceCurveSweptAreaSolid`, the IFC4X3 swept solids along curves
(`IfcFixedReferenceSweptAreaSolid`, `IfcDirectrixDerivedReferenceSweptAreaSolid`,
`IfcSectionedSolidHorizontal`, `IfcSectionedSurface`), `IfcAdvancedBrep`, `IfcCsgSolid`,
`IfcFacetedBrepWithVoids`, extrusions over any other profile (parameterized I, L, T, U, C, Z
shapes, hollow rectangles and circles, `IfcCompositeProfileDef`, derived profiles), profile
curves with arcs (`IfcCompositeCurve`, `IfcIndexedPolyCurve` with `IfcArcIndex`, trimmed curves),
alignments and other curve-only products. `IfcOpeningElement` voids are listed (flag `is_opening`)
but **not subtracted** from the walls they cut; `IfcRelVoidsElement` is not processed.

Coverage on 33 independent files (`IFC_SAMPLES_DIR`, default `/home/user/ifcsamples/files`; see
below), spatial elements left out:

| Schema | Files | Physical products | Body read (fully or partly) | Placeholder box | Body not read | No body representation |
| --- | --- | --- | --- | --- | --- | --- |
| IFC2X3 | 6 | 420 | 215 | 0 | 12 | 193 |
| IFC4 | 13 | 355 | 277 | 4 | 6 | 68 |
| IFC4X3_ADD2 | 14 | 493 | 345 | 0 | 10 | 138 |
| all | 33 | 1268 | 837 (66 %) | 4 | 28 | 399 |

Of the 869 products that have a body representation, 837 (96.3 %) were read. The 399 without one
are mostly `IfcAnnotation` (2D annotation without a body), `IfcElementAssembly` (their parts carry
the geometry), `IfcAlignment` with its segments and parts, and `IfcDistributionPort`; they have no
body representation to read. (Alignments have curve geometry in IfcOpenShell's eyes; the importer
does not read alignment curves as geometry. It evaluates them only to place products linearly.) By body representation type (products read or
partly read / not read): `Tessellation` 475/0, `Brep` 132/0, `SurfaceModel` 60/0,
`MappedRepresentation` 101/3, `SweptSolid` 69/5, `Clipping` 0/14, `AdvancedSweptSolid` 0/9,
`AdvancedSurface` 0/1. The items not read in these files: `IfcBooleanClippingResult` 14,
`IfcIndexedPolyCurve` (with arcs) 22, `IfcExtrudedAreaSolid` over `IfcCircleHollowProfileDef` 5,
`IfcFixedReferenceSweptAreaSolid` 4, `IfcSectionedSolidHorizontal` 4, `IfcRevolvedAreaSolid` 3,
`IfcTrimmedCurve` 2, and one each of `IfcGeometricCurveSet`,
`IfcDirectrixDerivedReferenceSweptAreaSolid`, `IfcSectionedSurface`.

### Verification against IfcOpenShell

`tests/import_ifcopenshell.rs` and `tests/check_import.py` (skipped, printing why, when `python3` or
IfcOpenShell is missing, unless `AFROBIM_REQUIRE_IFCOPENSHELL` is set) import each file and let
IfcOpenShell 0.9.0.post1's geometry iterator (`use-world-coords`, `disable-opening-subtractions`)
build the same products, then compare:

- **Product counts by type**: equal in all 38 comparisons.
- **World bounding box of each product's geometry**, for products both read: the products read
  fully are held to `0.5 mm + 0.5 % of the extent`. Result: **938 products fully read, all within
  tolerance; the largest deviation is 0.275 mm** (a 32-gon circle against IfcOpenShell's circle on a
  2000 mm product); all others agree to 1e-6 mm. Nine products read partly (bounding box placeholder
  or some items unread) were compared and reported but not held to the tolerance.
- **Signed volume** of each fully read product against IfcOpenShell's: all 878 with a volume agree
  within 0.99488 to 1.00000 (the low end are the inscribed circle polygons).
- **Geometry only on one side**: IfcOpenShell builds geometry for products the importer does not
  (boolean results, revolved and swept-along-curve solids, hollow profiles, alignment curves);
  the importer never produced geometry IfcOpenShell lacked.

Inputs: the crate's own exported road model (`tests/common/roads.rs`, 61 products, 41 with solids,
all within tolerance); the hand-written cases of `tests/common/cases.rs` (slanted extrusion with
rotated profile and solid frames, mapped items with a rotated origin and a scaled, rotated
operator, triangulated sets with and without `PnIndex`, a polygonal face set and a faceted brep
with holes and a reversed bound, circle and voided profiles, a four-deep placement chain);
and 33 files from three sources: buildingSMART `Sample-Test-Files` (`Simple-Scene`, ten files
of IFC4X3_ADD2 and four each of IFC4 and IFC2X3: building architecture, structural, HVAC,
landscaping, and infra road, bridge, rail, plumbing, electrical), the KIT FZK-Haus (IFC4, 2.5 MB,
from the IfcOpenShell repository) and the BCF-XML repository's `Architectural.ifc` and `MEP.ifc`
(IFC2X3), the Bonsai test and tutorial files (IFC4: walls, openings, aggregates, map conversions)
and IfcOpenShell's geometry fixtures (IFC4X3_ADD2). The sample files are not in this repository
(licences differ); `IFC_SAMPLES_DIR` points the tests at a directory.

**Failures found by this verification, and fixed:** a hole-bridging bug in the first triangulator
(two holes whose rightmost vertices share a scanline gave a face with the wrong area; found as a
volume ratio of 1.71 on five FZK windows, now covered by unit tests and 300 random multi-hole
cases) and the mapped item origin (see above). **Not verified:** no file was opened in a
commercial BIM tool or any viewer; nothing was tested with the importer's output in the AfroBIM
app (this crate has no app code). The 3D positions of IFC4X3 linear placements are verified on one
foreign file only (the buildingSMART sample in "Alignments, complete"), and on this crate's own
exports.

### Performance

A generated 40.6 MB IFC4 file (`tests/import_perf.rs`, ignored by default: 15,986 products, 287,785
instances, faceted breps, tessellated blocks, extrusions with arbitrary profiles and mapped items,
1,326,972 triangles): parse into the instance table 0.3 s; full import (geometry, tree,
properties) **2.1 s and a peak of 103 MB** resident set (`VmHWM`, fresh process,
`cargo run --example import_stats`), **in a debug build** (release builds were not made on this
machine, so the release figure is not measured and will be lower). The peak holds the 40.6 MB
buffer, the instance table and the output meshes. The largest real file tried is the 2.5 MB FZK-Haus
(127 products, about 22,000 triangles) at about 0.1 s in a debug build. Attribute lists are parsed on
demand, point lists are read straight from the text, and a mapped representation is
triangulated once per use of its map.

### Limits

- Read-only and for reference; nothing is converted to editable elements.
- Geometry not read as listed above; openings are not subtracted, so walls show without their holes.
- Colours, materials, layers, styles, groups, systems, classifications, type objects, units other
  than length, and `IfcRelDefinesByType` properties are not read.
- A product's world mesh is f64 millimetres; coordinates of tens of millions of mm keep
  sub-micron accuracy.
- Ear clipping is quadratic per face; a face with tens of thousands of vertices is slow.
- Files of 4 GiB or more are refused (32-bit offsets). The whole file is read into memory.
- `IfcMapConversion` with a map unit different from the project unit is converted by the CRS
  `MapUnit` only when it is a metre or conversion based unit; the IfcOpenShell comparison covers a
  map unit equal to the project unit.
