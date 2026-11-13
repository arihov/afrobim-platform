//! Lays out one A3 landscape fabrication sheet. Port of C# `SheetComposer`.
//!
//! The sheet carries the sign face at a standard scale with its overall and panel dimensions, a
//! sign-data block (x-height, stroke width, size, face area, standard reference), the colour
//! schedule, and a title block.
//!
//! Sheet layout, in millimetres from the bottom-left corner:
//!
//! ```text
//!   10..300  drawing area: heading, the face and its dimensions
//!  300..410  right column: sign data and colour schedule above, title block below (10..80)
//! ```
//!
//! Dimensions are composed from lines, ticks and text rather than CAD DIMENSION entities, so the
//! same geometry draws in an R12 DXF and in the PDF.

use afrobim_draw::{Drawing, Primitive, Pt2};
use afrobim_signs::{Colour, Diagnostic, FontFace, Severity, SignDefinition, SignEngine, SignKind, SignLayout};

use crate::num::{fixed, utf16_len, utf16_prefix};
use crate::palette::{self, Palette};
use crate::quantities::{self, SignQuantity};
use crate::sheet::{
    ColourRow, DimensionOrientation, FabricationSheet, SheetBox, SheetDimension, SheetInfo, SheetLine, SheetSwatch, SheetText,
    SheetTextAlign, TitleField,
};

pub const FRAME_LAYER: &str = "SHEET";
pub const DIM_LAYER: &str = "DIM";
pub const TEXT_LAYER: &str = "TITLE";

/// Standard drawing scales, largest first; the composer picks the first that fits.
pub const SCALE_LADDER: [u32; 10] = [1, 2, 5, 10, 20, 25, 50, 100, 200, 500];

/// Inside the frame, left of the right-hand column.
pub const DRAWING_AREA: SheetBox = SheetBox::new(10.0, 10.0, 300.0, 287.0);

const COLUMN_X: f64 = 300.0;
const COLUMN_RIGHT: f64 = 410.0;
const TITLE_TOP: f64 = 80.0; // 7 rows x 10 mm down to the frame
const DIM_GAP: f64 = 9.0; // spacing between stacked dimension lines
const DIM_TEXT: f64 = 3.0; // dimension text height
const MAX_DIM_LEVELS: usize = 3;
const MIN_PANEL_SHARE: f64 = 0.04; // a panel is at least 4 % of the face area
const RECTANGULAR_SHARE: f64 = 0.85;
const MAX_PANELS: usize = 8;

