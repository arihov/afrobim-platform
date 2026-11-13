//! AfroBIM desktop application.

mod app;
mod canvas;
mod commands;
mod icons;
mod render;
mod signs_glue;
mod view3d;

use std::path::PathBuf;

use eframe::egui;

/// Command-line options. `--screenshot <file.png>` renders a few frames, saves the window and
/// exits (used to check the UI on machines without a desktop); `--demo` starts with sample signs.
#[derive(Default, Clone)]
pub struct Options {
    pub screenshot: Option<PathBuf>,
    pub demo: bool,
    /// Start in the 3D view.
    pub view3d: bool,
    pub open: Option<PathBuf>,
}

fn parse_args() -> Options {
    let mut o = Options::default();
    let mut args = std::env::args().skip(1);
    while let Some(a) = args.next() {
        match a.as_str() {
            "--screenshot" => o.screenshot = args.next().map(PathBuf::from),
            "--demo" => o.demo = true,
            "--3d" => o.view3d = true,
            other if !other.starts_with("--") => o.open = Some(PathBuf::from(other)),
            _ => {}
        }
    }
    o
}

fn run(options: &Options, multisampling: u16) -> eframe::Result<()> {
    let native = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_title("AfroBIM")
            .with_inner_size([1440.0, 860.0])
            .with_min_inner_size([1000.0, 600.0]),
        multisampling,
        ..Default::default()
    };
    let options = options.clone();
    eframe::run_native("AfroBIM", native, Box::new(move |cc| Ok(Box::new(app::AfroBimApp::new(cc, options)))))
}

fn main() -> eframe::Result<()> {
    let options = parse_args();
    // 4× multisampling smooths sign edges; software and remote displays may not offer it.
    match run(&options, 4) {
        Ok(()) => Ok(()),
        Err(first) => {
            eprintln!("AfroBIM: starting without multisampling ({first})");
            run(&options, 0)
        }
    }
}
