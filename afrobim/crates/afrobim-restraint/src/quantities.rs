//! Take-off for barrier runs.
//!
//! Every figure is the entered run put through the geometry of its alignment: length along the
//! offset line ([`crate::ResolvedRun::length_m`]), posts from the entered spacing
//! ([`crate::post_count`]), and the end treatments counted as entered. Nothing comes from a
//! display mesh, a product catalogue or a rate.

use std::collections::BTreeMap;

use afrobim_kernel::Element;

use crate::{fixed, BarrierElement, BarrierRun, ContainmentLevel, EndTreatment, ResolvedRun, WorkingWidthClass, CATEGORY};

/// One run measured.
#[derive(Debug, Clone, PartialEq)]
pub struct RunQuantities {
    pub mark: String,
    pub alignment: String,
    /// Station labels of the two ends (displayed stations).
    pub start_station: String,
    pub end_station: String,
    pub offset_m: f64,
    pub traffic_face: String,
    pub containment: ContainmentLevel,
    pub working_width: WorkingWidthClass,
    pub impact_severity: String,
    pub system: String,
    /// Length along the offset line between the two ends of the barrier, m. Terminals are not
    /// in it.
    pub length_m: f64,
    /// End station − start station, m, for comparison.
    pub centreline_length_m: f64,
    pub post_spacing_m: Option<f64>,
    /// 0 for a run without posts.
    pub posts: usize,
    pub start_end: EndTreatment,
    pub end_end: EndTreatment,
}

impl RunQuantities {
    pub fn measure(run: &BarrierRun, resolved: &ResolvedRun) -> RunQuantities {
        RunQuantities {
            mark: run.name.clone(),
            alignment: run.alignment.clone(),
            start_station: resolved.start_label.clone(),
            end_station: resolved.end_label.clone(),
            offset_m: run.offset_m,
            traffic_face: run.traffic_face.label().to_string(),
            containment: run.containment,
            working_width: run.working_width,
            impact_severity: run.impact_severity.label().to_string(),
            system: run.system.clone(),
            length_m: resolved.length_m,
            centreline_length_m: resolved.centreline_length_m,
            post_spacing_m: run.post_spacing_m,
            posts: resolved.posts.len(),
            start_end: run.start_end.clone(),
            end_end: run.end_end.clone(),
        }
    }

    fn ends(&self) -> [&EndTreatment; 2] {
        [&self.start_end, &self.end_end]
    }
}

/// A run that could not be measured, and why.
#[derive(Debug, Clone, PartialEq)]
pub struct SkippedRun {
    pub mark: String,
    pub reason: String,
}

/// Totals for one combination of containment level and working width class.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ClassTotal {
    pub containment: ContainmentLevel,
    pub working_width: WorkingWidthClass,
    pub runs: usize,
    pub length_m: f64,
    pub posts: usize,
}

/// The take-off of every barrier run in a list of elements.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct RestraintQuantities {
    pub runs: Vec<RunQuantities>,
    /// Runs whose stored data is unreadable or invalid. They are in no total; a schedule with
    /// anything here is incomplete.
    pub skipped: Vec<SkippedRun>,
}

