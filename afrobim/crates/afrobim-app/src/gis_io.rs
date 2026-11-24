//! GIS exchange of the drawing: export to Google Earth KML and GeoJSON (WGS 84), the way
//! CADEarth and Spatial Manager do it.
//!
//! The drawing primitives of each element (the same ones DXF export writes) are turned into
//! lines, rings and points in model millimetres, moved to the project system in metres, and from
//! there to WGS 84 through the kernel's transformation plan. Lines that meet end to end inside one
//! element are chained into one polyline, arcs are cut into chords no further than
//! [`ARC_CHORD_MM`] from the true curve, and lettering is left out (a GIS has labels of its own).

use afrobim_draw::{Drawing, Primitive};
use afrobim_kernel::{Document, ElementId, Pt, TransformPlan};
use afrobim_raster::vector::{Feature, Geometry};

/// Largest gap between an arc and its chords, millimetres.
pub const ARC_CHORD_MM: f64 = 20.0;
/// Ends closer than this are joined, millimetres.
const JOIN_MM: f64 = 0.5;

/// One piece of an element in model millimetres.
#[derive(Debug, Clone, PartialEq)]
pub enum Part {
    Line(Vec<Pt>),
    Ring(Vec<Pt>),
    Point(Pt),
}

/// The chord points of an arc, ends included.
pub fn tessellate_arc(centre: Pt, radius: f64, start_deg: f64, end_deg: f64) -> Vec<Pt> {
    let mut sweep = end_deg - start_deg;
    while sweep <= 0.0 {
        sweep += 360.0;
    }
    let step = if radius > ARC_CHORD_MM { 2.0 * (1.0 - ARC_CHORD_MM / radius).acos().to_degrees() } else { 90.0 };
    let n = ((sweep / step.max(0.5)).ceil() as usize).clamp(2, 720);
    (0..=n)
        .map(|i| {
            let a = (start_deg + sweep * i as f64 / n as f64).to_radians();
            Pt::new(centre.x + radius * a.cos(), centre.y + radius * a.sin())
        })
        .collect()
}

/// The parts of one element's drawing placed at `origin`. Bands are written as their centreline.
pub fn parts_of(drawing: &Drawing, origin: Pt) -> Vec<Part> {
    let at = |p: &Pt| Pt::new(p.x + origin.x, p.y + origin.y);
    let mut out = Vec::new();
    let mut run: Vec<Pt> = Vec::new();
    let flush = |run: &mut Vec<Pt>, out: &mut Vec<Part>| {
        if run.len() >= 2 {
            out.push(Part::Line(std::mem::take(run)));
        }
        run.clear();
    };
    for prim in &drawing.primitives {
        let seg: Option<Vec<Pt>> = match prim {
            Primitive::Line { from, to, .. } => Some(vec![at(from), at(to)]),
            Primitive::Arc { centre, radius, start_deg, end_deg, .. } => Some(tessellate_arc(at(centre), *radius, *start_deg, *end_deg)),
            Primitive::Fill { outline, .. } => {
                flush(&mut run, &mut out);
                if outline.len() >= 3 {
                    let mut ring: Vec<Pt> = outline.iter().map(at).collect();
                    if ring.first().unwrap().distance_to(*ring.last().unwrap()) > JOIN_MM {
                        ring.push(ring[0]);
                    }
                    out.push(Part::Ring(ring));
                }
                None
            }
            Primitive::Symbol { origin: o, .. } => {
                flush(&mut run, &mut out);
                out.push(Part::Point(at(o)));
                None
            }
            Primitive::Glyphs { .. } => None,
        };
        let Some(seg) = seg else { continue };
        if seg[0].distance_to(*seg.last().unwrap()) < 1e-9 && seg.len() == 2 {
            continue;
        }
        match run.last() {
            Some(end) if end.distance_to(seg[0]) <= JOIN_MM => run.extend(seg.into_iter().skip(1)),
            _ => {
                flush(&mut run, &mut out);
                run = seg;
            }
        }
    }
    flush(&mut run, &mut out);
    out
}

/// A model point (millimetres) to [lon, lat, 0] through a project-to-WGS 84 plan.
pub fn to_lonlat(plan: &TransformPlan, p: Pt) -> Result<[f64; 3], String> {
    // The plan's result for a geographic target is (latitude, longitude).
    let g = plan.apply(p.x / 1000.0, p.y / 1000.0, 0.0).map_err(|e| e.to_string())?;
    Ok([g.second, g.first, 0.0])
}

