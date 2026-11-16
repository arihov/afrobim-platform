//! The menu bar and the context menus. Every item is a command from the command table, with its
//! icon, its label and, right-aligned, its keyboard shortcut (or the command to type when it has
//! none). The menus are File, Edit, View, Insert, Modify, Analyse, Tools, Window and Help.

use egui::{vec2, RichText, Ui};

use crate::anno_glue::Kind;
use crate::app::AfroBimApp;
use crate::commands::{Action, COMMANDS};
use crate::theme::{self, FONT_BODY};
use crate::{glyphs, ribbon, signs_glue};

/// What the layers list's context menu asked for.
#[derive(Clone, Copy)]
pub enum LayerPick {
    Isolate,
    ShowAll,
    UnlockAll,
    Manager,
}

/// What a menu asked for.
pub enum Pick {
    Action(Action),
    OpenFile(String),
    Workspace(usize),
    Look(crate::theme::Look),
    ToggleLeft,
    ToggleRight,
}

/// The text typed for an action: its command name.
pub fn command_name(action: Action) -> &'static str {
    COMMANDS.iter().find(|c| c.action == action).map(|c| c.name).unwrap_or("")
}

/// One menu item: icon, label, and right-aligned shortcut. Returns true when clicked (and closes the menu).
pub fn item(ui: &mut Ui, action: Action, label: &str) -> bool {
    item_on(ui, action, label, None)
}

/// A menu item that shows a check mark when `on` is Some(true).
pub fn item_on(ui: &mut Ui, action: Action, label: &str, on: Option<bool>) -> bool {
    let t = theme::tokens_ui(ui);
    let glyph = glyphs::glyph(action).unwrap_or("");
    let mut shortcut = ribbon::shortcut(action).to_string();
    if shortcut.is_empty() {
        shortcut = command_name(action).to_string();
    }
    let colour = t.text;
    let mut button = egui::Button::new(theme::icon_text(glyph, label, FONT_BODY, colour)).min_size(vec2(ui.available_width().max(260.0), 26.0));
    if !shortcut.is_empty() {
        button = button.shortcut_text(RichText::new(shortcut).size(theme::FONT_SMALL).color(t.text_muted));
    }
    if on == Some(true) {
        button = button.selected(true);
    }
    let clicked = ui.add(button).clicked();
    if clicked {
        ui.close_menu();
    }
    clicked
}

/// A menu entry that is not a command (a workspace, a recent file).
fn plain_item(ui: &mut Ui, glyph: &str, label: &str, right: &str, on: bool) -> bool {
    let t = theme::tokens_ui(ui);
    let mut button = egui::Button::new(theme::icon_text(glyph, label, FONT_BODY, t.text)).min_size(vec2(ui.available_width().max(260.0), 26.0));
    if !right.is_empty() {
        button = button.shortcut_text(RichText::new(right).size(theme::FONT_SMALL).color(t.text_muted));
    }
    if on {
        button = button.selected(true);
    }
    let clicked = ui.add(button).clicked();
    if clicked {
        ui.close_menu();
    }
    clicked
}

/// A submenu with an icon.
fn submenu(ui: &mut Ui, glyph: &str, label: &str, body: impl FnOnce(&mut Ui)) {
    let t = theme::tokens_ui(ui);
    ui.menu_button(theme::icon_text(glyph, label, FONT_BODY, t.text), |ui| {
        ui.set_min_width(280.0);
        body(ui)
    });
}

fn sep(ui: &mut Ui) {
    ui.add_space(2.0);
    ui.separator();
    ui.add_space(2.0);
}

/// A command item that records itself in `pick` when clicked.
fn a(ui: &mut Ui, pick: &mut Option<Pick>, action: Action, label: &str) {
    if item(ui, action, label) {
        *pick = Some(Pick::Action(action));
    }
}