impl RestraintQuantities {
    /// Measures the barrier runs among `elements`, in order, from the geometry stored with
    /// each. Elements of other categories are ignored. Call [`crate::sync_barrier_runs`] first
    /// if an alignment may have changed.
    pub fn measure<'a>(elements: impl IntoIterator<Item = &'a Element>) -> RestraintQuantities {
        let mut out = RestraintQuantities::default();
        for e in elements.into_iter().filter(|e| e.category == CATEGORY) {
            match BarrierElement::from_value(&e.data).and_then(|b| b.run.check().map(|_| b)) {
                Ok(b) => {
                    let mut q = RunQuantities::measure(&b.run, &b.resolved);
                    q.mark = e.name.clone();
                    out.runs.push(q);
                }
                Err(err) => out.skipped.push(SkippedRun { mark: e.name.clone(), reason: err.to_string() }),
            }
        }
        out
    }

    pub fn length_m(&self) -> f64 {
        self.runs.iter().map(|r| r.length_m).sum()
    }

    pub fn posts(&self) -> usize {
        self.runs.iter().map(|r| r.posts).sum()
    }

    /// Length, posts and number of runs by containment level and working width class, in the
    /// order of the classes.
    pub fn by_class(&self) -> Vec<ClassTotal> {
        let mut map: BTreeMap<(ContainmentLevel, WorkingWidthClass), ClassTotal> = BTreeMap::new();
        for r in &self.runs {
            let t = map.entry((r.containment, r.working_width)).or_insert(ClassTotal {
                containment: r.containment,
                working_width: r.working_width,
                runs: 0,
                length_m: 0.0,
                posts: 0,
            });
            t.runs += 1;
            t.length_m += r.length_m;
            t.posts += r.posts;
        }
        map.into_values().collect()
    }

    /// Number of terminals by performance class label (`T110`, `P4`, ...), sorted by label.
    pub fn terminals(&self) -> BTreeMap<String, usize> {
        let mut map = BTreeMap::new();
        for end in self.runs.iter().flat_map(|r| r.ends()) {
            if let EndTreatment::Terminal { performance_class, .. } = end {
                *map.entry(performance_class.label()).or_insert(0) += 1;
            }
        }
        map
    }

    /// Number of transitions by type, sorted: the key is what the transition leads to and its
    /// declared containment level, e.g. `to parapet, N2` or `to safety barrier, containment not
    /// stated`. A transition is counted where it is entered, so one between two runs must be
    /// entered on one of them only.
    pub fn transitions(&self) -> BTreeMap<String, usize> {
        let mut map = BTreeMap::new();
        for end in self.runs.iter().flat_map(|r| r.ends()) {
            if let EndTreatment::ConnectsTo { system, transition: Some(t), .. } = end {
                let level = t.containment.map(|c| c.label().to_string()).unwrap_or_else(|| "containment not stated".into());
                *map.entry(format!("to {}, {level}", system.label())).or_insert(0) += 1;
            }
        }
        map
    }

    /// Number of run ends with neither a terminal nor a connection entered.
    pub fn unspecified_ends(&self) -> usize {
        self.runs.iter().flat_map(|r| r.ends()).filter(|e| matches!(e, EndTreatment::Unspecified)).count()
    }
}

pub const RUNS_CSV_HEADER: &str = "mark,alignment,start_station,end_station,offset_m,traffic_face,containment,working_width_class,impact_severity,system,length_m,centreline_length_m,post_spacing_m,posts,start_end,end_end";

/// Barrier schedule, one row per run, then a `TOTAL` row carrying the number of runs in the
/// second column and the summed length and posts. Lines end with `\n`.
///
/// Rounding: lengths, offsets and spacings to 3 decimals (millimetres). Totals are sums of the
/// unrounded values, rounded once. A run without posts has an empty spacing and 0 posts.
pub fn runs_to_csv(rows: &[RunQuantities]) -> String {
    let mut out = String::from(RUNS_CSV_HEADER);
    out.push('\n');
    for r in rows {
        let fields = [
            csv(&r.mark),
            csv(&r.alignment),
            csv(&r.start_station),
            csv(&r.end_station),
            fixed(r.offset_m, 0, 3),
            r.traffic_face.clone(),
            r.containment.label().to_string(),
            r.working_width.label().to_string(),
            r.impact_severity.clone(),
            csv(&r.system),
            fixed(r.length_m, 3, 3),
            fixed(r.centreline_length_m, 3, 3),
            r.post_spacing_m.map(|s| fixed(s, 0, 3)).unwrap_or_default(),
            r.posts.to_string(),
            csv(&r.start_end.label()),
            csv(&r.end_end.label()),
        ];
        out.push_str(&fields.join(","));
        out.push('\n');
    }
    out.push_str(&format!(
        "TOTAL,{},,,,,,,,,{},{},,{},,\n",
        rows.len(),
        fixed(rows.iter().map(|r| r.length_m).sum::<f64>(), 3, 3),
        fixed(rows.iter().map(|r| r.centreline_length_m).sum::<f64>(), 3, 3),
        rows.iter().map(|r| r.posts).sum::<usize>(),
    ));
    out
}

pub const SUMMARY_CSV_HEADER: &str = "item,unit,quantity";

