//! Tests for the WBS roll-up, XER exchange, the time-location chart, 4D playback, several
//! constraints and the longest path. Every expected number is worked by hand in the comments.

use afrobim_draw::{Colour, Primitive, Pt2};
use afrobim_kernel::{Document, Pt, SitePlacement};

use crate::*;

fn d(m: u32, day: u32) -> Date {
    Date::from_ymd(2026, m, day).unwrap()
}

/// Test data only: Monday to Friday, no holidays.
fn five() -> Calendar {
    Calendar::new("Five", &[Weekday::Mon, Weekday::Tue, Weekday::Wed, Weekday::Thu, Weekday::Fri])
}

// ---------------------------------------------------------------- WBS roll-up

/// Start Monday 2 March 2026. A 3 days, B 5 days, C 2 days; A -FS-> B -FS-> C.
/// A: Mar 2, 3, 4. B: Mar 5, 6, 9, 10, 11. C: Mar 12, 13.
/// Summaries: "1" holds "1.1" (A, B) and "1.2" (C).
fn wbs_programme() -> Programme {
    let mut p = Programme::new("WBS test", d(3, 2), five());
    p.add_summary(SummaryTask::new("S1", "1", "Works", None));
    p.add_summary(SummaryTask::new("S11", "1.1", "Earthworks", Some("S1")));
    p.add_summary(SummaryTask::new("S12", "1.2", "Drainage", Some("S1")));
    for (id, name, dur, parent, w) in [("A", "Clear", 3, "S11", 10.0), ("B", "Cut", 5, "S11", 30.0), ("C", "Pipe", 2, "S12", 60.0)] {
        let a = p.add_activity(Activity::new(id, "", name, dur));
        a.parent = Some(parent.to_string());
        a.weight = Some(w);
    }
    p.link("A", "B", LinkType::FS, 0);
    p.link("B", "C", LinkType::FS, 0);
    p
}

#[test]
fn summary_dates_span_children_and_percent_rolls_up_by_duration() {
    let mut p = wbs_programme();
    // Data date Friday 6 March. A is complete (Mar 2 to Mar 4). B started Mar 5, 40 percent, 3 days
    // left: they are worked Mar 9, 10, 11, so B still finishes Mar 11. C follows on Mar 12, 13.
    p.data_date = Some(d(3, 6));
    {
        let a = p.activity_mut("A").unwrap();
        a.progress.percent_complete = 100.0;
        a.progress.actual_start = Some(d(3, 2));
        a.progress.actual_finish = Some(d(3, 4));
        let b = p.activity_mut("B").unwrap();
        b.progress.percent_complete = 40.0;
        b.progress.actual_start = Some(d(3, 5));
        b.progress.remaining_duration = Some(3);
    }
    let s = schedule(&p).unwrap();
    let r = wbs_rollup(&p, &s, RollupBasis::Duration).unwrap();
    assert_eq!(r.rows.iter().map(|x| x.id.as_str()).collect::<Vec<_>>(), ["S1", "S11", "S12"]);
    // 1.1: dates Mar 2 to Mar 11; percent (3 * 100 + 5 * 40) / 8 = 62.5; critical: B only (A is complete).
    let s11 = r.row("S11").unwrap();
    assert_eq!((s11.start, s11.finish), (Some(d(3, 2)), Some(d(3, 11))));
    assert!((s11.percent_complete - 62.5).abs() < 1e-9);
    assert_eq!((s11.activities, s11.critical, s11.level), (2, 1, 2));
    // 1.2: C, not started.
    let s12 = r.row("S12").unwrap();
    assert_eq!((s12.start, s12.finish, s12.percent_complete), (Some(d(3, 12)), Some(d(3, 13)), 0.0));
    // 1: spans Mar 2 to Mar 13; percent (300 + 200 + 0) / 10 = 50.
    let s1 = r.row("S1").unwrap();
    assert_eq!((s1.start, s1.finish, s1.level), (Some(d(3, 2)), Some(d(3, 13)), 1));
    assert!((s1.percent_complete - 50.0).abs() < 1e-9);
    assert_eq!(s1.total_float, Some(0));
    // By the weights entered (10, 30, 60): 1.1 = (10 * 100 + 30 * 40) / 40 = 55; 1 = 2200 / 100 = 22.
    let w = wbs_rollup(&p, &s, RollupBasis::Weight).unwrap();
    assert!((w.row("S11").unwrap().percent_complete - 55.0).abs() < 1e-9);
    assert!((w.row("S1").unwrap().percent_complete - 22.0).abs() < 1e-9);
    let csv = wbs_rollup_csv(&r);
    assert!(csv.starts_with(ROLLUP_CSV_HEADER));
    assert!(csv.contains("S11,1.1,Earthworks,2,2026-03-02,2026-03-11,62.50,2,1,0"));
}

#[test]
fn rollup_by_weight_needs_every_weight_and_flat_programmes_still_work() {
    let mut p = wbs_programme();
    p.activity_mut("C").unwrap().weight = None;
    let s = schedule(&p).unwrap();
    let e = wbs_rollup(&p, &s, RollupBasis::Weight).unwrap_err().to_string();
    assert!(e.contains("missing on: C"), "{e}");
    // A flat programme has no summaries: empty roll-up, outline is the activities in order.
    let mut flat = Programme::new("Flat", d(3, 2), five());
    flat.add_activity(Activity::new("X", "1", "x", 2));
    flat.add_activity(Activity::new("Y", "2", "y", 2));
    let sf = schedule(&flat).unwrap();
    assert!(wbs_rollup(&flat, &sf, RollupBasis::Duration).unwrap().rows.is_empty());
    assert_eq!(outline(&flat).iter().map(|i| (i.kind, i.level)).collect::<Vec<_>>(), [(OutlineKind::Activity(0), 1), (OutlineKind::Activity(1), 1)]);
}

#[test]
fn outline_order_and_wbs_errors() {
    let mut p = wbs_programme();
    p.add_activity(Activity::new("Z", "", "orphan", 1));
    // S1, S11, A, B, S12, C, then the orphan: sub-summaries first, then own activities.
    let kinds: Vec<_> = outline(&p).iter().map(|i| (i.kind, i.level)).collect();
    assert_eq!(
        kinds,
        [
            (OutlineKind::Summary(0), 1),
            (OutlineKind::Summary(1), 2),
            (OutlineKind::Activity(0), 3),
            (OutlineKind::Activity(1), 3),
            (OutlineKind::Summary(2), 2),
            (OutlineKind::Activity(2), 3),
            (OutlineKind::Activity(3), 1),
        ]
    );
    let mut bad = wbs_programme();
    bad.summaries[0].parent = Some("S12".into()); // S1 -> S12 -> S1
    let codes: Vec<String> = validate(&bad).into_iter().map(|i| i.code).collect();
    assert!(codes.contains(&"wbs_cycle".to_string()));
    let mut bad = wbs_programme();
    bad.activity_mut("A").unwrap().parent = Some("nope".into());
    assert!(schedule(&bad).unwrap_err().to_string().contains("parent 'nope'"));
}

