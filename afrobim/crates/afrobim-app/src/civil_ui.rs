// SPDX-License-Identifier: LicenseRef-AfroBIM-Proprietary
// Copyright (c) 2026 AfroBIM. All rights reserved.
//! The dialogs of the civil commands and the dispatcher that opens them.

use crate::app::AfroBimApp;
use crate::civil_cmds::Cv;

#[derive(Default)]
pub struct CivilUi {
    pub tree: crate::civil_tree::TreeState,
    pub points: crate::civil_points::PointsUi,
    pub surf: crate::civil_surface::SurfaceUi,
    pub prof: crate::civil_profile::ProfileUi,
    pub asm: crate::civil_assembly::AssemblyUi,
    pub cor: crate::civil_corridor::CorridorUi,
    pub parcels: crate::civil_parcels::ParcelUi,
    pub qty: crate::civil_quantities::QtyUi,
    pub pipes: crate::pipes_ui::PipesUi,
    pub grade: crate::grading_ui::GradeUi,
    pub junction: crate::junction_ui::JunctionUi,
    pub transform: crate::transform_ui::TransformUi,
    pub sdb: crate::sdb_ui::SdbUi,
    pub cloud: crate::cloud_ui::CloudUi,
    pub shortcuts: crate::shortcuts_ui::ShortcutsUi,
    /// Positions picked in the model for the dialog that asked (easting, northing).
    pub queue: Vec<(f64, f64)>,
    /// Keep picking after each click (until the dialog closes).
    pub pick_repeat: bool,
    /// The next click in the model is a point for this dialog.
    pub pick: Option<PickFor>,
    /// Temporary drawing in the model (a flow path): not saved.
    pub overlay: crate::civil_scene::Scene,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PickFor {
    Create,
    Spot,
    Flow,
    ParcelCorner,
    /// Each click is added to `CivilUi::queue` for the open dialog.
    Queue,
}

impl AfroBimApp {
    /// Runs a civil command.
    pub(crate) fn civil_run(&mut self, _ctx: &egui::Context, cv: Cv) {
        match cv {
            Cv::Toolspace => self.show_left = !self.show_left,
            Cv::CivilLandXml => self.civil_landxml_out(),
            Cv::CivilDemo => self.civil_demo(),
            Cv::Defaults => self.defaults_cmd(),
            Cv::PipeNew => self.pipe_new_cmd(),
            Cv::PipeParts => self.pipe_parts_cmd(),
            Cv::PipeProps => self.pipe_props_cmd(),
            Cv::PipeEdit => self.pipe_edit_cmd(),
            Cv::PipeCheck => self.pipe_check_cmd(),
            Cv::PipeTables => self.pipe_tables_cmd(),
            Cv::PipeQty => self.pipe_qty_cmd(),
            Cv::PipeProfile => self.pipe_profile_cmd(),
            Cv::FeatureLine | Cv::FeatureLineEdit | Cv::GradeSite | Cv::GradeCreate | Cv::GradeGroup | Cv::GradeHelpers => self.grading_cmd(cv),
            Cv::Intersection | Cv::Roundabout => self.junction_cmd(cv),
            Cv::CloudAttach | Cv::CloudProps | Cv::CloudClip | Cv::CloudSlice | Cv::CloudSurface | Cv::CloudCompare | Cv::CloudExport => self.cloud_cmd(cv),
            Cv::ShortcutsManager | Cv::ShortcutFolder | Cv::ShortcutPublish | Cv::ShortcutReference | Cv::ShortcutSync | Cv::ShortcutPromote | Cv::ShortcutRepair | Cv::ShortcutValidate | Cv::ShortcutUnderlay => self.shortcuts_cmd(cv),
            Cv::Transform => self.transform_cmd(),
            Cv::Calibrate => self.calibrate_cmd(),
            Cv::SurveyNew | Cv::SurveyOpen | Cv::SurveySettings | Cv::SurveyImport | Cv::SurveyObs | Cv::SurveyTraverse | Cv::SurveyAdjust | Cv::SurveyApply | Cv::SurveyFigures | Cv::SurveyInsert | Cv::SurveyUpdate | Cv::SurveyEvents => self.sdb_cmd(cv),
            Cv::ParcelPick | Cv::ParcelDeed | Cv::ParcelProps | Cv::ParcelTopology | Cv::ParcelSplit | Cv::ParcelSchedule => self.civil_parcel_cmd(cv),
            Cv::Earthworks => self.civil_quantities_cmd(),
            Cv::Assembly => self.civil_assembly_cmd(),
            Cv::CorridorModel | Cv::SampleLines | Cv::SectionViews => self.civil_corridor_cmd(cv),
            Cv::GroundProfile | Cv::ProfileView => self.civil_profile_cmd(cv),
            Cv::SurfaceNew | Cv::SurfaceDef | Cv::SurfaceStyle | Cv::SpotElev | Cv::FlowPath | Cv::Volumes => self.civil_surface_cmd(cv),
            Cv::Points | Cv::PointCreate | Cv::PointTable | Cv::PointGroups | Cv::DescKeys | Cv::PointExport | Cv::Cogo | Cv::Traverse => self.civil_points_cmd(cv),
        }
    }

    pub(crate) fn civil_windows(&mut self, ctx: &egui::Context) {
        self.civil_points_windows(ctx);
        self.civil_surface_windows(ctx);
        self.civil_profile_windows(ctx);
        self.civil_assembly_windows(ctx);
        self.civil_corridor_windows(ctx);
        self.civil_quantities_windows(ctx);
        self.civil_parcel_windows(ctx);
        self.pipes_windows(ctx);
        self.grading_windows(ctx);
        self.junction_windows(ctx);
        self.transform_windows(ctx);
        self.sdb_windows(ctx);
        self.cloud_windows(ctx);
        self.shortcuts_windows(ctx);
    }
}
