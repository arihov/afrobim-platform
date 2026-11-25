// SPDX-License-Identifier: LicenseRef-AfroBIM-Proprietary
// Copyright (c) 2026 AfroBIM. All rights reserved.
//! Corridor targets as the interface edits them: an offset target (a fixed offset, a station table or
//! a plan line taken from an alignment or a feature line by name), an elevation target (a fixed level,
//! a station table, the profile of an alignment or the alignment nearest the section point), and the
//! list of targets an assembly declares. Used by the assembly editor and by the corridor dialog.
//!
//! The engine stores a target as data (a table, a polyline, a profile or an alignment), not as a
//! name, so the interface keeps the name beside it (`TargetRef`) and takes the data from the
//! document again whenever the name is chosen or the form is saved. Nothing here is a design value:
//! a new table is empty and an empty table is reported as not yet bound.

use afrobim_alignment::Alignment;
use afrobim_corridor::{polyline_from_alignment, Assembly, Dim, ElevationTarget, LinkMode, OffsetTarget, Side, SubKind};
use afrobim_kernel::Document;
use egui::{RichText, Ui};
use serde::{Deserialize, Serialize};

use crate::ui_kit::{self, Level, NumRule};

/// The name a target was taken from, kept beside the data the engine stores.
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
pub struct TargetRef {
    /// "", "alignment", "featureline" or "entered".
    pub kind: String,
    pub name: String,
    /// The spacing of the points sampled along an alignment, as typed.
    pub step: String,
}

/// The objects of the document a target can be taken from.
#[derive(Default, Clone)]
pub struct DocObjs {
    pub aligns: Vec<(String, Alignment)>,
    pub lines: Vec<(String, Vec<(f64, f64)>)>,
}

impl DocObjs {
    pub fn gather(doc: &Document) -> DocObjs {
        let mut o = DocObjs::default();
        for (id, name) in crate::alignment_glue::alignments(doc) {
            if let Some(a) = crate::alignment_glue::alignment_of(doc, id) {
                o.aligns.push((name, a));
            }
        }
        for (id, name) in crate::civil::of_category(doc, crate::civil::FEATURELINE) {
            if let Some(d) = crate::civil::get::<crate::civil_grading::FeatureLineData>(doc, id, crate::civil::FEATURELINE) {
                if let Ok(flat) = d.line.flatten(crate::civil_grading::DRAW_FACET_RAD) {
                    o.lines.push((name, flat.iter().map(|p| (p.p[0], p.p[1])).collect()));
                }
            }
        }
        o
    }
    fn align(&self, name: &str) -> Option<&Alignment> {
        self.aligns.iter().find(|a| a.0 == name).map(|a| &a.1)
    }
}

/// Takes the plan line of an offset target from the document again.
pub fn rebind_offset(t: &mut OffsetTarget, r: &TargetRef, o: &DocObjs) -> Result<(), String> {
    match r.kind.as_str() {
        "alignment" => {
            let step = r.step.trim().parse::<f64>().ok().filter(|v| v.is_finite() && *v > 0.0).ok_or("enter the spacing of the sampled points (greater than 0)")?;
            let a = o.align(&r.name).ok_or_else(|| format!("the alignment '{}' is not in the document", r.name))?;
            *t = OffsetTarget::Polyline(polyline_from_alignment(a, step)?);
            Ok(())
        }
        "featureline" => {
            let l = o.lines.iter().find(|l| l.0 == r.name).ok_or_else(|| format!("the feature line '{}' is not in the document", r.name))?;
            *t = OffsetTarget::Polyline(l.1.clone());
            Ok(())
        }
        _ => Ok(()),
    }
}

/// Takes the profile or the alignment of an elevation target from the document again.
pub fn rebind_elevation(t: &mut ElevationTarget, r: &TargetRef, o: &DocObjs) -> Result<(), String> {
    match r.kind.as_str() {
        "profile" => {
            let a = o.align(&r.name).ok_or_else(|| format!("the alignment '{}' is not in the document", r.name))?;
            let p = a.profile.clone().ok_or_else(|| format!("the alignment '{}' has no design profile", r.name))?;
            *t = ElevationTarget::Profile(p);
            Ok(())
        }
        "alignment" => {
            let a = o.align(&r.name).ok_or_else(|| format!("the alignment '{}' is not in the document", r.name))?;
            *t = ElevationTarget::NearestOnAlignment(Box::new(a.clone()));
            Ok(())
        }
        _ => Ok(()),
    }
}