#[test]
fn gantt_with_summaries_has_a_row_per_summary_and_activity() {
    let p = wbs_programme();
    let s = schedule(&p).unwrap();
    let ru = wbs_rollup(&p, &s, RollupBasis::Duration).unwrap();
    let g = gantt_drawing_wbs(&p, &s, &GanttOptions::default(), &ru);
    assert_eq!(g.rows, ["wbs:S1", "wbs:S11", "A", "B", "wbs:S12", "C"]);
    // The plain chart is unchanged: activities only.
    assert_eq!(gantt_drawing(&p, &s, &GanttOptions::default()).rows, ["A", "B", "C"]);
}

#[test]
fn mspdi_keeps_the_wbs() {
    let p = wbs_programme();
    let s = schedule(&p).unwrap();
    let xml = export_mspdi(&p, Some(&s));
    assert!(xml.contains("<Summary>1</Summary>"));
    assert!(xml.contains("<OutlineNumber>1.1.2</OutlineNumber>"), "A is the first task of 1.1, B the second: outline 1.1.1 and 1.1.2");
    let (q, rep) = import_mspdi(&xml).unwrap();
    assert!(rep.warnings.iter().all(|w| !w.contains("summary")), "{:?}", rep.warnings);
    assert_eq!(q.summaries, p.summaries.iter().map(|s| SummaryTask { wbs: s.wbs.clone(), ..s.clone() }).collect::<Vec<_>>());
    assert_eq!(q.activity("A").unwrap().parent.as_deref(), Some("S11"));
    assert_eq!(q.activity("C").unwrap().parent.as_deref(), Some("S12"));
    // And the rescheduled result is the same.
    let s2 = schedule(&q).unwrap();
    for r in &s.rows {
        let r2 = s2.row(&r.id).unwrap();
        assert_eq!((r.early_start, r.early_finish), (r2.early_start, r2.early_finish));
    }
}

// ---------------------------------------------------------------- XER

/// The calendar data of a project calendar in a P6 8.2.0 export (the "5 Day" calendar of the
/// sample file SuccessorCalendar.xer of the MPXJ test data), with its 14 non-working days. The
/// date numbers are days since 1899-12-30: 43650 is Thursday 4 July 2019 and 43710 is Monday 2
/// September 2019, 43797 is Thursday 28 November 2019, 43824 is Wednesday 25 December 2019.
const P6_CALENDAR_DATA: &str = "(0||CalendarData()((0||DaysOfWeek()((0||1()())(0||2()((0||0(s|08:00|f|12:00)())(0||1(s|13:00|f|17:00)())))(0||3()((0||0(s|08:00|f|12:00)())(0||1(s|13:00|f|17:00)())))(0||4()((0||0(s|08:00|f|12:00)())(0||1(s|13:00|f|17:00)())))(0||5()((0||0(s|08:00|f|12:00)())(0||1(s|13:00|f|17:00)())))(0||6()((0||0(s|08:00|f|12:00)())(0||1(s|13:00|f|17:00)())))(0||7()())))(0||Exceptions()((0||0(d|43650)())(0||1(d|43651)())(0||2(d|43710)())(0||3(d|43797)())(0||4(d|43798)())(0||5(d|43823)())(0||6(d|43824)())(0||7(d|43831)())(0||8(d|43976)())(0||9(d|44015)())(0||10(d|44081)())(0||11(d|44161)())(0||12(d|44189)())(0||13(d|44190)())))))";

#[test]
fn calendar_data_of_a_real_p6_export_is_read_and_written_back_byte_for_byte() {
    let line = |serial: i64| crate::xer::date_of_serial(serial).unwrap();
    assert_eq!(line(43650).to_string(), "2019-07-04");
    assert_eq!(line(43650).weekday(), Weekday::Thu);
    assert_eq!(line(43710).to_string(), "2019-09-02");
    assert_eq!(line(43710).weekday(), Weekday::Mon);
    assert_eq!(line(43797).to_string(), "2019-11-28");
    assert_eq!(line(43824).to_string(), "2019-12-25");
    let data = crate::xer::read_calendar_data(P6_CALENDAR_DATA).unwrap();
    assert_eq!(data.weekdays.as_ref().unwrap().iter().copied().collect::<Vec<_>>(), [Weekday::Mon, Weekday::Tue, Weekday::Wed, Weekday::Thu, Weekday::Fri]);
    assert_eq!(data.holidays.len(), 14);
    assert!(data.holidays.contains(&Date::from_ymd(2019, 7, 4).unwrap()));
    assert_eq!(data.hours[&Weekday::Mon], 8.0);
    // Writing a calendar with the same days and holidays reproduces the file's text exactly.
    let mut c = five();
    c.holidays = data.holidays.clone();
    assert_eq!(crate::xer::write_calendar_data(&c), P6_CALENDAR_DATA);
}

fn tsv(cells: &[&str]) -> String {
    cells.join("\t")
}

/// A small XER file in the shape of a P6 export, with a WBS, a start and a finish milestone, two
/// constraints on one activity, two relationships between the same pair and a level-of-effort
/// activity (skipped).
fn hand_xer() -> String {
    let cal5 = P6_CALENDAR_DATA;
    let rows: Vec<String> = vec![
        tsv(&["ERMHDR", "8.2.0", "2026-03-01", "Project", "admin", "Primavera Admin", "dbxDatabaseNoName", "Project Management", "USD"]),
        tsv(&["%T", "PROJECT"]),
        tsv(&["%F", "proj_id", "proj_short_name", "clndr_id", "plan_start_date", "plan_end_date", "last_recalc_date", "export_flag", "name_sep_char", "critical_path_type"]),
        tsv(&["%R", "1", "HAND", "10", "2026-03-02 08:00", "", "2026-03-02 00:00", "Y", ".", "CT_TotFloat"]),
        tsv(&["%T", "CALENDAR"]),
        tsv(&["%F", "clndr_id", "default_flag", "clndr_name", "proj_id", "base_clndr_id", "clndr_type", "day_hr_cnt", "clndr_data"]),
        tsv(&["%R", "10", "N", "Five", "1", "", "CA_Project", "8", cal5]),
        tsv(&["%T", "PROJWBS"]),
        tsv(&["%F", "wbs_id", "proj_id", "seq_num", "proj_node_flag", "wbs_short_name", "wbs_name", "parent_wbs_id"]),
        tsv(&["%R", "100", "1", "10", "Y", "H", "Hand project", ""]),
        tsv(&["%R", "101", "1", "10", "N", "1", "Roadworks", "100"]),
        tsv(&["%R", "102", "1", "10", "N", "1", "Drainage", "101"]),
        tsv(&["%T", "TASK"]),
        tsv(&[
            "%F", "task_id", "proj_id", "wbs_id", "clndr_id", "complete_pct_type", "task_type", "status_code", "task_code", "task_name", "target_drtn_hr_cnt", "remain_drtn_hr_cnt", "cstr_type", "cstr_date",
            "cstr_type2", "cstr_date2", "phys_complete_pct",
        ]),
        tsv(&["%R", "201", "1", "102", "10", "CP_Drtn", "TT_Task", "TK_NotStart", "A", "Pipes", "24", "24", "CS_MSOA", "2026-03-04 00:00", "CS_MEOB", "2026-03-20 00:00", "0"]),
        tsv(&["%R", "202", "1", "101", "10", "CP_Drtn", "TT_Task", "TK_NotStart", "B", "Backfill", "40", "40", "", "", "", "", "0"]),
        tsv(&["%R", "203", "1", "101", "10", "CP_Drtn", "TT_FinMile", "TK_NotStart", "M", "Done", "0", "0", "", "", "", "", "0"]),
        tsv(&["%R", "204", "1", "101", "10", "CP_Drtn", "TT_LOE", "TK_NotStart", "L", "Supervision", "0", "0", "", "", "", "", "0"]),
        tsv(&["%R", "205", "1", "100", "10", "CP_Drtn", "TT_Mile", "TK_NotStart", "S", "Notice", "0", "0", "CS_ALAP", "", "", "", "0"]),
        tsv(&["%T", "TASKPRED"]),
        tsv(&["%F", "task_pred_id", "task_id", "pred_task_id", "proj_id", "pred_proj_id", "pred_type", "lag_hr_cnt"]),
        tsv(&["%R", "1", "202", "201", "1", "1", "PR_FS", "0"]),
        tsv(&["%R", "2", "202", "201", "1", "1", "PR_SS", "16"]),
        tsv(&["%R", "3", "203", "202", "1", "1", "PR_FS", "0"]),
        tsv(&["%R", "4", "204", "202", "1", "1", "PR_FS", "0"]),
        tsv(&["%E"]),
    ];
    rows.join("\n") + "\n"
}

