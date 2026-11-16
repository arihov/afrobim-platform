//! The dialogs of the civil commands and the dispatcher that opens them.

use crate::app::AfroBimApp;
use crate::civil_cmds::Cv;

#[derive(Default)]
pub struct CivilUi {
    pub tree: crate::civil_tree::TreeState,
    pub points: crate::civil_points::PointsUi,
    pub surf: crate::civil_surface::SurfaceUi,
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
}

impl AfroBimApp {
    /// Runs a civil command.
    pub(crate) fn civil_run(&mut self, _ctx: &egui::Context, cv: Cv) {
        match cv {
            Cv::Toolspace => self.show_left = !self.show_left,
            Cv::CivilDemo => self.civil_demo(),
            Cv::SurfaceNew | Cv::SurfaceDef | Cv::SurfaceStyle | Cv::SpotElev | Cv::FlowPath | Cv::Volumes => self.civil_surface_cmd(cv),
            Cv::Points | Cv::PointCreate | Cv::PointTable | Cv::PointGroups | Cv::DescKeys | Cv::PointExport | Cv::Cogo | Cv::Traverse => self.civil_points_cmd(cv),
            other => self.status = format!("{other:?} is not available yet."),
        }
    }

    pub(crate) fn civil_windows(&mut self, ctx: &egui::Context) {
        self.civil_points_windows(ctx);
        self.civil_surface_windows(ctx);
    }
}
