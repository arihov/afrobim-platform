//! Take-off and column schedule. Counts and sums of entered data only: no rates, no cable or
//! electrical quantities, nothing from a product table.

use afrobim_kernel::{Document, Element};

use crate::model::LightingData;
use crate::module::CATEGORY;
use crate::{error_text, fixed};

/// One column in the schedule.
#[derive(Debug, Clone, PartialEq)]
pub struct ScheduleRow {
    pub mark: String,
    /// Name of the element the column belongs to.
    pub element: String,
    /// Displayed station, e.g. `1+250.000`.
    pub station: String,
    pub side: String,
    pub offset_m: f64,
    pub easting_m: f64,
    pub northing_m: f64,
    pub level_m: Option<f64>,
    pub mounting_height_m: f64,
    pub outreach_m: f64,
    pub luminaire: String,
    /// Luminaires on this column.
    pub luminaires: usize,
    /// Entered power of those luminaires together, W.
    pub power_w: f64,
}

/// Columns of one mounting height in one arrangement.
#[derive(Debug, Clone, PartialEq)]
pub struct ColumnGroup {
    pub mounting_height_m: f64,
    pub arrangement: String,
    pub columns: usize,
}

/// Luminaires of one product and power.
#[derive(Debug, Clone, PartialEq)]
pub struct LuminaireGroup {
    pub product: String,
    /// Entered power of one luminaire, W.
    pub power_w: f64,
    pub luminaires: usize,
}

/// A lighting element left out of the take-off, and why.
#[derive(Debug, Clone, PartialEq)]
pub struct SkippedElement {
    pub name: String,
    pub reason: String,
}

/// The lighting take-off of a document.
///
/// - `schedule`: one row per stored column position, in document order.
/// - `columns`: number of columns by mounting height and arrangement. The height is the entered
///   mounting height; a separate nominal column height is not modelled.
/// - `luminaires`: number of luminaires by product name and entered power. A twin central
///   column carries two.
/// - `connected_load_w`: the sum of the entered powers of all luminaires. Luminaires only: no
///   control gear losses beyond what the entered figure includes, no signs, no cable losses, no
///   diversity and no allowance for spare capacity.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct LightingQuantities {
    pub schedule: Vec<ScheduleRow>,
    pub columns: Vec<ColumnGroup>,
    pub luminaires: Vec<LuminaireGroup>,
    pub connected_load_w: f64,
    pub skipped: Vec<SkippedElement>,
}

impl LightingQuantities {
    /// Measures every lighting element of the document.
    pub fn of_document(doc: &Document) -> LightingQuantities {
        Self::of_elements(doc.elements().iter())
    }

    /// Measures the lighting elements among `elements`; others are ignored. An element whose
    /// data cannot be read or whose definition has errors is listed in `skipped`.
    pub fn of_elements<'a>(elements: impl Iterator<Item = &'a Element>) -> LightingQuantities {
        let mut q = LightingQuantities::default();
        for e in elements.filter(|e| e.category == CATEGORY) {
            let data = match LightingData::from_value(&e.data) {
                Ok(d) => d,
                Err(err) => {
                    q.skipped.push(SkippedElement { name: e.name.clone(), reason: err.to_string() });
                    continue;
                }
            };
            if let Some(errors) = error_text(&data.definition.notes()) {
                q.skipped.push(SkippedElement { name: e.name.clone(), reason: errors });
                continue;
            }
            q.add(&e.name, &data);
        }
        q
    }

    /// Adds one element's columns.
    pub fn add(&mut self, element: &str, data: &LightingData) {
        let spec = data.definition.column();
        let arrangement = data.definition.arrangement_label();
        for c in &data.layout.columns {
            let n = c.bracket_directions_rad.len();
            self.schedule.push(ScheduleRow {
                mark: c.mark.clone(),
                element: element.to_string(),
                station: c.station_label.clone(),
                side: c.side.label().to_string(),
                offset_m: c.offset_m,
                easting_m: c.easting_m,
                northing_m: c.northing_m,
                level_m: c.level_m,
                mounting_height_m: spec.mounting_height_m,
                outreach_m: spec.bracket_outreach_m,
                luminaire: spec.luminaire.product.clone(),
                luminaires: n,
                power_w: n as f64 * spec.luminaire.power_w,
            });
            match self.columns.iter_mut().find(|g| g.mounting_height_m == spec.mounting_height_m && g.arrangement == arrangement) {
                Some(g) => g.columns += 1,
                None => self.columns.push(ColumnGroup { mounting_height_m: spec.mounting_height_m, arrangement: arrangement.to_string(), columns: 1 }),
            }
            match self.luminaires.iter_mut().find(|g| g.product == spec.luminaire.product && g.power_w == spec.luminaire.power_w) {
                Some(g) => g.luminaires += n,
                None => self.luminaires.push(LuminaireGroup { product: spec.luminaire.product.clone(), power_w: spec.luminaire.power_w, luminaires: n }),
            }
            self.connected_load_w += n as f64 * spec.luminaire.power_w;
        }
    }

    pub fn column_count(&self) -> usize {
        self.schedule.len()
    }

    pub fn luminaire_count(&self) -> usize {
        self.schedule.iter().map(|r| r.luminaires).sum()
    }
}

