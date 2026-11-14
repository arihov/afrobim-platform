# afrobim-sheets

Documentation stage for AfroBIM: drawing sheets composed from the live model, written as PDF
and DXF, previewable on screen.

A **sheet set** is stored in the document as one element of category `sheets`. It lists sheets
(or series of sheets). Each sheet has a page (ISO 216 A0 to A4, orientation, margins, frame), a
data-driven title block and **views**: plan, profile, cross-sections, table, detail, notes.
`compose_set` reads the live document, so sheets regenerate when the model changes.

Nothing here is a drafting standard. Every margin, line weight, text height, scale, interval and
title block size is entered by the user. The defaults below are neutral starting values, labelled
as defaults in the code, and none is taken from a standard or from any organisation's title block.

## Conventions

- Sheet space is paper millimetres, Y up, origin at the paper's lower left corner.
- Model space follows the rest of AfroBIM: plan coordinates in metres in the alignment crate
  (X east, Y north), element drawings in millimetres placed by `SitePlacement` (millimetres).
- Scale 1:`n`: 1 m on the ground is `1000 / n` mm on paper (`Scale::paper_mm_per_m`).
- A plan point `p` lands on paper at `R(theta) (p - ref) k + c` with `k = 1000 / n`; `ref` is
  chosen so the middle of the turned centreline's box lands on the middle of the viewport.
- Profile: `x = plot.x0 + (station - from) kh`, `y = plot.y0 + (level - datum) kv`, with
  `kh = 1000 / n` and `kv = kh * exaggeration`.
- Text heights are capital heights in paper mm. The PDF font size is `height / 0.718`.
- Stations on plans and profiles are **displayed** stations (station equations honoured):
  ticks and labels stand on whole multiples of the interval in each region.

## Pieces

| Part | Where |
|---|---|
| Paper sizes (derived from A0 = 1 m2, ratio 1 : root 2, halving rule; nothing typed from a table) | `PaperSize`, `Page`, `Margins`, `FrameStyle` |
| Scale 1:n | `Scale` |
| Title block layout as data (rows and cells with sizes, revision table, logo rectangle) | `TitleBlockLayout`, `Block`, `Row`, `Cell`, `FieldKey` |
| Project information stored in the document | `ProjectInfo`, `set_project_info`, `project_info` (category `project_info`) |
| North arrow, scale bar | `north_arrow`, `scale_bar` |
| Plan view, plan series with match lines | `PlanView`, `PlanSeries`, `plan_windows` |
| Profile view and its vertical geometry notes | `ProfileView`, `ProfileAnnotations`, `vertical_notes` |
| Cross-sections in a grid | `SectionsView`, `SectionSeries` |
| Tables, wrapping, CSV, series over several sheets | `Table`, `TableSeries`, `*_table` helpers |
| Detail of any `Drawing` | `DetailView` |
| Notes | `Notes` |
| Pens and text heights | `Styles`, `PenTable` (layer to pen), `TextHeights` |
| Output | `ComposedSet::pdf`, `ComposedSet::dxf`, `ComposedSheet::to_drawing` |
| Persistence | `add_sheet_set`, `store_sheet_set`, `load_sheet_set`, `SheetsModule` |

### Defaults (all editable)

Margins 10 mm on every side; frame is one rectangle at the margins; A3 landscape; title block
column 70 mm wide at the right with a 5-row revision table, a logo rectangle, project, client,
contract number, drawing title, drawing number, scale / revision / date, drawn / checked /
approved, sheet n of m; text heights title 5, heading 3.5, normal 2.5, label 2.0, small 1.8 mm;
pen weights from 0.1 mm (grid) to 0.5 mm (frame, profile design line), hairlines 0.13 mm;
plan ticks every 20 m, labels every 100 m; profile grid 100 m by 5 m.

## Usage

Compose and write a set (the app already holds the document and the module registry):

