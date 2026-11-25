// SPDX-License-Identifier: LicenseRef-AfroBIM-Proprietary
// Copyright (c) 2026 AfroBIM. All rights reserved.
//! Cross-sections of a LandXML file as section view objects: one sample line group per alignment
//! built from the stations of the file, each line carrying the file's named section surfaces. The
//! comparison mode puts AfroBIM's own cut of the matching document surface over the file's line
//! at the same station and reports the largest level difference per station.

use afrobim_alignment::{LandXmlCrossSection, LandXmlFile};
use afrobim_corridor::{SampleLineGroup, SectionView};
use afrobim_kernel::{Document, ElementId};

use crate::civil::{self, Bounds, ImportedSection, ImportedSectionSurface, SampleData, SAMPLES};

/// The tag of a line the file gives, in the name of the section's ground line.
pub const FILE_TAG: &str = " (file)";
/// The tag of AfroBIM's own cut of a document surface.
pub const OWN_TAG: &str = " (AfroBIM)";

/// What `import_sections` did, for the import notes.
#[derive(Debug, Default, Clone)]
pub struct SectionsImported {
    pub groups: Vec<ElementId>,
    pub lines: usize,
    pub notes: Vec<String>,
}

/// The largest level difference at one station between the file's line and AfroBIM's cut.
#[derive(Debug, Clone, PartialEq)]
pub struct StationDiff {
    pub label: String,
    pub station_m: f64,
    pub surface: String,
    pub max_diff_m: f64,
    pub at_offset_m: f64,
    /// Offsets compared (vertices of both lines inside both).
    pub samples: usize,
    /// Plan width in metres of the file's line that AfroBIM's cut does not cover.
    pub uncovered_m: f64,
}

/// A level on a set of runs at an offset: the first run that covers it.
fn level_on(runs: &[Vec<(f64, f64)>], offset: f64) -> Option<f64> {
    for run in runs {
        for w in run.windows(2) {
            let (a, b) = (w[0], w[1]);
            let (lo, hi) = if a.0 <= b.0 { (a.0, b.0) } else { (b.0, a.0) };
            if offset >= lo - 1e-9 && offset <= hi + 1e-9 {
                if (b.0 - a.0).abs() < 1e-12 {
                    return Some(a.1);
                }
                return Some(a.1 + (b.1 - a.1) * (offset - a.0) / (b.0 - a.0));
            }
        }
    }
    None
}

fn strip<'a>(name: &'a str, tag: &str) -> Option<&'a str> {
    name.strip_suffix(tag)
}

/// The largest level difference per station and surface, between the file's lines and AfroBIM's
/// cut, for views that carry both (see `civil_build2::sample_views`). Both are piecewise linear,
/// so the largest difference lies at a vertex of either.
pub fn compare_views(views: &[SectionView]) -> Vec<StationDiff> {
    let mut out = Vec::new();
    for v in views {
        for own in v.ground_lines.iter().filter(|g| g.surface.ends_with(OWN_TAG)) {
            let base = strip(&own.surface, OWN_TAG).unwrap_or(&own.surface);
            let Some(file) = v.ground_lines.iter().find(|g| strip(&g.surface, FILE_TAG).is_some_and(|n| n.eq_ignore_ascii_case(base))) else { continue };
            let mut worst = (0.0f64, 0.0f64);
            let mut samples = 0usize;
            let mut offsets: Vec<f64> = file.runs.iter().chain(own.runs.iter()).flat_map(|r| r.iter().map(|p| p.0)).collect();
            offsets.sort_by(f64::total_cmp);
            offsets.dedup_by(|a, b| (*a - *b).abs() < 1e-9);
            for o in offsets {
                if let (Some(a), Some(b)) = (level_on(&file.runs, o), level_on(&own.runs, o)) {
                    samples += 1;
                    if (a - b).abs() > worst.0 {
                        worst = ((a - b).abs(), o);
                    }
                }
            }
            let uncovered: f64 = file.runs.iter().flat_map(|r| r.windows(2)).filter(|w| level_on(&own.runs, (w[0].0 + w[1].0) / 2.0).is_none()).map(|w| (w[1].0 - w[0].0).abs()).sum();
            out.push(StationDiff { label: v.label.clone(), station_m: v.line.station_m, surface: base.to_string(), max_diff_m: worst.0, at_offset_m: worst.1, samples, uncovered_m: uncovered });
        }
    }
    out
}

