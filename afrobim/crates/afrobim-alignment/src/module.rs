// SPDX-License-Identifier: LicenseRef-AfroBIM-Proprietary
// Copyright (c) 2026 AfroBIM. All rights reserved.
//! The alignment as a kernel element: category `"alignment"`, `data` = the serialised [`Alignment`].

use std::collections::BTreeMap;

use afrobim_draw::{Drawing, Extents, Primitive, Pt2};
use afrobim_kernel::{Element, Generated, Module, Note, Severity};

use crate::geometry::{format_station, Alignment, ElementKind};

/// The element category this crate owns.
pub const CATEGORY: &str = "alignment";

/// The plan of an alignment as drawing primitives, with what is needed to put it back on the map.
///
/// The drawing is in millimetres with Y up (north), not rotated or scaled otherwise: 1 m on the
/// ground is 1000 drawing units. Its lower-left extent is on the drawing origin, and that origin
/// is the plan point (`origin_x_m`, `origin_y_m`). So a drawing point (u, v) is the plan point
///
/// ```text
///   easting  = origin_x_m + u / 1000
///   northing = origin_y_m + v / 1000        (metres)
/// ```
///
/// and to show the alignment in site coordinates (millimetres, as in `SitePlacement`) the
/// drawing is translated by (`origin_x_m` × 1000, `origin_y_m` × 1000). The extent covers the
/// station ticks as well as the centreline, so the origin is usually a little south-west of the
/// centreline's own lower-left corner.
#[derive(Debug, Clone, PartialEq)]
pub struct PlanDrawing {
    pub drawing: Drawing,
    /// Plan easting of the drawing origin, m.
    pub origin_x_m: f64,
    /// Plan northing of the drawing origin, m.
    pub origin_y_m: f64,
    /// Spacing of the station ticks, m. Ticks stand on whole multiples of it.
    pub tick_interval_m: f64,
    /// Longest chord used to draw curves and spirals, m.
    pub max_chord_m: f64,
}

/// Greatest distance allowed between a drawn chord and the true curve, m.
const CHORD_SAG_M: f64 = 0.005;

/// Draws an alignment in plan.
///
/// - The centreline is a run of hairlines (`Primitive::Line` of width 0, black): straights as one
///   line, curves and spirals as chords that stay within 5 mm of the true curve.
/// - Station ticks are hairlines square to the centreline at whole multiples of
///   `tick_interval_m`, the smallest of 1, 2, 5, 10, 20, 50, … m that gives at most 50 ticks.
///   A tick reaches a tenth of the interval to each side; at each whole multiple of the next
///   power of ten (every 100 m for 10, 20 or 50 m ticks) it is twice as long.
/// - There are no station labels: the draw crate has no text primitive, only glyph outlines.
///   [`format_station`] gives the text for a front end that can draw it.
///
/// `None` when the alignment has no length.
pub fn plan_drawing(alignment: &Alignment) -> Option<PlanDrawing> {
    let length = alignment.length();
    if alignment.elements.is_empty() || !(length > 0.0 && length.is_finite()) {
        return None;
    }
    // Sagitta of a chord c on radius R is c²/8R.
    let max_chord_m = alignment.min_radius().map_or(20.0, |r| (8.0 * r * CHORD_SAG_M).sqrt().clamp(0.25, 20.0));
    let centreline = alignment.polyline(max_chord_m);

    let mut interval = 1.0;
    'ladder: loop {
        for step in [1.0, 2.0, 5.0] {
            if length / (interval * step) <= 50.0 {
                interval *= step;
                break 'ladder;
            }
        }
        interval *= 10.0;
    }
    let major = 10f64.powf((interval.log10() + 1e-9).floor() + 1.0);
    let mut ticks: Vec<[(f64, f64); 2]> = Vec::new();
    let first = ((alignment.start_station_m - 1e-9) / interval).ceil() as i64;
    let last = ((alignment.end_station_m() + 1e-9) / interval).floor() as i64;
    for i in first..=last {
        let station = (i as f64 * interval).clamp(alignment.start_station_m, alignment.end_station_m());
        let Some(at) = alignment.point_at(station) else { continue };
        let on_major = ((i as f64 * interval) / major - ((i as f64 * interval) / major).round()).abs() < 1e-9;
        let half = if on_major { 0.2 * interval } else { 0.1 * interval };
        ticks.push([at.offset(-half), at.offset(half)]);
    }

    let all = centreline.iter().chain(ticks.iter().flatten());
    let (mut min_x, mut min_y, mut max_x, mut max_y) = (f64::INFINITY, f64::INFINITY, f64::NEG_INFINITY, f64::NEG_INFINITY);
    for &(x, y) in all {
        min_x = min_x.min(x);
        min_y = min_y.min(y);
        max_x = max_x.max(x);
        max_y = max_y.max(y);
    }
    // Subtract in metres before scaling, so large map coordinates lose nothing.
    let mm = |p: (f64, f64)| Pt2::new((p.0 - min_x) * 1000.0, (p.1 - min_y) * 1000.0);
    let hairline = |a: (f64, f64), b: (f64, f64)| Primitive::Line { from: mm(a), to: mm(b), width: 0.0, colour: None };
    let mut primitives: Vec<Primitive> = centreline.windows(2).map(|w| hairline(w[0], w[1])).collect();
    primitives.extend(ticks.iter().map(|t| hairline(t[0], t[1])));

    Some(PlanDrawing {
        drawing: Drawing { primitives, extents: Extents { min: Pt2::new(0.0, 0.0), max: mm((max_x, max_y)) } },
        origin_x_m: min_x,
        origin_y_m: min_y,
        tick_interval_m: interval,
        max_chord_m,
    })
}