/// Composes the sheet for a laid-out sign. `info` supplies the title-block fields.
pub fn compose(engine: &SignEngine, sign: &SignDefinition, layout: &SignLayout, info: &SheetInfo) -> FabricationSheet {
    let q = quantities::measure(engine, sign, layout);
    let palette = palette::resolve(sign, engine.pack());
    let sw_mm = layout.sw_mm;
    let face = &layout.drawing;

    let mut lines = Vec::new();
    let mut texts = Vec::new();
    let mut swatches = Vec::new();
    let mut dims = Vec::new();

    // ── panels and dimension levels (in sw, before the scale is known) ──
    let ext = face.extents;
    let panels = find_panels(face);
    let width_spans = levels(panels.iter().map(|p| (p.min_x, p.max_x)), ext.min.x, ext.max.x);
    let height_spans = levels(panels.iter().map(|p| (p.min_y, p.max_y)), ext.min.y, ext.max.y);
    let level_count = |spans: &[Span]| spans.iter().map(|s| s.level + 1).max().unwrap_or(0) as f64;
    let (top_levels, right_levels) = (level_count(&width_spans), level_count(&height_spans));

    // ── scale and face placement ──
    let avail = SheetBox::new(
        DRAWING_AREA.min_x + 28.0,
        DRAWING_AREA.min_y + 26.0,
        COLUMN_X - 10.0 - DIM_GAP * right_levels,
        DRAWING_AREA.max_y - 22.0 - DIM_GAP * top_levels,
    );
    let (w_mm, h_mm) = (ext.width() * sw_mm, ext.height() * sw_mm);
    let scale = SCALE_LADDER
        .into_iter()
        .find(|&n| w_mm / f64::from(n) <= avail.width() && h_mm / f64::from(n) <= avail.height())
        .unwrap_or(SCALE_LADDER[SCALE_LADDER.len() - 1]);
    let (fw, fh) = (w_mm / f64::from(scale), h_mm / f64::from(scale));
    let face_box = SheetBox::new(
        avail.centre_x() - fw / 2.0,
        avail.centre_y() - fh / 2.0,
        avail.centre_x() + fw / 2.0,
        avail.centre_y() + fh / 2.0,
    );
    let k = sw_mm / f64::from(scale);
    let sx = |sw: f64| face_box.min_x + (sw - ext.min.x) * k;
    let sy = |sw: f64| face_box.min_y + (sw - ext.min.y) * k;

    // ── dimensions ──
    let dim = |name: String, orientation, start, end, offset, value_mm| SheetDimension { name, orientation, start, end, offset, value_mm };
    dims.push(dim("overall-width".into(), DimensionOrientation::Horizontal, face_box.min_x, face_box.max_x, face_box.min_y - 12.0, w_mm));
    dims.push(dim("overall-height".into(), DimensionOrientation::Vertical, face_box.min_y, face_box.max_y, face_box.min_x - 12.0, h_mm));
    for (n, s) in width_spans.iter().enumerate() {
        dims.push(dim(
            format!("panel-width-{}", n + 1),
            DimensionOrientation::Horizontal,
            sx(s.min),
            sx(s.max),
            face_box.max_y + DIM_GAP * (s.level + 1) as f64,
            (s.max - s.min) * sw_mm,
        ));
    }
    for (n, s) in height_spans.iter().enumerate() {
        dims.push(dim(
            format!("panel-height-{}", n + 1),
            DimensionOrientation::Vertical,
            sy(s.min),
            sy(s.max),
            face_box.max_x + DIM_GAP * (s.level + 1) as f64,
            (s.max - s.min) * sw_mm,
        ));
    }
    for d in &dims {
        draw_dimension(d, &face_box, &mut lines, &mut texts);
    }

    // ── frame and drawing heading ──
    rect(&mut lines, &SheetBox::new(10.0, 10.0, 410.0, 287.0), FRAME_LAYER, 0.5);
    lines.push(line(COLUMN_X, 10.0, COLUMN_X, 287.0, FRAME_LAYER, 0.5));
    let type_name = type_name(sign);
    texts.push(SheetText::new(16.0, 278.0, 5.0, format!("{}  {}", sign.code, type_name), TEXT_LAYER).bold());
    texts.push(SheetText::new(
        16.0,
        272.0,
        2.5,
        format!("Sign face at 1:{scale}. Dimensions in mm, taken from the layout extents."),
        TEXT_LAYER,
    ));

    // ── title block ──
    let field = |label: &str, value: &str| TitleField { label: label.to_string(), value: value.to_string() };
    let title = vec![
        field("PROJECT", &info.project),
        field("SIGN CODE", &sign.code),
        field("DRAWING NO.", &info.drawing_number),
        field("SCALE", &format!("1:{scale}")),
        field("DATE", &info.date),
        field("DRAWN BY", &info.drawn_by),
        field("REVISION", &info.revision),
    ];
    draw_title_block(&title, &mut lines, &mut texts);

    // ── sign data and colour schedule ──
    let colours = colour_schedule(face, &palette);
    let reference = reference(engine, sign);
    draw_sign_data(
        &SignData { type_name: &type_name, pack_id: engine.pack_id(), q: &q, palette: &palette, reference: &reference, colours: &colours, diagnostics: &layout.diagnostics },
        &mut lines,
        &mut texts,
        &mut swatches,
    );

    FabricationSheet {
        sign: sign.clone(),
        face: face.clone(),
        diagnostics: layout.diagnostics.clone(),
        sw_mm,
        scale,
        face_box,
        quantity: q,
        reference,
        colours,
        title_block: title,
        dimensions: dims,
        lines,
        texts,
        swatches,
    }
}

