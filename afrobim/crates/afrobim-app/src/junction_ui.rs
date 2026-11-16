//! Intersection dialogs (stub, replaced below).
use crate::app::AfroBimApp;
use crate::civil_cmds::Cv;

#[derive(Default)]
pub struct JunctionUi {}

impl AfroBimApp {
    pub(crate) fn junction_cmd(&mut self, _cv: Cv) {}
    pub(crate) fn junction_windows(&mut self, _ctx: &egui::Context) {}
}
