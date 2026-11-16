//! LandXML out of everything civil that the document holds: the alignments with their profiles,
//! point sets as CgPoints, parcels, surfaces (with the source data of a definition), ground
//! profiles, the cross sections of sample line groups, and the project coordinate system. All at
//! source coordinates. (`afrobim_design::write_landxml_file` does the writing.)

use afrobim_alignment::{LandXmlCgPoint, LandXmlCgPointGroup, LandXmlCoordinateSystem, LandXmlCoordGeom, LandXmlCrossSectSurf, LandXmlCrossSection, LandXmlFile, LandXmlGeomElement, LandXmlParcel, LandXmlPoint, LandXmlProfSurf, LandXmlProject};
use afrobim_cogo::{ArcGeom, Parcel, Segment};
use afrobim_design::{surface_from_tin, write_landxml_file, LandXmlOptions};
use afrobim_kernel::Document;

use crate::civil::*;
use crate::civil_build::Ctx;

/// The LandXML file and what was left out of it.
pub fn build_file(doc: &Document, ctx: &Ctx) -> (LandXmlFile, Vec<String>) {
    let mut notes = Vec::new();
    let mut file = LandXmlFile::default();
    file.units.declared = true;
    file.header.version = Some("1.2".into());
    if let Some(crs) = afrobim_kernel::project_crs(doc) {
        let mut cs = LandXmlCoordinateSystem { name: Some(crs.name.clone()), epsg_code: crs.epsg.map(|c| c.to_string()), ..Default::default() };
        if !crs.horizontal_datum.trim().is_empty() {
            cs.horizontal_datum = Some(crs.horizontal_datum.clone());
        }
        if !crs.vertical_datum.trim().is_empty() {
            cs.vertical_datum = Some(crs.vertical_datum.clone());
        }
        file.coordinate_system = Some(cs);
    } else {
        notes.push("The project has no coordinate system, so none is written.".into());
    }
    file.project = Some(LandXmlProject { name: "AfroBIM civil data".into(), desc: None, state: None });
    file.alignments = ctx.alignments();
    for e in doc.elements() {
        match e.category.as_str() {
            POINTS => {
                if let Some(d) = get::<PointsData>(doc, e.id, POINTS) {
                    let points = d
                        .store
                        .iter()
                        .map(|p| LandXmlCgPoint {
                            name: p.name.clone().unwrap_or_else(|| p.number.to_string()),
                            code: (!p.raw_description.is_empty()).then(|| p.raw_description.clone()),
                            desc: (!p.full_description.is_empty()).then(|| p.full_description.clone()),
                            state: None,
                            oid: None,
                            point_geometry: None,
                            dtm_attribute: None,
                            latitude: None,
                            longitude: None,
                            easting_m: p.easting,
                            northing_m: p.northing,
                            elevation_m: p.elevation,
                        })
                        .collect();
                    file.cg_points.push(LandXmlCgPointGroup { name: Some(e.name.clone()), points, ..Default::default() });
                }
            }
            PARCEL => {
                if let Some(p) = get::<Parcel>(doc, e.id, PARCEL) {
                    if hidden(e) {
                        continue;
                    }
                    file.parcels.push(landxml_parcel(&p));
                }
            }
            SURFACE => {
                if let (Some(d), Some(tin)) = (get::<SurfaceData>(doc, e.id, SURFACE), ctx.tin(e.id.0)) {
                    let mut s = surface_from_tin(&e.name, &tin);
                    for (name, _, pl) in afrobim_terrain::display::breaklines(&d.surface.definition) {
                        s.source_data.breaklines.push(afrobim_alignment::LandXmlBreakline { name: Some(name), points: pl.points.iter().map(|p| LandXmlPoint { x: p[0], y: p[1], z: Some(p[2]) }).collect(), ..Default::default() });
                    }
                    file.surfaces.push(s);
                } else if e.category == SURFACE {
                    notes.push(format!("Surface {} has not been built yet and is left out.", e.name));
                }
            }
            c if c == afrobim_corridor::SURFACE_CATEGORY => {
                if let Some(tin) = ctx.tin(e.id.0) {
                    file.surfaces.push(surface_from_tin(&e.name, &tin));
                }
            }
            PROFILE => {
                if let Some(d) = get::<ProfileData>(doc, e.id, PROFILE) {
                    if let Ok(pts) = crate::civil_build::profile_points(&d, ctx) {
                        let list: Vec<[f64; 2]> = pts.iter().filter_map(|(s, z)| z.map(|z| [*s, z])).collect();
                        if list.len() >= 2 {
                            file.ground_profiles.push(LandXmlProfSurf { alignment: d.alignment.clone(), profile: None, name: e.name.clone(), desc: None, state: None, point_lists: vec![list] });
                        }
                    }
                }
            }
            SAMPLES => {
                if let Some(d) = get::<SampleData>(doc, e.id, SAMPLES) {
                    if !d.imported.is_empty() {
                        // Sections that came from a file go back as the file's own lines, under their own names.
                        for sec in &d.imported {
                            let surfaces = sec.surfaces.iter().map(|s| LandXmlCrossSectSurf { name: s.name.clone(), desc: None, state: None, point_lists: s.runs.clone() }).collect();
                            file.cross_sections.push(LandXmlCrossSection { alignment: d.group.alignment.clone(), station_m: sec.station_m, name: Some(sec.line.clone()), desc: None, surfaces, design_surfaces: vec![] });
                        }
                    } else if let Some(al) = ctx.alignment_named(&d.group.alignment) {
                        let tins: Vec<(String, std::sync::Arc<afrobim_corridor::Tin>)> = d.surfaces.iter().filter_map(|id| ctx.element(*id).and_then(|el| ctx.tin(*id).map(|t| (el.name.clone(), t)))).collect();
                        let refs: Vec<(&str, &afrobim_corridor::Tin)> = tins.iter().map(|(n, t)| (n.as_str(), t.as_ref())).collect();
                        match afrobim_corridor::section_views(&d.group, &al, &refs, None) {
                            Ok(views) => {
                                for v in views {
                                    let surfaces = v.ground_lines.iter().map(|g| LandXmlCrossSectSurf { name: g.surface.clone(), desc: None, state: None, point_lists: g.runs.iter().map(|r| r.iter().map(|p| [p.0, p.1]).collect()).collect() }).collect();
                                    file.cross_sections.push(LandXmlCrossSection { alignment: al.name.clone(), station_m: v.line.station_m, name: Some(v.label.clone()), desc: None, surfaces, design_surfaces: vec![] });
                                }
                            }
                            Err(x) => notes.push(format!("Sample lines {}: {x}", e.name)),
                        }
                    }
                }
            }
            ASSEMBLY | PROFILE_VIEW | CORRIDOR | PIPENET | PIPEPROFILE => {}
            _ => {}
        }
    }
    notes.push("Assemblies, profile views and corridors have no LandXML form here and are not written (a corridor's top and datum surfaces are not written either).".into());
    (file, notes)
}