/// The line name of a section: the name in the file, or the displayed station.
fn line_name(alignment: &afrobim_alignment::Alignment, s: &LandXmlCrossSection) -> String {
    match s.name.as_deref().map(str::trim) {
        Some(n) if !n.is_empty() => n.to_string(),
        _ => format!("Section {}", alignment.station_label(s.station_m, 3)),
    }
}

/// Builds the sample line groups of a file's cross-sections in `doc`. `alignment_names` maps the
/// name of an alignment in the file to its name in the document. Surfaces of the document that
/// carry the name of a section surface (any letter case) are chosen for the comparison.
pub fn import_sections(doc: &mut Document, file: &LandXmlFile, alignment_names: &[(String, String)], source: &str, insertion_origin: (f64, f64)) -> SectionsImported {
    let mut res = SectionsImported::default();
    let mut next_x = insertion_origin.0;
    for (file_name, doc_name) in alignment_names {
        let sections: Vec<&LandXmlCrossSection> = file.cross_sections.iter().filter(|s| &s.alignment == file_name).collect();
        if sections.is_empty() {
            continue;
        }
        let Some(alignment) = crate::alignment_glue::alignment_by_name(doc, doc_name) else { continue };
        let group_title = file.cross_sect_groups.iter().find(|g| &g.alignment == file_name).and_then(|g| g.name.clone()).filter(|n| !n.trim().is_empty());
        let mut group = SampleLineGroup::new(&doc.unique_name(&format!("Sections {}", group_title.unwrap_or_else(|| doc_name.clone()))), &alignment);
        let (start, end) = (alignment.start_station_m, alignment.end_station_m());
        let mut imported = Vec::new();
        let mut names: Vec<String> = Vec::new();
        let (mut widest, mut skipped) = (0.0f64, 0usize);
        for s in &sections {
            let pts: Vec<f64> = s.surfaces.iter().flat_map(|x| x.point_lists.iter().flatten().map(|p| p[0])).collect();
            if pts.is_empty() {
                skipped += 1;
                continue;
            }
            let left = pts.iter().cloned().fold(0.0f64, |a, b| a.max(-b));
            let right = pts.iter().cloned().fold(0.0f64, |a, b| a.max(b));
            if left + right <= 0.0 {
                skipped += 1;
                continue;
            }
            // The station is as the file gives it; one within a millimetre of an end is moved onto it.
            let sta = if (s.station_m - start).abs() < 1e-3 { start } else if (s.station_m - end).abs() < 1e-3 { end } else { s.station_m };
            if !(start - 1e-9..=end + 1e-9).contains(&sta) {
                skipped += 1;
                continue;
            }
            let name = line_name(&alignment, s);
            if group.add_line(&name, sta, left, right).is_err() {
                skipped += 1;
                continue;
            }
            widest = widest.max(left + right);
            let surfaces: Vec<ImportedSectionSurface> = s.surfaces.iter().map(|x| ImportedSectionSurface { name: x.name.clone(), runs: x.point_lists.clone() }).collect();
            for x in &surfaces {
                if !names.iter().any(|n| n == &x.name) {
                    names.push(x.name.clone());
                }
            }
            imported.push(ImportedSection { line: name, station_m: sta, surfaces });
        }
        if imported.is_empty() {
            res.notes.push(format!("Sections of {doc_name}: none could be placed on the alignment."));
            continue;
        }
        if skipped > 0 {
            res.notes.push(format!("Sections of {doc_name}: {skipped} section(s) left out (no points, or a station off the alignment)."));
        }
        // The surfaces of the document that carry a section surface's name (any letter case).
        let mut surface_ids: Vec<u64> = Vec::new();
        for e in doc.elements() {
            if (e.category == civil::SURFACE || e.category == afrobim_corridor::SURFACE_CATEGORY) && names.iter().any(|n| n.eq_ignore_ascii_case(&e.name)) {
                surface_ids.push(e.id.0);
            }
        }
        let cols = 10usize;
        let cell_w = widest + 8.0;
        let data = SampleData { group, corridor: None, surfaces: surface_ids, insertion: [next_x, insertion_origin.1], columns: cols, vertical_exaggeration: 1.0, slices: vec![], imported, compare: false };
        next_x += cols as f64 * cell_w + 150.0;
        res.lines += data.group.lines.len();
        let name = data.group.name.clone();
        let b = Bounds::point(data.insertion[0], data.insertion[1]);
        let id = civil::add(doc, SAMPLES, &name, serde_json::to_value(&data).unwrap_or_default(), Some(b), source);
        res.groups.push(id);
    }
    res
}

