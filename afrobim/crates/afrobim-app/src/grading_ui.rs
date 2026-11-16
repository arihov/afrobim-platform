//! Grading dialogs (stub, replaced below).
use crate::app::AfroBimApp;
use crate::civil_cmds::Cv;

#[derive(Default)]
pub struct GradeUi {}

impl AfroBimApp {
    pub(crate) fn grading_cmd(&mut self, _cv: Cv) {}
    pub(crate) fn grading_windows(&mut self, _ctx: &egui::Context) {}
}
