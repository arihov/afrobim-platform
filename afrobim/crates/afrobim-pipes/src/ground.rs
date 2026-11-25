// SPDX-License-Identifier: LicenseRef-AfroBIM-Proprietary
// Copyright (c) 2026 AfroBIM. All rights reserved.
//! A ground surface as the checks see it: a level at an easting and northing.

use afrobim_corridor::Tin;

/// Ground level lookup. `None` means the point is off the surface.
pub trait Ground {
    fn level_m(&self, easting_m: f64, northing_m: f64) -> Option<f64>;
}

impl Ground for Tin {
    fn level_m(&self, easting_m: f64, northing_m: f64) -> Option<f64> {
        self.elevation_at(easting_m, northing_m)
    }
}

/// A closure `(easting, northing) -> level` as a ground surface. It is defined everywhere.
pub struct GroundFn<F>(pub F);

impl<F: Fn(f64, f64) -> f64> Ground for GroundFn<F> {
    fn level_m(&self, easting_m: f64, northing_m: f64) -> Option<f64> {
        Some((self.0)(easting_m, northing_m))
    }
}

/// A closure that may decline a point.
pub struct GroundOptFn<F>(pub F);

impl<F: Fn(f64, f64) -> Option<f64>> Ground for GroundOptFn<F> {
    fn level_m(&self, easting_m: f64, northing_m: f64) -> Option<f64> {
        (self.0)(easting_m, northing_m)
    }
}