fn landxml_parcel(p: &Parcel) -> LandXmlParcel {
    let pt = |x: &afrobim_cogo::Xy| LandXmlPoint { x: x.e, y: x.n, z: None };
    let mut elements = Vec::new();
    let mut at = p.boundary.start;
    let mut segs = p.boundary.segments.clone();
    // Close the boundary back to its start.
    if segs.last().is_none_or(|s| (s.to().e - at.e).abs() > 1e-9 || (s.to().n - at.n).abs() > 1e-9) {
        segs.push(Segment::Line { to: p.boundary.start });
    }
    for s in &segs {
        match s {
            Segment::Line { to } => elements.push(LandXmlGeomElement::Line { start: pt(&at), end: pt(to) }),
            Segment::Arc { to, radius, turn, large } => match ArcGeom::new(at, *to, *radius, *turn, *large) {
                Ok(g) => elements.push(LandXmlGeomElement::Curve { start: pt(&at), center: pt(&g.center), end: pt(to), pi: None, clockwise: matches!(turn, afrobim_cogo::Turn::Right), radius_m: Some(*radius), length_m: Some(g.length()) }),
                Err(_) => elements.push(LandXmlGeomElement::Line { start: pt(&at), end: pt(to) }),
            },
        }
        at = s.to();
    }
    LandXmlParcel {
        name: p.name.clone(),
        desc: p.attributes.get("Description").cloned(),
        state: Some(p.status.label()),
        owner: (!p.owner.is_empty()).then(|| p.owner.clone()),
        parcel_type: (!p.land_use.is_empty()).then(|| p.land_use.clone()),
        class: None,
        area: None,
        area_m2: p.area().ok(),
        center: p.centroid().ok().map(|c| pt(&c)),
        coord_geoms: vec![LandXmlCoordGeom { name: None, desc: None, state: None, elements }],
        parcels: vec![],
    }
}

