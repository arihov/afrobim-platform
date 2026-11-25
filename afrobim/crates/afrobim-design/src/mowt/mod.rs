// SPDX-License-Identifier: LicenseRef-AfroBIM-Proprietary
// Copyright (c) 2026 AfroBIM. All rights reserved.
//! The geometric design limits of the Uganda Ministry of Works and Transport, **Road Design Manual
//! Volume I: Geometric Design, January 2010**, as typed lookups and as a built-in [`DesignBasis`].
//!
//! # Source
//!
//! Document: Ministry of Works and Transport, Entebbe, "Road Design Manual Volume I: Geometric
//! Design" (running header "Geometric Design Manual"). Edition, from the document itself: the
//! preamble (printed page i, PDF page 1) is signed "Ministry of Works and Transport, P.O. Box 10,
//! Entebbe, January 2010" and says the Manual "is part of the revised Road Design Manual, November
//! 1994"; section 1.1.1 (printed page 1) says it supersedes the Road Design Manual of November
//! 1994. The copy has no title page and no foreword; the PDF metadata gives a creation date of
//! 3 May 2011. The copy used was a third-party one hosted by the Uganda Association of Consulting
//! Engineers (`https://uace.or.ug/wp-content/uploads/2023/03/Volume-1-Geometric-Design-Manual.pdf`),
//! not the Ministry's own site, fetched 2026-11-15.
//!
//! Every value in this module is read from that PDF and carries its table or figure number, its
//! section, its **printed** page and its **PDF** page (printed page + 9 throughout). Nothing is
//! taken from memory, from another standard or from a derivation: where the manual prints a table
//! value, the table value is used, even where it differs from the crate's first-principles relation
//! (for example the manual's `127` in `R = V^2 / (127 (e + f))` against the `12.96 g = 127.10` of
//! [`crate::physics`]).
//!
//! # Rules every lookup follows
//!
//! * The result is a [`Cited`] value: the number, its unit as printed, and the [`Source`].
//! * Outside the tabulated range a lookup **refuses** with a plain message ([`DesignError::Input`]).
//!   Nothing is extrapolated. Nothing is interpolated: the manual states no interpolation rule for
//!   any of its tables.
//! * A cell the manual prints as a dash is refused ("the manual gives no value").
//! * A cell whose printed value contradicts another table of the same manual is **withheld** and
//!   refused with the reason (see [`known_slips`]); the owner decides which figure governs.
//!
//! # Modules
//!
//! | module | contents |
//! |---|---|
//! | [`classes`] | road design classes, traffic ranges, design speeds by terrain (Tables 4-2a, 4-2b), the class parameter tables (4-3 to 4-9) |
//! | [`horizontal`] | minimum radius, `e` and `f` (6-1), relative gradient (6-2), runoff (6-3), superelevation rates (6-4 to 6-9), widening (6-10), spirals, sight distance on curves |
//! | [`vertical`] | crest and sag `K` (6-11 to 6-13), their formulas, gradients (6-14, 6-15), critical length (6-16), climbing lanes (6-17, 6-18) |
//! | [`sight`] | stopping sight distance (6.19, 6.20), passing sight distance (6.21, 6.22), meeting sight distance |
//! | [`cross_section`] | lane, shoulder and edge strip widths, normal cross fall, headroom at crests |
//!
//! [`DesignBasis::mowt_2010`] builds a one-row basis that `run_checks` uses directly;
//! [`builtin_bases`] lists what the app can offer.

pub mod classes;
pub mod cross_section;
pub mod horizontal;
mod se;
mod se_data;
pub mod sight;
pub mod vertical;

mod basis;

use serde::Serialize;

pub use basis::{builtin_bases, known_slips, mowt_2010_row_label, not_checked_under_mowt_2010, BuiltinBasisInfo, BuiltinSelection, ClassInfo, GradeLimit, MowtOptions, NotCheckedInfo, Slip, TerrainInfo};
pub use classes::{RoadClass, Surface, Terrain};

