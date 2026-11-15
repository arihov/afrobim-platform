//! AfroBIM desktop application.

mod alignment_glue;
mod anno_glue;
mod anno_ui;
mod app;
mod canvas;
mod check_all;
mod check_inputs;
mod check_ui;
mod commands;
mod construction_glue;
mod construction_ui;
mod coordination_glue;
mod coordination_ui;
mod cost_glue;
mod cost_ui;
mod demo_glue;
mod ext_markings_glue;
mod ext_markings_ui;
mod design_glue;
mod design_profile;
mod design_ui;
mod design_windows;
mod icons;
mod ifc_options;
mod issues_ui;
mod lighting_inputs;
mod markings_glue;
mod modules_glue;
mod photometry_ui;
mod projset;
mod reference_models;
mod report_pack;
mod render;
mod ribbon;
mod road_dialogs;
mod road_glue;
mod schedule_ext;
mod schedule_glue;
mod schedule_ui;
mod sheets_glue;
mod sheets_ui;
mod signs_glue;
mod signsched;
mod station_input;
mod supports_glue;
#[cfg(test)]
mod u1_tests;
#[cfg(test)]
mod u2_align_tests;
#[cfg(test)]
mod u2_drain_tests;
#[cfg(test)]
mod u2_pack_tests;
#[cfg(test)]
mod u2_ifc_tests;
#[cfg(test)]
mod u2_tests;
mod underlay;
mod view3d;
mod visibility_ui;
mod win;

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
    /// Show the model at this date (YYYY-MM-DD) in the 3D view, coloured by the programme.
    pub date: Option<String>,
    /// Sub-tab to show in the Bill and Estimate windows (0 is the first), to look at each without a pointer.
    pub subtab: Option<u8>,
    /// After the demo clash run, zoom to the clash with this index (from 0) in the 3D view.
    pub zoom_clash: Option<usize>,
    /// A limits table (CSV or JSON) that ALIGNCHECK loads when it opens, and the row of it to check against.
    pub limits: Option<PathBuf>,
    /// Start ALIGNDESIGN with this name and start station (`Name,1000`), and the PIs given by `--pis` (`x,y;x,y;...`); `--pis-finish` makes the alignment.
    pub new_alignment: Option<String>,
    pub pis: Option<String>,
    pub pis_finish: bool,
    pub limits_row: Option<String>,
    /// Write the 4D playback as PNG frames into this folder, one every `frames_step` days, then exit.
    /// Import this XER file, or export the programme to this XER file, at start-up (to see the reports without a file dialog).
    pub xer_in: Option<PathBuf>,
    pub xer_out: Option<PathBuf>,
    pub frames_dir: Option<PathBuf>,
    pub frames_step: Option<i64>,
    pub frames_from: Option<String>,
    pub frames_to: Option<String>,
    /// Compose a drawing sheet set and write it to this path, then exit: a `.pdf` path gets the PDF,
    /// a `.dxf` path one DXF with every sheet, anything else is a folder that gets `sheets.pdf` and
    /// one DXF per sheet.
    pub export_set: Option<PathBuf>,
    /// The sheet set to export or preview, by name; the first one when not given.
    pub sheet_set: Option<String>,
    /// The sheet to show first in the preview (from 0).
    pub sheet: Option<usize>,
    /// Open the sheet-set manager with the add form of this template (0 plan, 1 profile, 2 sections,
    /// 3 schedule, 4 detail, 5 notes), to look at it without a pointer.
    pub sheet_form: Option<usize>,
    /// Import this IFC file as a reference model at start-up: `path` or `path,dx,dy,dz,rotation`
    /// (offsets in mm, rotation in degrees), to look at the result without a file dialog.
    pub ifcref: Option<String>,
    /// Load this IES or LDT file into the photometry window at start-up.
    pub photometry: Option<PathBuf>,
    /// Write the calculation report pack here (`.pdf`, `.txt` or `.csv`; the other two variants go beside it), then exit.
    pub export_pack: Option<PathBuf>,
    /// Take a viewpoint (camera and snapshot) of the first issue from the 3D view at start-up.
    pub capture_viewpoint: bool,
    /// After a visibility run: `drawings` shows only the drawings window, `overlay` only the 3D
    /// view framed on the sign with the limiting sight line (to look at each without a pointer).
    pub vis_view: Option<String>,
    /// With `--photometry`: fill the window with arbitrary demo settings (80 percent maintenance factor, 60 m
    /// contribution distance, 3 m by 1 m cells, orientation accepted) and run, for a screenshot.
    pub photometry_run: bool,
    /// Attach this DXF as an underlay at start-up: `path` or `path,dx,dy,rotation` (mm, degrees).
    pub underlay: Option<String>,
    /// Read this sign schedule CSV at start-up and open its mapping and plan windows.
    pub signschedule: Option<PathBuf>,
    /// Write the model as DXF to this file, then exit.
    pub dxf_out: Option<PathBuf>,
    /// Layers to switch on at start-up, comma separated.
    pub layers_on: Option<String>,
    /// Frame the 2D view on `x,y,width` (millimetres: the centre and the width shown).
    pub zoom_to: Option<String>,
}

