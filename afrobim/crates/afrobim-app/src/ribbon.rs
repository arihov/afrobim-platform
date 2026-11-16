//! The ribbon: tabs of panels of icon buttons, in the manner of AutoCAD and Civil 3D.
//!
//! Every button runs a command from `commands::COMMANDS`, so the ribbon, the menus, the palette
//! and the command line cannot drift apart. Hovering a button shows a preview card: what the
//! command does, the name to type and its shortcut. Tools that are not built yet are shown
//! disabled and say so, so the ribbon is also an honest map of what the platform covers.

use egui::{pos2, Align2, Color32, FontId, Rect, RichText, Sense, Ui, Vec2};

use crate::commands::{self, Action, CommandInfo};
use crate::icons::{self, Icon};
use crate::signs_glue;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Tab {
    Home,
    Edit,
    Insert,
    Annotate,
    Modify,
    Signs,
    Roads,
    Design,
    Structures,
    Coordination,
    FourD,
    FiveD,
    Construction,
    Output,
    Manage,
    View,
    Help,
}

pub const TABS: [(Tab, &str); 17] = [
    (Tab::Home, "Home"),
    (Tab::Edit, "Edit"),
    (Tab::Insert, "Insert"),
    (Tab::Annotate, "Annotate"),
    (Tab::Modify, "Modify"),
    (Tab::Signs, "Signs"),
    (Tab::Roads, "Roads"),
    (Tab::Design, "Design"),
    (Tab::Structures, "Structures"),
    (Tab::Coordination, "Coordination"),
    (Tab::FourD, "4D"),
    (Tab::FiveD, "5D"),
    (Tab::Construction, "Construction"),
    (Tab::Output, "Output"),
    (Tab::Manage, "Manage"),
    (Tab::View, "View"),
    (Tab::Help, "Help"),
];

/// One ribbon button.
#[derive(Clone, Copy)]
enum Item {
    /// A large button for the command with this name.
    Big(Icon, &'static str, &'static str),
    /// A small button (stacked three high) for the command with this name.
    Small(Icon, &'static str, &'static str),
    /// A tool that is not built yet: label and one line on what it will do.
    Planned(Icon, &'static str, &'static str),
}

struct Panel {
    title: &'static str,
    items: &'static [Item],
}

use Item::{Big, Planned, Small};

const FILE: Panel = Panel {
    title: "File",
    items: &[Big(Icon::New, "New", "NEW"), Big(Icon::Open, "Open", "OPEN"), Big(Icon::Save, "Save", "SAVE"), Small(Icon::SaveAs, "Save as", "SAVEAS"), Small(Icon::Undo, "Undo", "UNDO"), Small(Icon::Redo, "Redo", "REDO")],
};
const MODIFY: Panel = Panel {
    title: "Modify",
    items: &[Big(Icon::Move, "Move", "MOVE"), Big(Icon::Copy, "Copy", "COPY"), Big(Icon::Erase, "Erase", "ERASE"), Big(Icon::Arrange, "Arrange", "ARRANGE")],
};
const NAVIGATE: Panel = Panel {
    title: "Navigate",
    items: &[Big(Icon::ZoomExtents, "Zoom extents", "ZOOM"), Big(Icon::ZoomWindow, "Zoom window", "ZOOMWINDOW"), Big(Icon::ZoomSelection, "Zoom selection", "ZS"), Big(Icon::View3d, "2D / 3D", "3D")],
};
const SUPPORTS: Panel = Panel {
    title: "Supports",
    items: &[Big(Icon::Supports, "Add supports", "SUPPORTS"), Big(Icon::Design, "Design supports", "DESIGN"), Big(Icon::NoSupports, "Remove supports", "NOSUPPORTS")],
};
const ALIGNMENT: Panel = Panel {
    title: "Alignment",
    items: &[Big(Icon::Alignment, "Import LandXML", "LANDXML"), Big(Icon::Station, "Place by station", "STATION")],
};
const EXPORT: Panel = Panel {
    title: "Export",
    items: &[Big(Icon::Dxf, "DXF", "DXFOUT"), Big(Icon::Ifc, "IFC 4.3", "IFCOUT"), Big(Icon::Quantities, "Quantities", "QUANTITIES"), Big(Icon::Sheets, "Fabrication sheets", "SHEETS")],
};
const STANDARD: Panel = Panel { title: "Design standard", items: &[Big(Icon::Standard, "UK TSM", "UK"), Big(Icon::Standard, "Uganda TSM", "UGANDA")] };
const DRAFTING: Panel = Panel { title: "Drafting aids", items: &[Small(Icon::Grid, "Grid", "GRID"), Small(Icon::Snap, "Snap", "SNAP"), Small(Icon::Ortho, "Ortho", "ORTHO")] };

