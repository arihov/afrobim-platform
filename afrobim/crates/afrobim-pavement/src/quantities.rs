//! Take-off of pavement layers, by geometry only.
//!
//! Every figure is the entered dimensions put through the offset-strip formula of the
//! `geometry` module. Nothing comes from a display mesh. There are no rates, densities, bulking
//! or wastage factors here: the volume is the compacted volume in place.

use afrobim_kernel::Element;

use crate::geometry::{heading_change_rad, strip_area_m2};
use crate::model::PavementDefinition;
use crate::{PavementError, PAVEMENT_CATEGORY};

/// One layer of one strip of one pavement, measured.
#[derive(Debug, Clone, PartialEq)]
pub struct LayerQuantity {
    /// The pavement element's name.
    pub mark: String,
    /// "Carriageway", "Left shoulder" or "Right shoulder".
    pub strip: String,
    pub layer: String,
    pub material: String,
    pub thickness_mm: f64,
    /// Width of the strip, m.
    pub width_m: f64,
    /// Offset of the strip's own centre from the alignment, m, positive to the right.
    pub mean_offset_m: f64,
    /// Length along the alignment between the two stations, m.
    pub length_m: f64,
    /// Change of heading between the two stations, radians, positive turning left.
    pub heading_change_rad: f64,
    /// Length of the strip's own centre line: `length + mean offset × heading change`, m.
    pub strip_length_m: f64,
    /// Plan area: `width × strip length`, m². The surface is not developed for the cross-fall
    /// (at 2.5 per cent the sloping area is 0.03 per cent larger).
    pub area_m2: f64,
    /// Compacted volume in place: `plan area × thickness`, m³. Exact for a layer of constant
    /// vertical thickness, whatever the cross-fall.
    pub volume_m3: f64,
}

/// Measures one pavement: a row per layer of each strip, left shoulder first, top layer first.
/// Fails when the definition has Error notes.
pub fn measure_definition(mark: &str, def: &PavementDefinition) -> Result<Vec<LayerQuantity>, PavementError> {
    def.check()?;
    let length = def.length_m();
    let turn = heading_change_rad(&def.alignment, def.start_station_m, def.end_station_m);
    let mut rows = Vec::new();
    for strip in def.strips() {
        let area = strip_area_m2(length, turn, strip.left_offset_m(), strip.right_offset_m());
        for layer in &strip.layers {
            rows.push(LayerQuantity {
                mark: mark.to_string(),
                strip: strip.kind.label().to_string(),
                layer: layer.name.clone(),
                material: layer.material.label().to_string(),
                thickness_mm: layer.thickness_mm,
                width_m: strip.width_m(),
                mean_offset_m: strip.mean_offset_m(),
                length_m: length,
                heading_change_rad: turn,
                strip_length_m: length + strip.mean_offset_m() * turn,
                area_m2: area,
                volume_m3: area * layer.thickness_mm / 1000.0,
            });
        }
    }
    Ok(rows)
}

/// A pavement that could not be measured, and why.
#[derive(Debug, Clone, PartialEq)]
pub struct SkippedElement {
    pub mark: String,
    pub reason: String,
}

/// The same layer added up over strips and pavements.
#[derive(Debug, Clone, PartialEq)]
pub struct LayerTotal {
    pub layer: String,
    pub material: String,
    pub thickness_mm: f64,
    /// Sum of the strip lengths, m: lane-metres of this layer, not road length.
    pub strip_length_m: f64,
    pub area_m2: f64,
    pub volume_m3: f64,
}

/// The take-off of every pavement in a list of elements.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct PavementQuantities {
    pub rows: Vec<LayerQuantity>,
    /// Road length of each pavement measured: (mark, length along the alignment in metres).
    pub lengths: Vec<(String, f64)>,
    /// Pavements whose definition is unreadable or has Error notes. They are in no total; a
    /// schedule with anything here is incomplete.
    pub skipped: Vec<SkippedElement>,
}