#[test]
fn import_of_a_hand_made_xer_in_the_p6_shape() {
    let (p, rep) = import_xer(&hand_xer()).unwrap();
    assert_eq!((p.name.as_str(), p.start, p.calendar.as_str()), ("Hand project", d(3, 2), "Five"));
    assert_eq!(p.data_date, None, "no progress, so no data date");
    // WBS: root is the project; "1" holds "1.1" (the code joins the short names).
    assert_eq!(p.summaries, vec![SummaryTask::new("101", "1", "Roadworks", None), SummaryTask::new("102", "1.1", "Drainage", Some("101"))]);
    let a = p.activity("A").unwrap();
    assert_eq!((a.duration, a.parent.as_deref(), a.wbs.as_str()), (3, Some("102"), "1.1"));
    assert_eq!(a.constraint, Some(Constraint::StartNoEarlierThan(d(3, 4))));
    assert_eq!(a.extra_constraints, vec![Constraint::FinishNoLaterThan(d(3, 20))]);
    assert_eq!(p.activity("B").unwrap().duration, 5);
    let m = p.activity("M").unwrap();
    assert_eq!((m.duration, m.milestone_kind, m.parent.as_deref()), (0, MilestoneKind::Finish, Some("101")));
    let s0 = p.activity("S").unwrap();
    assert_eq!((s0.milestone_kind, s0.parent.as_deref()), (MilestoneKind::Start, None));
    assert!(p.activity("L").is_none());
    // Three relationships kept (the fourth ends at the level-of-effort activity). 16 hours of lag
    // on an 8-hour calendar is two days.
    assert_eq!(
        p.dependencies,
        vec![
            Dependency { predecessor: "A".into(), successor: "B".into(), kind: LinkType::FS, lag: 0 },
            Dependency { predecessor: "A".into(), successor: "B".into(), kind: LinkType::SS, lag: 2 },
            Dependency { predecessor: "B".into(), successor: "M".into(), kind: LinkType::FS, lag: 0 },
        ]
    );
    let joined = rep.warnings.join("\n");
    assert!(joined.contains("TT_LOE"), "{joined}");
    assert!(joined.contains("CS_ALAP"), "{joined}");
    assert!(joined.contains("1 relationship(s)"), "{joined}");
    // Hand-worked schedule, start Monday 2 March. A: start no earlier than Wed 4 March, 3 days:
    // Mar 4, 5, 6. B: FS from A gives Mon 9 March; SS+2 from A's start (Mar 4) gives Fri 6 March;
    // the larger is Mar 9, 5 days: Mar 9 to Mar 13. M: finish milestone after B, on Fri 13 March.
    let s = schedule(&p).unwrap();
    let r = |id: &str| s.row(id).unwrap();
    assert_eq!((r("A").early_start, r("A").early_finish), (d(3, 4), d(3, 6)));
    assert_eq!((r("B").early_start, r("B").early_finish), (d(3, 9), d(3, 13)));
    assert_eq!(r("M").early_start, d(3, 13));
}

#[test]
fn import_reads_the_real_sample_shape_calendar_inheritance_and_other_warnings() {
    // A project calendar that lists no week days and names a base calendar (as in the Aspose sample).
    let base = P6_CALENDAR_DATA;
    let child = "(0||CalendarData()((0||DaysOfWeek()())(0||Exceptions()())))";
    let mut t = hand_xer();
    t = t.replace(&format!("%R\t10\tN\tFive\t1\t\tCA_Project\t8\t{base}"), &format!("%R\t10\tN\tFive\t1\t11\tCA_Project\t8\t{child}\n%R\t11\tN\tBase\t\t\tCA_Base\t8\t{base}"));
    let (p, rep) = import_xer(&t).unwrap();
    let c = p.calendar_named("Five").unwrap();
    assert!(c.working_days.contains(&Weekday::Mon) && !c.working_days.contains(&Weekday::Sat));
    assert!(c.holidays.contains(&Date::from_ymd(2019, 7, 4).unwrap()), "holidays come from the base too");
    assert!(rep.warnings.iter().any(|w| w.contains("base calendar 'Base'")));
    // Not an XER file; a missing table.
    assert!(matches!(import_xer("<xml/>"), Err(XerError::NotXer)));
    assert!(matches!(import_xer("ERMHDR\t8\n%T\tPROJECT\n%F\tproj_id\n"), Err(XerError::Missing(_))));
    // Windows-1252 bytes are decoded.
    assert_eq!(decode_xer_bytes(&[0x4D, 0x69, 0x64, 0x93, 0x78, 0x94]), "Mid\u{201C}x\u{201D}");
}

