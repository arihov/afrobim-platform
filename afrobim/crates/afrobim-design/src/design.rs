//! A whole alignment design (horizontal plus optional vertical) and the `afrobim-alignment`
//! [`Alignment`] it produces.

use afrobim_alignment::{format_station, Alignment};
use serde::{Deserialize, Serialize};

use crate::error::DesignError;
use crate::horizontal::{HorizontalDesign, HorizontalSolution};
use crate::landxml::{write_landxml, LandXmlOptions};
use crate::vertical::{VerticalDesign, VerticalSolution};

/// The inputs of a designed alignment.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct AlignmentDesign {
    pub horizontal: HorizontalDesign,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub vertical: Option<VerticalDesign>,
}

/// A built design: the alignment (with its profile when there is a vertical design) and the
/// worked-out data behind it.
#[derive(Debug, Clone, PartialEq)]
pub struct DesignedAlignment {
    /// Identical in kind to an imported alignment: every module that takes an `Alignment` works on it.
    pub alignment: Alignment,
    pub horizontal: HorizontalSolution,
    pub vertical: Option<VerticalSolution>,
    pub warnings: Vec<String>,
}

impl AlignmentDesign {
    pub fn new(horizontal: HorizontalDesign, vertical: Option<VerticalDesign>) -> AlignmentDesign {
        AlignmentDesign { horizontal, vertical }
    }

    pub fn name(&self) -> &str {
        &self.horizontal.name
    }

    /// Solves both parts and joins them. Fails with every reason if either is impossible.
    ///
    /// A warning is added when the profile does not cover the whole alignment: the corridor and
    /// the sheets need a level at every station.
    pub fn build(&self) -> Result<DesignedAlignment, DesignError> {
        let h = self.horizontal.solve()?;
        let v = self.vertical.as_ref().map(|v| v.solve()).transpose()?;
        let mut alignment = h.alignment.clone();
        let mut warnings = h.warnings.clone();
        if let Some(v) = &v {
            warnings.extend(v.warnings.iter().cloned());
            if let Some((a, b)) = v.profile.range() {
                let (s0, s1) = (alignment.start_station_m, alignment.end_station_m());
                if a > s0 + 1e-6 {
                    warnings.push(format!("The profile starts at station {} but the alignment starts at {}: there is no level for the first {:.3} m.", format_station(a, 3), format_station(s0, 3), a - s0));
                }
                if b < s1 - 1e-6 {
                    warnings.push(format!("The profile ends at station {} but the alignment ends at {}: there is no level for the last {:.3} m.", format_station(b, 3), format_station(s1, 3), s1 - b));
                }
            }
            alignment.profile = Some(v.profile.clone());
        }
        Ok(DesignedAlignment { alignment, horizontal: h, vertical: v, warnings })
    }

    /// LandXML 1.2 text of the designed alignment and profile.
    pub fn to_landxml(&self, options: &LandXmlOptions) -> Result<String, DesignError> {
        Ok(write_landxml(&self.build()?.alignment, options))
    }
}
