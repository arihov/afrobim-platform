//! Where windows open. Every floating window of the app is made here, so that none can open over
//! the menu bar or the ribbon, or be dragged under them: each opens just inside the central area
//! (the part of the screen left once the menu, ribbon, toolbar, side panels and status bar have
//! taken theirs) and is kept inside it, so its title bar always stays reachable.

use egui::{vec2, Context, Id, Pos2, Rect, WidgetText};

const WORK_RECT: &str = "afrobim-work-rect";

/// Remembers the central area for this frame; called once a frame by the canvas, before any window.
pub fn set_work_rect(ctx: &Context, rect: Rect) {
    ctx.data_mut(|d| d.insert_temp(Id::new(WORK_RECT), rect));
}

/// The central area, or the whole screen before the first frame has recorded it.
pub fn work_rect(ctx: &Context) -> Rect {
    ctx.data(|d| d.get_temp::<Rect>(Id::new(WORK_RECT))).unwrap_or_else(|| ctx.screen_rect())
}

/// Where a new window opens: the top left of the central area, a little in from its edges.
pub fn first_position(work: Rect) -> Pos2 {
    work.min + vec2(16.0, 12.0)
}

/// A window that opens inside the central area and cannot leave it. Its size is capped to the
/// area, so a window made for a large screen shrinks (and scrolls) rather than hang off the top.
pub fn new<'a>(ctx: &Context, title: impl Into<WidgetText>) -> egui::Window<'a> {
    let work = work_rect(ctx);
    egui::Window::new(title).constrain_to(work).default_pos(first_position(work)).max_size(work.size() - vec2(8.0, 8.0))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_first_position_is_inside_the_central_area_below_the_ribbon() {
        // A screen 1440 x 860 with 150 px of menu and ribbon on top and 200 px of tool panel on the
        // left: windows start at the central area's corner (plus the margin), not at the screen's.
        let work = Rect::from_min_max(Pos2::new(200.0, 150.0), Pos2::new(1100.0, 820.0));
        let p = first_position(work);
        assert!(work.contains(p) && p.y >= 150.0 && p.x >= 200.0);
    }

    #[test]
    fn a_window_made_here_opens_below_the_ribbon_and_inside_the_area_even_when_too_tall() {
        let ctx = Context::default();
        let work = Rect::from_min_max(Pos2::new(200.0, 150.0), Pos2::new(1100.0, 820.0));
        let input = || egui::RawInput { screen_rect: Some(Rect::from_min_size(Pos2::ZERO, vec2(1440.0, 860.0))), ..Default::default() };
        // A window that asks for 2000 px of height on a 670 px area, opened with no position given.
        for _ in 0..3 {
            let _ = ctx.run(input(), |ctx| {
                set_work_rect(ctx, work);
                new(ctx, "Test window").default_width(900.0).default_height(2000.0).show(ctx, |ui| {
                    ui.label("body");
                });
            });
        }
        let r = ctx.memory(|m| m.area_rect(Id::new("Test window"))).expect("the window was shown");
        assert!(r.min.y >= work.min.y - 0.5 && r.min.x >= work.min.x - 0.5, "{r:?}");
        assert!(r.max.y <= work.max.y + 0.5 && r.max.x <= work.max.x + 0.5, "{r:?}");
        // Without a recorded area the screen is used.
        let fresh = Context::default();
        assert_eq!(work_rect(&fresh), fresh.screen_rect());
    }
}
