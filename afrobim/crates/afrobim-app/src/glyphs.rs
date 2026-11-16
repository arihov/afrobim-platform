//! The icon of every command, from the bundled Phosphor icon font (see `theme::install_fonts`).
//!
//! `glyph` is the one table: the ribbon, the menus, the toolbar, the context menus, the command
//! palette and the dialog headers all ask it. The hand-drawn icons in `icons.rs` stay as the
//! fallback for the sign templates (which draw the sign itself) and for planned tools.

use egui_phosphor::regular as ph;

use crate::anno_glue::Kind;
use crate::commands::{Action, CommandInfo};

/// The icon glyph of an action, or None when none has been chosen (a test keeps this empty).
pub fn glyph(action: Action) -> Option<&'static str> {
    use Action as A;
    Some(match action {
        A::New => ph::FILE_PLUS,
        A::Open => ph::FOLDER_OPEN,
        A::Save => ph::FLOPPY_DISK,
        A::SaveAs => ph::FLOPPY_DISK_BACK,
        A::ExportDxf => ph::FILE_DASHED,
        A::ExportIfc => ph::CUBE_TRANSPARENT,
        A::ExportQuantities => ph::TABLE,
        A::ExportSheets => ph::FILES,
        A::Undo => ph::ARROW_COUNTER_CLOCKWISE,
        A::Redo => ph::ARROW_CLOCKWISE,
        A::CopyClip => ph::COPY,
        A::PasteClip => ph::CLIPBOARD,
        A::Move => ph::ARROWS_OUT_CARDINAL,
        A::Copy => ph::COPY_SIMPLE,
        A::Erase => ph::ERASER,
        A::SelectAll => ph::SELECTION_ALL,
        A::ZoomExtents => ph::FRAME_CORNERS,
        A::ZoomWindow => ph::MAGNIFYING_GLASS,
        A::ZoomIn => ph::MAGNIFYING_GLASS_PLUS,
        A::ZoomOut => ph::MAGNIFYING_GLASS_MINUS,
        A::ToggleGrid => ph::GRID_FOUR,
        A::ToggleSnap => ph::MAGNET,
        A::ToggleOrtho => ph::ARROWS_HORIZONTAL,
        A::RepReal => ph::CUBE,
        A::RepOutlined => ph::BOUNDING_BOX,
        A::ToggleCanvas => ph::CIRCLE_HALF,
        A::ToggleMarks => ph::TAG,
        A::ToggleTheme => ph::MOON_STARS,
        A::Toggle3d => ph::THREE_D,
        A::Arrange => ph::SQUARES_FOUR,
        A::InsertSign(i) => match i {
            0 => ph::ARROWS_OUT_LINE_VERTICAL,
            1 => ph::SIGNPOST,
            2 => ph::TRAFFIC_SIGN,
            3 => ph::MAP_PIN,
            4 => ph::ROWS,
            5 => ph::WARNING_DIAMOND,
            6 => ph::PLUS_CIRCLE,
            7 => ph::ARROW_ELBOW_RIGHT_UP,
            _ => ph::ARROW_CLOCKWISE,
        },
        A::Library => ph::BOOKS,
        A::AddSupports => ph::ANCHOR_SIMPLE,
        A::AddMarking => ph::ROAD_HORIZON,
        A::AddExtMarking(i) => match i {
            0 => ph::LINE_SEGMENT,
            1 => ph::ARROW_FAT_LINE_UP,
            2 => ph::LIST_DASHES,
            3 => ph::MINUS,
            4 => ph::DOTS_THREE,
            5 => ph::TEXT_T,
            _ => ph::LINE_SEGMENTS,
        },
        A::AddRoadElement(i) => match i {
            0 => ph::DROP,
            1 => ph::PIPE,
            2 => ph::BRIDGE,
            _ => ph::SIGN_OUT,
        },
        A::CheckElement => ph::SEAL_CHECK,
        A::AddLighting => ph::LIGHTBULB_FILAMENT,
        A::AddLightColumn => ph::LAMP,
        A::AddBarrier => ph::BARRICADE,
        A::AddPavement => ph::STACK,
        A::PavementDesign => ph::RULER,
        A::ImportSurface => ph::MOUNTAINS,
        A::ImportSurvey => ph::MAP_PIN_AREA,
        A::AddCorridor => ph::ROAD_HORIZON,
        A::ShowSections => ph::CHART_LINE,
        A::HangSign => ph::LINK_SIMPLE,
        A::UnhangSign => ph::LINK_SIMPLE_BREAK,
        A::DesignSupports => ph::CALCULATOR,
        A::ImportLandXml => ph::PATH,
        A::PlaceOnAlignment => ph::MAP_PIN_LINE,
        A::RemoveSupports => ph::TRASH_SIMPLE,
        A::Standard(_) => ph::BOOK_OPEN_TEXT,
        A::ClashTests => ph::SLIDERS_HORIZONTAL,
        A::ClashRun => ph::PLAY,
        A::ClashResults => ph::LIST_MAGNIFYING_GLASS,
        A::ClashMarkers => ph::MAP_PIN_SIMPLE,
        A::ClashCsv => ph::FILE_CSV,
        A::Issues => ph::CHAT_TEARDROP_TEXT,
        A::BcfOut => ph::UPLOAD_SIMPLE,
        A::BcfIn => ph::DOWNLOAD_SIMPLE,
        A::IfcRef => ph::CUBE_FOCUS,
        A::RefModels => ph::STACK_SIMPLE,
        A::Programme => ph::LIST_CHECKS,
        A::Calendar => ph::CALENDAR,
        A::ScheduleRun => ph::TIMER,
        A::Gantt => ph::CHART_BAR_HORIZONTAL,
        A::SetBaseline => ph::FLAG_BANNER,
        A::Variance => ph::PLUS_MINUS,
        A::Progress => ph::CHART_LINE_UP,
        A::Link4d => ph::LINK,
        A::LinkReport => ph::LIST_BULLETS,
        A::FourD => ph::CLOCK_COUNTDOWN,
        A::MspIn => ph::FILE_ARROW_DOWN,
        A::MspOut => ph::FILE_ARROW_UP,
        A::ProgCsv => ph::FILE_CSV,
        A::XerIn => ph::FILE_ARROW_DOWN,
        A::XerOut => ph::FILE_ARROW_UP,
        A::Wbs => ph::TREE_STRUCTURE,
        A::CriticalBasis => ph::FLOW_ARROW,
        A::TimeLocation => ph::CHART_SCATTER,
        A::Play4d => ph::PLAY_CIRCLE,
        A::Bill => ph::RECEIPT,
        A::Rates => ph::COINS,
        A::Currency => ph::CURRENCY_CIRCLE_DOLLAR,
        A::Estimate => ph::CALCULATOR,
        A::CashFlow => ph::CHART_LINE,
        A::Certificate => ph::CERTIFICATE,
        A::CostCsv => ph::FILE_CSV,
        A::CheckAll => ph::LIST_CHECKS,
        A::Rational => ph::CLOUD_RAIN,
        A::Visibility => ph::EYE,
        A::VisibilityRun => ph::SCAN,
        A::Photometry => ph::SUN_HORIZON,
        A::ReportPack => ph::FILE_PDF,
        A::SiteRecord => ph::NOTE_PENCIL,
        A::SiteLog => ph::LOG,
        A::RollUp => ph::ARROWS_MERGE,
        A::Evm => ph::GAUGE,
        A::LookAhead => ph::BINOCULARS,
        A::SiteView => ph::PAINT_BUCKET,
        A::EvmCsv => ph::FILE_CSV,
        A::SheetSets => ph::FILES,
        A::SheetPreview => ph::PRESENTATION,
        A::TitleBlock => ph::SQUARE_HALF_BOTTOM,
        A::ProjectInfo => ph::IDENTIFICATION_CARD,
        A::SheetPdf => ph::FILE_PDF,
        A::SheetDxf => ph::FILE_DASHED,
        A::AlignDesign => ph::BEZIER_CURVE,
        A::AlignEdit => ph::PENCIL_SIMPLE_LINE,
        A::PiTable => ph::TABLE,
        A::AlignLabels => ph::TEXT_AA,
        A::ProfileDesign => ph::CHART_LINE_UP,
        A::AlignCheck => ph::SEAL_CHECK,
        A::LandXmlOut => ph::EXPORT,
        A::Options => ph::GIT_DIFF,
        A::Anno(k) => match k {
            Kind::Text => ph::TEXT_T,
            Kind::MText => ph::TEXTBOX,
            Kind::Dim | Kind::DimLinear => ph::ARROWS_HORIZONTAL,
            Kind::DimAligned => ph::RULER,
            Kind::DimAngular => ph::ANGLE,
            Kind::DimRadius => ph::CIRCLE_NOTCH,
            Kind::DimDiameter => ph::CIRCLE_DASHED,
            Kind::DimArc => ph::ARROW_ARC_RIGHT,
            Kind::DimContinue => ph::ARROWS_LEFT_RIGHT,
            Kind::DimBaseline => ph::ALIGN_LEFT_SIMPLE,
            Kind::DimStation => ph::MAP_PIN_LINE,
            Kind::DimLevel => ph::ARROWS_DOWN_UP,
            Kind::Leader => ph::ARROW_BEND_UP_LEFT,
            Kind::MLeader => ph::ARROW_ELBOW_UP_LEFT,
            Kind::Dist => ph::RULER,
            Kind::MeasureAngle => ph::ANGLE,
            Kind::MeasureRadius => ph::CIRCLE_NOTCH,
            Kind::Area => ph::POLYGON,
            Kind::Id => ph::CROSSHAIR,
        },
        A::TextEdit => ph::PENCIL_SIMPLE,
        A::MeasureResults => ph::LIST_NUMBERS,
        A::TextStyles => ph::TEXT_AA,
        A::DimStyles => ph::SLIDERS,
        A::ToggleOsnap => ph::CROSSHAIR_SIMPLE,
        A::OsnapSettings => ph::GEAR_SIX,
        A::Layers => ph::STACK,
        A::Units => ph::RULER,
        A::ProjectSettings => ph::GEAR,
        A::Underlay => ph::IMAGE_SQUARE,
        A::SignSchedule => ph::FILE_ARROW_DOWN,
        A::SignScheduleOut => ph::FILE_ARROW_UP,
        A::SignScheduleTemplate => ph::FILE_DOTTED,
        A::Help => ph::QUESTION,
        A::About => ph::INFO,
        A::Quit => ph::SIGN_OUT,
        A::Appearance => ph::PALETTE,
        A::Palette => ph::COMMAND,
    })
}

