//! Demo records for `--demo`: a small made-up programme linked to the demo elements, a neutral
//! bill with mapping rules and arbitrary rates, two clash tests, and a few made-up site records and
//! actual costs.
//!
//! Every number below is an arbitrary demo value chosen to make the windows show something. None
//! is a duration, rate, percentage or tolerance from any project or standard. The estimate is
//! named "DEMO, not real rates".

use afrobim_cost::{Addition, AdditionBase, AdditionKind, Bill, Decimal, Item, MappingRule, Phasing, PhasingTarget, Rate};
use afrobim_coordination::{ClashTest, Coordination, Selection};
use afrobim_kernel::Document;
use afrobim_schedule::{Activity, Date, ElementRule, ElementSelector, LinkAction, LinkType, LocationRange, WorkDirection, Weekday};

use crate::{cost_glue, schedule_glue};

fn rule(category: &str) -> ElementSelector {
    ElementSelector::Rule(ElementRule { category: Some(category.to_string()), layer: None, mark_pattern: None })
}

fn date(y: i32, m: u32, d: u32) -> Date {
    Date::from_ymd(y, m, d).expect("a fixed demo date")
}

/// Adds the demo programme, estimate and clash tests. Returns what could not be added.
pub fn add_records(doc: &mut Document) -> Vec<String> {
    let mut problems = Vec::new();
    if let Err(e) = add_programme(doc) {
        problems.push(format!("Demo programme: {e}"));
    }
    if let Err(e) = add_estimate(doc) {
        problems.push(format!("Demo estimate: {e}"));
    }
    if let Err(e) = add_clash_tests(doc) {
        problems.push(format!("Demo clash tests: {e}"));
    }
    add_check_inputs(doc);
    if let Err(e) = add_site_progress(doc) {
        problems.push(format!("Demo site progress: {e}"));
    }
    problems
}