/// Why an offset target cannot be used yet, or `None` when it is bound.
pub fn offset_status(t: &OffsetTarget) -> Option<String> {
    match t {
        OffsetTarget::Fixed(_) => None,
        OffsetTarget::Table(v) if v.is_empty() => Some("the station table has no rows".into()),
        OffsetTarget::Table(v) if !increasing(v) => Some("the stations of the table must increase".into()),
        OffsetTarget::Table(_) => None,
        OffsetTarget::Polyline(v) if v.len() < 2 => Some("no plan line is chosen (an alignment, a feature line or at least two points)".into()),
        OffsetTarget::Polyline(_) => None,
    }
}

pub fn elevation_status(t: &ElevationTarget) -> Option<String> {
    match t {
        ElevationTarget::Fixed(_) => None,
        ElevationTarget::Table(v) if v.is_empty() => Some("the level table has no rows".into()),
        ElevationTarget::Table(v) if !increasing(v) => Some("the stations of the table must increase".into()),
        ElevationTarget::Table(_) => None,
        ElevationTarget::Profile(p) if p.pvis.len() < 2 => Some("no profile is chosen".into()),
        ElevationTarget::Profile(_) => None,
        ElevationTarget::NearestOnAlignment(a) if a.profile.is_none() => Some(format!("the alignment '{}' has no design profile", a.name)),
        ElevationTarget::NearestOnAlignment(_) => None,
    }
}

fn increasing(v: &[(f64, f64)]) -> bool {
    v.windows(2).all(|w| w[1].0 > w[0].0)
}

fn kind_of_offset(t: &OffsetTarget) -> usize {
    match t {
        OffsetTarget::Fixed(_) => 0,
        OffsetTarget::Table(_) => 1,
        OffsetTarget::Polyline(_) => 2,
    }
}

fn kind_of_elevation(t: &ElevationTarget) -> usize {
    match t {
        ElevationTarget::Fixed(_) => 0,
        ElevationTarget::Table(_) => 1,
        ElevationTarget::Profile(_) => 2,
        ElevationTarget::NearestOnAlignment(_) => 3,
    }
}

/// An inline table of two columns (a station and a value, or an x and a y).
pub fn table_ui(ui: &mut Ui, id: &str, rows: &mut Vec<(f64, f64)>, heads: (&str, &str)) {
    let mut remove = None;
    egui::ScrollArea::vertical().id_salt(("tgt-scroll", id)).max_height(150.0).auto_shrink([true, true]).show(ui, |ui| {
        egui::Grid::new(("tgt-table", id)).num_columns(3).spacing([10.0, 4.0]).show(ui, |ui| {
            ui_kit::th(ui, heads.0);
            ui_kit::th(ui, heads.1);
            ui.label("");
            ui.end_row();
            for (i, r) in rows.iter_mut().enumerate() {
                ui.add(egui::DragValue::new(&mut r.0).speed(1.0).min_decimals(0).max_decimals(4));
                ui.add(egui::DragValue::new(&mut r.1).speed(0.05).min_decimals(0).max_decimals(4));
                if ui_kit::icon_button(ui, crate::glyphs::ui::TRASH, "Remove this row").clicked() {
                    remove = Some(i);
                }
                ui.end_row();
            }
        });
    });
    if let Some(i) = remove {
        rows.remove(i);
    }
    if ui.button("Add row").clicked() {
        let next = rows.last().copied().unwrap_or((0.0, 0.0));
        rows.push(next);
    }
}

