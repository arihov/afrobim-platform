# afrobim-ifc

IFC 4.3 export of AfroBIM documents. Export only; there is no importer.

`write_ifc` turns a `Document` and the generated drawings of its elements into an
**IFC4X3_ADD2** STEP physical file (ISO 10303-21). The crate is pure Rust; it writes the file
itself and does not link an IFC toolkit.

```rust
use afrobim_ifc::{export_ifc, IfcElementInput, IfcOptions};

let inputs: Vec<IfcElementInput> = document
    .elements()
    .iter()
    .map(|element| IfcElementInput { element, generated: generated_for(element) })
    .collect();
let export = export_ifc(&document, &inputs, &IfcOptions { project_name: "A104 signs".into(), ..IfcOptions::default() });
std::fs::write("a104-signs.ifc", &export.text)?;
for note in &export.notes {
    eprintln!("{note}"); // what was skipped or approximated
}
```

## API

| Item | Purpose |
| --- | --- |
| `IfcOptions` | Project name, header author / organization / application, time stamp (`None` = now), plate thickness (3 mm), artwork offset (0.1 mm). |
| `IfcElementInput` | An element and, optionally, the `Generated` its module produced for it. |
| `write_ifc(document, elements, options) -> String` | The file. |
| `export_ifc(document, elements, options) -> IfcExport` | The file (`text`) plus `notes` on what was left out or approximated. |
| `element_global_id(project_name, id)` / `global_id(name)` | The deterministic GlobalIds the exporter uses. |
| `tessellate_artwork(drawing) -> Artwork` | The triangulation on its own: one `ArtworkMesh` per colour and paint layer. |
| `step_string`, `step_real` | STEP literal encoding. |

With a fixed `IfcOptions::timestamp` the same input gives byte-identical text.

## What is exported

| AfroBIM | IFC 4.3 |
| --- | --- |
| The export | `IfcProject` (name from the options), units `IfcSIUnit` millimetre, square metre, radian |
| Model space | `IfcGeometricRepresentationContext` "Model", 3D, with a "Body" `IfcGeometricRepresentationSubContext` (`MODEL_VIEW`) |
| (implicit) | One `IfcSite` "Site", aggregated by the project with `IfcRelAggregates` |
| Element, category `sign` | `IfcSign`, `PredefinedType` `PICTORAL` |
| Element, any other category | `IfcBuildingElementProxy`, `PredefinedType` `USERDEFINED`, `ObjectType` = the category |
| `Element::id` | `Tag`, and (with the project name) the `GlobalId` |
| `Element::name` (the mark) | `Name` |
| `data.kind` | `ObjectType` (signs), `AfroBIM_Sign.Kind` |
| `data.kind`, `data.catalog_key`, `data.speed_kph` | `Description`, e.g. "kind StackType, catalog key GFS-B1, design speed 80 km/h" |
| `Element::origin` | `IfcLocalPlacement` relative to the site's placement |
| Membership of the document | One `IfcRelContainedInSpatialStructure` from the site to every element |
| `Element::width` × `height`, plate thickness | The plate: a closed `IfcTriangulatedFaceSet` box (12 triangles), grey |
| `Generated::drawing` | The artwork: one open `IfcTriangulatedFaceSet` per colour and paint layer |
| `Colour` | `IfcStyledItem` → `IfcSurfaceStyle` (`BOTH`) → `IfcSurfaceStyleShading` → `IfcColourRgb`, one style per colour per file, from `Colour::display_rgb()` |
| Mark, category, layer, `Document::standard`, kind, catalog key, design speed | `IfcPropertySet` `AfroBIM_Sign` (`AfroBIM_Element` for a proxy): `Mark`, `Category`, `Layer`, `Standard`, `Kind`, `CatalogKey`, `DesignSpeedKph` |
| `data.catalog_key`; a road sign stands outdoors | `Pset_SignCommon`: `Reference`, `IsExternal` = true (signs only) |
| `Element::properties` | `IfcPropertySet` `AfroBIM_Properties`, each value an `IfcLabel` (an `IfcText` beyond 255 characters) |
| `Generated::derived` | `IfcPropertySet` `AfroBIM_Derived` |
| Width, height, plate thickness | `IfcElementQuantity` `Qto_SignBaseQuantities`: `Width`, `Height`, `Thickness` (signs only) |
| Width, height, face area | `IfcElementQuantity` `AfroBIM_BaseQuantities`: `Width`, `Height` (mm), `GrossArea` (m²) |