fn panels(tab: Tab) -> &'static [Panel] {
    const HOME: &[Panel] = &[
        FILE,
        Panel { title: "Signs", items: &[Big(Icon::Stack, "Direction sign", "STACK"), Big(Icon::Library, "Standard signs", "LIBRARY")] },
        MODIFY,
        Panel { title: "Supports", items: &[Big(Icon::Supports, "Add supports", "SUPPORTS"), Big(Icon::Design, "Design supports", "DESIGN")] },
        Panel { title: "Alignment", items: &[Big(Icon::Station, "Place by station", "STATION")] },
        NAVIGATE,
        DRAFTING,
    ];
    const EDIT: &[Panel] = &[
        Panel { title: "History", items: &[Big(Icon::Undo, "Undo", "UNDO"), Big(Icon::Redo, "Redo", "REDO")] },
        Panel { title: "Clipboard", items: &[Big(Icon::Copy, "Copy", "COPYCLIP"), Big(Icon::Paste, "Paste", "PASTECLIP")] },
        Panel { title: "Selection", items: &[Big(Icon::SelectAll, "Select all", "SELECTALL"), Big(Icon::Erase, "Erase", "ERASE")] },
    ];
    const INSERT: &[Panel] = &[
        Panel { title: "Signs", items: &[Big(Icon::Library, "Standard signs", "LIBRARY")] },
        Panel { title: "Import", items: &[Big(Icon::Alignment, "LandXML alignments", "LANDXML"), Big(Icon::Open, "AfroBIM document", "OPEN"), Big(Icon::Schedule, "Sign schedule", "SIGNSCHEDULE")] },
        Panel { title: "Reference", items: &[Big(Icon::Dxf, "DXF underlay", "UNDERLAY"), Big(Icon::Surface, "Survey surface", "SURFACE"), Big(Icon::Surface, "Survey points", "SURVEY"), Big(Icon::RefModel, "IFC reference model", "IFCREF"), Big(Icon::RefList, "Reference models", "REFMODELS")] },
    ];
    const ANNOTATE: &[Panel] = &[
        Panel { title: "Marks", items: &[Big(Icon::Marks, "Element marks", "MARKS")] },
        Panel { title: "Text", items: &[Big(Icon::Text, "Text", "TEXT"), Big(Icon::MText, "Multiline", "MTEXT"), Big(Icon::TextEdit, "Edit text", "TEXTEDIT")] },
        Panel {
            title: "Dimensions",
            items: &[
                Big(Icon::Dimension, "Dimension", "DIM"),
                Small(Icon::Dimension, "Linear", "DIMLINEAR"),
                Small(Icon::DimAligned, "Aligned", "DIMALIGNED"),
                Small(Icon::DimAngular, "Angular", "DIMANGULAR"),
                Small(Icon::DimRadius, "Radius", "DIMRADIUS"),
                Small(Icon::DimDiameter, "Diameter", "DIMDIAMETER"),
                Small(Icon::DimArc, "Arc length", "DIMARC"),
                Small(Icon::DimContinue, "Continue", "DIMCONTINUE"),
                Small(Icon::DimBaseline, "Baseline", "DIMBASELINE"),
                Big(Icon::DimStation, "Station", "DIMSTATION"),
                Big(Icon::DimLevel, "Level", "DIMLEVEL"),
            ],
        },
        Panel { title: "Leaders", items: &[Big(Icon::Leader, "Leader", "LEADER"), Big(Icon::MLeader, "Mark leader", "MLEADER")] },
        Panel {
            title: "Measure",
            items: &[
                Big(Icon::Measure, "Distance", "DIST"),
                Small(Icon::MeasureAngle, "Angle", "MEASUREANGLE"),
                Small(Icon::MeasureRadius, "Radius", "MEASURERADIUS"),
                Small(Icon::Area, "Area", "AREA"),
                Big(Icon::IdPoint, "ID point", "ID"),
                Big(Icon::ResultsList, "Results", "MEASURERESULTS"),
            ],
        },
        Panel { title: "Styles", items: &[Big(Icon::StyleText, "Text styles", "STYLE"), Big(Icon::StyleDim, "Dimension styles", "DIMSTYLE")] },
        Panel { title: "Snap", items: &[Big(Icon::Osnap, "Object snap", "OSNAP"), Big(Icon::Options, "Snap settings", "OSNAPSETTINGS")] },
    ];
    const MODIFY_TAB: &[Panel] = &[MODIFY, SUPPORTS, Panel { title: "Alignment", items: &[Big(Icon::Station, "Place by station", "STATION")] }, DRAFTING];
    const ROADS: &[Panel] = &[
        ALIGNMENT,
        Panel {
            title: "Markings",
            items: &[
                Big(Icon::Markings, "Road markings", "MARKINGS"),
                Big(Icon::StopLine, "Stop and give way", "MARKTRANSVERSE"),
                Big(Icon::Arrow, "Arrows", "MARKARROW"),
                Big(Icon::Hatch, "Hatching, chevrons", "MARKHATCH"),
                Big(Icon::YellowLine, "Yellow lines", "MARKYELLOW"),
                Big(Icon::Studs, "Road studs", "MARKSTUDS"),
                Big(Icon::WordedMark, "Worded", "MARKWORD"),
                Big(Icon::Markings, "Uganda lines", "MARKLINES2"),
            ],
        },
        Panel { title: "Drainage", items: &[Big(Icon::Drainage, "Side drain", "DRAIN"), Big(Icon::Drainage, "Pipe culvert", "CULVERT"), Big(Icon::Rational, "Rational flow", "RATIONAL"), Big(Icon::Design, "Check", "CHECK")] },
        Panel { title: "Lighting", items: &[Big(Icon::Lighting, "Street lighting", "LIGHTING"), Big(Icon::Lighting, "Light column", "LIGHTCOLUMN"), Big(Icon::Photometry, "Photometry", "PHOTOMETRY")] },
        Panel { title: "Restraint", items: &[Big(Icon::Barrier, "Road restraint", "BARRIER")] },
        Panel { title: "Pavement", items: &[Big(Icon::Pavement, "Pavement", "PAVEMENT"), Big(Icon::Design, "Pavement design", "PAVEDESIGN")] },
        Panel { title: "Earthworks", items: &[Big(Icon::Surface, "Survey surface", "SURFACE"), Big(Icon::Surface, "Survey points", "SURVEY"), Big(Icon::Corridor, "Corridor", "CORRIDOR"), Big(Icon::Section, "Sections", "SECTIONS")] },
    ];
    const DESIGN: &[Panel] = &[
        Panel { title: "Alignment", items: &[Big(Icon::AlignDesign, "New alignment", "ALIGNDESIGN"), Big(Icon::AlignEdit, "Edit in plan", "ALIGNEDIT"), Big(Icon::PiTable, "PI table", "PITABLE"), Big(Icon::Labels, "Curve labels", "ALIGNLABELS")] },
        Panel { title: "Profile", items: &[Big(Icon::ProfileDesign, "Profile editor", "PROFILEDESIGN")] },
        Panel { title: "Checks", items: &[Big(Icon::AlignCheck, "Design checks", "ALIGNCHECK"), Big(Icon::CheckAll, "Check the model", "CHECKALL"), Big(Icon::Visibility, "Sign visibility", "VISIBILITY"), Big(Icon::Visibility, "Run visibility", "VISIBILITYRUN")] },
        Panel { title: "Options", items: &[Big(Icon::Options, "Options", "OPTIONS")] },
        Panel {
            title: "Exchange",
            items: &[
                Big(Icon::LandXmlOut, "LandXML out", "LANDXMLOUT"),
                Big(Icon::Alignment, "Import LandXML", "LANDXML"),
                Planned(Icon::Alignment, "Make an imported alignment editable", "Not available: the alignment engine holds an imported alignment as elements (lines, arcs, spirals), not as PIs with radii, and there is no conversion that gives back a faithful PI design"),
            ],
        },
    ];
    const STRUCTURES: &[Panel] = &[SUPPORTS, Panel { title: "Gantries", items: &[Big(Icon::Barrier, "Portal gantry", "GANTRY"), Big(Icon::Barrier, "Cantilever gantry", "CANTILEVER"), Big(Icon::Station, "Hang sign", "HANG"), Small(Icon::NoSupports, "Take sign off", "UNHANG"), Small(Icon::Design, "Check", "CHECK")] }];
    const COORDINATION: &[Panel] = &[
        Panel { title: "Tests", items: &[Big(Icon::ClashTests, "Clash tests", "CLASHTESTS")] },
        Panel { title: "Run", items: &[Big(Icon::Clash, "Run tests", "CLASH"), Big(Icon::ClashList, "Results", "CLASHES")] },
        Panel { title: "Display", items: &[Big(Icon::Marker, "Clash markers", "CLASHMARKERS")] },
        Panel { title: "Report", items: &[Big(Icon::Csv, "Export clashes", "CLASHCSV")] },
        Panel { title: "Issues", items: &[Big(Icon::Issue, "Issues", "ISSUES"), Big(Icon::BcfOut, "BCF export", "BCFOUT"), Big(Icon::BcfIn, "BCF import", "BCFIN")] },
        Panel { title: "Reference models", items: &[Big(Icon::RefModel, "IFC reference", "IFCREF"), Big(Icon::RefList, "Models", "REFMODELS")] },
    ];
    const FOURD: &[Panel] = &[
        Panel { title: "Programme", items: &[Big(Icon::Activities, "Activities", "PROGRAMME"), Big(Icon::Wbs, "WBS", "WBS"), Big(Icon::Calendar, "Calendar", "CALENDAR"), Big(Icon::Schedule, "Schedule", "SCHEDULE"), Big(Icon::CriticalPath, "Critical basis", "CRITICAL")] },
        Panel { title: "Chart", items: &[Big(Icon::Gantt, "Gantt chart", "GANTT"), Big(Icon::TimeLocation, "Time-location", "TIMELOCATION")] },
        Panel { title: "Baseline", items: &[Big(Icon::Baseline, "Set baseline", "BASELINE"), Big(Icon::Variance, "Variance", "VARIANCE")] },
        Panel { title: "Progress", items: &[Big(Icon::Progress, "Progress", "PROGRESS")] },
        Panel { title: "Links", items: &[Big(Icon::Link, "Link elements", "LINK4D"), Big(Icon::LinkReport, "Link report", "LINKREPORT")] },
        Panel { title: "Date", items: &[Big(Icon::DateSlider, "4D date", "FOURD"), Big(Icon::Play, "Play", "PLAY4D")] },
        Panel { title: "MS Project", items: &[Big(Icon::MspIn, "Import", "MSPIN"), Big(Icon::MspOut, "Export", "MSPOUT"), Big(Icon::Csv, "Reports", "PROGCSV")] },
        Panel { title: "Primavera P6", items: &[Big(Icon::XerIn, "Import XER", "XERIN"), Big(Icon::XerOut, "Export XER", "XEROUT")] },
    ];
    const FIVED: &[Panel] = &[
        Panel { title: "Bill", items: &[Big(Icon::Bill, "Bill and rules", "BILL")] },
        Panel { title: "Rates", items: &[Big(Icon::Rates, "Rate library", "RATES"), Big(Icon::Currency, "Currency", "CURRENCY")] },
        Panel { title: "Estimate", items: &[Big(Icon::Estimate, "Estimate", "ESTIMATE")] },
        Panel { title: "Time", items: &[Big(Icon::CashFlow, "Cash flow", "CASHFLOW")] },
        Panel { title: "Payment", items: &[Big(Icon::Certificate, "Interim certificate", "CERTIFICATE")] },
        Panel { title: "Export", items: &[Big(Icon::Csv, "Cost reports", "COSTCSV")] },
    ];
    const CONSTRUCTION: &[Panel] = &[
        Panel { title: "Site records", items: &[Big(Icon::SiteRecord, "Record progress", "SITEREC"), Big(Icon::SiteLog, "Records", "SITELOG")] },
        Panel { title: "Programme", items: &[Big(Icon::Rollup, "Roll up and update", "ROLLUP"), Big(Icon::LookAhead, "Look-ahead and delays", "LOOKAHEAD")] },
        Panel { title: "Earned value", items: &[Big(Icon::EarnedValue, "Earned value", "EVM"), Big(Icon::Csv, "Earned value CSV", "EVMCSV")] },
        Panel { title: "Display", items: &[Big(Icon::SiteColour, "Progress colour", "SITEVIEW"), Big(Icon::DateSlider, "4D date", "FOURD")] },
        Panel { title: "Payment", items: &[Big(Icon::Certificate, "Interim certificate", "CERTIFICATE")] },
    ];
    const OUTPUT: &[Panel] = &[
        EXPORT,
        Panel {
            title: "Sheets",
            items: &[Big(Icon::SheetSet, "Sheet sets", "SHEETSET"), Big(Icon::Real, "Preview", "SHEETPREVIEW"), Big(Icon::TitleBlock, "Title block", "TITLEBLOCK"), Big(Icon::Pdf, "Sheets PDF", "SHEETPDF"), Big(Icon::Dxf, "Sheets DXF", "SHEETDXF")],
        },
        Panel { title: "Reports", items: &[Big(Icon::ReportPack, "Report pack", "REPORTPACK")] },
        Panel { title: "Sign schedule", items: &[Big(Icon::ScheduleOut, "Schedule out", "SIGNSCHEDULEOUT"), Big(Icon::Template, "Blank template", "SIGNSCHEDULETEMPLATE")] },
        Panel { title: "Document", items: &[Big(Icon::Save, "Save", "SAVE"), Big(Icon::SaveAs, "Save as", "SAVEAS"), Big(Icon::Quit, "Quit", "QUIT")] }];
    const MANAGE: &[Panel] = &[
        STANDARD,
        Panel { title: "Analysis", items: &[Big(Icon::CheckAll, "Check the model", "CHECKALL")] },
        Panel {
            title: "Project",
            items: &[
                Big(Icon::Project, "Project information", "PROJECTINFO"),
                Big(Icon::Layers, "Layer manager", "LAYER"),
                Big(Icon::Units, "Units", "UNITS"),
                Big(Icon::ProjectSettings, "Project settings", "PROJECTSETTINGS"),
                Big(Icon::Crs, "Coordinate system", "CRS"),
            ],
        },
        Panel { title: "Appearance", items: &[Big(Icon::Theme, "Theme", "THEME"), Big(Icon::Canvas, "Canvas colour", "CANVAS"), Big(Icon::Theme, "Look and scale", "APPEARANCE")] },
    ];
    const VIEW: &[Panel] = &[
        Panel { title: "Zoom", items: &[Big(Icon::ZoomExtents, "Extents", "ZOOM"), Big(Icon::ZoomWindow, "Window", "ZOOMWINDOW"), Big(Icon::ZoomSelection, "Selection", "ZS"), Big(Icon::ZoomPrevious, "Previous", "ZP"), Big(Icon::ZoomIn, "In", "ZOOMIN"), Big(Icon::ZoomOut, "Out", "ZOOMOUT")] },
        Panel { title: "Views", items: &[Big(Icon::View3d, "2D / 3D", "3D")] },
        Panel { title: "Display", items: &[Big(Icon::Real, "Real", "REAL"), Big(Icon::Outlined, "Outlined", "OUTLINED"), Big(Icon::Marks, "Element marks", "MARKS")] },
        DRAFTING,
        Panel { title: "Appearance", items: &[Big(Icon::Theme, "Theme", "THEME"), Big(Icon::Canvas, "Canvas colour", "CANVAS"), Big(Icon::Theme, "Look and scale", "APPEARANCE")] },
    ];
    const HELP: &[Panel] = &[Panel { title: "Help", items: &[Big(Icon::Help, "Command list", "HELP"), Big(Icon::Help, "Command palette", "PALETTE"), Big(Icon::About, "About AfroBIM", "ABOUT")] }];
    match tab {
        Tab::Home => HOME,
        Tab::Edit => EDIT,
        Tab::Insert => INSERT,
        Tab::Annotate => ANNOTATE,
        Tab::Modify => MODIFY_TAB,
        Tab::Signs => &[], // built from the active standard's templates, see `show`
        Tab::Roads => ROADS,
        Tab::Design => DESIGN,
        Tab::Structures => STRUCTURES,
        Tab::Coordination => COORDINATION,
        Tab::FourD => FOURD,
        Tab::FiveD => FIVED,
        Tab::Construction => CONSTRUCTION,
        Tab::Output => OUTPUT,
        Tab::Manage => MANAGE,
        Tab::View => VIEW,
        Tab::Help => HELP,
    }
}

