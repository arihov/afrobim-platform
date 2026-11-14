//! Luminaire photometric files: IES LM-63 and EULUMDAT, read into one [`Photometry`] model.
//!
//! Neither format's standard text is held. Each field below rests on the descriptions named,
//! and every field was then checked against real files (see the README for the files and the
//! results). Nothing here is written from memory alone.
//!
//! # IES LM-63 (`.ies`)
//!
//! | Field | Rests on |
//! |---|---|
//! | Version line (none = 1986, `IESNA91`, `IESNA:LM-63-1995`, `IESNA:LM-63-2002`) and keyword lines `[KEY] value` before `TILT=` | Radiance forum, "Overview of the IES file format" (R. Fritz, 5 Nov 2021, discourse.radiance-online.org/t/5749): versions, label lines, 1986 has no tag; AGi32 Photometric Toolbox, "IESNA LM-63 Format" page (keyword scheme, line order); luxpy `io_lid_files.py` (version table) |
//! | `TILT=NONE`, `TILT=INCLUDE` (four records: lamp-to-luminaire geometry, n, n angles, n factors), `TILT=<file>` | AGi32 "IESNA LM-63 Format" page (the `@` lines); Radiance forum overview ("exactly four records; the second gives the number of values in the third and fourth"). `INCLUDE` data are skipped with a warning; `TILT=<file>` is refused |
//! | Number of lamps, lumens per lamp (-1 = absolute photometry), candela multiplier, number of vertical angles, number of horizontal angles, photometric type, units type, width, length, height | Autodesk 3ds Max help, "IES Standard File Format" (LM-63-1991: lumens or -1 for absolute, multiplier "for all candela values", units 1 feet 2 metres, angle ranges); Lambda Research TracePro KB, "IES File format" (items 3.4 to 3.13 of LM-63-95: 1 = type C, 1 feet 2 metres); AGi32 page (order) |
//! | Ballast factor, ballast-lamp photometric factor (the "future use" field from 1995), input watts | AGi32 page (order); luxpy reader; Radiance overview (the 2019 edition changes the meaning of the unused field). The ballast factor is reported and applied only if asked ([`IesOptions::apply_ballast_factor`]): no retrievable source stating that LM-63 wants it applied to the candela values was found |
//! | Vertical angles 0 to 90, 90 to 180 or 0 to 180, first angle 0 | Autodesk page. Intensity outside the tabulated range is zero (this crate's reading) |
//! | Horizontal angles: one angle = rotationally symmetric; 0 to 90 = symmetric about both planes; 0 to 180 = about the 0-180 plane; more than 180 = no symmetry; 90 to 270 = about the 90-270 plane | Autodesk page (first three and "greater than 180"); AGi32 "Photometric File Type C - Concepts" (bilateral symmetry "when the luminaire is tested such that it is symmetrical about the 90-270 photometric plane"); luxpy `_complete_ies_lid` |
//! | Candela values: one record per horizontal angle, one value per vertical angle, in candela | Autodesk page. Intensity is the file's value times the candela multiplier |
//! | Photometric type | 1 = type C (TracePro KB). Anything else is refused: types B and A are not handled |
//!
//! # EULUMDAT (`.ldt`)
//!
//! | Field | Rests on |
//! |---|---|
//! | Lines 1 to 26 (company, Ityp, Isym, Mc, Dc, Ng, Dg, report, name, number, file, date, luminaire dimensions, luminous area, DFF, LORL, conversion factor, measured tilt, number of lamp sets) | AGi32 Photometric Toolbox, "EULUMDAT File Format" (table, citing Stockmar 1990 and 1998); Paul Bourke, "ldt EULUMDAT File Format" (April 2021, the same table); DIALux evo knowledge base, "Description of the EULUMDAT format" (16 Jul 2021) |
//! | Lamp sets: number of lamps (negative = absolute photometry), type, total luminous flux, colour, CRI, wattage | AGi32 table lines 26a to 26f. Real files (including one with two sets) lay these out set by set, six lines per set; the flux of the sets is added |
//! | Direct ratios (10 values), C angles (Mc), gamma angles (Ng), intensities in cd per 1000 lm | AGi32 table lines 27 to 30 |
//! | Planes stored per symmetry: Isym 0 all Mc; 1 one; 2 planes C0 to C180 (Mc/2+1); 3 planes C270 through C0 to C90 (Mc/2+1, starting at list entry 3Mc/4+1); 4 planes C0 to C90 (Mc/4+1) | DIALux evo knowledge base table for line 30 (Mc1, Mc2) and its note that for Isym 3 the intensities start at C270 |
//! | Intensity in candela = value x conversion factor x total lamp flux / 1000 | AGi32 table (values in cd/1000 lm; for absolute photometry the flux field is the luminaire's flux). Checked by integrating real files against the stated light output ratio |
//! | Measured tilt (line 25) | AGi32 table; CEN EN 13032-1:2004+A1:2012 clause 4.2.3 (public preview): the C-plane system is fixed in space and does not follow a tilt of the luminaire during measurement; the tilt must be declared |
//!
//! The reader accepts the free number layout many files use after the lamp sets (several values
//! per line) but needs the first 26 lines and the lamp sets line by line, as the specification
//! has them.
//!
//! # Orientation
//!
//! Where the C-planes point on site is a convention, set by [`crate::calc::Orientation`]; see
//! there.