impl AfroBimApp {
    pub(crate) fn menu_bar(&mut self, ctx: &egui::Context) {
        let mut pick: Option<Pick> = None;
        let standard = self.standard.clone();
        let recent = self.prefs.recent.clone();
        let (left, right) = (self.show_left, self.show_right);
        let workspace = self.workspace;
        let look = self.theme.look;
        egui::TopBottomPanel::top("menu").show(ctx, |ui| {
            egui::menu::bar(ui, |ui| {
                ui.menu_button("File", |ui| {
                    ui.set_min_width(280.0);
                    a(ui, &mut pick, Action::New, "New");
                    a(ui, &mut pick, Action::Open, "Open…");
                    ui.menu_button(theme::icon_text(glyphs::ui::FOLDER, "Open recent", FONT_BODY, theme::tokens_ui(ui).text), |ui| {
                        ui.set_min_width(320.0);
                        if recent.is_empty() {
                            ui.label(RichText::new("No recent files yet").color(theme::tokens_ui(ui).text_muted));
                        }
                        for (i, path) in recent.iter().enumerate() {
                            let name = std::path::Path::new(path).file_name().map(|n| n.to_string_lossy().to_string()).unwrap_or_else(|| path.clone());
                            if plain_item(ui, glyphs::ui::FOLDER, &name, &format!("{}", i + 1), false) {
                                pick = Some(Pick::OpenFile(path.clone()));
                            }
                        }
                    });
                    a(ui, &mut pick, Action::Save, "Save");
                    a(ui, &mut pick, Action::SaveAs, "Save as…");
                    sep(ui);
                    submenu(ui, egui_phosphor::regular::FILE_ARROW_DOWN, "Import", |ui| {
                        a(ui, &mut pick, Action::ImportLandXml, "LandXML alignments…");
                        a(ui, &mut pick, Action::ImportSurface, "LandXML ground surface…");
                        a(ui, &mut pick, Action::ImportSurvey, "Survey points (CSV)…");
                        a(ui, &mut pick, Action::SignSchedule, "Sign schedule from CSV…");
                        a(ui, &mut pick, Action::Underlay, "DXF underlay…");
                        a(ui, &mut pick, Action::IfcRef, "IFC reference model…");
                        a(ui, &mut pick, Action::BcfIn, "Issues from BCF…");
                        a(ui, &mut pick, Action::MspIn, "Programme from MS Project XML…");
                        a(ui, &mut pick, Action::XerIn, "Programme from Primavera XER…");
                    });
                    submenu(ui, egui_phosphor::regular::FILE_ARROW_UP, "Export", |ui| {
                        a(ui, &mut pick, Action::ExportDxf, "DXF…");
                        a(ui, &mut pick, Action::ExportIfc, "IFC 4.3…");
                        a(ui, &mut pick, Action::LandXmlOut, "Alignment as LandXML…");
                        a(ui, &mut pick, Action::ExportQuantities, "Quantities (BoQ)…");
                        a(ui, &mut pick, Action::ExportSheets, "Fabrication sheets…");
                        a(ui, &mut pick, Action::SheetPdf, "Sheet set as PDF…");
                        a(ui, &mut pick, Action::SheetDxf, "Sheet set as DXF…");
                        a(ui, &mut pick, Action::ReportPack, "Calculation report pack…");
                        a(ui, &mut pick, Action::SignScheduleOut, "Sign schedule (CSV)…");
                        a(ui, &mut pick, Action::SignScheduleTemplate, "Blank sign schedule template…");
                        a(ui, &mut pick, Action::BcfOut, "Issues as BCF…");
                        a(ui, &mut pick, Action::ClashCsv, "Clash results (CSV)…");
                        a(ui, &mut pick, Action::MspOut, "Programme as MS Project XML…");
                        a(ui, &mut pick, Action::XerOut, "Programme as Primavera XER…");
                        a(ui, &mut pick, Action::ProgCsv, "Programme reports (CSV)…");
                        a(ui, &mut pick, Action::CostCsv, "Cost reports (CSV)…");
                        a(ui, &mut pick, Action::EvmCsv, "Earned value tables (CSV)…");
                    });
                    sep(ui);
                    a(ui, &mut pick, Action::ProjectInfo, "Project information…");
                    a(ui, &mut pick, Action::ProjectSettings, "Project settings…");
                    a(ui, &mut pick, Action::CoordSystem, "Coordinate system…");
                    sep(ui);
                    a(ui, &mut pick, Action::Quit, "Quit");
                });
                ui.menu_button("Edit", |ui| {
                    ui.set_min_width(280.0);
                    a(ui, &mut pick, Action::Undo, "Undo");
                    a(ui, &mut pick, Action::Redo, "Redo");
                    sep(ui);
                    a(ui, &mut pick, Action::CopyClip, "Copy");
                    a(ui, &mut pick, Action::PasteClip, "Paste");
                    a(ui, &mut pick, Action::Erase, "Erase");
                    sep(ui);
                    a(ui, &mut pick, Action::SelectAll, "Select all");
                });
                ui.menu_button("View", |ui| {
                    ui.set_min_width(280.0);
                    a(ui, &mut pick, Action::ZoomExtents, "Zoom extents");
                    a(ui, &mut pick, Action::ZoomWindow, "Zoom window");
                    a(ui, &mut pick, Action::ZoomSelection, "Zoom to selection");
                    a(ui, &mut pick, Action::ZoomPrevious, "Zoom previous");
                    a(ui, &mut pick, Action::ZoomIn, "Zoom in");
                    a(ui, &mut pick, Action::ZoomOut, "Zoom out");
                    sep(ui);
                    if item_on(ui, Action::ToggleGrid, "Grid", Some(self.grid_on)) {
                        pick = Some(Pick::Action(Action::ToggleGrid));
                    }
                    if item_on(ui, Action::ToggleSnap, "Snap", Some(self.ctl.snap_on)) {
                        pick = Some(Pick::Action(Action::ToggleSnap));
                    }
                    if item_on(ui, Action::ToggleOrtho, "Ortho", Some(self.ctl.ortho_on)) {
                        pick = Some(Pick::Action(Action::ToggleOrtho));
                    }
                    if item_on(ui, Action::ToggleOsnap, "Object snap", Some(self.anno.osnap.on)) {
                        pick = Some(Pick::Action(Action::ToggleOsnap));
                    }
                    sep(ui);
                    a(ui, &mut pick, Action::Toggle3d, "2D model space / 3D view");
                    if item_on(ui, Action::RepReal, "Real (full colour)", Some(self.rep == crate::render::Representation::Real)) {
                        pick = Some(Pick::Action(Action::RepReal));
                    }
                    if item_on(ui, Action::RepOutlined, "Outlined", Some(self.rep == crate::render::Representation::Outlined)) {
                        pick = Some(Pick::Action(Action::RepOutlined));
                    }
                    if item_on(ui, Action::ToggleMarks, "Element marks", Some(self.show_labels)) {
                        pick = Some(Pick::Action(Action::ToggleMarks));
                    }
                    sep(ui);
                    a(ui, &mut pick, Action::ToggleCanvas, "Dark / light model space");
                });
                ui.menu_button("Insert", |ui| {
                    ui.set_min_width(300.0);
                    for (i, t) in signs_glue::templates(&standard).iter().enumerate() {
                        let act = Action::InsertSign(i as u8);
                        if item(ui, act, t.label) {
                            pick = Some(Pick::Action(act));
                        }
                    }
                    a(ui, &mut pick, Action::Library, "Standard signs…");
                    sep(ui);
                    submenu(ui, egui_phosphor::regular::ROAD_HORIZON, "Road markings", |ui| {
                        a(ui, &mut pick, Action::AddMarking, "Lines along an alignment…");
                        a(ui, &mut pick, Action::AddExtMarking(0), "Stop and give way…");
                        a(ui, &mut pick, Action::AddExtMarking(1), "Arrows…");
                        a(ui, &mut pick, Action::AddExtMarking(2), "Hatching and chevrons…");
                        a(ui, &mut pick, Action::AddExtMarking(3), "Yellow lines…");
                        a(ui, &mut pick, Action::AddExtMarking(4), "Road studs…");
                        a(ui, &mut pick, Action::AddExtMarking(5), "Worded markings…");
                        a(ui, &mut pick, Action::AddExtMarking(6), "Uganda lane and edge lines…");
                    });
                    submenu(ui, egui_phosphor::regular::PIPE, "Road elements", |ui| {
                        a(ui, &mut pick, Action::AddRoadElement(0), "Side drain…");
                        a(ui, &mut pick, Action::AddRoadElement(1), "Pipe culvert…");
                        a(ui, &mut pick, Action::AddRoadElement(2), "Portal gantry…");
                        a(ui, &mut pick, Action::AddRoadElement(3), "Cantilever gantry…");
                        a(ui, &mut pick, Action::AddLighting, "Street lighting…");
                        a(ui, &mut pick, Action::AddLightColumn, "Lighting column…");
                        a(ui, &mut pick, Action::AddBarrier, "Safety barrier…");
                        a(ui, &mut pick, Action::AddPavement, "Pavement…");
                        a(ui, &mut pick, Action::AddCorridor, "Corridor…");
                    });
                    sep(ui);
                    a(ui, &mut pick, Action::ImportSurvey, "Survey points (CSV)…");
                    a(ui, &mut pick, Action::ImportSurface, "Ground surface (LandXML)…");
                    a(ui, &mut pick, Action::Underlay, "DXF underlay…");
                    a(ui, &mut pick, Action::IfcRef, "IFC reference model…");
                });
                ui.menu_button("Civil", |ui| {
                    ui.set_min_width(280.0);
                    use crate::civil_cmds::Cv::*;
                    let c = Action::Civil;
                    a(ui, &mut pick, c(Toolspace), "Toolspace");
                    sep(ui);
                    submenu(ui, egui_phosphor::regular::DOTS_NINE, "Points", |ui| {
                        for (cv, l) in [(Points, "Import points…"), (PointCreate, "Create point…"), (PointTable, "Point table…"), (PointGroups, "Point groups…"), (DescKeys, "Description keys…"), (PointExport, "Export points…"), (Cogo, "COGO tools…"), (Traverse, "Traverse…")] {
                            a(ui, &mut pick, c(cv), l);
                        }
                    });
                    submenu(ui, egui_phosphor::regular::MOUNTAINS, "Surfaces", |ui| {
                        for (cv, l) in [(SurfaceNew, "Create surface…"), (SurfaceDef, "Surface definition…"), (SurfaceStyle, "Surface style…"), (SpotElev, "Spot elevation…"), (FlowPath, "Flow path…"), (Volumes, "Volumes…")] {
                            a(ui, &mut pick, c(cv), l);
                        }
                    });
                    submenu(ui, egui_phosphor::regular::CHART_LINE, "Profiles", |ui| {
                        for (cv, l) in [(GroundProfile, "Existing ground profile…"), (ProfileView, "Profile view…")] {
                            a(ui, &mut pick, c(cv), l);
                        }
                    });
                    submenu(ui, egui_phosphor::regular::ROAD_HORIZON, "Corridors and sections", |ui| {
                        for (cv, l) in [(Assembly, "Assembly editor…"), (CorridorModel, "Corridor…"), (SampleLines, "Sample lines…"), (SectionViews, "Section views…")] {
                            a(ui, &mut pick, c(cv), l);
                        }
                    });
                    submenu(ui, egui_phosphor::regular::POLYGON, "Parcels", |ui| {
                        for (cv, l) in [(ParcelPick, "Create parcel…"), (ParcelDeed, "From deed…"), (ParcelProps, "Parcel properties…"), (ParcelTopology, "Topology check…"), (ParcelSplit, "Split by road reserve…"), (ParcelSchedule, "Acquisition schedule…")] {
                            a(ui, &mut pick, c(cv), l);
                        }
                    });
                    submenu(ui, egui_phosphor::regular::PIPE, "Pipe networks", |ui| {
                        for (cv, l) in [(PipeNew, "Create network…"), (PipeParts, "Parts list…"), (PipeProps, "Properties and rules…"), (PipeEdit, "Edit network…"), (PipeCheck, "Checks…"), (PipeTables, "Tables…"), (PipeQty, "Quantities and trenches…"), (PipeProfile, "Profile of a run…")] {
                            a(ui, &mut pick, c(cv), l);
                        }
                    });
                    submenu(ui, egui_phosphor::regular::VECTOR_TWO, "Grading", |ui| {
                        for (cv, l) in [(FeatureLine, "Create feature line…"), (FeatureLineEdit, "Edit feature line…"), (GradeSite, "Sites and crossings…"), (GradeCreate, "Create grading…"), (GradeGroup, "Grading groups and volumes…"), (GradeHelpers, "Pad, pond, ditch…")] {
                            a(ui, &mut pick, c(cv), l);
                        }
                    });
                    submenu(ui, egui_phosphor::regular::ARROWS_SPLIT, "Intersections", |ui| {
                        for (cv, l) in [(Intersection, "Intersection…"), (Roundabout, "Roundabout…")] {
                            a(ui, &mut pick, c(cv), l);
                        }
                    });
                    submenu(ui, egui_phosphor::regular::GLOBE_HEMISPHERE_WEST, "Reprojection", |ui| {
                        for (cv, l) in [(Transform, "Transform coordinates…"), (Calibrate, "Site calibration…")] {
                            a(ui, &mut pick, c(cv), l);
                        }
                    });
                    a(ui, &mut pick, c(Earthworks), "Earthworks and mass haul…");
                    sep(ui);
                    a(ui, &mut pick, c(CivilLandXml), "Export civil data as LandXML…");
                    a(ui, &mut pick, c(Defaults), "Engineering defaults…");
                    a(ui, &mut pick, c(CivilDemo), "Demonstration data");
                });
                ui.menu_button("Modify", |ui| {
                    ui.set_min_width(300.0);
                    a(ui, &mut pick, Action::Move, "Move");
                    a(ui, &mut pick, Action::Copy, "Copy");
                    a(ui, &mut pick, Action::Erase, "Erase");
                    a(ui, &mut pick, Action::Arrange, "Arrange");
                    sep(ui);
                    a(ui, &mut pick, Action::PlaceOnAlignment, "Place the sign on an alignment…");
                    a(ui, &mut pick, Action::AddSupports, "Add supports to the sign…");
                    a(ui, &mut pick, Action::DesignSupports, "Design the sign's supports…");
                    a(ui, &mut pick, Action::RemoveSupports, "Remove the sign's supports");
                    a(ui, &mut pick, Action::HangSign, "Hang the sign on a gantry…");
                    a(ui, &mut pick, Action::UnhangSign, "Take the sign off its gantry");
                    sep(ui);
                    a(ui, &mut pick, Action::AlignDesign, "Design an alignment…");
                    a(ui, &mut pick, Action::AlignEdit, "Edit the alignment in plan…");
                    a(ui, &mut pick, Action::ProfileDesign, "Profile editor…");
                    a(ui, &mut pick, Action::TextEdit, "Edit text…");
                });
                ui.menu_button("Analyse", |ui| {
                    ui.set_min_width(300.0);
                    a(ui, &mut pick, Action::CheckAll, "Check the whole model…");
                    a(ui, &mut pick, Action::CheckElement, "Check the selected element…");
                    a(ui, &mut pick, Action::AlignCheck, "Alignment design checks…");
                    a(ui, &mut pick, Action::PavementDesign, "Pavement design…");
                    a(ui, &mut pick, Action::Rational, "Rational method flow…");
                    a(ui, &mut pick, Action::Visibility, "Sign visibility…");
                    a(ui, &mut pick, Action::VisibilityRun, "Run the sign visibility checks");
                    a(ui, &mut pick, Action::Photometry, "Photometry…");
                    a(ui, &mut pick, Action::ShowSections, "Corridor cross-sections…");
                    sep(ui);
                    submenu(ui, egui_phosphor::regular::CUBE_FOCUS, "Coordination", |ui| {
                        a(ui, &mut pick, Action::ClashTests, "Clash tests…");
                        a(ui, &mut pick, Action::ClashRun, "Run the clash tests");
                        a(ui, &mut pick, Action::ClashResults, "Clash results…");
                        a(ui, &mut pick, Action::ClashMarkers, "Show or hide clash markers");
                        a(ui, &mut pick, Action::Issues, "Issues…");
                        a(ui, &mut pick, Action::RefModels, "Reference models…");
                    });
                    a(ui, &mut pick, Action::Options, "Alignment options…");
                    a(ui, &mut pick, Action::ReportPack, "Calculation report pack…");
                });
                ui.menu_button("Tools", |ui| {
                    ui.set_min_width(300.0);
                    a(ui, &mut pick, Action::Layers, "Layer manager…");
                    a(ui, &mut pick, Action::Units, "Units…");
                    a(ui, &mut pick, Action::OsnapSettings, "Object snap settings…");
                    a(ui, &mut pick, Action::TextStyles, "Text styles…");
                    a(ui, &mut pick, Action::DimStyles, "Dimension styles…");
                    sep(ui);
                    submenu(ui, egui_phosphor::regular::TEXT_T, "Annotate", |ui| {
                        a(ui, &mut pick, Action::Anno(Kind::Text), "Text");
                        a(ui, &mut pick, Action::Anno(Kind::MText), "Multiline text");
                        a(ui, &mut pick, Action::Anno(Kind::Dim), "Dimension");
                        a(ui, &mut pick, Action::Anno(Kind::DimLinear), "Linear dimension");
                        a(ui, &mut pick, Action::Anno(Kind::DimAligned), "Aligned dimension");
                        a(ui, &mut pick, Action::Anno(Kind::DimAngular), "Angular dimension");
                        a(ui, &mut pick, Action::Anno(Kind::DimRadius), "Radius dimension");
                        a(ui, &mut pick, Action::Anno(Kind::DimDiameter), "Diameter dimension");
                        a(ui, &mut pick, Action::Anno(Kind::DimArc), "Arc length");
                        a(ui, &mut pick, Action::Anno(Kind::DimContinue), "Continue dimension");
                        a(ui, &mut pick, Action::Anno(Kind::DimBaseline), "Baseline dimension");
                        a(ui, &mut pick, Action::Anno(Kind::DimStation), "Station and offset");
                        a(ui, &mut pick, Action::Anno(Kind::DimLevel), "Level");
                        a(ui, &mut pick, Action::Anno(Kind::Leader), "Leader");
                        a(ui, &mut pick, Action::Anno(Kind::MLeader), "Mark leader");
                    });
                    submenu(ui, egui_phosphor::regular::RULER, "Measure", |ui| {
                        a(ui, &mut pick, Action::Anno(Kind::Dist), "Distance");
                        a(ui, &mut pick, Action::Anno(Kind::MeasureAngle), "Angle");
                        a(ui, &mut pick, Action::Anno(Kind::MeasureRadius), "Radius of three points");
                        a(ui, &mut pick, Action::Anno(Kind::Area), "Area");
                        a(ui, &mut pick, Action::Anno(Kind::Id), "ID point");
                        a(ui, &mut pick, Action::MeasureResults, "Measure results…");
                    });
                    submenu(ui, egui_phosphor::regular::CLOCK_COUNTDOWN, "Programme (4D)", |ui| {
                        a(ui, &mut pick, Action::Programme, "Programme: activities and logic…");
                        a(ui, &mut pick, Action::Wbs, "Work breakdown structure…");
                        a(ui, &mut pick, Action::Calendar, "Programme calendar…");
                        a(ui, &mut pick, Action::ScheduleRun, "Schedule (critical path)");
                        a(ui, &mut pick, Action::CriticalBasis, "Critical path basis…");
                        a(ui, &mut pick, Action::Gantt, "Gantt chart");
                        a(ui, &mut pick, Action::TimeLocation, "Time-location chart");
                        a(ui, &mut pick, Action::SetBaseline, "Set baseline…");
                        a(ui, &mut pick, Action::Variance, "Variance against the baseline");
                        a(ui, &mut pick, Action::Progress, "Progress…");
                        a(ui, &mut pick, Action::Link4d, "Link elements to activities…");
                        a(ui, &mut pick, Action::LinkReport, "Link report");
                        a(ui, &mut pick, Action::FourD, "4D date…");
                        a(ui, &mut pick, Action::Play4d, "Play the programme");
                    });
                    submenu(ui, egui_phosphor::regular::COINS, "Cost (5D)", |ui| {
                        a(ui, &mut pick, Action::Bill, "Bill of quantities…");
                        a(ui, &mut pick, Action::Rates, "Rate library…");
                        a(ui, &mut pick, Action::Currency, "Currency…");
                        a(ui, &mut pick, Action::Estimate, "Estimate…");
                        a(ui, &mut pick, Action::CashFlow, "Cash flow…");
                        a(ui, &mut pick, Action::Certificate, "Interim payment certificate…");
                    });
                    submenu(ui, egui_phosphor::regular::NOTE_PENCIL, "Construction", |ui| {
                        a(ui, &mut pick, Action::SiteRecord, "Record site progress…");
                        a(ui, &mut pick, Action::SiteLog, "Site records…");
                        a(ui, &mut pick, Action::RollUp, "Roll up site progress and update the programme…");
                        a(ui, &mut pick, Action::LookAhead, "Look-ahead and delays…");
                        a(ui, &mut pick, Action::Evm, "Earned value…");
                        a(ui, &mut pick, Action::SiteView, "Colour the 3D view by progress");
                    });
                    submenu(ui, egui_phosphor::regular::FILES, "Sheets", |ui| {
                        a(ui, &mut pick, Action::SheetSets, "Sheet sets…");
                        a(ui, &mut pick, Action::SheetPreview, "Sheet preview…");
                        a(ui, &mut pick, Action::TitleBlock, "Title block…");
                    });
                    sep(ui);
                    submenu(ui, egui_phosphor::regular::BOOK_OPEN_TEXT, "Design standard", |ui| {
                        for (i, (id, label, _)) in signs_glue::STANDARDS.iter().enumerate() {
                            let act = Action::Standard(i as u8);
                            if item_on(ui, act, label, Some(*id == standard)) {
                                pick = Some(Pick::Action(act));
                            }
                        }
                    });
                });
                ui.menu_button("Window", |ui| {
                    ui.set_min_width(300.0);
                    submenu(ui, glyphs::ui::WORKSPACE, "Workspace", |ui| {
                        for (i, w) in crate::workspace::WORKSPACES.iter().enumerate() {
                            if plain_item(ui, w.glyph, w.name, "", i == workspace) {
                                pick = Some(Pick::Workspace(i));
                            }
                        }
                    });
                    submenu(ui, egui_phosphor::regular::SIDEBAR_SIMPLE, "Panels", |ui| {
                        if plain_item(ui, egui_phosphor::regular::SIDEBAR_SIMPLE, "Tool sets and model tree", "", left) {
                            pick = Some(Pick::ToggleLeft);
                        }
                        if plain_item(ui, egui_phosphor::regular::SIDEBAR, "Properties and layers", "", right) {
                            pick = Some(Pick::ToggleRight);
                        }
                    });
                    submenu(ui, egui_phosphor::regular::PALETTE, "Look", |ui| {
                        for l in crate::theme::Look::ALL {
                            if plain_item(ui, egui_phosphor::regular::CIRCLE_HALF, l.label(), "", l == look) {
                                pick = Some(Pick::Look(l));
                            }
                        }
                    });
                    a(ui, &mut pick, Action::Appearance, "Appearance…");
                    a(ui, &mut pick, Action::ToggleTheme, "Dark / light");
                    sep(ui);
                    a(ui, &mut pick, Action::Palette, "Command palette…");
                });
                ui.menu_button("Help", |ui| {
                    ui.set_min_width(280.0);
                    a(ui, &mut pick, Action::Palette, "Search commands…");
                    a(ui, &mut pick, Action::Help, "Command list");
                    a(ui, &mut pick, Action::About, "About AfroBIM");
                });
            });
        });
        match pick {
            Some(Pick::Action(a)) => self.run(ctx, a),
            Some(Pick::OpenFile(p)) => self.open_path(std::path::PathBuf::from(p)),
            Some(Pick::Workspace(i)) => self.set_workspace(i),
            Some(Pick::Look(l)) => {
                let mut t = self.theme;
                t.look = l;
                self.set_theme(ctx, t);
            }
            Some(Pick::ToggleLeft) => self.show_left = !self.show_left,
            Some(Pick::ToggleRight) => self.show_right = !self.show_right,
            None => {}
        }
    }

