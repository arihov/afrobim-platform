//! Take-off for gantries, by geometry only. No rates.
//!
//! Every figure is the entered dimensions put through the formula stated on its field. Steel
//! mass is reported only where the engineer entered a mass per metre for the member: it is never
//! estimated from an envelope and a density.

use afrobim_kernel::Element;

use crate::{fixed, GantryDefinition, GantryError, GantryType, GANTRY_CATEGORY};

/// One gantry measured.
#[derive(Debug, Clone, PartialEq)]
pub struct GantryQuantities {
    pub mark: String,
    pub gantry_type: GantryType,
    pub alignment: String,
    pub station_m: f64,
    pub skew_deg: f64,
    /// Portal: clear span between the legs' inner faces. Cantilever: outreach from the traffic
    /// face of the leg to the tip. Measured along the gantry line, so a skewed gantry is longer
    /// than the square distance by 1 / cos(skew).
    pub span_or_reach_mm: f64,
    /// Envelope length of each leg, in order of increasing offset: base to top of boom.
    pub leg_lengths_mm: Vec<f64>,
    /// Height of each support as CD 365 Revision 1 clause 6.9 measures it: base (top of plinth)
    /// to underside of boom.
    pub support_heights_mm: Vec<f64>,
    /// Envelope length of the boom: equal to `span_or_reach_mm`.
    pub boom_length_mm: f64,
    pub sign_count: usize,
    /// Sum of width × height of the carried signs' bounding rectangles, m².
    pub sign_area_m2: f64,
    /// Sum of length × width × depth of the entered foundation blocks, m³.
    pub foundation_concrete_m3: f64,
    /// Σ (leg length × its entered kg/m) + boom length × its entered kg/m. `None` unless every
    /// member has a mass per metre entered.
    pub steel_mass_kg: Option<f64>,
}

impl GantryQuantities {
    /// Measures one gantry. Fails when the definition has Error notes.
    pub fn measure(mark: &str, def: &GantryDefinition) -> Result<GantryQuantities, GantryError> {
        def.check()?;
        let legs = def.legs_sorted();
        let boom_length_mm = def.boom_length_mm();
        let leg_mass: Option<f64> = legs.iter().map(|l| l.mass_kg_per_m.map(|m| m * def.leg_length_mm(l) / 1000.0)).sum();
        let boom_mass = def.boom.mass_kg_per_m.map(|m| m * boom_length_mm / 1000.0);
        Ok(GantryQuantities {
            mark: mark.to_string(),
            gantry_type: def.gantry_type,
            alignment: def.alignment.clone(),
            station_m: def.station_m,
            skew_deg: def.skew_deg,
            span_or_reach_mm: boom_length_mm,
            leg_lengths_mm: legs.iter().map(|l| def.leg_length_mm(l)).collect(),
            support_heights_mm: legs.iter().map(|l| def.support_height_mm(l)).collect(),
            boom_length_mm,
            sign_count: def.signs.len(),
            sign_area_m2: def.signs.iter().map(|s| s.width_mm * s.height_mm).sum::<f64>() / 1.0e6,
            foundation_concrete_m3: legs.iter().filter_map(|l| l.foundation).map(|f| f.volume_m3()).sum(),
            steel_mass_kg: leg_mass.zip(boom_mass).map(|(a, b)| a + b),
        })
    }

    /// Measures every valid `gantry` element among `elements`, in order. Elements of other
    /// categories are ignored; gantries that cannot be measured are returned by name with the
    /// reason.
    pub fn measure_all<'a>(elements: impl IntoIterator<Item = &'a Element>) -> (Vec<GantryQuantities>, Vec<(String, String)>) {
        let (mut rows, mut skipped) = (Vec::new(), Vec::new());
        for e in elements.into_iter().filter(|e| e.category == GANTRY_CATEGORY) {
            match GantryDefinition::from_value(&e.data).and_then(|d| GantryQuantities::measure(&e.name, &d)) {
                Ok(q) => rows.push(q),
                Err(err) => skipped.push((e.name.clone(), err.to_string())),
            }
        }
        (rows, skipped)
    }

    pub fn leg_length_total_mm(&self) -> f64 {
        self.leg_lengths_mm.iter().sum()
    }
}

pub const GANTRIES_CSV_HEADER: &str = "mark,type,alignment,station_m,skew_deg,span_or_reach_mm,legs,leg_lengths_mm,support_heights_mm,leg_length_total_mm,boom_length_mm,signs,sign_area_m2,foundation_concrete_m3,steel_mass_kg";

