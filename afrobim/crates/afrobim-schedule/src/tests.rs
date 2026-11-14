use afrobim_draw::Primitive;
use afrobim_kernel::{Document, Module, Pt, Severity};

use crate::*;

fn d(m: u32, day: u32) -> Date {
    Date::from_ymd(2026, m, day).unwrap()
}

/// Test data only: Monday to Friday, with one entered holiday on Monday 16 March 2026.
fn five_day() -> Calendar {
    Calendar::new("Five-day", &[Weekday::Mon, Weekday::Tue, Weekday::Wed, Weekday::Thu, Weekday::Fri]).with_holiday(d(3, 16))
}

/// The hand-worked network. Programme start Monday 2 March 2026.
///
/// Working days, numbered from the start (the holiday on Monday 16 March is skipped):
///
/// ```text
///  0 Mar 2   1 Mar 3   2 Mar 4   3 Mar 5   4 Mar 6
///  5 Mar 9   6 Mar 10  7 Mar 11  8 Mar 12  9 Mar 13
/// 10 Mar 17 11 Mar 18 12 Mar 19 13 Mar 20
/// 14 Mar 23 15 Mar 24 16 Mar 25 17 Mar 26 18 Mar 27
/// ```
///
/// Activities (durations in working days): A 3, B 5, C 4, D 3, E 4, F 3, G 2, H 1, T 2 and the
/// milestone M (finish milestone, 0).
/// Logic: A -FS-> B; B -SS+2-> C; C -FS-1-> D; B -FF+0-> D; D -FS-> E; C -FS-> F;
/// E -FS-> G; F -FS-> G; G -FS-> H; H -FS-> M; E -SF+0-> T; T -FS-> M.
///
/// Forward pass (day numbers):
///   A  ES 0 EF 2.
///   B  FS from A: first day after 2 = 3.                ES 3 EF 7 (5 days: 3,4,5,6,7).
///   C  SS+2 from B: start of B 3, +2 working days = 5.  ES 5 EF 8.
///   D  FS-1 from C: first day after 8 is 9, minus 1 = 8; FF from B: finish >= 7, so start >= 5;
///      the larger is 8.                                 ES 8 EF 10 (8, 9, then 10 = Mar 17).
///   E  FS from D: 11.                                   ES 11 EF 14.
///   F  FS from C: 9.                                    ES 9 EF 11.
///   G  FS from E: 15, from F: 12; larger 15.            ES 15 EF 16.
///   H  FS from G: 17.                                   ES 17 EF 17.
///   T  SF+0 from E: finish >= the working day before E's start (11) = 10, so start = 9.
///                                                       ES 9 EF 10.
///   M  FS from H: next day is 18, a finish milestone is the end of the day before = 17;
///      FS from T gives 10. Larger: 17.                  date 17 (Mar 26).
/// Project finish: day 17 = Thursday 26 March 2026.
///
/// Backward pass (finish limit = project finish, 17):
///   M  LF 17.   H  LS 17 LF 17.   G  LF 16 LS 15.   F  LF 14 LS 12.   E  LF 14 LS 11.
///   T  from M: LF 17, LS 16.      (E from T by SF: S-bound 18, no constraint.)
///   D  LF 10 LS 8.   C  from D (FS-1): F-bound 8; from F: 11; LF 8 LS 5.
///   B  from C (SS+2): S-bound 5-2 = 3; from D (FF): LF 10 gives LS 6; LS 3, LF 7.
///   A  LF 2 LS 0.
/// Total float: A B C D E G H M = 0 (critical); F: LF 14 vs EF 11 = days 11,12,13 = 3;
/// T: LF 17 vs EF 10 = days 10..16 = 7.
/// Free float: F 3 (G starts at 15, needs 12); T 7; all the others 0.
fn network() -> Programme {
    let mut p = Programme::new("Test network", d(3, 2), five_day());
    for (id, name, dur) in [("A", "Site clearance", 3), ("B", "Earthworks", 5), ("C", "Drainage", 4), ("D", "Sub-base", 3), ("E", "Base", 4), ("F", "Kerbs", 3), ("G", "Surfacing", 2), ("H", "Markings", 1), ("T", "Traffic diversion", 2)] {
        p.add_activity(Activity::new(id, &format!("1.{id}"), name, dur));
    }
    p.add_activity(Activity::new("M", "1.M", "Completion", 0));
    p.link("A", "B", LinkType::FS, 0);
    p.link("B", "C", LinkType::SS, 2);
    p.link("C", "D", LinkType::FS, -1);
    p.link("B", "D", LinkType::FF, 0);
    p.link("D", "E", LinkType::FS, 0);
    p.link("C", "F", LinkType::FS, 0);
    p.link("E", "G", LinkType::FS, 0);
    p.link("F", "G", LinkType::FS, 0);
    p.link("G", "H", LinkType::FS, 0);
    p.link("H", "M", LinkType::FS, 0);
    p.link("E", "T", LinkType::SF, 0);
    p.link("T", "M", LinkType::FS, 0);
    p
}

