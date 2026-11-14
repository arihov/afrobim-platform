//! Pavement structure design checks, one sub-module per published method.
//!
//! Which method governs a project is the engineer's choice (or the client's); nothing here
//! chooses for them, and the methods are not interchangeable: each has its own definition of
//! design traffic and of subgrade strength, so the input of one is not the input of another.
//!
//! | [`DesignMethod`] | Module | Basis |
//! |---|---|---|
//! | `MowtRdmVol3` | [`mowt`] | Uganda MoWT *Road Design Manual Volume 3: Pavement Design, Part I: Flexible Pavements* (January 2010) |
//! | `DmrbCd226` | [`dmrb`] | UK DMRB CD 224 *Traffic assessment* (Revision 0, March 2020) and CD 226 *Design for new pavement construction* (Version 0.1.0, November 2021) |
//!
//! Every method returns its own typed result and implements [`DesignResult`], which gives the
//! common output: a line-by-line calculation with references, the warnings, and (where the
//! method arrives at one) a layer stack ready for a [`crate::PavementDefinition`]. A further
//! method (for example the South African SAPEM / TRH4) is added as another sub-module, another
//! `DesignMethod` and `DesignInput` variant, and nothing else changes.
//!
//! The result of any of them is a calculation for an engineer to check, not a certified design.

pub mod dmrb;
pub mod mowt;
mod mowt_catalogue;

use crate::model::Layer;

/// The published methods this crate can follow.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub enum DesignMethod {
    /// Uganda MoWT Road Design Manual Volume 3, Part I (January 2010): catalogue of flexible
    /// pavement structures by traffic class and subgrade class.
    MowtRdmVol3,
    /// UK Design Manual for Roads and Bridges: design traffic to CD 224 and the flexible
    /// pavement requirements of CD 226 that are stated as equations or tables.
    DmrbCd226,
}

impl DesignMethod {
    /// Every method available, in the order they are offered.
    pub fn all() -> &'static [DesignMethod] {
        &[DesignMethod::MowtRdmVol3, DesignMethod::DmrbCd226]
    }

    /// Short name for a menu.
    pub fn name(self) -> &'static str {
        match self {
            DesignMethod::MowtRdmVol3 => "Uganda MoWT Road Design Manual Vol. 3",
            DesignMethod::DmrbCd226 => "UK DMRB CD 224 / CD 226",
        }
    }

    /// The governing document, with edition.
    pub fn reference(self) -> &'static str {
        match self {
            DesignMethod::MowtRdmVol3 => {
                "Republic of Uganda, Ministry of Works and Transport, Road Design Manual Volume 3: Pavement Design, Part I: Flexible Pavements, January 2010"
            }
            DesignMethod::DmrbCd226 => {
                "UK Design Manual for Roads and Bridges, CD 224 Traffic assessment, Revision 0, March 2020, and CD 226 Design for new pavement construction, Version 0.1.0, November 2021 (both since superseded)"
            }
        }
    }
}

/// Why a design could not be run at all. Conditions a method itself excludes (traffic or
/// subgrade outside its range) are not errors: they come back in the result as warnings with no
/// structure, so that the calculation up to that point can still be read.
#[derive(Debug, Clone, PartialEq, thiserror::Error)]
pub enum DesignError {
    #[error("{0}")]
    InvalidInput(String),
}

/// What every method's result offers.
pub trait DesignResult {
    fn method(&self) -> DesignMethod;
    /// The calculation set out line by line with its references, for the engineer to check.
    fn calculation(&self) -> Vec<String>;
    /// Conditions the engineer must look at: limits of the method, doubts in the source,
    /// reminders the source attaches to the answer.
    fn warnings(&self) -> &[String];
    /// The structure as a layer stack, top first, when the method arrives at one.
    fn layers(&self) -> Option<Vec<Layer>>;
}

/// The input of one method.
#[derive(Debug, Clone, PartialEq)]
#[non_exhaustive]
pub enum DesignInput {
    Mowt(mowt::MowtInput),
    Dmrb(dmrb::DmrbInput),
}

impl DesignInput {
    pub fn method(&self) -> DesignMethod {
        match self {
            DesignInput::Mowt(_) => DesignMethod::MowtRdmVol3,
            DesignInput::Dmrb(_) => DesignMethod::DmrbCd226,
        }
    }
}

/// Runs the method the input belongs to.
pub fn design(input: &DesignInput) -> Result<Box<dyn DesignResult>, DesignError> {
    match input {
        DesignInput::Mowt(i) => Ok(Box::new(mowt::design(i)?)),
        DesignInput::Dmrb(i) => Ok(Box::new(dmrb::design(i)?)),
    }
}

/// A number for a calculation line: up to `decimals` places, trailing zeros removed.
pub(crate) fn num(v: f64, decimals: usize) -> String {
    crate::quantities::fixed(v, 0, decimals)
}