// ─── panels ───────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Copy, PartialEq)]
struct SwBox {
    min_x: f64,
    min_y: f64,
    max_x: f64,
    max_y: f64,
}

impl SwBox {
    fn area(&self) -> f64 {
        (self.max_x - self.min_x) * (self.max_y - self.min_y)
    }
    fn of(points: &[Pt2]) -> SwBox {
        let fold = |f: fn(f64, f64) -> f64, pick: fn(&Pt2) -> f64| points.iter().map(pick).reduce(f).unwrap_or(0.0);
        SwBox { min_x: fold(f64::min, |p| p.x), min_y: fold(f64::min, |p| p.y), max_x: fold(f64::max, |p| p.x), max_y: fold(f64::max, |p| p.y) }
    }
}

/// The main panels of a face: filled, near-rectangular shapes covering at least
/// [`MIN_PANEL_SHARE`] of the face, other than the outer plate itself. That picks the ground
/// inside a border and the sections of a stack-type sign, and leaves out symbols, arrows and
/// lettering.
fn find_panels(face: &Drawing) -> Vec<SwBox> {
    let ext = face.extents;
    let face_area = ext.width() * ext.height();
    let outer = SwBox { min_x: ext.min.x, min_y: ext.min.y, max_x: ext.max.x, max_y: ext.max.y };
    let mut found: Vec<SwBox> = Vec::new();
    for prim in &face.primitives {
        let Primitive::Fill { outline, .. } = prim else { continue };
        if outline.len() < 4 {
            continue;
        }
        let b = SwBox::of(outline);
        if b.area() < MIN_PANEL_SHARE * face_area || same(&b, &outer) {
            continue;
        }
        if shoelace_area(outline).abs() < RECTANGULAR_SHARE * b.area() {
            continue;
        }
        if found.iter().any(|o| same(o, &b)) {
            continue;
        }
        found.push(b);
    }
    // Stable, largest first.
    found.sort_by(|a, b| b.area().partial_cmp(&a.area()).unwrap_or(std::cmp::Ordering::Equal));
    found.truncate(MAX_PANELS);
    found
}

fn same(a: &SwBox, b: &SwBox) -> bool {
    (a.min_x - b.min_x).abs() < 1e-6 && (a.min_y - b.min_y).abs() < 1e-6 && (a.max_x - b.max_x).abs() < 1e-6 && (a.max_y - b.max_y).abs() < 1e-6
}

fn shoelace_area(pts: &[Pt2]) -> f64 {
    let mut a = 0.0;
    for (i, p) in pts.iter().enumerate() {
        let q = pts[(i + 1) % pts.len()];
        a += p.x * q.y - q.x * p.y;
    }
    a / 2.0
}

#[derive(Debug, Clone, Copy, PartialEq)]
struct Span {
    min: f64,
    max: f64,
    level: usize,
}

/// Distinct spans (other than the overall one), each on the lowest dimension level where it does
/// not overlap a span already placed. Spans beyond [`MAX_DIM_LEVELS`] are dropped.
fn levels(spans: impl Iterator<Item = (f64, f64)>, overall_min: f64, overall_max: f64) -> Vec<Span> {
    let mut distinct: Vec<(f64, f64)> = Vec::new();
    for s in spans {
        if (s.0 - overall_min).abs() < 1e-6 && (s.1 - overall_max).abs() < 1e-6 {
            continue;
        }
        if distinct.iter().any(|d| (d.0 - s.0).abs() < 1e-6 && (d.1 - s.1).abs() < 1e-6) {
            continue;
        }
        distinct.push(s);
    }
    // Stable: by start, then longest first.
    distinct.sort_by(|a, b| {
        a.0.partial_cmp(&b.0).unwrap_or(std::cmp::Ordering::Equal).then((b.1 - b.0).partial_cmp(&(a.1 - a.0)).unwrap_or(std::cmp::Ordering::Equal))
    });

    let mut placed: Vec<Span> = Vec::new();
    for s in distinct {
        for level in 0..MAX_DIM_LEVELS {
            if placed.iter().any(|p| p.level == level && s.0 < p.max - 1e-6 && p.min < s.1 - 1e-6) {
                continue;
            }
            placed.push(Span { min: s.0, max: s.1, level });
            break;
        }
    }
    placed
}

