//! The standard a marking is taken from: the UK Traffic Signs Manual Chapter 5 (2018) or the
//! Uganda Traffic Signs Manual (2004).
//!
//! An element with no basis recorded is UK, as before. Under the Uganda basis a type the Uganda
//! manual does not define is **not defined**: the lookup says so, and the caller may choose the
//! UK value explicitly, in which case the choice is written into the element and its output.

use serde::{Deserialize, Serialize};

/// The edition of the UK manual all UK dimensions are read from (Chapter 5, 2018).
pub use crate::catalogue::EDITION as UK_EDITION;

/// The Uganda manual, as its own title pages give it.
pub const UGANDA_EDITION: &str =
    "Republic of Uganda, Ministry of Works, Housing and Communications, Road Development Program Phase 2, Traffic Signs Manual Volume 1 and Volume 2 (Appendices), TRL Limited, July 2004 (Contract RDP/GN/S008)";

/// The standard a marking is taken from.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Default, Serialize, Deserialize)]
pub enum Basis {
    /// UK Traffic Signs Manual Chapter 5 (2018) and, where the chapter refers to a diagram it
    /// does not dimension, TSRGD 2016. Speeds in mph.
    #[default]
    Uk,
    /// Uganda Traffic Signs Manual Volumes 1 and 2 (July 2004). Speeds in km/h.
    Uganda,
}

impl Basis {
    pub const ALL: [Basis; 2] = [Basis::Uk, Basis::Uganda];

    pub fn label(self) -> &'static str {
        match self {
            Basis::Uk => "UK TSM Chapter 5 (2018)",
            Basis::Uganda => "Uganda TSM (2004)",
        }
    }

    pub fn edition(self) -> &'static str {
        match self {
            Basis::Uk => UK_EDITION,
            Basis::Uganda => UGANDA_EDITION,
        }
    }

    /// The unit of the speed values that basis's tables are written in.
    pub fn speed_unit(self) -> &'static str {
        match self {
            Basis::Uk => "mph",
            Basis::Uganda => "km/h",
        }
    }
}

/// That the chosen basis does not define something.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NotDefined {
    pub basis: Basis,
    /// What was asked for, e.g. `"warning line (UK diagram 1004)"`.
    pub what: String,
    /// True where the thing is not defined in this crate's catalogue of that basis, not
    /// necessarily in the manual itself (the UK chapters not read, such as Chapter 6).
    pub catalogued_elsewhere: bool,
}

impl NotDefined {
    /// `"warning line (UK diagram 1004): not defined in the Uganda manual"`.
    pub fn message(&self) -> String {
        match self.basis {
            Basis::Uganda => format!("{}: not defined in the Uganda manual", self.what),
            Basis::Uk if self.catalogued_elsewhere => format!("{}: not catalogued for the UK basis in this crate (the UK manual may define it in a chapter not read)", self.what),
            Basis::Uk => format!("{}: not defined in the UK manual", self.what),
        }
    }
}

impl std::fmt::Display for NotDefined {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.message())
    }
}

/// The result of a catalogue lookup under a basis. There is no silent fall back: a type the
/// basis does not define is [`Lookup::NotDefined`].
#[derive(Debug, Clone, PartialEq)]
pub enum Lookup<T> {
    Found(T),
    NotDefined(NotDefined),
}

impl<T> Lookup<T> {
    pub fn found(self) -> Option<T> {
        match self {
            Lookup::Found(t) => Some(t),
            Lookup::NotDefined(_) => None,
        }
    }

    pub fn is_defined(&self) -> bool {
        matches!(self, Lookup::Found(_))
    }

    pub fn into_result(self) -> Result<T, NotDefined> {
        match self {
            Lookup::Found(t) => Ok(t),
            Lookup::NotDefined(n) => Err(n),
        }
    }
}
