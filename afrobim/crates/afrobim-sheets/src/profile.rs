//! The profile view: the vertical profile of an alignment over a station range, with a level
//! datum, separate horizontal and vertical scales, a grid, and a data band of stations and levels.
//!
//! On paper, with `kh = 1000 / n` paper mm per metre of station and `kv = kh * e` paper mm per
//! metre of level (`e` is the vertical exaggeration, so the vertical scale is 1:(n / e)):
//!
//! ```text
//!   x = plot.x0 + (station - from) * kh          y = plot.y0 + (level - datum) * kv
//! ```
//!
//! The existing ground along the centreline is drawn and tabulated only when a surface is named.
//!
//! The vertical geometry is annotated (see [`crate::annotate`]): a block above the plot for each
//! PVI in range with its station and level and, on a vertical curve, its length, grade change, K
//! value and start and end stations, staggered into rows with a leader down to the PVI; the grade
//! of each tangent in percent along the straight part of the tangent; ticks at the curve ends.

use afrobim_alignment::Alignment;
use afrobim_corridor::{surface_of, Tin};
use afrobim_draw::Pt2;
use serde::{Deserialize, Serialize};

use crate::annotate::{layout_blocks, leader_pieces, percent_text, vertical_notes, ProfileAnnotations, BOX_GAP, BOX_PAD, LINE_PITCH};
use crate::geom::{clip_polyline, Rect};
use crate::item::{layers, text as mk_text, ComposedSheet};
use crate::model::{station_text, stations_at_interval, Ctx};
use crate::paper::{trim_number, Scale};
use crate::text::{text_width, Align};

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ProfileView {
    pub alignment: String,
    pub from_station_m: f64,
    pub to_station_m: f64,
    /// Horizontal scale 1:n.
    pub h_scale: Scale,
    /// Vertical exaggeration: the vertical scale is the horizontal one divided by this.
    pub vertical_exaggeration: f64,
    /// Level of the bottom of the plot; `None` picks a grid line just under the lowest level.
    pub datum_level_m: Option<f64>,
    /// Spacing of the vertical grid lines, displayed stations, m.
    pub grid_station_interval_m: f64,
    /// Spacing of the horizontal grid lines, m of level.
    pub grid_level_interval_m: f64,
    /// Spacing of the data band columns, displayed stations, m; 0 uses the grid spacing.
    pub band_interval_m: f64,
    pub show_band: bool,
    pub station_decimals: usize,
    pub level_decimals: usize,
    /// Name of a surface element (category `surface`): its level along the centreline is drawn
    /// and tabulated. `None` draws no ground.
    pub surface: Option<String>,
    /// Spacing of the ground samples, m.
    pub ground_sample_m: f64,
    pub caption: String,
    /// The vertical geometry notes (PVIs, grades, curves, K). Views saved before these existed
    /// read them as the defaults.
    #[serde(default)]
    pub annotations: ProfileAnnotations,
}

impl ProfileView {
    pub fn new(alignment: &str, from_station_m: f64, to_station_m: f64, h_scale: Scale, vertical_exaggeration: f64) -> ProfileView {
        ProfileView {
            alignment: alignment.to_string(),
            from_station_m,
            to_station_m,
            h_scale,
            vertical_exaggeration,
            datum_level_m: None,
            grid_station_interval_m: 100.0,
            grid_level_interval_m: 5.0,
            band_interval_m: 0.0,
            show_band: true,
            station_decimals: 0,
            level_decimals: 3,
            surface: None,
            ground_sample_m: 5.0,
            caption: String::new(),
            annotations: ProfileAnnotations::default(),
        }
    }
}

/// The paper mapping of a profile plot.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ProfileMap {
    pub plot: Rect,
    pub from_station_m: f64,
    pub datum_level_m: f64,
    /// Paper mm per metre of station.
    pub kh: f64,
    /// Paper mm per metre of level.
    pub kv: f64,
}

impl ProfileMap {
    pub fn at(&self, station_m: f64, level_m: f64) -> Pt2 {
        Pt2::new(self.plot.x0 + (station_m - self.from_station_m) * self.kh, self.plot.y0 + (level_m - self.datum_level_m) * self.kv)
    }
}

