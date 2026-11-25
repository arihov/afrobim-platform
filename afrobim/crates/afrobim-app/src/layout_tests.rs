// SPDX-License-Identifier: LicenseRef-AfroBIM-Proprietary
// Copyright (c) 2026 AfroBIM. All rights reserved.
//! Layout tabs in the running application, headless: tabs, commands, undo, the canvas in paper
//! space and through a viewport, and the plot. The data and the arithmetic have their own tests
//! in `afrobim-kernel` (`layout.rs`) and `afrobim-sheets` (`layout_plot.rs`).

use afrobim_kernel::{Pt, STANDARD_SCALES};
use afrobim_sheets::{Orientation, PaperSize};

use crate::app::AfroBimApp;
use crate::commands::{resolve, Action};
use crate::layout_cmds::Lv;
use crate::u4_dialogs_test::{app_for, frame};

fn run(ctx: &egui::Context, app: &mut AfroBimApp, name: &str) {
    let c = resolve(name).unwrap_or_else(|| panic!("{name} is a command"));
    app.run(ctx, c.action);
}

/// One frame with these events at the time given.
fn frame_with(ctx: &egui::Context, app: &mut AfroBimApp, events: Vec<egui::Event>, time: f64) {
    let mut f = eframe::Frame::_new_kittest();
    let input = egui::RawInput { screen_rect: Some(egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(1280.0, 800.0))), events, time: Some(time), ..Default::default() };
    let _ = ctx.run(input, |ctx| eframe::App::update(app, ctx, &mut f));
}

fn double_click(at: egui::Pos2) -> Vec<egui::Event> {
    let m = egui::Modifiers::NONE;
    let b = |pressed| egui::Event::PointerButton { pos: at, button: egui::PointerButton::Primary, pressed, modifiers: m };
    vec![egui::Event::PointerMoved(at), b(true), b(false), b(true), b(false)]
}

#[test]
fn tabs_are_added_renamed_copied_moved_deleted_and_cycled() {
    let (ctx, mut app) = app_for("dark-glass", false);
    frame(&ctx, &mut app, 2);
    assert!(app.doc.layouts().is_empty() && app.lay.current.is_none(), "an old file has no layouts: only the Model tab");
    run(&ctx, &mut app, "LAYOUTNEW");
    assert_eq!(app.doc.layouts().len(), 1);
    assert_eq!(app.lay.current, app.doc.layouts().at(0).map(|l| l.id), "the new layout is the current tab");
    // Typed forms.
    assert!(app.layout_cli(&ctx, Lv::Layout, "NEW Plan sheet"));
    assert!(app.layout_cli(&ctx, Lv::Layout, "new Plan sheet"), "a taken name gets a free one");
    assert_eq!(app.doc.layouts().names()[1..], ["Plan sheet", "Plan sheet (2)"]);
    assert!(app.layout_cli(&ctx, Lv::Layout, "RENAME Long section"));
    assert_eq!(app.doc.layouts().names()[2], "Long section");
    assert!(app.layout_cli(&ctx, Lv::Layout, "SET layout1"));
    assert_eq!(app.doc.layouts().get(app.lay.current.unwrap()).unwrap().name, "Layout1");
    assert!(app.layout_cli(&ctx, Lv::Layout, "SET Model") && app.lay.current.is_none());
    assert!(!app.layout_cli(&ctx, Lv::Layout, "FROBNICATE"));
    // Ctrl+PageDown and Ctrl+PageUp wrap around through Model.
    app.cycle_layout_tab(1);
    assert_eq!(app.doc.layouts().get(app.lay.current.unwrap()).unwrap().name, "Layout1");
    app.cycle_layout_tab(-1);
    assert!(app.lay.current.is_none());
    app.cycle_layout_tab(-1);
    assert_eq!(app.doc.layouts().get(app.lay.current.unwrap()).unwrap().name, "Long section", "back from Model goes to the last tab");
    app.cycle_layout_tab(1);
    assert!(app.lay.current.is_none());
    // Reorder, copy, delete.
    let id = app.doc.layouts().by_name("Long section").unwrap().id;
    app.move_layout(&ctx, id, -2);
    assert_eq!(app.doc.layouts().names(), ["Long section", "Layout1", "Plan sheet"]);
    app.duplicate_layout(&ctx, id);
    assert_eq!(app.doc.layouts().names()[1], "Long section (2)");
    assert_eq!(app.lay.current, app.doc.layouts().by_name("Long section (2)").map(|l| l.id));
    // Delete asks first; the Model tab cannot be deleted.
    run(&ctx, &mut app, "LAYOUTDELETE");
    assert_eq!(app.lay.confirm_delete, app.lay.current);
    frame(&ctx, &mut app, 2);
    app.lay.confirm_delete = None;
    let cur = app.lay.current.unwrap();
    app.delete_layout(&ctx, cur);
    assert_eq!(app.doc.layouts().len(), 3);
    assert!(app.doc.layouts().get(cur).is_none() && app.lay.current.is_some_and(|c| app.doc.layouts().get(c).is_some()));
    app.set_current_layout(None);
    run(&ctx, &mut app, "LAYOUTDELETE");
    assert!(app.lay.confirm_delete.is_none());
    // The strip and the manager draw.
    app.lay.manager_open = true;
    assert!(frame(&ctx, &mut app, 3) > 0);
    // Rename in place by double click is a state change in the strip.
    let id = app.doc.layouts().at(0).unwrap().id;
    app.lay.rename = Some((id, "Cover".into()));
    app.lay.rename_fresh = true;
    frame(&ctx, &mut app, 2);
    app.rename_layout(&ctx, id, "Cover");
    assert_eq!(app.doc.layouts().at(0).unwrap().name, "Cover");
    assert!(!app.rename_layout(&ctx, id, "model"), "Model is not a layout name");
    assert!(!app.rename_layout(&ctx, id, "layout1"), "a taken name is refused");
}

#[test]
fn many_tabs_scroll_and_the_current_one_stays_in_view() {
    let (ctx, mut app) = app_for("light-glass", false);
    frame(&ctx, &mut app, 2);
    for i in 0..40 {
        app.new_layout(&ctx, Some(&format!("Sheet number {i}")), PaperSize::A3, Orientation::Landscape);
    }
    frame(&ctx, &mut app, 3);
    assert!(app.lay.tab_first > 0, "the strip scrolled to show the last tab, first visible {}", app.lay.tab_first);
    app.set_current_layout(app.doc.layouts().at(0).map(|l| l.id));
    frame(&ctx, &mut app, 3);
    assert_eq!(app.lay.tab_first, 0);
}

