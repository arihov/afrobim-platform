//! The cross-section view: corridor sections arranged in a grid at a scale, with their labels.

use afrobim_corridor::{corridor_of, section_drawing, LabelKind, Section};
use afrobim_draw::Pt2;
use serde::{Deserialize, Serialize};

use crate::draw::draw_drawing;
use crate::geom::{Aff, Rect};
use crate::item::{layers, text as mk_text, ComposedSheet};
use crate::model::Ctx;
use crate::paper::Scale;
use crate::text::{text_width, Align};

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SectionsView {
    /// The corridor element's name.
    pub corridor: String,
    /// The stations to show (internal, m); a section within a millimetre of each is taken. `None`
    /// shows every section of the corridor between `from_station_m` and `to_station_m`.
    pub stations_m: Option<Vec<f64>>,
    pub from_station_m: Option<f64>,
    pub to_station_m: Option<f64>,
    /// Scale 1:n of offsets, and of levels before the exaggeration.
    pub scale: Scale,
    pub vertical_exaggeration: f64,
    pub columns: usize,
    /// Label the offset and level of each finished point (crowded: off by default).
    pub show_design_labels: bool,
    /// Label the daylight points and the centreline ground level.
    pub show_daylight_labels: bool,
    pub caption: String,
}

impl SectionsView {
    pub fn new(corridor: &str, scale: Scale, columns: usize) -> SectionsView {
        SectionsView {
            corridor: corridor.to_string(),
            stations_m: None,
            from_station_m: None,
            to_station_m: None,
            scale,
            vertical_exaggeration: 1.0,
            columns,
            show_design_labels: false,
            show_daylight_labels: true,
            caption: String::new(),
        }
    }
}

/// The sections a view shows, in station order.
pub fn pick_sections<'a>(sections: &'a [Section], view: &SectionsView) -> Vec<&'a Section> {
    match &view.stations_m {
        Some(list) => list.iter().filter_map(|s| sections.iter().find(|x| (x.station_m - s).abs() <= 1e-3)).collect(),
        None => sections
            .iter()
            .filter(|s| view.from_station_m.is_none_or(|f| s.station_m >= f - 1e-6) && view.to_station_m.is_none_or(|t| s.station_m <= t + 1e-6))
            .collect(),
    }
}