// ─── drawing helpers ──────────────────────────────────────────────────────

fn line(x1: f64, y1: f64, x2: f64, y2: f64, layer: &'static str, weight: f64) -> SheetLine {
    SheetLine { x1, y1, x2, y2, layer, weight }
}

fn draw_dimension(d: &SheetDimension, face: &SheetBox, lines: &mut Vec<SheetLine>, texts: &mut Vec<SheetText>) {
    const EXT: f64 = 2.0;
    const GAP: f64 = 1.5;
    const TICK: f64 = 1.5;
    match d.orientation {
        DimensionOrientation::Horizontal => {
            let y = d.offset;
            let below = y < face.min_y;
            let edge = if below { face.min_y - GAP } else { face.max_y + GAP };
            let over = if below { y - EXT } else { y + EXT };
            for x in [d.start, d.end] {
                lines.push(line(x, edge, x, over, DIM_LAYER, 0.18));
                lines.push(line(x - TICK, y - TICK, x + TICK, y + TICK, DIM_LAYER, 0.35));
            }
            lines.push(line(d.start - EXT, y, d.end + EXT, y, DIM_LAYER, 0.18));
            texts.push(SheetText::new((d.start + d.end) / 2.0, y + 1.2, DIM_TEXT, d.label(), DIM_LAYER).aligned(SheetTextAlign::Centre));
        }
        DimensionOrientation::Vertical => {
            let x = d.offset;
            let left = x < face.min_x;
            let edge = if left { face.min_x - GAP } else { face.max_x + GAP };
            let over = if left { x - EXT } else { x + EXT };
            for y in [d.start, d.end] {
                lines.push(line(edge, y, over, y, DIM_LAYER, 0.18));
                lines.push(line(x - TICK, y - TICK, x + TICK, y + TICK, DIM_LAYER, 0.35));
            }
            lines.push(line(x, d.start - EXT, x, d.end + EXT, DIM_LAYER, 0.18));
            // Rotated 90° anticlockwise, the baseline runs up the sheet left of the line.
            texts.push(
                SheetText::new(x - 1.2, (d.start + d.end) / 2.0, DIM_TEXT, d.label(), DIM_LAYER).aligned(SheetTextAlign::Centre).rotated(90.0),
            );
        }
    }
}

fn draw_title_block(fields: &[TitleField], lines: &mut Vec<SheetLine>, texts: &mut Vec<SheetText>) {
    const ROW: f64 = 10.0;
    let top = TITLE_TOP;
    lines.push(line(COLUMN_X, top, COLUMN_RIGHT, top, FRAME_LAYER, 0.5));
    let mut y = top;
    for f in fields {
        texts.push(SheetText::new(COLUMN_X + 2.0, y - 3.2, 1.8, f.label.clone(), TEXT_LAYER));
        texts.push(fit(SheetText::new(COLUMN_X + 2.0, y - ROW + 2.0, 3.2, f.value.clone(), TEXT_LAYER).bold(), COLUMN_RIGHT - COLUMN_X - 4.0));
        y -= ROW;
        if y > 10.01 {
            lines.push(line(COLUMN_X, y, COLUMN_RIGHT, y, FRAME_LAYER, 0.25));
        }
    }
}

struct SignData<'a> {
    type_name: &'a str,
    pack_id: &'a str,
    q: &'a SignQuantity,
    palette: &'a Palette,
    reference: &'a str,
    colours: &'a [ColourRow],
    diagnostics: &'a [Diagnostic],
}

