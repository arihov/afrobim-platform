//! What the views read: the live document, its modules, and station helpers.

use afrobim_alignment::{Alignment, StationDirection};
use afrobim_kernel::{Document, Element, ElementId, ModuleRegistry};

use crate::style::Styles;

/// The live model a sheet is composed from.
pub struct Model<'a> {
    pub doc: &'a Document,
    /// The modules that turn elements into drawings. Elements whose category has no module here
    /// are left off plan views.
    pub registry: &'a ModuleRegistry,
}

impl Model<'_> {
    /// The element of `category` called `name`.
    pub fn find(&self, category: &str, name: &str) -> Option<&Element> {
        self.doc.elements().iter().find(|e| e.category == category && e.name == name)
    }

    /// The alignment element called `name`, read from its data.
    pub fn alignment(&self, name: &str) -> Result<Alignment, String> {
        let e = self.find(afrobim_alignment::CATEGORY, name).ok_or_else(|| format!("There is no alignment called '{name}' in the document."))?;
        serde_json::from_value::<Alignment>(e.data.clone()).map_err(|err| format!("Alignment '{name}' cannot be read: {err}"))
    }

    /// The id of the element of `category` called `name`.
    pub fn id_of(&self, category: &str, name: &str) -> Option<ElementId> {
        self.find(category, name).map(|e| e.id)
    }
}

/// What a view needs besides the model: the pens and text heights.
pub struct Ctx<'a> {
    pub model: &'a Model<'a>,
    pub styles: &'a Styles,
}

/// The internal stations between `from_m` and `to_m` at which the *displayed* station is a whole
/// multiple of `interval_m`, in order along the alignment. Station equations are honoured: in each
/// region the displayed station is the internal one shifted (or mirrored for a decreasing
/// equation), so the ticks of a region stand on its own multiples. A station that lies on an
/// equation appears once, in the region ahead of it.
pub fn stations_at_interval(a: &Alignment, from_m: f64, to_m: f64, interval_m: f64) -> Vec<f64> {
    if !(interval_m > 0.0 && interval_m.is_finite() && to_m >= from_m) {
        return Vec::new();
    }
    let eqs = &a.station_equations.equations;
    let mut out: Vec<f64> = Vec::new();
    for r in 0..=eqs.len() {
        let lo = if r == 0 { a.start_station_m } else { eqs[r - 1].internal_m };
        let hi = if r < eqs.len() { eqs[r].internal_m } else { a.end_station_m() };
        let (lo, hi) = (lo.max(from_m), hi.min(to_m));
        if hi - lo < 1e-9 {
            continue;
        }
        let mid = (lo + hi) / 2.0;
        let sign = if r == 0 || eqs[r - 1].direction == StationDirection::Increasing { 1.0 } else { -1.0 };
        let disp_mid = a.station_equations.to_displayed(mid).station_m;
        let (d_a, d_b) = (disp_mid + sign * (lo - mid), disp_mid + sign * (hi - mid));
        let (dmin, dmax) = (d_a.min(d_b), d_a.max(d_b));
        let k0 = (dmin / interval_m - 1e-9).ceil() as i64;
        let k1 = (dmax / interval_m + 1e-9).floor() as i64;
        for k in k0..=k1 {
            let d = k as f64 * interval_m;
            let s = mid + sign * (d - disp_mid);
            // A station on the end of the region belongs to the next region when there is one.
            if r < eqs.len() && (s - hi).abs() < 1e-9 && hi < to_m - 1e-9 {
                continue;
            }
            out.push(s.clamp(lo, hi));
        }
    }
    out.sort_by(f64::total_cmp);
    out.dedup_by(|a, b| (*a - *b).abs() < 1e-6);
    out
}

/// The displayed-station text of an internal station, with `decimals` decimals.
pub fn station_text(a: &Alignment, internal_m: f64, decimals: usize) -> String {
    a.station_label(internal_m, decimals)
}

#[cfg(test)]
mod tests {
    use super::*;
    use afrobim_alignment::{AlignmentBuilder, StationEquation, StationEquations};

    fn straight() -> Alignment {
        AlignmentBuilder::new("A", 0.0, 0.0, 0.0, 0.0).line(1000.0).build().unwrap()
    }

    #[test]
    fn ticks_stand_on_whole_multiples() {
        let a = straight();
        let s = stations_at_interval(&a, 30.0, 175.0, 50.0);
        assert_eq!(s, vec![50.0, 100.0, 150.0]);
        assert_eq!(stations_at_interval(&a, 0.0, 100.0, 20.0), vec![0.0, 20.0, 40.0, 60.0, 80.0, 100.0]);
        assert!(stations_at_interval(&a, 0.0, 100.0, 0.0).is_empty());
    }

    #[test]
    fn ticks_follow_displayed_stations_through_an_equation() {
        // At internal 500 the displayed station becomes 1000: internal 600 reads 1100.
        let mut a = straight();
        a.station_equations = StationEquations::new(vec![StationEquation::new(500.0, 1000.0)]).unwrap();
        let s = stations_at_interval(&a, 400.0, 700.0, 100.0);
        // Before the equation 400 and (just) 500 read 400 and 500; ahead of it displayed 1000,
        // 1100 and 1200 stand at internal 500, 600 and 700. The station on the equation appears once.
        assert_eq!(s, vec![400.0, 500.0, 600.0, 700.0]);
        assert_eq!(station_text(&a, 600.0, 0), "1+100");
        // Multiples of 1000 over internal 0 to 700: displayed 0 at internal 0, and displayed 1000
        // (ahead of the equation) at internal 500.
        assert_eq!(stations_at_interval(&a, 0.0, 700.0, 1000.0), vec![0.0, 500.0]);
    }
}
