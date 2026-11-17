//! Where windows open. Every floating window of the app is made here, so that none can open over
//! the menu bar or the ribbon, or be dragged under them: each opens just inside the central area
//! (the part of the screen left once the menu, ribbon, toolbar, side panels and status bar have
//! taken theirs) and is kept inside it, so its title bar always stays reachable.

use egui::{vec2, Context, Id, Pos2, Rect};

const WORK_RECT: &str = "afrobim-work-rect";

/// Remembers the central area for this frame; called once a frame by the canvas, before any window.
pub fn set_work_rect(ctx: &Context, rect: Rect) {
    ctx.data_mut(|d| d.insert_temp(Id::new(WORK_RECT), rect));
}

/// The central area, or the whole screen before the first frame has recorded it.
pub fn work_rect(ctx: &Context) -> Rect {
    ctx.data(|d| d.get_temp::<Rect>(Id::new(WORK_RECT))).unwrap_or_else(|| ctx.screen_rect())
}

/// The area a window may be moved and sized in: the height of the central area (below the menu,
/// ribbon and toolbar, above the status bar) and the width of the whole screen, so that a dialog
/// wider than the central area (the side panels take a lot of a small screen) can lie over the panels.
pub fn area_rect(ctx: &Context) -> Rect {
    let work = work_rect(ctx);
    let screen = ctx.screen_rect();
    Rect::from_min_max(Pos2::new(screen.min.x.min(work.min.x), work.min.y), Pos2::new(screen.max.x.max(work.max.x), work.max.y))
}

/// Where a new window opens: the top left of the central area, a little in from its edges.
pub fn first_position(work: Rect) -> Pos2 {
    work.min + vec2(16.0, 12.0)
}

/// The icon a window title gets in its title bar: the first keyword the title contains.
pub fn icon_for(title: &str) -> &'static str {
    use egui_phosphor::regular as ph;
    const TABLE: &[(&str, &str)] = &[
        ("layer", ph::STACK),
        ("units", ph::RULER),
        ("project settings", ph::GEAR),
        ("project information", ph::IDENTIFICATION_CARD),
        ("landxml", ph::PATH),
        ("survey", ph::MAP_PIN_AREA),
        ("corridor", ph::ROAD_HORIZON),
        ("cross-section", ph::CHART_LINE),
        ("align", ph::BEZIER_CURVE),
        ("pi table", ph::TABLE),
        ("profile", ph::CHART_LINE_UP),
        ("clash", ph::LIST_MAGNIFYING_GLASS),
        ("issue", ph::CHAT_TEARDROP_TEXT),
        ("bcf", ph::CHAT_TEARDROP_TEXT),
        ("reference", ph::CUBE_FOCUS),
        ("ifc", ph::CUBE_TRANSPARENT),
        ("programme", ph::LIST_CHECKS),
        ("schedule", ph::CALENDAR),
        ("calendar", ph::CALENDAR),
        ("gantt", ph::CHART_BAR_HORIZONTAL),
        ("baseline", ph::FLAG_BANNER),
        ("variance", ph::PLUS_MINUS),
        ("progress", ph::CHART_LINE_UP),
        ("wbs", ph::TREE_STRUCTURE),
        ("critical", ph::FLOW_ARROW),
        ("time-location", ph::CHART_SCATTER),
        ("4d", ph::CLOCK_COUNTDOWN),
        ("bill", ph::RECEIPT),
        ("rate", ph::COINS),
        ("currency", ph::CURRENCY_CIRCLE_DOLLAR),
        ("estimate", ph::CALCULATOR),
        ("cash flow", ph::CHART_LINE),
        ("certificate", ph::CERTIFICATE),
        ("earned value", ph::GAUGE),
        ("site", ph::NOTE_PENCIL),
        ("roll up", ph::ARROWS_MERGE),
        ("look-ahead", ph::BINOCULARS),
        ("sheet", ph::FILES),
        ("title block", ph::SQUARE_HALF_BOTTOM),
        ("report", ph::FILE_PDF),
        ("check", ph::SEAL_CHECK),
        ("calculation", ph::CALCULATOR),
        ("visibility", ph::EYE),
        ("photometry", ph::SUN_HORIZON),
        ("lighting", ph::LIGHTBULB_FILAMENT),
        ("lamp", ph::LAMP),
        ("barrier", ph::BARRICADE),
        ("pavement", ph::STACK),
        ("marking", ph::ROAD_HORIZON),
        ("support", ph::ANCHOR_SIMPLE),
        ("gantry", ph::BRIDGE),
        ("sign", ph::TRAFFIC_SIGN),
        ("text", ph::TEXT_T),
        ("dimension", ph::ARROWS_HORIZONTAL),
        ("measure", ph::RULER),
        ("snap", ph::CROSSHAIR_SIMPLE),
        ("underlay", ph::IMAGE_SQUARE),
        ("commands", ph::COMMAND),
        ("about", ph::INFO),
        ("export", ph::EXPORT),
        ("import", ph::FILE_ARROW_DOWN),
        ("dropped", ph::FILE_ARROW_DOWN),
        ("appearance", ph::PALETTE),
        ("add a", ph::PLUS_CIRCLE),
        ("place", ph::MAP_PIN_LINE),
        ("hang", ph::LINK_SIMPLE),
        ("working", ph::SPINNER),
    ];
    let t = title.to_lowercase();
    TABLE.iter().find(|(k, _)| t.contains(k)).map(|(_, g)| *g).unwrap_or(ph::APP_WINDOW)
}

