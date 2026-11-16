//! The CAD interaction state machine: pointer and keyboard input in; selection changes, moves,
//! view changes, prompts and [`CanvasEvent`]s out. It holds no GUI types.

use crate::drafting::{ortho, parse_point, snap};
use crate::{BBox, Document, ElementId, Pt, SelectMode, ViewTransform};

/// What the canvas is waiting for.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CanvasTool {
    /// No command: click to select, drag to box-select or move, grips are live.
    Idle,
    /// A selection box is open; the next point (or the mouse release) closes it.
    BoxSecond,
    /// The selection is being dragged with the button held.
    DragMove,
    /// A grip is being dragged with the button held.
    GripDrag,
    /// A command is collecting its selection set.
    SelectObjects,
    /// MOVE / COPY is waiting for the base point.
    BasePoint,
    /// MOVE / COPY is waiting for the second point.
    SecondPoint,
    ZoomFirst,
    ZoomSecond,
    /// A new element hangs from the cursor, waiting for its insertion point.
    Place,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CanvasCommand {
    None,
    Move,
    Copy,
    Erase,
}

/// Something the owner must act on or record. Drain with [`CanvasController::take_events`].
#[derive(Debug, Clone, PartialEq)]
pub enum CanvasEvent {
    /// A line for the command history.
    Echo(String),
    /// An undoable edit finished.
    Committed,
    /// ERASE confirmed for these elements.
    Erase(Vec<ElementId>),
    /// COPY placed a copy of these elements displaced by (dx, dy).
    Copy(Vec<ElementId>, f64, f64),
    /// Double-click: open the element's properties.
    Details(ElementId),
}

pub struct CanvasController {
    // settings
    pub snap_on: bool,
    pub ortho_on: bool,
    pub snap_spacing: f64,
    /// Half the pickbox side, px.
    pub pick_box_px: f64,
    /// Half the grip side, px.
    pub grip_px: f64,
    /// Screen rectangle of the drawing area: left, top, width, height.
    pub viewport: (f64, f64, f64, f64),

    // state the canvas draws
    tool: CanvasTool,
    command: CanvasCommand,
    cursor_screen: Pt,
    cursor_world: Pt,
    box_start: Option<Pt>,
    base_point: Option<Pt>,
    displacement: Pt,
    ghost_ids: Vec<ElementId>,
    hover: Option<ElementId>,
    hover_grip: Option<(ElementId, usize)>,
    prompt: String,

    press_on_item: bool,
    press_screen: Pt,
    press_world: Pt,
    /// The object snap point under the pointer when it was pressed on an element: the base point
    /// of the press-drag move, as the base point of MOVE is.
    press_snap: Option<Pt>,
    box_pressed: bool,
    box_removes: bool,
    box_return: CanvasTool,
    place_id: Option<ElementId>,
    /// A model point from object snap, which stands for the pointer while a point is being picked.
    object_snap: Option<Pt>,
    events: Vec<CanvasEvent>,
    /// Views ZOOM Previous returns to, oldest first.
    view_history: Vec<ViewTransform>,
}

/// The displacement of a press-drag move of the selection: the pointer at `raw`, the grab point
/// `base`. An object snap point (`object_snap`) is where the grab point lands, exactly: the
/// displacement is the vector from the grab point to it, as a MOVE with that second point has, and
/// ortho and the grid do not move it. With none, the displacement follows the pointer, kept to an
/// axis by ortho and rounded to the grid spacing. The displacement is snapped, not the pointer:
/// the grab point is arbitrary.
pub fn drag_displacement(base: Pt, raw: Pt, object_snap: Option<Pt>, ortho_on: bool, snap_on: bool, spacing: f64) -> Pt {
    if let Some(target) = object_snap {
        return target - base;
    }
    let mut d = raw - base;
    if ortho_on {
        d = ortho(Pt::default(), d);
    }
    if snap_on {
        d = snap(d, spacing);
    }
    d
}

impl Default for CanvasController {
    fn default() -> Self {
        Self {
            snap_on: false,
            ortho_on: false,
            snap_spacing: 50.0,
            pick_box_px: 5.0,
            grip_px: 5.0,
            viewport: (0.0, 0.0, 800.0, 600.0),
            tool: CanvasTool::Idle,
            command: CanvasCommand::None,
            cursor_screen: Pt::default(),
            cursor_world: Pt::default(),
            box_start: None,
            base_point: None,
            displacement: Pt::default(),
            ghost_ids: Vec::new(),
            hover: None,
            hover_grip: None,
            prompt: String::new(),
            press_on_item: false,
            press_screen: Pt::default(),
            press_world: Pt::default(),
            press_snap: None,
            box_pressed: false,
            box_removes: false,
            box_return: CanvasTool::Idle,
            place_id: None,
            object_snap: None,
            events: Vec::new(),
            view_history: Vec::new(),
        }
    }
}

impl CanvasController {
    /// Pointer travel (px) that turns a press into a drag.
    pub const DRAG_THRESHOLD_PX: f64 = 4.0;