fn draw_sign_data(data: &SignData, lines: &mut Vec<SheetLine>, texts: &mut Vec<SheetText>, swatches: &mut Vec<SheetSwatch>) {
    let (x, value_x, width) = (COLUMN_X + 2.0, COLUMN_X + 40.0, COLUMN_RIGHT - COLUMN_X - 4.0);
    let mut y = 287.0 - 7.0;
    texts.push(SheetText::new(x, y, 3.5, "SIGN DATA", TEXT_LAYER).bold());
    y -= 8.0;

    let q = data.q;
    let rows: [(&str, String); 7] = [
        ("SIGN TYPE", data.type_name.to_string()),
        ("SIZE (W x H)", format!("{} x {} mm", format_mm(q.width_mm), format_mm(q.height_mm))),
        ("X-HEIGHT", format!("{} mm", format_mm(q.x_height_mm))),
        ("STROKE WIDTH (SW)", format!("{} mm", format_mm(q.sw_mm))),
        ("FACE AREA", format!("{} m2", fixed(q.area_m2(), 3, 3))),
        ("LETTERING", if data.palette.face == FontFace::TransportHeavy { "Transport Heavy" } else { "Transport Medium" }.to_string()),
        ("RULE PACK", data.pack_id.to_string()),
    ];
    for (label, value) in rows {
        texts.push(SheetText::new(x, y, 2.2, label, TEXT_LAYER));
        texts.push(fit(SheetText::new(value_x, y, 2.5, value, TEXT_LAYER).bold(), COLUMN_RIGHT - value_x - 2.0));
        y -= 5.5;
    }

    texts.push(SheetText::new(x, y, 2.2, "STANDARD REFERENCE", TEXT_LAYER));
    y -= 4.5;
    for l in wrap(data.reference, width, 2.2) {
        texts.push(SheetText::new(x, y, 2.2, l, TEXT_LAYER));
        y -= 3.6;
    }

    y -= 4.0;
    lines.push(line(COLUMN_X, y + 5.5, COLUMN_RIGHT, y + 5.5, FRAME_LAYER, 0.25));
    texts.push(SheetText::new(x, y, 3.5, "COLOUR SCHEDULE", TEXT_LAYER).bold());
    y -= 6.5;
    let (c_swatch, c_name, c_bs, c_ral) = (x + 22.0, x + 30.0, x + 54.0, x + 80.0);
    for (label, cx) in [("ROLE", x), ("COLOUR", c_name), ("BS 381C", c_bs), ("RAL", c_ral)] {
        texts.push(SheetText::new(cx, y, 2.0, label, TEXT_LAYER));
    }
    y -= 1.5;
    lines.push(line(x, y, COLUMN_RIGHT - 2.0, y, FRAME_LAYER, 0.18));
    y -= 4.5;
    for c in data.colours {
        texts.push(SheetText::new(x, y, 2.4, c.role.clone(), TEXT_LAYER));
        swatches.push(SheetSwatch { rect: SheetBox::new(c_swatch, y - 0.6, c_swatch + 6.0, y + 3.0), colour: c.colour });
        texts.push(SheetText::new(c_name, y, 2.4, colour_name(c.colour), TEXT_LAYER).bold());
        texts.push(SheetText::new(c_bs, y, 2.4, c.bs381c.clone(), TEXT_LAYER));
        texts.push(SheetText::new(c_ral, y, 2.4, c.ral.clone(), TEXT_LAYER));
        y -= 5.5;
    }

    let issues: Vec<&Diagnostic> = data.diagnostics.iter().filter(|d| d.severity != Severity::Info).collect();
    if !issues.is_empty() && y > TITLE_TOP + 12.0 {
        y -= 2.0;
        texts.push(SheetText::new(x, y, 2.2, "CHECK BEFORE CUTTING", TEXT_LAYER).bold());
        y -= 4.0;
        let room = (((y - TITLE_TOP - 3.0) / 3.6) as i64).max(1) as usize;
        for d in issues.into_iter().take(room) {
            texts.push(fit(SheetText::new(x, y, 2.0, format!("{:?}: {}", d.severity, d.code), TEXT_LAYER), width));
            y -= 3.6;
        }
    }
}

