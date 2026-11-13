//! Toolbar icons as polylines in a 16 × 16 box (Y down), so they stay crisp at any scale and
//! follow the theme colour. No icon font or image asset is shipped.

use egui::{pos2, Color32, Pos2, Rect, Response, Sense, Shape, Stroke, Ui, Vec2};

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Icon {
    New,
    Open,
    Save,
    Undo,
    Redo,
    Move,
    Copy,
    Erase,
    ZoomExtents,
    ZoomWindow,
    ZoomIn,
    ZoomOut,
    Arrange,
    Theme,
    Stack,
    Map,
    Warning,
    Regulatory,
    Information,
    Supplementary,
    Direction,
    Confirmatory,
    Lane,
    Tee,
    Roundabout,
}

type Poly = &'static [(f32, f32)];

fn circle(cx: f32, cy: f32, r: f32) -> Vec<(f32, f32)> {
    (0..=20).map(|i| {
        let a = i as f32 / 20.0 * std::f32::consts::TAU;
        (cx + r * a.cos(), cy + r * a.sin())
    }).collect()
}

fn lines(icon: Icon) -> Vec<Vec<(f32, f32)>> {
    let s = |p: &[Poly]| p.iter().map(|l| l.to_vec()).collect::<Vec<_>>();
    match icon {
        Icon::New => s(&[&[(4., 2.), (10., 2.), (13., 5.), (13., 14.), (4., 14.), (4., 2.)], &[(10., 2.), (10., 5.), (13., 5.)]]),
        Icon::Open => s(&[&[(2., 13.), (2., 4.), (6., 4.), (7.5, 5.5), (13., 5.5), (13., 7.)], &[(2., 13.), (4., 7.), (14.5, 7.), (12.5, 13.), (2., 13.)]]),
        Icon::Save => s(&[&[(3., 3.), (11., 3.), (13., 5.), (13., 13.), (3., 13.), (3., 3.)], &[(5., 3.), (5., 6.5), (10., 6.5), (10., 3.)], &[(5., 13.), (5., 9.5), (11., 9.5), (11., 13.)]]),
        Icon::Undo => s(&[&[(6., 4.), (3., 7.), (6., 10.)], &[(3., 7.), (10., 7.), (12.5, 8.), (13.5, 10.5), (12.5, 13.), (10., 14.), (7., 14.)]]),
        Icon::Redo => s(&[&[(10., 4.), (13., 7.), (10., 10.)], &[(13., 7.), (6., 7.), (3.5, 8.), (2.5, 10.5), (3.5, 13.), (6., 14.), (9., 14.)]]),
        Icon::Move => s(&[&[(8., 2.), (8., 14.)], &[(2., 8.), (14., 8.)], &[(6., 4.), (8., 2.), (10., 4.)], &[(6., 12.), (8., 14.), (10., 12.)], &[(4., 6.), (2., 8.), (4., 10.)], &[(12., 6.), (14., 8.), (12., 10.)]]),
        Icon::Copy => s(&[&[(5., 5.), (13., 5.), (13., 14.), (5., 14.), (5., 5.)], &[(3., 11.), (3., 2.), (11., 2.)]]),
        Icon::Erase => s(&[&[(6., 13.5), (2.5, 10.), (9., 3.5), (13.5, 8.), (8., 13.5), (6., 13.5)], &[(5.8, 6.8), (10.2, 11.2)], &[(8., 13.5), (14., 13.5)]]),
        Icon::ZoomExtents => s(&[&[(2., 6.), (2., 2.), (6., 2.)], &[(10., 2.), (14., 2.), (14., 6.)], &[(14., 10.), (14., 14.), (10., 14.)], &[(6., 14.), (2., 14.), (2., 10.)], &[(6., 6.), (10., 6.), (10., 10.), (6., 10.), (6., 6.)]]),
        Icon::ZoomWindow => vec![circle(7., 7., 5.), vec![(10.6, 10.6), (14., 14.)], vec![(4.8, 5.2), (9.2, 5.2), (9.2, 8.8), (4.8, 8.8), (4.8, 5.2)]],
        Icon::ZoomIn => vec![circle(7., 7., 5.), vec![(10.6, 10.6), (14., 14.)], vec![(4.8, 7.), (9.2, 7.)], vec![(7., 4.8), (7., 9.2)]],
        Icon::ZoomOut => vec![circle(7., 7., 5.), vec![(10.6, 10.6), (14., 14.)], vec![(4.8, 7.), (9.2, 7.)]],
        Icon::Arrange => s(&[&[(2., 3.), (7., 3.), (7., 7.), (2., 7.), (2., 3.)], &[(9., 3.), (14., 3.), (14., 7.), (9., 7.), (9., 3.)], &[(2., 9.), (7., 9.), (7., 13.), (2., 13.), (2., 9.)], &[(9., 9.), (14., 9.), (14., 13.), (9., 13.), (9., 9.)]]),
        Icon::Theme => vec![circle(8., 8., 6.), vec![(8., 2.), (8., 14.)], vec![(8., 5.), (12.6, 5.)], vec![(8., 8.), (14., 8.)], vec![(8., 11.), (12.6, 11.)]],
        Icon::Stack => s(&[&[(2., 3.), (14., 3.), (14., 13.), (2., 13.), (2., 3.)], &[(2., 8.), (14., 8.)], &[(4., 5.5), (9., 5.5)], &[(4., 10.5), (9., 10.5)], &[(11.5, 4.5), (11.5, 6.5)], &[(11.5, 9.5), (11.5, 11.5)]]),
        Icon::Map => s(&[&[(2., 2.), (14., 2.), (14., 14.), (2., 14.), (2., 2.)], &[(8., 12.5), (8., 4.)], &[(4., 8.5), (12., 8.5)], &[(6.5, 5.5), (8., 4.), (9.5, 5.5)]]),
        Icon::Warning => s(&[&[(8., 2.5), (14., 13.), (2., 13.), (8., 2.5)], &[(8., 6.5), (8., 9.6)], &[(8., 11.1), (8., 11.5)]]),
        Icon::Regulatory => vec![circle(8., 8., 5.5), vec![(4.2, 11.8), (11.8, 4.2)]],
        Icon::Information => s(&[&[(3., 2.), (13., 2.), (13., 14.), (3., 14.), (3., 2.)], &[(8., 7.2), (8., 11.2)], &[(8., 4.7), (8., 5.1)]]),
        Icon::Supplementary => s(&[&[(2., 5.), (14., 5.), (14., 11.), (2., 11.), (2., 5.)], &[(4.5, 8.), (11.5, 8.)]]),
        Icon::Direction => s(&[&[(2., 5.), (14., 5.), (14., 11.), (2., 11.), (2., 5.)], &[(4., 8.), (8., 8.)], &[(10., 8.), (12.5, 8.)], &[(11., 6.5), (12.5, 8.), (11., 9.5)]]),
        Icon::Confirmatory => s(&[&[(2., 3.), (14., 3.), (14., 13.), (2., 13.), (2., 3.)], &[(4., 6.), (8.5, 6.)], &[(4., 10.), (8.5, 10.)], &[(10.5, 6.), (12., 6.)], &[(10.5, 10.), (12., 10.)]]),
        Icon::Tee => s(&[&[(2., 2.), (14., 2.), (14., 14.), (2., 14.), (2., 2.)], &[(8., 12.5), (8., 6.5)], &[(4., 6.5), (12., 6.5)]]),
        Icon::Roundabout => vec![vec![(2., 2.), (14., 2.), (14., 14.), (2., 14.), (2., 2.)], circle(8., 7.5, 2.6), vec![(8., 12.5), (8., 10.1)], vec![(8., 4.9), (8., 3.5)], vec![(5.4, 7.5), (4., 7.5)], vec![(10.6, 7.5), (12., 7.5)]],
        Icon::Lane => s(&[&[(2., 3.), (14., 3.), (14., 13.), (2., 13.), (2., 3.)], &[(6., 3.), (6., 13.)], &[(10., 3.), (10., 13.)], &[(4., 10.), (4., 6.)], &[(8., 10.), (8., 6.)], &[(12., 10.), (12., 6.)]]),
    }
}