```rust
use afrobim_sheets::*;

let model = Model { doc: &doc, registry: &registry };

// 1. Project information (stored in the document, one element).
set_project_info(&mut doc, &ProjectInfo { project: "Rural road".into(), client: "Client".into(), ..Default::default() });

// 2. A plan series: A3, 1:1000, 300 m a sheet, 30 m overlap, numbered RD-PL-001...
let plan = PlanView::new("MC1", 0.0, 1.0, Scale(1000.0)); // range is replaced per sheet
let template = SheetSpec::new("PLAN", Page::new(PaperSize::A3, Orientation::Landscape), vec![ViewDef::full(ViewKind::Plan(plan))]);
let mut set = SheetSet::new("Road sheets");
set.numbering = Numbering { prefix: "RD-PL-".into(), ..Numbering::default() };
set.sources.push(SheetSource::PlanSeries(PlanSeries { template, plan_view: 0, from_station_m: None, to_station_m: None, length_m: 300.0, overlap_m: 30.0 }));

// 3. A profile with a data band and the ground from a surface element, 1:2000, vertical x10.
let mut profile = ProfileView::new("MC1", 0.0, 600.0, Scale(2000.0), 10.0);
profile.surface = Some("Ground".into());
set.sources.push(SheetSource::Single(SheetSpec::new("PROFILE", Page::default(), vec![ViewDef::full(ViewKind::Profile(profile))])));

// 4. A schedule from CSV text the engines already write; it continues over as many sheets as it needs.
let table = priced_bill_table(&bill_csv);
set.sources.push(SheetSource::TableSeries(TableSeries { template: SheetSpec::new("BILL OF QUANTITIES", Page::new(PaperSize::A4, Orientation::Landscape), vec![ViewDef::full(ViewKind::Table(table))]), table_view: 0 }));

// Store it in the document; it regenerates from the live model whenever it is composed.
let id = add_sheet_set(&mut doc, "Road sheets", &set);

// Compose and write.
let composed = compose_set(&model, &load_sheet_set(&doc, id).unwrap());
for w in &composed.warnings { eprintln!("{w}"); }          // what did not fit
std::fs::write("sheets.pdf", composed.pdf("Road sheets")).unwrap();
std::fs::write("sheets.dxf", composed.dxf(&set.styles, 50.0)).unwrap();

// On-screen preview of one sheet: a Drawing in paper mm plus its texts.
let preview = composed.sheets[0].to_drawing(); // preview.drawing, preview.labels
```

A detail view of a Gantt chart (units of the Gantt drawing are already sheet-sized, so 1:1 with
unit 1; `fit` picks a scale that fits instead):

```rust
let g = afrobim_schedule::gantt_drawing(&programme, &schedule, &Default::default());
let labels = g.labels.iter().map(|l| DetailLabel::new(l.at, &l.text)).collect();
let mut view = DetailView::inline(g.drawing, labels, Scale(1.0), 1.0, "CONSTRUCTION PROGRAMME");
view.fit = true;
```

Tables from other sources: `sign_schedule_table(csv)`, `priced_bill_table(csv)`,
`activity_table(csv)`, `clash_table(csv)`, `quantity_table(title, &rows)`, and
`module_quantities(&model, title, Some("marking"), &["plan_origin_x_m"])` for the values modules
report. Numeric columns are right aligned; `Table::with_widths` fixes column widths.

`Table::feed` and `DetailView::feed` are free names for the application: it fills such a table or
inline drawing from live data just before composing. `SheetSpec::number_prefix` gives the sheets of
one source their own numbering prefix, counted from the set's start number.

A user-defined title block is a `TitleBlockLayout` value assigned to `SheetSpec::title_block`
(it serialises). Register the modules `SheetsModule` and `ProjectInfoModule` so the elements show
in the app.

## Outputs

- **PDF**: PDF 1.4, one page per sheet, vector strokes and fills, text as real text in the
  standard Helvetica and Helvetica-Bold faces (nothing embedded, WinAnsi, so Latin-1 text and a
  few punctuation marks; other characters print as `?`). No date or id is written, so output is
  reproducible. The fabrication crate's PDF writer was not reused: it is tied to its sign sheet
  type and to TrueType fonts from a rule pack and exposes no general page API.
