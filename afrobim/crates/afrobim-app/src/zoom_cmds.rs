//! The zoom commands that need the whole application: zoom to the selection or to one object
//! (from the model tree and the right-click menu), and the previous view, in 2D and in 3D.

use afrobim_kernel::ElementId;

use crate::app::AfroBimApp;

/// What one screen pixel is on the model, for the status bar: "1 px = 2.5 mm", "1 px = 1.20 m",
/// "1 px = 0.0031 mm" or "1 px = 4.2 um". There is no zoom limit, so this reads in any range.
pub(crate) fn pixel_size_text(scale: f64) -> String {
    if !(scale.is_finite() && scale > 0.0) {
        return "1 px = ?".into();
    }
    let mm = 1.0 / scale;
    let three = |v: f64| {
        let digits = (2 - v.abs().log10().floor() as i32).clamp(0, 12) as usize;
        format!("{v:.digits$}")
    };
    if mm >= 1.0e6 {
        format!("1 px = {} km", three(mm / 1.0e6))
    } else if mm >= 1.0e3 {
        format!("1 px = {} m", three(mm / 1.0e3))
    } else if mm >= 1.0 {
        format!("1 px = {} mm", three(mm))
    } else if mm >= 1.0e-3 {
        format!("1 px = {} mm", three(mm))
    } else if mm >= 1.0e-6 {
        format!("1 px = {} um", three(mm * 1.0e3))
    } else {
        format!("1 px = {} nm", three(mm * 1.0e6))
    }
}

#[cfg(test)]
mod tests {
    use super::pixel_size_text;

    #[test]
    fn the_pixel_size_reads_in_every_range() {
        assert_eq!(pixel_size_text(0.4), "1 px = 2.50 mm");
        assert_eq!(pixel_size_text(1.0e-3), "1 px = 1.00 m");
        assert_eq!(pixel_size_text(1.0e-8), "1 px = 100 km");
        assert_eq!(pixel_size_text(1000.0), "1 px = 0.00100 mm");
        assert_eq!(pixel_size_text(2000.0), "1 px = 0.500 um");
        assert_eq!(pixel_size_text(2.0e6), "1 px = 0.500 nm");
        assert_eq!(pixel_size_text(f64::NAN), "1 px = ?");
    }
}

impl AfroBimApp {
    /// ZS: zooms to the selected objects. Says so on the command line when nothing is selected.
    pub(crate) fn zoom_selection(&mut self) {
        let ids: Vec<ElementId> = self.doc.selection().to_vec();
        if ids.is_empty() {
            self.log("Zoom selection: nothing is selected. Select objects first, or use ZOOM for the extents.".to_string());
            return;
        }
        self.zoom_to_ids(&ids);
    }

    fn zoom_to_ids(&mut self, ids: &[ElementId]) {
        if self.view_3d {
            if !self.zoom_3d_to(ids) {
                self.log("Zoom: the object has nothing to show in 3D.".to_string());
            }
            return;
        }
        let bounds = ids.iter().filter_map(|id| self.doc.find(*id)).map(|e| e.bounds()).reduce(|a, b| a.union(&b));
        match bounds {
            Some(b) => self.ctl.zoom_to_bounds(&b, &mut self.view, 60.0),
            None => self.log("Zoom: the object no longer exists.".to_string()),
        }
    }

    /// ZP: back to the view before the last zoom.
    pub(crate) fn zoom_previous_view(&mut self) {
        let done = if self.view_3d {
            match self.camera_history.pop() {
                Some(c) => {
                    self.camera = c;
                    true
                }
                None => false,
            }
        } else {
            self.ctl.zoom_previous(&mut self.view)
        };
        if !done {
            self.log("Zoom previous: there is no earlier view.".to_string());
        }
    }
}