fn add_programme(doc: &mut Document) -> Result<(), String> {
    // Arbitrary demo values: start Monday 2 March 2026, a six-day working week, no days off.
    let days = [Weekday::Mon, Weekday::Tue, Weekday::Wed, Weekday::Thu, Weekday::Fri, Weekday::Sat];
    let mut p = schedule_glue::new_programme("Demo programme (made-up durations)", date(2026, 3, 2), "Demo six-day week", &days)?;
    // Arbitrary demo durations in working days, and the elements each activity builds.
    let list: [(&str, &str, &str, u32, &[&str]); 7] = [
        ("A20", "1", "Earthworks and corridor", 20, &[afrobim_corridor::CORRIDOR_CATEGORY]),
        ("A30", "2", "Drainage and culverts", 12, &[afrobim_drainage::DRAIN_CATEGORY, afrobim_drainage::CULVERT_CATEGORY]),
        ("A40", "3", "Pavement layers", 15, &[afrobim_pavement::PAVEMENT_CATEGORY]),
        ("A50", "4", "Barrier and street lighting", 10, &[afrobim_restraint::CATEGORY, afrobim_lighting::CATEGORY]),
        ("A60", "5", "Signs, supports and gantry", 8, &[crate::signs_glue::CATEGORY, afrobim_supports::POST_CATEGORY, afrobim_supports::FOUNDATION_CATEGORY, afrobim_gantry::GANTRY_CATEGORY]),
        ("A70", "6", "Road markings", 4, &[afrobim_markings::CATEGORY]),
        ("A80", "7", "Completion", 0, &[]),
    ];
    for (id, wbs, name, duration, categories) in list {
        p.add_activity(Activity::new(id, wbs, name, duration));
        for c in categories {
            p.link_elements(id, rule(c), LinkAction::Construct);
        }
    }
    // Demo work breakdown: every id, code and grouping is made up. Two summary tasks, the second with
    // a sub-summary; the completion milestone stays at the top level.
    schedule_glue::add_summary(&mut p, "S1", "1", "Earthworks and drainage", None)?;
    schedule_glue::add_summary(&mut p, "S2", "2", "Pavement and furniture", None)?;
    schedule_glue::add_summary(&mut p, "S2A", "2.2", "Road furniture", Some("S2"))?;
    for (activity, code) in [("A20", "1.1"), ("A30", "1.2"), ("A40", "2.1"), ("A50", "2.2.1"), ("A60", "2.2.2"), ("A70", "2.3"), ("A80", "3")] {
        if let Some(a) = p.activity_mut(activity) {
            a.wbs = code.to_string();
        }
    }
    for (activity, summary) in [("A20", "S1"), ("A30", "S1"), ("A40", "S2"), ("A70", "S2"), ("A50", "S2A"), ("A60", "S2A")] {
        if let Some(a) = p.activity_mut(activity) {
            a.parent = Some(summary.to_string());
        }
    }
    // Demo constraints on the pavement layers: two of the engine's three kinds, with arbitrary dates.
    if let Some(a) = p.activity_mut("A40") {
        a.constraint = Some(afrobim_schedule::Constraint::StartNoEarlierThan(date(2026, 4, 1)));
        a.extra_constraints = vec![afrobim_schedule::Constraint::FinishNoLaterThan(date(2026, 6, 30))];
    }
    // Demo location ranges on the first imported alignment (MC10), as internal stations in metres,
    // with an arbitrary direction and dwell each. They are demo values only.
    let ranges: [(&str, f64, f64, WorkDirection, Option<u32>); 6] = [
        ("A20", 1000.0, 1700.0, WorkDirection::Increasing, Some(4)),
        ("A30", 1050.0, 1500.0, WorkDirection::Increasing, Some(3)),
        ("A40", 1000.0, 1700.0, WorkDirection::Increasing, Some(3)),
        ("A50", 1100.0, 1600.0, WorkDirection::Decreasing, Some(2)),
        ("A60", 1200.0, 1600.0, WorkDirection::Fixed, None),
        ("A70", 1000.0, 1700.0, WorkDirection::Increasing, Some(1)),
    ];
    for (activity, from, to, direction, dwell) in ranges {
        if let Some(a) = p.activity_mut(activity) {
            let mut r = LocationRange::new("MC10", from, to, direction);
            r.dwell_days = dwell;
            a.location = Some(r);
        }
    }
    // Arbitrary demo logic and lags.
    p.link("A20", "A30", LinkType::FS, 0);
    p.link("A20", "A40", LinkType::FS, 0);
    p.link("A30", "A40", LinkType::SS, 5);
    p.link("A40", "A50", LinkType::FS, 0);
    p.link("A50", "A60", LinkType::FS, 0);
    p.link("A40", "A70", LinkType::FS, 0);
    p.link("A60", "A80", LinkType::FS, 0);
    p.link("A70", "A80", LinkType::FS, 0);
    // A baseline, then a longer drainage activity and some progress (arbitrary demo values), so
    // the variance table, the baseline bars and the data date have something to show.
    let s = afrobim_schedule::schedule(&p).map_err(|e| e.to_string())?;
    p.set_baseline("Demo baseline", &s);
    if let Some(a) = p.activity_mut("A30") {
        a.duration = 14;
    }
    p.data_date = Some(date(2026, 3, 21));
    if let Some(a) = p.activity_mut("A20") {
        a.progress.percent_complete = 80.0;
        a.progress.actual_start = Some(date(2026, 3, 2));
    }
    schedule_glue::store(doc, None, &p).map(|_| ())
}