The element's `data` is treated as opaque JSON: only `kind`, `catalog_key` and `speed_kph` are
read, and only when they are scalars. Empty property sets are not written. There is no
`IfcOwnerHistory`: it is optional in IFC 4.3, and the author, organization and application go in
the file header instead.

The Body representation has `RepresentationType` `Tessellation`, so every item in it is a
tessellated face set; that is why the plate is a triangulated box and not an `IfcExtrudedAreaSolid`.

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
`"<project>/element/<id>"`, `"<project>/element/<id>/pset/<set name>"` and so on. The same project
name and element id therefore always give the same GlobalId, and re-exporting a document keeps
the identity of its objects. A kernel `Document` has no identifier of its own, so the project name
is the scope: renaming the project changes every GlobalId, and two different documents exported
under one project name share GlobalIds for elements with the same id.

## Placement convention (temporary)

AfroBIM model space is, today, a flat sheet on which signs are laid out side by side; there is no
site placement yet. Until there is, the exporter stands the sheet upright:

| Sheet | World |
| --- | --- |
| X | X |
| Y (up the sheet) | Z (up) |
| The face of the sheet | The plane Y = 0, facing −Y |
| Plate thickness | Extruded towards +Y, so the plate occupies 0 ≤ Y ≤ thickness |
| Artwork | In front of the face, at Y = −0.1 mm, −0.2 mm, ... |

An element with origin (x, y) on the sheet gets `IfcLocalPlacement` (x, 0, y) relative to the
site, with unrotated axes; its geometry is given in those axes (X along the face, Y into the
plate, Z up) from its lower-left front corner. All signs therefore stand in one vertical plane,
arranged as on the sheet. This says nothing about where a sign stands on a road, and will be
replaced when elements carry a real position and orientation.

## What is not exported

- Alignments, chainage or any linear placement (`IfcAlignment`, `IfcLinearPlacement`).
- Real site placement: no map conversion, coordinate reference system, georeferencing or site
  coordinates; no road, facility or facility-part breakdown (elements are contained in the site).
- Posts, foundations and fixings, and the `IfcSignalAssembly`-style grouping of several panels.
- Materials (`IfcMaterial`), sheeting classes and retro-reflectivity; colours are display colours
  for viewing, not specification values.
- Type objects (`IfcSignType`), classifications, layers as `IfcPresentationLayerAssignment`
  (the layer name is a property), owner history, and the notes of `Generated`.
- `Pset_SignCommon.Category` and `TactileMarking`, and `Qto_SignBaseQuantities.Weight`: the
  document has no values for them.
- Import: the crate cannot read IFC.

## Validating a file

The check is done with [IfcOpenShell](https://ifcopenshell.org), an independent IFC toolkit, as a
test-time tool only:

```sh
pip install ifcopenshell pytest      # pytest is what IfcOpenShell's EXPRESS rule checker needs
python3 crates/afrobim-ifc/tests/check_ifc.py model.ifc
```

`check_ifc.py` opens the file, runs `ifcopenshell.validate.validate` (attribute types and
cardinalities of IFC4X3_ADD2 and, with pytest installed, the EXPRESS WHERE rules), and checks the
units, contexts, spatial structure, GlobalId uniqueness and that every element has a coloured,
tessellated Body. Given a second argument, a JSON description of what the file should contain, it
also compares every element: class, names, placement, property sets, quantities, the plate's
bounding box and volume, the plane, area, facing and colour of each artwork face set, and the
bounding box the IfcOpenShell geometry kernel computes for the shape.

`cargo test -p afrobim-ifc` does this for a hand-built document: `tests/ifcopenshell.rs` exports
it, writes the expectations and runs the script. When `python3` or `ifcopenshell` is missing the
test prints `SKIPPED: IfcOpenShell validation not run ...` and passes; set
`AFROBIM_REQUIRE_IFCOPENSHELL=1` to make that a failure instead. `tests/export.rs` holds the checks
that need no toolkit: determinism, GlobalIds, string escaping, sequential instance ids, dangling
references, and triangle counts and areas.

IfcOpenShell's validator checks the schema, not the buildingSMART implementer agreements
(the normative rules of the bSI Validation Service) and not any model view definition. Those have
not been run against this exporter.