#[allow(clippy::too_many_arguments)]
fn check(s: &Schedule, id: &str, es: Date, ef: Date, ls: Date, lf: Date, tf: i64, ff: i64) {
    let r = s.row(id).unwrap();
    assert_eq!((r.early_start, r.early_finish, r.late_start, r.late_finish, r.total_float, r.free_float), (es, ef, ls, lf, tf, ff), "activity {id}");
    assert_eq!(r.critical, tf <= 0, "critical flag of {id}");
}

#[test]
fn hand_worked_network_with_all_four_relationship_types() {
    let s = schedule(&network()).unwrap();
    check(&s, "A", d(3, 2), d(3, 4), d(3, 2), d(3, 4), 0, 0);
    check(&s, "B", d(3, 5), d(3, 11), d(3, 5), d(3, 11), 0, 0);
    check(&s, "C", d(3, 9), d(3, 12), d(3, 9), d(3, 12), 0, 0);
    check(&s, "D", d(3, 12), d(3, 17), d(3, 12), d(3, 17), 0, 0);
    check(&s, "E", d(3, 18), d(3, 23), d(3, 18), d(3, 23), 0, 0);
    check(&s, "F", d(3, 13), d(3, 18), d(3, 19), d(3, 23), 3, 3);
    check(&s, "G", d(3, 24), d(3, 25), d(3, 24), d(3, 25), 0, 0);
    check(&s, "H", d(3, 26), d(3, 26), d(3, 26), d(3, 26), 0, 0);
    check(&s, "T", d(3, 13), d(3, 17), d(3, 25), d(3, 26), 7, 7);
    check(&s, "M", d(3, 26), d(3, 26), d(3, 26), d(3, 26), 0, 0);
    assert_eq!(s.project_finish, d(3, 26));
    assert_eq!(s.critical().map(|r| r.id.as_str()).collect::<Vec<_>>(), ["A", "B", "C", "D", "E", "G", "H", "M"]);
    // D spans a weekend and the entered holiday: 12, 13, then 17 March.
    assert_eq!(s.row("D").unwrap().duration, 3);
}

#[test]
fn negative_float_from_a_finish_constraint_and_required_finish() {
    // Finish no later than Wednesday 25 March (day 16) on H, which the logic finishes on day 17.
    let mut p = network();
    p.activity_mut("H").unwrap().constraint = Some(Constraint::FinishNoLaterThan(d(3, 25)));
    let s = schedule(&p).unwrap();
    assert_eq!(s.project_finish, d(3, 26));
    assert_eq!(s.row("H").unwrap().total_float, -1); // LF 16 against EF 17
    assert_eq!(s.row("G").unwrap().total_float, -1); // the chain back to the start carries it
    assert_eq!(s.row("A").unwrap().total_float, -1);
    assert_eq!(s.row("F").unwrap().total_float, 2); // 3 - 1
    assert_eq!(s.row("M").unwrap().total_float, 0);
    assert!(s.row("H").unwrap().critical);
    assert!(s.issues.iter().any(|i| i.code == "negative_float"));

    // A contract completion one working day after the computed finish gives one day of float...
    let mut p = network();
    p.required_finish = Some(d(3, 27));
    let s = schedule(&p).unwrap();
    assert_eq!(s.row("A").unwrap().total_float, 1);
    assert!(s.row("M").unwrap().total_float == 1 && s.critical().next().is_none());
    // ...and one working day before it, one day short.
    p.required_finish = Some(d(3, 25));
    assert_eq!(schedule(&p).unwrap().row("A").unwrap().total_float, -1);
}