- **DXF**: R12 in paper millimetres, one drawing unit per mm on the printed sheet; sheets of a
  set are laid side by side with a gap. Text is TEXT entities (added to `afrobim-dxf`:
  `DxfText`, `add_text`, `add_polyline`, `add_line`; existing output is unchanged when they are
  not used). The CAD "txt" font is used, which is narrower or wider than Helvetica, so text
  that just fits in the PDF may not just fit in CAD.
- **Preview**: `ComposedSheet::to_drawing` gives `Primitive`s (strokes as `Line` of pen width,
  fills as `Fill`, fills with holes as `Glyphs`) and `SheetLabel`s for the texts.

## Limits and not done

- R12 DXF has no lineweights (a pen width is written as polyline width), no hatch (a filled
  region is written as its outline), no MTEXT (one TEXT per line) and no layouts or viewports
  (the sheet is drawn at 1:1 in drawing space, not as a paper-space layout with model viewports).
  Colours snap to the nearest AutoCAD colour index.
- Text widths use Helvetica metrics for wrapping and alignment in both outputs.
- Plan view: elements are drawn only if they have a `SitePlacement`, a module is registered for
  them, and their category is not excluded. Signs are drawn as a footprint line, not their face.
  Surfaces are excluded by default. A view is clipped at the viewport; the alignment is warned
  about, not shrunk, when it does not fit the scale. Match lines are straight across the
  alignment at the middle of the overlap and so are skewed on a curve. Station labels are not
  de-conflicted; element marks are placed at the middle of the visible part of the element and
  skipped when they would overlap a mark already placed.
- Profile: a design line sampled every millimetre of paper, PVI stations included. The vertical
  geometry is annotated (`annotate.rs`, `ProfileView::annotations`): per PVI in range a block
  above the plot (station, level and, on a curve, length L, grade change A, K = L / |A|, start and
  end stations) staggered into rows with a leader down to the PVI, a grade in percent along the
  straight part of each tangent, and ticks at the curve ends. K is a computed number only: no
  limit, no pass or fail. For a circular curve L is the horizontal length. A grade label that
  does not fit along its tangent at the scale is left off with a warning. PVI blocks use PVIs
  inside the station range only. No existing ground without a named surface. The data band shows
  station, design level and (with a surface) ground level only.
- Cross-sections: station, cut and fill areas, daylight and ground labels; finished point
  labels are off by default because they crowd. Sections come from `corridor_of`, which rebuilds
  the corridor (and decodes its surface) each time a sections sheet is composed.
- Tables do not merge cells or span columns. A table row taller than a sheet is cut.
- Layer visibility: plan views draw hidden layers too (markings and corridors start hidden on the
  sign design sheet); set `PlanView::only_visible_layers` to follow the document.
- Revision table text is shortened with `...` to its cell; it does not wrap.
- Not done: viewport-in-viewport, images (the logo is a placeholder rectangle), dimensions,
  hatches, revision clouds, grid references on the frame, A-series only (no B or ANSI sizes).
  The app has the dialogs for project information, sheet sets, the title block and a preview
  (see the AfroBIM user manual).

## Tests

`cargo test -p afrobim-sheets`. Unit tests cover paper sizes (computed from the definition),
clipping and affine maths with hand-worked numbers in comments, plan windows (cover the range
with the entered overlap and no gap), title block fields and "sheet n of m", table wrapping and
pagination. Integration tests build a road (alignment with profile and station equation, ground
surface, corridor, markings), compose plan, profile, sections, table and detail sheets, write
them to PDF and read the structure back (cross-reference offsets, page count, inflated streams),
and, when present on the machine, check the PDF with `pdfinfo`, `pdftotext` and `qpdf`, and the
DXF with `ezdxf` (counts of TEXT and POLYLINE entities, text content). The tool checks are
skipped with a message when Python, ezdxf or poppler are missing. Set `AFROBIM_SHEETS_SHOTS` to a
folder to keep the PDFs, DXFs and PNGs the tests render.