fn parse_args() -> Options {
    parse_args_from(std::env::args().skip(1))
}

/// The options from an argument list. An argument that is not an option is the document to
/// open (the Windows file association passes the file that way).
fn parse_args_from(args: impl Iterator<Item = String>) -> Options {
    let mut o = Options::default();
    let mut args = args;
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
            "--date" => o.date = args.next(),
            "--zoom-clash" => o.zoom_clash = args.next().and_then(|v| v.parse().ok()),
            "--subtab" => o.subtab = args.next().and_then(|v| v.parse().ok()),
            "--export-set" => o.export_set = args.next().map(PathBuf::from),
            "--sheet-set" => o.sheet_set = args.next(),
            "--sheet-form" => o.sheet_form = args.next().and_then(|v| v.parse().ok()),
            "--limits" => o.limits = args.next().map(PathBuf::from),
            "--new-alignment" => o.new_alignment = args.next(),
            "--pis" => o.pis = args.next(),
            "--pis-finish" => o.pis_finish = true,
            "--limits-row" => o.limits_row = args.next(),
            "--xer-in" => o.xer_in = args.next().map(PathBuf::from),
            "--xer-out" => o.xer_out = args.next().map(PathBuf::from),
            "--frames-dir" => o.frames_dir = args.next().map(PathBuf::from),
            "--frames-step" => o.frames_step = args.next().and_then(|v| v.parse().ok()),
            "--frames-from" => o.frames_from = args.next(),
            "--frames-to" => o.frames_to = args.next(),
            "--photometry-run" => o.photometry_run = true,
            "--vis-view" => o.vis_view = args.next(),
            "--capture-viewpoint" => o.capture_viewpoint = true,
            "--ifcref" => o.ifcref = args.next(),
            "--photometry" => o.photometry = args.next().map(PathBuf::from),
            "--underlay" => o.underlay = args.next(),
            "--signschedule" => o.signschedule = args.next().map(PathBuf::from),
            "--dxf-out" => o.dxf_out = args.next().map(PathBuf::from),
            "--layers-on" => o.layers_on = args.next(),
            "--zoom-to" => o.zoom_to = args.next(),
            "--export-pack" => o.export_pack = args.next().map(PathBuf::from),
            "--sheet" => o.sheet = args.next().and_then(|v| v.parse().ok()),
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

#[cfg(test)]
mod arg_tests {
    use super::*;

    #[test]
    fn a_positional_argument_is_the_document_to_open() {
        let o = parse_args_from(["road.afrobim".to_string()].into_iter());
        assert_eq!(o.open, Some(PathBuf::from("road.afrobim")));
        let o = parse_args_from(["road.afrobim", "--demo", "--tab", "Roads"].map(String::from).into_iter());
        assert_eq!(o.open, Some(PathBuf::from("road.afrobim")));
        assert!(o.demo && o.tab.as_deref() == Some("Roads"));
        // An option's value is not mistaken for a document.
        let o = parse_args_from(["--landxml", "a.xml"].map(String::from).into_iter());
        assert_eq!(o.open, None);
    }
}
