# afrobim-ifc

IFC 4.3 export of AfroBIM documents. Export only; there is no importer.

The crate writes an **IFC4X3_ADD2** STEP physical file (ISO 10303-21) itself, in pure Rust, and
does not link an IFC toolkit. It has two entry points that take the same input:

| Entry point | Writes |
| --- | --- |
| `export_ifc` | The signs, stood upright as they are laid out on the design sheet. Unchanged since the first version. |
| `export_ifc_model` | The model: signs with their posts and foundations as assemblies, every element where it stands on site, and the alignments. |

For a document without supports, site placements and alignments the two give byte-identical
files. `export_ifc` ignores `Element::site` and `Element::host` and writes a post or a foundation
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
| `IfcOptions` | Project name, header author / organization / application, time stamp (`None` = now), plate thickness (3 mm), artwork offset (0.1 mm). |
| `IfcElementInput` | An element and, optionally, the `Generated` its module produced for it (drawing, derived values, solids). |
| `export_ifc_model(document, elements, options) -> IfcExport` | The model file (`text`) plus `notes`. `write_ifc_model` returns the text only. |
| `export_ifc(document, elements, options) -> IfcExport` | The sign-sheet file plus `notes`. `write_ifc` returns the text only. |
| `element_global_id(project_name, id)` / `global_id(name)` | The deterministic GlobalIds the exporter uses. |
| `POST_CATEGORY`, `FOUNDATION_CATEGORY`, `ALIGNMENT_CATEGORY`, `SIGN_CATEGORY` | The element categories with a mapping. |
| `PROP_DESIGN_BASIS` | The element property (`"design_basis"`) copied to `DesignBasis`. |
| `ALIGNMENT_CHORD_M` | Longest chord of an alignment's Axis polyline (1 m). |
| `tessellate_artwork(drawing) -> Artwork` | The triangulation on its own: one `ArtworkMesh` per colour and paint layer. |
| `step_string`, `step_real` | STEP literal encoding. |

With a fixed `IfcOptions::timestamp` the same input gives byte-identical text.

`export_ifc_model` needs nothing the application does not already hand to `export_ifc`: the
elements of the document (it follows `Element::host` and reads `Element::site`) and, per element,
the `Generated` of its module. The post and foundation definitions are read from `Element::data`
with `afrobim-supports`, and alignments with `afrobim-alignment`; this crate depends on both.

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

### Alignments

An alignment is exported as business logic plus a simple curve:

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
- Only the horizontal geometry is exported. There is no `IfcAlignmentVertical` or cant.

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

## What is not exported

- Linear placement (`IfcLinearPlacement`), alignment curve geometry (`IfcCompositeCurve`,
  `IfcGradientCurve`), vertical profiles and cant.
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
- Import: the crate cannot read IFC.

## Validating a file

The check is done with [IfcOpenShell](https://ifcopenshell.org), an independent IFC toolkit, as a
test-time tool only. The Rust crate and its Rust tests do not depend on Python.

```sh
pip install ifcopenshell pytest      # pytest is what IfcOpenShell's EXPRESS rule checker needs
python3 crates/afrobim-ifc/tests/check_ifc.py sheet.ifc          # a file from export_ifc
python3 crates/afrobim-ifc/tests/check_model_ifc.py model.ifc    # a file from export_ifc_model
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

`cargo test -p afrobim-ifc` runs both: `tests/ifcopenshell.rs` and `tests/ifcopenshell_model.rs`
export their fixtures to `target/tmp`, write the expectations and run the scripts. When `python3`
or `ifcopenshell` is missing these tests print `SKIPPED: IfcOpenShell validation not run ...` and
pass; set `AFROBIM_REQUIRE_IFCOPENSHELL=1` to make that a failure instead. `tests/export.rs` and
`tests/model.rs` hold the checks that need no toolkit: determinism, byte-identity of the plain
export, GlobalIds, string escaping, instance ids and references, entity counts, units,
placements, relationships, swept-solid parameters, property sets, quantities, the alignment's
segments, and the fallbacks.

### What was validated, and what was not

Validated with IfcOpenShell 0.9.0 on the fixture of `tests/common/site.rs` (five signs with
supports of every section and shape, placed and unplaced, one sign alone, one loose post, one
unknown category, one alignment of lines, arcs and clothoids turning both ways) and on the same
model with nothing placed on site: schema and EXPRESS rules with no issue; every placement to
3.3e-7 mm; kernel volumes to 2e-5; the cylinder D600 × 1200 and the block 800 × 800 × 1000 by
parameters exactly and by kernel mesh to 1.7e-5 and 1e-15; the Axis polyline to 0.0002 mm of
IfcOpenShell's composite curve.

Not validated:

- The **buildingSMART Validation Service** has not been run. Its rules were read and followed
  where named above (SPS007, OJP001, ALB004, ALB015, ALS005, the spatial composition table), but
  IfcOpenShell's validator checks the schema, not those rules, and not any model view definition.
  Rule ALB010 (industry practice: an alignment nests a referent) is met with one start-station
  referent whose placement is an `IfcLocalPlacement`, not the `IfcLinearPlacement` a stationing
  referent would normally have.
- No **viewer** has opened these files. That an assembly "shows as one object" follows from the
  aggregation relationship; it was not looked at.
- Alignments read from **LandXML**, and alignments with discontinuous joints, were not exported
  in any test against the toolkit.
- `IfcFooting` for a deep planted foundation, and `IfcRelPositions` without a linear placement,
  are choices that conform to the schema; whether a given receiving tool expects something else
  is unknown.