// ─── content ──────────────────────────────────────────────────────────────

/// Ground, border and legend colours, then any other colour drawn on the face (patches, panels,
/// symbols). The border is the outer plate's colour when it differs from the ground. The rule
/// packs carry no BS 381C or RAL references, so those cells are left blank.
fn colour_schedule(face: &Drawing, palette: &Palette) -> Vec<ColourRow> {
    let row = |role: &str, colour| ColourRow { role: role.to_string(), colour, bs381c: String::new(), ral: String::new() };
    let mut rows = vec![row("Ground", palette.ground)];
    let ext = face.extents;
    let outer = face.primitives.iter().find_map(|p| match p {
        Primitive::Fill { outline, colour } if outline.len() >= 3 => {
            let b = SwBox::of(outline);
            let fills = (b.min_x - ext.min.x).abs() < 1e-6
                && (b.max_x - ext.max.x).abs() < 1e-6
                && (b.min_y - ext.min.y).abs() < 1e-6
                && (b.max_y - ext.max.y).abs() < 1e-6;
            fills.then_some(*colour)
        }
        _ => None,
    });
    if let Some(border) = outer.filter(|c| *c != palette.ground) {
        rows.push(row("Border", border));
    }
    rows.push(row("Legend", palette.lettering));

    let mut used: Vec<Colour> = Vec::new();
    for p in &face.primitives {
        let c = match p {
            Primitive::Fill { colour, .. } => Some(*colour),
            Primitive::Glyphs { ink, .. } => *ink,
            Primitive::Line { colour, .. } | Primitive::Arc { colour, .. } => *colour,
            Primitive::Symbol { .. } => None,
        };
        if let Some(c) = c.filter(|c| !used.contains(c) && rows.iter().all(|r| r.colour != *c)) {
            used.push(c);
        }
    }
    used.sort_by_key(|c| palette::colour_index(*c));
    rows.extend(used.into_iter().map(|c| row("Other", c)));
    rows
}

fn reference(engine: &SignEngine, sign: &SignDefinition) -> String {
    let mut parts = Vec::new();
    if let Some(entry) = sign.catalog_key.as_deref().filter(|k| !k.is_empty()).and_then(|k| engine.catalog_entry(k)) {
        parts.push(format!("{}: {}.", entry.key, entry.name));
    }
    parts.push(format!("{}.", engine.pack().source));
    parts.join(" ")
}

/// The sign's type as the sheets and the schedule print it, with its catalog key when it has one.
pub fn type_name(sign: &SignDefinition) -> String {
    let name = match sign.kind {
        SignKind::StackType => "Stack-type",
        SignKind::MapTypeCrossroads => "Map-type, crossroads",
        SignKind::MapTypeRoundabout => "Map-type, roundabout",
        SignKind::MapTypeTee => "Map-type, T-junction",
        SignKind::DirectionSign => "Direction",
        SignKind::Confirmatory => "Route confirmatory",
        SignKind::PlaceIdentification => "Place identification",
        SignKind::LanePreselection => "Lane pre-selection",
        SignKind::Diversion => "Diversion",
        SignKind::InformationSign => "Information",
        SignKind::WarningSign => "Warning",
        SignKind::RegulatoryProhibitory => "Regulatory, prohibitory",
        SignKind::RegulatoryMandatory => "Regulatory, mandatory",
        SignKind::Stop => "Stop",
        SignKind::GiveWay => "Give way",
        SignKind::SupplementaryPlate => "Supplementary plate",
    };
    match sign.catalog_key.as_deref().filter(|k| !k.is_empty()) {
        Some(key) => format!("{name} ({key})"),
        None => name.to_string(),
    }
}

/// The colour's name as the colour schedule prints it.
pub fn colour_name(c: Colour) -> &'static str {
    palette::colour_name(c)
}