/// Gantry schedule, one row per gantry, then a `TOTAL` row carrying the number of gantries in
/// the second column and the summed leg length, boom length, signs, sign area, concrete and
/// steel mass. Lines end with `\n`.
///
/// Several legs share a cell separated by `;`. Rounding: mm to at most 1 decimal, stations to 3,
/// areas and volumes to 3, mass to 1. Totals are sums of the unrounded values, rounded once.
/// `steel_mass_kg` is empty for a gantry without a mass per metre on every member, and the total
/// is empty unless every gantry has one.
pub fn gantries_to_csv(rows: &[GantryQuantities]) -> String {
    let list = |v: &[f64]| v.iter().map(|x| fixed(*x, 0, 1)).collect::<Vec<_>>().join(";");
    let mut out = String::from(GANTRIES_CSV_HEADER);
    out.push('\n');
    for r in rows {
        let fields = [
            csv(&r.mark),
            r.gantry_type.label().to_string(),
            csv(&r.alignment),
            fixed(r.station_m, 3, 3),
            fixed(r.skew_deg, 0, 3),
            fixed(r.span_or_reach_mm, 0, 1),
            r.leg_lengths_mm.len().to_string(),
            list(&r.leg_lengths_mm),
            list(&r.support_heights_mm),
            fixed(r.leg_length_total_mm(), 0, 1),
            fixed(r.boom_length_mm, 0, 1),
            r.sign_count.to_string(),
            fixed(r.sign_area_m2, 3, 3),
            fixed(r.foundation_concrete_m3, 3, 3),
            r.steel_mass_kg.map_or(String::new(), |m| fixed(m, 1, 1)),
        ];
        out.push_str(&fields.join(","));
        out.push('\n');
    }
    let total_mass: Option<f64> = rows.iter().map(|r| r.steel_mass_kg).sum();
    out.push_str(&format!(
        "TOTAL,{},,,,,{},,,{},{},{},{},{},{}\n",
        rows.len(),
        rows.iter().map(|r| r.leg_lengths_mm.len()).sum::<usize>(),
        fixed(rows.iter().map(|r| r.leg_length_total_mm()).sum::<f64>(), 0, 1),
        fixed(rows.iter().map(|r| r.boom_length_mm).sum::<f64>(), 0, 1),
        rows.iter().map(|r| r.sign_count).sum::<usize>(),
        fixed(rows.iter().map(|r| r.sign_area_m2).sum::<f64>(), 3, 3),
        fixed(rows.iter().map(|r| r.foundation_concrete_m3).sum::<f64>(), 3, 3),
        total_mass.filter(|_| !rows.is_empty()).map_or(String::new(), |m| fixed(m, 1, 1)),
    ));
    out
}

fn csv(s: &str) -> String {
    if s.contains([',', '"', '\n']) {
        format!("\"{}\"", s.replace('"', "\"\""))
    } else {
        s.to_string()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testing::*;
    use crate::SignPosition;

    #[test]
    fn portal_quantities_by_hand() {
        let mut d = portal();
        d.signs.push(mount(1, SignPosition::Lane { index: 1 }, 0.0));
        d.signs.push(mount(2, SignPosition::Lane { index: 2 }, 0.0));
        let q = GantryQuantities::measure("G1", &d).unwrap();
        assert_eq!(q.gantry_type, GantryType::Portal);
        assert!(close(q.span_or_reach_mm, 17_400.0) && close(q.boom_length_mm, 17_400.0));
        assert_eq!(q.leg_lengths_mm, [7000.0, 6500.0]);
        assert_eq!(q.support_heights_mm, [6200.0, 5700.0]);
        // Two signs of 3000 × 2000 = 12 m².
        assert_eq!((q.sign_count, q.sign_area_m2), (2, 12.0));
        // Two blocks of 3.0 × 2.0 × 1.5 = 9 m³ each.
        assert!(close(q.foundation_concrete_m3, 18.0));
        // No mass per metre entered: no mass, and no density × envelope guess.
        assert_eq!(q.steel_mass_kg, None);
    }

    #[test]
    fn steel_mass_only_when_every_member_has_a_mass_per_metre() {
        let mut d = portal();
        d.legs[0].mass_kg_per_m = Some(200.0);
        d.boom.mass_kg_per_m = Some(150.0);
        assert_eq!(GantryQuantities::measure("G1", &d).unwrap().steel_mass_kg, None);
        d.legs[1].mass_kg_per_m = Some(200.0);
        // Legs: 200 × (7.0 + 6.5) = 2700 kg; boom: 150 × 17.4 = 2610 kg.
        assert!(close(GantryQuantities::measure("G1", &d).unwrap().steel_mass_kg.unwrap(), 5310.0));
    }

    #[test]
    fn skew_lengthens_the_boom_and_cantilever_reports_outreach() {
        let mut d = portal();
        d.skew_deg = 60.0;
        assert!(close(GantryQuantities::measure("G1", &d).unwrap().boom_length_mm, 35_400.0));
        let q = GantryQuantities::measure("C1", &cantilever()).unwrap();
        assert!(close(q.span_or_reach_mm, 7750.0));
        assert_eq!((q.leg_lengths_mm.clone(), q.foundation_concrete_m3), (vec![6800.0], 0.0));
    }

    #[test]
    fn csv_schedule() {
        let mut p = portal();
        p.legs.iter_mut().for_each(|l| l.mass_kg_per_m = Some(200.0));
        p.boom.mass_kg_per_m = Some(150.0);
        p.signs.push(mount(1, SignPosition::Lane { index: 1 }, 0.0));
        let rows = vec![GantryQuantities::measure("G1", &p).unwrap(), GantryQuantities::measure("C, 1", &cantilever()).unwrap()];
        let text = gantries_to_csv(&rows);
        let lines: Vec<&str> = text.lines().collect();
        assert_eq!(lines[0], GANTRIES_CSV_HEADER);
        assert_eq!(lines[1], "G1,portal,A1,100.000,0,17400,2,7000;6500,6200;5700,13500,17400,1,6.000,18.000,5310.0");
        assert_eq!(lines[2], "\"C, 1\",cantilever,A1,100.000,0,7750,1,6800,6000,6800,7750,0,0.000,0.000,");
        // The total mass is empty because one gantry has none.
        assert_eq!(lines[3], "TOTAL,2,,,,,3,,,20300,25150,1,6.000,18.000,");
        assert_eq!(lines.len(), 4);
        assert!(text.ends_with('\n'));
        assert_eq!(lines[0].split(',').count(), lines[3].split(',').count());
        assert_eq!(gantries_to_csv(&rows[..1]).lines().last().unwrap(), "TOTAL,1,,,,,2,,,13500,17400,1,6.000,18.000,5310.0");
    }
}