/// The titles of the panels of a tab, for the workspaces to name them.
#[allow(dead_code)] // used by the workspace tests
pub fn panel_titles(tab: Tab) -> Vec<&'static str> {
    if tab == Tab::Signs {
        return vec!["Direction signs", "Map-type and temporary", "Catalogue", "Visibility", STANDARD.title, "Fabrication"];
    }
    panels(tab).iter().map(|p| p.title).collect()
}

/// The tab with this name, ignoring case.
pub fn tab_named(name: &str) -> Option<Tab> {
    TABS.iter().find(|(_, n)| n.eq_ignore_ascii_case(name)).map(|(t, _)| *t)
}

/// The keyboard shortcut of an action, or "" when it has none.
pub fn shortcut(action: Action) -> &'static str {
    match action {
        Action::New => "Ctrl+N",
        Action::Open => "Ctrl+O",
        Action::Save => "Ctrl+S",
        Action::SaveAs => "Ctrl+Shift+S",
        Action::Undo => "Ctrl+Z",
        Action::Redo => "Ctrl+Y",
        Action::CopyClip => "Ctrl+C",
        Action::PasteClip => "Ctrl+V",
        Action::SelectAll => "Ctrl+A",
        Action::Erase => "Delete",
        Action::ToggleGrid => "F7",
        Action::ToggleOrtho => "F8",
        Action::ToggleSnap => "F9",
        Action::ToggleOsnap => "F3",
        Action::Palette => "Ctrl+K",
        _ => "",
    }
}