#[test]
fn xer_progress_and_constraints_import() {
    // B is in progress: target 40 hours, 16 remaining, started Mon 2 March; the project's last
    // recalculated date is 2026-03-06 17:00. Duration percent = 1 - 16 / 40 = 60. B has no
    // relationships here, so the data date is the only thing that moves it.
    let cal = P6_CALENDAR_DATA;
    let text = [
        tsv(&["ERMHDR", "8.2.0", "2026-03-06", "Project", "admin", "Primavera Admin", "dbxDatabaseNoName", "Project Management", "USD"]),
        tsv(&["%T", "PROJECT"]),
        tsv(&["%F", "proj_id", "proj_short_name", "clndr_id", "plan_start_date", "plan_end_date", "last_recalc_date", "export_flag"]),
        tsv(&["%R", "1", "P", "10", "2026-03-02 08:00", "2026-04-30 17:00", "2026-03-06 17:00", "Y"]),
        tsv(&["%T", "CALENDAR"]),
        tsv(&["%F", "clndr_id", "clndr_name", "proj_id", "clndr_type", "day_hr_cnt", "clndr_data"]),
        tsv(&["%R", "10", "Five", "1", "CA_Project", "8", cal]),
        tsv(&["%T", "PROJWBS"]),
        tsv(&["%F", "wbs_id", "proj_id", "proj_node_flag", "wbs_short_name", "wbs_name", "parent_wbs_id"]),
        tsv(&["%R", "100", "1", "Y", "P", "Progress project", ""]),
        tsv(&["%T", "TASK"]),
        tsv(&["%F", "task_id", "proj_id", "wbs_id", "clndr_id", "complete_pct_type", "task_type", "status_code", "task_code", "task_name", "target_drtn_hr_cnt", "remain_drtn_hr_cnt", "act_start_date", "act_end_date", "phys_complete_pct", "cstr_type", "cstr_date"]),
        tsv(&["%R", "1", "1", "100", "10", "CP_Drtn", "TT_Task", "TK_Active", "B", "Backfill", "40", "16", "2026-03-02 08:00", "", "0", "CS_MANDSTART", "2026-03-02 08:00"]),
        tsv(&["%R", "2", "1", "100", "10", "CP_Phys", "TT_Task", "TK_Complete", "C", "Done", "16", "0", "2026-03-02 08:00", "2026-03-03 17:00", "100", "CS_MSO", "2026-03-02 00:00"]),
        tsv(&["%E"]),
    ]
    .join("\n")
        + "\n";
    let (p, rep) = import_xer(&text).unwrap();
    assert_eq!(p.data_date, Some(d(3, 6)));
    assert_eq!(p.required_finish, Some(d(4, 30)));
    let b = p.activity("B").unwrap();
    assert_eq!(b.progress.actual_start, Some(d(3, 2)));
    assert!((b.progress.percent_complete - 60.0).abs() < 1e-9);
    assert_eq!(b.progress.remaining_duration, Some(2));
    assert_eq!(b.status(), Status::InProgress);
    assert_eq!(b.constraint, Some(Constraint::MustStartOn(d(3, 2))));
    let c = p.activity("C").unwrap();
    assert_eq!((c.status(), c.progress.actual_finish), (Status::Complete, Some(d(3, 3))));
    // CS_MSO (start on) is read as start no earlier than, with a warning.
    assert_eq!(c.constraint, Some(Constraint::StartNoEarlierThan(d(3, 2))));
    assert!(rep.warnings.iter().any(|w| w.contains("CS_MSO")), "{:?}", rep.warnings);
    // The programme schedules: B's 2 remaining days run Mar 9 and 10 (from the day after the data date).
    let s = schedule(&p).unwrap();
    assert_eq!((s.row("B").unwrap().early_start, s.row("B").unwrap().early_finish), (d(3, 2), d(3, 10)));
}

/// Imports every `.xer` file in the directory named by `AFROBIM_XER_SAMPLES` (files exported by
/// P6 and published by other projects; they are not stored in this repository) and schedules the
/// result. Skipped when the variable is not set.
#[test]
fn import_of_real_p6_sample_files() {
    let Ok(dir) = std::env::var("AFROBIM_XER_SAMPLES") else {
        eprintln!("SKIPPED import_of_real_p6_sample_files: AFROBIM_XER_SAMPLES is not set");
        return;
    };
    let mut n = 0;
    for e in std::fs::read_dir(dir).unwrap() {
        let path = e.unwrap().path();
        if path.extension().and_then(|x| x.to_str()) != Some("xer") {
            continue;
        }
        let bytes = std::fs::read(&path).unwrap();
        if !bytes.starts_with(b"ERMHDR") {
            continue;
        }
        let (p, rep) = import_xer_bytes(&bytes).unwrap_or_else(|e| panic!("{}: {e}", path.display()));
        let s = schedule(&p).unwrap_or_else(|e| panic!("{}: {e}", path.display()));
        eprintln!("{}: {} activities, {} summaries, {} dependencies, finish {}, {} warning(s)", path.display(), p.activities.len(), p.summaries.len(), p.dependencies.len(), s.project_finish, rep.warnings.len());
        for w in rep.warnings.iter().take(4) {
            eprintln!("    {w}");
        }
        n += 1;
    }
    assert!(n > 0, "no sample files found");
}

/// A programme that exercises most of the model, with progress.
fn rich_programme() -> Programme {
    let mut p = Programme::new("Rich programme", d(3, 2), five().with_holiday(d(3, 16)).with_holiday(d(4, 3)));
    p.calendars.push(Calendar::new("Six", &[Weekday::Mon, Weekday::Tue, Weekday::Wed, Weekday::Thu, Weekday::Fri, Weekday::Sat]));
    p.calendar = "Five".into();
    p.add_summary(SummaryTask::new("S1", "1", "Works", None));
    p.add_summary(SummaryTask::new("S2", "1.1", "Earthworks", Some("S1")));
    {
        let a = p.add_activity(Activity::new("A", "1.1", "Clear", 3));
        a.parent = Some("S2".into());
        a.constraint = Some(Constraint::StartNoEarlierThan(d(3, 2)));
        a.extra_constraints = vec![Constraint::FinishNoLaterThan(d(4, 30))];
        a.progress.percent_complete = 100.0;
        a.progress.actual_start = Some(d(3, 2));
        a.progress.actual_finish = Some(d(3, 4));
    }
    {
        let b = p.add_activity(Activity::new("B", "1", "Cut", 5));
        b.parent = Some("S1".into());
        b.calendar = Some("Six".into());
        b.progress.percent_complete = 40.0;
        b.progress.actual_start = Some(d(3, 5));
        b.progress.remaining_duration = Some(3);
    }
    {
        let c = p.add_activity(Activity::new("C", "", "Notice", 0));
        c.milestone_kind = MilestoneKind::Start;
    }
    p.add_activity(Activity::new("D", "", "Done", 0));
    p.add_activity(Activity::new("E", "", "Finish works", 4));
    p.link("A", "B", LinkType::FS, 1);
    p.link("A", "E", LinkType::SS, 2);
    p.link("B", "E", LinkType::FF, -1);
    p.link("C", "A", LinkType::SF, 0);
    p.link("E", "D", LinkType::FS, 0);
    p.data_date = Some(d(3, 6));
    p.required_finish = Some(d(4, 30));
    p
}

