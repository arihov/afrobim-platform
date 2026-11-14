//! AfroBIM desktop application.

mod alignment_glue;
mod app;
mod canvas;
mod commands;
mod icons;
mod lighting_inputs;
mod markings_glue;
mod modules_glue;
mod render;
mod ribbon;
mod road_dialogs;
mod road_glue;
mod signs_glue;
mod station_input;
mod supports_glue;
mod view3d;

use std::path::PathBuf;

use eframe::egui;

/// Command-line options. `--screenshot <file.png>` renders a few frames, saves the window and
/// exits (used to check the UI on machines without a desktop); `--demo` starts with sample signs.
#[derive(Default, Clone)]
pub struct Options {
    pub screenshot: Option<PathBuf>,
    pub demo: bool,
    /// Ribbon tab to start on, e.g. `Roads`.
    pub tab: Option<String>,
    /// Show the ribbon preview card of this command at start-up (to check it without a pointer).
    pub preview: Option<String>,
    /// With `--demo`: size the demo supports for wind (MoWT open terrain, the dialog's starting
    /// ground values) instead of leaving placeholders, and show the first calculation.
    pub design: bool,
    /// Start in the 3D view.
    pub view3d: bool,
    pub open: Option<PathBuf>,
    /// Design standard to start with, e.g. `UG-TSM-2004`.
    pub standard: Option<String>,
    /// Export the starting model as IFC to this file, then exit (no window interaction needed).
    pub export_ifc: Option<PathBuf>,
    /// Import the alignments of this LandXML file at start-up. With `--demo`, the demo signs that
    /// have supports are then placed along the first alignment.
    pub landxml: Option<PathBuf>,
    /// Frame the 3D view on the element with this mark.
    pub focus: Option<String>,
    /// Write quantities and fabrication sheets for the starting model into this folder, then exit.
    pub export_sheets: Option<PathBuf>,
    /// Run this command once at start-up, on the element named by `--focus` when there is one
    /// (to look at a dialog or window without a pointer).
    pub run: Option<String>,
}

fn parse_args() -> Options {
    let mut o = Options::default();
    let mut args = std::env::args().skip(1);
    while let Some(a) = args.next() {
        match a.as_str() {
            "--screenshot" => o.screenshot = args.next().map(PathBuf::from),
            "--demo" => o.demo = true,
            "--tab" => o.tab = args.next(),
            "--preview" => o.preview = args.next(),
            "--design" => o.design = true,
            "--3d" => o.view3d = true,
            "--standard" => o.standard = args.next(),
            "--export-ifc" => o.export_ifc = args.next().map(PathBuf::from),
            "--landxml" => o.landxml = args.next().map(PathBuf::from),
            "--focus" => o.focus = args.next(),
            "--run" => o.run = args.next(),
            "--export-sheets" => o.export_sheets = args.next().map(PathBuf::from),
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