/// What a button shows: a glyph of the icon font, or a hand-drawn icon where no glyph fits.
#[derive(Clone, Copy)]
pub enum Pic {
    Glyph(&'static str),
    Drawn(Icon),
}

/// The glyph of the command when it has one, else the hand-drawn icon.
fn pic_of(info: Option<&CommandInfo>, fallback: Icon) -> Pic {
    match info.and_then(|c| crate::glyphs::glyph(c.action)) {
        Some(g) => Pic::Glyph(g),
        None => Pic::Drawn(fallback),
    }
}

pub fn paint_pic(painter: &egui::Painter, rect: Rect, pic: Pic, colour: Color32) {
    match pic {
        Pic::Drawn(i) => icons::paint(painter, rect, i, colour),
        Pic::Glyph(g) => {
            let side = rect.width().min(rect.height());
            painter.text(rect.center(), Align2::CENTER_CENTER, g, crate::theme::icon_font(side * 0.98), colour);
        }
    }
}

const BIG: Vec2 = Vec2::new(62.0, 62.0);
const SMALL_H: f32 = 20.0;


/// The preview card shown when the pointer rests on a button.
fn preview(ui: &mut Ui, icon: Pic, label: &str, info: Option<&CommandInfo>, planned: Option<&str>) {
    ui.set_max_width(300.0);
    ui.horizontal(|ui| {
        let (rect, _) = ui.allocate_exact_size(Vec2::splat(44.0), Sense::hover());
        ui.painter().rect_stroke(rect, 4.0, ui.visuals().widgets.noninteractive.bg_stroke, egui::StrokeKind::Inside);
        paint_pic(ui.painter(), rect.shrink(8.0), icon, ui.visuals().strong_text_color());
        ui.vertical(|ui| {
            ui.label(RichText::new(label).strong());
            match (info, planned) {
                (Some(c), _) => {
                    ui.label(c.help);
                }
                (None, Some(what)) => {
                    ui.label(what);
                }
                _ => {}
            }
        });
    });
    ui.separator();
    if let Some(c) = info {
        let aliases = if c.aliases.is_empty() { String::new() } else { format!("  (also {})", c.aliases.join(", ")) };
        ui.label(RichText::new(format!("Command: {}{aliases}", c.name)).small().monospace());
        let keys = shortcut(c.action);
        if !keys.is_empty() {
            ui.label(RichText::new(format!("Shortcut: {keys}")).small().monospace());
        }
        ui.label(RichText::new("Click, or type the command and press Enter. Esc cancels.").small().weak());
    } else {
        ui.label(RichText::new("Planned: not built yet.").small().color(crate::theme::warn()));
    }
}

/// The preview card of the first ribbon button that runs `command`, for checking the card
/// without a pointer (`--preview`). Returns false when no button runs it.
pub fn preview_of(ui: &mut Ui, command: &str) -> bool {
    let Some(info) = commands::resolve(command) else { return false };
    for (tab, _) in TABS {
        for item in panels(tab).iter().flat_map(|p| p.items) {
            if let Big(icon, label, name) | Small(icon, label, name) = *item {
                if commands::resolve(name).is_some_and(|c| c.action == info.action) {
                    preview(ui, pic_of(Some(info), icon), label, Some(info), None);
                    return true;
                }
            }
        }
    }
    false
}

/// Draws one button and returns true when it was clicked.
fn button(ui: &mut Ui, big: bool, icon: Pic, label: &str, info: Option<&CommandInfo>, planned: Option<&str>, on: bool) -> bool {
    let enabled = info.is_some() && ui.is_enabled();
    let font = FontId::proportional(11.0);
    let size = if big {
        // Wide enough that no word of the label has to break.
        let longest = label.split_whitespace().map(|word| ui.painter().layout_no_wrap(word.to_string(), font.clone(), Color32::WHITE).size().x).fold(0.0, f32::max);
        Vec2::new(BIG.x.max(longest + 10.0), BIG.y)
    } else {
        let text = ui.painter().layout_no_wrap(label.to_string(), font.clone(), Color32::WHITE);
        Vec2::new(24.0 + text.size().x + 8.0, SMALL_H)
    };
    let (rect, response) = ui.allocate_exact_size(size, if enabled { Sense::click() } else { Sense::hover() });
    let visuals = ui.style().interact(&response);
    if on {
        ui.painter().rect_filled(rect, 4.0, ui.visuals().selection.bg_fill.gamma_multiply(0.45));
    }
    if enabled && (response.hovered() || response.is_pointer_button_down_on()) {
        ui.painter().rect_filled(rect, 4.0, visuals.bg_fill);
    }
    let colour = if enabled { ui.visuals().text_color() } else { ui.visuals().weak_text_color().gamma_multiply(0.7) };
    if big {
        let icon_rect = Rect::from_center_size(pos2(rect.center().x, rect.top() + 19.0), Vec2::splat(26.0));
        paint_pic(ui.painter(), icon_rect, icon, colour);
        let galley = ui.painter().layout(label.to_string(), font, colour, size.x - 4.0);
        // Centre each wrapped line under the icon.
        let mut y = rect.top() + 36.0;
        for row in &galley.rows {
            let text: String = row.glyphs.iter().map(|g| g.chr).collect();
            ui.painter().text(pos2(rect.center().x, y), Align2::CENTER_TOP, text.trim(), FontId::proportional(11.0), colour);
            y += 12.0;
        }
    } else {
        let icon_rect = Rect::from_min_size(rect.min + Vec2::new(4.0, 3.0), Vec2::splat(14.0));
        paint_pic(ui.painter(), icon_rect, icon, colour);
        ui.painter().text(pos2(icon_rect.right() + 6.0, rect.center().y), Align2::LEFT_CENTER, label, font, colour);
    }
    let clicked = response.clicked() && enabled;
    response.on_hover_ui(|ui| preview(ui, icon, label, info, planned));
    clicked
}

fn panel(ui: &mut Ui, title: &str, pick: &mut Option<Action>, is_on: &dyn Fn(Action) -> bool, body: impl FnOnce(&mut Ui, &mut Option<Action>, &dyn Fn(Action) -> bool)) {
    ui.vertical(|ui| {
        ui.spacing_mut().item_spacing = Vec2::new(2.0, 1.0);
        let body_rect = ui
            .horizontal_top(|ui| {
                ui.set_min_height(BIG.y);
                body(ui, pick, is_on);
            })
            .response
            .rect;
        // The title sits centred under the buttons, no wider than they are.
        let (rect, _) = ui.allocate_exact_size(Vec2::new(body_rect.width(), 13.0), Sense::hover());
        ui.painter().text(rect.center(), Align2::CENTER_CENTER, title, FontId::proportional(10.0), ui.visuals().weak_text_color());
    });
    let (line, _) = ui.allocate_exact_size(Vec2::new(5.0, BIG.y + 13.0), Sense::hover());
    ui.painter().vline(line.center().x, line.y_range(), ui.visuals().widgets.noninteractive.bg_stroke);
}

fn items(ui: &mut Ui, list: &[Item], pick: &mut Option<Action>, is_on: &dyn Fn(Action) -> bool) {
    let mut i = 0;
    while i < list.len() {
        match list[i] {
            Big(icon, label, command) => {
                let info = commands::resolve(command);
                if button(ui, true, pic_of(info, icon), label, info, None, info.is_some_and(|c| is_on(c.action))) {
                    *pick = info.map(|c| c.action);
                }
                i += 1;
            }
            Planned(icon, label, what) => {
                button(ui, true, Pic::Drawn(icon), label, None, Some(what), false);
                i += 1;
            }
            Small(..) => {
                // A run of small buttons stacks in columns of three.
                let run: Vec<Item> = list[i..].iter().copied().take_while(|x| matches!(x, Small(..))).take(3).collect();
                ui.vertical(|ui| {
                    for item in &run {
                        if let Small(icon, label, command) = *item {
                            let info = commands::resolve(command);
                            if button(ui, false, pic_of(info, icon), label, info, None, info.is_some_and(|c| is_on(c.action))) {
                                *pick = info.map(|c| c.action);
                            }
                        }
                    }
                });
                i += run.len();
            }
        }
    }
}

/// Draws the workspace switcher, the tab strip and the active tab's panels. `is_on` tells which
/// toggles are active. Returns the action of the button that was clicked, if any; `workspace` is
/// changed when another workspace is picked (the caller then applies its panel settings).
pub fn show(ui: &mut Ui, tab: &mut Tab, standard: &str, signs_ready: bool, is_on: &dyn Fn(Action) -> bool, workspace: &mut usize) -> (Option<Action>, bool) {
    let mut pick = None;
    let ws = &crate::workspace::WORKSPACES[(*workspace).min(crate::workspace::WORKSPACES.len() - 1)];
    let visible = ws.visible_tabs();
    if !visible.iter().any(|v| v.tab == *tab) {
        *tab = Tab::Home;
    }
    let t = crate::theme::tokens_ui(ui);
    let mut switched = false;
    ui.horizontal(|ui| {
        ui.spacing_mut().item_spacing.x = 4.0;
        switched = crate::workspace::switcher(ui, workspace);
        ui.add_space(6.0);
        for v in &visible {
            let name = TABS.iter().find(|(x, _)| *x == v.tab).map(|(_, n)| *n).unwrap_or("");
            let on = *tab == v.tab;
            let r = ui.add(egui::Label::new(RichText::new(name).size(12.5).color(if on { t.accent_text } else { t.text })).selectable(false).sense(Sense::click()));
            let r = r.on_hover_cursor(egui::CursorIcon::PointingHand);
            let line = Rect::from_min_max(pos2(r.rect.left() - 4.0, r.rect.bottom() + 1.0), pos2(r.rect.right() + 4.0, r.rect.bottom() + 3.0));
            if on {
                ui.painter().rect_filled(line, 1.0, t.accent_text);
            } else if r.hovered() {
                ui.painter().rect_filled(line, 1.0, t.border);
            }
            if r.clicked() {
                *tab = v.tab;
            }
            ui.add_space(6.0);
        }
    });
    ui.add_space(3.0);
    let allowed = visible.iter().find(|v| v.tab == *tab).cloned();
    egui::ScrollArea::horizontal().id_salt("ribbon").auto_shrink([false, true]).max_height(BIG.y + 18.0).show(ui, |ui| {
        ui.horizontal_top(|ui| {
            ui.spacing_mut().item_spacing.x = 4.0;
            if *tab == Tab::Signs {
                ui.add_enabled_ui(signs_ready, |ui| {
                    let templates = signs_glue::templates(standard);
                    for (title, range) in [("Direction signs", 0..5.min(templates.len())), ("Map-type and temporary", 5.min(templates.len())..templates.len())] {
                        panel(ui, title, &mut pick, is_on, |ui, pick, _| {
                            for (i, t) in templates.iter().enumerate().filter(|(i, _)| range.contains(i)) {
                                if button(ui, true, Pic::Drawn(t.icon), t.label, commands::resolve(t.command), None, false) {
                                    *pick = Some(Action::InsertSign(i as u8));
                                }
                            }
                        });
                    }
                    panel(ui, "Catalogue", &mut pick, is_on, |ui, pick, on| items(ui, &[Big(Icon::Library, "Standard signs", "LIBRARY")], pick, on));
                });
                panel(ui, "Visibility", &mut pick, is_on, |ui, pick, on| items(ui, &[Big(Icon::Visibility, "Sign visibility", "VISIBILITY"), Big(Icon::Visibility, "Run visibility", "VISIBILITYRUN")], pick, on));
                panel(ui, STANDARD.title, &mut pick, is_on, |ui, pick, on| items(ui, STANDARD.items, pick, on));
                panel(ui, "Fabrication", &mut pick, is_on, |ui, pick, on| items(ui, &[Big(Icon::Quantities, "Quantities", "QUANTITIES"), Big(Icon::Sheets, "Fabrication sheets", "SHEETS")], pick, on));
            } else {
                for p in panels(*tab).iter().filter(|p| allowed.as_ref().is_none_or(|a| a.allows(p.title))) {
                    panel(ui, p.title, &mut pick, is_on, |ui, pick, on| items(ui, p.items, pick, on));
                }
            }
        });
    });
    (pick, switched)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_ribbon_button_names_a_real_command() {
        for (tab, _) in TABS {
            for p in panels(tab) {
                for item in p.items {
                    if let Big(_, label, command) | Small(_, label, command) = *item {
                        assert!(commands::resolve(command).is_some(), "{label} on {tab:?} names the unknown command {command}");
                    }
                }
            }
        }
        for command in ["LIBRARY", "QUANTITIES", "SHEETS", "SHEETSET", "SHEETPDF", "SHEETDXF", "SHEETPREVIEW", "TITLEBLOCK", "PROJECTINFO"] {
            assert!(commands::resolve(command).is_some());
        }
    }

    #[test]
    fn the_cad_basics_are_live_commands_not_planned_items() {
        // Text, dimension, leader, measure, sign schedule, DXF underlay, layer manager and units
        // are all buttons that run a command; what is still planned is listed here by label.
        let planned: Vec<&str> = TABS.iter().flat_map(|(tab, _)| panels(*tab)).flat_map(|p| p.items).filter_map(|i| if let Planned(_, label, _) = *i { Some(label) } else { None }).collect();
        for label in ["Text", "Dimension", "Leader", "Measure", "Sign schedule", "DXF underlay", "Layer manager", "Units"] {
            assert!(!planned.contains(&label), "{label} is still planned");
        }
        for command in ["TEXT", "MTEXT", "TEXTEDIT", "DIM", "DIMLINEAR", "DIMALIGNED", "DIMANGULAR", "DIMRADIUS", "DIMDIAMETER", "DIMARC", "DIMCONTINUE", "DIMBASELINE", "DIMSTATION", "DIMLEVEL", "LEADER", "MLEADER", "DIST", "AREA", "ID", "STYLE", "DIMSTYLE", "OSNAP", "LAYER", "UNITS", "PROJECTSETTINGS", "UNDERLAY", "SIGNSCHEDULE", "SIGNSCHEDULEOUT", "XREF", "MEASURE"] {
            assert!(commands::resolve(command).is_some(), "{command} is not a command");
        }
    }

    #[test]
    fn every_command_can_be_reached_from_the_ribbon() {
        // The sign inserts are on the Signs tab, built from the templates; Quit stays on the
        // File menu.
        let on_ribbon: Vec<Action> = TABS
            .iter()
            .flat_map(|(tab, _)| panels(*tab))
            .flat_map(|p| p.items)
            .filter_map(|item| match *item {
                Big(_, _, command) | Small(_, _, command) => commands::resolve(command).map(|c| c.action),
                Planned(..) => None,
            })
            .collect();
        for c in commands::COMMANDS {
            let elsewhere = matches!(c.action, Action::InsertSign(_) | Action::Quit);
            assert!(elsewhere || on_ribbon.contains(&c.action), "{} is not on the ribbon", c.name);
        }
    }
}