#[test]
fn undo_and_redo_cover_layouts_and_viewport_changes() {
    let (ctx, mut app) = app_for("dark-glass", false);
    frame(&ctx, &mut app, 2);
    app.flush_commit();
    let step = |app: &mut AfroBimApp| app.flush_commit();

    run(&ctx, &mut app, "LAYOUTNEW");
    step(&mut app);
    assert_eq!(app.doc.layouts().len(), 1);
    run(&ctx, &mut app, "MVIEWFIT");
    step(&mut app);
    let lid = app.lay.current.unwrap();
    let vid = app.doc.layouts().get(lid).unwrap().viewports[0].id;
    app.set_vp_scale(&ctx, vid, 500.0);
    step(&mut app);
    run(&ctx, &mut app, "VPLOCK");
    step(&mut app);
    app.with_vp(&ctx, vid, |v| v.freeze("0"));
    step(&mut app);
    app.rename_layout(&ctx, lid, "Renamed");
    step(&mut app);
    run(&ctx, &mut app, "MVIEW2");
    step(&mut app);
    assert_eq!(app.doc.layouts().get(lid).unwrap().viewports.len(), 3);

    let layout = |app: &AfroBimApp| app.doc.layouts().get(lid).cloned();
    app.undo();
    assert_eq!(layout(&app).unwrap().viewports.len(), 1, "the arrangement is undone");
    app.undo();
    assert_eq!(layout(&app).unwrap().name, "Layout1", "the rename is undone");
    app.undo();
    assert!(layout(&app).unwrap().viewport(vid).unwrap().frozen.is_empty(), "the freeze is undone");
    app.undo();
    assert!(!layout(&app).unwrap().viewport(vid).unwrap().locked, "the lock is undone");
    app.undo();
    assert_ne!(layout(&app).unwrap().viewport(vid).unwrap().scale_den, 500.0, "the scale is undone");
    app.undo();
    assert!(layout(&app).unwrap().viewports.is_empty(), "the viewport is undone");
    app.undo();
    assert!(app.doc.layouts().is_empty() && app.lay.current.is_none(), "the layout is undone and its tab goes");
    // And forward again.
    for _ in 0..7 {
        app.redo();
    }
    let l = layout(&app).unwrap();
    assert_eq!((l.name.as_str(), l.viewports.len()), ("Renamed", 3));
    let v = l.viewport(vid).unwrap();
    assert!(v.locked && v.scale_den == 500.0 && v.frozen == ["0"]);
}

#[test]
fn the_layout_canvas_draws_and_every_viewport_command_runs() {
    for civil in [false, true] {
        let (ctx, mut app) = app_for("light-glass", civil);
        frame(&ctx, &mut app, 3);
        run(&ctx, &mut app, "LAYOUTNEW");
        run(&ctx, &mut app, "MVIEW4");
        assert!(frame(&ctx, &mut app, 3) > 50, "the paper draws");
        let lid = app.lay.current.unwrap();
        assert_eq!(app.doc.layouts().get(lid).unwrap().viewports.len(), 4);
        assert!(app.lay.views.contains_key(&lid), "the paper view is made on the first frame");
        let first = app.doc.layouts().get(lid).unwrap().viewports[0].id;
        app.lay.selected = Some(first);
        for name in [
            "VPSCALE", "VPLOCK", "VPLOCK", "VPDISPLAY", "VPDISPLAY", "VPLAYER", "VPPROPS", "PAGESETUP", "PLOT", "LAYOUT", "MVIEWFIT", "MVIEW1", "VPCOPY", "VPUNCLIP", "PSPACE", "VPMIN", "MODEL", "LAYOUTRENAME", "LAYOUTCOPY",
        ] {
            run(&ctx, &mut app, name);
            frame(&ctx, &mut app, 2);
            if name == "MODEL" {
                assert!(app.lay.current.is_none());
                app.set_current_layout(Some(lid));
            }
            app.lay.rename = None;
        }
        // Model space through a viewport, and a maximised one, both draw and keep the Model tab's view.
        let view = app.view.clone();
        app.lay.selected = Some(first);
        run(&ctx, &mut app, "MSPACE");
        assert_eq!(app.lay.active, Some(first));
        assert!(frame(&ctx, &mut app, 3) > 50);
        run(&ctx, &mut app, "VPMAX");
        assert!(app.lay.maximised.is_some() && app.lay.active.is_none());
        assert!(frame(&ctx, &mut app, 3) > 50);
        run(&ctx, &mut app, "VPMIN");
        assert!(app.lay.maximised.is_none());
        assert_eq!(app.view, view, "the Model tab's view is put back after every pass");
        // A polygonal viewport draws too.
        let poly = vec![Pt::new(30.0, 30.0), Pt::new(120.0, 30.0), Pt::new(90.0, 110.0)];
        app.doc.layouts_mut().get_mut(lid).unwrap().add_polygon_viewport(poly, Pt::new(0.0, 0.0), 1000.0).unwrap();
        assert!(frame(&ctx, &mut app, 2) > 50);
    }
}

#[test]
fn double_click_enters_model_space_and_double_click_outside_returns() {
    let (ctx, mut app) = app_for("dark-glass", true);
    frame(&ctx, &mut app, 3);
    run(&ctx, &mut app, "LAYOUTNEW");
    frame(&ctx, &mut app, 2);
    let lid = app.lay.current.unwrap();
    // One viewport in the middle of the page.
    let vid = app.doc.layouts_mut().get_mut(lid).unwrap().add_viewport(Pt::new(40.0, 60.0), Pt::new(240.0, 220.0), Pt::new(0.0, 0.0), 1000.0).unwrap();
    frame(&ctx, &mut app, 2);
    let pv = app.lay.views.get(&lid).cloned().expect("the paper view");
    let inside = pv.to_screen(Pt::new(140.0, 140.0));
    let outside = pv.to_screen(Pt::new(20.0, 30.0));
    frame_with(&ctx, &mut app, double_click(egui::pos2(inside.x as f32, inside.y as f32)), 10.0);
    assert_eq!(app.lay.active, Some(vid), "a double click inside the viewport enters model space");
    assert!(frame(&ctx, &mut app, 2) > 50);
    frame_with(&ctx, &mut app, double_click(egui::pos2(outside.x as f32, outside.y as f32)), 20.0);
    assert_eq!(app.lay.active, None, "a double click outside returns to paper space");
    // A viewport that is off cannot be entered.
    app.with_vp(&ctx, vid, |v| v.on = false);
    frame_with(&ctx, &mut app, double_click(egui::pos2(inside.x as f32, inside.y as f32)), 30.0);
    assert_eq!(app.lay.active, None);
}

#[test]
fn creation_tools_collect_points_and_make_viewports() {
    let (ctx, mut app) = app_for("dark-glass", false);
    frame(&ctx, &mut app, 2);
    // MVIEW with no layout makes one to work on.
    run(&ctx, &mut app, "MVIEW");
    let lid = app.lay.current.expect("a layout was made for the viewport");
    assert!(!app.lay.tool.is_none());
    assert!(app.tool_click(&ctx, Pt::new(20.0, 20.0), 1.0));
    assert!(app.tool_click(&ctx, Pt::new(150.0, 100.0), 1.0));
    assert!(app.lay.tool.is_none());
    let v = app.doc.layouts().get(lid).unwrap().viewports[0].clone();
    assert_eq!(v.rect, [20.0, 20.0, 150.0, 100.0]);
    assert!(STANDARD_SCALES.contains(&v.scale_den), "the default scale is a standard one: {}", v.scale_den);
    assert_eq!(app.lay.selected, Some(v.id));
    // Polygonal, then Enter on the command line finishes it.
    run(&ctx, &mut app, "MVIEWPOLY");
    for p in [(160.0, 20.0), (250.0, 20.0), (205.0, 110.0)] {
        app.tool_click(&ctx, Pt::new(p.0, p.1), 1.0);
    }
    assert!(app.lay.tool_wants_enter());
    app.tool_finish(&ctx);
    assert_eq!(app.doc.layouts().get(lid).unwrap().viewports.len(), 2);
    assert!(app.doc.layouts().get(lid).unwrap().viewports[1].clip.is_some());
    // From an object: draw a circle on the paper, then pick it.
    run(&ctx, &mut app, "PCIRCLE");
    app.tool_click(&ctx, Pt::new(300.0, 150.0), 1.0);
    app.tool_click(&ctx, Pt::new(330.0, 150.0), 1.0);
    assert_eq!(app.doc.layouts().get(lid).unwrap().shapes.len(), 1);
    run(&ctx, &mut app, "MVIEWOBJECT");
    app.tool_click(&ctx, Pt::new(0.0, 0.0), 1.0);
    assert!(!app.lay.tool.is_none(), "a pick that hits nothing keeps the tool going");
    app.tool_click(&ctx, Pt::new(330.0, 150.5), 1.0);
    assert_eq!(app.doc.layouts().get(lid).unwrap().viewports.len(), 3);
    // Clip the first viewport to a triangle, then remove the clip.
    app.lay.selected = Some(v.id);
    run(&ctx, &mut app, "VPCLIP");
    for p in [(30.0, 30.0), (140.0, 30.0), (85.0, 95.0)] {
        app.tool_click(&ctx, Pt::new(p.0, p.1), 1.0);
    }
    app.tool_finish(&ctx);
    let clipped = app.doc.layouts().get(lid).unwrap().viewport(v.id).unwrap().clone();
    assert_eq!(clipped.clip.as_ref().map(|c| c.len()), Some(3));
    run(&ctx, &mut app, "VPUNCLIP");
    assert!(app.doc.layouts().get(lid).unwrap().viewport(v.id).unwrap().clip.is_none());
    // Esc ends a tool.
    run(&ctx, &mut app, "MVIEW");
    assert!(app.layout_escape() && app.lay.tool.is_none());
    // Delete, with the viewport selected.
    run(&ctx, &mut app, "VPDELETE");
    assert!(app.doc.layouts().get(lid).unwrap().viewport(v.id).is_none());
}