#[test]
fn must_start_on_and_start_no_earlier_than() {
    let mut p = network();
    p.activity_mut("A").unwrap().constraint = Some(Constraint::StartNoEarlierThan(d(3, 4)));
    let s = schedule(&p).unwrap();
    assert_eq!(s.row("A").unwrap().early_start, d(3, 4));
    assert_eq!(s.project_finish, d(3, 30)); // two days later
    let mut p = network();
    p.activity_mut("B").unwrap().constraint = Some(Constraint::MustStartOn(d(3, 4)));
    let s = schedule(&p).unwrap();
    assert_eq!(s.row("B").unwrap().early_start, d(3, 4));
    // B is pulled before A finishes: the overridden logic is reported.
    assert!(s.issues.iter().any(|i| i.code == "mso_overrides_logic"));
}

#[test]
fn cycles_and_dangling_logic_are_errors_with_clear_messages() {
    let mut p = network();
    p.link("G", "B", LinkType::FS, 0);
    let err = schedule(&p).unwrap_err().to_string();
    assert!(err.contains("logic loop: ") && err.contains("B -> ") && err.contains(" -> B"), "{err}");

    let mut p = network();
    p.link("A", "ZZ", LinkType::FS, 0);
    p.link("A", "A", LinkType::SS, 0);
    let err = schedule(&p).unwrap_err().to_string();
    assert!(err.contains("'ZZ'") && err.contains("dangling logic") && err.contains("linked to itself"), "{err}");

    let mut p = Programme::new("x", d(3, 2), five_day());
    p.add_activity(Activity::new("1", "", "one", 1));
    p.add_activity(Activity::new("2", "", "two", 1));
    p.add_activity(Activity::new("3", "", "three", 1));
    p.link("1", "2", LinkType::FS, 0);
    let issues = validate(&p);
    assert!(issues.iter().any(|i| i.code == "open_end" && i.activity.as_deref() == Some("2")));
    assert!(issues.iter().any(|i| i.code == "open_start" && i.activity.as_deref() == Some("3")));
    assert!(issues.iter().all(|i| i.severity == Severity::Warning));
}

/// Progress as of the end of Tuesday 10 March (the data date), so remaining work cannot start
/// before Wednesday 11 March, day 7. A is complete: started 2 March, finished Friday 6 March
/// (5 days worked, 3 planned). B started Monday 9 March (day 5), is 20 percent done, 4 days left.
///
///   B   FS from A: day 5; floor day 7. Remaining start 7, 4 days: EF day 10 (Tue 17 Mar).
///       Reported start is the actual one, 9 March.
///   C   SS+2 from B's actual start (day 5) = 7; floor 7. ES 7 (Mar 11), EF 10 (Mar 17).
///   D   FS-1 from C: first day after 10 is 11, minus 1 = 10; FF from B: finish >= 10, start >= 8;
///       ES 10 (Mar 17), EF 12 (Mar 19).
///   E   ES 13 (Fri Mar 20), EF 16 (Wed Mar 25).      F   ES 11 (Mar 18), EF 13 (Mar 20).
///   G   after E: 17, after F: 14; ES 17 (Thu Mar 26), EF 18 (Fri Mar 27).
///   H   ES 19 (Mon Mar 30), EF 19.   T   SF from E: finish >= 12, ES 11, EF 12.   M   date 19.
fn progressed() -> Programme {
    let mut p = network();
    p.data_date = Some(d(3, 10));
    let a = p.activity_mut("A").unwrap();
    a.progress = Progress { percent_complete: 100.0, actual_start: Some(d(3, 2)), actual_finish: Some(d(3, 6)), remaining_duration: None };
    let b = p.activity_mut("B").unwrap();
    b.progress = Progress { percent_complete: 20.0, actual_start: Some(d(3, 9)), actual_finish: None, remaining_duration: None };
    p
}