/// Edits an offset target: a fixed offset, a station table or a plan line.
pub fn offset_ui(ui: &mut Ui, id: &str, t: &mut OffsetTarget, r: &mut TargetRef, o: &DocObjs) {
    let mut k = kind_of_offset(t);
    ui.vertical(|ui| {
        if ui_kit::segmented(ui, &mut k, &["Fixed", "Station table", "Plan line"]) {
            *t = match k {
                0 => OffsetTarget::Fixed(0.0),
                1 => OffsetTarget::Table(vec![]),
                _ => OffsetTarget::Polyline(vec![]),
            };
            *r = TargetRef::default();
        }
        match t {
            OffsetTarget::Fixed(v) => {
                ui_kit::f64_field(ui, v, "m, positive to the right of the alignment", -100000.0..=100000.0, 0.05);
            }
            OffsetTarget::Table(rows) => {
                table_ui(ui, id, rows, ("Station (m)", "Offset (m)"));
            }
            OffsetTarget::Polyline(_) => {
                let mut labels: Vec<String> = o.aligns.iter().map(|a| format!("Alignment: {}", a.0)).collect();
                labels.extend(o.lines.iter().map(|l| format!("Feature line: {}", l.0)));
                labels.push("Points entered here".into());
                let entered = labels.len() - 1;
                let mut sel = match r.kind.as_str() {
                    "alignment" => o.aligns.iter().position(|a| a.0 == r.name),
                    "featureline" => o.lines.iter().position(|l| l.0 == r.name).map(|i| i + o.aligns.len()),
                    "entered" => Some(entered),
                    _ => None,
                };
                let shown = sel.map(|i| labels[i].clone()).unwrap_or_else(|| "(choose a plan line)".into());
                let mut changed = false;
                egui::ComboBox::from_id_salt(("tgt-src", id)).selected_text(shown).width(260.0).show_ui(ui, |ui| {
                    for (i, l) in labels.iter().enumerate() {
                        if ui.selectable_label(sel == Some(i), l).clicked() {
                            sel = Some(i);
                            changed = true;
                        }
                    }
                });
                if changed {
                    let i = sel.unwrap_or(entered);
                    let step = r.step.clone();
                    *r = if i < o.aligns.len() {
                        TargetRef { kind: "alignment".into(), name: o.aligns[i].0.clone(), step }
                    } else if i < entered {
                        TargetRef { kind: "featureline".into(), name: o.lines[i - o.aligns.len()].0.clone(), step }
                    } else {
                        TargetRef { kind: "entered".into(), ..Default::default() }
                    };
                    if r.kind == "entered" {
                        *t = OffsetTarget::Polyline(vec![]);
                    } else {
                        let _ = rebind_offset(t, r, o);
                    }
                }
                if r.kind == "alignment" {
                    let before = r.step.clone();
                    ui.horizontal(|ui| {
                        ui.label("Point spacing");
                        ui_kit::num_field(ui, &mut r.step, "m", &NumRule::required().above(0.0));
                    });
                    if r.step != before {
                        let _ = rebind_offset(t, r, o);
                    }
                    ui.label(RichText::new("Small against the curve radius: the line cuts the chord on a curve.").size(crate::theme::FONT_SMALL).weak());
                }
                if let (OffsetTarget::Polyline(pts), "entered") = (&mut *t, r.kind.as_str()) {
                    table_ui(ui, id, pts, ("Easting (m)", "Northing (m)"));
                } else if let OffsetTarget::Polyline(pts) = t {
                    if r.kind == "alignment" || r.kind == "featureline" {
                        ui.label(RichText::new(format!("{} points taken from the document.", pts.len())).size(crate::theme::FONT_SMALL).weak());
                    }
                }
            }
        }
        if let Some(why) = offset_status(t) {
            ui_kit::callout(ui, Level::Warning, format!("Not yet bound: {why}."));
        }
    });
}