pub const SCHEDULE_CSV_HEADER: &str =
    "mark,element,station,side,offset_m,easting_m,northing_m,level_m,mounting_height_m,outreach_m,luminaire,luminaires_nr,power_w";
pub const SUMMARY_CSV_HEADER: &str = "item,description,unit,quantity";

fn csv(s: &str) -> String {
    if s.contains([',', '"', '\n', '\r']) {
        format!("\"{}\"", s.replace('"', "\"\""))
    } else {
        s.to_string()
    }
}

/// The column schedule as CSV: [`SCHEDULE_CSV_HEADER`], one line per column and a TOTAL line.
/// Offsets, coordinates and levels are in metres to 3 decimals; a column without a profile level
/// has an empty level.
pub fn schedule_to_csv(q: &LightingQuantities) -> String {
    let mut out = format!("{SCHEDULE_CSV_HEADER}\n");
    for r in &q.schedule {
        out.push_str(&format!(
            "{},{},{},{},{},{},{},{},{},{},{},{},{}\n",
            csv(&r.mark),
            csv(&r.element),
            csv(&r.station),
            r.side,
            fixed(r.offset_m, 3, 3),
            fixed(r.easting_m, 3, 3),
            fixed(r.northing_m, 3, 3),
            r.level_m.map_or(String::new(), |z| fixed(z, 3, 3)),
            fixed(r.mounting_height_m, 0, 3),
            fixed(r.outreach_m, 0, 3),
            csv(&r.luminaire),
            r.luminaires,
            fixed(r.power_w, 0, 1),
        ));
    }
    out.push_str(&format!("TOTAL,{},,,,,,,,,,{},{}\n", q.column_count(), q.luminaire_count(), fixed(q.connected_load_w, 0, 1)));
    out
}