#[test]
fn progress_reschedules_remaining_work_from_the_data_date() {
    let p = progressed();
    assert_eq!(p.activity("B").unwrap().remaining(), 4); // ceil(5 * 0.8)
    let s = schedule(&p).unwrap();
    let a = s.row("A").unwrap();
    assert_eq!((a.status, a.early_start, a.early_finish, a.total_float, a.critical), (Status::Complete, d(3, 2), d(3, 6), 0, false));
    let b = s.row("B").unwrap();
    assert_eq!((b.status, b.early_start, b.early_finish, b.remaining_duration), (Status::InProgress, d(3, 9), d(3, 17), 4));
    assert_eq!((s.row("C").unwrap().early_start, s.row("C").unwrap().early_finish), (d(3, 11), d(3, 17)));
    assert_eq!((s.row("D").unwrap().early_start, s.row("D").unwrap().early_finish), (d(3, 17), d(3, 19)));
    assert_eq!((s.row("E").unwrap().early_start, s.row("E").unwrap().early_finish), (d(3, 20), d(3, 25)));
    assert_eq!((s.row("F").unwrap().early_start, s.row("F").unwrap().early_finish), (d(3, 18), d(3, 20)));
    assert_eq!((s.row("G").unwrap().early_start, s.row("G").unwrap().early_finish), (d(3, 26), d(3, 27)));
    assert_eq!(s.row("H").unwrap().early_start, d(3, 30));
    assert_eq!((s.row("T").unwrap().early_start, s.row("T").unwrap().early_finish), (d(3, 18), d(3, 19)));
    assert_eq!(s.project_finish, d(3, 30));
    // Nothing remaining is scheduled on or before the data date.
    assert!(s.rows.iter().filter(|r| r.status == Status::NotStarted).all(|r| r.early_start > d(3, 10)));

    // Progress without a data date, or after it, is refused.
    let mut bad = progressed();
    bad.data_date = None;
    assert!(schedule(&bad).unwrap_err().to_string().contains("no data date"));
    bad.data_date = Some(d(3, 5));
    assert!(schedule(&bad).unwrap_err().to_string().contains("after the data date"));
}

#[test]
fn baseline_variance_per_activity() {
    let mut p = network();
    let base = schedule(&p).unwrap();
    p.set_baseline("Contract programme", &base);
    assert!(p.variance(&base).unwrap().rows.iter().all(|v| v.status == VarianceStatus::Unchanged));
    let cur = { p.data_date = progressed().data_date; p.activities = progressed().activities; schedule(&p).unwrap() };
    let v = p.variance(&cur).unwrap();
    assert_eq!(v.baseline, "Contract programme");
    assert_eq!(v.project_finish_variance, 2); // 26 -> 30 March
    let row = |id: &str| v.rows.iter().find(|r| r.id == id).unwrap();
    // A finished on 6 March (baseline 4 March) after working 5 days instead of 3.
    assert_eq!((row("A").start_variance, row("A").finish_variance, row("A").duration_variance), (Some(0), Some(2), Some(2)));
    // H moved from 26 to 30 March: 2 working days.
    assert_eq!((row("H").start_variance, row("H").finish_variance), (Some(2), Some(2)));
    assert_eq!(row("M").finish_variance, Some(2));
    // F: start 13 -> 18 March (day 9 -> day 11: 2 working days, over the holiday); float is still 3.
    assert_eq!((row("F").start_variance, row("F").float_variance), (Some(2), Some(0)));
    assert_eq!(row("F").status, VarianceStatus::Changed);
    // Added and removed activities.
    p.activities.retain(|a| a.id != "T");
    p.dependencies.retain(|d| d.predecessor != "T" && d.successor != "T");
    p.add_activity(Activity::new("N", "", "New", 1));
    p.link("H", "N", LinkType::FS, 0);
    let v = p.variance(&schedule(&p).unwrap()).unwrap();
    assert!(v.rows.iter().any(|r| r.id == "N" && r.status == VarianceStatus::Added));
    assert!(v.rows.iter().any(|r| r.id == "T" && r.status == VarianceStatus::Removed));
    assert!(variance_csv(&v).starts_with(VARIANCE_CSV_HEADER));
}