fn add_estimate(doc: &mut Document) -> Result<(), String> {
    let (id, mut est) = cost_glue::create(doc, "DEMO, not real rates")?;
    est.name = "DEMO, not real rates".into();
    est.bill = Bill::neutral(4);
    // (code, section, description, unit, line category, arbitrary demo rate in UGX)
    let items: [(&str, &str, &str, &str, &str, &str); 12] = [
        ("1.1", "1", "Earthworks, cut", "m3", "earthworks.cut", "4"),
        ("1.2", "1", "Earthworks, fill", "m3", "earthworks.fill", "6"),
        ("2.1", "2", "Pavement layers (compacted volume)", "m3", "pavement.layer_volume", "90"),
        ("3.1", "3", "Side drain, length", "m", "drain.length", "30"),
        ("3.2", "3", "Pipe culvert, length", "m", "culvert.length", "250"),
        ("4.1", "4", "Safety barrier, length", "m", "barrier.length", "55"),
        ("4.2", "4", "Lighting columns", "nr", "lighting.column", "2000"),
        ("4.3", "4", "Road marking, painted length", "m", "marking.painted_length", "3"),
        ("4.4", "4", "Sign faces", "nr", "sign.face", "120"),
        ("4.5", "4", "Sign posts", "nr", "post.count", "80"),
        ("4.6", "4", "Sign foundations", "nr", "foundation.count", "150"),
        ("4.7", "4", "Gantries", "nr", "gantry.count", "5000"),
    ];
    for (code, section, description, unit, category, rate) in items {
        est.bill.items.push(Item::measured(code, section, description, unit));
        est.bill.rules.push(MappingRule::new(&format!("demo {category}"), code, category, vec![]));
        if let Ok(rate) = Decimal::parse(rate) {
            est.rates.set(Rate { item_code: code.into(), description: description.into(), unit: unit.into(), rate, currency: "UGX".into(), source_note: "demo value, not a real rate".into(), date: None });
        }
    }
    // A manual item with an arbitrary demo quantity and no rate, to show an item entered by hand
    // and an unpriced item.
    est.bill.items.push(Item::manual("4.8", "4", "Provisional item entered by hand (left unpriced on purpose)", "nr", Decimal::from_int(1), "demo: a quantity entered by hand"));
    // An arbitrary demo percentage on the bill total.
    est.additions.push(Addition {
        id: "D1".into(),
        name: "Demo addition (arbitrary 10 %)".into(),
        kind: AdditionKind::Percentage { percent: Decimal::parse("10").map_err(|e| e.to_string())?, base: AdditionBase::BillTotal },
    });
    // Arbitrary demo windows, matching the demo programme's order.
    let windows: [(&str, (i32, u32, u32), (i32, u32, u32)); 6] =
        [("1.1", (2026, 3, 2), (2026, 3, 27)), ("1.2", (2026, 3, 2), (2026, 3, 27)), ("2.1", (2026, 3, 30), (2026, 4, 24)), ("3.1", (2026, 3, 16), (2026, 4, 4)), ("4.1", (2026, 4, 27), (2026, 5, 9)), ("4.3", (2026, 5, 11), (2026, 5, 16))];
    for (code, s, f) in windows {
        est.phasing.push(Phasing {
            target: PhasingTarget::Item(code.into()),
            start: afrobim_cost::Date::new(s.0, s.1 as u8, s.2 as u8).map_err(|e| e.to_string())?,
            finish: afrobim_cost::Date::new(f.0, f.1 as u8, f.2 as u8).map_err(|e| e.to_string())?,
            distribution: Default::default(),
        });
    }
    cost_glue::store(doc, id, &est, None, 0)?;
    // Arbitrary demo payment terms: a 10 percent retention and a lump deduction of 1000, not terms
    // of any contract.
    let terms = afrobim_cost::PaymentTerms {
        rules: vec![
            afrobim_cost::PaymentRule { name: "Demo retention (arbitrary 10 %)".into(), kind: afrobim_cost::PaymentRuleKind::Retention { percent: Decimal::from_int(10), cap: None } },
            afrobim_cost::PaymentRule { name: "Demo lump deduction (arbitrary)".into(), kind: afrobim_cost::PaymentRuleKind::Lump { amount: Decimal::from_int(1000), effect: afrobim_cost::Effect::Deduct } },
        ],
    };
    cost_glue::set_property(doc, id, cost_glue::PROP_TERMS, &terms);
    Ok(())
}

/// Check inputs for two demo elements, so that CHECKALL has results to show beside the elements
/// that still need input. **The numbers are arbitrary demo values**: a speed limit of 100 km/h for
/// the barrier run, and, for the street lighting, nothing at all (an empty form), which makes the
/// engine list what it needs and shows its warning about the superseded edition.
fn add_check_inputs(doc: &mut Document) {
    use crate::check_inputs::{save, KEY_LIGHTING, KEY_RESTRAINT};
    let find = |doc: &Document, category: &str| doc.elements().iter().find(|e| e.category == category).map(|e| e.id);
    if let Some(id) = find(doc, afrobim_restraint::CATEGORY) {
        save(doc, id, KEY_RESTRAINT, &crate::road_glue::SpeedInput { value: 100.0, mph: false });
    }
    if let Some(id) = find(doc, afrobim_lighting::CATEGORY) {
        save(doc, id, KEY_LIGHTING, &crate::lighting_inputs::Form::default());
    }
}

