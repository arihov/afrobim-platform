// SPDX-License-Identifier: LicenseRef-AfroBIM-Proprietary
// Copyright (c) 2026 AfroBIM. All rights reserved.
//! The cross-section of an open drain and what it is lined with.

use afrobim_kernel::Note;
use serde::{Deserialize, Serialize};

use crate::design::mowt::ChannelMaterial;
use crate::design::ChannelGeometry;
use crate::{error_note, fixed, not_negative, positive};

/// The finished (water-side) cross-section of an open drain.
///
/// The governing manual defines no standard side-drain shapes, so these three are plain
/// geometry: all are the general trapezoid with bed width `b`, depth `d` and side slopes `zl`,
/// `zr` (horizontal to one vertical), with `b = 0` for a V and `zl = zr = 0` for a rectangle.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type")]
pub enum ChannelSection {
    Trapezoidal {
        bottom_width_mm: f64,
        depth_mm: f64,
        /// Left side slope looking up-station, horizontal to one vertical.
        left_side_slope: f64,
        right_side_slope: f64,
    },
    V {
        depth_mm: f64,
        left_side_slope: f64,
        right_side_slope: f64,
    },
    Rectangular {
        width_mm: f64,
        depth_mm: f64,
    },
}

impl ChannelSection {
    /// Width of the bed, mm (0 for a V).
    pub fn bottom_width_mm(&self) -> f64 {
        match *self {
            ChannelSection::Trapezoidal { bottom_width_mm, .. } => bottom_width_mm,
            ChannelSection::V { .. } => 0.0,
            ChannelSection::Rectangular { width_mm, .. } => width_mm,
        }
    }

    /// Depth from the top edges to the bed, mm.
    pub fn depth_mm(&self) -> f64 {
        match *self {
            ChannelSection::Trapezoidal { depth_mm, .. } | ChannelSection::V { depth_mm, .. } | ChannelSection::Rectangular { depth_mm, .. } => depth_mm,
        }
    }

    /// (left, right) side slopes, horizontal to one vertical.
    pub fn side_slopes(&self) -> (f64, f64) {
        match *self {
            ChannelSection::Trapezoidal { left_side_slope, right_side_slope, .. } | ChannelSection::V { left_side_slope, right_side_slope, .. } => {
                (left_side_slope, right_side_slope)
            }
            ChannelSection::Rectangular { .. } => (0.0, 0.0),
        }
    }

    /// Width between the top edges, mm: `b + (zl + zr) · d`.
    pub fn top_width_mm(&self) -> f64 {
        let (zl, zr) = self.side_slopes();
        self.bottom_width_mm() + (zl + zr) * self.depth_mm()
    }

    /// The section in metres, for the hydraulics.
    pub fn geometry(&self) -> ChannelGeometry {
        let (zl, zr) = self.side_slopes();
        ChannelGeometry { bottom_width_m: self.bottom_width_mm() / 1000.0, left_side_slope: zl, right_side_slope: zr }
    }

    /// A description of the entered dimensions, e.g. `Trapezoidal b500 d600 1.5:1/1.5:1`.
    pub fn label(&self) -> String {
        let f = |v: f64| fixed(v, 0, 2);
        match *self {
            ChannelSection::Trapezoidal { bottom_width_mm, depth_mm, left_side_slope, right_side_slope } => {
                format!("Trapezoidal b{} d{} {}:1/{}:1", f(bottom_width_mm), f(depth_mm), f(left_side_slope), f(right_side_slope))
            }
            ChannelSection::V { depth_mm, left_side_slope, right_side_slope } => {
                format!("V d{} {}:1/{}:1", f(depth_mm), f(left_side_slope), f(right_side_slope))
            }
            ChannelSection::Rectangular { width_mm, depth_mm } => format!("Rectangular b{} d{}", f(width_mm), f(depth_mm)),
        }
    }