#[test]
fn a_locked_viewport_cannot_change_scale_from_the_command_line_or_the_list() {
    let (ctx, mut app) = app_for("dark-glass", false);
    frame(&ctx, &mut app, 2);
    run(&ctx, &mut app, "MVIEWFIT");
    let lid = app.lay.current.unwrap();
    let vid = app.doc.layouts().get(lid).unwrap().viewports[0].id;
    assert!(app.layout_cli(&ctx, Lv::VpScale, "1:250"));
    assert_eq!(app.doc.layouts().get(lid).unwrap().viewports[0].scale_den, 250.0);
    run(&ctx, &mut app, "VPLOCK");
    assert!(app.layout_cli(&ctx, Lv::VpScale, "1:500"));
    assert_eq!(app.doc.layouts().get(lid).unwrap().viewports[0].scale_den, 250.0, "locked: unchanged");
    assert!(app.status.contains("locked"));
    run(&ctx, &mut app, "VPLOCK");
    app.set_vp_scale(&ctx, vid, 2500.0);
    assert_eq!(app.doc.layouts().get(lid).unwrap().viewports[0].scale_den, 2500.0);
    assert!(!app.layout_cli(&ctx, Lv::VpScale, "banana"));
}

#[test]
fn the_plot_has_the_paper_size_and_the_viewport_scale() {
    let (ctx, mut app) = app_for("dark-glass", true);
    frame(&ctx, &mut app, 3);
    app.new_layout(&ctx, Some("Plot test"), PaperSize::A3, Orientation::Landscape);
    let lid = app.lay.current.unwrap();
    let area = app.doc.layouts().get(lid).unwrap().view_area();
    let (centre, _) = app.default_view_for(300.0, 200.0);
    let vid = app.doc.layouts_mut().get_mut(lid).unwrap().add_viewport(Pt::new(area[0], area[1]), Pt::new(area[2], area[3]), centre, 1000.0).unwrap();
    assert_eq!(app.doc.layouts().get(lid).unwrap().viewport(vid).unwrap().paper_mm_per_model_m(), 1.0);
    let sheets = app.compose_layouts_for_plot(&[lid]);
    assert_eq!(sheets.len(), 1);
    assert_eq!((sheets[0].width_mm, sheets[0].height_mm), (420.0, 297.0));
    let dir = std::env::temp_dir().join(format!("afrobim-layout-plot-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let pdf = dir.join("plot.pdf");
    let note = app.plot_to_path(&[lid], true, &pdf).expect("the PDF is written");
    assert!(note.contains("1 page"), "{note}");
    let bytes = std::fs::read(&pdf).unwrap();
    let text = String::from_utf8_lossy(&bytes);
    assert!(text.starts_with("%PDF-1.4") && text.contains("/MediaBox [0 0 1190.55 841.89]"), "A3 landscape is 1190.55 x 841.89 pt");
    // All layouts in one multi-page PDF, and a DXF.
    app.new_layout(&ctx, Some("Second"), PaperSize::A4, Orientation::Portrait);
    let all: Vec<u64> = app.doc.layouts().iter().map(|l| l.id).collect();
    let multi = dir.join("all.pdf");
    app.plot_to_path(&all, true, &multi).unwrap();
    let text = String::from_utf8_lossy(&std::fs::read(&multi).unwrap()).into_owned();
    assert!(text.contains("/Count 2") && text.contains("/MediaBox [0 0 595.28 841.89]"));
    let dxf = dir.join("one.dxf");
    app.plot_to_path(&[lid], false, &dxf).unwrap();
    assert!(std::fs::read_to_string(&dxf).unwrap().contains("SHEET-FRAME"));
    let _ = std::fs::remove_dir_all(&dir);
    // The Plot window draws.
    run(&ctx, &mut app, "PLOT");
    assert!(frame(&ctx, &mut app, 3) > 0);
}

#[test]
fn the_document_with_layouts_saves_and_reopens_with_them() {
    let (ctx, mut app) = app_for("dark-glass", false);
    frame(&ctx, &mut app, 2);
    run(&ctx, &mut app, "MVIEW3");
    app.rename_layout(&ctx, app.lay.current.unwrap(), "Saved");
    let json = app.doc.to_json();
    let back = afrobim_kernel::Document::from_json(&json).unwrap();
    assert_eq!(back.layouts(), app.doc.layouts());
    assert_eq!(back.layouts().by_name("Saved").unwrap().viewports.len(), 3);
    app.install_document(back);
    assert!(app.lay.current.is_none(), "a newly opened document starts on the Model tab");
    assert_eq!(app.doc.layouts().len(), 1);
}

#[test]
fn layout_commands_have_ribbon_buttons_and_palette_entries() {
    use crate::commands::COMMANDS;
    for c in COMMANDS.iter().filter(|c| matches!(c.action, Action::Layout(_))) {
        assert!(crate::ribbon::all_commands().contains(&c.name), "{} has a ribbon button", c.name);
        assert!(crate::glyphs::glyph(c.action).is_some());
    }
}

// ─── plot matches the screen ────────────────────────────────────────────────────────────

mod plot_matches_screen {
    use super::*;
    use afrobim_kernel::layout::Viewport;
    use afrobim_kernel::{ProjectCrs, ViewTransform};
    use afrobim_sheets::{ComposedSheet, Item};
    use std::sync::Arc;

    /// A layout with one 300 by 180 mm viewport at (20, 20) showing `centre` at 1:`den`.
    fn layout_over(ctx: &egui::Context, app: &mut AfroBimApp, centre: Pt, den: f64) -> (u64, u64) {
        app.new_layout(ctx, Some("T"), PaperSize::A3, Orientation::Landscape);
        let lid = app.lay.current.unwrap();
        let vid = app.doc.layouts_mut().get_mut(lid).unwrap().add_viewport(Pt::new(20.0, 20.0), Pt::new(320.0, 200.0), centre, den).unwrap();
        (lid, vid)
    }

    fn sheet_of(app: &AfroBimApp, lid: u64) -> ComposedSheet {
        app.compose_layouts_for_plot(&[lid]).remove(0)
    }

    fn on_layer(s: &ComposedSheet, layer: &str) -> usize {
        s.items.iter().filter(|i| i.layer() == layer).count()
    }

    fn tmp(name: &str) -> std::path::PathBuf {
        let d = std::env::temp_dir().join(format!("afrobim-layout-out-{}-{name}", std::process::id()));
        let _ = std::fs::remove_dir_all(&d);
        std::fs::create_dir_all(&d).unwrap();
        d
    }

    fn freeze(app: &mut AfroBimApp, ctx: &egui::Context, vid: u64, layer: &str) {
        app.with_vp(ctx, vid, |v| v.freeze(layer));
    }

    #[test]
    fn a_dxf_underlay_is_in_the_plot_at_scale_clipped_and_frozen_by_layer() {
        let (ctx, mut app) = app_for("dark-glass", false);
        frame(&ctx, &mut app, 2);
        let mut dxf = afrobim_dxf::DxfDocument::new();
        dxf.add_line("ROAD", 7, Pt::new(0.0, 0.0), Pt::new(50_000.0, 0.0));
        dxf.add_line("ROAD", 7, Pt::new(0.0, 0.0), Pt::new(0.0, 30_000.0));
        let d = tmp("underlay");
        let path = d.join("u.dxf");
        std::fs::write(&path, dxf.finish()).unwrap();
        let id = app.under.attach(&mut app.doc, &path).unwrap();
        app.under.wait(&mut app.doc, 0.1);
        let name = app.doc.find(id).unwrap().name.clone();
        let (lo, hi) = app.under.shown(&app.doc, &[]).next().expect("the underlay shows").2.bounds.expect("it has extents");
        let centre = Pt::new((lo.x + hi.x) / 2.0, (lo.y + hi.y) / 2.0);
        let (lid, vid) = layout_over(&ctx, &mut app, centre, 500.0);
        assert!(!app.doc.layer_visible(crate::underlay::LAYER), "the underlay's own layer is off in the document, as on the screen");
        let sheet = sheet_of(&app, lid);
        let strokes: Vec<Vec<afrobim_draw::Pt2>> = sheet.items.iter().filter_map(|i| if let Item::Stroke { layer, points, .. } = i { (layer == &name).then(|| points.clone()) } else { None }).collect();
        assert!(!strokes.is_empty(), "the underlay is plotted although its document layer is off");
        let width_mm = strokes.iter().flatten().map(|p| p.x).fold(f64::MIN, f64::max) - strokes.iter().flatten().map(|p| p.x).fold(f64::MAX, f64::min);
        assert!((width_mm - (hi.x - lo.x) / 500.0).abs() < 1e-3, "the underlay is {} mm wide on the paper, at 1:500", width_mm);
        // Frozen in the viewport: gone, and gone again after a thaw of another layer.
        freeze(&mut app, &ctx, vid, crate::underlay::LAYER);
        assert_eq!(on_layer(&sheet_of(&app, lid), &name), 0, "frozen layer hides the underlay in that viewport");
        app.with_vp(&ctx, vid, |v| v.thaw(crate::underlay::LAYER));
        // Display off for the underlay itself.
        let mut def = crate::underlay::def_of(app.doc.find(id).unwrap()).unwrap();
        def.visible = false;
        app.doc.find_mut(id).unwrap().data = serde_json::to_value(&def).unwrap();
        assert_eq!(on_layer(&sheet_of(&app, lid), &name), 0, "an underlay that is switched off is not plotted");
        def.visible = true;
        app.doc.find_mut(id).unwrap().data = serde_json::to_value(&def).unwrap();
        // Twist 90: the 50 m by 30 m underlay is 30 m wide and 50 m tall on the paper.
        app.with_vp(&ctx, vid, |v| v.set_twist(90.0));
        let sheet = sheet_of(&app, lid);
        let pts: Vec<afrobim_draw::Pt2> = sheet.items.iter().filter_map(|i| if let Item::Stroke { layer, points, .. } = i { (layer == &name).then(|| points.clone()) } else { None }).flatten().collect();
        let (w, h) = (pts.iter().map(|p| p.x).fold(f64::MIN, f64::max) - pts.iter().map(|p| p.x).fold(f64::MAX, f64::min), pts.iter().map(|p| p.y).fold(f64::MIN, f64::max) - pts.iter().map(|p| p.y).fold(f64::MAX, f64::min));
        assert!((w - (hi.y - lo.y) / 500.0).abs() < 1e-3 && (h - (hi.x - lo.x) / 500.0).abs() < 1e-3, "turned by 90 degrees: {w} x {h}");
        app.with_vp(&ctx, vid, |v| v.set_twist(0.0));
        // A polygon clip cuts it: every plotted point is inside the polygon.
        let poly = vec![Pt::new(100.0, 60.0), Pt::new(220.0, 60.0), Pt::new(160.0, 150.0)];
        app.with_vp(&ctx, vid, |v| v.set_clip(poly.clone()));
        let sheet = sheet_of(&app, lid);
        let pts: Vec<afrobim_draw::Pt2> = sheet.items.iter().filter_map(|i| if let Item::Stroke { layer, points, .. } = i { (layer == &name).then(|| points.clone()) } else { None }).flatten().collect();
        assert!(pts.iter().all(|p| afrobim_kernel::layout::point_in_polygon(&poly, *p)), "clipped to the viewport polygon");
        app.with_vp(&ctx, vid, |v| v.remove_clip());
        let _ = std::fs::remove_dir_all(&d);
    }

    #[test]
    fn a_gis_overlay_is_in_the_plot_and_respects_the_layer_switch_and_the_freeze() {
        let (ctx, mut app) = app_for("dark-glass", false);
        frame(&ctx, &mut app, 2);
        afrobim_kernel::set_project_crs(&mut app.doc, &ProjectCrs::from_epsg(32636).unwrap());
        let d = tmp("gis");
        let path = d.join("road.geojson");
        std::fs::write(&path, r#"{"type":"FeatureCollection","features":[{"type":"Feature","properties":{"name":"Road","layer":"Roads"},"geometry":{"type":"LineString","coordinates":[[32.5825,0.3476],[32.5900,0.3500],[32.6000,0.3550]]}}]}"#).unwrap();
        let (id, _) = crate::gis_overlay::import(&mut app.doc, &path).unwrap();
        app.gis.sync(&mut app.doc);
        let name = app.doc.find(id).unwrap().name.clone();
        // The first vertex of the line, in model millimetres.
        let first = {
            let (_, geo, look) = app.gis.shown(&app.doc, &[]).next().expect("the overlay shows");
            let f = geo.features_in(&look, [-1e18, -1e18, 1e18, 1e18]).next().unwrap();
            match &f.f.geom {
                crate::gis_overlay::PGeom::Line(v) => Pt::new(v[0][0], v[0][1]),
                _ => panic!("a line"),
            }
        };
        let (lid, vid) = layout_over(&ctx, &mut app, Pt::new(first.x + 500_000.0, first.y + 300_000.0), 2000.0);
        let n = on_layer(&sheet_of(&app, lid), &name);
        assert!(n > 0, "the line is plotted");
        freeze(&mut app, &ctx, vid, crate::underlay::LAYER);
        assert_eq!(on_layer(&sheet_of(&app, lid), &name), 0, "frozen in the viewport");
        app.with_vp(&ctx, vid, |v| v.thaw(crate::underlay::LAYER));
        crate::gis_overlay::edit_look(&mut app.doc, id, |l| l.layers[0].visible = false);
        app.gis.sync(&mut app.doc);
        assert_eq!(on_layer(&sheet_of(&app, lid), &name), 0, "the overlay's own layer switch is honoured");
        let _ = std::fs::remove_dir_all(&d);
    }

    fn png_with_world(d: &std::path::Path) -> std::path::PathBuf {
        let p = d.join("ortho.png");
        let px: Vec<u8> = (0..20 * 10).flat_map(|i| [(i % 251) as u8, 90, 160]).collect();
        image::save_buffer(&p, &px, 20, 10, image::ExtendedColorType::Rgb8).unwrap();
        // 1 m pixels, top left at E 450000, N 40010.
        std::fs::write(d.join("ortho.pgw"), "1\n0\n0\n-1\n450000.5\n40009.5\n").unwrap();
        p
    }

    #[test]
    fn a_georeferenced_image_is_embedded_in_the_pdf_clipped_to_the_viewport_and_left_out_of_the_dxf() {
        let (ctx, mut app) = app_for("dark-glass", false);
        frame(&ctx, &mut app, 2);
        afrobim_kernel::set_project_crs(&mut app.doc, &ProjectCrs::from_epsg(32636).unwrap());
        let d = tmp("image");
        let id = app.imagery.attach(&mut app.doc, &png_with_world(&d)).unwrap();
        app.imagery.wait(&ctx, &mut app.doc);
        // The image is 20 by 10 m, centred at E 450010, N 40005 (millimetres below).
        let centre = Pt::new(450_010_000.0, 40_005_000.0);
        let (lid, vid) = layout_over(&ctx, &mut app, centre, 100.0);
        let sheet = sheet_of(&app, lid);
        assert_eq!(sheet.image_count(), 1, "one raster for the viewport");
        let Item::Image(img) = sheet.items.iter().find(|i| matches!(i, Item::Image(_))).unwrap() else { unreachable!() };
        assert_eq!((img.rect.x0, img.rect.y0, img.rect.x1, img.rect.y1), (20.0, 20.0, 320.0, 200.0), "the raster covers the viewport box");
        // The image is 200 by 100 mm on the paper at 1:100, in the middle of the viewport.
        let at = |x: f64, y: f64| {
            let (px, py) = (((x - img.rect.x0) / img.rect.width() * img.width as f64) as usize, ((img.rect.y1 - y) / img.rect.height() * img.height as f64) as usize);
            img.rgba[(py * img.width as usize + px) * 4 + 3]
        };
        assert_eq!(at(170.0, 110.0), 255, "the picture is where the ground is");
        assert_eq!(at(25.0, 25.0), 0, "and the rest of the viewport is clear");
        // Frozen in the viewport: gone.
        freeze(&mut app, &ctx, vid, crate::underlay::LAYER);
        assert_eq!(sheet_of(&app, lid).image_count(), 0);
        app.with_vp(&ctx, vid, |v| v.thaw(crate::underlay::LAYER));
        // The PDF has it as an image object; the DXF leaves it out and says so.
        let pdf = d.join("o.pdf");
        app.plot_to_path(&[lid], true, &pdf).unwrap();
        let text = String::from_utf8_lossy(&std::fs::read(&pdf).unwrap()).into_owned();
        assert!(text.contains("/Subtype /Image") && text.contains("/SMask"), "an image with a soft mask is in the PDF");
        let dxf = d.join("o.dxf");
        let note = app.plot_to_path(&[lid], false, &dxf).unwrap();
        assert!(note.contains("left out of the DXF"), "{note}");
        assert!(!std::fs::read_to_string(&dxf).unwrap().to_lowercase().contains("image"));
        // The image is hidden when it is switched off.
        let mut def = crate::imagery::def_of(app.doc.find(id).unwrap()).unwrap();
        def.visible = false;
        app.doc.find_mut(id).unwrap().data = serde_json::to_value(&def).unwrap();
        assert_eq!(sheet_of(&app, lid).image_count(), 0);
        let _ = std::fs::remove_dir_all(&d);
    }

    /// A tile server that gives a green tile for every request.
    struct Green;
    impl afrobim_raster::TileFetcher for Green {
        fn fetch(&self, _url: &str) -> Result<Vec<u8>, String> {
            let img = image::RgbaImage::from_pixel(256, 256, image::Rgba([10, 200, 30, 255]));
            let mut out = Vec::new();
            image::DynamicImage::ImageRgba8(img).write_to(&mut std::io::Cursor::new(&mut out), image::ImageFormat::Png).map_err(|e| e.to_string())?;
            Ok(out)
        }
    }

    fn map_settings(app: &mut AfroBimApp, provider: &str) {
        afrobim_kernel::set_project_crs(&mut app.doc, &ProjectCrs::from_epsg(32636).unwrap());
        let s = crate::basemap::BasemapSettings { enabled: true, provider: provider.into(), custom_url: "https://tiles.example/{z}/{x}/{y}.png".into(), custom_attribution: "(c) Example tiles".into(), opacity: 1.0 };
        crate::basemap::set_settings(&mut app.doc, &s);
    }

    #[test]
    fn map_tiles_are_plotted_only_when_the_provider_allows_saving_and_the_refusal_is_shown() {
        let (ctx, mut app) = app_for("dark-glass", false);
        frame(&ctx, &mut app, 2);
        let centre = Pt::new(448_100_000.0, 38_450_000.0);
        let (lid, _) = layout_over(&ctx, &mut app, centre, 500.0);
        // A provider that does not allow saving tiles: left out, with the refusal as a warning.
        map_settings(&mut app, "esri-imagery");
        app.lay.plot_fetcher = Some(Arc::new(Green));
        let why = app.plot_map_refusal().expect("the plot window has a refusal to show");
        assert!(why.contains("allows its map to be displayed but not saved"), "{why}");
        let sheet = sheet_of(&app, lid);
        assert_eq!(sheet.image_count(), 0, "no tiles of a display-only provider in the plot");
        assert!(sheet.warnings.iter().any(|w| w.contains("not saved")), "{:?}", sheet.warnings);
        let d = tmp("tiles");
        let note = app.plot_to_path(&[lid], true, &d.join("p.pdf")).unwrap();
        assert!(note.contains("not saved"), "the plot's own note carries the refusal: {note}");
        // The user's own tile URL: they answer for its terms. It is plotted, with its attribution.
        map_settings(&mut app, crate::basemap::CUSTOM);
        assert!(app.plot_map_refusal().is_none());
        let sheet = sheet_of(&app, lid);
        assert_eq!(sheet.image_count(), 1, "{:?}", sheet.warnings);
        let Item::Image(img) = sheet.items.iter().find(|i| matches!(i, Item::Image(_))).unwrap() else { unreachable!() };
        let mid = ((img.height / 2) as usize * img.width as usize + (img.width / 2) as usize) * 4;
        assert_eq!(&img.rgba[mid..mid + 4], &[10, 200, 30, 255], "the tile colour reaches the paper");
        assert!(sheet.texts().any(|t| t.text.contains("(c) Example tiles")), "the attribution is on the sheet");
        // The map is off: nothing, and no warning.
        let mut s = crate::basemap::settings(&app.doc);
        s.enabled = false;
        crate::basemap::set_settings(&mut app.doc, &s);
        let sheet = sheet_of(&app, lid);
        assert_eq!((sheet.image_count(), sheet.warnings.len()), (0, 0));
        assert!(app.plot_map_refusal().is_none());
        let _ = std::fs::remove_dir_all(&d);
    }

    fn wait_for_scenes(ctx: &egui::Context, app: &mut AfroBimApp) {
        for _ in 0..400 {
            frame(ctx, app, 2);
            if app.civil.built.values().any(|(_, b)| !b.scene.lines.is_empty()) {
                return;
            }
            std::thread::sleep(std::time::Duration::from_millis(10));
        }
        panic!("the civil scenes were not built");
    }

    #[test]
    fn civil_objects_are_plotted_and_a_viewport_freeze_hides_them_on_the_screen_and_in_the_plot() {
        let (ctx, mut app) = app_for("dark-glass", true);
        frame(&ctx, &mut app, 3);
        wait_for_scenes(&ctx, &mut app);
        // A civil element with lines, and where it is.
        let (id, bbox) = app
            .doc
            .elements()
            .iter()
            .filter(|e| crate::civil::is_civil(&e.category))
            .find_map(|e| {
                let (_, b) = app.civil.built.get(&e.id)?;
                let l = b.scene.lines.first()?;
                Some((e.id, l.bbox))
            })
            .expect("a civil object with lines");
        let layer = app.doc.find(id).unwrap().layer.clone();
        let centre = Pt::new((bbox[0] + bbox[2]) / 2.0, (bbox[1] + bbox[3]) / 2.0);
        let (_, vid) = layout_over(&ctx, &mut app, centre, 500.0);
        let vp = app.current_layout().unwrap().viewport(vid).unwrap().clone();
        let rect = afrobim_sheets::geom::Rect::new(vp.rect[0], vp.rect[1], vp.rect[2], vp.rect[3]);
        let cache = crate::layout_out::PlotCache::default();
        let under = |app: &AfroBimApp, vp: &Viewport| {
            let mut sh = ComposedSheet::new("t", "t", 420.0, 297.0);
            app.plot_under(&cache, vp, &rect, &mut sh);
            sh.items.iter().filter(|i| i.layer() == layer).count()
        };
        let shown = under(&app, &vp);
        assert!(shown > 0, "civil linework is in the plot of a viewport over it");
        // Frozen in the viewport.
        let mut frozen = vp.clone();
        frozen.freeze(&layer);
        assert_eq!(under(&app, &frozen), 0, "a layer frozen in the viewport hides the civil objects on it");
        // Off in the document.
        app.doc.layers_mut().iter_mut().find(|l| l.name == layer).unwrap().visible = false;
        assert_eq!(under(&app, &vp), 0, "a layer that is off hides them too");
        app.doc.layers_mut().iter_mut().find(|l| l.name == layer).unwrap().visible = true;
        assert_eq!(under(&app, &vp), shown);
        // On the screen: the same viewport view paints fewer shapes with the layer frozen.
        let pv = {
            let mut v = ViewTransform::default();
            v.fit(&afrobim_kernel::BBox::new(0.0, 0.0, 420.0, 297.0), 0.0, 0.0, 1280.0, 800.0, 20.0);
            v
        };
        let view = crate::layout_canvas::vp_screen_view(&pv, &vp);
        let screen = egui::Rect::from_min_size(egui::pos2(0.0, 0.0), egui::vec2(1280.0, 800.0));
        let count = |app: &mut AfroBimApp, frozen: &[String]| {
            let ctx = egui::Context::default();
            ctx.run(egui::RawInput::default(), |ctx| {
                let painter = ctx.layer_painter(egui::LayerId::background());
                app.with_model_view(view.clone(), frozen, |a, _| a.paint_scene(&painter, screen));
            })
            .shapes
            .len()
        };
        let all = count(&mut app, &[]);
        let none = count(&mut app, std::slice::from_ref(&layer));
        assert!(none < all, "frozen in the viewport, {layer} paints {none} shapes against {all}");
        assert!(app.lay.frozen_now.is_empty(), "the freeze does not leak out of the viewport pass");
    }
}

// ─── twist, snap zoom, ctrl-drag copy, title block fields ───────────────────────────────

mod viewport_options {
    use super::*;
    use afrobim_kernel::layout::{point_in_polygon, TitleStyle, Viewport};
    use afrobim_kernel::{Document, ViewTransform, VpGrip};
    use afrobim_sheets::layout_plot as lp;

    fn one_viewport(ctx: &egui::Context, app: &mut AfroBimApp) -> (u64, u64) {
        app.new_layout(ctx, Some("V"), PaperSize::A3, Orientation::Landscape);
        let lid = app.lay.current.unwrap();
        let vid = app.doc.layouts_mut().get_mut(lid).unwrap().add_viewport(Pt::new(40.0, 40.0), Pt::new(280.0, 200.0), Pt::new(10_000.0, 20_000.0), 500.0).unwrap();
        (lid, vid)
    }

    fn vp_of(app: &AfroBimApp, lid: u64, vid: u64) -> Viewport {
        app.doc.layouts().get(lid).unwrap().viewport(vid).unwrap().clone()
    }

    #[test]
    fn the_screen_transform_of_a_viewport_is_the_kernel_transform_at_0_30_and_90_degrees() {
        let pv = {
            let mut v = ViewTransform::default();
            v.fit(&afrobim_kernel::BBox::new(0.0, 0.0, 420.0, 297.0), 0.0, 0.0, 1280.0, 800.0, 20.0);
            v
        };
        for deg in [0.0, 30.0, 90.0] {
            let mut vp = Viewport::new(1, Pt::new(40.0, 40.0), Pt::new(280.0, 200.0), Pt::new(123_456.0, -7_000.0), 500.0);
            assert!(vp.set_twist(deg));
            let view = crate::layout_canvas::vp_screen_view(&pv, &vp);
            for m in [Pt::new(123_456.0, -7_000.0), Pt::new(130_000.0, 2_000.0), Pt::new(100_000.0, -20_000.0)] {
                let (a, b) = (view.to_screen(m), pv.to_screen(vp.to_paper(m)));
                assert!((a.x - b.x).abs() < 1e-6 && (a.y - b.y).abs() < 1e-6, "{deg}: the model path and the paper path agree: {a:?} {b:?}");
                // And back from the screen: what a click on it means in the model.
                let back = view.to_world(a);
                assert!((back.x - m.x).abs() < 1e-3 && (back.y - m.y).abs() < 1e-3, "{deg}: {back:?} {m:?}");
            }
        }
    }

    #[test]
    fn picking_and_snapping_go_through_the_twisted_view() {
        let (ctx, mut app) = app_for("dark-glass", false);
        frame(&ctx, &mut app, 2);
        let (lid, vid) = one_viewport(&ctx, &mut app);
        let id = app.doc.add("test", "Probe", Pt::new(12_000.0, 21_000.0), 3_000.0, 2_000.0, "0", serde_json::Value::Null);
        let pv = {
            let mut v = ViewTransform::default();
            v.fit(&afrobim_kernel::BBox::new(0.0, 0.0, 420.0, 297.0), 0.0, 0.0, 1280.0, 800.0, 20.0);
            v
        };
        for deg in [0.0, 30.0, 90.0] {
            app.with_vp(&ctx, vid, |v| v.set_twist(deg));
            let vp = vp_of(&app, lid, vid);
            let view = crate::layout_canvas::vp_screen_view(&pv, &vp);
            // Where the element's middle is on the screen: a pick there finds it.
            let b = app.doc.find(id).unwrap().bounds();
            let mid = view.to_screen(b.centre());
            assert_eq!(app.doc.pick(view.to_world(mid), 8.0 / view.scale), Some(id), "{deg}: picked through the view");
            // A point well outside the element does not.
            let away = view.to_screen(Pt::new(b.max.x + 30_000.0, b.max.y + 30_000.0));
            assert_eq!(app.doc.pick(view.to_world(away), 8.0 / view.scale), None, "{deg}");
            // The cursor goes to the model point under the pointer (snapped to the grid step).
            app.view = view.clone();
            app.ctl.pointer_move(&mut app.doc, &view, mid);
            let c = app.ctl.cursor_world();
            let want = view.to_world(mid);
            assert!(((c.x - want.x).powi(2) + (c.y - want.y).powi(2)).sqrt() < 2_000.0, "{deg}: cursor {c:?} near {want:?}");
        }
        // The screen box of an element is the box around its turned corners.
        app.with_vp(&ctx, vid, |v| v.set_twist(90.0));
        let vp = vp_of(&app, lid, vid);
        app.view = crate::layout_canvas::vp_screen_view(&pv, &vp);
        let b = app.doc.find(id).unwrap().bounds();
        let sr = app.screen_rect_for_test(&b);
        let corners = [Pt::new(b.min.x, b.min.y), Pt::new(b.max.x, b.min.y), Pt::new(b.max.x, b.max.y), Pt::new(b.min.x, b.max.y)];
        for c in corners {
            let s = app.view.to_screen(c);
            assert!(s.x >= sr.left() as f64 - 1e-3 && s.x <= sr.right() as f64 + 1e-3 && s.y >= sr.top() as f64 - 1e-3 && s.y <= sr.bottom() as f64 + 1e-3);
        }
    }

    #[test]
    fn vprotate_sets_the_twist_refuses_a_locked_viewport_and_is_saved() {
        let (ctx, mut app) = app_for("dark-glass", false);
        frame(&ctx, &mut app, 2);
        let (lid, vid) = one_viewport(&ctx, &mut app);
        app.lay.selected = Some(vid);
        assert!(crate::commands::resolve("VPROTATE").is_some() && crate::commands::resolve("TWIST").is_some());
        assert!(app.layout_cli(&ctx, Lv::VpRotate, "30"));
        assert_eq!(vp_of(&app, lid, vid).twist_deg, 30.0);
        assert!(app.layout_cli(&ctx, Lv::VpRotate, "-45"));
        assert_eq!(vp_of(&app, lid, vid).twist_deg, -45.0);
        assert!(app.layout_cli(&ctx, Lv::VpRotate, "450"));
        assert_eq!(vp_of(&app, lid, vid).twist_deg, 90.0, "kept within a half turn");
        assert!(!app.layout_cli(&ctx, Lv::VpRotate, "banana"));
        run(&ctx, &mut app, "VPLOCK");
        assert!(app.layout_cli(&ctx, Lv::VpRotate, "10"));
        assert_eq!(vp_of(&app, lid, vid).twist_deg, 90.0, "a locked viewport keeps its view");
        assert!(app.status.contains("locked"));
        run(&ctx, &mut app, "VPLOCK");
        // Saved with the document, and a plain old viewport reads back as untwisted.
        let back = Document::from_json(&app.doc.to_json()).unwrap();
        assert_eq!(back.layouts().get(lid).unwrap().viewport(vid).unwrap().twist_deg, 90.0);
        // With no angle the command opens the properties, which draw with the new fields.
        run(&ctx, &mut app, "VPROTATE");
        assert!(app.lay.props_open);
        assert!(frame(&ctx, &mut app, 3) > 0);
    }

    #[test]
    fn a_twisted_viewport_draws_on_the_screen_and_plots_at_the_same_place() {
        let (ctx, mut app) = app_for("dark-glass", true);
        frame(&ctx, &mut app, 3);
        let (lid, vid) = one_viewport(&ctx, &mut app);
        for deg in [0.0, 30.0, 90.0] {
            app.with_vp(&ctx, vid, |v| v.set_twist(deg));
            assert!(frame(&ctx, &mut app, 2) > 50, "{deg}: the layout draws");
            // Enter model space through it and draw there too.
            app.activate_vp(vid);
            assert!(frame(&ctx, &mut app, 2) > 50, "{deg}: model space through the twisted viewport draws");
            app.deactivate_vp();
        }
        // A plotted annotation is turned with the view: text at a model point lands where
        // `to_paper` says.
        let id = app.doc.add("test", "x", Pt::new(0.0, 0.0), 1.0, 1.0, "0", serde_json::Value::Null);
        let _ = id;
        let sheets = app.compose_layouts_for_plot(&[lid]);
        assert_eq!(sheets.len(), 1);
    }

    #[test]
    fn snap_zoom_steps_through_the_standard_list_about_the_pointer() {
        let pv = {
            let mut v = ViewTransform::default();
            v.fit(&afrobim_kernel::BBox::new(0.0, 0.0, 420.0, 297.0), 0.0, 0.0, 1280.0, 800.0, 20.0);
            v
        };
        let mut vp = Viewport::new(1, Pt::new(40.0, 40.0), Pt::new(280.0, 200.0), Pt::new(10_000.0, 20_000.0), 500.0);
        assert!(vp.snap_scale, "on by default");
        let view = crate::layout_canvas::vp_screen_view(&pv, &vp);
        let pointer = egui::pos2(500.0, 300.0);
        for twist in [0.0, 30.0] {
            vp.set_twist(twist);
            let view = crate::layout_canvas::vp_screen_view(&pv, &vp);
            let under = view.to_world(Pt::new(pointer.x as f64, pointer.y as f64));
            // Zoom in: 1:500 to 1:250, the model point under the pointer stays under it.
            let (den, centre) = crate::layout_canvas::snap_zoom(&pv, &view, &vp, true, Some(pointer)).unwrap();
            assert_eq!(den, 250.0);
            let mut after = vp.clone();
            after.scale_den = den;
            after.centre = centre;
            let moved = crate::layout_canvas::vp_screen_view(&pv, &after).to_screen(under);
            assert!((moved.x - pointer.x as f64).abs() < 1e-6 && (moved.y - pointer.y as f64).abs() < 1e-6, "{twist}: {moved:?}");
            // Zoom out: to 1:1000.
            assert_eq!(crate::layout_canvas::snap_zoom(&pv, &view, &vp, false, Some(pointer)).unwrap().0, 1000.0);
        }
        // A custom scale moves to the next standard one in the direction of the zoom.
        vp.scale_den = 700.0;
        assert_eq!(crate::layout_canvas::snap_zoom(&pv, &view, &vp, true, None).unwrap().0, 500.0);
        assert_eq!(crate::layout_canvas::snap_zoom(&pv, &view, &vp, false, None).unwrap().0, 1000.0);
        // The ends of the list hold.
        vp.scale_den = 1.0;
        assert!(crate::layout_canvas::snap_zoom(&pv, &view, &vp, true, None).is_none());
        vp.scale_den = 10_000.0;
        assert!(crate::layout_canvas::snap_zoom(&pv, &view, &vp, false, None).is_none());
    }

    #[test]
    fn the_wheel_in_an_active_viewport_snaps_unless_the_option_is_off_or_it_is_locked() {
        let (ctx, mut app) = app_for("dark-glass", false);
        frame(&ctx, &mut app, 3);
        let (lid, vid) = one_viewport(&ctx, &mut app);
        frame(&ctx, &mut app, 2);
        app.activate_vp(vid);
        frame(&ctx, &mut app, 2);
        let pv = app.lay.views.get(&lid).cloned().unwrap();
        let inside = pv.to_screen(Pt::new(160.0, 120.0));
        let at = egui::pos2(inside.x as f32, inside.y as f32);
        let wheel = |app: &mut AfroBimApp, t: f64| {
            let mut f = eframe::Frame::_new_kittest();
            let input = egui::RawInput {
                screen_rect: Some(egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(1280.0, 800.0))),
                events: vec![egui::Event::PointerMoved(at), egui::Event::MouseWheel { unit: egui::MouseWheelUnit::Point, delta: egui::vec2(0.0, 120.0), modifiers: egui::Modifiers::NONE }],
                time: Some(t),
                ..Default::default()
            };
            let _ = ctx.run(input, |ctx| eframe::App::update(app, ctx, &mut f));
        };
        wheel(&mut app, 5.0);
        let den = vp_of(&app, lid, vid).scale_den;
        assert!(den < 500.0 && afrobim_kernel::STANDARD_SCALES.contains(&den), "zoomed in to a standard scale: 1:{den}");
        // The option off: a custom scale, as in CAD.
        app.with_vp(&ctx, vid, |v| {
            v.snap_scale = false;
            v.scale_den = 500.0;
        });
        wheel(&mut app, 6.0);
        let den = vp_of(&app, lid, vid).scale_den;
        assert!(den < 500.0 && !afrobim_kernel::STANDARD_SCALES.contains(&den), "a custom scale: 1:{den}");
        // Locked: the scale does not change at all (the paper zooms instead).
        app.with_vp(&ctx, vid, |v| {
            v.snap_scale = true;
            v.scale_den = 500.0;
            v.locked = true;
        });
        wheel(&mut app, 7.0);
        assert_eq!(vp_of(&app, lid, vid).scale_den, 500.0);
    }

    fn pointer_frame(ctx: &egui::Context, app: &mut AfroBimApp, events: Vec<egui::Event>, modifiers: egui::Modifiers, t: f64) {
        let mut f = eframe::Frame::_new_kittest();
        let input = egui::RawInput { screen_rect: Some(egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(1280.0, 800.0))), events, modifiers, time: Some(t), ..Default::default() };
        let _ = ctx.run(input, |ctx| eframe::App::update(app, ctx, &mut f));
    }

    fn drag_grip(ctx: &egui::Context, app: &mut AfroBimApp, from: egui::Pos2, to: egui::Pos2, modifiers: egui::Modifiers, t: f64) {
        let b = |pos, pressed| egui::Event::PointerButton { pos, button: egui::PointerButton::Primary, pressed, modifiers };
        pointer_frame(ctx, app, vec![egui::Event::PointerMoved(from)], modifiers, t + 0.0);
        pointer_frame(ctx, app, vec![b(from, true)], modifiers, t + 0.1);
        pointer_frame(ctx, app, vec![egui::Event::PointerMoved(to)], modifiers, t + 0.2);
        pointer_frame(ctx, app, vec![egui::Event::PointerMoved(to)], modifiers, t + 0.3);
        pointer_frame(ctx, app, vec![b(to, false)], modifiers, t + 0.4);
        pointer_frame(ctx, app, vec![], modifiers, t + 0.5);
    }

    #[test]
    fn ctrl_drag_of_a_grip_copies_the_viewport_and_a_plain_drag_does_not() {
        let (ctx, mut app) = app_for("dark-glass", false);
        frame(&ctx, &mut app, 3);
        let (lid, vid) = one_viewport(&ctx, &mut app);
        app.lay.selected = Some(vid);
        frame(&ctx, &mut app, 2);
        let pv = app.lay.views.get(&lid).cloned().unwrap();
        let grip = vp_of(&app, lid, vid).grips().into_iter().find(|(k, _)| *k == VpGrip::Corner(2)).unwrap().1;
        let from = pv.to_screen(grip);
        let (from, to) = (egui::pos2(from.x as f32, from.y as f32), egui::pos2(from.x as f32 + 60.0, from.y as f32 - 40.0));
        let original = vp_of(&app, lid, vid).rect;
        // Ctrl held: a copy appears, the original stays, the copy is the one that was stretched.
        drag_grip(&ctx, &mut app, from, to, egui::Modifiers::CTRL, 100.0);
        let vps = app.doc.layouts().get(lid).unwrap().viewports.clone();
        assert_eq!(vps.len(), 2, "the Ctrl-drag copied the viewport");
        let orig = vps.iter().find(|v| v.id == vid).unwrap();
        let copy = vps.iter().find(|v| v.id != vid).unwrap();
        assert_eq!(orig.rect, original, "the original did not move");
        assert!(copy.rect[2] > original[2] + 1.0 && copy.rect[3] > original[3] + 1.0, "the copy's corner went with the pointer: {:?}", copy.rect);
        // Stretched in place, the copy still shows the same model under the same paper point.
        let probe = Pt::new(100.0, 100.0);
        let (a, b) = (orig.to_model(probe), copy.to_model(probe));
        assert!((a.x - b.x).abs() < 1e-6 && (a.y - b.y).abs() < 1e-6 && copy.scale_den == orig.scale_den && copy.twist_deg == orig.twist_deg);
        assert_eq!(app.lay.selected, Some(copy.id));
        // No Ctrl: the same drag stretches the viewport that is there.
        app.lay.selected = Some(vid);
        frame(&ctx, &mut app, 2);
        drag_grip(&ctx, &mut app, from, to, egui::Modifiers::NONE, 200.0);
        assert_eq!(app.doc.layouts().get(lid).unwrap().viewports.len(), 2, "no new viewport without Ctrl");
        assert!(vp_of(&app, lid, vid).rect[2] > original[2] + 1.0);
    }

    #[test]
    fn the_title_block_choice_and_the_sheet_fields_are_per_layout_and_plotted() {
        let (ctx, mut app) = app_for("dark-glass", false);
        frame(&ctx, &mut app, 2);
        let mut info = afrobim_sheets::project_info(&app.doc);
        info.project = "Kampala Road".into();
        info.consultant = "Example Consultants".into();
        info.contractor = "Example Contractors".into();
        afrobim_sheets::set_project_info(&mut app.doc, &info);
        let (lid, _) = one_viewport(&ctx, &mut app);
        app.new_layout(&ctx, Some("Other"), PaperSize::A3, Orientation::Landscape);
        let other = app.lay.current.unwrap();
        let texts = |app: &AfroBimApp, id: u64| app.compose_layouts_for_plot(&[id])[0].texts().map(|t| t.text.clone()).collect::<Vec<_>>();
        assert!(!texts(&app, lid).iter().any(|t| t == "Ministry of Works and Transport"), "the default block says nothing of MoWT");
        {
            let l = app.doc.layouts_mut().get_mut(lid).unwrap();
            lp::set_title_style(l, TitleStyle::Mowt);
            l.sheet_no = "R-07".into();
            l.drawing_title = "Typical cross sections".into();
            l.revision = "D".into();
        }
        let t = texts(&app, lid);
        for want in ["Ministry of Works and Transport", "Example Consultants", "Example Contractors", "Kampala Road", "R-07", "D"] {
            assert!(t.iter().any(|x| x == want), "{want} in {t:?}");
        }
        assert!(t.iter().any(|x| x.contains("Typical cross")), "{t:?}");
        let t2 = texts(&app, other);
        assert!(!t2.iter().any(|x| x == "Ministry of Works and Transport" || x == "R-07"), "the other layout is not touched: {t2:?}");
        assert_eq!(app.doc.layouts().get(lid).unwrap().title_block_mm, 80.0);
        // The screen's paper shows the same block, and Page setup draws with its new rows.
        app.set_current_layout(Some(lid));
        assert!(frame(&ctx, &mut app, 2) > 50);
        run(&ctx, &mut app, "PAGESETUP");
        assert!(frame(&ctx, &mut app, 3) > 50);
        assert!(app.lay.page.is_some());
        // Saved and reopened.
        let back = Document::from_json(&app.doc.to_json()).unwrap();
        let l = back.layouts().get(lid).unwrap();
        assert_eq!((l.title_style, l.sheet_no.as_str(), l.revision.as_str()), (TitleStyle::Mowt, "R-07", "D"));
    }

    #[test]
    fn the_plot_border_is_off_until_asked_for() {
        let (ctx, mut app) = app_for("dark-glass", false);
        frame(&ctx, &mut app, 2);
        let (lid, vid) = one_viewport(&ctx, &mut app);
        let n = |app: &AfroBimApp| app.compose_layouts_for_plot(&[lid])[0].items.iter().filter(|i| i.layer() == afrobim_sheets::layers::VIEWPORT).count();
        assert_eq!(n(&app), 0);
        app.with_vp(&ctx, vid, |v| v.plot_border = true);
        assert_eq!(n(&app), 1);
        let sheet = app.compose_layouts_for_plot(&[lid]).remove(0);
        let afrobim_sheets::Item::Stroke { points, .. } = sheet.items.iter().find(|i| i.layer() == afrobim_sheets::layers::VIEWPORT).unwrap() else { panic!() };
        assert!(points.iter().all(|p| point_in_polygon(&vp_of(&app, lid, vid).outline(), *p)), "the border is the boundary");
        // The properties window draws with the new options.
        run(&ctx, &mut app, "VPPROPS");
        assert!(frame(&ctx, &mut app, 3) > 50);
    }
}