/// A few made-up site records and actual costs. **Every number is an arbitrary demo value**: the
/// dates, the percents, the quantity and its target, the amounts and the name of the recorder
/// belong to no project. The EAC formula is an arbitrary demo choice, not advice; a new document
/// starts with none chosen.
fn add_site_progress(doc: &mut Document) -> Result<(), String> {
    use crate::construction_glue::{self as glue, EacKind, Measure, RecordEntry, Settings};
    use afrobim_progress::ProgressBook;
    let find = |doc: &Document, category: &str| doc.elements().iter().find(|e| e.category == category).map(|e| e.id);
    let mut book = ProgressBook::new();
    let entry = |date: &str, measure: Measure| RecordEntry { date: date.into(), measure, recorded_by: "Demo recorder (made up)".into(), note: "demo record, arbitrary value".into(), files: vec![], correction: None };
    let percent = |p: &str| Measure::Percent(p.into());
    // The corridor: 30 %, 60 % and 80 % on three arbitrary dates (the demo programme has the
    // earthworks activity at 80 % on the data date).
    if let Some(id) = find(doc, afrobim_corridor::CORRIDOR_CATEGORY) {
        for (date, p) in [("2026-03-09", "30"), ("2026-03-16", "60"), ("2026-03-20", "80")] {
            glue::add_records(&mut book.log, &[id], &entry(date, percent(p)))?;
        }
    }
    // The side drain: 25 % and then 50 % (arbitrary).
    if let Some(id) = find(doc, afrobim_drainage::DRAIN_CATEGORY) {
        for (date, p) in [("2026-03-18", "25"), ("2026-03-20", "50")] {
            glue::add_records(&mut book.log, &[id], &entry(date, percent(p)))?;
        }
    }
    // The culvert, by quantity: a target of 10 m of barrel and 4 m laid (arbitrary).
    if let Some(id) = find(doc, afrobim_drainage::CULVERT_CATEGORY) {
        book.log.set_target(id, afrobim_cost::Decimal::from_int(10), "m").map_err(|e| e.to_string())?;
        glue::add_records(&mut book.log, &[id], &entry("2026-03-20", Measure::Quantity { measured: "4".into(), unit: "m".into() }))?;
    }
    // Arbitrary actual costs: two against the earthworks activity, one against drainage and one
    // for the project as a whole.
    for (date, activity, amount, note) in [
        ("2026-03-13", Some("A20"), "70000", "demo cost, arbitrary"),
        ("2026-03-20", Some("A20"), "45000", "demo cost, arbitrary"),
        ("2026-03-20", Some("A30"), "5000", "demo cost, arbitrary"),
        ("2026-03-20", None, "4000", "demo cost for the project, arbitrary"),
    ] {
        glue::add_cost(&mut book, date, activity, amount, note)?;
    }
    let settings = Settings { data_date: "2026-03-21".into(), budget_from_estimate: true, eac: Some(EacKind::AcPlusRemainingAtBudget), look_ahead_days: "14".into(), ..Settings::default() };
    glue::store(doc, None, &book, &settings).map(|_| ())
}

fn add_clash_tests(doc: &mut Document) -> Result<(), String> {
    let mut co = Coordination::default();
    let supports = Selection { categories: vec![afrobim_supports::POST_CATEGORY.to_string(), afrobim_supports::FOUNDATION_CATEGORY.to_string()], ..Selection::default() };
    co.tests.push(ClashTest::hard("Demo hard: sign supports against the corridor", supports, Selection::category(afrobim_corridor::CORRIDOR_CATEGORY)));
    // An arbitrary demo clearance of 3000 mm, chosen so the demo shows a clash. It is not a requirement.
    co.tests.push(ClashTest::clearance("Demo clearance: lighting against barrier", Selection::category(afrobim_lighting::CATEGORY), Selection::category(afrobim_restraint::CATEGORY), 3000.0));
    crate::coordination_glue::store(doc, &co).map(|_| ())
}

/// One demo issue with a viewpoint, so the Issues window has something to show. **Everything in it
/// is made up for the demo**: the title and comment say so, and the 6 m viewing distance is an
/// arbitrary choice. It is raised from the first clash when there is one.
pub fn add_issue(co: &mut Coordination) -> Option<String> {
    let author = "Demo author (made up)";
    let now = crate::issues_ui::now_unix();
    let guid = match co.results.clashes.first().map(|c| c.id.clone()) {
        Some(id) => co.raise_issue_from_clash(&id, author, now)?,
        None => co.new_issue("DEMO issue", author, now).guid.clone(),
    };
    let issue = co.issue_mut(&guid)?;
    issue.title = format!("DEMO (made up): {}", issue.title);
    issue.description = format!("Demo issue made for --demo, not a review finding. {}", issue.description);
    issue.assignee = "Demo assignee (made up)".into();
    issue.add_comment(author, now, "Demo comment. Nothing here is a real finding.");
    if issue.location.is_none() {
        issue.location = Some([0.0, 0.0, 0.0]);
    }
    issue.add_location_viewpoint([-1.0, -1.0, 0.6], 6000.0, afrobim_coordination::Projection::Perspective { fov_deg: crate::view3d::FOV_DEG });
    Some(guid)
}