/// The LandXML text of the document and the notes about what was left out.
pub fn landxml_text(doc: &Document, ctx: &Ctx) -> (String, Vec<String>) {
    let (file, mut notes) = build_file(doc, ctx);
    let out = write_landxml_file(&file, &LandXmlOptions::default());
    notes.extend(out.warnings);
    let mut text = out.text;
    // The pipe networks are written by the pipes crate and spliced in before the closing tag.
    let nets: Vec<afrobim_pipes::PipeNetwork> = doc.elements().iter().filter(|e| e.category == PIPENET && !hidden(e)).filter_map(|e| get::<crate::civil_pipes::PipeNetData>(doc, e.id, PIPENET).map(|d| {
        let mut n = d.network;
        n.name = e.name.clone();
        n
    })).collect();
    if !nets.is_empty() {
        match afrobim_pipes::write_landxml_pipes(&nets, &afrobim_pipes::WriteOptions::now()) {
            Ok(xml) => match (xml.find("<PipeNetworks"), xml.rfind("</PipeNetworks>")) {
                (Some(a), Some(b)) => match text.rfind("</LandXML>") {
                    Some(at) => text.insert_str(at, &format!("{}\n", &xml[a..b + "</PipeNetworks>".len()])),
                    None => notes.push("The pipe networks could not be added: the LandXML text has no closing tag.".into()),
                },
                _ => notes.push("The pipe networks could not be added: the pipes writer gave no PipeNetworks element.".into()),
            },
            Err(e) => notes.push(format!("Pipe networks are left out of the LandXML: {e}")),
        }
    }
    (text, notes)
}

impl crate::app::AfroBimApp {
    /// The context of a build over the whole document, with the finished TINs.
    pub(crate) fn civil_ctx(&mut self) -> Ctx {
        self.civil_build_now();
        let elements = self.doc.elements().iter().filter(|e| is_civil(&e.category) || e.category == afrobim_alignment::CATEGORY || e.category == afrobim_corridor::SURFACE_CATEGORY).cloned().collect();
        let tins = self.civil.built.iter().filter_map(|(id, (_, b))| b.tin.clone().map(|t| (id.0, t))).collect();
        Ctx { elements, tins }
    }

    pub(crate) fn civil_landxml_out(&mut self) {
        let ctx = self.civil_ctx();
        let (text, notes) = landxml_text(&self.doc, &ctx);
        if let Some(path) = rfd::FileDialog::new().add_filter("LandXML", &["xml"]).set_file_name("civil.xml").save_file() {
            self.status = match std::fs::write(&path, text) {
                Ok(()) => format!("Wrote {} ({} note(s) in the command history).", path.display(), notes.len()),
                Err(e) => format!("The file could not be written: {e}"),
            };
            for n in notes {
                self.log(format!("CIVILXML: {n}"));
            }
        }
    }
}
