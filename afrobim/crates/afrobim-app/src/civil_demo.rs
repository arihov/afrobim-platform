//! The civil demonstration data (`--demo civil`, the CIVILDEMO command). Every object is named and
//! sourced as demonstration data with example values: nothing here is a design, a survey or a
//! standard. The coordinates are of true UTM size (WGS 84 / UTM zone 36S, easting about 450 000 m,
//! northing about 9 960 000 m) so that nothing in the application meets small numbers only.

use afrobim_alignment::{Alignment, AlignmentElement, Profile, Pvi, Turn};
use afrobim_cogo::{CogoPoint, DescriptionKey, PointGroup, Rule, Xy};
use afrobim_corridor::{Assembly, AssemblyRegion, Baseline, CorridorModel, Frequency, SampleLineGroup, SampleStations, VolumeOptions};
use afrobim_kernel::{Document, ElementId};
use afrobim_terrain::{Operation, Surface, SurfacePoint, SurfaceRole};

use crate::civil::{self, *};

pub const E0: f64 = 450_000.0;
pub const N0: f64 = 9_960_000.0;
const SOURCE: &str = "DEMONSTRATION DATA: example values, not from any survey or standard";

/// The ground of the demonstration: an undulation about 1150 m (made up).
pub fn ground_z(e: f64, n: f64) -> f64 {
    1150.0 + 2.2 * ((e - E0) / 70.0).sin() * ((n - N0) / 55.0).cos() + 0.012 * (n - N0)
}

pub fn alignment() -> Alignment {
    let h0 = 0.35_f64;
    let (x, y) = (E0 + 25.0, N0 + 70.0);
    let l1 = 90.0;
    let (x1, y1) = (x + l1 * h0.cos(), y + l1 * h0.sin());
    let r = 130.0;
    let arc_len = 95.0;
    let sweep = arc_len / r;
    let h1 = h0 + sweep;
    let (cx, cy) = (x1 - r * h0.sin(), y1 + r * h0.cos());
    let (x2, y2) = (cx + r * h1.sin(), cy - r * h1.cos());
    let els = vec![AlignmentElement::line(x, y, h0, l1), AlignmentElement::arc(x1, y1, h0, r, arc_len, Turn::Left), AlignmentElement::line(x2, y2, h1, 90.0)];
    let mut a = Alignment::from_elements("Demo road (example)", 0.0, els).expect("the demonstration alignment is valid");
    let len = a.length();
    a.profile = Profile::new("Demo design profile (example values)", vec![Pvi::point(0.0, 1150.8), Pvi::parabola(len * 0.45, 1153.4, 70.0), Pvi::point(len, 1151.9)]).ok();
    a
}

/// The assembly of the demonstration, as the assembly editor holds it (example values).
pub fn editor_assembly() -> crate::civil_assembly::EdAssembly {
    use crate::civil_assembly::{EdAssembly, EdPart};
    let part = |kind: usize, name: &str| {
        let mut p = EdPart::new(kind);
        p.name = name.to_string();
        p.example();
        p
    };
    let mut lane = part(0, "Lane (example 3.5 m)");
    lane.layers.push(crate::civil_assembly::EdLayer { code: "Sub".into(), depth: "0.2".into(), material: "example material C".into() });
    let right = vec![lane, part(1, "Shoulder (example 1.5 m)"), part(11, "Daylight (example 2:1)")];
    EdAssembly { name: "Demo assembly (example values)".into(), left: right.clone(), right }
}

pub fn assembly() -> Assembly {
    editor_assembly().build().0
}