/// Paints `icon` inside `rect` (drawn in the largest centred square).
pub fn paint(painter: &egui::Painter, rect: Rect, icon: Icon, colour: Color32) {
    let side = rect.width().min(rect.height());
    let k = side / 16.0;
    let o = rect.center() - Vec2::splat(side / 2.0);
    let stroke = Stroke::new((1.25 * k).max(1.0), colour);
    for line in lines(icon) {
        let pts: Vec<Pos2> = line.iter().map(|(x, y)| pos2(o.x + x * k, o.y + y * k)).collect();
        painter.add(Shape::line(pts, stroke));
    }
}

/// A square icon button for the toolbar.
pub fn button(ui: &mut Ui, icon: Icon, tip: &str) -> Response {
    let (rect, response) = ui.allocate_exact_size(Vec2::new(28.0, 26.0), Sense::click());
    let visuals = ui.style().interact(&response);
    if response.hovered() || response.is_pointer_button_down_on() {
        ui.painter().rect_filled(rect, 4.0, visuals.bg_fill);
    }
    let colour = if ui.is_enabled() { ui.visuals().text_color() } else { ui.visuals().weak_text_color() };
    paint(ui.painter(), rect.shrink2(Vec2::new(6.0, 5.0)), icon, colour);
    response.on_hover_text(tip)
}

/// An icon + caption row button for the tool palette.
pub fn palette_button(ui: &mut Ui, icon: Icon, label: &str, tip: &str) -> Response {
    let width = ui.available_width();
    let (rect, response) = ui.allocate_exact_size(Vec2::new(width, 26.0), Sense::click());
    let visuals = ui.style().interact(&response);
    if response.hovered() || response.is_pointer_button_down_on() {
        ui.painter().rect_filled(rect, 4.0, visuals.bg_fill);
    }
    let colour = if ui.is_enabled() { ui.visuals().text_color() } else { ui.visuals().weak_text_color() };
    let icon_rect = Rect::from_min_size(rect.min + Vec2::new(8.0, 5.0), Vec2::splat(16.0));
    paint(ui.painter(), icon_rect, icon, colour);
    ui.painter().text(
        pos2(icon_rect.right() + 8.0, rect.center().y),
        egui::Align2::LEFT_CENTER,
        label,
        egui::FontId::proportional(13.0),
        colour,
    );
    response.on_hover_text(tip)
}
