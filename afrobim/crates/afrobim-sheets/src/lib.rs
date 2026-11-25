// SPDX-License-Identifier: LicenseRef-AfroBIM-Proprietary
// Copyright (c) 2026 AfroBIM. All rights reserved.
//! Documentation stage: drawing sheets composed from the live model.
//!
//! A [`SheetSet`] is stored in the document (category `sheets`) and lists sheets: a [`Page`]
//! (ISO 216 A0 to A4, margins, frame), a data-driven [`TitleBlockLayout`], and [`ViewDef`]s. A
//! view is a plan ([`PlanView`]), a profile ([`ProfileView`]), cross-sections ([`SectionsView`]),
//! a [`Table`], a detail ([`DetailView`]) or [`Notes`]. Series (plan sheets every L metres with
//! overlap and match lines, a table over as many sheets as it needs, sections n to a sheet) expand
//! when the set is composed. [`compose_set`] reads the live document, so the sheets regenerate
//! when the model changes.
//!
//! The composed sheet ([`ComposedSheet`]) is a flat list of strokes, fills and texts in paper
//! millimetres. It is written as PDF ([`write_pdf`], vector, one page per sheet, real text) or DXF
//! ([`write_dxf_set`], paper millimetres, TEXT entities), or turned into an `afrobim_draw::Drawing`
//! with labels for a screen preview ([`ComposedSheet::to_drawing`]).
//!
//! Every line weight, text height, margin and layout size is entered by the user. The defaults
//! are neutral starting values and are labelled as defaults where they are defined; none is taken
//! from a drafting standard or from an organisation's title block.

mod afm;
pub mod annotate;
pub mod detail;
pub mod draw;
pub mod dxf_out;
pub mod geom;
pub mod item;
pub mod layout_plot;
pub mod model;
pub mod paper;
pub mod pdf;
pub mod plan;
pub mod profile;
pub mod project;
pub mod sections;
pub mod sheetset;
pub mod style;
pub mod table;
pub mod text;
pub mod titleblock;
pub mod view;

pub use annotate::{block_lines, layout_blocks, leader_pieces, percent_text, vertical_notes, NoteBlock, CurveNote, ProfileAnnotations, PviNote, TangentNote, VerticalNotes};
pub use detail::{compose_detail, DetailLabel, DetailSource, DetailView};
pub use draw::draw_drawing;
pub use dxf_out::{aci_for_rgb, dxf_layer_name, write_dxf, write_dxf_set};
pub use geom::{Aff, Fraction, Rect};
pub use item::{layers, ComposedSheet, Item, SheetDrawing, SheetLabel, SheetText};
pub use model::{station_text, stations_at_interval, Ctx, Model};
pub use paper::{round_up_1_2_5, trim_number, FrameStyle, Margins, Orientation, Page, PaperSize, Scale};
pub use pdf::{write_pdf, PRODUCER};
pub use plan::{compose_plan, match_lines_for, plan_transform, plan_windows, MatchLine, PlanRotation, PlanTransform, PlanView};
pub use profile::{auto_datum, compose_profile, ProfileMap, ProfileView};
pub use project::{
    project_info, project_info_id, set_project_info, ProjectInfo, ProjectInfoModule, Revision, UserField, PROJECT_INFO_CATEGORY, PROJECT_INFO_LAYER,
};
pub use sections::{compose_sections, pick_sections, SectionsView};
pub use sheetset::{
    add_sheet_set, compose_set, compose_set_with, compose_sheet, expand, load_sheet_set, sheet_sets, store_sheet_set, table_sheet, ComposedSet,
    ExpandedSheet, Numbering, PlanSeries, SectionSeries, SheetSet, SheetSource, SheetSpec, SheetsModule, TableSeries, SHEETS_CATEGORY, SHEETS_LAYER,
};
pub use style::{Pen, PenTable, Styles, TextHeights};
pub use table::{
    activity_table, clash_table, derived_values_table, draw_notes, draw_table_in, layout_table, parse_csv, priced_bill_table, quantity_table, rows_that_fit,
    sign_schedule_table, Notes, QuantityRow, Table, TableLayout,
};
pub use text::{text_width, wrap, Align, CAP_HEIGHT};
pub use titleblock::{
    draw_title_block, north_arrow, round_down_1_2_5, scale_bar, Block, Cell, CellContent, FieldKey, RevColumn, RevField, Row, SheetFields, TitleBlockLayout,
    TitlePlacement, TitleValues,
};
pub use view::{compose_view, module_quantities, ViewDef, ViewKind};