mod checks;
mod ies;
mod ldt;
mod model;
mod sha256;

pub use checks::{FluxCheck, PeakIntensity};
pub use ies::{parse_ies, IesOptions};
pub use ldt::parse_ldt;
pub use model::{Photometry, SourceFormat};
pub use sha256::sha256_hex;

use std::path::Path;

/// Why a photometric file could not be read.
#[derive(Debug, Clone, PartialEq, thiserror::Error)]
pub enum PhotometryError {
    /// The file could not be opened or read.
    #[error("the photometric file cannot be read: {0}")]
    Io(String),
    /// The content does not follow the format.
    #[error("{0}")]
    Parse(String),
    /// A valid file of a kind this crate does not handle (for example type A or B photometry).
    #[error("{0}")]
    Unsupported(String),
}

/// A distribution read from a file, with the identity of the file it came from.
#[derive(Debug, Clone, PartialEq)]
pub struct LoadedPhotometry {
    pub photometry: Photometry,
    /// SHA-256 of the file's bytes, lower-case hexadecimal.
    pub sha256: String,
    /// The file name without its directory.
    pub file_name: String,
}

fn decode(bytes: &[u8]) -> String {
    match std::str::from_utf8(bytes) {
        Ok(s) => s.to_string(),
        // Photometric files are often Windows-1252 or Latin-1: map each byte to its code point.
        Err(_) => bytes.iter().map(|b| *b as char).collect(),
    }
}

/// Reads a photometric file's bytes. The format follows the extension (`.ies`, `.ldt`, any
/// case); with another extension a `TILT=` line means IES and otherwise EULUMDAT.
pub fn parse_photometry_bytes(file_name: &str, bytes: &[u8]) -> Result<LoadedPhotometry, PhotometryError> {
    let text = decode(bytes);
    let ext = Path::new(file_name).extension().and_then(|e| e.to_str()).map(|e| e.to_ascii_lowercase()).unwrap_or_default();
    let is_ies = match ext.as_str() {
        "ies" => true,
        "ldt" => false,
        _ => text.lines().any(|l| l.trim_start().to_ascii_uppercase().starts_with("TILT")),
    };
    let photometry = if is_ies { parse_ies(&text, IesOptions::default())? } else { parse_ldt(&text)? };
    Ok(LoadedPhotometry { photometry, sha256: sha256_hex(bytes), file_name: file_name.to_string() })
}

/// Reads a photometric file from disk.
pub fn load_photometry_file(path: &Path) -> Result<LoadedPhotometry, PhotometryError> {
    let bytes = std::fs::read(path).map_err(|e| PhotometryError::Io(format!("{}: {e}", path.display())))?;
    let name = path.file_name().map(|n| n.to_string_lossy().to_string()).unwrap_or_default();
    parse_photometry_bytes(&name, &bytes)
}

#[cfg(test)]
pub(crate) mod testdata;
#[cfg(test)]
mod tests;