fn model_with_links() -> (Programme, Document) {
    let mut doc = Document::default();
    let add = |doc: &mut Document, cat: &str, name: &str, layer: &str| doc.add(cat, name, Pt::new(0.0, 0.0), 1.0, 1.0, layer, serde_json::Value::Null);
    let old = add(&mut doc, "existing", "OLD-1", "0");
    let sub = add(&mut doc, "pavement", "SB-1", "0");
    let tm = add(&mut doc, "temporary", "TM-1", "0");
    let loose = add(&mut doc, "pavement", "LOOSE", "0");
    let dr1 = add(&mut doc, "drainage", "DR-01", "0");
    let dr2 = add(&mut doc, "drainage", "DR-02", "0");
    let xx = add(&mut doc, "drainage", "XX-1", "0");
    let mut p = network();
    p.link_elements("A", ElementSelector::Id(old), LinkAction::Demolish);
    p.link_elements("D", ElementSelector::Id(sub), LinkAction::Construct);
    p.link_elements("T", ElementSelector::Id(tm), LinkAction::Temporary);
    p.link_elements("C", ElementSelector::Rule(ElementRule { category: Some("drainage".into()), mark_pattern: Some("DR-*".into()), layer: None }), LinkAction::Construct);
    let _ = (loose, dr1, dr2, xx);
    (p, doc)
}

#[test]
fn four_d_state_at_three_dates() {
    let (p, doc) = model_with_links();
    let s = schedule(&p).unwrap();
    let id = |n: &str| doc.elements().iter().find(|e| e.name == n).unwrap().id;
    let near = |a: f64, b: f64| (a - b).abs() < 1e-9;

    // Tuesday 3 March: A (2..4 March, 3 days) has run 2 of 3 days.
    let st = element_states(&p, &s, &doc, d(3, 3));
    let old = st.get(id("OLD-1")).unwrap();
    assert!(matches!(old.phase, Phase::InProgress { fraction } if near(fraction, 2.0 / 3.0)) && old.present);
    assert_eq!(st.get(id("SB-1")).unwrap().phase, Phase::NotStarted);
    assert!(!st.get(id("SB-1")).unwrap().present);
    assert_eq!(st.get(id("DR-01")).unwrap().phase, Phase::NotStarted);

    // Thursday 12 March: demolition done; drainage (9..12 March) done; sub-base (12..17) one day of 3.
    let st = element_states(&p, &s, &doc, d(3, 12));
    assert_eq!(st.get(id("OLD-1")).unwrap().phase, Phase::Removed);
    assert!(!st.get(id("OLD-1")).unwrap().present);
    assert_eq!(st.get(id("DR-01")).unwrap().phase, Phase::Complete);
    assert_eq!(st.get(id("DR-02")).unwrap().phase, Phase::Complete);
    assert!(matches!(st.get(id("SB-1")).unwrap().phase, Phase::InProgress { fraction } if near(fraction, 1.0 / 3.0)));
    assert_eq!(st.get(id("TM-1")).unwrap().phase, Phase::NotStarted); // diversion runs 13..17 March
    assert!(!st.get(id("TM-1")).unwrap().present);

    // Tuesday 17 March: sub-base complete; the temporary diversion is gone again.
    let st = element_states(&p, &s, &doc, d(3, 17));
    assert_eq!(st.get(id("SB-1")).unwrap().phase, Phase::Complete);
    assert_eq!(st.get(id("TM-1")).unwrap().phase, Phase::Removed);
    // Wednesday 18 March is not yet the end of the diversion's life in between: check it is
    // present on the 13th (first day of 2).
    let st = element_states(&p, &s, &doc, d(3, 13));
    assert!(matches!(st.get(id("TM-1")).unwrap().phase, Phase::InProgress { fraction } if near(fraction, 0.5)));
    assert!(st.get(id("TM-1")).unwrap().present);

    // Elements no link reaches are reported, in the states and in the link report.
    assert_eq!(st.unlinked, vec![id("LOOSE"), id("XX-1")]);
    let rep = link_report(&p, &doc);
    assert_eq!(rep.unlinked_elements, vec![id("LOOSE"), id("XX-1")]);
    assert!(rep.activities_without_links.contains(&"B".to_string()));
    assert!(rep.empty_rules.is_empty() && rep.missing_elements.is_empty());
}

