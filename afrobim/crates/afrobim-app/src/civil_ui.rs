//! The dialogs of the civil commands and the dispatcher that opens them.

use crate::app::AfroBimApp;
use crate::civil_cmds::Cv;

#[derive(Default)]
pub struct CivilUi {
    pub tree: crate::civil_tree::TreeState,
}

impl AfroBimApp {
    /// Runs a civil command.
    pub(crate) fn civil_run(&mut self, _ctx: &egui::Context, cv: Cv) {
        match cv {
            Cv::Toolspace => self.show_left = !self.show_left,
            other => self.status = format!("{other:?} is not available yet."),
        }
    }

    pub(crate) fn civil_windows(&mut self, _ctx: &egui::Context) {}
}