/// Builds the data. Returns notes for the status line.
pub fn build(doc: &mut Document) -> Vec<String> {
    let mut notes = Vec::new();
    if afrobim_kernel::project_crs(doc).is_none() {
        if let Some(crs) = afrobim_kernel::ProjectCrs::utm_wgs84(36, true) {
            afrobim_kernel::set_project_crs(doc, &crs);
        }
    }
    // Points: a 16 by 16 grid at 20 m, field codes GR (ground), TR (tree), EP (edge of pavement).
    let mut data = PointsData::default();
    data.keys.keys.push(DescriptionKey { code: "GR".into(), full_description: "Ground shot".into(), point_style: "Cross".into(), label_style: "Elevation".into(), layer: "Points".into(), classification: None });
    data.keys.keys.push(DescriptionKey { code: "TR*".into(), full_description: "Tree".into(), point_style: "Tree".into(), label_style: "Description".into(), layer: "Vegetation".into(), classification: None });
    let mut n = 1;
    for i in 0..16 {
        for j in 0..16 {
            let (e, nn) = (E0 + 20.0 * i as f64, N0 + 20.0 * j as f64);
            let code = if (i * 7 + j * 3) % 41 == 0 { "TR1" } else { "GR" };
            let mut p = CogoPoint::new(n, e, nn, Some(ground_z(e, nn)), code);
            p.source.file = Some("Demonstration grid".into());
            let _ = data.store.add(p);
            n += 1;
        }
    }
    let mut g = PointGroup::new("Ground");
    g.include.push(Rule::RawDescription { pattern: "GR*".into() });
    g.surface_points = true;
    let _ = data.groups.add(g);
    let _ = data.keys.clone().apply(&mut data.store);
    data.keys.apply(&mut data.store);
    let ground_pts: Vec<SurfacePoint> = data.store.iter().filter(|p| p.raw_description.starts_with("GR")).filter_map(|p| p.elevation.map(|z| SurfacePoint::new(p.number.to_string(), p.easting, p.northing, z))).collect();
    let b = Bounds::of(data.store.iter().map(|p| (p.easting, p.northing)));
    let pts_id = civil::add(doc, POINTS, "Demo points (example data)", serde_json::to_value(&data).unwrap_or_default(), b, SOURCE);

    // Surface from the Ground group.
    let mut surface = Surface::new("Demo existing ground", SurfaceRole::ExistingGround);
    surface.source.description = SOURCE.into();
    surface.definition.push(Operation::AddPoints { group: "Ground".into(), points: ground_pts.clone() });
    let sb = Bounds::of(ground_pts.iter().map(|p| (p.x, p.y)));
    let sd = SurfaceData { surface, style: SurfaceStyle::example() };
    let surf_id = civil::add(doc, SURFACE, "Demo existing ground (example data)", serde_json::to_value(&sd).unwrap_or_default(), sb, SOURCE);

    // Alignment with a design profile.
    let al = alignment();
    let al_name = al.name.clone();
    if crate::alignment_glue::add(doc, &al).is_none() {
        notes.push("The demonstration alignment could not be added.".into());
    }
    if let Some(layer) = doc.layers_mut().iter_mut().find(|l| l.name == crate::alignment_glue::LAYER) {
        layer.visible = true;
    }
    let len = al.length();

    // Existing ground profile and its profile view, placed south of the site.
    let prof = ProfileData { alignment: al_name.clone(), surface: surf_id.0, interval_m: 10.0, imported: vec![] };
    let prof_id = civil::add(doc, PROFILE, "Demo ground profile (example data)", serde_json::to_value(&prof).unwrap_or_default(), None, SOURCE);
    let pv = ProfileViewData { alignment: al_name.clone(), profiles: vec![prof_id.0], show_design: true, vertical_exaggeration: 5.0, datum: None, insertion: [E0, N0 - 110.0], show_bands: true, band_height_m: 3.0 };
    let pv_b = Bounds { min_e: E0, min_n: N0 - 110.0, max_e: E0 + len, max_n: N0 - 70.0 };
    civil::add(doc, PROFILE_VIEW, "Demo profile view (example values)", serde_json::to_value(&pv).unwrap_or_default(), Some(pv_b), SOURCE);

    // Assembly, corridor.
    let asm = assembly();
    let asm_id = civil::add(doc, ASSEMBLY, "Demo assembly (example values)", serde_json::to_value(&asm).unwrap_or_default(), None, SOURCE);
    if let Some(e) = doc.find_mut(asm_id) {
        e.properties.insert(crate::civil_assembly::PROP_EDITOR.into(), serde_json::to_string(&editor_assembly()).unwrap_or_default());
    }
    let model = CorridorModel {
        name: "Demo corridor (example values)".into(),
        baselines: vec![Baseline { name: "Demo baseline".into(), alignment: al_name.clone(), regions: vec![AssemblyRegion { name: "Whole length".into(), start_station_m: 0.0, end_station_m: len, assembly: asm, frequency: Frequency::uniform(20.0) }], crossfall_table: vec![] }],
        volumes: VolumeOptions::default(),
    };
    let cor = CorridorData { model, ground: surf_id.0, notes: vec![] };
    let cor_id = civil::add(doc, CORRIDOR, "Demo corridor (example values)", serde_json::to_value(&cor).unwrap_or_default(), Some(Bounds { min_e: E0 + 10.0, min_n: N0 + 40.0, max_e: E0 + 270.0, max_n: N0 + 230.0 }), SOURCE);

    // Sample lines every 30 m, 18 m swath each side, with the corridor and the ground.
    let mut grp = SampleLineGroup::new("Demo sample lines", &al);
    if let Err(e) = grp.add_stations(&al, &SampleStations::Interval { interval_m: 30.0, start_station_m: None, end_station_m: None }, 18.0, 18.0, "SL") {
        notes.push(format!("Sample lines: {e}"));
    }
    let sd = SampleData { group: grp, corridor: Some(cor_id.0), surfaces: vec![surf_id.0], insertion: [E0 + 330.0, N0 + 250.0], columns: 4, vertical_exaggeration: 2.0 };
    civil::add(doc, SAMPLES, "Demo sample lines (example values)", serde_json::to_value(&sd).unwrap_or_default(), Some(Bounds { min_e: E0 + 330.0, min_n: N0 + 100.0, max_e: E0 + 500.0, max_n: N0 + 250.0 }), SOURCE);

    // Two parcels the road crosses.
    for (i, (e, nn, w, h, owner)) in [(E0 + 70.0, N0 + 20.0, 110.0, 130.0, "Example owner A"), (E0 + 180.0, N0 + 20.0, 90.0, 130.0, "Example owner B")].into_iter().enumerate() {
        let corners = [Xy::new(e, nn), Xy::new(e + w, nn), Xy::new(e + w, nn + h), Xy::new(e, nn + h)];
        if let Ok(bd) = afrobim_cogo::Boundary::polygon(&corners) {
            let mut p = afrobim_cogo::Parcel::new(&format!("Demo parcel {}", i + 1), (i + 1) as u32, bd);
            p.owner = owner.into();
            p.land_reference = format!("EXAMPLE/{}", i + 1);
            p.land_use = "example".into();
            crate::civil_landxml::add_parcel(doc, &p, SOURCE);
        }
    }
    // The road reserve (example: 15 m each side) splits the parcels: take and remainder pieces, and the schedule.
    match crate::civil_parcels::split_by_reserve(doc, &al, 15.0, 15.0, 0.01, 0.005) {
        Ok(_) => {}
        Err(e) => notes.push(format!("Road reserve split: {e}")),
    }
    notes.extend(pipe_demo(doc, surf_id, &al));
    notes.extend(grading_demo(doc, surf_id));
    let _ = pts_id;
    let _: Option<ElementId> = None;
    notes
}

