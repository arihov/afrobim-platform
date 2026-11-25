// SPDX-License-Identifier: LicenseRef-AfroBIM-Proprietary
// Copyright (c) 2026 AfroBIM. All rights reserved.
//! The layout (paper space) commands: one enum, the icon of each, and the entries the command
//! table adds. The work is in `layout_ui.rs`.

use egui_phosphor::regular as ph;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Lv {
    /// The layout manager: list, new, rename, delete, set current.
    Layout,
    LayoutNew,
    LayoutRename,
    LayoutDelete,
    LayoutDuplicate,
    /// Back to the model tab.
    Model,
    // Viewports
    Mview,
    MviewPoly,
    MviewObject,
    MviewFit,
    /// The 1, 2, 3 or 4 viewport arrangement.
    MviewPreset(u8),
    Mspace,
    Pspace,
    VpScale,
    /// VPROTATE: the twist angle of the view.
    VpRotate,
    VpLock,
    VpDisplay,
    VpMax,
    VpMin,
    VpLayer,
    VpClip,
    VpUnclip,
    VpDelete,
    VpCopy,
    VpProps,
    // Things drawn on the paper
    PaperRect,
    PaperCircle,
    PaperPoly,
    // Output
    PageSetup,
    Plot,
}

/// The icon of a layout command.
pub fn glyph(lv: Lv) -> &'static str {
    match lv {
        Lv::Layout => ph::FILES,
        Lv::LayoutNew => ph::FILE_PLUS,
        Lv::LayoutRename => ph::PENCIL_SIMPLE,
        Lv::LayoutDelete => ph::TRASH,
        Lv::LayoutDuplicate => ph::COPY,
        Lv::Model => ph::CUBE,
        Lv::Mview => ph::FRAME_CORNERS,
        Lv::MviewPoly => ph::POLYGON,
        Lv::MviewObject => ph::SELECTION,
        Lv::MviewFit => ph::CORNERS_OUT,
        Lv::MviewPreset(1) => ph::SQUARE,
        Lv::MviewPreset(2) => ph::COLUMNS,
        Lv::MviewPreset(3) => ph::SQUARE_SPLIT_HORIZONTAL,
        Lv::MviewPreset(_) => ph::SQUARES_FOUR,
        Lv::Mspace => ph::CURSOR_CLICK,
        Lv::Pspace => ph::FILE,
        Lv::VpScale => ph::RULER,
        Lv::VpRotate => ph::ARROW_CLOCKWISE,
        Lv::VpLock => ph::LOCK_SIMPLE,
        Lv::VpDisplay => ph::EYE,
        Lv::VpMax => ph::ARROWS_OUT,
        Lv::VpMin => ph::ARROWS_IN,
        Lv::VpLayer => ph::STACK,
        Lv::VpClip => ph::SCISSORS,
        Lv::VpUnclip => ph::SELECTION_SLASH,
        Lv::VpDelete => ph::TRASH_SIMPLE,
        Lv::VpCopy => ph::COPY_SIMPLE,
        Lv::VpProps => ph::SLIDERS,
        Lv::PaperRect => ph::RECTANGLE,
        Lv::PaperCircle => ph::CIRCLE,
        Lv::PaperPoly => ph::LINE_SEGMENTS,
        Lv::PageSetup => ph::GEAR,
        Lv::Plot => ph::PRINTER,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::commands::Action;

    #[test]
    fn every_layout_command_is_in_the_table_with_a_glyph_and_clean_text() {
        let ours: Vec<_> = crate::commands::COMMANDS.iter().filter(|c| matches!(c.action, Action::Layout(_))).collect();
        assert!(ours.len() >= 30, "{} layout commands", ours.len());
        for c in ours {
            let Action::Layout(lv) = c.action else { unreachable!() };
            assert!(!glyph(lv).is_empty());
            assert!(!c.help.contains('\u{2014}') && !c.help.contains('\u{2013}'), "{}: no dashes of that kind in text", c.name);
        }
        for name in ["LAYOUT", "MVIEW", "MSPACE", "PSPACE", "VPSCALE", "VPLOCK", "VPROTATE", "VPMAX", "VPMIN", "VPLAYER", "VPCLIP", "PAGESETUP", "PLOT"] {
            assert!(crate::commands::resolve(name).is_some(), "{name}");
        }
    }
}
