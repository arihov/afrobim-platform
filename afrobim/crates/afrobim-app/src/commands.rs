//! The command table: one entry per command, shared by the menus, the toolbar, the palette and
//! the command line, so every route runs the same action and the `?` list is always complete.

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Action {
    New,
    Open,
    Save,
    SaveAs,
    ExportDxf,
    ExportIfc,
    ExportQuantities,
    ExportSheets,
    Undo,
    Redo,
    CopyClip,
    PasteClip,
    Move,
    Copy,
    Erase,
    SelectAll,
    ZoomExtents,
    ZoomWindow,
    ZoomIn,
    ZoomOut,
    ToggleGrid,
    ToggleSnap,
    ToggleOrtho,
    RepReal,
    RepOutlined,
    ToggleCanvas,
    ToggleMarks,
    ToggleTheme,
    Toggle3d,
    Arrange,
    /// Insert the sign template with this index (see `signs_glue::templates`).
    InsertSign(u8),
    Library,
    AddSupports,
    AddMarking,
    /// Add the road element with this index in `modules_glue::KINDS`.
    AddRoadElement(u8),
    CheckElement,
    /// Street lighting: an installation along an alignment, or a single column.
    AddLighting,
    AddLightColumn,
    /// A safety barrier run along an alignment.
    AddBarrier,
    AddPavement,
    /// Run a pavement design method (MoWT, UK DMRB or AASHTO 1993).
    PavementDesign,
    /// Import ground surfaces from LandXML.
    ImportSurface,
    AddCorridor,
    /// Cross-sections of the selected corridor.
    ShowSections,
    HangSign,
    UnhangSign,
    DesignSupports,
    ImportLandXml,
    PlaceOnAlignment,
    RemoveSupports,
    /// Switch the design standard to the one with this index in `signs_glue::STANDARDS`.
    Standard(u8),
    // Coordination
    ClashTests,
    ClashRun,
    ClashResults,
    ClashMarkers,
    ClashCsv,
    /// The Issues window.
    Issues,
    BcfOut,
    BcfIn,
    /// Import IFC files as read-only reference models.
    IfcRef,
    RefModels,
    // 4D
    Programme,
    Calendar,
    ScheduleRun,
    Gantt,
    SetBaseline,
    Variance,
    Progress,
    Link4d,
    LinkReport,
    FourD,
    MspIn,
    MspOut,
    ProgCsv,
    XerIn,
    XerOut,
    Wbs,
    CriticalBasis,
    TimeLocation,
    Play4d,
    // 5D
    Bill,
    Rates,
    Currency,
    Estimate,
    CashFlow,
    Certificate,
    CostCsv,
    /// Run every check on every element and open the report.
    CheckAll,
    /// Peak flow by the rational method.
    Rational,
    /// Sign visibility: inputs, results and drawings for the selected sign or all signs.
    Visibility,
    /// Photometry from an IES or LDT file for a lighting installation.
    Photometry,
    /// The calculation report pack (PDF, text and CSV).
    ReportPack,
    // Construction
    SiteRecord,
    SiteLog,
    RollUp,
    Evm,
    LookAhead,
    SiteView,
    EvmCsv,
    // Drawing sheets
    SheetSets,
    SheetPreview,
    TitleBlock,
    ProjectInfo,
    SheetPdf,
    SheetDxf,
    // Alignment design
    AlignDesign,
    AlignEdit,
    PiTable,
    AlignLabels,
    ProfileDesign,
    AlignCheck,
    LandXmlOut,
    Options,
    Help,
    About,
    Quit,
}

pub struct CommandInfo {
    pub name: &'static str,
    pub aliases: &'static [&'static str],
    pub help: &'static str,
    pub action: Action,
}