#[test]
fn calendar_skipping_weekend_and_holiday_in_a_schedule() {
    let mut p = Programme::new("cal", d(3, 12), five_day()); // Thursday
    p.add_activity(Activity::new("X", "", "x", 4));
    let s = schedule(&p).unwrap();
    // Thu 12, Fri 13, (weekend), (Mon 16 holiday), Tue 17, Wed 18.
    assert_eq!(s.row("X").unwrap().early_finish, d(3, 18));
    // A six-day working week with the same holiday.
    let mut p = Programme::new("cal6", d(3, 12), Calendar::new("Six-day", &[Weekday::Mon, Weekday::Tue, Weekday::Wed, Weekday::Thu, Weekday::Fri, Weekday::Sat]).with_holiday(d(3, 16)));
    p.add_activity(Activity::new("X", "", "x", 4));
    assert_eq!(schedule(&p).unwrap().row("X").unwrap().early_finish, d(3, 17)); // Thu, Fri, Sat, then Tue (Monday is the holiday)
}

#[test]
fn outputs_gantt_csv_and_s_curve() {
    let p = network();
    let s = schedule(&p).unwrap();
    let g = gantt_drawing(&p, &s, &GanttOptions::default());
    assert_eq!(g.rows.len(), 10);
    assert!(g.labels.iter().any(|l| l.kind == GanttLabelKind::Name && l.text == "Drainage"));
    assert!(g.labels.iter().any(|l| l.kind == GanttLabelKind::Month && l.text == "Mar 2026"));
    let reds = g.drawing.primitives.iter().filter(|p| matches!(p, Primitive::Fill { colour: afrobim_draw::Colour::Red, .. })).count();
    assert!(reds >= 8, "critical bars and milestone are red, got {reds}"); // 7 bars + diamond + arrowheads
    // The bar of A starts where its date says.
    let xa = g.x_of(d(3, 2));
    assert!(g.drawing.primitives.iter().any(|p| match p {
        Primitive::Fill { outline, colour: afrobim_draw::Colour::Red } => (outline[0].x - xa).abs() < 1e-9,
        _ => false,
    }));
    assert!(g.drawing.extents.width() > 0.0 && g.drawing.extents.height() > 0.0);

    let csv = activity_table_csv(&p, &s);
    assert!(csv.starts_with(ACTIVITY_CSV_HEADER));
    assert!(csv.contains("F,1.F,Kerbs,Five-day,3,3,0,,2026-03-13,2026-03-18,2026-03-19,2026-03-23,3,3,no,NotStarted\n"), "{csv}");
    assert_eq!(csv.lines().count(), 11);

    // S-curve by duration: total 27 working days; A alone has earned 3 by the end of 4 March.
    let c = s_curve(&p, &s, SCurveBasis::Duration).unwrap();
    assert_eq!(c.total, 27.0);
    let at = |date: Date| c.points.iter().find(|q| q.date == date).unwrap();
    assert!((at(d(3, 4)).fraction - 3.0 / 27.0).abs() < 1e-9);
    assert!((c.points.last().unwrap().fraction - 1.0).abs() < 1e-9);
    assert!(c.points.windows(2).all(|w| w[1].cumulative >= w[0].cumulative));
    assert!(s_curve_csv(&c).starts_with("date,cumulative,fraction\n"));
    let (dr, labels) = s_curve_drawing(&c, 200.0, 100.0);
    assert!(!dr.primitives.is_empty() && labels.iter().any(|l| l.text == "100%"));

    // By weight: every activity needs one.
    assert!(s_curve(&p, &s, SCurveBasis::Weight).unwrap_err().to_string().contains("missing on"));
    let mut pw = network();
    for a in &mut pw.activities {
        a.weight = Some(if a.id == "B" { 30.0 } else { 10.0 });
    }
    let sw = schedule(&pw).unwrap();
    let cw = s_curve(&pw, &sw, SCurveBasis::Weight).unwrap();
    assert_eq!(cw.total, 120.0); // eight activities of 10, B of 30, the milestone 10 (it earns on its date)
}