/// How to set a text on a band column at `x` so that no column line runs through it: just to the
/// right of the line, or, where that would stick out of the plot, just to the left of it
/// (alignment and x shift).
fn ends(x: f64, width: f64, plot: &Rect) -> (Align, f64) {
    if x + 0.8 + width > plot.x1 - 0.3 {
        (Align::Right, -0.8)
    } else {
        (Align::Left, 0.8)
    }
}

/// The datum when none is entered: the grid line at or below the lowest level, with at least a
/// quarter of a grid step of room under it.
pub fn auto_datum(min_level_m: f64, grid_m: f64) -> f64 {
    let g = if grid_m > 0.0 { grid_m } else { 1.0 };
    let d = (min_level_m / g).floor() * g;
    if min_level_m - d < g * 0.25 {
        d - g
    } else {
        d
    }
}

/// Composes a profile view into `rect`.
pub fn compose_profile(sheet: &mut ComposedSheet, ctx: &Ctx, view: &ProfileView, rect: &Rect) {
    let styles = ctx.styles;
    if !view.h_scale.is_valid() || !(view.vertical_exaggeration.is_finite() && view.vertical_exaggeration > 0.0) {
        sheet.warn("A profile view needs a valid horizontal scale and a positive vertical exaggeration.");
        return;
    }
    let alignment: Alignment = match ctx.model.alignment(&view.alignment) {
        Ok(a) => a,
        Err(e) => {
            sheet.warn(e);
            return;
        }
    };
    let Some(profile) = alignment.profile.as_ref() else {
        sheet.warn(format!("Alignment '{}' has no vertical profile.", view.alignment));
        return;
    };
    let Some((p0, p1)) = profile.range() else {
        sheet.warn(format!("The profile of '{}' has fewer than two PVIs.", view.alignment));
        return;
    };
    let (s0, s1) = (view.from_station_m.max(p0).max(alignment.start_station_m), view.to_station_m.min(p1).min(alignment.end_station_m()));
    if s1.is_nan() || s1 <= s0 {
        sheet.warn(format!("The profile range {:.3} to {:.3} is outside the profile of '{}' ({p0:.3} to {p1:.3}).", view.from_station_m, view.to_station_m, view.alignment));
        return;
    }
    let tin: Option<Tin> = match &view.surface {
        Some(name) => match ctx.model.id_of(afrobim_corridor::SURFACE_CATEGORY, name).and_then(|id| surface_of(ctx.model.doc, id)) {
            Some(t) => Some(t),
            None => {
                sheet.warn(format!("Surface '{name}' was not found; no ground is drawn."));
                None
            }
        },
        None => None,
    };

    let ann = view.annotations;
    let notes = if ann.any() {
        match vertical_notes(profile) {
            Ok(n) => Some(n),
            Err(e) => {
                sheet.warn(format!("The vertical annotation of '{}' was left out: {e}", view.alignment));
                None
            }
        }
    } else {
        None
    };
    let (kh, kv) = (view.h_scale.paper_mm_per_m(), view.h_scale.paper_mm_per_m() * view.vertical_exaggeration);
    let (lh, small) = (styles.text.label, styles.text.small);
    let design_pen = styles.pens.pen(layers::PROFILE_DESIGN);
    let ground_pen = styles.pens.pen(layers::PROFILE_GROUND);
    let grid_pen = styles.pens.pen(layers::PROFILE_GRID);
    let band_pen = styles.pens.pen(layers::PROFILE_BAND);

    // The design line, sampled every millimetre of paper, with the PVIs and curve ends in it.
    let step = (1.0 / kh).clamp(0.05, 50.0);
    let n = (((s1 - s0) / step).ceil() as usize).clamp(1, 20_000);
    let mut stations: Vec<f64> = (0..=n).map(|i| s0 + (s1 - s0) * i as f64 / n as f64).collect();
    stations.extend(profile.pvis.iter().map(|p| p.station_m).filter(|s| *s > s0 && *s < s1));
    stations.sort_by(f64::total_cmp);
    let design: Vec<(f64, f64)> = stations.iter().filter_map(|s| profile.elevation_at(*s).ok().map(|z| (*s, z))).collect();
    let ground: Vec<(f64, Option<f64>)> = match &tin {
        Some(t) => {
            let gn = (((s1 - s0) / view.ground_sample_m.max(0.1)).ceil() as usize).clamp(1, 20_000);
            (0..=gn)
                .map(|i| {
                    let s = s0 + (s1 - s0) * i as f64 / gn as f64;
                    (s, alignment.point_at(s).and_then(|p| t.elevation_at(p.x, p.y)))
                })
                .collect()
        }
        None => Vec::new(),
    };
    let (mut zmin, mut zmax) = (f64::INFINITY, f64::NEG_INFINITY);
    // The PVIs in range are part of the levels the plot must hold: a PVI stands above a crest.
    let pvi_levels: Vec<f64> = match &notes {
        Some(n) if ann.pvi => n.pvis.iter().filter(|p| p.station_m >= s0 - 1e-9 && p.station_m <= s1 + 1e-9).map(|p| p.elevation_m).collect(),
        _ => Vec::new(),
    };
    for z in design.iter().map(|d| d.1).chain(ground.iter().filter_map(|g| g.1)).chain(pvi_levels.iter().copied()) {
        zmin = zmin.min(z);
        zmax = zmax.max(z);
    }
    if !zmin.is_finite() {
        sheet.warn("The profile has no levels in the range.");
        return;
    }
    let grid_v = if view.grid_level_interval_m > 0.0 { view.grid_level_interval_m } else { 1.0 };
    let datum = view.datum_level_m.unwrap_or_else(|| auto_datum(zmin, grid_v));

    // Layout: caption on top, plot, data band below; level labels left of the plot.
    let band_interval = if view.band_interval_m > 0.0 { view.band_interval_m } else { view.grid_station_interval_m };
    let band_stations = stations_at_interval(&alignment, s0, s1, band_interval);
    let station_texts: Vec<String> = band_stations.iter().map(|s| station_text(&alignment, *s, view.station_decimals)).collect();
    let widest_station = station_texts.iter().map(|t| text_width(t, lh, false)).fold(0.0, f64::max);
    let spacing_mm = band_interval * kh;
    let rotate_stations = spacing_mm < widest_station + 1.5;
    let rows: Vec<(&str, bool)> = {
        let mut r = vec![("STATION", false), ("DESIGN LEVEL", false)];
        if tin.is_some() {
            r.push(("GROUND LEVEL", false));
        }
        r
    };
    let level_text = |z: f64| format!("{:.*}", view.level_decimals, z);
    let widest_level = text_width(&level_text(zmax.abs().max(zmin.abs()) + grid_v), lh, false);
    let caption_w = rows.iter().map(|(t, _)| text_width(t, small, true)).fold(0.0, f64::max);
    let left = (widest_level + 3.0).max(caption_w + 3.0);
    let row_h = lh * 2.4;
    let station_row_h = if rotate_stations { widest_station + 3.0 } else { row_h };
    let band_h = if view.show_band { station_row_h + (rows.len() - 1) as f64 * row_h } else { 0.0 };
    let top_pad = styles.text.heading * 2.2;
    // The plot is as wide as the profile needs and as high as the grid line above the highest
    // level, hung from the caption; the data band follows directly under it.
    // The PVI blocks sit in rows between the caption and the plot.
    let x_left = rect.x0 + left;
    let plot_w_est = ((s1 - s0) * kh).min(rect.x1 - 3.0 - x_left);
    let sta = |s: f64| station_text(&alignment, s, ann.station_decimals);
    let (blocks, block_rows) = match &notes {
        Some(n) if ann.pvi || ann.curves => {
            layout_blocks(n, &ann, s0, s1, &|s| x_left + (s - s0) * kh, x_left, x_left + plot_w_est, lh, &sta, view.level_decimals, &|t| text_width(t, lh, false))
        }
        _ => (Vec::new(), 0),
    };
    let block_h = blocks.iter().map(|b| b.height).fold(0.0, f64::max);
    let row_pitch = block_h + BOX_GAP;
    let strip_h = if block_rows > 0 { block_rows as f64 * row_pitch + 1.0 } else { 0.0 };
    let avail = Rect::new(x_left, rect.y0 + band_h + 2.0, rect.x1 - 3.0, rect.y1 - top_pad - strip_h);
    let top_level = datum + (((zmax - datum) / grid_v) + 1e-9).ceil().max(1.0) * grid_v;
    let plot_h = ((top_level - datum) * kv).min(avail.height());
    let plot_w = ((s1 - s0) * kh).min(avail.width());
    let plot = Rect::new(avail.x0, avail.y1 - plot_h, avail.x0 + plot_w, avail.y1);
    if plot.width() <= 10.0 || plot.height() <= 10.0 {
        sheet.warn("The viewport is too small for a profile.");
        return;
    }
    let map = ProfileMap { plot, from_station_m: s0, datum_level_m: datum, kh, kv };
    let need_w = (s1 - s0) * kh;
    let need_h = (zmax - datum) * kv;
    if need_w > avail.width() + 0.5 {
        sheet.warn(format!("The profile is {need_w:.0} mm long at {} but the plot is {:.0} mm wide: the end is cut off.", view.h_scale.label(), avail.width()));
    }
    if need_h > avail.height() + 0.5 {
        sheet.warn(format!("The profile is {need_h:.0} mm high above the datum but the plot is {:.0} mm high: lower the exaggeration or the datum, or use a smaller scale.", avail.height()));
    }

    // Grid.
    let gl = |sheet: &mut ComposedSheet, a: Pt2, b: Pt2| {
        for piece in clip_polyline(&[a, b], &plot) {
            sheet.stroke(layers::PROFILE_GRID, piece, false, grid_pen.weight_mm, grid_pen.rgb);
        }
    };
    for s in stations_at_interval(&alignment, s0, s1, view.grid_station_interval_m) {
        gl(sheet, map.at(s, datum), Pt2::new(map.at(s, datum).x, plot.y1));
    }
    let mut z = datum;
    let mut guard = 0;
    while map.at(s0, z).y <= plot.y1 + 1e-9 && guard < 2000 {
        let y = map.at(s0, z).y;
        gl(sheet, Pt2::new(plot.x0, y), Pt2::new(plot.x1, y));
        sheet.text(mk_text(layers::PROFILE_GRID, Pt2::new(plot.x0 - 1.0, y - lh / 2.0), lh, level_text(z), Align::Right, 0.0, false, band_pen.rgb));
        z += grid_v;
        guard += 1;
    }
    sheet.rect(layers::PROFILE_BAND, &plot, band_pen.weight_mm, band_pen.rgb);

    // Ground and design lines.
    if tin.is_some() {
        let mut run: Vec<Pt2> = Vec::new();
        let flush = |sheet: &mut ComposedSheet, run: &mut Vec<Pt2>| {
            for piece in clip_polyline(run, &plot) {
                sheet.stroke(layers::PROFILE_GROUND, piece, false, ground_pen.weight_mm, ground_pen.rgb);
            }
            run.clear();
        };
        for (s, z) in &ground {
            match z {
                Some(z) => run.push(map.at(*s, *z)),
                None => flush(sheet, &mut run),
            }
        }
        flush(sheet, &mut run);
    }
    let design_pts: Vec<Pt2> = design.iter().map(|(s, z)| map.at(*s, *z)).collect();
    for piece in clip_polyline(&design_pts, &plot) {
        sheet.stroke(layers::PROFILE_DESIGN, piece, false, design_pen.weight_mm, design_pen.rgb);
    }

    // Vertical geometry notes.
    if let Some(n) = &notes {
        let pen = styles.pens.pen(layers::PROFILE_NOTES);
        let inside = |st: f64| st >= s0 - 1e-9 && st <= s1 + 1e-9;
        // PVI markers: a small circle on the PVI itself.
        if ann.pvi {
            for p in n.pvis.iter().filter(|p| inside(p.station_m)) {
                let c = map.at(p.station_m, p.elevation_m);
                let ring: Vec<Pt2> = (0..12).map(|i| {
                    let a = i as f64 / 12.0 * std::f64::consts::TAU;
                    Pt2::new(c.x + 0.7 * a.cos(), c.y + 0.7 * a.sin())
                }).collect();
                if plot.contains(c) {
                    sheet.stroke(layers::PROFILE_NOTES, ring, true, pen.weight_mm, pen.rgb);
                }
            }
        }
        // Curve ends: a short tick across the design line at the start and the end of each curve.
        if ann.curves {
            for c in n.pvis.iter().filter_map(|p| p.curve.as_ref()) {
                for st in [c.start_station_m, c.end_station_m].into_iter().filter(|st| inside(*st)) {
                    if let Ok(z) = profile.elevation_at(st) {
                        let p = map.at(st, z);
                        for piece in clip_polyline(&[Pt2::new(p.x, p.y - 1.4), Pt2::new(p.x, p.y + 1.4)], &plot) {
                            sheet.stroke(layers::PROFILE_NOTES, piece, false, pen.weight_mm, pen.rgb);
                        }
                    }
                }
            }
        }
        // The grade of each tangent along its straight part, turned to follow the line and set
        // just above it.
        if ann.grades {
            for t in &n.tangents {
                let (a, b) = (t.run_start_m.max(s0), t.run_end_m.min(s1));
                if b - a <= 1e-9 {
                    continue;
                }
                let text = percent_text(t.grade_percent, ann.decimals);
                let slope = t.grade_percent / 100.0 * kv / kh;
                let phi = slope.atan();
                let run_mm = (b - a) * kh / phi.cos();
                if run_mm < text_width(&text, lh, false) + 2.0 {
                    sheet.warn(format!("The grade {text} of the tangent from PVI {} to PVI {} does not fit along its straight part at {}: it is not labelled.", t.from_pvi + 1, t.to_pvi + 1, view.h_scale.label()));
                    continue;
                }
                let mid = (a + b) / 2.0;
                if let Ok(z) = profile.elevation_at(mid) {
                    let p = map.at(mid, z);
                    let at = Pt2::new(p.x - phi.sin() * 0.8, p.y + phi.cos() * 0.8);
                    sheet.text(mk_text(layers::PROFILE_NOTES, at, lh, text, Align::Centre, phi.to_degrees(), false, pen.rgb));
                }
            }
        }
        // The PVI blocks with leaders (a leader never runs through another block).
        let row0_y = plot.y1 + 1.0;
        let rects: Vec<Rect> = blocks.iter().map(|b| b.rect(row0_y, row_pitch)).collect();
        for (i, (b, r)) in blocks.iter().zip(&rects).enumerate() {
            sheet.rect(layers::PROFILE_NOTES, r, pen.weight_mm, pen.rgb);
            for (k, line) in b.lines.iter().enumerate() {
                sheet.text(mk_text(layers::PROFILE_NOTES, Pt2::new(r.x0 + BOX_PAD, r.y1 - BOX_PAD - lh - k as f64 * lh * LINE_PITCH), lh, line.clone(), Align::Left, 0.0, false, pen.rgb));
            }
            let p = &n.pvis[b.pvi];
            let y_pvi = map.at(p.station_m, p.elevation_m).y.clamp(plot.y0, plot.y1);
            let others: Vec<Rect> = rects.iter().enumerate().filter(|(j, _)| *j != i).map(|(_, r)| *r).collect();
            if b.leader_x < r.x0 || b.leader_x > r.x1 {
                // The box was kept inside the plot: join the leader to the nearest end of the box's base.
                let edge = if b.leader_x < r.x0 { r.x0 } else { r.x1 };
                sheet.line(layers::PROFILE_NOTES, Pt2::new(b.leader_x, r.y0), Pt2::new(edge, r.y0), grid_pen.weight_mm, grid_pen.rgb);
            }
            for (hi, lo) in leader_pieces(b.leader_x, r.y0, y_pvi, &others) {
                sheet.line(layers::PROFILE_NOTES, Pt2::new(b.leader_x, hi), Pt2::new(b.leader_x, lo), grid_pen.weight_mm, grid_pen.rgb);
            }
        }
    }

    // Data band.
    if view.show_band {
        let band_top = plot.y0 - 2.0;
        let mut y = band_top;
        let heights: Vec<f64> = (0..rows.len()).map(|i| if i == 0 { station_row_h } else { row_h }).collect();
        let band_rect = Rect::new(plot.x0, band_top - heights.iter().sum::<f64>(), plot.x1, band_top);
        sheet.rect(layers::PROFILE_BAND, &band_rect, band_pen.weight_mm, band_pen.rgb);
        for (i, (name, _)) in rows.iter().enumerate() {
            let h = heights[i];
            sheet.line(layers::PROFILE_BAND, Pt2::new(rect.x0, y), Pt2::new(plot.x1, y), band_pen.weight_mm, band_pen.rgb);
            sheet.text(mk_text(layers::PROFILE_BAND, Pt2::new(rect.x0 + 1.0, y - h / 2.0 - small / 2.0), small, *name, Align::Left, 0.0, true, band_pen.rgb));
            y -= h;
        }
        sheet.line(layers::PROFILE_BAND, Pt2::new(rect.x0, y), Pt2::new(plot.x1, y), band_pen.weight_mm, band_pen.rgb);
        sheet.line(layers::PROFILE_BAND, Pt2::new(rect.x0, band_top), Pt2::new(rect.x0, y), band_pen.weight_mm, band_pen.rgb);
        for (idx, s) in band_stations.iter().enumerate() {
            let x = map.at(*s, datum).x;
            sheet.line(layers::PROFILE_BAND, Pt2::new(x, band_top), Pt2::new(x, y), grid_pen.weight_mm, grid_pen.rgb);
            // Station: upright and centred on the column, or, when columns are too close, turned to
            // read upward from the bottom of the row.
            let (at, align, rot) = if rotate_stations {
                (Pt2::new(x + lh / 2.0, band_top - heights[0] + 1.5), Align::Left, 90.0)
            } else {
                let (a, dx) = ends(x, text_width(&station_texts[idx], lh, false), &plot);
                (Pt2::new(x + dx, band_top - heights[0] / 2.0 - lh / 2.0), a, 0.0)
            };
            sheet.text(mk_text(layers::PROFILE_BAND, at, lh, station_texts[idx].clone(), align, rot, false, band_pen.rgb));
            // Design level, and ground level.
            let design_z = profile.elevation_at(*s).ok();
            let mut row_top = band_top - heights[0];
            let values: Vec<Option<f64>> = if tin.is_some() {
                let g = tin.as_ref().and_then(|t| alignment.point_at(*s).and_then(|p| t.elevation_at(p.x, p.y)));
                vec![design_z, g]
            } else {
                vec![design_z]
            };
            for v in values {
                let t = v.map_or("-".to_string(), level_text);
                let (a, dx) = ends(x, text_width(&t, lh, false), &plot);
                sheet.text(mk_text(layers::PROFILE_BAND, Pt2::new(x + dx, row_top - row_h / 2.0 - lh / 2.0), lh, t, a, 0.0, false, band_pen.rgb));
                row_top -= row_h;
            }
        }
    }

    // Caption.
    let caption = if view.caption.is_empty() {
        format!("PROFILE  H {}  V {}  DATUM {} m", view.h_scale.label(), Scale(view.h_scale.0 / view.vertical_exaggeration).label(), trim_number(datum, 3))
    } else {
        view.caption.clone()
    };
    sheet.text(mk_text(layers::TEXT, Pt2::new(rect.x0 + 2.0, rect.y1 - 2.0 - styles.text.heading), styles.text.heading, caption, Align::Left, 0.0, true, band_pen.rgb));
    let vp = styles.pens.pen(layers::VIEWPORT);
    sheet.rect(layers::VIEWPORT, rect, vp.weight_mm, vp.rgb);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_map_follows_the_two_scales() {
        // 1:1000 horizontally (1 mm per m) with exaggeration 10 (10 mm per m of level): from
        // station 200, datum 100, plot corner at (30, 40): station 250 level 102.5 is at
        // x = 30 + 50 = 80 and y = 40 + 2.5 x 10 = 65.
        let map = ProfileMap { plot: Rect::new(30.0, 40.0, 300.0, 200.0), from_station_m: 200.0, datum_level_m: 100.0, kh: 1.0, kv: 10.0 };
        let p = map.at(250.0, 102.5);
        assert_eq!((p.x, p.y), (80.0, 65.0));
    }

    #[test]
    fn the_automatic_datum_is_a_grid_line_under_the_lowest_level() {
        assert_eq!(auto_datum(103.2, 5.0), 100.0);
        // 100.5 is too close to 100 (under a quarter step of room): one step lower.
        assert_eq!(auto_datum(100.5, 5.0), 95.0);
        assert_eq!(auto_datum(-3.0, 2.0), -4.0);
    }
}
