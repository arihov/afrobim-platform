pub mod composer;
pub mod num;
pub mod palette;
pub mod quantities;
pub mod sheet;
pub mod sheet_dxf;

pub use composer::compose;
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
    /// A file or folder could not be written.
    #[error("{path}: {source}")]
    Io {
        path: std::path::PathBuf,
        #[source]
        source: std::io::Error,
    },
}