/// The features of the given elements, in WGS 84.
pub fn features(doc: &Document, drawings: &[(ElementId, &Drawing)], plan: &TransformPlan) -> Result<Vec<Feature>, String> {
    let mut out = Vec::new();
    for (id, drawing) in drawings {
        let Some(e) = doc.find(*id) else { continue };
        let props = vec![("category".to_string(), e.category.clone()), ("id".to_string(), id.0.to_string())];
        for part in parts_of(drawing, e.origin) {
            let conv = |pts: &[Pt]| pts.iter().map(|p| to_lonlat(plan, *p)).collect::<Result<Vec<_>, _>>();
            let geometry = match part {
                Part::Line(v) => Geometry::LineString(conv(&v)?),
                Part::Ring(v) => Geometry::Polygon(vec![conv(&v)?]),
                Part::Point(p) => Geometry::Point(to_lonlat(plan, p)?),
            };
            out.push(Feature { name: e.name.clone(), layer: e.layer.clone(), geometry, properties: props.clone() });
        }
    }
    Ok(out)
}

/// Whether an element's drawing goes to a GIS export (the same rule as DXF export).
pub fn exportable(e: &afrobim_kernel::Element) -> bool {
    !(afrobim_annotate::is_annotation(e) || crate::imagery::is_backdrop(&e.category) || e.category == afrobim_annotate::SETTINGS_CATEGORY || e.category == afrobim_kernel::PROJECT_SETTINGS_CATEGORY)
}