fn rename_summaries_like(q: &mut Programme, original: &Programme) {
    // XER gives WBS nodes numeric ids, so match the imported summaries to the original ones by code.
    let pairs: Vec<(String, String)> = q.summaries.iter().map(|s| (s.id.clone(), original.summaries.iter().find(|o| o.wbs == s.wbs).unwrap().id.clone())).collect();
    let map = |id: &Option<String>| id.as_ref().map(|i| pairs.iter().find(|(a, _)| a == i).unwrap().1.clone());
    for s in &mut q.summaries {
        s.parent = map(&s.parent);
    }
    for s in &mut q.summaries {
        s.id = pairs.iter().find(|(a, _)| *a == s.id).unwrap().1.clone();
    }
    for a in &mut q.activities {
        a.parent = map(&a.parent);
    }
}

#[test]
fn xer_round_trip_keeps_the_programme_and_its_schedule() {
    let p = rich_programme();
    let s = schedule(&p).unwrap();
    let (text, rep) = export_xer_with_report(&p, Some(&s));
    assert!(text.starts_with("ERMHDR\t"));
    assert!(text.trim_end().ends_with("%E"));
    assert!(rep.warnings.is_empty(), "{:?}", rep.warnings);
    for t in ["PROJECT", "CALENDAR", "SCHEDOPTIONS", "PROJWBS", "TASK", "TASKPRED"] {
        assert!(text.contains(&format!("%T\t{t}\n")), "{t}");
    }
    let (mut q, irep) = import_xer(&text).unwrap();
    assert!(irep.warnings.is_empty(), "{:?}", irep.warnings);
    rename_summaries_like(&mut q, &p);
    assert_eq!(q.name, p.name);
    assert_eq!((q.start, q.data_date, q.required_finish, &q.calendar), (p.start, p.data_date, p.required_finish, &p.calendar));
    assert_eq!(q.calendars, p.calendars);
    assert_eq!(q.summaries, p.summaries);
    assert_eq!(q.activities, p.activities);
    assert_eq!(q.dependencies, p.dependencies);
    let s2 = schedule(&q).unwrap();
    assert_eq!(s2.rows, s.rows);
}

#[test]
fn xer_export_reports_what_it_drops() {
    let mut p = rich_programme();
    p.activity_mut("E").unwrap().extra_constraints = vec![Constraint::StartNoEarlierThan(d(3, 2)), Constraint::FinishNoLaterThan(d(5, 1)), Constraint::MustStartOn(d(3, 10))];
    p.activity_mut("E").unwrap().constraint = Some(Constraint::StartNoEarlierThan(d(3, 3)));
    p.activity_mut("E").unwrap().location = Some(LocationRange::new("MAIN", 0.0, 10.0, WorkDirection::Fixed));
    let (_, rep) = export_xer_with_report(&p, None);
    let j = rep.warnings.join("\n");
    assert!(j.contains("activity E: XER holds two constraints and it has 4"), "{j}");
    assert!(j.contains("location ranges"), "{j}");
    let (_, mrep) = export_mspdi_with_report(&p, None);
    assert!(mrep.warnings.iter().any(|w| w.contains("one constraint per task")));
}

/// Reads an exported file with the third-party `xerparser` Python package and compares what it
/// finds with the programme. Skipped (with a message) when python3 or the package is missing; set
/// `AFROBIM_XER_PYTHON` to the interpreter of an environment that has `pip install xerparser`.
#[test]
fn xer_file_parses_with_xerparser() {
    let p = rich_programme();
    let s = schedule(&p).unwrap();
    let text = export_xer(&p, Some(&s));
    if let Ok(dir) = std::env::var("AFROBIM_XER_DUMP") {
        // For trying the file in other readers by hand.
        std::fs::write(std::path::Path::new(&dir).join("rich.xer"), &text).unwrap();
    }
    let file = std::env::temp_dir().join(format!("afrobim-xer-check-{}.xer", std::process::id()));
    std::fs::write(&file, &text).unwrap();
    let python = std::env::var("AFROBIM_XER_PYTHON").unwrap_or_else(|_| "python3".to_string());
    let script = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("tests").join("xerparser_check.py");
    let out = std::process::Command::new(&python).arg("-I").arg(&script).arg(&file).output();
    let _ = std::fs::remove_file(&file);
    let out = match out {
        Ok(o) => o,
        Err(e) => {
            eprintln!("SKIPPED xer_file_parses_with_xerparser: cannot run {python}: {e}");
            return;
        }
    };
    if out.status.code() == Some(3) {
        eprintln!("SKIPPED xer_file_parses_with_xerparser: the xerparser package is not installed for {python}");
        return;
    }
    assert!(out.status.success(), "xerparser failed:\n{}\n{}", String::from_utf8_lossy(&out.stdout), String::from_utf8_lossy(&out.stderr));
    let v: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap();
    assert_eq!(v["name"], p.name.as_str());
    assert_eq!(v["data_date"], "2026-03-06");
    let tasks = v["tasks"].as_array().unwrap();
    assert_eq!(tasks.len(), p.activities.len());
    for a in &p.activities {
        let r = s.row(&a.id).unwrap();
        let t = tasks.iter().find(|t| t["code"] == a.id.as_str()).unwrap_or_else(|| panic!("task {} missing", a.id));
        assert_eq!(t["name"], a.name.as_str());
        assert_eq!(t["target_hours"].as_f64().unwrap(), a.duration as f64 * 8.0, "{}", a.id);
        assert_eq!(t["start"], r.early_start.to_string().as_str(), "start of {}", a.id);
        assert_eq!(t["finish"], r.early_finish.to_string().as_str(), "finish of {}", a.id);
        let status = match a.status() {
            Status::NotStarted => "TK_NotStart",
            Status::InProgress => "TK_Active",
            Status::Complete => "TK_Complete",
        };
        assert_eq!(t["status"].as_str().unwrap(), status);
        let want_wbs = a.parent.as_deref().and_then(|q| p.summary(q)).map(|q| q.wbs.clone()).unwrap_or_default();
        assert_eq!(t["wbs"].as_str().unwrap(), want_wbs, "wbs of {}", a.id);
        let cal = a.calendar.as_deref().unwrap_or(&p.calendar);
        assert_eq!(t["calendar"].as_str().unwrap(), cal);
        let type_want = if !a.is_milestone() { "TT_Task" } else if a.milestone_kind == MilestoneKind::Start { "TT_Mile" } else { "TT_FinMile" };
        assert_eq!(t["type"].as_str().unwrap(), type_want);
    }
    // Constraints of A: primary start no earlier than (CS_MSOA), secondary finish no later than (CS_MEOB).
    let ta = tasks.iter().find(|t| t["code"] == "A").unwrap();
    assert_eq!((ta["cstr_type"].as_str(), ta["cstr_date"].as_str()), (Some("CS_MSOA"), Some("2026-03-02")));
    assert_eq!((ta["cstr_type2"].as_str(), ta["cstr_date2"].as_str()), (Some("CS_MEOB"), Some("2026-04-30")));
    // Relationships with their lags in hours.
    let mut got: Vec<(String, String, String, f64)> =
        v["relationships"].as_array().unwrap().iter().map(|r| (r["pred"].as_str().unwrap().into(), r["succ"].as_str().unwrap().into(), r["link"].as_str().unwrap().into(), r["lag_hours"].as_f64().unwrap())).collect();
    let mut want: Vec<(String, String, String, f64)> = p
        .dependencies
        .iter()
        .map(|d| (d.predecessor.clone(), d.successor.clone(), format!("{:?}", d.kind), d.lag as f64 * 8.0))
        .collect();
    got.sort_by(|a, b| a.partial_cmp(b).unwrap());
    want.sort_by(|a, b| a.partial_cmp(b).unwrap());
    assert_eq!(got, want);
    let wbs: Vec<&str> = v["wbs"].as_array().unwrap().iter().map(|x| x.as_str().unwrap()).collect();
    assert_eq!(wbs, ["1", "1.1"]);
    let hol: Vec<&str> = v["holidays"]["Five"].as_array().unwrap().iter().map(|x| x.as_str().unwrap()).collect();
    assert_eq!(hol, ["2026-03-16", "2026-04-03"]);
}