    pub fn tool(&self) -> CanvasTool {
        self.tool
    }
    pub fn command(&self) -> CanvasCommand {
        self.command
    }
    pub fn cursor_screen(&self) -> Pt {
        self.cursor_screen
    }
    /// The cursor in model space, after snap and ortho where they apply.
    pub fn cursor_world(&self) -> Pt {
        self.cursor_world
    }
    pub fn box_start(&self) -> Option<Pt> {
        self.box_start
    }
    /// Right-to-left boxes are crossing boxes.
    pub fn box_is_crossing(&self) -> bool {
        self.box_start.is_some_and(|s| self.cursor_screen.x < s.x)
    }
    pub fn base_point(&self) -> Option<Pt> {
        self.base_point
    }
    /// Displacement currently previewed for [`Self::ghost_ids`].
    pub fn displacement(&self) -> Pt {
        self.displacement
    }
    /// True while a point is being picked that object snap may give: the base point and the second
    /// point of MOVE and COPY, a grip being dragged, a new element hanging from the cursor, and a
    /// press-drag move of the selection (its grab point when the pointer is pressed on an element,
    /// which is when the pointer is over one, and its target while it is dragged), as MOVE does.
    pub fn wants_object_snap(&self) -> bool {
        match self.tool {
            CanvasTool::BasePoint | CanvasTool::SecondPoint | CanvasTool::GripDrag | CanvasTool::Place | CanvasTool::DragMove => true,
            CanvasTool::Idle => self.hover.is_some(),
            _ => false,
        }
    }

    /// Gives the point object snap found under the pointer (model coordinates), or none. The caller
    /// sets it before each pointer event it forwards, from [`CanvasController::wants_object_snap`]
    /// onward. While it is set it is the point picked: it replaces the pointer position and the
    /// grid snap and ortho do not move it (an object snap point is exact, as in a CAD program).
    /// A new element hanging from the cursor is centred on it.
    pub fn set_object_snap(&mut self, p: Option<Pt>) {
        self.object_snap = p;
    }

    pub fn ghost_ids(&self) -> &[ElementId] {
        &self.ghost_ids
    }
    pub fn hover(&self) -> Option<ElementId> {
        self.hover
    }
    pub fn hover_grip(&self) -> Option<(ElementId, usize)> {
        self.hover_grip
    }
    pub fn prompt(&self) -> &str {
        &self.prompt
    }
    pub fn is_busy(&self) -> bool {
        self.tool != CanvasTool::Idle
    }
    pub fn take_events(&mut self) -> Vec<CanvasEvent> {
        std::mem::take(&mut self.events)
    }

    fn echo(&mut self, s: impl Into<String>) {
        self.events.push(CanvasEvent::Echo(s.into()));
    }

    // ─── view ────────────────────────────────────────────────────────────────

    pub fn wheel(&mut self, doc: &mut Document, view: &mut ViewTransform, screen: Pt, zoom_in: bool) {
        view.zoom_about(screen, if zoom_in { 1.25 } else { 0.8 });
        self.pointer_move(doc, view, screen);
    }

    pub fn zoom_extents(&self, doc: &Document, view: &mut ViewTransform) {
        let ext = doc.extents().unwrap_or(BBox::new(0.0, 0.0, 5000.0, 3000.0));
        let (l, t, w, h) = self.viewport;
        view.fit(&ext, l, t, w, h, 60.0);
    }

    /// Zooms to the bounds of the selected elements, with a 60 px margin. Returns false (and
    /// leaves the view alone) when nothing is selected. The previous view is remembered for
    /// [`zoom_previous`](Self::zoom_previous).
    pub fn zoom_to_selection(&mut self, doc: &Document, view: &mut ViewTransform) -> bool {
        let bounds = doc.selection().iter().filter_map(|id| doc.find(*id)).map(|e| e.bounds()).reduce(|a, b| a.union(&b));
        match bounds {
            Some(b) => {
                self.zoom_to_bounds(&b, view, 60.0);
                true
            }
            None => false,
        }
    }

    /// Zooms to `bounds` with a pixel margin, remembering the previous view.
    pub fn zoom_to_bounds(&mut self, bounds: &BBox, view: &mut ViewTransform, margin_px: f64) {
        self.remember_view(view);
        let (l, t, w, h) = self.viewport;
        view.zoom_to_bounds(bounds, l, t, w, h, margin_px);
    }

    /// Records `view` as the one ZOOM Previous returns to. Zoom window and zoom to selection do
    /// this themselves; a front end calls it before a zoom it makes through `zoom_extents`, which
    /// takes `&self`. The history keeps the last 50 views and drops a repeat of the latest one.
    pub fn remember_view(&mut self, view: &ViewTransform) {
        if self.view_history.last() != Some(view) {
            self.view_history.push(view.clone());
            if self.view_history.len() > 50 {
                self.view_history.remove(0);
            }
        }
    }

    /// ZOOM Previous: restores the last remembered view. False when there is none.
    pub fn zoom_previous(&mut self, view: &mut ViewTransform) -> bool {
        match self.view_history.pop() {
            Some(v) => {
                *view = v;
                true
            }
            None => false,
        }
    }

    pub fn zoom_by(&self, view: &mut ViewTransform, factor: f64) {
        let (l, t, w, h) = self.viewport;
        view.zoom_about(Pt::new(l + w / 2.0, t + h / 2.0), factor);
    }

    // ─── commands ────────────────────────────────────────────────────────────

    pub fn start_move(&mut self, doc: &mut Document) {
        self.start_modify(doc, CanvasCommand::Move);
    }
    pub fn start_copy(&mut self, doc: &mut Document) {
        self.start_modify(doc, CanvasCommand::Copy);
    }
    pub fn start_erase(&mut self, doc: &mut Document) {
        self.start_modify(doc, CanvasCommand::Erase);
    }

    fn start_modify(&mut self, doc: &mut Document, command: CanvasCommand) {
        self.reset();
        self.command = command;
        self.echo(format!("Command: {}", format!("{command:?}").to_uppercase()));
        if doc.selection().is_empty() {
            self.set_tool(CanvasTool::SelectObjects);
        } else {
            self.after_selection(doc);
        }
    }

    pub fn start_zoom_window(&mut self) {
        self.reset();
        self.echo("Command: ZOOM Window");
        self.set_tool(CanvasTool::ZoomFirst);
    }