impl crate::app::AfroBimApp {
    /// Writes the selection (or everything) as KML or GeoJSON, chosen by the file's extension.
    pub(crate) fn export_gis(&mut self, path: std::path::PathBuf) {
        self.sync_generated();
        let project = afrobim_kernel::project_crs(&self.doc);
        let plan = match crate::basemap::plans_for(project.as_ref()) {
            Ok((to, _, _)) => to,
            Err(e) => {
                self.status = format!("Not exported: {e}");
                return;
            }
        };
        let selected_only = !self.doc.selection().is_empty();
        let drawings: Vec<(ElementId, &Drawing)> = self
            .doc
            .elements()
            .iter()
            .filter(|e| (!selected_only || self.doc.is_selected(e.id)) && exportable(e))
            .filter_map(|e| self.cache.get(&e.id).and_then(|c| c.generated.as_ref().ok()).map(|g| (e.id, &g.drawing)))
            .collect();
        let n = drawings.len();
        let candidates = self.doc.elements().iter().filter(|e| (!selected_only || self.doc.is_selected(e.id)) && exportable(e)).count();
        let skipped = candidates - n;
        let feats = match features(&self.doc, &drawings, &plan) {
            Ok(f) => f,
            Err(e) => {
                self.status = format!("Not exported: a point is outside what the projection can compute ({e}).");
                return;
            }
        };
        let kml = path.extension().is_some_and(|x| x.eq_ignore_ascii_case("kml"));
        let name = path.file_stem().map(|s| s.to_string_lossy().to_string()).unwrap_or_default();
        let text = if kml { afrobim_raster::vector::to_kml(&name, &feats) } else { afrobim_raster::vector::to_geojson(&feats) };
        self.status = match std::fs::write(&path, text) {
            Ok(()) => format!(
                "Exported {} features from {n} objects to {} (WGS 84{}){}.",
                feats.len(),
                path.display(),
                crate::imagery::plan_words(&plan),
                if skipped > 0 { format!("; {skipped} objects with no plan drawing were left out") } else { String::new() }
            ),
            Err(e) => format!("Could not write {}: {e}", path.display()),
        };
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use afrobim_draw::{Colour, Extents};
    use afrobim_kernel::ProjectCrs;

    fn drawing(primitives: Vec<Primitive>) -> Drawing {
        Drawing { primitives, extents: Extents::default() }
    }

    #[test]
    fn lines_that_meet_are_chained_and_arcs_stay_within_the_chord_tolerance() {
        let d = drawing(vec![
            Primitive::Line { from: Pt::new(0.0, 0.0), to: Pt::new(1000.0, 0.0), width: 0.0, colour: None },
            Primitive::Line { from: Pt::new(1000.0, 0.0), to: Pt::new(1000.0, 1000.0), width: 150.0, colour: Some(Colour::White) },
            Primitive::Arc { centre: Pt::new(5000.0, 0.0), radius: 50_000.0, start_deg: 0.0, end_deg: 90.0, colour: None },
            Primitive::Glyphs { contours: vec![], text: "A".into(), ink: None },
        ]);
        let parts = parts_of(&d, Pt::new(10.0, 20.0));
        assert_eq!(parts.len(), 2, "{parts:?}");
        let Part::Line(l) = &parts[0] else { panic!() };
        assert_eq!(l, &vec![Pt::new(10.0, 20.0), Pt::new(1010.0, 20.0), Pt::new(1010.0, 1020.0)]);
        let Part::Line(arc) = &parts[1] else { panic!() };
        let c = Pt::new(5010.0, 20.0);
        for w in arc.windows(2) {
            let mid = Pt::new((w[0].x + w[1].x) / 2.0, (w[0].y + w[1].y) / 2.0);
            assert!(50_000.0 - mid.distance_to(c) <= ARC_CHORD_MM + 1e-6);
        }
        assert!((arc.last().unwrap().distance_to(Pt::new(5010.0, 50_020.0))) < 1e-6, "the arc ends where it should");
    }

    #[test]
    fn a_wgs84_utm_point_comes_out_as_longitude_then_latitude() {
        // UTM 36N, E 448 000 N 38 400 is close to Kampala: about 32.53 E, 0.35 N.
        let p = ProjectCrs::from_epsg(32636).unwrap();
        let (to, _, _) = crate::basemap::plans_for(Some(&p)).unwrap();
        let [lon, lat, _] = to_lonlat(&to, Pt::new(448_000_000.0, 38_400_000.0)).unwrap();
        assert!((lon - 32.53).abs() < 0.02 && (lat - 0.347).abs() < 0.02, "lon {lon} lat {lat}");
    }

    #[test]
    fn a_document_exports_to_kml_and_geojson_and_needs_a_real_system() {
        let mut doc = afrobim_kernel::Document::default();
        let id = doc.add("wall", "Kerb A", Pt::new(448_000_000.0, 38_400_000.0), 1.0, 1.0, "0", serde_json::json!({}));
        let d = drawing(vec![Primitive::Line { from: Pt::new(0.0, 0.0), to: Pt::new(10_000.0, 0.0), width: 0.0, colour: None }]);
        assert!(crate::basemap::plans_for(Some(&ProjectCrs::local_grid("Site"))).is_err());
        let p = ProjectCrs::from_epsg(32636).unwrap();
        let (to, _, _) = crate::basemap::plans_for(Some(&p)).unwrap();
        let f = features(&doc, &[(id, &d)], &to).unwrap();
        assert_eq!(f.len(), 1);
        let Geometry::LineString(v) = &f[0].geometry else { panic!() };
        // Ten metres east is about 0.00009 degrees of longitude at the equator.
        assert!((v[1][0] - v[0][0] - 10.0 / 111_320.0).abs() < 2e-6 && (v[1][1] - v[0][1]).abs() < 1e-6);
        let kml = afrobim_raster::vector::to_kml("t", &f);
        assert!(kml.contains("Kerb A") && kml.contains("<coordinates>32."));
        let gj = afrobim_raster::vector::to_geojson(&f);
        assert!(gj.contains("\"Kerb A\"") && gj.contains("[32."));
    }

    #[test]
    fn the_app_exports_an_imported_alignment_to_geojson() {
        let ctx = egui::Context::default();
        let cc = eframe::CreationContext::_new_kittest(ctx);
        let mut app = crate::app::AfroBimApp::new(&cc, crate::Options { civil_demo: false, screenshot: Some(std::env::temp_dir().join("afrobim-gis-test.png")), ..crate::Options::default() });
        let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../afrobim-alignment/tests/landxml/corridor.xml");
        crate::alignment_glue::import_landxml(&mut app.doc, &path).unwrap();
        if afrobim_kernel::project_crs(&app.doc).is_none() {
            afrobim_kernel::set_project_crs(&mut app.doc, &ProjectCrs::from_epsg(21036).unwrap());
        }
        let cats: Vec<String> = app.doc.elements().iter().map(|e| e.category.clone()).collect();
        let out = std::env::temp_dir().join(format!("afrobim-gis-{}.geojson", std::process::id()));
        app.export_gis(out.clone());
        assert!(app.status.starts_with("Exported"), "{} (categories {cats:?})", app.status);
        let f = afrobim_raster::vector::from_geojson(&std::fs::read_to_string(&out).unwrap()).unwrap();
        let al: Vec<_> = f.iter().filter(|x| x.properties.iter().any(|(k, v)| k == "category" && v == crate::alignment_glue::CATEGORY)).collect();
        assert!(!al.is_empty(), "no alignment in the export: {} ; categories in features {:?}", app.status, f.iter().map(|x| x.properties.clone()).collect::<Vec<_>>());
        assert!(al.iter().any(|x| matches!(&x.geometry, Geometry::LineString(v) if v.len() > 2)));
    }
}
