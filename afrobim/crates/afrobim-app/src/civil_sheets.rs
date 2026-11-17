//! Section views of a sample line group for the section sheets: the group is cut here (the sheets
//! crate does not know the civil objects) and handed over as drawings, the same kind the
//! corridor's own sections give, so that both kinds of section share the sheet layout.

use std::collections::HashMap;
use std::sync::Arc;

use afrobim_corridor::{LabelKind, SectionDrawing, SectionLabel, SectionView, SlopeKind};
use afrobim_draw::{Colour, Drawing, Extents, Primitive, Pt2};
use afrobim_kernel::Document;
use afrobim_sheets::SectionsView;

use crate::civil::{self, SampleData, SurfaceData};
use crate::civil_build::{build_tin, Ctx};

/// The elements the cut reads, with the TIN of every civil surface the group (or its corridor's
/// ground) uses built.
fn context(doc: &Document, d: &SampleData) -> Ctx {
    let elements: Vec<_> = doc.elements().iter().filter(|e| civil::is_civil(&e.category) || e.category == afrobim_alignment::CATEGORY || e.category == afrobim_corridor::SURFACE_CATEGORY).cloned().collect();
    let mut wanted: Vec<u64> = d.surfaces.clone();
    if let Some(c) = d.corridor.and_then(|id| elements.iter().find(|e| e.id.0 == id)).and_then(|e| serde_json::from_value::<civil::CorridorData>(e.data.clone()).ok()) {
        wanted.push(c.ground);
    }
    let mut tins: HashMap<u64, Arc<afrobim_corridor::Tin>> = HashMap::new();
    for id in wanted {
        if let Some(e) = elements.iter().find(|e| e.id.0 == id && e.category == civil::SURFACE) {
            if let Some(Ok((tin, _))) = serde_json::from_value::<SurfaceData>(e.data.clone()).ok().map(|sd| build_tin(&sd)) {
                tins.insert(id, Arc::new(tin));
            }
        }
    }
    Ctx { elements, tins }
}

/// Fills the view's drawings from the sample line group it names. Returns how many section views
/// there are, or why there are none.
pub fn feed_sample_sections(view: &mut SectionsView, doc: &Document) -> Result<usize, String> {
    let name = view.sample_group.clone().ok_or("The view has no sample line group.")?;
    let e = doc.elements().iter().find(|e| e.category == civil::SAMPLES && e.name == name).ok_or_else(|| format!("There is no sample line group called '{name}' in the document."))?;
    let d: SampleData = serde_json::from_value(e.data.clone()).map_err(|x| format!("The sample line data cannot be read: {x}"))?;
    let ctx = context(doc, &d);
    let views = crate::civil_build2::sample_views(&d, &ctx)?;
    let k = view.vertical_exaggeration;
    view.fed = views.iter().map(|v| sample_section_drawing(v, k)).collect();
    view.fed_stations_m = views.iter().map(|v| v.line.station_m).collect();
    Ok(view.fed.len())
}