    /// The right-click menu of the drawing canvas.
    pub(crate) fn canvas_context_menu(&mut self, ui: &mut Ui) -> Option<Action> {
        let mut pick = None;
        ui.set_min_width(240.0);
        let has_selection = !self.doc.selection().is_empty();
        let t = theme::tokens_ui(ui);
        if has_selection {
            ui.label(RichText::new(format!("{} selected", self.doc.selection().len())).size(theme::FONT_SMALL).color(t.text_muted));
            for (a, l) in [(Action::Move, "Move"), (Action::Copy, "Copy"), (Action::Erase, "Erase"), (Action::CopyClip, "Copy to clipboard")] {
                if item(ui, a, l) {
                    pick = Some(a);
                }
            }
            for (a, l) in [(Action::PlaceOnAlignment, "Place on an alignment…"), (Action::AddSupports, "Add supports…"), (Action::DesignSupports, "Design supports…"), (Action::CheckElement, "Check…")] {
                if item(ui, a, l) {
                    pick = Some(a);
                }
            }
            sep(ui);
        }
        for (a, l) in [(Action::PasteClip, "Paste"), (Action::SelectAll, "Select all"), (Action::Undo, "Undo"), (Action::Redo, "Redo")] {
            if item(ui, a, l) {
                pick = Some(a);
            }
        }
        sep(ui);
        for (a, l) in [(Action::ZoomExtents, "Zoom extents"), (Action::ZoomWindow, "Zoom window"), (Action::ZoomSelection, "Zoom to selection"), (Action::ZoomPrevious, "Zoom previous"), (Action::Toggle3d, "2D / 3D")] {
            if item(ui, a, l) {
                pick = Some(a);
            }
        }
        sep(ui);
        if item(ui, Action::Palette, "Command palette…") {
            pick = Some(Action::Palette);
        }
        pick
    }