impl PavementQuantities {
    /// Measures the pavements among `elements`, in order. Other categories are ignored.
    pub fn measure<'a>(elements: impl IntoIterator<Item = &'a Element>) -> PavementQuantities {
        let mut out = PavementQuantities::default();
        for e in elements.into_iter().filter(|e| e.category == PAVEMENT_CATEGORY) {
            match PavementDefinition::from_value(&e.data).and_then(|d| measure_definition(&e.name, &d).map(|rows| (d.length_m(), rows))) {
                Ok((length, rows)) => {
                    out.lengths.push((e.name.clone(), length));
                    out.rows.extend(rows);
                }
                Err(err) => out.skipped.push(SkippedElement { mark: e.name.clone(), reason: err.to_string() }),
            }
        }
        out
    }

    /// Totals per layer: rows with the same layer name, material and thickness are added, in
    /// order of first appearance.
    pub fn layer_totals(&self) -> Vec<LayerTotal> {
        let mut out: Vec<LayerTotal> = Vec::new();
        for r in &self.rows {
            match out.iter_mut().find(|t| t.layer == r.layer && t.material == r.material && t.thickness_mm == r.thickness_mm) {
                Some(t) => {
                    t.strip_length_m += r.strip_length_m;
                    t.area_m2 += r.area_m2;
                    t.volume_m3 += r.volume_m3;
                }
                None => out.push(LayerTotal {
                    layer: r.layer.clone(),
                    material: r.material.clone(),
                    thickness_mm: r.thickness_mm,
                    strip_length_m: r.strip_length_m,
                    area_m2: r.area_m2,
                    volume_m3: r.volume_m3,
                }),
            }
        }
        out
    }

    /// Road length measured, m: the sum over pavements of the length along the alignment.
    pub fn length_m(&self) -> f64 {
        self.lengths.iter().map(|l| l.1).sum()
    }

    /// Sum of the layer areas, m². A square metre of road counts once per layer.
    pub fn area_m2(&self) -> f64 {
        self.rows.iter().map(|r| r.area_m2).sum()
    }

    pub fn volume_m3(&self) -> f64 {
        self.rows.iter().map(|r| r.volume_m3).sum()
    }
}

pub const LAYERS_CSV_HEADER: &str =
    "mark,strip,layer,material,thickness_mm,width_m,mean_offset_m,length_m,heading_change_rad,strip_length_m,area_m2,volume_m3";

/// Layer schedule, one row per layer of each strip, then a `TOTAL` row carrying the number of
/// rows in the second column and the summed area and volume. Lines end with `\n`.
///
/// Rounding: thickness to at most 1 decimal, lengths, widths, areas and volumes to 3, the
/// heading change to 6. Totals are sums of the unrounded values, rounded once.
pub fn layers_to_csv(rows: &[LayerQuantity]) -> String {
    let mut out = String::from(LAYERS_CSV_HEADER);
    out.push('\n');
    for r in rows {
        let fields = [
            csv(&r.mark),
            csv(&r.strip),
            csv(&r.layer),
            csv(&r.material),
            fixed(r.thickness_mm, 0, 1),
            fixed(r.width_m, 3, 3),
            fixed(r.mean_offset_m, 3, 3),
            fixed(r.length_m, 3, 3),
            fixed(r.heading_change_rad, 6, 6),
            fixed(r.strip_length_m, 3, 3),
            fixed(r.area_m2, 3, 3),
            fixed(r.volume_m3, 3, 3),
        ];
        out.push_str(&fields.join(","));
        out.push('\n');
    }
    out.push_str(&format!(
        "TOTAL,{},,,,,,,,,{},{}\n",
        rows.len(),
        fixed(rows.iter().map(|r| r.area_m2).sum::<f64>(), 3, 3),
        fixed(rows.iter().map(|r| r.volume_m3).sum::<f64>(), 3, 3),
    ));
    out
}

pub const SUMMARY_CSV_HEADER: &str = "item,unit,quantity";

/// The totals as one bill-style list: pavements, road length, then area and volume of each
/// layer (see [`PavementQuantities::layer_totals`]) and the total volume. Lines end with `\n`.
/// A final `not measured` row counts the skipped elements when there are any.
pub fn summary_to_csv(q: &PavementQuantities) -> String {
    let mut out = String::from(SUMMARY_CSV_HEADER);
    out.push('\n');
    let mut row = |item: &str, unit: &str, quantity: String| out.push_str(&format!("{},{unit},{quantity}\n", csv(item)));
    row("pavements", "nr", q.lengths.len().to_string());
    row("road length", "m", fixed(q.length_m(), 3, 3));
    for t in q.layer_totals() {
        let what = if t.layer == t.material { t.layer.clone() } else { format!("{} ({})", t.layer, t.material) };
        let what = format!("{what} {} mm", fixed(t.thickness_mm, 0, 1));
        row(&format!("{what}: area"), "m2", fixed(t.area_m2, 3, 3));
        row(&format!("{what}: compacted volume"), "m3", fixed(t.volume_m3, 3, 3));
    }
    row("all layers: compacted volume", "m3", fixed(q.volume_m3(), 3, 3));
    if !q.skipped.is_empty() {
        row("not measured (invalid definition)", "nr", q.skipped.len().to_string());
    }
    out
}

/// `v` rounded to `max_decimals`, with trailing zeros removed down to `min_decimals`.
pub(crate) fn fixed(v: f64, min_decimals: usize, max_decimals: usize) -> String {
    let mut s = format!("{v:.max_decimals$}");
    if let Some(dot) = s.find('.') {
        while s.len() > dot + 1 + min_decimals && s.ends_with('0') {
            s.pop();
        }
        if s.ends_with('.') {
            s.pop();
        }
    }
    if s.starts_with('-') && s.chars().all(|c| matches!(c, '-' | '0' | '.')) {
        s.remove(0);
    }
    s
}

fn csv(s: &str) -> String {
    if s.contains([',', '"', '\n']) {
        format!("\"{}\"", s.replace('"', "\"\""))
    } else {
        s.to_string()
    }
}