/// A graded building pad: an outline, a platform level and slopes to the ground, all examples.
fn grading_demo(doc: &mut Document, surf_id: ElementId) -> Vec<String> {
    use afrobim_grading::{building_pad, PlatformSurface, ProjectionParams, Slope};
    let (e, n) = (E0 + 215.0, N0 + 225.0);
    let outline = [[e, n], [e + 36.0, n], [e + 36.0, n + 28.0], [e, n + 28.0]];
    let level = (ground_z(e + 18.0, n + 14.0) * 10.0).round() / 10.0 + 0.4;
    let params = ProjectionParams { ray_spacing: 2.0, facet_angle: 15.0_f64.to_radians() };
    match building_pad("Demo pad (example values)", &outline, &PlatformSurface::Level { level }, Slope::Ratio(2.0), Slope::Ratio(3.0), 40.0, params) {
        Ok(pkg) => {
            crate::civil_grading::add_package(doc, &pkg, Some(surf_id), SOURCE, true);
            vec![]
        }
        Err(e) => vec![format!("Pad: {e}")],
    }
}

/// A small pipe run beside the first straight of the demonstration road: four structures and three
/// pipes from the labelled example parts list. Every number is an example.
fn pipe_demo(doc: &mut Document, surf_id: ElementId, al: &Alignment) -> Vec<String> {
    use afrobim_pipes::{PartFamily, PipeSpec, RunPoint, RunSpec, StructureKind};
    let mut notes = Vec::new();
    let cat = crate::pipes_ui::example_catalogue();
    let Some(part) = cat.parts.iter().find(|p| p.family == PartFamily::Pipe).cloned() else { return vec!["The example parts list has no pipe.".into()] };
    let spart = cat.parts.iter().find(|p| p.family == PartFamily::Structure).cloned();
    let Some(first) = al.elements.first() else { return notes };
    let (x0, y0, h) = (first.x, first.y, first.heading_rad);
    // Offset 10 m to the right of the road's first straight, stations 8, 33, 58 and 83 m.
    let at = |s: f64| (x0 + s * h.cos() + 10.0 * h.sin(), y0 + s * h.sin() - 10.0 * h.cos());
    let pts: Vec<RunPoint> = [8.0, 33.0, 58.0, 83.0].iter().map(|s| { let (e, n) = at(*s); RunPoint { easting_m: e, northing_m: n, rim_level_m: ground_z(e, n) } }).collect();
    let mut data = crate::civil_pipes::PipeNetData::default();
    data.example = true;
    data.network = afrobim_pipes::PipeNetwork::new("Demo pipe network (example values)");
    data.network.network_type = afrobim_pipes::NetworkType::Storm;
    data.network.surface_name = Some("Demo existing ground (example data)".into());
    data.catalogue = cat.clone();
    data.surface = Some(surf_id.0);
    let start_invert = pts[0].rim_level_m - 1.6;
    let spec = RunSpec { structure_kind: StructureKind::Manhole, structure_prefix: "MH".into(), pipe_prefix: "P".into(), pipe: match PipeSpec::from_part(&part) { Ok(s) => s, Err(e) => return vec![e.to_string()] }, start_invert_m: start_invert, slope: 0.012 };
    match data.network.add_run(&pts, &spec) {
        Ok((structs, pipes)) => {
            if let Some(sp) = &spart {
                for s in &structs {
                    let _ = data.network.swap_structure_part(*s, sp);
                }
            }
            for p in &pipes {
                if let Ok(pp) = data.network.pipe_mut(*p) {
                    pp.design_flow_m3_s = Some(0.12);
                }
            }
            let last = *pipes.last().unwrap();
            let end_invert = data.network.pipe(last).map(|p| p.end_invert_m).unwrap_or(0.0);
            data.tailwater_m = Some(end_invert + 0.2);
            data.clearance_m = Some(0.3);
            data.rules.min_slope = Some(0.005);
            data.rules.min_cover_m = Some(0.6);
            let net_id = crate::civil_pipes::add(doc, "Demo pipe network (example values)", &data, SOURCE);
            let (e, n) = (E0, N0 + 150.0);
            let pp = crate::civil_pipes::PipeProfileData { network: net_id.0, pipes: pipes.iter().map(|p| p.0).collect(), alignment: None, surface: Some(surf_id.0), insertion: [e, n], vertical_exaggeration: 5.0, ground_spacing_m: 5.0, tailwater_m: data.tailwater_m };
            civil::add(doc, PIPEPROFILE, "Demo pipe profile (example values)", serde_json::to_value(&pp).unwrap_or_default(), Some(Bounds { min_e: e, min_n: n, max_e: e + 100.0, max_n: n + 30.0 }), SOURCE);
        }
        Err(e) => notes.push(format!("Pipe run: {e}")),
    }
    notes
}

impl crate::app::AfroBimApp {
    /// CIVILDEMO: builds the data, selects the surface and fits the view.
    pub(crate) fn civil_demo(&mut self) {
        let notes = build(&mut self.doc);
        self.sync_generated();
        self.civil_build_now();
        self.schedule_commit_now();
        self.status = format!("Built the civil demonstration data (example values, not a design). {}", notes.join(" "));
        self.fitted = false;
        self.fitted_3d = false;
    }
}