pub const COMMANDS: &[CommandInfo] = &[
    CommandInfo { name: "NEW", aliases: &[], help: "Start a new document", action: Action::New },
    CommandInfo { name: "OPEN", aliases: &[], help: "Open an AfroBIM document", action: Action::Open },
    CommandInfo { name: "SAVE", aliases: &["QSAVE"], help: "Save the document", action: Action::Save },
    CommandInfo { name: "SAVEAS", aliases: &[], help: "Save the document under a new name", action: Action::SaveAs },
    CommandInfo { name: "DXFOUT", aliases: &["EXPORT"], help: "Export the selection, or everything, as DXF", action: Action::ExportDxf },
    CommandInfo { name: "IFCOUT", aliases: &["IFC"], help: "Export the model as IFC 4.3", action: Action::ExportIfc },
    CommandInfo { name: "QUANTITIES", aliases: &["BOQ"], help: "Export sign-face quantities (CSV and summary)", action: Action::ExportQuantities },
    CommandInfo { name: "SHEETS", aliases: &["FABRICATION"], help: "Export fabrication sheets (DXF and PDF) and the schedule PDF", action: Action::ExportSheets },
    CommandInfo { name: "UNDO", aliases: &["U"], help: "Undo the last edit", action: Action::Undo },
    CommandInfo { name: "REDO", aliases: &[], help: "Redo the edit that was undone", action: Action::Redo },
    CommandInfo { name: "MOVE", aliases: &["M"], help: "Move the selection: base point, second point", action: Action::Move },
    CommandInfo { name: "COPY", aliases: &["CO", "CP"], help: "Copy the selection: base point, a point per copy", action: Action::Copy },
    CommandInfo { name: "ERASE", aliases: &["E", "DELETE"], help: "Erase the selection, or ask for one", action: Action::Erase },
    CommandInfo { name: "SELECTALL", aliases: &["ALL"], help: "Select every element", action: Action::SelectAll },
    CommandInfo { name: "ZOOM", aliases: &["Z", "ZE"], help: "Zoom to show every element", action: Action::ZoomExtents },
    CommandInfo { name: "ZOOMWINDOW", aliases: &["ZW"], help: "Zoom to a window picked with two corners", action: Action::ZoomWindow },
    CommandInfo { name: "GRID", aliases: &[], help: "Toggle the grid (F7)", action: Action::ToggleGrid },
    CommandInfo { name: "SNAP", aliases: &[], help: "Toggle grid snap for picked points (F9)", action: Action::ToggleSnap },
    CommandInfo { name: "ORTHO", aliases: &[], help: "Toggle ortho: moves held to X or Y (F8)", action: Action::ToggleOrtho },
    CommandInfo { name: "REAL", aliases: &[], help: "Show elements in full colour", action: Action::RepReal },
    CommandInfo { name: "OUTLINED", aliases: &["WIREFRAME"], help: "Show elements as outlines", action: Action::RepOutlined },
    CommandInfo { name: "ARRANGE", aliases: &[], help: "Arrange every element in rows", action: Action::Arrange },
    CommandInfo { name: "STACK", aliases: &["SIGN"], help: "Insert a stack-type direction sign", action: Action::InsertSign(0) },
    CommandInfo { name: "DIRECTION", aliases: &["DS"], help: "Insert a rectangular direction sign", action: Action::InsertSign(1) },
    CommandInfo { name: "CONFIRM", aliases: &["RC"], help: "Insert a route confirmatory sign", action: Action::InsertSign(2) },
    CommandInfo { name: "PLACE", aliases: &[], help: "Insert a place identification sign", action: Action::InsertSign(3) },
    CommandInfo { name: "LANE", aliases: &[], help: "Insert a lane pre-selection sign", action: Action::InsertSign(4) },
    CommandInfo { name: "DIVERSION", aliases: &[], help: "Insert a temporary diversion sign", action: Action::InsertSign(5) },
    CommandInfo { name: "MAP", aliases: &["CROSSROADS"], help: "Insert a map-type crossroads sign", action: Action::InsertSign(6) },
    CommandInfo { name: "TEE", aliases: &[], help: "Insert a map-type T-junction sign", action: Action::InsertSign(7) },
    CommandInfo { name: "ROUNDABOUT", aliases: &["RBT"], help: "Insert a map-type roundabout sign", action: Action::InsertSign(8) },
    CommandInfo { name: "LIBRARY", aliases: &["INSERT", "LIB"], help: "Open the standard-sign library", action: Action::Library },
    CommandInfo { name: "3D", aliases: &["ORBIT", "PLAN"], help: "Switch between the 2D model space and the 3D view", action: Action::Toggle3d },
    CommandInfo { name: "UK", aliases: &["UKTSM"], help: "Use the UK Traffic Signs Manual", action: Action::Standard(0) },
    CommandInfo { name: "UGANDA", aliases: &["UG", "UGTSM"], help: "Use the Uganda Traffic Signs Manual", action: Action::Standard(1) },
    CommandInfo { name: "LANDXML", aliases: &["ALIGNMENT"], help: "Import horizontal alignments from a LandXML file", action: Action::ImportLandXml },
    CommandInfo { name: "STATION", aliases: &["CHAINAGE"], help: "Place the selected sign on an alignment by station and offset", action: Action::PlaceOnAlignment },
    CommandInfo { name: "MARKINGS", aliases: &["MARKING", "ROADMARKING"], help: "Lay a road marking line along an alignment (Traffic Signs Manual Chapter 5)", action: Action::AddMarking },
    CommandInfo { name: "DRAIN", aliases: &["SIDEDRAIN"], help: "Add a side drain along an alignment, with placeholder dimensions to edit", action: Action::AddRoadElement(0) },
    CommandInfo { name: "CULVERT", aliases: &[], help: "Add a pipe culvert across an alignment at a station, with placeholder dimensions to edit", action: Action::AddRoadElement(1) },
    CommandInfo { name: "GANTRY", aliases: &["PORTAL"], help: "Add a portal sign gantry at a station, with placeholder dimensions to edit", action: Action::AddRoadElement(2) },
    CommandInfo { name: "CANTILEVER", aliases: &[], help: "Add a cantilever sign gantry at a station, with placeholder dimensions to edit", action: Action::AddRoadElement(3) },
    CommandInfo { name: "CHECK", aliases: &["VERIFY"], help: "Run the check of the selected gantry, side drain, culvert, street lighting, barrier run or pavement, with the inputs you enter (kept on the element for CHECKALL)", action: Action::CheckElement },
    CommandInfo { name: "LIGHTING", aliases: &["LIGHTS", "STREETLIGHTING"], help: "Lay a street lighting installation along an alignment (spacing, arrangement and the column are entered)", action: Action::AddLighting },
    CommandInfo { name: "LIGHTCOLUMN", aliases: &["COLUMN"], help: "Place one lighting column at a station and offset on an alignment", action: Action::AddLightColumn },
    CommandInfo { name: "BARRIER", aliases: &["RESTRAINT"], help: "Add a safety barrier run along an alignment (containment, working width and impact severity are entered)", action: Action::AddBarrier },
    CommandInfo { name: "PAVEMENT", aliases: &["PAVE"], help: "Add a pavement along an alignment: width, cross-fall and a list of layers", action: Action::AddPavement },
    CommandInfo { name: "PAVEDESIGN", aliases: &["PAVEMENTDESIGN"], help: "Design a pavement structure by the MoWT manual, UK DMRB or AASHTO 1993, and make a pavement from the layers", action: Action::PavementDesign },
    CommandInfo { name: "SURFACE", aliases: &["TIN", "TERRAIN"], help: "Import ground surfaces (TIN) from a LandXML file", action: Action::ImportSurface },
    CommandInfo { name: "CORRIDOR", aliases: &[], help: "Build a corridor on an alignment and a surface: a typical section, cut and fill, earthworks", action: Action::AddCorridor },
    CommandInfo { name: "SECTIONS", aliases: &["XSECTIONS"], help: "Show the cross-sections of the selected corridor, station by station", action: Action::ShowSections },
    CommandInfo { name: "HANG", aliases: &["GANTRYSIGN"], help: "Hang the selected sign on a gantry", action: Action::HangSign },
    CommandInfo { name: "UNHANG", aliases: &[], help: "Take the selected sign off its gantry", action: Action::UnhangSign },
    CommandInfo { name: "SUPPORTS", aliases: &["POSTS"], help: "Add posts and foundations to the selected sign", action: Action::AddSupports },
    CommandInfo { name: "DESIGN", aliases: &["SIZE"], help: "Size the selected sign's posts and foundations for wind (UK method)", action: Action::DesignSupports },
    CommandInfo { name: "NOSUPPORTS", aliases: &[], help: "Remove the selected sign's posts and foundations", action: Action::RemoveSupports },
    CommandInfo { name: "CLASHTESTS", aliases: &["CLASHSETUP"], help: "Add, edit and delete clash tests: what is checked against what, hard or clearance, with your own tolerance", action: Action::ClashTests },
    CommandInfo { name: "CLASH", aliases: &["CLASHRUN", "CLASHDETECT"], help: "Run every clash test on the model and open the results", action: Action::ClashRun },
    CommandInfo { name: "CLASHES", aliases: &["CLASHRESULTS"], help: "Open the clash results: filter, review, comment, assign, zoom to a clash", action: Action::ClashResults },
    CommandInfo { name: "CLASHMARKERS", aliases: &[], help: "Show or hide the clash markers in the 3D view", action: Action::ClashMarkers },
    CommandInfo { name: "CLASHCSV", aliases: &[], help: "Export the clash results as CSV", action: Action::ClashCsv },
    CommandInfo { name: "ISSUES", aliases: &["ISSUELIST", "TOPICS"], help: "Open the issues: raise them from clashes or by hand, comment, assign, keep viewpoints from the 3D camera with a snapshot", action: Action::Issues },
    CommandInfo { name: "BCFOUT", aliases: &["EXPORTBCF"], help: "Export the issues as BCF 2.1 (not tested in other BCF tools; BCF 3.0 is not supported)", action: Action::BcfOut },
    CommandInfo { name: "BCFIN", aliases: &["IMPORTBCF"], help: "Import issues from a BCF 2.1 file and show what could not be carried", action: Action::BcfIn },
    CommandInfo { name: "IFCREF", aliases: &["REFERENCEMODEL", "IMPORTIFC"], help: "Import IFC files as read-only reference models, stored by path and hash, shown in the 3D site view and usable in clash tests", action: Action::IfcRef },
    CommandInfo { name: "REFMODELS", aliases: &["REFERENCES", "REFLIST"], help: "List the reference models: visibility, transparency, alignment offset and rotation, import report, and the properties of a picked product", action: Action::RefModels },
    CommandInfo { name: "PROGRAMME", aliases: &["ACTIVITIES", "PROG"], help: "Edit the construction programme: activities, durations, constraints and dependencies", action: Action::Programme },
    CommandInfo { name: "CALENDAR", aliases: &["WORKDAYS"], help: "Edit the programme calendar: working weekdays and the days off you enter", action: Action::Calendar },
    CommandInfo { name: "SCHEDULE", aliases: &["CPM"], help: "Run the critical path method and show dates, float, the critical path and any problems", action: Action::ScheduleRun },
    CommandInfo { name: "GANTT", aliases: &[], help: "Show the Gantt chart: critical path, baseline bars and the data date", action: Action::Gantt },
    CommandInfo { name: "BASELINE", aliases: &[], help: "Keep the current schedule as the baseline", action: Action::SetBaseline },
    CommandInfo { name: "VARIANCE", aliases: &[], help: "Show how the programme differs from the baseline", action: Action::Variance },
    CommandInfo { name: "PROGRESS", aliases: &["DATADATE"], help: "Enter progress and the data date, then reschedule", action: Action::Progress },
    CommandInfo { name: "LINK4D", aliases: &["LINKACTIVITY"], help: "Link model elements to activities, by selection or by rule", action: Action::Link4d },
    CommandInfo { name: "LINKREPORT", aliases: &[], help: "List elements no activity reaches and activities with no elements", action: Action::LinkReport },
    CommandInfo { name: "FOURD", aliases: &["DATE4D"], help: "Show the model at a chosen date: not started, in progress, complete, removed", action: Action::FourD },
    CommandInfo { name: "MSPIN", aliases: &["IMPORTMSP"], help: "Import a programme from MS Project XML and show what the file could not carry", action: Action::MspIn },
    CommandInfo { name: "MSPOUT", aliases: &["EXPORTMSP"], help: "Export the programme as MS Project XML", action: Action::MspOut },
    CommandInfo { name: "PROGCSV", aliases: &["PROGRAMMEREPORTS"], help: "Export the activity table, variance and S-curve as CSV", action: Action::ProgCsv },
    CommandInfo { name: "XERIN", aliases: &["IMPORTXER", "P6IN"], help: "Import a programme from a Primavera P6 XER file and show what the file could not carry (files have not been tested in P6)", action: Action::XerIn },
    CommandInfo { name: "XEROUT", aliases: &["EXPORTXER", "P6OUT"], help: "Export the programme as a Primavera P6 XER file and show what the file does not carry (not tested in P6)", action: Action::XerOut },
    CommandInfo { name: "WBS", aliases: &["SUMMARYTASKS", "ROLLUPWBS"], help: "Edit the work breakdown structure: summary tasks, which summary each activity sits under, and the roll-up on the basis you choose", action: Action::Wbs },
    CommandInfo { name: "CRITICAL", aliases: &["CRITICALBASIS", "LONGESTPATH"], help: "Choose what makes an activity critical: total float of zero or less, or the longest path, with the engine's definition", action: Action::CriticalBasis },
    CommandInfo { name: "TIMELOCATION", aliases: &["TLCHART", "TIMECHAINAGE"], help: "Time-location chart of the programme along an alignment: location ranges in stations, direction of working, dwell, critical marking, data date and baseline", action: Action::TimeLocation },
    CommandInfo { name: "PLAY4D", aliases: &["PLAYBACK", "ANIMATE4D"], help: "Play the programme in the 3D site view: play, pause, speed and a scrub bar, with the date shown large", action: Action::Play4d },
    CommandInfo { name: "ALIGNDESIGN", aliases: &["NEWALIGNMENT", "DESIGNALIGNMENT"], help: "Design an alignment from PIs picked on the plan or typed as coordinates: name, start station; curves start with no radius", action: Action::AlignDesign },
    CommandInfo { name: "ALIGNEDIT", aliases: &["PLANEDIT", "PIEDIT"], help: "Edit a designed alignment in plan: drag PIs, insert a PI on a tangent, delete a PI; a move the engine refuses shows its reason", action: Action::AlignEdit },
    CommandInfo { name: "PITABLE", aliases: &["PIS", "CURVETABLE"], help: "PI table of the designed alignment: station, coordinates, deflection, radius, spirals, tangent and curve length, with the feasible radius range", action: Action::PiTable },
    CommandInfo { name: "ALIGNLABELS", aliases: &["CURVELABELS"], help: "Show or hide the curve data labels on the plan", action: Action::AlignLabels },
    CommandInfo { name: "PROFILEDESIGN", aliases: &["PROFILEEDIT", "PVI"], help: "Profile editor of the designed alignment: ground line, PVIs to click and drag, the PVI table, vertical exaggeration and Fit to ground", action: Action::ProfileDesign },
    CommandInfo { name: "ALIGNCHECK", aliases: &["DESIGNCHECK", "GEOMCHECK"], help: "Check a designed alignment against a limits table you load (CSV or JSON): per element, with the source of each limit", action: Action::AlignCheck },
    CommandInfo { name: "LANDXMLOUT", aliases: &["EXPORTLANDXML"], help: "Export the designed alignment and its profile as LandXML", action: Action::LandXmlOut },
    CommandInfo { name: "OPTIONS", aliases: &["ALIGNOPTIONS", "OPTIONSET"], help: "Alignment options: add the current design as an option, compare the options on figures worked out from the model, score them with your own weights", action: Action::Options },
    CommandInfo { name: "BILL", aliases: &["BOQSETUP"], help: "Edit the bill of quantities and its mapping rules; see measured quantities, unmapped and ambiguous lines", action: Action::Bill },
    CommandInfo { name: "RATES", aliases: &["RATELIBRARY"], help: "Enter or load rates (with currency, source note and date); lists unpriced items", action: Action::Rates },
    CommandInfo { name: "CURRENCY", aliases: &["EXCHANGE"], help: "Set the primary currency label, an optional secondary currency and dated exchange rates", action: Action::Currency },
    CommandInfo { name: "ESTIMATE", aliases: &["PRICE"], help: "Show the priced bill, additions, audit trail and revision comparison", action: Action::Estimate },
    CommandInfo { name: "CASHFLOW", aliases: &["SCURVE"], help: "Phase the estimate in time (dates from the programme if you like): monthly table and S-curve", action: Action::CashFlow },
    CommandInfo { name: "CERTIFICATE", aliases: &["IPC", "INTERIM"], help: "Work out an interim payment certificate from your own payment terms", action: Action::Certificate },
    CommandInfo { name: "COSTCSV", aliases: &["COSTREPORTS"], help: "Export every cost report as CSV into a folder", action: Action::CostCsv },
    CommandInfo { name: "CHECKALL", aliases: &["CHECKMODEL", "CALCPACK"], help: "Run every check on every element and open one report: a status per element, the calculation lines, filters, CSV and a calculation pack", action: Action::CheckAll },
    CommandInfo { name: "RATIONAL", aliases: &["RUNOFF"], help: "Peak flow by the rational method from the catchment areas, runoff coefficients and intensity you enter; keep it on the selected drain or culvert", action: Action::Rational },
    CommandInfo { name: "VISIBILITY", aliases: &["SIGHTLINE", "SIGNVISIBILITY"], help: "Sign visibility along the alignment: enter the inputs for the selected sign or all signs, run, and see the check lines with citations, the plan envelope and the sight-line profile", action: Action::Visibility },
    CommandInfo { name: "PHOTOMETRY", aliases: &["IES", "LDT", "LUMINAIREFILE"], help: "Load an IES or LDT file for the selected lighting installation, check it and see its polar diagram, then calculate illuminance on grids you define", action: Action::Photometry },
    CommandInfo { name: "REPORTPACK", aliases: &["CALCREPORT", "REPORTS"], help: "Compose the calculation report pack (one A4 PDF, plain text and CSV): cover, contents, design basis, every check and the summaries", action: Action::ReportPack },
    CommandInfo { name: "SITEREC", aliases: &["SITERECORD"], help: "Record percent complete or a measured quantity at a date for the selected elements, or by category, layer or mark pattern", action: Action::SiteRecord },
    CommandInfo { name: "SITELOG", aliases: &["SITERECORDS"], help: "List the site records, filtered by element and date", action: Action::SiteLog },
    CommandInfo { name: "ROLLUP", aliases: &["PROGRESSUPDATE"], help: "Roll the element records up to the activities, see the inferred actual dates, then apply the update to the programme and reschedule", action: Action::RollUp },
    CommandInfo { name: "EVM", aliases: &["EARNEDVALUE"], help: "Earned value at a data date: budget, actual costs, the EAC formula you choose, the figures, the tables and the S-curve", action: Action::Evm },
    CommandInfo { name: "LOOKAHEAD", aliases: &["DELAYS", "DELAYLIST"], help: "Activities due to start or finish in the next days you enter, and the delay list", action: Action::LookAhead },
    CommandInfo { name: "SITEVIEW", aliases: &["PROGRESSCOLOUR"], help: "Colour the 3D view by recorded percent complete at the 4D date, or back to the programme state", action: Action::SiteView },
    CommandInfo { name: "EVMCSV", aliases: &["CONSTRUCTIONCSV"], help: "Export every earned value table as CSV into a folder", action: Action::EvmCsv },
    CommandInfo { name: "SHEETSET", aliases: &["DRAWINGSHEETS", "SHEETMANAGER"], help: "Open the sheet set manager: make sheet sets and add plan, profile, section, schedule, detail and notes sheets", action: Action::SheetSets },
    CommandInfo { name: "SHEETPREVIEW", aliases: &["PREVIEWSHEETS"], help: "Compose the sheet set from the live model and page through the sheets, with the composer's warnings", action: Action::SheetPreview },
    CommandInfo { name: "TITLEBLOCK", aliases: &["TBLOCK"], help: "Edit the title block layout of the sheet set: fields shown, rows and cells, load or save it as JSON", action: Action::TitleBlock },
    CommandInfo { name: "PROJECTINFO", aliases: &["PROJECT"], help: "Edit the project information that title blocks print: project, client, contract number, revisions and other fields", action: Action::ProjectInfo },
    CommandInfo { name: "SHEETPDF", aliases: &["PLOTSHEETS"], help: "Export the whole sheet set as one PDF, a page per sheet", action: Action::SheetPdf },
    CommandInfo { name: "SHEETDXF", aliases: &["SHEETSDXF"], help: "Export the sheet set as DXF, one file per sheet in a folder", action: Action::SheetDxf },
    CommandInfo { name: "COPYCLIP", aliases: &[], help: "Copy the selection to the clipboard", action: Action::CopyClip },
    CommandInfo { name: "PASTECLIP", aliases: &["PASTE"], help: "Paste the clipboard into the model space", action: Action::PasteClip },
    CommandInfo { name: "ZOOMIN", aliases: &["ZI"], help: "Zoom in about the centre of the view", action: Action::ZoomIn },
    CommandInfo { name: "ZOOMOUT", aliases: &["ZO"], help: "Zoom out about the centre of the view", action: Action::ZoomOut },
    CommandInfo { name: "MARKS", aliases: &["LABELS"], help: "Show or hide the mark under each element", action: Action::ToggleMarks },
    CommandInfo { name: "CANVAS", aliases: &[], help: "Switch the model space between dark and light", action: Action::ToggleCanvas },
    CommandInfo { name: "THEME", aliases: &[], help: "Switch between the light and dark themes", action: Action::ToggleTheme },
    CommandInfo { name: "HELP", aliases: &["?"], help: "List every command", action: Action::Help },
    CommandInfo { name: "ABOUT", aliases: &[], help: "About AfroBIM", action: Action::About },
    CommandInfo { name: "QUIT", aliases: &["EXIT"], help: "Close AfroBIM", action: Action::Quit },
];