/// The kernel module for alignments.
///
/// `generate` returns the [`plan_drawing`] of the element's alignment and these derived values:
///
/// | key | value |
/// |---|---|
/// | `length_m` | total length, m, 3 decimals |
/// | `start_station`, `end_station` | as `1+250.000` |
/// | `start_station_m`, `end_station_m` | the same in metres, 3 decimals |
/// | `elements` | number of elements |
/// | `lines`, `arcs`, `spirals` | number of each kind |
/// | `min_radius_m` | smallest radius, m, 3 decimals, or `none` for an all-straight alignment |
/// | `plan_origin_x_m`, `plan_origin_y_m` | plan coordinates of the drawing origin (see [`PlanDrawing`]), m, 6 decimals |
/// | `tick_interval_m` | spacing of the station ticks, m |
///
/// It adds a warning note for every joint where consecutive elements are more than 1 mm apart or
/// change direction by more than 0.0001 rad, since a road centreline is normally smooth.
#[derive(Debug, Clone, Copy, Default)]
pub struct AlignmentModule;

impl Module for AlignmentModule {
    fn category(&self) -> &str {
        CATEGORY
    }

    fn display_name(&self) -> &str {
        "Alignments"
    }

    fn generate(&self, element: &Element) -> Result<Generated, String> {
        let alignment: Alignment =
            serde_json::from_value(element.data.clone()).map_err(|e| format!("'{}' is not a valid alignment: {e}", element.name))?;
        let alignment = Alignment::from_elements(&alignment.name, alignment.start_station_m, alignment.elements)
            .map_err(|e| format!("Alignment '{}': {e}", element.name))?;
        let plan = plan_drawing(&alignment).ok_or_else(|| format!("Alignment '{}' has no length.", element.name))?;

        let count = |kind: ElementKind| alignment.elements.iter().filter(|e| e.kind() == kind).count().to_string();
        let mut derived = BTreeMap::new();
        let mut put = |k: &str, v: String| {
            derived.insert(k.to_string(), v);
        };
        put("length_m", format!("{:.3}", alignment.length()));
        put("start_station", format_station(alignment.start_station_m, 3));
        put("end_station", format_station(alignment.end_station_m(), 3));
        put("start_station_m", format!("{:.3}", alignment.start_station_m));
        put("end_station_m", format!("{:.3}", alignment.end_station_m()));
        put("elements", alignment.elements.len().to_string());
        put("lines", count(ElementKind::Line));
        put("arcs", count(ElementKind::Arc));
        put("spirals", count(ElementKind::Spiral));
        put("min_radius_m", alignment.min_radius().map_or("none".to_string(), |r| format!("{r:.3}")));
        put("plan_origin_x_m", format!("{:.6}", plan.origin_x_m));
        put("plan_origin_y_m", format!("{:.6}", plan.origin_y_m));
        put("tick_interval_m", format!("{}", plan.tick_interval_m));

        let notes = alignment
            .discontinuities(1e-3, 1e-4)
            .into_iter()
            .map(|j| Note {
                severity: Severity::Warning,
                code: "alignment.joint".to_string(),
                message: format!(
                    "At station {} element {} starts {:.3} m from the end of the one before and turns {:.4}° from it.",
                    format_station(j.station_m, 3),
                    j.index + 1,
                    j.gap_m,
                    j.heading_change_rad.to_degrees()
                ),
            })
            .collect();

        Ok(Generated { drawing: plan.drawing, notes, derived, solids: Vec::new() })
    }
}