    /// Attaches an existing element to the cursor so the next click sets its position.
    pub fn start_place(&mut self, doc: &Document, view: &ViewTransform, id: ElementId) {
        self.reset();
        if doc.find(id).is_none() {
            return;
        }
        self.place_id = Some(id);
        self.ghost_ids = vec![id];
        self.set_tool(CanvasTool::Place);
        self.update_place_ghost(doc, view);
    }

    fn after_selection(&mut self, doc: &mut Document) {
        if self.command == CanvasCommand::Erase {
            let ids = doc.selection().to_vec();
            self.echo(format!("{} erased.", ids.len()));
            self.events.push(CanvasEvent::Erase(ids));
            self.events.push(CanvasEvent::Committed);
            self.reset();
            return;
        }
        self.ghost_ids = doc.selection().to_vec();
        self.set_tool(CanvasTool::BasePoint);
    }

    // ─── pointer ─────────────────────────────────────────────────────────────

    pub fn pointer_move(&mut self, doc: &mut Document, view: &ViewTransform, screen: Pt) {
        self.cursor_screen = screen;
        let raw = view.to_world(screen);
        self.cursor_world = raw;

        if self.tool == CanvasTool::Idle && self.press_on_item && screen.distance_to(self.press_screen) > Self::DRAG_THRESHOLD_PX {
            self.press_on_item = false;
            self.ghost_ids = doc.selection().to_vec();
            self.base_point = Some(self.press_snap.take().unwrap_or(self.press_world));
            self.set_tool(CanvasTool::DragMove);
        }

        match self.tool {
            CanvasTool::Idle | CanvasTool::SelectObjects => self.update_hover(doc, view, raw),
            CanvasTool::DragMove => {
                let base = self.base_point.unwrap_or(raw);
                let d = drag_displacement(base, raw, self.object_snap, self.ortho_on, self.snap_on, self.snap_spacing);
                self.displacement = d;
                self.cursor_world = base + d;
            }
            CanvasTool::GripDrag | CanvasTool::SecondPoint => {
                self.cursor_world = self.object_snap.unwrap_or_else(|| self.constrain(raw));
                self.displacement = self.cursor_world - self.base_point.unwrap_or(self.cursor_world);
            }
            CanvasTool::BasePoint => self.cursor_world = self.object_snap.unwrap_or_else(|| self.snapped(raw)),
            CanvasTool::Place => self.update_place_ghost(doc, view),
            _ => {}
        }
    }

    pub fn left_down(&mut self, doc: &mut Document, view: &mut ViewTransform, screen: Pt, shift: bool, double_click: bool) {
        self.pointer_move(doc, view, screen);
        let raw = view.to_world(screen);
        let pick = self.pick_box_px / view.scale;

        match self.tool {
            CanvasTool::Idle => {
                if double_click {
                    if let Some(id) = doc.pick(raw, pick) {
                        self.events.push(CanvasEvent::Details(id));
                    }
                    return;
                }
                // Shift + click always deselects, even on a grip (the centre grip covers the middle).
                if !shift {
                    if let Some((id, g)) = self.hover_grip {
                        if let Some(e) = doc.find(id) {
                            self.base_point = Some(e.grips()[g]);
                            self.ghost_ids = doc.selection().to_vec();
                            self.displacement = Pt::default();
                            self.set_tool(CanvasTool::GripDrag);
                            return;
                        }
                    }
                }
                if let Some(id) = doc.pick(raw, pick) {
                    if shift {
                        doc.select(&[id], SelectMode::Remove);
                        return;
                    }
                    doc.select(&[id], SelectMode::Add);
                    self.press_on_item = true;
                    self.press_screen = screen;
                    self.press_world = raw;
                    self.press_snap = self.object_snap;
                    return;
                }
                self.open_box(screen, shift, CanvasTool::Idle);
            }
            CanvasTool::SelectObjects => {
                if let Some(id) = doc.pick(raw, pick) {
                    doc.select(&[id], if shift { SelectMode::Remove } else { SelectMode::Add });
                } else {
                    self.open_box(screen, shift, CanvasTool::SelectObjects);
                }
            }
            CanvasTool::BoxSecond => {
                if !self.box_pressed {
                    self.close_box(doc, view);
                }
            }
            CanvasTool::BasePoint | CanvasTool::SecondPoint | CanvasTool::Place => {
                let p = self.cursor_world;
                self.point_given(doc, p);
            }
            CanvasTool::ZoomFirst => {
                self.box_start = Some(screen);
                self.box_pressed = true;
                self.set_tool(CanvasTool::ZoomSecond);
            }
            CanvasTool::ZoomSecond => {
                if !self.box_pressed {
                    self.close_zoom(view);
                }
            }
            _ => {}
        }
    }

    pub fn left_up(&mut self, doc: &mut Document, view: &mut ViewTransform, screen: Pt) {
        self.pointer_move(doc, view, screen);
        self.press_on_item = false;
        self.press_snap = None;

        match self.tool {
            CanvasTool::DragMove | CanvasTool::GripDrag => {
                let d = self.displacement;
                let ids = self.ghost_ids.clone();
                self.reset();
                if d.x != 0.0 || d.y != 0.0 {
                    doc.translate(&ids, d.x, d.y);
                    self.events.push(CanvasEvent::Committed);
                }
            }
            // Press-drag-release closes the box; a plain click leaves it open for a second click.
            CanvasTool::BoxSecond if self.box_pressed => {
                if self.box_start.is_some_and(|s| screen.distance_to(s) > Self::DRAG_THRESHOLD_PX) {
                    self.close_box(doc, view);
                } else {
                    self.box_pressed = false;
                }
            }
            CanvasTool::ZoomSecond if self.box_pressed => {
                if self.box_start.is_some_and(|s| screen.distance_to(s) > Self::DRAG_THRESHOLD_PX) {
                    self.close_zoom(view);
                } else {
                    self.box_pressed = false;
                }
            }
            _ => {}
        }
    }