// ---------------------------------------------------------------- several constraints and the longest path

#[test]
fn several_constraints_on_one_activity() {
    // X, 3 days, start Mon 2 March. Start no earlier than Mar 4 (primary) and Mar 9 (extra): the
    // later wins, so X works Mar 9, 10, 11. Finish no later than Fri 6 March (extra): the late
    // finish is Mar 6 (late start Mar 4), so the total float is the working days in [Mar 11, Mar 6),
    // which is -(Mar 6, Mar 9, Mar 10) = -3.
    let mut p = Programme::new("Constraints", d(3, 2), five());
    let x = p.add_activity(Activity::new("X", "", "x", 3));
    x.constraint = Some(Constraint::StartNoEarlierThan(d(3, 4)));
    x.extra_constraints = vec![Constraint::StartNoEarlierThan(d(3, 9)), Constraint::FinishNoLaterThan(d(3, 6))];
    let s = schedule(&p).unwrap();
    let r = s.row("X").unwrap();
    assert_eq!((r.early_start, r.early_finish), (d(3, 9), d(3, 11)));
    assert_eq!((r.late_start, r.late_finish, r.total_float), (d(3, 4), d(3, 6), -3));
    assert!(s.issues.iter().any(|i| i.code == "negative_float"));
    assert!(activity_table_csv(&p, &s).contains("SNET 2026-03-04; SNET 2026-03-09; FNLT 2026-03-06"));
    // The extra constraints survive serialisation, and an old file without the field loads.
    let q = Programme::from_value(&p.to_value()).unwrap();
    assert_eq!(q, p);
    let mut v = p.to_value();
    v["activities"][0].as_object_mut().unwrap().remove("extra_constraints");
    v["activities"][0].as_object_mut().unwrap().remove("parent");
    v.as_object_mut().unwrap().remove("summaries");
    assert!(Programme::from_value(&v).unwrap().activities[0].extra_constraints.is_empty());
}

/// A 3 days, B 2 days, C 5 days, D 1 day. A -FS-> B, A -FS-> C, B -FS-> D, C -FS-> D.
/// A: Mar 2 to Mar 4. B: Mar 5, 6. C: Mar 5, 6, 9, 10, 11. D: Mar 12. Project finish Thu 12 March.
/// The longest path is A, C, D; B has 3 days of float.
fn diamond() -> Programme {
    let mut p = Programme::new("Diamond", d(3, 2), five());
    for (id, dur) in [("A", 3), ("B", 2), ("C", 5), ("D", 1)] {
        p.add_activity(Activity::new(id, "", id, dur));
    }
    p.link("A", "B", LinkType::FS, 0);
    p.link("A", "C", LinkType::FS, 0);
    p.link("B", "D", LinkType::FS, 0);
    p.link("C", "D", LinkType::FS, 0);
    p
}

#[test]
fn longest_path_marking_next_to_total_float() {
    let mut p = diamond();
    let s = schedule(&p).unwrap();
    assert_eq!(s.row("D").unwrap().early_finish, d(3, 12));
    assert_eq!(s.row("B").unwrap().total_float, 3);
    let crit = |s: &Schedule| s.rows.iter().filter(|r| r.critical).map(|r| r.id.clone()).collect::<Vec<_>>();
    let lp = |s: &Schedule| s.rows.iter().filter(|r| r.on_longest_path).map(|r| r.id.clone()).collect::<Vec<_>>();
    assert_eq!(crit(&s), ["A", "C", "D"]);
    assert_eq!(lp(&s), ["A", "C", "D"]);
    // A required finish ten days after the computed finish: nothing has zero float, so by total
    // float nothing is critical; the longest path is the same chain, and the option marks it.
    p.required_finish = Some(d(3, 26));
    let s = schedule(&p).unwrap();
    assert!(crit(&s).is_empty());
    assert_eq!(lp(&s), ["A", "C", "D"]);
    let s = schedule_with(&p, &ScheduleOptions { critical: CriticalBasis::LongestPath }).unwrap();
    assert_eq!(crit(&s), ["A", "C", "D"]);
    // Work already done is never on the path: with A complete, the path starts at C.
    let mut q = diamond();
    q.data_date = Some(d(3, 4));
    let a = q.activity_mut("A").unwrap();
    a.progress.percent_complete = 100.0;
    a.progress.actual_start = Some(d(3, 2));
    a.progress.actual_finish = Some(d(3, 4));
    let s = schedule_with(&q, &ScheduleOptions { critical: CriticalBasis::LongestPath }).unwrap();
    assert_eq!(crit(&s), ["C", "D"]);
}

// ---------------------------------------------------------------- time-location chart

fn linear_programme() -> Programme {
    // Earthworks: 5 days from Mon 2 March (Mar 2 to Mar 6), station 0 to 500 m working up-chainage.
    // Culvert: 2 days from Mon 9 March (Mar 9, 10), fixed at station 250.
    let mut p = Programme::new("Road", d(3, 2), five());
    let e = p.add_activity(Activity::new("EW", "", "Earthworks", 5));
    e.location = Some(LocationRange::new("MAIN", 0.0, 500.0, WorkDirection::Increasing));
    let c = p.add_activity(Activity::new("CV", "", "Culvert", 2));
    c.location = Some(LocationRange::new("MAIN", 250.0, 250.0, WorkDirection::Fixed));
    c.constraint = Some(Constraint::StartNoEarlierThan(d(3, 9)));
    p
}

fn opts() -> TimeLocationOptions {
    TimeLocationOptions { metre_scale: 0.1, day_scale: 2.0, margin: 10.0, station_tick_m: Some(100.0), ..TimeLocationOptions::default() }
}

fn lines(c: &TimeLocationChart) -> Vec<(Pt2, Pt2, f64, Option<Colour>)> {
    c.drawing.primitives.iter().filter_map(|p| if let Primitive::Line { from, to, width, colour } = p { Some((*from, *to, *width, *colour)) } else { None }).collect()
}

fn fills(c: &TimeLocationChart) -> Vec<(Vec<Pt2>, Colour)> {
    c.drawing.primitives.iter().filter_map(|p| if let Primitive::Fill { outline, colour } = p { Some((outline.clone(), *colour)) } else { None }).collect()
}

