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
