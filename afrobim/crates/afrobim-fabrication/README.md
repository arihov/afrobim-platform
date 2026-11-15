# afrobim-fabrication

Sign-face quantities (BoQ) and fabrication sheets; see the crate docs. This README covers the
sign schedule import and export (`afrobim_fabrication::sign_schedule`).

## Sign schedule CSV

One row per sign. The header row names the columns; the order does not matter. Lines whose first
cell starts with `#` are comments, blank lines are ignored, and both are listed in the plan, not
dropped silently. Any of comma, semicolon or tab separates cells, fields may be quoted (`"` doubled
inside), and UTF-8 with or without a byte order mark is read. `row N` in every message is line N of
the file where the record starts (the header is usually line 1).

| Column (canonical header) | Required | Meaning |
|---|---|---|
| `mark` | yes | The mark the sign is scheduled under, e.g. `ADS-001`. Unique in the file (ignoring case). Must not start with `#`. |
| `code` | yes | The rule pack's catalog key (e.g. `UK06.1`), or failing that its TSRGD diagram number when that names exactly one entry. Ignoring case. |
| `description` | no | Free text. Stored as element property `Description`. |
| `alignment` | yes | The alignment's name, ignoring case. |
| `station` | yes | The station as displayed on the drawings: `k+mmm.mmm` (`12+345.678`, `0+250`) or plain metres (`12345.678`, `250 m`). The metres after `+` are under 1000. A leading `-` negates. |
| `region` | no | The station-equation region, a whole number, 0 before the first equation. Needed when the displayed station occurs more than once. |
| `side` | yes | `left` or `right`, looking up-station (`l`, `r`, `lhs`, `rhs` accepted). `both` is refused: write two rows. |
| `offset_m` | yes | Distance from the centreline in metres, not negative. The side gives the side. |
| `mounting_height_mm` | no | Ground to the lower edge of the face. Stored as `Mounting height (mm)`, the key the supports crate reads. |
| `size_mm` | no | The nominal size (x-height, or the size the pack tables). Stored as `Nominal size (mm)`. See limits. |
| `legend` | for lettered signs | Destinations, see below. |
| `rotation_deg` | no | Replaces the rotation the alignment gives (degrees, site frame). A warning on the row says so. |
| `notes` | no | Free text. Stored as `Notes`. |
| `param:<name>` | no | One parameter of the sign definition, see below. |

Units: a bare number in `offset_m` is metres, in `mounting_height_mm` and `size_mm` millimetres.
A cell may carry its own unit (`2.4 m`, `2400mm`). A header may say the unit (`Offset (m)`,
`Mounting Height (m)`); the mapping then keeps it. The decimal mark is `.`; a comma in a number is
refused rather than guessed.

### Legend

For the sign types with lettered destinations (stack, direction, confirmatory, place identification,
lane pre-selection, diversion, map-type) the legend is required; for the others it is refused
(`LegendNotAccepted`), not dropped. Destinations are separated by `|`. Each is
`Name`, then optionally `@Direction` (Ahead, Left or Right; Ahead if omitted), `#distance` and
`~route number`, in that order. A literal `| @ # ~ \` in a name is written with a backslash before
it. A destination's proximity order is its position among those of the same direction.

`Kasese@Ahead#12 km | Masheruka@Right | Fort Portal@Ahead~A109`

### Parameters

A `param:<name>` column sets one field of the definition the catalog entry seeds. A blank cell
leaves the catalog's value. Names, all lower case:

`speed_kph` (whole km/h), `approach_class` (primary, secondary, local), `scheme` (auto, primary,
secondary, information, tourist, temporary, motorway), `variant` (primary, secondary), `background`
(white, black, green, blue, brown, yellow, red, grey), `mirror` (none, left, right), `text_lines`
(whole number), `info_arrow` (ahead, left, right, none), `info_distance` (text), `info_template`
(symbol_plate, service_panel), `symbol_id` (text). The list is `sign_schedule::PARAMS`. An unknown
name is `UnknownParam` on every row that gives it a value.

### Template and export

* `template_csv(&CsvStyle)` writes the header row (all fixed columns and all `param:` columns) and one example line that starts with `#`, so an import skips it. The example holds placeholders, not values.
* `export_csv(&doc, &alignments, &catalog, &CsvStyle)` writes the document's `sign` elements in the same format and returns `Export { csv, signs, warnings }`. Stations are written as displayed (through the station equations) with the region when the alignment has equations. Only the parameters that differ from the catalog's seed are written, and only the `param:` columns some sign needs. A rotation is written only when it differs from what the alignment gives.
* `CsvStyle { delimiter, bom }`: comma and no BOM by default.

Export then import round-trips the stations, sides, offsets, definitions and the schedule's properties of signs that have a catalog code. The warnings list what a file cannot carry: a sign with no catalog code the loaded pack knows (its code is blank, so it cannot be imported back), a sign not placed on an alignment, a destination symbol or route status, a proximity order other than the position in the list, an unreadable definition.

## Column mapping

```rust
use afrobim_fabrication::sign_schedule::*;

let table = parse_csv(&text)?;                       // delimiter detected from the header line
let proposal = propose_mapping(&table.headers);      // by synonym, nothing guessed beyond them
// proposal.missing_required, .unmapped_headers, .duplicate_headers, .unknown_params
let mut mapping = proposal.mapping;
mapping.set("Wibble", Field::Code, None);            // the caller overrides or supplies
std::fs::write("mapping.json", mapping.to_json())?;  // saved; ColumnMapping::from_json loads it
```

A header matches when, lower-cased with bracketed parts removed and every run of other characters
made one space, it equals one of the names below. `param:<name>` headers map to that parameter.
A field that two headers match goes to the first; the second is listed and not read. A header that
matches nothing is listed and not read.