fn near(a: Pt2, x: f64, y: f64) -> bool {
    (a.x - x).abs() < 1e-9 && (a.y - y).abs() < 1e-9
}

#[test]
fn time_location_chart_with_hand_worked_coordinates() {
    let p = linear_programme();
    let s = schedule(&p).unwrap();
    // EW ends Mar 6 and has float to the project finish (Mar 10): 2 working days (Mar 6, Mar 9),
    // so it is blue; CV finishes the project at zero float, so it is red.
    assert_eq!(s.row("EW").unwrap().total_float, 2);
    assert!(s.row("CV").unwrap().critical);
    let res = resolve_locations(&p, None);
    let chart = time_location_chart(&p, &s, &res, &opts()).unwrap();
    // Stations 0 to 500, ticks every 100: scale 0 to 500. First date Mar 2 is a Monday; last date
    // Mar 10; 8 + 2 = 10 days. Position = (margin + station * 0.1, margin + days * 2).
    assert_eq!((chart.station_min, chart.station_max, chart.origin_date), (0.0, 500.0, d(3, 2)));
    // EW: from (station 0, start of Mar 2 = day 0) = (10, 10) to (station 500, end of Mar 6 = day 5)
    // = (60, 20), a red-free blue line of width 0.6.
    let blue = lines(&chart).into_iter().find(|l| l.3 == Some(Colour::Blue) && l.2 == 0.6).expect("the EW line");
    assert!(near(blue.0, 10.0, 10.0) && near(blue.1, 60.0, 20.0), "{blue:?}");
    // CV: a bar. Width 0 is widened to the minimum 2.0 units = 20 m around 250 m: stations 240 to
    // 260, x = 34 to 36. From the start of Mar 9 (day 7, y = 24) to the end of Mar 10 (day 9, y = 28).
    let bar = fills(&chart).into_iter().find(|f| f.1 == Colour::Red).expect("the CV bar");
    let pts = &bar.0;
    assert!(near(pts[0], 34.0, 24.0) && near(pts[1], 36.0, 24.0) && near(pts[2], 36.0, 28.0) && near(pts[3], 34.0, 28.0), "{pts:?}");
    // pos() agrees.
    assert!(near(chart.pos_at(500.0, d(3, 6), true), 60.0, 20.0));
    // Labels are separate: both activity ids, and the stations 0 to 500.
    let acts: Vec<&str> = chart.labels.iter().filter(|l| l.kind == TlLabelKind::Activity).map(|l| l.text.as_str()).collect();
    assert_eq!(acts, ["EW", "CV"]);
    assert_eq!(chart.labels.iter().filter(|l| l.kind == TlLabelKind::Station).count(), 6);
    // The CSV of the plotted data.
    let csv = time_location_csv(&p, &s, &res, &opts());
    let mut it = csv.lines();
    assert_eq!(it.next().unwrap(), TIME_LOCATION_CSV_HEADER);
    assert_eq!(it.next().unwrap(), "EW,Earthworks,MAIN,entered,line,Increasing,no,NotStarted,0,0,2026-03-02,500,2026-03-06,,,");
    assert_eq!(it.next().unwrap(), "CV,Culvert,MAIN,entered,bar,Fixed,yes,NotStarted,0,250,2026-03-09,250,2026-03-10,,,");
}

#[test]
fn parallelogram_decreasing_direction_baseline_and_data_date() {
    let mut p = linear_programme();
    {
        let e = p.activity_mut("EW").unwrap();
        e.location = Some(LocationRange { dwell_days: Some(2), direction: WorkDirection::Decreasing, ..LocationRange::new("MAIN", 0.0, 500.0, WorkDirection::Decreasing) });
    }
    let s = schedule(&p).unwrap();
    p.set_baseline("Base", &s);
    let res = resolve_locations(&p, None);
    let chart = time_location_chart(&p, &s, &res, &opts()).unwrap();
    // Decreasing: the front starts at 500 m. Corners (a, start) (b, finish - 2) (b, finish) (a, start + 2)
    // with a = 500 and b = 0: (60, 10) (10, 16) (10, 20) (60, 14). Day 0 is y 10, day 3 is y 16, day 5 is y 20, day 2 is y 14.
    let f = fills(&chart).into_iter().find(|f| f.1 == Colour::Blue).expect("the parallelogram");
    let c = &f.0;
    assert!(near(c[0], 60.0, 10.0) && near(c[1], 10.0, 16.0) && near(c[2], 10.0, 20.0) && near(c[3], 60.0, 14.0), "{c:?}");
    // The baseline has the same shape as a thin grey outline (4 edges per parallelogram, 4 per bar).
    let thin: Vec<_> = lines(&chart).into_iter().filter(|l| l.3 == Some(Colour::Grey) && (l.2 - 0.6 * 0.35).abs() < 1e-9).collect();
    assert_eq!(thin.len(), 8);
    // The baseline is in a lighter style than the plan: thinner than the 0.6 plan line.
    assert!(thin.iter().all(|l| l.2 < 0.6));
    // With a data date the green line is drawn across at the end of that day.
    let mut q = p.clone();
    q.data_date = Some(d(3, 6));
    q.activity_mut("EW").unwrap().progress.percent_complete = 100.0;
    q.activity_mut("EW").unwrap().progress.actual_start = Some(d(3, 2));
    q.activity_mut("EW").unwrap().progress.actual_finish = Some(d(3, 6));
    let sq = schedule(&q).unwrap();
    let cq = time_location_chart(&q, &sq, &resolve_locations(&q, None), &opts()).unwrap();
    let green = lines(&cq).into_iter().find(|l| l.3 == Some(Colour::Green)).unwrap();
    // End of Mar 6 is day 5: y = 10 + 5 * 2 = 20, from station 0 (x = 10) to 500 (x = 60).
    assert!(near(green.0, 10.0, 20.0) && near(green.1, 60.0, 20.0));
    // A complete activity is grey.
    assert!(fills(&cq).iter().any(|f| f.1 == Colour::Grey));
    // Horizontal time axis swaps the axes.
    let h = time_location_chart(&p, &s, &res, &TimeLocationOptions { time_vertical: false, ..opts() }).unwrap();
    assert!(near(h.pos(500.0, 5.0), 20.0, 60.0));
}