use crate::error::DesignError;

/// Id of the built-in basis, used in [`builtin_bases`].
pub const MOWT_2010_ID: &str = "mowt_2010";

/// Short name of the document, used in every citation.
pub const DOCUMENT_SHORT: &str = "MoWT Road Design Manual Vol. I Geometric Design (January 2010)";

/// The document, edition and where the copy came from, in full.
pub const DOCUMENT_FULL: &str = "Uganda Ministry of Works and Transport, Road Design Manual Volume I: Geometric Design, January 2010 (preamble, printed p. i; part of the revised Road Design Manual, November 1994), 271-page PDF fetched 2026-11-15 from https://uace.or.ug/wp-content/uploads/2023/03/Volume-1-Geometric-Design-Manual.pdf, a copy hosted by the Uganda Association of Consulting Engineers (third party, not the Ministry's own site)";

/// Where in the manual a value stands.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub struct Source {
    /// Table or figure, for example `Table 6-1`, or `text` with the section naming the passage.
    pub what: &'static str,
    /// Section number, for example `6.2.3`.
    pub section: &'static str,
    /// Page number printed on the page.
    pub printed_page: u16,
    /// Page of the PDF file (printed page + 9 in this copy).
    pub pdf_page: u16,
}

impl Source {
    pub const fn new(what: &'static str, section: &'static str, printed_page: u16, pdf_page: u16) -> Source {
        Source { what, section, printed_page, pdf_page }
    }

    /// The citation as one line, for reports.
    pub fn citation(&self) -> String {
        format!("{DOCUMENT_SHORT}, {}, section {}, p. {} (PDF p. {})", self.what, self.section, self.printed_page, self.pdf_page)
    }

    /// A shorter citation: table or figure, printed page and PDF page.
    pub fn short(&self) -> String {
        format!("{} p. {} (PDF p. {})", self.what, self.printed_page, self.pdf_page)
    }
}

/// A value with its unit as printed and where it comes from.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Cited<T> {
    pub value: T,
    /// The unit as the manual prints it (`km/h`, `m`, `%`, `K`, ...).
    pub unit: &'static str,
    pub source: Source,
}

impl<T> Cited<T> {
    pub fn new(value: T, unit: &'static str, source: Source) -> Cited<T> {
        Cited { value, unit, source }
    }
}

pub(crate) fn refuse<T>(message: impl Into<String>) -> Result<T, DesignError> {
    Err(DesignError::Input(message.into()))
}

/// The design speeds the manual tabulates, km/h.
pub const SPEEDS_KMH: [f64; 10] = [30.0, 40.0, 50.0, 60.0, 70.0, 80.0, 90.0, 100.0, 110.0, 120.0];

/// Index of a design speed in [`SPEEDS_KMH`], or a refusal saying which speeds are tabulated.
pub(crate) fn speed_index(speed_kmh: f64) -> Result<usize, DesignError> {
    SPEEDS_KMH.iter().position(|&s| (s - speed_kmh).abs() < 1e-9).ok_or_else(|| {
        DesignError::Input(format!("The manual tabulates design speeds of 30, 40, 50, 60, 70, 80, 90, 100, 110 and 120 km/h; {speed_kmh} km/h is not one of them and nothing is interpolated."))
    })
}

/// A number in a table cell that is either a value or a dash (the manual gives no value).
#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) enum Entry {
    V(f64),
    Dash,
    /// The printed cell contradicts another table of the manual, so it is held back (reason).
    Withheld(&'static str),
}

impl Entry {
    pub(crate) fn get(self, what: &str, source: Source) -> Result<Cited<f64>, DesignError> {
        match self {
            Entry::V(v) => Ok(Cited::new(v, "", source)),
            Entry::Dash => refuse(format!("The manual prints a dash for {what} ({}): it gives no value for this case.", source.short())),
            Entry::Withheld(why) => refuse(format!("The printed value of {what} ({}) is withheld: {why}", source.short())),
        }
    }
}
