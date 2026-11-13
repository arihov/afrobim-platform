//! Fabrication sheets for a list of signs: `<code>-sheet.dxf` (R12) and `<code>-sheet.pdf` per
//! sign, and `schedule.pdf` for the run. Port of C# `FabricationBatch.Run` (the entry point the
//! desktop app's *Fabrication sheets* command uses).
//!
//! The C# driver loads the rule pack and the fonts itself; here the caller passes the loaded
//! [`SignEngine`] and [`PdfFonts`], so a missing pack or font is the caller's error to report.

use std::path::{Path, PathBuf};

use afrobim_signs::{Severity, SignDefinition, SignEngine, SignError};

use crate::composer::{compose, type_name};
use crate::pdf::{write_schedule_pdf_stamped, write_sheet_pdf_stamped, PdfFonts, PdfStamp, ScheduleInfo, ScheduleRow};
use crate::sheet::SheetInfo;
use crate::sheet_dxf;

pub const SCHEDULE_FILE_NAME: &str = "schedule.pdf";

/// A sign to put on a fabrication sheet, with the schedule's optional quantity and location.
#[derive(Debug, Clone, PartialEq)]
pub struct SheetSign {
    pub sign: SignDefinition,
    pub quantity: Option<u32>,
    pub location: Option<String>,
}

impl SheetSign {
    pub fn new(sign: SignDefinition) -> Self {
        SheetSign { sign, quantity: None, location: None }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct FabricationOptions {
    /// The folder the files are written to; it is created when missing.
    pub out_dir: PathBuf,
    /// Title-block fields. A blank date becomes today's (UTC); the drawing number is
    /// `drawing_prefix` plus the sign's position in the run.
    pub sheet: SheetInfo,
    pub drawing_prefix: String,
    /// The stamp for every PDF of the run; `None` takes the current time. A fixed stamp makes
    /// the run reproducible byte for byte.
    pub stamp: Option<PdfStamp>,
}

impl FabricationOptions {
    pub fn new(out_dir: impl Into<PathBuf>) -> Self {
        FabricationOptions { out_dir: out_dir.into(), sheet: SheetInfo::default(), drawing_prefix: "FS-".to_string(), stamp: None }
    }
}

/// What a run did.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct FabricationResult {
    /// The C# batch's exit code: 0 clean; 5 when a sheet carries a layout error, a sign failed
    /// unexpectedly or nothing was written; 6 when a sign's kind is not supported.
    pub exit_code: i32,
    pub files_written: Vec<PathBuf>,
    /// Signs that could not be drawn and files that could not be written.
    pub errors: Vec<String>,
    /// One row per sheet written, as printed in the schedule.
    pub rows: Vec<ScheduleRow>,
    /// Layout diagnostics of severity error and warning across the sheets written.
    pub error_count: usize,
    pub warning_count: usize,
    /// Progress lines, as the C# batch logs them.
    pub log: Vec<String>,
}

/// Writes the sheets for `signs` into `options.out_dir` and returns a summary. A sign that
/// cannot be laid out or written is reported in the result and the run carries on.
pub fn run(engine: &SignEngine, fonts: &PdfFonts, signs: &[SheetSign], options: &FabricationOptions) -> FabricationResult {
    let mut result = FabricationResult::default();
    let stamp = options.stamp.clone().unwrap_or_else(PdfStamp::now);
    let date = if options.sheet.date.is_empty() { stamp.iso_date() } else { options.sheet.date.clone() };

    if let Err(e) = std::fs::create_dir_all(&options.out_dir) {
        result.errors.push(format!("{}: {e}", options.out_dir.display()));
        result.exit_code = 5;
        return result;
    }

    for (index, s) in signs.iter().enumerate() {
        let n = index + 1;
        let sign = &s.sign;
        let layout = match engine.layout(sign) {
            Ok(layout) => layout,
            Err(e @ SignError::Unsupported(_)) => {
                result.errors.push(format!("sign #{n}: {e}"));
                result.exit_code = 6;
                result.log.push(format!("sign #{n}: {e}"));
                continue;
            }
            Err(e) => {
                result.errors.push(format!("sign #{n}: unexpected error: {e}"));
                result.exit_code = 5;
                result.log.push(format!("sign #{n}: unexpected error: {e}"));
                continue;
            }
        };
        let info = SheetInfo { date: date.clone(), drawing_number: format!("{}{n:03}", options.drawing_prefix), ..options.sheet.clone() };
        let sheet = compose(engine, sign, &layout, &info);

        let stem = format!("{}-sheet", safe_file_name(&sign.code));
        let (dxf_path, pdf_path) = (options.out_dir.join(format!("{stem}.dxf")), options.out_dir.join(format!("{stem}.pdf")));
        let written = write_file(&dxf_path, sheet_dxf::write(&sheet).as_bytes())
            .and_then(|()| write_file(&pdf_path, &write_sheet_pdf_stamped(&sheet, fonts, &stamp)));
        if let Err(e) = written {
            result.errors.push(format!("sign #{n}: unexpected error: {e}"));
            result.exit_code = 5;
            result.log.push(format!("sign #{n}: unexpected error: {e}"));
            continue;
        }
        result.files_written.push(dxf_path);
        result.files_written.push(pdf_path);
        result.log.push(format!("sign #{n}: {} → {stem}.dxf/.pdf (1:{})", sign.code, sheet.scale));
        result.rows.push(ScheduleRow { q: sheet.quantity, type_name: type_name(sign), quantity: s.quantity, location: s.location.clone() });

        for d in &layout.diagnostics {
            result.log.push(format!("  [{:?}] {}: {}", d.severity, d.code, d.message));
            match d.severity {
                Severity::Error => {
                    result.error_count += 1;
                    result.exit_code = 5;
                }
                Severity::Warning => result.warning_count += 1,
                Severity::Info => {}
            }
        }
    }

    if result.rows.is_empty() {
        if result.exit_code == 0 {
            result.exit_code = 5;
        }
        return result;
    }
    let path = options.out_dir.join(SCHEDULE_FILE_NAME);
    let info = ScheduleInfo { project: options.sheet.project.clone(), date, standard: engine.pack_name().to_string() };
    match write_file(&path, &write_schedule_pdf_stamped(&result.rows, &info, fonts, &stamp)) {
        Ok(()) => {
            result.files_written.push(path);
            result.log.push(format!("schedule: {} sign(s) → {SCHEDULE_FILE_NAME}", result.rows.len()));
        }
        Err(e) => {
            result.errors.push(e.clone());
            result.exit_code = 5;
            result.log.push(e);
        }
    }
    result
}

fn write_file(path: &Path, bytes: &[u8]) -> Result<(), String> {
    std::fs::write(path, bytes).map_err(|e| format!("{}: {e}", path.display()))
}

/// A sign code as a file name: characters a file name cannot hold become `_`.
///
/// The C# batch replaces whatever `Path.GetInvalidFileNameChars()` lists on the machine it runs
/// on (only `/` and NUL on Linux). This replaces the Windows set everywhere, so a run names its
/// files the same on every platform.
pub fn safe_file_name(code: &str) -> String {
    code.chars().map(|c| if matches!(c, '"' | '<' | '>' | '|' | ':' | '*' | '?' | '\\' | '/') || (c as u32) < 32 { '_' } else { c }).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn file_names_lose_separators_and_reserved_characters() {
        assert_eq!(safe_file_name("R-670/60"), "R-670_60");
        assert_eq!(safe_file_name("A:B*C?\"<>|\\\t"), "A_B_C_______");
        assert_eq!(safe_file_name("UK06.1 – café"), "UK06.1 – café");
    }
}
