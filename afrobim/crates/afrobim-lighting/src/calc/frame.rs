// SPDX-License-Identifier: LicenseRef-AfroBIM-Proprietary
// Copyright (c) 2026 AfroBIM. All rights reserved.
//! The coordinate transforms between the site and a luminaire's photometric (C, gamma) system.
//!
//! These are first-principles geometry, derived here and tested against hand-worked values; no
//! standard prescribes them beyond the C-gamma definition (EN 13032-1:2004+A1:2012 clause 4.2.3:
//! the polar axis is the vertical through the photometric centre, gamma is measured from the
//! nadir, the C-plane system is fixed in space and does not follow a tilt of the luminaire).
//!
//! # Frames
//!
//! World: x east, y north, z up (metres). For one luminaire with the horizontal bracket
//! direction at azimuth `theta` (radians, counter-clockwise from +x; the direction from the
//! column towards the carriageway) the luminaire frame is (`street`, `left`, `up`):
//! `street = (cos theta, sin theta, 0)`, `left = (-sin theta, cos theta, 0)`, `up = (0, 0, 1)`.
//!
//! # Direction to (C, gamma)
//!
//! For a unit vector `d` from the luminaire towards a point, components `(a, b, c)` along
//! (`street`, `left`, `up`):
//!
//! 1. Tilt. The installed luminaire is tilted by `tilt` (positive: the street end raised, as
//!    `ColumnSpec::tilt_deg`: tilt above horizontal) and the data were measured at
//!    `measured_tilt`. The luminaire body is rotated about the `left` axis by
//!    `delta = tilt - measured_tilt`, which takes `street` to `(cos d, 0, sin d)` and `up` to
//!    `(-sin d, 0, cos d)`. A direction `d` in the world is therefore, in the photometric frame
//!    (the frame in which the data were measured), `a_p = a cos d + c sin d`,
//!    `c_p = -a sin d + c cos d`, `b_p = b`. (A beam straight down, `c_p = -1`, leaves a luminaire
//!    raised by `delta` towards the street: `a = sin d`, so the lit spot moves to the street
//!    side, as expected.)
//! 2. Gamma is the angle from the nadir: `gamma = acos(-c_p)`.
//! 3. The azimuth in the photometric frame, counter-clockwise from `street`, is
//!    `alpha = atan2(b_p, a_p)`. The C-plane is `C = C_toward + s * alpha` (degrees, modulo 360)
//!    where `C_toward` is the C-plane that faces the carriageway and `s` is +1 when C increases
//!    counter-clockwise seen from above and -1 when it increases clockwise ([`Orientation`]).

use serde::{Deserialize, Serialize};

use crate::LightingData;

/// Which C-plane faces the carriageway, and the sense in which C increases.
///
/// This is a convention that the file does not state, and the two formats are used differently
/// in practice. What the sources and the real files say:
///
/// - CEN EN 13032-1:2004+A1:2012, note to clause 4.2.3 (public preview, p. 12): "For road
///   lighting calculations, the usual convention is that the C0/C180 intensity planes lie parallel
///   to the road." This is the EULUMDAT/European practice: the carriageway is on the C90 or the
///   C270 side.
/// - AGi32, "Photometric File Type C - Concepts": for IES files "horizontal values between 270
///   and 90 degrees are in front of the luminaire and are referenced as Street Side"; angles are
///   taken counter-clockwise (seen from above). So in IES files C0 faces the street.
/// - Real files agree. The IES roadway files read for this crate (Type II and Type III
///   luminaires, one with a house side shield) are stored for horizontal angles 0 to 180 and
///   throw on both sides of the C0 direction (peak at C58 to C78 degrees); 83 % of the flux of the
///   house-side-shielded one falls on the C0 side. The EULUMDAT road luminaire files read (Isym 3)
///   throw along the C0-C180 axis and put 67 to 74 % of their flux on the C270 side, which suggests
///   the C270 side is the street side.
/// - The sense of C in EULUMDAT is not stated in any text retrieved; counter-clockwise is assumed.
///
/// [`Orientation::for_format`] gives these defaults. They are **assumptions**: check the polar
/// diagram and [`crate::Photometry::flux_fraction_towards_c`] (the share of the flux on the
/// carriageway side) for every luminaire used, and override the setting if needed.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct Orientation {
    /// The C-plane (degrees) whose half-plane points from the column towards the carriageway.
    pub c_toward_carriageway_deg: f64,
    /// True when C increases counter-clockwise seen from above.
    pub counterclockwise: bool,
}