static POSITIONS: std::sync::Mutex<Option<std::collections::BTreeMap<String, [f32; 2]>>> = std::sync::Mutex::new(None);
static DIRTY: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);
thread_local! {
    static SEEN: std::cell::RefCell<Vec<String>> = const { std::cell::RefCell::new(Vec::new()) };
}

/// Gives the window positions kept from the last run (title to x, y).
pub fn load_positions(map: std::collections::BTreeMap<String, [f32; 2]>) {
    if let Ok(mut p) = POSITIONS.lock() {
        *p = Some(map);
    }
}

/// The positions, when any window was moved since the last call.
pub fn take_changed_positions() -> Option<std::collections::BTreeMap<String, [f32; 2]>> {
    if DIRTY.swap(false, std::sync::atomic::Ordering::Relaxed) {
        POSITIONS.lock().ok().and_then(|p| p.clone())
    } else {
        None
    }
}

fn saved_position(title: &str) -> Option<Pos2> {
    POSITIONS.lock().ok()?.as_ref()?.get(title).map(|p| Pos2::new(p[0], p[1]))
}

/// Called once a frame after the windows: remembers where each window shown this frame now is.
pub fn end_frame(ctx: &Context) {
    let seen = SEEN.with(|s| std::mem::take(&mut *s.borrow_mut()));
    for title in seen {
        let Some(rect) = ctx.memory(|m| m.area_rect(Id::new(title.as_str()))) else { continue };
        let now = [rect.min.x.round(), rect.min.y.round()];
        if let Ok(mut p) = POSITIONS.lock() {
            let map = p.get_or_insert_with(Default::default);
            if map.get(&title) != Some(&now) {
                map.insert(title, now);
                DIRTY.store(true, std::sync::atomic::Ordering::Relaxed);
            }
        }
    }
}

/// A window with the same placement rules and frame and no title bar decoration, for the dialog
/// kit, which draws its own header.
pub fn plain<'a>(ctx: &Context, title: &str) -> egui::Window<'a> {
    place(ctx, egui::Window::new(title.to_string()), title)
}

fn place<'a>(ctx: &Context, win: egui::Window<'a>, title: &str) -> egui::Window<'a> {
    let work = work_rect(ctx);
    let area = area_rect(ctx);
    SEEN.with(|s| s.borrow_mut().push(title.to_string()));
    let id = Id::new(title);
    let win = win.id(id).constrain_to(area).max_size(area.size() - vec2(8.0, 8.0)).frame(crate::theme::window_frame(ctx));
    // A saved position applies only on the very first frame of the window; after that the window is its own.
    match saved_position(title) {
        Some(p) if ctx.memory(|m| m.area_rect(id)).is_none() => win.current_pos(clamp_to(area, p)),
        _ => win.default_pos(first_position(work)),
    }
}

fn clamp_to(work: Rect, p: Pos2) -> Pos2 {
    Pos2::new(p.x.clamp(work.min.x, (work.max.x - 120.0).max(work.min.x)), p.y.clamp(work.min.y, (work.max.y - 60.0).max(work.min.y)))
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
        let ctx = crate::theme::test_context();
        let work = Rect::from_min_max(Pos2::new(200.0, 150.0), Pos2::new(1100.0, 820.0));
        let input = || egui::RawInput { screen_rect: Some(Rect::from_min_size(Pos2::ZERO, vec2(1440.0, 860.0))), ..Default::default() };
        // A window that asks for 2000 px of height on a 670 px area, opened with no position given.
        for _ in 0..3 {
            let _ = ctx.run(input(), |ctx| {
                set_work_rect(ctx, work);
                plain(ctx, "Test window").default_width(900.0).default_height(2000.0).show(ctx, |ui| {
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