| Field | Accepted headers |
|---|---|
| mark | mark, sign mark, sign no, sign number, sign ref, sign reference, sign id, ref, reference, id, item no |
| code | code, sign code, diagram, diagram no, diagram number, diagram ref, sign diagram, catalog key, catalogue key, sign page, sign type, type |
| description | description, desc, sign description, name, sign name, details |
| alignment | alignment, alignment name, road, road name, route |
| station | station, stationing, chainage, chain, ch, sta, stn, displayed station, station displayed, station m |
| region | region, station region, stationing region, equation region, station equation region, eq region |
| side | side, side of road, left right, l r, lr, hand |
| offset | offset, offset m, centreline offset, centerline offset, offset from centreline, offset from cl, cl offset, distance from centreline, dist from cl |
| mounting height | mounting height, mounting height mm, mounting height m, mount height, installation height, height above ground |
| size | size, size mm, sign size, nominal size, x height, xheight, x height mm, text height |
| legend | legend, text, legend text, sign text, message, wording, destinations, destination |
| rotation | rotation, rotation deg, facing, facing deg, orientation, angle |
| notes | notes, note, remarks, remark, comments, comment |

The list is `sign_schedule::SYNONYMS`. Required fields: mark, code, alignment, station, side, offset.
`plan_import` fails with `PlanError::MissingColumns` if the mapping leaves one unmapped or absent
from the file; every other problem is a row issue.

## Import is a plan, then an apply

```rust
let ctx = PlanContext {
    catalog: engine.catalog(),                       // SignEngine::catalog()
    alignments: &alignments_in(&doc),                // or the app's alignment_glue::alignment_of
    existing: &existing_signs(&doc),
    traffic: afrobim_alignment::TrafficSide::Left,
};
let plan = plan_import(&table, &mapping, &ctx)?;     // changes nothing; no fonts needed
for line in plan.error_lines() { println!("{line}"); }          // "row 14: the mark is empty"
for dup in plan.duplicates() { /* already in the document: ask skip or update */ }

let report = apply_plan(&mut doc, &plan, &mut my_builder, DuplicatePolicy::Skip);
```

`ImportPlan.rows` has one `PlanRow` per data record, in file order, each `Valid(PlannedSign)` or
`Invalid(Vec<RowIssue>)`. Nothing is left out; comment and blank lines are listed separately, as
are file headers the mapping does not read. A `PlannedSign` holds the catalog key, the full
`SignDefinition` (the catalog's seed, then the parameters, then the legend, `code` set to the
mark, `location` set), the `PlacementRequest` (alignment, displayed station, region, side,
offset, and the internal station it resolved to), the `SitePlacement` the alignment gives (with
any rotation override), the element properties, the existing element with that mark if any, and
warnings (a size not among the pack's permitted sizes, a rotation override, an existing mark).

Row issues (`IssueKind`): `MissingValue`, `ExtraCells`, `DuplicateMarkInFile`, `UnknownCode`,
`AlignmentNotFound`, `BadNumber`, `BadStation`, `StationOutOfRange`, `StationAmbiguous` (the message
lists the regions), `RegionInvalid`, `BadSide`, `UnknownParam`, `BadValue`, `MissingRequiredParam`,
`LegendNotAccepted`, `PlacementFailed`. A row can carry several.

A mark already in the document makes the row valid with `existing: Some(id)`. `apply_plan` then
skips it (`DuplicatePolicy::Skip`, reported in `ApplyReport.skipped`) or updates the existing sign
(`Update`: definition, size, site placement and the schedule's four properties are replaced, a
cell left blank removes the property; the sheet position, layer, host and other properties stay).
Duplicates of a mark within the file are `DuplicateMarkInFile` on the later rows.

### What the app must supply to apply an import

1. The catalog: `SignEngine::catalog()` of the loaded rule pack, for the plan.
2. The alignments and existing signs: `alignments_in(&doc)` and `existing_signs(&doc)` do it from the document, or the app builds them.
3. A `SignBuilder`:
   * `measure(&mut self, &SignDefinition) -> Result<(f64, f64), String>`: the face's width and height in mm, from the sign engine's layout (`layout.drawing.scaled_to_origin(layout.sw_mm).extents`). An `Err` fails that row only.
   * `placed(&mut self, &mut Document, ElementId)` (optional): called after each created or updated sign, for what the app does after placing a sign (for example `afrobim_supports::sync_support_sites`).

`apply_plan` adds `sign` elements on layer `Signs` at `Document::next_free_origin`, named by the
mark, with the site placement and properties set. It does not undo; take the app's snapshot first.

## Limits

* Every row must be placed: alignment, station, side and offset are required. There is no unplaced sign.
* Stations are read as the drawings show them. Cross-fall is not applied to the level; `z` is the profile's centreline level (0 without a profile). The mounting height is recorded as a property; whether a sign's `z` is its lower edge is the app's decision (see the supports crate).
* `size_mm` is recorded and checked against the sizes the pack permits (a warning, not an error). The layout takes its size from the design speed, so it does not change the face.
* The legend carries name, direction, distance and route number. Destination symbols, route status and a proximity order other than the list position are not in the format; export lists them as lost.
* Parameters are the eleven listed. Anything else in a definition (embedded signs, for instance) is not set by a schedule and is lost on update.
* `code` resolves against the loaded pack only; a sign created by the app's own templates may have no catalog key and so exports with a blank code.
* Synonyms are an exact list. A header outside it is not read until the caller maps it.
* No decimal commas, no thousands separators, no unit conversion beyond `m` and `mm`.
* Traffic keeps to the left by default (UK and Uganda); pass `TrafficSide::Right` otherwise.