    pub fn pointer_leave(&mut self) {
        self.hover = None;
        self.hover_grip = None;
    }

    // ─── keyboard / command line ─────────────────────────────────────────────

    /// Enter, Space or right-click while a command is active. Returns false when idle.
    pub fn enter(&mut self, doc: &mut Document) -> bool {
        match self.tool {
            CanvasTool::Idle => false,
            CanvasTool::SelectObjects => {
                if doc.selection().is_empty() {
                    self.echo("No objects selected.");
                    self.reset();
                } else {
                    self.after_selection(doc);
                }
                true
            }
            _ => {
                self.reset();
                true
            }
        }
    }

    /// Escape: cancels the active command, else clears the selection. False when nothing happened.
    pub fn escape(&mut self, doc: &mut Document) -> bool {
        if self.is_busy() {
            self.echo("*Cancel*");
            self.reset();
            return true;
        }
        self.press_on_item = false;
        if doc.selection().is_empty() {
            return false;
        }
        doc.clear_selection();
        true
    }

    /// Typed input while a command is active: a point or a keyword. Returns false when idle, so
    /// the caller treats the text as a command name.
    pub fn submit_text(&mut self, doc: &mut Document, view: &mut ViewTransform, text: &str) -> bool {
        if !self.is_busy() {
            return false;
        }
        let t = text.trim();
        if t.is_empty() {
            return self.enter(doc);
        }
        if self.tool == CanvasTool::SelectObjects {
            if t.eq_ignore_ascii_case("all") {
                doc.select_all();
            } else {
                self.echo("Pick objects, type ALL, or press Enter to finish.");
            }
            return true;
        }
        if let Some(p) = parse_point(t, self.base_point) {
            match self.tool {
                CanvasTool::ZoomFirst => {
                    self.box_start = Some(view.to_screen(p));
                    self.box_pressed = false;
                    self.set_tool(CanvasTool::ZoomSecond);
                    return true;
                }
                CanvasTool::ZoomSecond => {
                    self.cursor_screen = view.to_screen(p);
                    self.close_zoom(view);
                    return true;
                }
                CanvasTool::BasePoint | CanvasTool::SecondPoint | CanvasTool::Place => {
                    self.cursor_world = p;
                    if self.tool == CanvasTool::Place {
                        if let Some(e) = self.place_id.and_then(|id| doc.find(id)) {
                            self.displacement = p - e.origin;
                        }
                    }
                    self.point_given(doc, p);
                    return true;
                }
                _ => {}
            }
        }
        self.echo("Point required: x,y  or  @dx,dy  or  @distance<angle.");
        true
    }

    // ─── internals ───────────────────────────────────────────────────────────

    fn point_given(&mut self, doc: &mut Document, world: Pt) {
        match self.tool {
            CanvasTool::BasePoint => {
                self.base_point = Some(world);
                self.displacement = Pt::default();
                self.set_tool(CanvasTool::SecondPoint);
            }
            CanvasTool::SecondPoint => {
                let d = world - self.base_point.unwrap_or(world);
                let ids = self.ghost_ids.clone();
                if self.command == CanvasCommand::Copy {
                    // COPY repeats until Enter.
                    self.events.push(CanvasEvent::Copy(ids, d.x, d.y));
                    self.events.push(CanvasEvent::Committed);
                } else {
                    self.reset();
                    doc.translate(&ids, d.x, d.y);
                    self.events.push(CanvasEvent::Committed);
                }
            }
            CanvasTool::Place => {
                let d = self.displacement;
                let id = self.place_id;
                self.reset();
                if let Some(id) = id {
                    doc.translate(&[id], d.x, d.y);
                }
                self.events.push(CanvasEvent::Committed);
            }
            _ => {}
        }
    }

    fn update_place_ghost(&mut self, doc: &Document, view: &ViewTransform) {
        let Some(e) = self.place_id.and_then(|id| doc.find(id)) else {
            self.reset();
            return;
        };
        // The element hangs from the cursor by its centre; snap lands its lower-left corner on the grid.
        let raw = view.to_world(self.cursor_screen);
        // An object snap point is where the centre goes, exactly; otherwise the grid takes the corner.
        let origin = match self.object_snap {
            Some(p) => Pt::new(p.x - e.width / 2.0, p.y - e.height / 2.0),
            None => self.snapped(Pt::new(raw.x - e.width / 2.0, raw.y - e.height / 2.0)),
        };
        self.displacement = origin - e.origin;
    }

    fn update_hover(&mut self, doc: &Document, view: &ViewTransform, raw: Pt) {
        self.hover_grip = None;
        if self.tool == CanvasTool::Idle {
            let r = (self.grip_px + 2.0) / view.scale;
            for id in doc.selection() {
                let Some(e) = doc.find(*id) else { continue };
                for (i, g) in e.grips().iter().enumerate() {
                    if (g.x - raw.x).abs() <= r && (g.y - raw.y).abs() <= r {
                        self.hover_grip = Some((*id, i));
                        self.hover = Some(*id);
                        return;
                    }
                }
            }
        }
        self.hover = doc.pick(raw, self.pick_box_px / view.scale);
    }