/// One section view as a drawing in millimetres at full size, as `afrobim_corridor::section_drawing`
/// gives for a corridor section: ground lines of every surface sampled, and when a corridor was
/// sampled its links, earthworks line and shapes. Levels are stretched by `exaggeration`.
pub fn sample_section_drawing(v: &SectionView, exaggeration: f64) -> SectionDrawing {
    let k = if exaggeration.is_finite() && exaggeration > 0.0 { exaggeration } else { 1.0 };
    let mut pts: Vec<(f64, f64)> = v.ground_lines.iter().flat_map(|g| g.runs.iter().flatten().copied()).collect();
    if let Some(dv) = &v.design {
        pts.extend(dv.geometry.links.iter().flat_map(|l| l.points.iter().copied()));
        pts.extend(dv.section.earthworks.iter().copied());
        pts.extend(dv.section.finished.iter().map(|p| (p.offset_m, p.elevation_m)));
    }
    if pts.is_empty() {
        pts.push((0.0, 0.0));
    }
    let min_o = pts.iter().map(|p| p.0).fold(f64::INFINITY, f64::min);
    let max_o = pts.iter().map(|p| p.0).fold(f64::NEG_INFINITY, f64::max);
    let min_z = pts.iter().map(|p| p.1).fold(f64::INFINITY, f64::min);
    let max_z = pts.iter().map(|p| p.1).fold(f64::NEG_INFINITY, f64::max);
    let pt = |p: (f64, f64)| Pt2::new((p.0 - min_o) * 1000.0, (p.1 - min_z) * 1000.0 * k);
    let mut primitives = Vec::new();
    let mut line = |l: &[(f64, f64)], colour: Option<Colour>| primitives.extend(l.windows(2).map(|w| Primitive::Line { from: pt(w[0]), to: pt(w[1]), width: 0.0, colour }));
    let ground_colours = [Colour::Green, Colour::Brown, Colour::Blue, Colour::Yellow];
    for (i, g) in v.ground_lines.iter().enumerate() {
        for run in &g.runs {
            line(run, Some(ground_colours[i % ground_colours.len()]));
        }
    }
    if let Some(dv) = &v.design {
        line(&dv.section.earthworks, Some(Colour::Red));
        for l in &dv.geometry.links {
            line(&l.points, None);
        }
        for sh in &dv.geometry.shapes {
            if sh.polygon.len() >= 2 {
                let mut ring = sh.polygon.clone();
                ring.push(ring[0]);
                line(&ring, Some(Colour::Grey));
            }
        }
    }
    line(&[(0.0, min_z), (0.0, max_z)], Some(Colour::Grey));

    let mut labels = vec![SectionLabel { at: pt((min_o, max_z)), text: format!("Station {}", v.label), kind: LabelKind::Title }];
    let point = |o: f64, z: f64| format!("{o:+.3} / {z:.3}");
    if let Some(dv) = &v.design {
        for p in &dv.section.finished {
            labels.push(SectionLabel { at: pt((p.offset_m, p.elevation_m)), text: format!("{} {}", p.code, point(p.offset_m, p.elevation_m)), kind: LabelKind::Design });
        }
        for d in [dv.section.left_daylight, dv.section.right_daylight].into_iter().flatten() {
            let what = match d.kind {
                SlopeKind::Cut => "cut daylight",
                SlopeKind::Fill => "fill daylight",
                SlopeKind::AtGround => "at ground",
            };
            labels.push(SectionLabel { at: pt((d.offset_m, d.elevation_m)), text: format!("{what} {}", point(d.offset_m, d.elevation_m)), kind: LabelKind::Daylight });
        }
        let areas = match (dv.section.cut_area_m2, dv.section.fill_area_m2) {
            (Some(c), Some(f)) => format!("Cut {c:.3} m2   Fill {f:.3} m2"),
            _ => dv.section.notes.join(" "),
        };
        labels.push(SectionLabel { at: pt(((min_o + max_o) / 2.0, max_z)), text: areas, kind: LabelKind::Areas });
    }
    // The level of the first surface on the centreline.
    if let Some(g) = v.ground_lines.first() {
        for run in &g.runs {
            if let Some(w) = run.windows(2).find(|w| w[0].0 <= 0.0 && w[1].0 >= 0.0 && w[1].0 > w[0].0) {
                let t = (0.0 - w[0].0) / (w[1].0 - w[0].0);
                let z = w[0].1 + t * (w[1].1 - w[0].1);
                labels.push(SectionLabel { at: pt((0.0, z)), text: format!("ground {z:.3}"), kind: LabelKind::Ground });
                break;
            }
        }
    }
    SectionDrawing {
        drawing: Drawing { primitives, extents: Extents { min: Pt2::new(0.0, 0.0), max: pt((max_o, max_z)) } },
        labels,
        origin_offset_m: min_o,
        origin_level_m: min_z,
        vertical_exaggeration: k,
    }
}