/// Summary: totals, then length and posts by containment level and working width class, then
/// terminals by performance class and transitions by type. No rates.
pub fn summary_to_csv(q: &RestraintQuantities) -> String {
    let mut out = String::from(SUMMARY_CSV_HEADER);
    out.push('\n');
    let mut row = |item: &str, unit: &str, quantity: String| out.push_str(&format!("{},{unit},{quantity}\n", csv(item)));
    row("barrier runs", "nr", q.runs.len().to_string());
    row("safety barrier (length along the barrier)", "m", fixed(q.length_m(), 3, 3));
    row("posts", "nr", q.posts().to_string());
    for t in q.by_class() {
        row(&format!("safety barrier {} {}", t.containment, t.working_width), "m", fixed(t.length_m, 3, 3));
        row(&format!("posts in {} {}", t.containment, t.working_width), "nr", t.posts.to_string());
    }
    for (class, n) in q.terminals() {
        row(&format!("terminal {class}"), "nr", n.to_string());
    }
    for (kind, n) in q.transitions() {
        row(&format!("transition {kind}"), "nr", n.to_string());
    }
    if q.unspecified_ends() > 0 {
        row("run ends with no terminal or connection entered", "nr", q.unspecified_ends().to_string());
    }
    if !q.skipped.is_empty() {
        row("not measured (invalid run)", "nr", q.skipped.len().to_string());
    }
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
    use crate::run::sample_run;
    use crate::{add_barrier_run, resolve, ConnectedSystem, TerminalClass, Transition};
    use afrobim_alignment::{AlignmentBuilder, Turn};
    use afrobim_kernel::Document;
    use std::f64::consts::PI;

    fn alignment() -> afrobim_alignment::Alignment {
        // 100 m straight, then a quarter circle of 200 m radius to the right.
        AlignmentBuilder::new("A1", 0.0, 0.0, 0.0, 0.0).line(100.0).arc(200.0, 100.0 * PI, Turn::Right).build().unwrap()
    }

    fn document() -> Document {
        let a = alignment();
        let mut doc = Document::default();
        // VRS-001: stations 0 to 100 on the straight, 5 m right, posts at 2 m: 100 m, 51 posts.
        let mut r1 = sample_run();
        r1.start_station_m = 0.0;
        r1.end_station_m = 100.0;
        r1.end_end = EndTreatment::ConnectsTo { system: ConnectedSystem::SafetyBarrier, name: "VRS-002".into(), transition: None };
        add_barrier_run(&mut doc, &a, r1).unwrap();
        // VRS-002: the whole curve on the inside: 97.5π = 306.305 m; posts at 4 m: 77 bays, 78.
        let mut r2 = sample_run();
        r2.name = "VRS-002".into();
        r2.start_station_m = 100.0;
        r2.end_station_m = 100.0 + 100.0 * PI;
        r2.post_spacing_m = Some(4.0);
        r2.start_end = EndTreatment::ConnectsTo { system: ConnectedSystem::SafetyBarrier, name: "VRS-001".into(), transition: None };
        r2.end_end = EndTreatment::ConnectsTo {
            system: ConnectedSystem::Parapet,
            name: "BR-1".into(),
            transition: Some(Transition { containment: Some(ContainmentLevel::N2), ..Default::default() }),
        };
        add_barrier_run(&mut doc, &a, r2).unwrap();
        // VRS-003: H2 W2 concrete, no posts, on the left of the straight, 40 m, two P4 terminals.
        let mut r3 = sample_run();
        r3.name = "VRS-003".into();
        r3.start_station_m = 20.0;
        r3.end_station_m = 60.0;
        r3.offset_m = -6.5;
        r3.containment = ContainmentLevel::H2;
        r3.working_width = WorkingWidthClass::W2;
        r3.system = "concrete, \"step\" profile".into();
        r3.post_spacing_m = None;
        r3.start_end = EndTreatment::terminal(TerminalClass::P4);
        r3.end_end = EndTreatment::terminal(TerminalClass::P4);
        add_barrier_run(&mut doc, &a, r3).unwrap();
        doc
    }

    #[test]
    fn take_off_over_a_document() {
        let doc = document();
        let q = RestraintQuantities::measure(doc.elements());
        assert!(q.skipped.is_empty());
        assert_eq!(q.runs.len(), 3);
        assert_eq!(q.runs[0].length_m, 100.0);
        assert_eq!(q.runs[0].posts, 51);
        assert!((q.runs[1].length_m - 97.5 * PI).abs() < 1e-9);
        assert!((q.runs[1].centreline_length_m - 100.0 * PI).abs() < 1e-9);
        assert_eq!(q.runs[1].posts, 78);
        assert_eq!((q.runs[2].length_m, q.runs[2].posts), (40.0, 0));
        assert!((q.length_m() - (140.0 + 97.5 * PI)).abs() < 1e-9);
        assert_eq!(q.posts(), 129);

        let classes = q.by_class();
        assert_eq!(classes.len(), 2);
        assert_eq!((classes[0].containment, classes[0].working_width, classes[0].runs, classes[0].posts), (ContainmentLevel::N2, WorkingWidthClass::W4, 2, 129));
        assert!((classes[0].length_m - (100.0 + 97.5 * PI)).abs() < 1e-9);
        assert_eq!((classes[1].containment, classes[1].working_width, classes[1].length_m), (ContainmentLevel::H2, WorkingWidthClass::W2, 40.0));

        assert_eq!(q.terminals().into_iter().collect::<Vec<_>>(), [("P4".to_string(), 2), ("T110".to_string(), 1)]);
        assert_eq!(q.transitions().into_iter().collect::<Vec<_>>(), [("to parapet, N2".to_string(), 1)]);
        assert_eq!(q.unspecified_ends(), 0);
    }

    #[test]
    fn csv_content() {
        let q = RestraintQuantities::measure(document().elements());
        let runs = runs_to_csv(&q.runs);
        let lines: Vec<&str> = runs.lines().collect();
        assert_eq!(lines[0], RUNS_CSV_HEADER);
        assert_eq!(lines[1], "VRS-001,A1,0+000.000,0+100.000,5,left,N2,W4,A,as specified,100.000,100.000,2,51,terminal T110,connects to safety barrier VRS-002");
        assert_eq!(
            lines[2],
            "VRS-002,A1,0+100.000,0+414.159,5,left,N2,W4,A,as specified,306.305,314.159,4,78,connects to safety barrier VRS-001,transition to parapet BR-1 (N2)"
        );
        assert_eq!(lines[3], "VRS-003,A1,0+020.000,0+060.000,-6.5,left,H2,W2,A,\"concrete, \"\"step\"\" profile\",40.000,40.000,,0,terminal P4,terminal P4");
        assert_eq!(lines[4], "TOTAL,3,,,,,,,,,446.305,454.159,,129,,");
        assert!(runs.ends_with('\n'));
        // Every row has the header's number of fields (quoted commas aside).
        assert_eq!(lines[4].matches(',').count(), RUNS_CSV_HEADER.matches(',').count());

        assert_eq!(
            summary_to_csv(&q),
            "item,unit,quantity\n\
             barrier runs,nr,3\n\
             safety barrier (length along the barrier),m,446.305\n\
             posts,nr,129\n\
             safety barrier N2 W4,m,406.305\n\
             posts in N2 W4,nr,129\n\
             safety barrier H2 W2,m,40.000\n\
             posts in H2 W2,nr,0\n\
             terminal P4,nr,2\n\
             terminal T110,nr,1\n\
             \"transition to parapet, N2\",nr,1\n"
        );
        assert_eq!(runs_to_csv(&[]), format!("{RUNS_CSV_HEADER}\nTOTAL,0,,,,,,,,,0.000,0.000,,0,,\n"));
    }

    #[test]
    fn unreadable_and_invalid_runs_are_skipped_and_reported() {
        let mut doc = document();
        let id = doc.elements()[0].id;
        doc.find_mut(id).unwrap().data = serde_json::json!({"nonsense": true});
        let id = doc.elements()[2].id;
        let mut b = BarrierElement::from_value(&doc.find(id).unwrap().data).unwrap();
        b.run.beam_height_m = 0.0;
        doc.find_mut(id).unwrap().data = b.to_value();
        let mut r = sample_run();
        r.name = "VRS-009".into();
        r.start_end = EndTreatment::Unspecified;
        add_barrier_run(&mut doc, &alignment(), r).unwrap();

        let q = RestraintQuantities::measure(doc.elements());
        assert_eq!(q.runs.len(), 2);
        assert_eq!(q.skipped.iter().map(|s| s.mark.as_str()).collect::<Vec<_>>(), ["VRS-001", "VRS-003"]);
        let summary = summary_to_csv(&q);
        assert!(summary.contains("run ends with no terminal or connection entered,nr,1\n"));
        assert!(summary.ends_with("not measured (invalid run),nr,2\n"));
    }

    #[test]
    fn measuring_one_run_directly() {
        let run = sample_run();
        let a = AlignmentBuilder::new("A1", 0.0, 0.0, 0.0, 0.0).line(500.0).build().unwrap();
        let q = RunQuantities::measure(&run, &resolve(&run, &a).unwrap());
        assert_eq!((q.length_m, q.posts, q.mark.as_str()), (100.0, 51, "VRS-001"));
    }
}