    fn open_box(&mut self, screen: Pt, removes: bool, return_to: CanvasTool) {
        self.box_start = Some(screen);
        self.box_pressed = true;
        self.box_removes = removes;
        self.box_return = return_to;
        self.tool = CanvasTool::BoxSecond;
        self.prompt = "Specify opposite corner:".into();
    }

    fn close_box(&mut self, doc: &mut Document, view: &ViewTransform) {
        let Some(start) = self.box_start else { return };
        let crossing = self.box_is_crossing();
        let area = view.box_to_world(start, self.cursor_screen);
        let ids = doc.in_box(&area, crossing);
        self.box_start = None;
        self.box_pressed = false;
        self.tool = self.box_return;
        self.prompt = self.prompt_for(self.tool);
        doc.select(&ids, if self.box_removes { SelectMode::Remove } else { SelectMode::Add });
        if !ids.is_empty() {
            self.echo(format!("{} found ({}).", ids.len(), if crossing { "crossing" } else { "window" }));
        }
    }

    fn close_zoom(&mut self, view: &mut ViewTransform) {
        if let Some(start) = self.box_start {
            if start.distance_to(self.cursor_screen) > 1.0 {
                let (l, t, w, h) = self.viewport;
                let area = view.box_to_world(start, self.cursor_screen);
                self.remember_view(view);
                view.fit(&area, l, t, w, h, 0.0);
            }
        }
        self.reset();
    }

    fn snapped(&self, p: Pt) -> Pt {
        if self.snap_on {
            snap(p, self.snap_spacing)
        } else {
            p
        }
    }

    fn constrain(&self, raw: Pt) -> Pt {
        let p = self.snapped(raw);
        match (self.ortho_on, self.base_point) {
            (true, Some(b)) => ortho(b, p),
            _ => p,
        }
    }

    fn reset(&mut self) {
        self.object_snap = None;
        self.press_snap = None;
        self.command = CanvasCommand::None;
        self.box_start = None;
        self.base_point = None;
        self.displacement = Pt::default();
        self.ghost_ids.clear();
        self.box_pressed = false;
        self.press_on_item = false;
        self.place_id = None;
        self.set_tool(CanvasTool::Idle);
    }

    fn set_tool(&mut self, tool: CanvasTool) {
        self.tool = tool;
        self.prompt = self.prompt_for(tool);
    }