/// Edits an elevation target: a fixed level, a station table, a profile or the nearest alignment.
pub fn elevation_ui(ui: &mut Ui, id: &str, t: &mut ElevationTarget, r: &mut TargetRef, o: &DocObjs) {
    if r.kind.is_empty() {
        match &*t {
            ElevationTarget::Profile(p) => {
                if let Some(a) = o.aligns.iter().find(|a| a.1.profile.as_ref() == Some(p)) {
                    *r = TargetRef { kind: "profile".into(), name: a.0.clone(), step: String::new() };
                }
            }
            ElevationTarget::NearestOnAlignment(a) => *r = TargetRef { kind: "alignment".into(), name: a.name.clone(), step: String::new() },
            _ => {}
        }
    }
    let mut k = kind_of_elevation(t);
    ui.vertical(|ui| {
        if ui_kit::combo(ui, &format!("{id}-kind"), &mut k, &["Fixed level", "Station table", "Profile of an alignment", "Alignment nearest the section point"]) {
            *t = match k {
                0 => ElevationTarget::Fixed(0.0),
                1 => ElevationTarget::Table(vec![]),
                2 => ElevationTarget::Table(vec![]),
                _ => ElevationTarget::Table(vec![]),
            };
            *r = TargetRef::default();
            if k >= 2 {
                r.kind = if k == 2 { "profile".into() } else { "alignment".into() };
            }
        }
        match t {
            ElevationTarget::Fixed(v) => {
                ui_kit::f64_field(ui, v, "m, level", -100000.0..=100000.0, 0.05);
            }
            ElevationTarget::Table(rows) if r.kind.is_empty() => {
                table_ui(ui, id, rows, ("Station (m)", "Level (m)"));
            }
            _ => {
                let names: Vec<String> = o.aligns.iter().map(|a| if a.1.profile.is_some() { a.0.clone() } else { format!("{} (no design profile)", a.0) }).collect();
                let mut sel = o.aligns.iter().position(|a| a.0 == r.name).unwrap_or(usize::MAX);
                let shown = names.get(sel).cloned().unwrap_or_else(|| "(choose an alignment)".into());
                let mut changed = false;
                egui::ComboBox::from_id_salt(("tgt-al", id)).selected_text(shown).width(260.0).show_ui(ui, |ui| {
                    for (i, n) in names.iter().enumerate() {
                        if ui.selectable_label(sel == i, n).clicked() {
                            sel = i;
                            changed = true;
                        }
                    }
                });
                if changed {
                    r.name = o.aligns[sel].0.clone();
                    let _ = rebind_elevation(t, r, o);
                }
                let what = if r.kind == "profile" { "Read at the station of the baseline." } else { "Read at the station of that alignment nearest the point the link ends at." };
                ui.label(RichText::new(what).size(crate::theme::FONT_SMALL).weak());
            }
        }
        if let Some(why) = elevation_status(t) {
            ui_kit::callout(ui, Level::Warning, format!("Not yet bound: {why}."));
        }
    });
}

/// Edits a width: entered, or the distance to an offset target.
pub fn dim_ui(ui: &mut Ui, id: &str, d: &mut Dim, r: &mut TargetRef, o: &DocObjs) {
    let mut to = matches!(d, Dim::ToOffset(_));
    ui.vertical(|ui| {
        if ui_kit::segmented(ui, &mut (to as usize), &["Entered", "To an offset target"]) {
            to = !to;
            *d = if to { Dim::ToOffset(OffsetTarget::Table(vec![])) } else { Dim::Fixed(0.0) };
            *r = TargetRef::default();
        }
        match d {
            Dim::Fixed(w) => {
                ui_kit::f64_field(ui, w, "m", 0.0..=1000.0, 0.05);
            }
            Dim::ToOffset(t) => {
                ui.label(RichText::new("The width runs from the attachment point to the target and must be greater than 0 where it is evaluated.").size(crate::theme::FONT_SMALL).weak());
                offset_ui(ui, id, t, r, o);
            }
        }
    });
}

// ─── the targets an assembly declares ──────────────────────────────────────

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Role {
    Width,
    Offset,
    Elevation,
}

impl Role {
    pub fn key(self) -> &'static str {
        match self {
            Role::Width => "width",
            Role::Offset => "offset",
            Role::Elevation => "elevation",
        }
    }
}

#[derive(Debug, Clone)]
pub struct Declared {
    pub side: Side,
    pub index: usize,
    pub role: Role,
    pub part: String,
    pub label: String,
}

impl Declared {
    pub fn key(&self) -> String {
        format!("{}-{}-{}", if self.side == Side::Left { "l" } else { "r" }, self.index, self.role.key())
    }
}

fn parts(a: &Assembly, side: Side) -> &[afrobim_corridor::Subassembly] {
    if side == Side::Left {
        &a.left
    } else {
        &a.right
    }
}

/// Every target the assembly declares: strip widths that run to an offset, and every target of a
/// generic link.
pub fn declared(a: &Assembly) -> Vec<Declared> {
    let mut out = Vec::new();
    for side in [Side::Left, Side::Right] {
        for (i, p) in parts(a, side).iter().enumerate() {
            let mut add = |role: Role, label: &str| out.push(Declared { side, index: i, role, part: p.name.clone(), label: label.into() });
            match &p.kind {
                SubKind::Lane(s) | SubKind::Shoulder(s) | SubKind::Footway(s) | SubKind::Verge(s) => {
                    if matches!(s.width, Dim::ToOffset(_)) {
                        add(Role::Width, "width runs to an offset");
                    }
                }
                SubKind::KerbAndChannel { channel, .. } => {
                    if matches!(channel.width, Dim::ToOffset(_)) {
                        add(Role::Width, "channel width runs to an offset");
                    }
                }
                SubKind::Link(l) => match &l.mode {
                    LinkMode::WidthSlope { width: Dim::ToOffset(_), .. } => add(Role::Width, "width runs to an offset"),
                    LinkMode::WidthSlope { .. } => {}
                    LinkMode::ToElevation { .. } => add(Role::Elevation, "runs to a level"),
                    LinkMode::ToOffset { .. } => add(Role::Offset, "runs to an offset"),
                    LinkMode::ToOffsetAndElevation { .. } => {
                        add(Role::Offset, "ends at an offset");
                        add(Role::Elevation, "and at a level");
                    }
                },
                _ => {}
            }
        }
    }
    out
}