/// The comparison of a group's views, as lines of text for the command history and the Properties
/// panel: the largest difference over the group and the station it is at.
pub fn summary(diffs: &[StationDiff]) -> Vec<String> {
    let mut out = Vec::new();
    if diffs.is_empty() {
        out.push("No section has both a file line and an AfroBIM cut of the same name (the document needs a surface with the name of a section surface).".into());
        return out;
    }
    let worst = diffs.iter().max_by(|a, b| a.max_diff_m.total_cmp(&b.max_diff_m)).unwrap();
    out.push(format!("{} station(s) compared; largest level difference {:.1} mm at station {} ({}, offset {:+.2} m).", diffs.len(), worst.max_diff_m * 1000.0, worst.label, worst.surface, worst.at_offset_m));
    let uncovered = diffs.iter().filter(|d| d.uncovered_m > 1e-6).count();
    if uncovered > 0 {
        out.push(format!("{uncovered} station(s) have parts of the file's line outside the surface (not compared there)."));
    }
    out
}

/// The largest level difference between a ground profile of the file and AfroBIM's own profile
/// sampled from a surface along the alignment, at the stations of the file.
#[derive(Debug, Clone, PartialEq)]
pub struct ProfileDiff {
    pub max_diff_m: f64,
    pub at_station_m: f64,
    pub compared: usize,
    /// Stations of the file where the surface has no level.
    pub off_surface: usize,
}

pub fn compare_profile(alignment: &afrobim_alignment::Alignment, file_points: &[[f64; 2]], tin: &afrobim_corridor::Tin) -> ProfileDiff {
    let mut d = ProfileDiff { max_diff_m: 0.0, at_station_m: 0.0, compared: 0, off_surface: 0 };
    for p in file_points {
        match alignment.point_at(p[0]).and_then(|q| tin.elevation_at(q.x, q.y)) {
            Some(z) => {
                d.compared += 1;
                if (z - p[1]).abs() > d.max_diff_m {
                    d.max_diff_m = (z - p[1]).abs();
                    d.at_station_m = p[0];
                }
            }
            None => d.off_surface += 1,
        }
    }
    d
}

/// The surface a ground profile of the file was taken from: the one that covers most of its
/// stations, and among those the one with the smallest largest difference.
pub fn best_surface(alignment: &afrobim_alignment::Alignment, file_points: &[[f64; 2]], tins: &[(u64, std::sync::Arc<afrobim_corridor::Tin>)]) -> Option<(u64, ProfileDiff)> {
    let mut best: Option<(u64, ProfileDiff)> = None;
    for (id, tin) in tins {
        let d = compare_profile(alignment, file_points, tin);
        if d.compared == 0 {
            continue;
        }
        let better = match &best {
            None => true,
            Some((_, b)) => d.compared > b.compared || (d.compared == b.compared && d.max_diff_m < b.max_diff_m),
        };
        if better {
            best = Some((*id, d));
        }
    }
    best
}

/// The TINs of the surfaces of a document that carry their triangles in their data (the imported
/// ones), by element id. A surface kept as source data is not here: it has no TIN until it is built.
pub fn stored_tins(doc: &Document) -> Vec<(u64, std::sync::Arc<afrobim_corridor::Tin>)> {
    doc.elements().iter().filter(|e| e.category == afrobim_corridor::SURFACE_CATEGORY).filter_map(|e| serde_json::from_value::<afrobim_corridor::SurfaceDefinition>(e.data.clone()).ok().map(|d| (e.id.0, std::sync::Arc::new(d.tin)))).collect()
}

/// Gives each ground profile just imported the surface it was cut from, when the TINs say which.
pub fn assign_profile_surfaces(doc: &mut Document, profiles: &[ElementId]) -> Vec<String> {
    let tins = stored_tins(doc);
    let mut notes = Vec::new();
    if tins.is_empty() {
        return notes;
    }
    for id in profiles {
        let Some(d) = civil::get::<civil::ProfileData>(doc, *id, civil::PROFILE) else { continue };
        let Some(al) = crate::alignment_glue::alignment_by_name(doc, &d.alignment) else { continue };
        if let Some((sid, diff)) = best_surface(&al, &d.imported, &tins) {
            let mut nd = d.clone();
            nd.surface = sid;
            civil::set(doc, *id, serde_json::to_value(&nd).unwrap_or_default(), None);
            let name = doc.find(*id).map(|e| e.name.clone()).unwrap_or_default();
            notes.push(format!("Ground profile {name}: AfroBIM's own profile from the surface differs from the file's by at most {:.1} mm (at station {:.3}, {} stations compared, {} off the surface).", diff.max_diff_m * 1000.0, diff.at_station_m, diff.compared, diff.off_surface));
        }
    }
    notes
}