    /// The right-click menu of a row in the layers list. It only says what was chosen;
    /// `apply_layer_pick` does it (the list is being drawn from the document).
    pub(crate) fn layer_context_menu(ui: &mut Ui, layer: &str) -> Option<LayerPick> {
        let mut pick = None;
        let t = theme::tokens_ui(ui);
        ui.set_min_width(220.0);
        if plain_item(ui, glyphs::ui::EYE, "Show only this layer", "", false) {
            pick = Some(LayerPick::Isolate);
        }
        if plain_item(ui, glyphs::ui::EYE, "Show all layers", "", false) {
            pick = Some(LayerPick::ShowAll);
        }
        if plain_item(ui, glyphs::ui::UNLOCK, "Unlock all layers", "", false) {
            pick = Some(LayerPick::UnlockAll);
        }
        sep(ui);
        ui.label(RichText::new(format!("Layer {layer}")).size(theme::FONT_SMALL).color(t.text_muted));
        if item(ui, Action::Layers, "Layer manager…") {
            pick = Some(LayerPick::Manager);
        }
        pick
    }

    pub(crate) fn apply_layer_pick(&mut self, ctx: &egui::Context, pick: LayerPick, layer: &str) {
        match pick {
            LayerPick::Isolate => {
                for l in self.doc.layers_mut().iter_mut() {
                    l.visible = l.name == layer || crate::anno_ui::hidden_layer(&l.name);
                }
            }
            LayerPick::ShowAll => {
                for l in self.doc.layers_mut().iter_mut() {
                    l.visible = true;
                }
            }
            LayerPick::UnlockAll => {
                for l in self.doc.layers_mut().iter_mut() {
                    l.locked = false;
                }
            }
            LayerPick::Manager => self.run(ctx, Action::Layers),
        }
    }

    /// The right-click menu of the properties panel.
    pub(crate) fn properties_context_menu(&mut self, ui: &mut Ui) -> Option<Action> {
        let mut pick = None;
        ui.set_min_width(220.0);
        for (a, l) in [(Action::CopyClip, "Copy element"), (Action::PasteClip, "Paste"), (Action::Erase, "Erase"), (Action::SelectAll, "Select all"), (Action::Undo, "Undo"), (Action::Redo, "Redo")] {
            if item(ui, a, l) {
                pick = Some(a);
            }
        }
        sep(ui);
        for (a, l) in [(Action::Layers, "Layer manager…"), (Action::Units, "Units…"), (Action::ProjectSettings, "Project settings…"), (Action::CoordSystem, "Coordinate system…")] {
            if item(ui, a, l) {
                pick = Some(a);
            }
        }
        pick
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_command_has_a_name_to_show_when_it_has_no_shortcut() {
        for c in COMMANDS {
            assert_eq!(command_name(c.action) == "", false, "{}", c.name);
        }
    }
}
