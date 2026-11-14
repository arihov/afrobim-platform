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
    CommandInfo { name: "CHECK", aliases: &["VERIFY"], help: "Run the check of the selected gantry, drain, street lighting, barrier run or pavement (the pavement check is a design method)", action: Action::CheckElement },
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