#[test]
fn mspdi_round_trip() {
    let mut p = progressed();
    p.activity_mut("F").unwrap().calendar = Some("Six-day".into());
    p.calendars.push(Calendar::new("Six-day", &[Weekday::Mon, Weekday::Tue, Weekday::Wed, Weekday::Thu, Weekday::Fri, Weekday::Sat]).with_holiday(d(3, 16)).with_holiday(d(4, 3)));
    p.activity_mut("T").unwrap().constraint = Some(Constraint::StartNoEarlierThan(d(3, 9)));
    p.activity_mut("H").unwrap().constraint = Some(Constraint::FinishNoLaterThan(d(4, 30)));
    p.activity_mut("G").unwrap().constraint = Some(Constraint::MustStartOn(d(3, 26)));
    p.data_date = Some(d(3, 10));
    let s = schedule(&p).unwrap();
    let xml = export_mspdi(&p, Some(&s));
    // Names written, as published by Microsoft.
    for tag in ["<PredecessorLink>", "<PredecessorUID>", "<LinkLag>", "<LagFormat>7</LagFormat>", "<PercentComplete>", "<WeekDay>", "<DayType>2</DayType>", "<Exception>", "<ConstraintType>4</ConstraintType>", "<MinutesPerDay>480</MinutesPerDay>"] {
        assert!(xml.contains(tag), "missing {tag}");
    }
    // SS link B -> C with lag +2 days: Type 3, lag 2 * 480 * 10 tenths of a minute.
    assert!(xml.contains("<Type>3</Type>\n        <CrossProject>0</CrossProject>\n        <LinkLag>9600</LinkLag>"));
    let (q, rep) = import_mspdi(&xml).unwrap();
    assert!(rep.warnings.is_empty(), "{:?}", rep.warnings);
    assert_eq!(q.name, p.name);
    assert_eq!(q.start, p.start);
    assert_eq!(q.data_date, p.data_date);
    assert_eq!(q.calendar, p.calendar);
    assert_eq!(q.calendars, p.calendars);
    assert_eq!(q.dependencies.len(), p.dependencies.len());
    for dep in &p.dependencies {
        assert!(q.dependencies.contains(dep), "lost {dep:?}");
    }
    for a in &p.activities {
        let b = q.activity(&a.id).unwrap_or_else(|| panic!("lost {}", a.id));
        assert_eq!((&b.wbs, &b.name, b.duration, &b.calendar, b.constraint), (&a.wbs, &a.name, a.duration, &a.calendar, a.constraint), "{}", a.id);
        assert_eq!(b.progress.actual_start, a.progress.actual_start);
        assert_eq!(b.progress.actual_finish, a.progress.actual_finish);
        assert_eq!(b.progress.percent_complete, a.progress.percent_complete);
    }
    // The imported programme schedules to the same dates.
    let s2 = schedule(&q).unwrap();
    for r in &s.rows {
        let r2 = s2.row(&r.id).unwrap();
        assert_eq!((r.early_start, r.early_finish, r.total_float), (r2.early_start, r2.early_finish, r2.total_float), "{}", r.id);
    }
    assert_eq!(s2.project_finish, s.project_finish);
}