/// Composes the sections view into `rect`. Returns how many sections were drawn.
pub fn compose_sections(sheet: &mut ComposedSheet, ctx: &Ctx, view: &SectionsView, rect: &Rect) -> usize {
    let styles = ctx.styles;
    if !view.scale.is_valid() || view.vertical_exaggeration.is_nan() || view.vertical_exaggeration <= 0.0 || view.columns == 0 {
        sheet.warn("A sections view needs a valid scale, a positive exaggeration and at least one column.");
        return 0;
    }
    let Some(id) = ctx.model.id_of(afrobim_corridor::CORRIDOR_CATEGORY, &view.corridor) else {
        sheet.warn(format!("There is no corridor called '{}' in the document.", view.corridor));
        return 0;
    };
    let corridor = match corridor_of(ctx.model.doc, id) {
        Ok(c) => c,
        Err(e) => {
            sheet.warn(format!("Corridor '{}' cannot be built: {e}", view.corridor));
            return 0;
        }
    };
    let chosen = pick_sections(&corridor.sections, view);
    if chosen.is_empty() {
        sheet.warn(format!("Corridor '{}' has no sections in the range asked for.", view.corridor));
        return 0;
    }
    let drawings: Vec<_> = chosen.iter().map(|s| section_drawing(s, view.vertical_exaggeration)).collect();
    let (lh, hh) = (styles.text.label, styles.text.heading);
    let pen = styles.pens.pen(layers::SECTION);
    // Paper size of a drawing: its units are millimetres at full size.
    let n = view.scale.0;
    let max_w = drawings.iter().map(|d| d.drawing.extents.width() / n).fold(0.0, f64::max);
    let max_h = drawings.iter().map(|d| d.drawing.extents.height() / n).fold(0.0, f64::max);
    let label_room = if view.show_design_labels || view.show_daylight_labels {
        let longest = drawings.iter().flat_map(|d| d.labels.iter()).filter(|l| matches!(l.kind, LabelKind::Design | LabelKind::Daylight | LabelKind::Ground)).map(|l| text_width(&l.text, lh, false)).fold(0.0, f64::max);
        longest.min(32.0) + 2.0
    } else {
        0.0
    };
    let header = hh * 1.4 + lh * 1.8 + 2.0;
    let cell_w = rect.width() / view.columns as f64;
    let cell_h = header + label_room + max_h + 4.0;
    if max_w > cell_w + 0.5 {
        sheet.warn(format!("A section is {max_w:.0} mm wide at {} but a column is {cell_w:.0} mm: use fewer columns or a smaller scale.", view.scale.label()));
    }
    let rows_fit = ((rect.height() - hh * 2.4) / cell_h).floor().max(0.0) as usize;
    let capacity = rows_fit * view.columns;
    if capacity < chosen.len() {
        sheet.warn(format!("{} of {} sections fit at {} in {} columns; the rest need another sheet.", capacity, chosen.len(), view.scale.label(), view.columns));
    }
    let mut drawn = 0;
    let top = rect.y1 - hh * 2.4;
    for (i, (section, sd)) in chosen.iter().zip(&drawings).enumerate().take(capacity) {
        let (row, col) = (i / view.columns, i % view.columns);
        let cell = Rect::new(rect.x0 + col as f64 * cell_w, top - (row + 1) as f64 * cell_h, rect.x0 + (col + 1) as f64 * cell_w, top - row as f64 * cell_h);
        let ext = &sd.drawing.extents;
        let (w, h) = (ext.width() / n, ext.height() / n);
        // The drawing sits centred in the cell, on the lower edge above a small margin.
        let origin = Pt2::new(cell.x0 + (cell.width() - w) / 2.0, cell.y0 + 2.0);
        let m = Aff::scale_rotate(1.0 / n, 0.0).then(&Aff::translate(origin.x - ext.min.x / n, origin.y - ext.min.y / n));
        draw_drawing(sheet, styles, &sd.drawing, &m, &cell.inset(0.2), layers::SECTION);
        // Station title and the cut and fill line sit at the head of the cell.
        for l in &sd.labels {
            match l.kind {
                LabelKind::Title => sheet.text(mk_text(layers::SECTION_TEXT, Pt2::new(cell.x0 + 2.0, cell.y1 - 1.0 - hh), hh, l.text.clone(), Align::Left, 0.0, true, pen.rgb)),
                LabelKind::Areas => sheet.text(mk_text(layers::SECTION_TEXT, Pt2::new(cell.x0 + 2.0, cell.y1 - 1.0 - hh - lh * 1.6), lh, l.text.clone(), Align::Left, 0.0, false, pen.rgb)),
                LabelKind::Design | LabelKind::Daylight | LabelKind::Ground => {
                    let wanted = match l.kind {
                        LabelKind::Design => view.show_design_labels,
                        _ => view.show_daylight_labels,
                    };
                    if !wanted {
                        continue;
                    }
                    // Turned to read upward from its point, which is inside the drawing; cut by the
                    // room above it.
                    // The ground level on the centreline is set above the whole section so it does
                    // not run through the pavement lines.
                    let y = if l.kind == LabelKind::Ground { origin.y + h } else { origin.y + (l.at.y - ext.min.y) / n };
                    let at = Pt2::new(origin.x + (l.at.x - ext.min.x) / n + lh / 2.0, y + 1.0);
                    let room = cell.y1 - header - at.y;
                    if text_width(&l.text, lh, false) <= room + label_room {
                        sheet.text(mk_text(layers::SECTION_TEXT, at, lh, l.text.clone(), Align::Left, 90.0, false, pen.rgb));
                    }
                }
            }
        }
        let _ = section;
        drawn += 1;
    }
    let vp = styles.pens.pen(layers::VIEWPORT);
    sheet.rect(layers::VIEWPORT, rect, vp.weight_mm, vp.rgb);
    let caption = if view.caption.is_empty() {
        let v = if (view.vertical_exaggeration - 1.0).abs() < 1e-9 { String::new() } else { format!("  VERTICAL x{}", crate::paper::trim_number(view.vertical_exaggeration, 2)) };
        format!("CROSS-SECTIONS  {}{v}", view.scale.label())
    } else {
        view.caption.clone()
    };
    sheet.text(mk_text(layers::TEXT, Pt2::new(rect.x0 + 2.0, rect.y1 - 2.0 - hh), hh, caption, Align::Left, 0.0, true, vp.rgb));
    drawn
}