#[test]
fn locations_derived_from_site_placements_of_linked_elements() {
    let mut doc = Document::default();
    let mut ids = Vec::new();
    for (name, st, al) in [("CV-1", 100.0, "MAIN"), ("CV-2", 180.0, "MAIN"), ("CV-3", 120.0, "MAIN"), ("OTHER", 5.0, "SIDE")] {
        let id = doc.add("culvert", name, Pt::new(0.0, 0.0), 1.0, 1.0, "0", serde_json::Value::Null);
        doc.find_mut(id).unwrap().site = Some(SitePlacement { x: 0.0, y: 0.0, z: 0.0, rotation_deg: 0.0, alignment: Some(al.into()), station_m: Some(st), offset_m: Some(0.0) });
        ids.push(id);
    }
    let nowhere = doc.add("culvert", "CV-4", Pt::new(0.0, 0.0), 1.0, 1.0, "0", serde_json::Value::Null);
    let mut p = Programme::new("Derive", d(3, 2), five());
    p.add_activity(Activity::new("CULV", "", "Culverts", 4));
    p.add_activity(Activity::new("MIX", "", "Mixed", 2));
    p.add_activity(Activity::new("OWN", "", "Own range", 2)).location = Some(LocationRange::new("MAIN", 0.0, 50.0, WorkDirection::Increasing));
    p.link_elements("CULV", ElementSelector::Rule(ElementRule { mark_pattern: Some("CV-*".into()), ..Default::default() }), LinkAction::Construct);
    p.link_elements("MIX", ElementSelector::Id(ids[0]), LinkAction::Construct);
    p.link_elements("MIX", ElementSelector::Id(ids[3]), LinkAction::Construct);
    p.link_elements("OWN", ElementSelector::Id(ids[0]), LinkAction::Construct);
    let _ = nowhere;
    let res = resolve_locations(&p, Some(&doc));
    // CULV: CV-1, CV-2, CV-3 are on MAIN at 100, 180, 120; CV-4 has no station. Range 100 to 180, fixed.
    let c = res.get("CULV").unwrap();
    assert!(c.derived);
    assert_eq!((c.range.alignment.as_str(), c.range.low(), c.range.high(), c.range.direction), ("MAIN", 100.0, 180.0, WorkDirection::Fixed));
    // MIX spans two alignments: no range, with a note. OWN keeps its entered range.
    assert!(res.get("MIX").is_none());
    assert!(res.notes.iter().any(|n| n.contains("MIX") && n.contains("2 alignments")), "{:?}", res.notes);
    assert!(res.notes.iter().any(|n| n.contains("CULV") && n.contains("without a station")), "{:?}", res.notes);
    assert!(!res.get("OWN").unwrap().derived);
    assert_eq!(res.alignments(), ["MAIN"]);
    // Without a document nothing is derived.
    assert!(resolve_locations(&p, None).get("CULV").is_none());
    let s = schedule(&p).unwrap();
    let csv = time_location_csv(&p, &s, &res, &TimeLocationOptions::default());
    assert!(csv.contains("CULV,Culverts,MAIN,derived,bar,Fixed"), "{csv}");
}

// ---------------------------------------------------------------- 4D playback

fn playback_model() -> (Programme, Document, Vec<afrobim_kernel::ElementId>) {
    // Start Monday 2 March 2026. A 3 days (Mar 2 to 4), B 2 days (Mar 5, 6), C 4 days (Mar 9 to 12),
    // A -FS-> B -FS-> C. Elements: E1 built by A, E2 demolished by B, E3 temporary during C, E4
    // linked to nothing, E5 built by A and by C (two links).
    let mut p = Programme::new("4D", d(3, 2), five());
    for (id, dur) in [("A", 3), ("B", 2), ("C", 4)] {
        p.add_activity(Activity::new(id, "", id, dur));
    }
    p.link("A", "B", LinkType::FS, 0);
    p.link("B", "C", LinkType::FS, 0);
    let mut doc = Document::default();
    let mut ids = Vec::new();
    for name in ["E1", "E2", "E3", "E4", "E5"] {
        ids.push(doc.add("thing", name, Pt::new(0.0, 0.0), 1.0, 1.0, "0", serde_json::Value::Null));
    }
    p.link_elements("A", ElementSelector::Id(ids[0]), LinkAction::Construct);
    p.link_elements("B", ElementSelector::Id(ids[1]), LinkAction::Demolish);
    p.link_elements("C", ElementSelector::Id(ids[2]), LinkAction::Temporary);
    p.link_elements("A", ElementSelector::Id(ids[4]), LinkAction::Construct);
    p.link_elements("C", ElementSelector::Id(ids[4]), LinkAction::Demolish);
    (p, doc, ids)
}

#[test]
fn playback_agrees_with_repeated_single_date_calls() {
    let (p, doc, ids) = playback_model();
    let s = schedule(&p).unwrap();
    let (from, to) = (d(2, 20), d(3, 31));
    let pb = element_timelines(&p, &s, &doc, from, to);
    assert_eq!(pb.unlinked, vec![ids[3]]);
    assert_eq!(pb.elements.len(), 4);
    let mut day = from;
    while day <= to {
        let single = element_states(&p, &s, &doc, day);
        for st in &single.states {
            let at = pb.timeline(st.element).unwrap().at(day).unwrap();
            assert_eq!((at.phase, at.present), (st.phase, st.present), "element {:?} on {day}", st.element);
        }
        day = day.add_days(1);
    }
    // Before the range there is no state.
    assert!(pb.timeline(ids[0]).unwrap().at(d(2, 19)).is_none());
    // E1 (built by A, Mar 2 to 4): absent until Mar 2, in progress Mar 2 and 3 (1/3, 2/3), complete from Mar 4.
    let t1 = pb.timeline(ids[0]).unwrap();
    assert_eq!(t1.initial, StateChange { date: d(2, 20), phase: Phase::NotStarted, present: false });
    let seq: Vec<(Date, Phase)> = t1.changes.iter().map(|c| (c.date, c.phase)).collect();
    assert_eq!(seq.len(), 3);
    assert_eq!(seq[0].0, d(3, 2));
    assert!(matches!(seq[0].1, Phase::InProgress { fraction } if (fraction - 1.0 / 3.0).abs() < 1e-9));
    assert_eq!(seq[1].0, d(3, 3));
    assert!(matches!(seq[1].1, Phase::InProgress { fraction } if (fraction - 2.0 / 3.0).abs() < 1e-9));
    assert_eq!(seq[2], (d(3, 4), Phase::Complete));
    // The dates at which anything changes: A Mar 2, 3, 4; B (Mar 5, 6: E2 is demolished, one day in
    // progress then removed) Mar 5 and Mar 6; C Mar 9, 10, 11, 12 (E3 appears, in progress 1/4, 2/4,
    // 3/4, then removed on Mar 12; E5 is demolished from Mar 12 on, and hidden from Mar 12).
    assert_eq!(pb.change_dates, vec![d(3, 2), d(3, 3), d(3, 4), d(3, 5), d(3, 6), d(3, 9), d(3, 10), d(3, 11), d(3, 12)]);
    // An empty range.
    assert!(element_timelines(&p, &s, &doc, to, from).elements.is_empty());
}

#[test]
fn playback_does_not_visit_every_calendar_day() {
    // A year-long range changes state only on the working days of the linked activities.
    let (p, doc, _) = playback_model();
    let s = schedule(&p).unwrap();
    let pb = element_timelines(&p, &s, &doc, d(1, 1), Date::from_ymd(2026, 12, 31).unwrap());
    assert_eq!(pb.change_dates.len(), 9);
    assert!(pb.elements.iter().all(|t| t.changes.len() <= 7));
}