#[test]
fn mspdi_import_of_a_foreign_file_reports_what_it_skips() {
    let xml = r#"<?xml version="1.0" encoding="UTF-8"?>
<Project xmlns="http://schemas.microsoft.com/project">
  <SaveVersion>14</SaveVersion><Name>Road.xml</Name><ScheduleFromStart>1</ScheduleFromStart>
  <StartDate>2026-03-02T08:00:00</StartDate><CalendarUID>1</CalendarUID><MinutesPerDay>480</MinutesPerDay>
  <Calendars><Calendar><UID>1</UID><Name>Standard</Name><IsBaseCalendar>1</IsBaseCalendar><WeekDays>
    <WeekDay><DayType>1</DayType><DayWorking>0</DayWorking></WeekDay>
    <WeekDay><DayType>2</DayType><DayWorking>1</DayWorking></WeekDay>
    <WeekDay><DayType>3</DayType><DayWorking>1</DayWorking></WeekDay>
    <WeekDay><DayType>7</DayType><DayWorking>0</DayWorking></WeekDay>
  </WeekDays></Calendar></Calendars>
  <Tasks>
    <Task><UID>0</UID><ID>0</ID><Name>Road</Name><Summary>1</Summary><Duration>PT24H0M0S</Duration></Task>
    <Task><UID>1</UID><ID>1</ID><Name>Clear</Name><WBS>1.1</WBS><Duration>PT8H0M0S</Duration><ConstraintType>1</ConstraintType></Task>
    <Task><UID>2</UID><ID>2</ID><Name>Dig</Name><WBS>1.2</WBS><Duration>PT16H0M0S</Duration>
      <PredecessorLink><PredecessorUID>1</PredecessorUID><Type>1</Type><LinkLag>4800</LinkLag><LagFormat>7</LagFormat></PredecessorLink>
      <PredecessorLink><PredecessorUID>0</PredecessorUID><Type>1</Type></PredecessorLink></Task>
  </Tasks>
</Project>"#;
    let (p, rep) = import_mspdi(xml).unwrap();
    assert_eq!(p.activities.len(), 2);
    assert_eq!(p.calendar_named("Standard").unwrap().working_days.len(), 2);
    assert_eq!(p.dependencies.len(), 1);
    assert_eq!((p.dependencies[0].kind, p.dependencies[0].lag), (LinkType::FS, 1)); // 4800 tenths of a minute = 480 min
    assert_eq!(p.activity("2").unwrap().duration, 2);
    assert!(rep.warnings.iter().any(|w| w.contains("constraint type 1")));
    assert!(rep.warnings.iter().any(|w| w.contains("link from UID 0")));
    assert!(matches!(import_mspdi("<Foo/>"), Err(MspdiError::NotMspdi(_))));
    assert!(matches!(import_mspdi("<Project><StartDate>2026-03-02</StartDate></Project>"), Err(MspdiError::Missing(_))));
}

#[test]
fn programme_lives_in_the_document_as_a_schedule_element() {
    let (p, mut doc) = model_with_links();
    let id = add_programme(&mut doc, "Programme", &p);
    // Survives a document save and load.
    let doc2 = Document::from_json(&doc.to_json()).unwrap();
    assert_eq!(load_programme(&doc2, id).unwrap(), p);
    let e = doc2.find(id).unwrap();
    assert_eq!(e.category, SCHEDULE_CATEGORY);
    let g = ScheduleModule::default().generate(e).unwrap();
    assert!(!g.drawing.primitives.is_empty());
    assert_eq!(g.derived["Finish"], "2026-03-26");
    assert_eq!(g.derived["Critical activities"], "8");
    // A broken programme yields error notes instead of a drawing.
    let mut bad = p.clone();
    bad.link("G", "A", LinkType::FS, 0);
    let mut doc3 = doc2.clone();
    store_programme(&mut doc3, id, &bad).unwrap();
    let g = ScheduleModule::default().generate(doc3.find(id).unwrap()).unwrap();
    assert!(g.notes.iter().any(|n| n.severity == Severity::Error && n.code == "cycle"));
    // The schedule element is not an unlinked model element.
    assert!(!link_report(&p, &doc2).unlinked_elements.contains(&id));
}