/// Whole millimetres when the value is within 0.05 mm of one, else one decimal.
pub fn format_mm(mm: f64) -> String {
    let whole = mm.round_ties_even();
    if (mm - whole).abs() < 0.05 {
        fixed(whole, 0, 0)
    } else {
        fixed(mm, 1, 1)
    }
}

// ─── text fitting (estimated widths; both renderers draw well inside them) ─

/// Average advance of a capital-height-`h` character, generously.
fn char_width(h: f64) -> f64 {
    0.78 * h
}

fn fit(mut t: SheetText, width: f64) -> SheetText {
    let len = utf16_len(&t.text);
    if len as f64 * char_width(t.height) <= width {
        return t;
    }
    let h = f64::max(1.8, width / (len as f64 * 0.78));
    let max = (width / char_width(h)) as i64;
    if len as i64 > max {
        t.text = utf16_prefix(&t.text, (max - 3).max(1) as usize) + "...";
    }
    t.height = h;
    t
}

fn wrap(text: &str, width: f64, h: f64) -> Vec<String> {
    // CAD's txt font runs wider than the PDF face, so wrap on a wider estimate.
    let max = ((width / (1.3 * char_width(h))) as i64).max(10) as usize;
    let mut out = Vec::new();
    let mut line = String::new();
    for word in text.split(' ').filter(|w| !w.is_empty()) {
        if !line.is_empty() && utf16_len(&line) + 1 + utf16_len(word) > max {
            out.push(std::mem::take(&mut line));
            line.push_str(word);
        } else {
            if !line.is_empty() {
                line.push(' ');
            }
            line.push_str(word);
        }
    }
    if !line.is_empty() {
        out.push(line);
    }
    out
}

fn rect(lines: &mut Vec<SheetLine>, b: &SheetBox, layer: &'static str, w: f64) {
    lines.push(line(b.min_x, b.min_y, b.max_x, b.min_y, layer, w));
    lines.push(line(b.max_x, b.min_y, b.max_x, b.max_y, layer, w));
    lines.push(line(b.max_x, b.max_y, b.min_x, b.max_y, layer, w));
    lines.push(line(b.min_x, b.max_y, b.min_x, b.min_y, layer, w));
}

#[cfg(test)]
mod tests {
    use super::*;
    use afrobim_draw::Extents;

    fn rect_fill(x0: f64, y0: f64, x1: f64, y1: f64, colour: Colour) -> Primitive {
        Primitive::Fill { outline: vec![Pt2::new(x0, y0), Pt2::new(x1, y0), Pt2::new(x1, y1), Pt2::new(x0, y1)], colour }
    }

    #[test]
    fn millimetres_print_whole_when_within_a_twentieth() {
        assert_eq!(format_mm(750.0), "750");
        assert_eq!(format_mm(749.96), "750");
        assert_eq!(format_mm(2303.75), "2303.8");
        assert_eq!(format_mm(1031.25), "1031.3");
        assert_eq!(format_mm(0.5), "0.5");
        assert_eq!(format_mm(31.25), "31.3");
    }

    #[test]
    fn type_names_carry_the_catalog_key() {
        let mut s = SignDefinition::stack("A", 60, vec![]);
        assert_eq!(type_name(&s), "Stack-type");
        s.kind = SignKind::WarningSign;
        s.catalog_key = Some("D512".into());
        assert_eq!(type_name(&s), "Warning (D512)");
        s.catalog_key = Some(String::new());
        assert_eq!(type_name(&s), "Warning");
    }

