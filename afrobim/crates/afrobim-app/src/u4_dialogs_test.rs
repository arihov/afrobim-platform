// SPDX-License-Identifier: LicenseRef-AfroBIM-Proprietary
// Copyright (c) 2026 AfroBIM. All rights reserved.
//! Opens every registered command headlessly for a few frames, in the dark and in the light
//! look, so that a dialog that panics (or a command that leaves the window empty) is found
//! without a screen. `ctx.run` drives the application the way eframe would, with no window.

use std::panic::{catch_unwind, AssertUnwindSafe};

use crate::app::AfroBimApp;
use crate::commands::COMMANDS;
use crate::Options;

pub(crate) fn app_for(theme: &str, civil: bool) -> (egui::Context, AfroBimApp) {
    let ctx = egui::Context::default();
    let cc = eframe::CreationContext::_new_kittest(ctx.clone());
    // A screenshot path keeps the application from reading or writing the user's preferences; the
    // frames run here stop before the one that would take the picture.
    let options = Options { demo: !civil, civil_demo: civil, theme: Some(theme.to_string()), screenshot: Some(std::env::temp_dir().join("afrobim-dialog-test.png")), ..Options::default() };
    let app = AfroBimApp::new(&cc, options);
    (ctx, app)
}

pub(crate) fn frame(ctx: &egui::Context, app: &mut AfroBimApp, frames: u32) -> usize {
    let mut shapes = 0;
    let mut f = eframe::Frame::_new_kittest();
    for _ in 0..frames {
        let input = egui::RawInput { screen_rect: Some(egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(1280.0, 800.0))), ..Default::default() };
        let out = ctx.run(input, |ctx| eframe::App::update(app, ctx, &mut f));
        shapes = out.shapes.len();
    }
    shapes
}

/// How many windows the application has open (areas on the middle layer).
pub(crate) fn windows(ctx: &egui::Context) -> usize {
    ctx.memory(|m| m.areas().visible_layer_ids().iter().filter(|l| l.order == egui::Order::Middle).count())
}

/// Opens each command in a fresh application and returns the problems found.
fn run_all(theme: &str, civil: bool) -> Vec<String> {
    let mut problems: Vec<String> = Vec::new();
    let mut no_window: Vec<&str> = Vec::new();
    let mut native: Vec<&str> = Vec::new();
    for c in COMMANDS {
        // An application per command: no window left open by the one before.
        let (ctx, mut app) = app_for(theme, civil);
        frame(&ctx, &mut app, 2);
        let before = windows(&ctx);
        app.options_mut().run = Some(c.name.to_string());
        let result = catch_unwind(AssertUnwindSafe(|| frame(&ctx, &mut app, 4)));
        match result {
            Err(e) => {
                let why = e.downcast_ref::<String>().cloned().or_else(|| e.downcast_ref::<&str>().map(|s| s.to_string())).unwrap_or_default();
                // A command that asks the operating system for a file chooser needs a desktop
                // session (the portal's runtime): it has no window of ours to check here.
                if why.contains("no reactor running") {
                    native.push(c.name);
                } else {
                    problems.push(format!("{} ({theme}{}): panicked: {why}", c.name, if civil { ", civil demo" } else { "" }));
                }
            }
            Ok(shapes) => {
                if shapes == 0 {
                    problems.push(format!("{} ({theme}): drew nothing at all", c.name));
                }
                if windows(&ctx) <= before {
                    no_window.push(c.name);
                }
            }
        }
    }
    // Commands that open no window are fine (zoom, undo, drawing tools); they are listed so that a
    // dialog that stopped opening shows up in the run.
    eprintln!("[{theme}{}] opened no window ({}): {}", if civil { ", civil" } else { "" }, no_window.len(), no_window.join(" "));
    eprintln!("[{theme}{}] need a native file chooser ({}): {}", if civil { ", civil" } else { "" }, native.len(), native.join(" "));
    problems
}

#[test]
fn every_command_opens_in_the_dark_look() {
    let p = run_all("dark-glass", false);
    assert!(p.is_empty(), "{} problem(s):\n{}", p.len(), p.join("\n"));
}

#[test]
fn every_command_opens_in_the_light_look() {
    let p = run_all("light-glass", false);
    assert!(p.is_empty(), "{} problem(s):\n{}", p.len(), p.join("\n"));
}

#[test]
fn every_command_opens_on_the_civil_demonstration_in_the_dark_look() {
    let p = run_all("dark-glass", true);
    assert!(p.is_empty(), "{} problem(s):\n{}", p.len(), p.join("\n"));
}

#[test]
fn every_command_opens_on_the_civil_demonstration_in_the_light_look() {
    let p = run_all("light-glass", true);
    assert!(p.is_empty(), "{} problem(s):\n{}", p.len(), p.join("\n"));
}
