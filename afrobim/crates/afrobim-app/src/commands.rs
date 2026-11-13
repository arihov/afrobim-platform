//! The command table: one entry per command, shared by the menus, the toolbar, the palette and
//! the command line, so every route runs the same action and the `?` list is always complete.

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Action {
    New,
    Open,
    Save,
    SaveAs,
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
    ToggleTheme,
    Arrange,
    InsertStack,
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
    CommandInfo { name: "STACK", aliases: &["SIGN"], help: "Insert a stack-type direction sign", action: Action::InsertStack },
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
        let mut seen = std::collections::HashSet::new();
        for c in COMMANDS {
            assert!(seen.insert(c.name.to_ascii_uppercase()), "duplicate {}", c.name);
            for a in c.aliases {
                assert!(seen.insert(a.to_ascii_uppercase()), "duplicate alias {a}");
            }
        }
    }
}