/// Resolves typed text to a command by name or alias, case-insensitively.
pub fn resolve(text: &str) -> Option<&'static CommandInfo> {
    let t = text.trim();
    COMMANDS.iter().find(|c| c.name.eq_ignore_ascii_case(t) || c.aliases.iter().any(|a| a.eq_ignore_ascii_case(t)))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn names_and_aliases_resolve_and_are_unique() {
        assert_eq!(resolve("m").unwrap().action, Action::Move);
        assert_eq!(resolve(" Co ").unwrap().action, Action::Copy);
        assert_eq!(resolve("?").unwrap().action, Action::Help);
        assert!(resolve("nonsense").is_none());
        // Every insert command points at an existing sign template, under the same name.
        let templates = crate::signs_glue::templates(crate::signs_glue::DEFAULT_STANDARD);
        for c in COMMANDS {
            if let Action::InsertSign(i) = c.action {
                assert_eq!(templates[i as usize].command, c.name);
            }
        }
        assert_eq!(COMMANDS.iter().filter(|c| matches!(c.action, Action::InsertSign(_))).count(), templates.len());
        let mut seen = std::collections::HashSet::new();
        for c in COMMANDS {
            assert!(seen.insert(c.name.to_ascii_uppercase()), "duplicate {}", c.name);
            for a in c.aliases {
                assert!(seen.insert(a.to_ascii_uppercase()), "duplicate alias {a}");
            }
        }
    }
}