/// The summary as CSV: [`SUMMARY_CSV_HEADER`], columns by height and arrangement, luminaires by
/// product, the connected load, and the number of elements not measured when there are any.
pub fn summary_to_csv(q: &LightingQuantities) -> String {
    let mut out = format!("{SUMMARY_CSV_HEADER}\n");
    for g in &q.columns {
        out.push_str(&format!(
            "lighting column,{},nr,{}\n",
            csv(&format!("{} m mounting height, {}", fixed(g.mounting_height_m, 0, 3), g.arrangement)),
            g.columns
        ));
    }
    for g in &q.luminaires {
        out.push_str(&format!("luminaire,{},nr,{}\n", csv(&format!("{}, {} W", g.product, fixed(g.power_w, 0, 1))), g.luminaires));
    }
    out.push_str(&format!("connected load,sum of entered luminaire powers,W,{}\n", fixed(q.connected_load_w, 0, 1)));
    if !q.skipped.is_empty() {
        out.push_str(&format!("lighting element,not measured (invalid definition),nr,{}\n", q.skipped.len()));
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::layout::tests::{installation, spec, straight};
    use crate::model::{Arrangement, BracketAim, SingleColumn};
    use crate::module::{add_column, add_installation};
    use afrobim_alignment::{Profile, Pvi};
    use afrobim_kernel::Pt;

    #[test]
    fn counts_load_and_csv() {
        let mut a = straight();
        a.profile = Some(Profile::new("P", vec![Pvi::point(0.0, 100.0), Pvi::point(300.0, 103.0)]).unwrap());
        let mut doc = Document::default();
        // 4 twin columns (8 luminaires of 100 W) at 10 m.
        add_installation(&mut doc, &a, installation(Arrangement::TwinCentral, 0.0, 300.0, 100.0)).unwrap();
        // 7 single columns at 8 m with a 60 W luminaire.
        let mut side = installation(Arrangement::SingleLeft, 0.0, 300.0, 50.0);
        side.reference = "Z2".to_string();
        side.mark_prefix = "LS".to_string();
        side.column.mounting_height_m = 8.0;
        side.column.luminaire.product = "Other, \"60\"".to_string();
        side.column.luminaire.power_w = 60.0;
        add_installation(&mut doc, &a, side).unwrap();
        // One junction column, 10 m, the 100 W luminaire.
        let single = SingleColumn {
            mark: "J1".to_string(),
            alignment: "A".to_string(),
            station_m: 150.0,
            offset_m: 9.0,
            aim: BracketAim::Bearing(0.0),
            column: spec(),
            lighting_class: "C2".to_string(),
        };
        add_column(&mut doc, &a, single).unwrap();
        // Not lighting: ignored. Unreadable lighting: skipped.
        doc.add("sign", "S", Pt::new(0.0, 0.0), 1.0, 1.0, "0", serde_json::Value::Null);
        doc.add(CATEGORY, "broken", Pt::new(0.0, 0.0), 1.0, 1.0, "0", serde_json::json!({}));

        let q = LightingQuantities::of_document(&doc);
        assert_eq!((q.column_count(), q.luminaire_count()), (12, 16));
        assert_eq!(q.connected_load_w, 8.0 * 100.0 + 7.0 * 60.0 + 100.0);
        assert_eq!(
            q.columns,
            vec![
                ColumnGroup { mounting_height_m: 10.0, arrangement: "twin central".to_string(), columns: 4 },
                ColumnGroup { mounting_height_m: 8.0, arrangement: "single-sided left".to_string(), columns: 7 },
                ColumnGroup { mounting_height_m: 10.0, arrangement: "single column".to_string(), columns: 1 },
            ]
        );
        assert_eq!(
            q.luminaires,
            vec![
                LuminaireGroup { product: "Test LED 100".to_string(), power_w: 100.0, luminaires: 9 },
                LuminaireGroup { product: "Other, \"60\"".to_string(), power_w: 60.0, luminaires: 7 },
            ]
        );
        assert_eq!(q.skipped.len(), 1);
        assert_eq!(q.skipped[0].name, "broken");

        let schedule = schedule_to_csv(&q);
        let lines: Vec<&str> = schedule.lines().collect();
        assert_eq!(lines.len(), 14);
        assert_eq!(lines[0], SCHEDULE_CSV_HEADER);
        assert_eq!(lines[2], "LC-002,Z1,0+100.000,central,0.000,1100.000,2000.000,101.000,10,1.5,Test LED 100,2,200");
        assert_eq!(lines[5], "LS-001,Z2,0+000.000,left,-5.000,1000.000,2005.000,100.000,8,1.5,\"Other, \"\"60\"\"\",1,60");
        assert_eq!(lines[12], "J1,J1,0+150.000,right,9.000,1150.000,1991.000,101.500,10,1.5,Test LED 100,1,100");
        assert_eq!(lines[13], "TOTAL,12,,,,,,,,,,16,1320");
        assert_eq!(
            summary_to_csv(&q),
            format!(
                "{SUMMARY_CSV_HEADER}\n\
                 lighting column,\"10 m mounting height, twin central\",nr,4\n\
                 lighting column,\"8 m mounting height, single-sided left\",nr,7\n\
                 lighting column,\"10 m mounting height, single column\",nr,1\n\
                 luminaire,\"Test LED 100, 100 W\",nr,9\n\
                 luminaire,\"Other, \"\"60\"\", 60 W\",nr,7\n\
                 connected load,sum of entered luminaire powers,W,1320\n\
                 lighting element,not measured (invalid definition),nr,1\n"
            )
        );
    }

    #[test]
    fn an_empty_document_gives_headers_and_zero_totals() {
        let q = LightingQuantities::of_document(&Document::default());
        assert_eq!(schedule_to_csv(&q), format!("{SCHEDULE_CSV_HEADER}\nTOTAL,0,,,,,,,,,,0,0\n"));
        assert_eq!(summary_to_csv(&q), format!("{SUMMARY_CSV_HEADER}\nconnected load,sum of entered luminaire powers,W,0\n"));
    }
}