impl Default for Orientation {
    /// The IES convention: C0 towards the carriageway, C counter-clockwise.
    fn default() -> Self {
        Orientation { c_toward_carriageway_deg: 0.0, counterclockwise: true }
    }
}

impl Orientation {
    /// The default for the format a distribution was read from: IES C0 towards the carriageway;
    /// EULUMDAT C270 towards the carriageway (C0/C180 parallel to the road). See the type
    /// documentation for what each rests on.
    pub fn for_format(format: &crate::photometry::SourceFormat) -> Orientation {
        match format {
            crate::photometry::SourceFormat::Ies { .. } => Orientation { c_toward_carriageway_deg: 0.0, counterclockwise: true },
            crate::photometry::SourceFormat::Eulumdat => Orientation { c_toward_carriageway_deg: 270.0, counterclockwise: true },
        }
    }
}

/// One luminaire on site.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct LuminaireInstance {
    /// The mark of the column carrying it.
    pub mark: String,
    pub x_m: f64,
    pub y_m: f64,
    /// Height of the photometric centre, taken as the luminaire position, m.
    pub z_m: f64,
    /// Direction from the column towards the carriageway, radians counter-clockwise from +x.
    pub bracket_azimuth_rad: f64,
    /// Tilt above horizontal, degrees.
    pub tilt_deg: f64,
}

/// The luminaires of a lighting element: each bracket of each column, at the column's position
/// plus the outreach along the bracket, at the profile level of the column (zero when the
/// alignment had no profile there) plus the mounting height.
pub fn luminaire_instances(data: &LightingData) -> Vec<LuminaireInstance> {
    let spec = data.definition.column();
    let mut out = Vec::new();
    for c in &data.layout.columns {
        for dir in &c.bracket_directions_rad {
            out.push(LuminaireInstance {
                mark: c.mark.clone(),
                x_m: c.easting_m + spec.bracket_outreach_m * dir.cos(),
                y_m: c.northing_m + spec.bracket_outreach_m * dir.sin(),
                z_m: c.level_m.unwrap_or(0.0) + spec.mounting_height_m,
                bracket_azimuth_rad: *dir,
                tilt_deg: spec.tilt_deg,
            });
        }
    }
    out
}

/// The (C, gamma) in degrees at which the distribution is read for the unit vector `d_world`
/// (east, north, up) pointing from the luminaire to the point. See the module documentation.
pub fn photometric_angles(inst: &LuminaireInstance, orientation: &Orientation, measured_tilt_deg: f64, d_world: [f64; 3]) -> (f64, f64) {
    let (st, ct) = inst.bracket_azimuth_rad.sin_cos();
    let a = d_world[0] * ct + d_world[1] * st;
    let b = -d_world[0] * st + d_world[1] * ct;
    let c = d_world[2];
    let delta = (inst.tilt_deg - measured_tilt_deg).to_radians();
    let (sd, cd) = delta.sin_cos();
    let a_p = a * cd + c * sd;
    let c_p = -a * sd + c * cd;
    let gamma = (-c_p).clamp(-1.0, 1.0).acos().to_degrees();
    let alpha = b.atan2(a_p).to_degrees();
    let sense = if orientation.counterclockwise { 1.0 } else { -1.0 };
    let c_deg = (orientation.c_toward_carriageway_deg + sense * alpha).rem_euclid(360.0);
    (c_deg, gamma)
}
