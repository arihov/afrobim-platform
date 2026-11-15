//! Sign-face quantities (BoQ) and fabrication sheets: a Rust port of the C# reference
//! (`SignCore.Layout.SignQuantities`, `SignCore.Fabrication` and the `FabricationBatch` driver).
//!
//! - [`quantities`]: per-sign face measurement, the per-ground-colour summary and their CSVs.
//! - [`composer`]: one A3 fabrication sheet per sign (face at a standard scale, dimensions,
//!   sign data, colour schedule, title block) as a renderer-neutral [`FabricationSheet`].
//! - [`sheet_dxf`]: the sheet as an R12 DXF.
//! - [`pdf`]: the sheet as a one-page A3 PDF and the A4 sign-schedule report.
//! - [`sign_schedule`]: the sign schedule CSV (template, export, column mapping, import plan, apply).
//! - [`batch`]: writes all of the above for a list of signs into a folder.
//!
//! The CSVs and DXFs are byte-identical to the C# output for the same signs. The PDFs are the
//! same documents object for object once their streams are inflated; `tests/golden.rs` states
//! exactly what is compared and what is not.
//!
//! ```no_run
//! use std::path::Path;
//! use afrobim_fabrication::{FabricationOptions, PdfFonts, SheetSign};
//! use afrobim_signs::{Destination, Direction, SignDefinition, SignEngine};
//!
//! let engine = SignEngine::load(Path::new("packs/UK-TSM-2019.json"), Path::new("fonts")).unwrap();
//! let sign = SignDefinition::stack("ADS-001", 60, vec![Destination::new("Kasese", Direction::Ahead)]);
//!
//! // Export quantities.
//! let layout = engine.layout(&sign).unwrap();
//! let rows = vec![afrobim_fabrication::measure(&engine, &sign, &layout)];
//! std::fs::write("sign-quantities.csv", afrobim_fabrication::to_csv(&rows)).unwrap();
//! let summary = afrobim_fabrication::summarise(&rows);
//! std::fs::write("sign-quantities.summary.csv", afrobim_fabrication::summary_to_csv(&summary)).unwrap();
//!
//! // Export fabrication sheets.
//! let fonts = PdfFonts::from_folder(Path::new("fonts"), &engine).unwrap();
//! let result = afrobim_fabrication::run(&engine, &fonts, &[SheetSign::new(sign)], &FabricationOptions::new("sheets"));
//! println!("{} file(s), {} error(s)", result.files_written.len(), result.errors.len());
//! ```

pub mod batch;
pub mod composer;
pub mod num;
pub mod palette;
pub mod pdf;
pub mod quantities;
pub mod sheet;
pub mod sheet_dxf;
pub mod sign_schedule;

pub use batch::{run, safe_file_name, FabricationOptions, FabricationResult, SheetSign, SCHEDULE_FILE_NAME};
pub use composer::compose;
pub use pdf::{
    schedule_totals, write_schedule_pdf, write_schedule_pdf_stamped, write_sheet_pdf, write_sheet_pdf_stamped, PdfFonts, PdfStamp,
    ScheduleInfo, ScheduleRow, ScheduleTotals,
};
pub use quantities::{measure, measure_with, summarise, summary_to_csv, to_csv, MeasureOptions, QuantitySummaryRow, SignQuantity};
pub use sheet::{
    ColourRow, DimensionOrientation, FabricationSheet, SheetBox, SheetDimension, SheetInfo, SheetLine, SheetSwatch, SheetText,
    SheetTextAlign, TitleField,
};

/// The sheet as R12 DXF text.
pub fn write_sheet_dxf(sheet: &FabricationSheet) -> String {
    sheet_dxf::write(sheet)
}

#[derive(Debug, thiserror::Error)]
pub enum FabricationError {
    /// An option has a value that cannot be used.
    #[error("{0}")]
    InvalidOption(String),
    /// A font file could not be read or is not a TrueType font the PDF writer can embed.
    #[error("{0}")]
    Font(String),
}