    /// Error notes for dimensions that cannot be drawn or measured.
    pub fn issues(&self) -> Vec<Note> {
        let mut out = Vec::new();
        if !positive(self.depth_mm()) {
            out.push(error_note("SECTION_DEPTH_NOT_POSITIVE", format!("The drain depth must be greater than zero (it is {}).", self.depth_mm())));
        }
        let (zl, zr) = self.side_slopes();
        let slopes_ok = not_negative(zl) && not_negative(zr);
        if !slopes_ok {
            out.push(error_note("SECTION_SLOPE_NEGATIVE", format!("Side slopes must not be negative (they are {zl} and {zr}).")));
        }
        match self {
            ChannelSection::Trapezoidal { bottom_width_mm, .. } => {
                if !positive(*bottom_width_mm) {
                    out.push(error_note(
                        "SECTION_WIDTH_NOT_POSITIVE",
                        format!("The bed width of a trapezoidal drain must be greater than zero (it is {bottom_width_mm}); use a V section for no bed."),
                    ));
                }
            }
            ChannelSection::V { .. } => {
                if slopes_ok && zl + zr <= 0.0 {
                    out.push(error_note("SECTION_V_HAS_NO_WIDTH", "A V drain needs at least one side slope greater than zero.".to_string()));
                }
            }
            ChannelSection::Rectangular { width_mm, .. } => {
                if !positive(*width_mm) {
                    out.push(error_note("SECTION_WIDTH_NOT_POSITIVE", format!("The drain width must be greater than zero (it is {width_mm}).")));
                }
            }
        }
        out
    }
}

/// What the wetted faces of a drain are made of.
///
/// The manual's only classification of channel materials is its Table 9.1 (permissible velocity
/// by material), so those rows are the built-in categories; anything else is free text and has
/// no permissible velocity here.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum Lining {
    /// A row of MoWT Drainage Design Manual (2010) Table 9.1.
    Table(ChannelMaterial),
    /// Free text, e.g. "grassed" or "stone pitching". Not interpreted.
    Other(String),
}

impl Lining {
    pub fn label(&self) -> String {
        match self {
            Lining::Table(m) => m.label().to_string(),
            Lining::Other(s) => s.clone(),
        }
    }

    /// The Table 9.1 material, when the lining is one.
    pub fn table_material(&self) -> Option<ChannelMaterial> {
        match self {
            Lining::Table(m) => Some(*m),
            Lining::Other(_) => None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use afrobim_kernel::Severity;

    #[test]
    fn the_three_shapes_are_one_trapezoid() {
        let t = ChannelSection::Trapezoidal { bottom_width_mm: 500.0, depth_mm: 600.0, left_side_slope: 1.5, right_side_slope: 2.0 };
        assert_eq!((t.bottom_width_mm(), t.depth_mm(), t.side_slopes()), (500.0, 600.0, (1.5, 2.0)));
        // 500 + (1.5 + 2.0) × 600 = 2600.
        assert_eq!(t.top_width_mm(), 2600.0);
        assert_eq!(t.label(), "Trapezoidal b500 d600 1.5:1/2:1");
        let v = ChannelSection::V { depth_mm: 400.0, left_side_slope: 3.0, right_side_slope: 1.0 };
        assert_eq!((v.bottom_width_mm(), v.top_width_mm()), (0.0, 1600.0));
        let r = ChannelSection::Rectangular { width_mm: 450.0, depth_mm: 600.0 };
        assert_eq!((r.side_slopes(), r.top_width_mm()), ((0.0, 0.0), 450.0));
        let g = t.geometry();
        assert_eq!((g.bottom_width_m, g.left_side_slope, g.right_side_slope), (0.5, 1.5, 2.0));
    }

    #[test]
    fn dimensions_that_cannot_be_drawn_are_errors() {
        let codes = |s: &ChannelSection| s.issues().into_iter().map(|n| (n.severity, n.code)).collect::<Vec<_>>();
        let e = |c: &str| (Severity::Error, c.to_string());
        assert_eq!(codes(&ChannelSection::Rectangular { width_mm: 0.0, depth_mm: 500.0 }), [e("SECTION_WIDTH_NOT_POSITIVE")]);
        assert_eq!(codes(&ChannelSection::Rectangular { width_mm: 500.0, depth_mm: f64::NAN }), [e("SECTION_DEPTH_NOT_POSITIVE")]);
        assert_eq!(codes(&ChannelSection::V { depth_mm: 300.0, left_side_slope: 0.0, right_side_slope: 0.0 }), [e("SECTION_V_HAS_NO_WIDTH")]);
        assert_eq!(
            codes(&ChannelSection::Trapezoidal { bottom_width_mm: 300.0, depth_mm: 300.0, left_side_slope: -1.0, right_side_slope: 1.0 }),
            [e("SECTION_SLOPE_NEGATIVE")]
        );
        // A V with one vertical face is allowed.
        assert!(ChannelSection::V { depth_mm: 300.0, left_side_slope: 0.0, right_side_slope: 2.0 }.issues().is_empty());
    }
}