    #[test]
    fn panels_are_large_rectangles_other_than_the_plate() {
        let face = Drawing {
            primitives: vec![
                rect_fill(0.0, 0.0, 100.0, 50.0, Colour::White), // the plate
                rect_fill(1.0, 1.0, 99.0, 49.0, Colour::Green),  // ground inside the border
                rect_fill(1.0, 1.0, 99.0, 49.0, Colour::Green),  // a duplicate
                rect_fill(10.0, 10.0, 12.0, 12.0, Colour::Red),  // too small
                Primitive::Fill { outline: vec![Pt2::new(20.0, 5.0), Pt2::new(80.0, 5.0), Pt2::new(50.0, 45.0), Pt2::new(50.0, 44.0)], colour: Colour::Red }, // a triangle
                rect_fill(5.0, 5.0, 45.0, 45.0, Colour::Blue),
            ],
            extents: Extents { min: Pt2::new(0.0, 0.0), max: Pt2::new(100.0, 50.0) },
        };
        let panels = find_panels(&face);
        assert_eq!(panels.len(), 2);
        assert_eq!(panels[0], SwBox { min_x: 1.0, min_y: 1.0, max_x: 99.0, max_y: 49.0 });
        assert_eq!(panels[1].max_x, 45.0);
    }

    #[test]
    fn spans_stack_on_the_lowest_free_level() {
        let spans = [(0.0, 100.0), (0.0, 40.0), (40.0, 100.0), (20.0, 60.0), (0.0, 40.0), (10.0, 30.0), (15.0, 25.0), (16.0, 24.0)];
        let placed = levels(spans.into_iter(), 0.0, 100.0);
        // The overall span and the duplicate are dropped; a span that overlaps one on every level
        // (16..24 and 20..60) is not dimensioned.
        let got: Vec<(f64, f64, usize)> = placed.iter().map(|s| (s.min, s.max, s.level)).collect();
        assert_eq!(got, [(0.0, 40.0, 0), (10.0, 30.0, 1), (15.0, 25.0, 2), (40.0, 100.0, 0)]);
    }

    #[test]
    fn long_values_shrink_then_truncate() {
        let short = fit(SheetText::new(0.0, 0.0, 3.2, "A109", TEXT_LAYER), 106.0);
        assert_eq!((short.height, short.text.as_str()), (3.2, "A109"));
        let long = fit(SheetText::new(0.0, 0.0, 3.2, "x".repeat(60), TEXT_LAYER), 106.0);
        assert!(long.height < 3.2 && long.height > 1.8);
        let very_long = fit(SheetText::new(0.0, 0.0, 3.2, "x".repeat(200), TEXT_LAYER), 106.0);
        assert_eq!(very_long.height, 1.8);
        assert!(very_long.text.ends_with("...") && very_long.text.len() == 75);
    }

    #[test]
    fn wrap_breaks_between_words() {
        let lines = wrap("UK Traffic Signs Manual Chapter 7 (2019) and TSRGD 2016  Schedule 2.", 30.0, 2.2);
        assert!(lines.len() > 1);
        assert!(lines.iter().all(|l| l.len() <= 13 || !l.contains(' ')));
        assert_eq!(lines.join(" "), "UK Traffic Signs Manual Chapter 7 (2019) and TSRGD 2016 Schedule 2.");
    }

    #[test]
    fn colour_schedule_lists_ground_border_legend_then_others() {
        let face = Drawing {
            primitives: vec![
                rect_fill(0.0, 0.0, 10.0, 10.0, Colour::White),
                rect_fill(1.0, 1.0, 9.0, 9.0, Colour::Green),
                Primitive::Line { from: Pt2::new(1.0, 1.0), to: Pt2::new(2.0, 2.0), width: 1.0, colour: Some(Colour::Red) },
                rect_fill(2.0, 2.0, 3.0, 3.0, Colour::Blue),
                Primitive::Glyphs { contours: vec![], text: "x".into(), ink: None },
            ],
            extents: Extents { min: Pt2::new(0.0, 0.0), max: Pt2::new(10.0, 10.0) },
        };
        let palette = Palette { ground: Colour::Green, lettering: Colour::White, arm: Colour::White, face: FontFace::TransportMedium };
        let rows = colour_schedule(&face, &palette);
        let got: Vec<(&str, Colour)> = rows.iter().map(|r| (r.role.as_str(), r.colour)).collect();
        assert_eq!(got, [("Ground", Colour::Green), ("Border", Colour::White), ("Legend", Colour::White), ("Other", Colour::Blue), ("Other", Colour::Red)]);
    }
}