pub fn offset_mut<'a>(a: &'a mut Assembly, d: &Declared) -> Option<&'a mut OffsetTarget> {
    let v = if d.side == Side::Left { &mut a.left } else { &mut a.right };
    match &mut v.get_mut(d.index)?.kind {
        SubKind::Lane(s) | SubKind::Shoulder(s) | SubKind::Footway(s) | SubKind::Verge(s) => match &mut s.width {
            Dim::ToOffset(t) if d.role == Role::Width => Some(t),
            _ => None,
        },
        SubKind::KerbAndChannel { channel, .. } => match &mut channel.width {
            Dim::ToOffset(t) if d.role == Role::Width => Some(t),
            _ => None,
        },
        SubKind::Link(l) => match &mut l.mode {
            LinkMode::WidthSlope { width: Dim::ToOffset(t), .. } if d.role == Role::Width => Some(t),
            LinkMode::ToOffset { target, .. } if d.role == Role::Offset => Some(target),
            LinkMode::ToOffsetAndElevation { offset, .. } if d.role == Role::Offset => Some(offset),
            _ => None,
        },
        _ => None,
    }
}

pub fn elevation_mut<'a>(a: &'a mut Assembly, d: &Declared) -> Option<&'a mut ElevationTarget> {
    let v = if d.side == Side::Left { &mut a.left } else { &mut a.right };
    match &mut v.get_mut(d.index)?.kind {
        SubKind::Link(l) => match &mut l.mode {
            LinkMode::ToElevation { target, .. } if d.role == Role::Elevation => Some(target),
            LinkMode::ToOffsetAndElevation { elevation, .. } if d.role == Role::Elevation => Some(elevation),
            _ => None,
        },
        _ => None,
    }
}

/// The status of one declared target, `None` when bound.
pub fn declared_status(a: &mut Assembly, d: &Declared) -> Option<String> {
    if d.role == Role::Elevation {
        elevation_mut(a, d).and_then(|t| elevation_status(t))
    } else {
        offset_mut(a, d).and_then(|t| offset_status(t))
    }
}

/// Edits one declared target of an assembly.
pub fn declared_ui(ui: &mut Ui, id: &str, a: &mut Assembly, d: &Declared, r: &mut TargetRef, o: &DocObjs) {
    if d.role == Role::Elevation {
        if let Some(t) = elevation_mut(a, d) {
            elevation_ui(ui, id, t, r, o);
        }
    } else if let Some(t) = offset_mut(a, d) {
        offset_ui(ui, id, t, r, o);
    }
}

/// Takes every named target of an assembly from the document again; the problems, in words.
pub fn rebind_all(a: &mut Assembly, refs: &std::collections::BTreeMap<String, TargetRef>, o: &DocObjs) -> Vec<String> {
    let mut problems = Vec::new();
    for d in declared(a) {
        let Some(r) = refs.get(&d.key()) else { continue };
        if r.kind.is_empty() || r.kind == "entered" {
            continue;
        }
        let res = if d.role == Role::Elevation { elevation_mut(a, &d).map(|t| rebind_elevation(t, r, o)) } else { offset_mut(a, &d).map(|t| rebind_offset(t, r, o)) };
        if let Some(Err(e)) = res {
            problems.push(format!("{}: {e}", d.part));
        }
    }
    problems
}

/// As `rebind_all` for one part whose references are keyed by role alone (width, offset, elevation).
pub fn rebind_sub(sub: &mut afrobim_corridor::Subassembly, refs: &std::collections::BTreeMap<String, TargetRef>, o: &DocObjs) -> Vec<String> {
    let mut a = Assembly::new("part");
    a.right.push(sub.clone());
    let keyed = refs.iter().map(|(k, v)| (format!("r-0-{k}"), v.clone())).collect();
    let problems = rebind_all(&mut a, &keyed, o);
    if let Some(p) = a.right.pop() {
        *sub = p;
    }
    problems
}

