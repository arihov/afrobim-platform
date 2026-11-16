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
}

impl AfroBimApp {
    /// Runs a civil command.
    pub(crate) fn civil_run(&mut self, _ctx: &egui::Context, cv: Cv) {
        match cv {
            Cv::Toolspace => self.show_left = !self.show_left,
            Cv::CivilLandXml => self.civil_landxml_out(),
            Cv::CivilDemo => self.civil_demo(),
            Cv::ParcelPick | Cv::ParcelDeed | Cv::ParcelProps | Cv::ParcelTopology | Cv::ParcelSplit | Cv::ParcelSchedule => self.civil_parcel_cmd(cv),
            Cv::Earthworks => self.civil_quantities_cmd(),
            Cv::Assembly => self.civil_assembly_cmd(),
            Cv::CorridorModel | Cv::SampleLines | Cv::SectionViews => self.civil_corridor_cmd(cv),
            Cv::GroundProfile | Cv::ProfileView => self.civil_profile_cmd(cv),
            Cv::SurfaceNew | Cv::SurfaceDef | Cv::SurfaceStyle | Cv::SpotElev | Cv::FlowPath | Cv::Volumes => self.civil_surface_cmd(cv),
            Cv::Points | Cv::PointCreate | Cv::PointTable | Cv::PointGroups | Cv::DescKeys | Cv::PointExport | Cv::Cogo | Cv::Traverse => self.civil_points_cmd(cv),
            other => self.status = format!("{other:?} is not available yet."),
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
    }
}
