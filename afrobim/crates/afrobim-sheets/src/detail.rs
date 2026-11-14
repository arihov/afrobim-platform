//! The detail view: any `Drawing` at a scale with a caption (a sign face, a Gantt chart, an
//! S-curve).

use afrobim_draw::{Drawing, Pt2};
use serde::{Deserialize, Serialize};

use crate::draw::draw_drawing;
use crate::geom::{Aff, Rect};
use crate::item::{layers, text as mk_text, ComposedSheet};
use crate::model::Ctx;
use crate::paper::{round_up_1_2_5, Scale};
use crate::text::Align;

/// A text of a detail drawing, in the drawing's own units, set at a paper height.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct DetailLabel {
    pub at: Pt2,
    pub text: String,
    /// Capital height on paper; `None` takes the style's label height.
    pub height_mm: Option<f64>,
    pub rotation_deg: f64,
    pub align: Align,
}

impl DetailLabel {
    pub fn new(at: Pt2, text: &str) -> DetailLabel {
        DetailLabel { at, text: text.to_string(), height_mm: None, rotation_deg: 0.0, align: Align::Left }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum DetailSource {
    /// The drawing a module generates for the element called this (any category), in millimetres.
    Element(String),
    /// A drawing handed in (stored with the view), with its labels.
    Inline { drawing: Drawing, labels: Vec<DetailLabel> },
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct DetailView {
    pub source: DetailSource,
    /// Scale 1:n of the drawing, where one drawing unit is `unit_mm` millimetres at full size.
    pub scale: Scale,
    /// Millimetres at full size per drawing unit: 1 for module drawings. For a chart such as the
    /// Gantt drawing, whose units are already sheet-sized, use scale 1:1 and unit 1.
    pub unit_mm: f64,
    /// Pick the scale automatically: the smallest of 1, 2, 5 times a power of ten that fits.
    pub fit: bool,
    pub caption: String,
}

impl DetailView {
    pub fn element(name: &str, scale: Scale, caption: &str) -> DetailView {
        DetailView { source: DetailSource::Element(name.to_string()), scale, unit_mm: 1.0, fit: false, caption: caption.to_string() }
    }

    pub fn inline(drawing: Drawing, labels: Vec<DetailLabel>, scale: Scale, unit_mm: f64, caption: &str) -> DetailView {
        DetailView { source: DetailSource::Inline { drawing, labels }, scale, unit_mm, fit: false, caption: caption.to_string() }
    }
}

/// Composes a detail view into `rect`. Returns the scale used.
pub fn compose_detail(sheet: &mut ComposedSheet, ctx: &Ctx, view: &DetailView, rect: &Rect) -> Option<Scale> {
    let styles = ctx.styles;
    let (drawing, labels): (Drawing, Vec<DetailLabel>) = match &view.source {
        DetailSource::Inline { drawing, labels } => (drawing.clone(), labels.clone()),
        DetailSource::Element(name) => {
            let Some(e) = ctx.model.doc.elements().iter().find(|e| &e.name == name) else {
                sheet.warn(format!("There is no element called '{name}' for a detail view."));
                return None;
            };
            match ctx.model.registry.generate(e) {
                Ok(g) => (g.drawing, Vec::new()),
                Err(err) => {
                    sheet.warn(format!("Element '{name}' has no drawing: {err}"));
                    return None;
                }
            }
        }
    };
    if view.unit_mm.is_nan() || view.unit_mm <= 0.0 || (!view.fit && !view.scale.is_valid()) {
        sheet.warn("A detail view needs a valid scale and unit.");
        return None;
    }
    let ext = drawing.extents;
    let caption_h = styles.text.heading * 2.6;
    let avail = Rect::new(rect.x0 + 4.0, rect.y0 + caption_h, rect.x1 - 4.0, rect.y1 - 4.0);
    let full = (ext.width() * view.unit_mm, ext.height() * view.unit_mm);
    let scale = if view.fit {
        let need = (full.0 / avail.width().max(1.0)).max(full.1 / avail.height().max(1.0));
        Scale(round_up_1_2_5(need.max(1e-6)))
    } else {
        view.scale
    };
    let k = view.unit_mm / scale.0;
    let (w, h) = (full.0 / scale.0, full.1 / scale.0);
    if w > avail.width() + 0.5 || h > avail.height() + 0.5 {
        sheet.warn(format!("The detail is {w:.0} x {h:.0} mm at {} but its area is {:.0} x {:.0} mm: it is cut at the edges.", scale.label(), avail.width(), avail.height()));
    }
    let c = avail.centre();
    let origin = Pt2::new(c.x - w / 2.0, c.y - h / 2.0);
    let m = Aff::scale_rotate(k, 0.0).then(&Aff::translate(origin.x - ext.min.x * k, origin.y - ext.min.y * k));
    draw_drawing(sheet, styles, &drawing, &m, &rect.inset(0.2), layers::DETAIL);
    let pen = styles.pens.pen(layers::DETAIL);
    for l in labels {
        let at = m.apply(l.at);
        if rect.contains(at) {
            sheet.text(mk_text(layers::DETAIL_TEXT, at, l.height_mm.unwrap_or(styles.text.label), l.text, l.align, l.rotation_deg, false, pen.rgb));
        }
    }
    let vp = styles.pens.pen(layers::VIEWPORT);
    sheet.rect(layers::VIEWPORT, rect, vp.weight_mm, vp.rgb);
    let caption = if view.caption.is_empty() { format!("DETAIL  {}", scale.label()) } else { format!("{}  {}", view.caption, scale.label()) };
    sheet.text(mk_text(layers::TEXT, Pt2::new(rect.centre().x, rect.y0 + styles.text.heading), styles.text.heading, caption, Align::Centre, 0.0, true, vp.rgb));
    Some(scale)
}