#[cfg(test)]
mod tests {
    use super::*;
    use afrobim_alignment::{AlignmentElement, Profile, Pvi};
    use afrobim_corridor::{Subassembly, Slope};

    fn objs() -> DocObjs {
        let mut al = Alignment::from_elements("Side road", 0.0, vec![AlignmentElement::line(0.0, 10.0, 100.0, 10.0)]).unwrap();
        al.profile = Profile::new("p", vec![Pvi::point(0.0, 5.0), Pvi::point(100.0, 6.0)]).ok();
        DocObjs { aligns: vec![("Side road".into(), al)], lines: vec![("Edge".into(), vec![(0.0, 5.0), (50.0, 5.0)])] }
    }

    #[test]
    fn an_empty_table_is_not_bound_and_a_named_line_is() {
        assert!(offset_status(&OffsetTarget::Table(vec![])).is_some());
        assert!(offset_status(&OffsetTarget::Table(vec![(0.0, 1.0), (0.0, 2.0)])).is_some());
        assert!(offset_status(&OffsetTarget::Table(vec![(0.0, 1.0), (10.0, 2.0)])).is_none());
        let o = objs();
        let mut t = OffsetTarget::Polyline(vec![]);
        assert!(offset_status(&t).is_some());
        let r = TargetRef { kind: "alignment".into(), name: "Side road".into(), step: "10".into() };
        rebind_offset(&mut t, &r, &o).unwrap();
        assert!(offset_status(&t).is_none());
        let bad = TargetRef { kind: "alignment".into(), name: "Side road".into(), step: String::new() };
        assert!(rebind_offset(&mut t, &bad, &o).is_err());
        let fl = TargetRef { kind: "featureline".into(), name: "Edge".into(), step: String::new() };
        rebind_offset(&mut t, &fl, &o).unwrap();
        assert_eq!(t, OffsetTarget::Polyline(vec![(0.0, 5.0), (50.0, 5.0)]));
        let mut z = ElevationTarget::Table(vec![]);
        rebind_elevation(&mut z, &TargetRef { kind: "profile".into(), name: "Side road".into(), step: String::new() }, &o).unwrap();
        assert!(matches!(z, ElevationTarget::Profile(_)) && elevation_status(&z).is_none());
        rebind_elevation(&mut z, &TargetRef { kind: "alignment".into(), name: "Side road".into(), step: String::new() }, &o).unwrap();
        assert!(matches!(z, ElevationTarget::NearestOnAlignment(_)));
    }

    #[test]
    fn declared_targets_of_an_assembly_are_listed_and_reached() {
        use afrobim_corridor::library;
        let mut a = Assembly::new("a");
        a.right.push(library::link_to_offset_and_elevation("Tie", &["Top"], "ETW", OffsetTarget::Table(vec![]), ElevationTarget::Fixed(1.0)));
        a.right.push(library::lane_to_offset("Widening", OffsetTarget::Table(vec![(0.0, 4.0), (10.0, 5.0)]), Slope::Percent(-2.0), vec![]));
        a.right.push(library::lane("Plain", 3.0, Slope::Percent(-2.0), vec![]));
        let d = declared(&a);
        assert_eq!(d.iter().map(|x| x.role.key()).collect::<Vec<_>>(), vec!["offset", "elevation", "width"]);
        assert!(declared_status(&mut a, &d[0]).is_some());
        assert!(declared_status(&mut a, &d[1]).is_none());
        assert!(declared_status(&mut a, &d[2]).is_none());
        *offset_mut(&mut a, &d[0]).unwrap() = OffsetTarget::Fixed(7.0);
        assert!(declared_status(&mut a, &d[0]).is_none());
        let _: Option<Subassembly> = None;
    }

    #[test]
    fn a_lane_that_runs_to_a_target_builds_with_a_width_target() {
        let mut p = crate::civil_assembly::EdPart::new(0);
        p.vals = vec![String::new(), "-2".into()];
        p.width_target = Some(OffsetTarget::Table(vec![(0.0, 4.0), (10.0, 5.0)]));
        let s = p.build().unwrap();
        assert!(matches!(&s.kind, SubKind::Lane(st) if matches!(st.width, Dim::ToOffset(_))));
        assert!(crate::civil_assembly::EdPart::new_link().build().is_err());
    }
}