    fn prompt_for(&self, tool: CanvasTool) -> String {
        match tool {
            CanvasTool::SelectObjects => "Select objects (Enter to finish):",
            CanvasTool::BasePoint => "Specify base point:",
            CanvasTool::SecondPoint if self.command == CanvasCommand::Copy => "Specify second point or <exit>:",
            CanvasTool::SecondPoint => "Specify second point:",
            CanvasTool::ZoomFirst => "Specify first corner:",
            CanvasTool::ZoomSecond | CanvasTool::BoxSecond => "Specify opposite corner:",
            CanvasTool::Place => "Specify insertion point (Esc keeps the automatic position):",
            _ => "",
        }
        .to_string()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    // View: 1 px = 10 mm, model origin at screen (0, 600). World (x, y) → screen (x/10, 600 - y/10).
    fn rig() -> (Document, ViewTransform, CanvasController) {
        let mut d = Document::default();
        d.add("sign", "A-001", Pt::new(0.0, 0.0), 1000.0, 500.0, "0", json!({})); // id 1
        d.add("sign", "A-002", Pt::new(2000.0, 0.0), 1000.0, 500.0, "0", json!({})); // id 2
        let mut v = ViewTransform::default();
        v.fit(&BBox::new(0.0, 0.0, 8000.0, 6000.0), 0.0, 0.0, 800.0, 600.0, 0.0);
        assert!((v.scale - 0.1).abs() < 1e-12);
        (d, v, CanvasController::default())
    }

    fn s(wx: f64, wy: f64) -> Pt {
        Pt::new(wx / 10.0, 600.0 - wy / 10.0)
    }

    fn click(c: &mut CanvasController, d: &mut Document, v: &mut ViewTransform, p: Pt, shift: bool) {
        c.left_down(d, v, p, shift, false);
        c.left_up(d, v, p);
    }

    fn ids(v: &[u64]) -> Vec<ElementId> {
        v.iter().map(|i| ElementId(*i)).collect()
    }

    fn origin(d: &Document, id: u64) -> Pt {
        d.find(ElementId(id)).unwrap().origin
    }

    fn close(a: f64, b: f64) {
        assert!((a - b).abs() < 1e-6, "{a} != {b}");
    }

    #[test]
    fn clicks_add_and_shift_click_removes() {
        let (mut d, mut v, mut c) = rig();
        click(&mut c, &mut d, &mut v, s(500.0, 250.0), false);
        click(&mut c, &mut d, &mut v, s(2500.0, 250.0), false);
        assert_eq!(d.selection(), ids(&[1, 2]));
        click(&mut c, &mut d, &mut v, s(500.0, 250.0), true); // on the centre grip: still deselects
        assert_eq!(d.selection(), ids(&[2]));
        assert!(c.escape(&mut d));
        assert!(d.selection().is_empty());
        assert!(!c.escape(&mut d));
    }

    #[test]
    fn left_to_right_is_a_window_right_to_left_a_crossing() {
        let (mut d, mut v, mut c) = rig();
        c.left_down(&mut d, &mut v, s(-100.0, 700.0), false, false);
        c.pointer_move(&mut d, &v, s(2500.0, -100.0));
        assert_eq!(c.tool(), CanvasTool::BoxSecond);
        assert!(!c.box_is_crossing());
        c.left_up(&mut d, &mut v, s(2500.0, -100.0));
        assert_eq!(d.selection(), ids(&[1]));
        assert_eq!(c.tool(), CanvasTool::Idle);

        c.escape(&mut d);
        c.left_down(&mut d, &mut v, s(2500.0, -100.0), false, false);
        c.pointer_move(&mut d, &v, s(-100.0, 700.0));
        assert!(c.box_is_crossing());
        c.left_up(&mut d, &mut v, s(-100.0, 700.0));
        assert_eq!(d.selection(), ids(&[1, 2]));
    }

    #[test]
    fn two_click_box() {
        let (mut d, mut v, mut c) = rig();
        click(&mut c, &mut d, &mut v, s(-100.0, 700.0), false);
        assert_eq!(c.tool(), CanvasTool::BoxSecond);
        c.pointer_move(&mut d, &v, s(1100.0, -100.0));
        click(&mut c, &mut d, &mut v, s(1100.0, -100.0), false);
        assert_eq!(c.tool(), CanvasTool::Idle);
        assert_eq!(d.selection(), ids(&[1]));
    }

    #[test]
    fn dragging_moves_the_selection_and_commits_once() {
        let (mut d, mut v, mut c) = rig();
        c.left_down(&mut d, &mut v, s(500.0, 250.0), false, false);
        c.pointer_move(&mut d, &v, s(900.0, 250.0));
        assert_eq!(c.tool(), CanvasTool::DragMove);
        c.left_up(&mut d, &mut v, s(1500.0, 1250.0));
        close(origin(&d, 1).x, 1000.0);
        close(origin(&d, 1).y, 1000.0);
        assert_eq!(origin(&d, 2), Pt::new(2000.0, 0.0));
        let commits = c.take_events().iter().filter(|e| **e == CanvasEvent::Committed).count();
        assert_eq!(commits, 1);
    }

    #[test]
    fn a_plain_click_does_not_move_or_commit() {
        let (mut d, mut v, mut c) = rig();
        click(&mut c, &mut d, &mut v, s(500.0, 250.0), false);
        assert_eq!(origin(&d, 1), Pt::new(0.0, 0.0));
        assert!(!c.take_events().contains(&CanvasEvent::Committed));
    }

    #[test]
    fn grip_drag_with_snap_lands_the_grip_on_the_grid() {
        let (mut d, mut v, mut c) = rig();
        c.snap_on = true;
        c.snap_spacing = 100.0;
        click(&mut c, &mut d, &mut v, s(500.0, 250.0), false);
        c.pointer_move(&mut d, &v, s(1000.0, 500.0)); // top-right grip
        assert_eq!(c.hover_grip(), Some((ElementId(1), 2)));
        c.left_down(&mut d, &mut v, s(1000.0, 500.0), false, false);
        assert_eq!(c.tool(), CanvasTool::GripDrag);
        c.left_up(&mut d, &mut v, s(1730.0, 880.0)); // snaps to (1700, 900)
        close(origin(&d, 1).x, 700.0);
        close(origin(&d, 1).y, 400.0);
    }

    #[test]
    fn move_with_typed_points() {
        let (mut d, mut v, mut c) = rig();
        click(&mut c, &mut d, &mut v, s(2500.0, 250.0), false);
        c.start_move(&mut d);
        assert_eq!(c.tool(), CanvasTool::BasePoint);
        assert!(c.submit_text(&mut d, &mut v, "0,0"));
        assert!(c.submit_text(&mut d, &mut v, "@1500<90"));
        assert_eq!(c.tool(), CanvasTool::Idle);
        close(origin(&d, 2).x, 2000.0);
        close(origin(&d, 2).y, 1500.0);
        assert!(!c.submit_text(&mut d, &mut v, "move"));
    }

    #[test]
    fn move_asks_for_objects_first_and_honours_ortho() {
        let (mut d, mut v, mut c) = rig();
        c.start_move(&mut d);
        assert_eq!(c.tool(), CanvasTool::SelectObjects);
        click(&mut c, &mut d, &mut v, s(500.0, 250.0), false);
        assert!(c.enter(&mut d));
        assert_eq!(c.tool(), CanvasTool::BasePoint);
        click(&mut c, &mut d, &mut v, s(0.0, 0.0), false);
        c.ortho_on = true;
        click(&mut c, &mut d, &mut v, s(3000.0, 400.0), false);
        close(origin(&d, 1).x, 3000.0);
        close(origin(&d, 1).y, 0.0);
    }

    #[test]
    fn copy_repeats_until_enter() {
        let (mut d, mut v, mut c) = rig();
        click(&mut c, &mut d, &mut v, s(500.0, 250.0), false);
        c.start_copy(&mut d);
        c.submit_text(&mut d, &mut v, "0,0");
        c.submit_text(&mut d, &mut v, "0,1000");
        c.submit_text(&mut d, &mut v, "0,2000");
        assert_eq!(c.tool(), CanvasTool::SecondPoint);
        assert!(c.enter(&mut d));
        assert_eq!(c.tool(), CanvasTool::Idle);
        let copies: Vec<_> = c.take_events().into_iter().filter(|e| matches!(e, CanvasEvent::Copy(..))).collect();
        assert_eq!(copies, vec![CanvasEvent::Copy(ids(&[1]), 0.0, 1000.0), CanvasEvent::Copy(ids(&[1]), 0.0, 2000.0)]);
        assert_eq!(origin(&d, 1), Pt::new(0.0, 0.0));
    }

    #[test]
    fn erase_uses_the_selection_or_asks_for_one() {
        let (mut d, mut v, mut c) = rig();
        c.start_erase(&mut d);
        assert_eq!(c.tool(), CanvasTool::SelectObjects);
        c.submit_text(&mut d, &mut v, "all");
        c.enter(&mut d);
        assert!(c.take_events().contains(&CanvasEvent::Erase(ids(&[1, 2]))));
        assert_eq!(c.tool(), CanvasTool::Idle);
    }

    #[test]
    fn escape_cancels_a_drag_without_moving() {
        let (mut d, mut v, mut c) = rig();
        c.left_down(&mut d, &mut v, s(500.0, 250.0), false, false);
        c.pointer_move(&mut d, &v, s(1500.0, 250.0));
        assert!(c.escape(&mut d));
        c.left_up(&mut d, &mut v, s(1500.0, 250.0));
        assert_eq!(origin(&d, 1), Pt::new(0.0, 0.0));
    }

    #[test]
    fn place_centres_the_element_on_the_cursor() {
        let (mut d, mut v, mut c) = rig();
        let id = d.add("sign", "n", Pt::new(5000.0, 5000.0), 600.0, 400.0, "0", json!({}));
        c.start_place(&d, &v, id);
        assert_eq!(c.tool(), CanvasTool::Place);
        click(&mut c, &mut d, &mut v, s(4000.0, 3000.0), false);
        assert_eq!(c.tool(), CanvasTool::Idle);
        close(d.find(id).unwrap().origin.x, 3700.0);
        close(d.find(id).unwrap().origin.y, 2800.0);
    }

    #[test]
    fn object_snap_gives_the_points_of_move_copy_and_place() {
        // The snap point replaces the pointer and is exact: grid snap and ortho do not move it.
        let (mut d, mut v, mut c) = rig();
        c.snap_on = true;
        c.snap_spacing = 100.0;
        c.ortho_on = true;
        click(&mut c, &mut d, &mut v, s(500.0, 250.0), false);
        c.start_move(&mut d);
        assert!(c.wants_object_snap());
        // Base point: the corner of A-001 (1000, 500) given by object snap, the pointer a few px away.
        c.set_object_snap(Some(Pt::new(1000.0, 500.0)));
        click(&mut c, &mut d, &mut v, s(1030.0, 540.0), false);
        assert_eq!(c.base_point(), Some(Pt::new(1000.0, 500.0)));
        assert_eq!(c.tool(), CanvasTool::SecondPoint);
        // Second point: (2437, 913), not a multiple of the grid and off the ortho line.
        c.set_object_snap(Some(Pt::new(2437.0, 913.0)));
        c.pointer_move(&mut d, &v, s(2400.0, 900.0));
        close(c.cursor_world().x, 2437.0);
        close(c.cursor_world().y, 913.0);
        click(&mut c, &mut d, &mut v, s(2400.0, 900.0), false);
        close(origin(&d, 1).x, 1437.0);
        close(origin(&d, 1).y, 413.0);
        // Back to idle with the pointer over empty space: nothing asks for object snap.
        c.pointer_move(&mut d, &v, s(7000.0, 5000.0));
        assert!(!c.wants_object_snap());
        // With no snap point the grid and ortho apply again.
        c.set_object_snap(None);
        d.select(&ids(&[1]), SelectMode::Replace);
        c.start_move(&mut d);
        click(&mut c, &mut d, &mut v, s(0.0, 0.0), false);
        click(&mut c, &mut d, &mut v, s(3000.0, 400.0), false);
        close(origin(&d, 1).x, 1437.0 + 3000.0);
        close(origin(&d, 1).y, 413.0);

        // COPY takes its points from object snap too, and repeats.
        let (mut d, mut v, mut c) = rig();
        click(&mut c, &mut d, &mut v, s(500.0, 250.0), false);
        c.start_copy(&mut d);
        c.set_object_snap(Some(Pt::new(0.0, 0.0)));
        click(&mut c, &mut d, &mut v, s(10.0, 10.0), false);
        c.set_object_snap(Some(Pt::new(2000.0, 500.0)));
        click(&mut c, &mut d, &mut v, s(1990.0, 510.0), false);
        let copies: Vec<_> = c.take_events().into_iter().filter(|e| matches!(e, CanvasEvent::Copy(..))).collect();
        assert_eq!(copies, vec![CanvasEvent::Copy(ids(&[1]), 2000.0, 500.0)]);

        // A new element hangs by its centre from the snap point, with no grid.
        let (mut d, mut v, mut c) = rig();
        c.snap_on = true;
        c.snap_spacing = 100.0;
        let id = d.add("sign", "n", Pt::new(5000.0, 5000.0), 600.0, 400.0, "0", json!({}));
        c.start_place(&d, &v, id);
        assert!(c.wants_object_snap());
        c.set_object_snap(Some(Pt::new(3037.0, 2011.0)));
        click(&mut c, &mut d, &mut v, s(3000.0, 2000.0), false);
        close(d.find(id).unwrap().origin.x, 3037.0 - 300.0);
        close(d.find(id).unwrap().origin.y, 2011.0 - 200.0);

        // The idle tool asks for object snap only with the pointer over an element (a press there
        // may start a drag).
        let (mut d, mut v, mut c) = rig();
        assert!(!c.wants_object_snap());
        c.pointer_move(&mut d, &v, s(4000.0, 4000.0));
        assert!(!c.wants_object_snap());
        c.pointer_move(&mut d, &v, s(500.0, 250.0));
        assert!(c.wants_object_snap());
    }

    #[test]
    fn a_press_drag_move_takes_its_grab_point_and_target_from_object_snap() {
        // Press on A-001 near its corner (1000, 0): object snap gives the corner as the grab point.
        let (mut d, mut v, mut c) = rig();
        c.snap_on = true;
        c.snap_spacing = 100.0;
        c.ortho_on = true;
        c.pointer_move(&mut d, &v, s(980.0, 20.0));
        assert!(c.wants_object_snap());
        c.set_object_snap(Some(Pt::new(1000.0, 0.0)));
        c.left_down(&mut d, &mut v, s(980.0, 20.0), false, false);
        // Dragging: the move is asked for object snap, and the target (4437, 913) is exact, not on
        // the grid and not on the ortho line.
        c.set_object_snap(None);
        c.pointer_move(&mut d, &v, s(1500.0, 20.0));
        assert_eq!(c.tool(), CanvasTool::DragMove);
        assert_eq!(c.base_point(), Some(Pt::new(1000.0, 0.0)));
        assert!(c.wants_object_snap());
        c.set_object_snap(Some(Pt::new(4437.0, 913.0)));
        c.pointer_move(&mut d, &v, s(4400.0, 900.0));
        close(c.displacement().x, 3437.0);
        close(c.displacement().y, 913.0);
        close(c.cursor_world().x, 4437.0);
        c.left_up(&mut d, &mut v, s(4400.0, 900.0));
        close(origin(&d, 1).x, 3437.0);
        close(origin(&d, 1).y, 913.0);
        assert_eq!(origin(&d, 2), Pt::new(2000.0, 0.0));
        assert_eq!(c.tool(), CanvasTool::Idle);

        // With no snap point the grab point is where the pointer was pressed and the grid and
        // ortho apply, as before.
        let (mut d, mut v, mut c) = rig();
        c.snap_on = true;
        c.snap_spacing = 100.0;
        c.left_down(&mut d, &mut v, s(500.0, 250.0), false, false);
        c.pointer_move(&mut d, &v, s(1000.0, 250.0));
        assert_eq!(c.base_point(), Some(Pt::new(500.0, 250.0)));
        c.left_up(&mut d, &mut v, s(1040.0, 330.0));
        close(origin(&d, 1).x, 500.0);
        close(origin(&d, 1).y, 100.0);
    }

    #[test]
    fn the_drag_displacement_function_snaps_the_displacement_or_takes_the_snap_point() {
        let (base, raw) = (Pt::new(100.0, 100.0), Pt::new(1340.0, 560.0));
        // No object snap, no grid, no ortho: the pointer's displacement.
        assert_eq!(drag_displacement(base, raw, None, false, false, 50.0), Pt::new(1240.0, 460.0));
        // The grid rounds the displacement (1240, 460) to 100: (1200, 500).
        assert_eq!(drag_displacement(base, raw, None, false, true, 100.0), Pt::new(1200.0, 500.0));
        // Ortho keeps the larger component.
        assert_eq!(drag_displacement(base, raw, None, true, false, 100.0), Pt::new(1240.0, 0.0));
        // An object snap point wins over both: the vector from the grab point to it.
        assert_eq!(drag_displacement(base, raw, Some(Pt::new(1337.0, 211.0)), true, true, 100.0), Pt::new(1237.0, 111.0));
    }

    #[test]
    fn zoom_window_and_wheel() {
        let (mut d, mut v, mut c) = rig();
        c.zoom_extents(&d, &mut v);
        let before = v.to_world(Pt::new(300.0, 200.0));
        c.wheel(&mut d, &mut v, Pt::new(300.0, 200.0), true);
        let after = v.to_world(Pt::new(300.0, 200.0));
        close(before.x, after.x);
        close(before.y, after.y);

        c.start_zoom_window();
        c.submit_text(&mut d, &mut v, "0,0");
        c.submit_text(&mut d, &mut v, "1000,500");
        assert_eq!(c.tool(), CanvasTool::Idle);
        close(v.scale, 0.8); // 800 px / 1000 mm
    }

    #[test]
    fn zoom_to_selection_and_previous() {
        let (mut d, mut v, mut c) = rig();
        c.zoom_extents(&d, &mut v);
        let before = v.clone();
        assert!(!c.zoom_to_selection(&d, &mut v), "nothing selected");
        assert_eq!(v, before);
        assert!(!c.zoom_previous(&mut v));
        let id = d.elements()[0].id;
        d.select(&[id], SelectMode::Replace);
        assert!(c.zoom_to_selection(&d, &mut v));
        let b = d.find(id).unwrap().bounds();
        let (l, t, w, h) = c.viewport;
        // The selection keeps the 60 px margin on every side.
        let tl = v.to_screen(Pt::new(b.min.x, b.max.y));
        let br = v.to_screen(Pt::new(b.max.x, b.min.y));
        let m = 60.0 - 1e-6;
        assert!(tl.x - l >= m && tl.y - t >= m && l + w - br.x >= m && t + h - br.y >= m);
        assert!(v.scale != before.scale);
        assert!(c.zoom_previous(&mut v));
        assert_eq!(v, before);
        assert!(!c.zoom_previous(&mut v));
    }

    #[test]
    fn the_wheel_zooms_without_the_old_limits() {
        let (mut d, mut v, mut c) = rig();
        for _ in 0..300 {
            c.wheel(&mut d, &mut v, Pt::new(300.0, 200.0), true);
        }
        assert!(v.scale > 50.0 && v.scale.is_finite(), "the old 50 px/mm clamp is gone: {}", v.scale);
        for _ in 0..600 {
            c.wheel(&mut d, &mut v, Pt::new(300.0, 200.0), false);
        }
        assert!(v.scale < 0.0005 && v.scale > 0.0, "the old 2 m per pixel clamp is gone: {}", v.scale);
    }

    #[test]
    fn double_click_asks_for_details() {
        let (mut d, mut v, mut c) = rig();
        c.left_down(&mut d, &mut v, s(2500.0, 250.0), false, true);
        assert!(c.take_events().contains(&CanvasEvent::Details(ElementId(2))));
    }
}