/// The icon of a command: its glyph, or a plain square when none is chosen.
pub fn of(info: &CommandInfo) -> &'static str {
    glyph(info.action).unwrap_or(ph::SQUARE)
}

/// The icon of a command by name or alias.
pub fn by_name(name: &str) -> &'static str {
    crate::commands::resolve(name).map(of).unwrap_or(ph::SQUARE)
}

/// Icons for the interface itself (menus, the footer, callouts), not for a command.
#[allow(dead_code)]
pub mod ui {
    use egui_phosphor::regular as ph;
    pub const INFO: &str = ph::INFO;
    pub const WARNING: &str = ph::WARNING;
    pub const ERROR: &str = ph::WARNING_OCTAGON;
    pub const OK: &str = ph::CHECK_CIRCLE;
    pub const CLOSE: &str = ph::X;
    pub const CHEVRON_RIGHT: &str = ph::CARET_RIGHT;
    pub const CHEVRON_DOWN: &str = ph::CARET_DOWN;
    pub const SEARCH: &str = ph::MAGNIFYING_GLASS;
    pub const WINDOW: &str = ph::APP_WINDOW;
    pub const TRASH: &str = ph::TRASH_SIMPLE;
    pub const PLUS: &str = ph::PLUS;
    pub const EYE: &str = ph::EYE;
    pub const EYE_OFF: &str = ph::EYE_SLASH;
    pub const LOCK: &str = ph::LOCK_SIMPLE;
    pub const UNLOCK: &str = ph::LOCK_SIMPLE_OPEN;
    pub const CHECK: &str = ph::CHECK;
    pub const HELP: &str = ph::QUESTION;
    pub const FOLDER: &str = ph::FOLDER_OPEN;
    pub const WORKSPACE: &str = ph::SQUARES_FOUR;
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::commands::COMMANDS;

    #[test]
    fn every_command_has_a_glyph_of_its_own_choosing() {
        for c in COMMANDS {
            assert!(glyph(c.action).is_some(), "{} has no icon", c.name);
        }
    }
}
