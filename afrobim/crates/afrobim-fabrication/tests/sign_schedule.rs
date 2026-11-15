//! The sign schedule: template, export, mapping, plan and apply, against the real UK rule pack's
//! catalog and an alignment with station equations. Hand-worked placements are noted where used.

use std::path::{Path, PathBuf};

use afrobim_alignment::{Alignment, AlignmentElement, StationEquation, StationEquations};
use afrobim_fabrication::sign_schedule::*;
use afrobim_kernel::{Document, ElementId, Pt};
use afrobim_signs::{CatalogEntry, SignDefinition, SignEngine, SignKind};

fn repo_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../..")
}

fn catalog() -> Vec<CatalogEntry> {
    SignEngine::load(&repo_root().join("packs/UK-TSM-2019.json"), &repo_root().join("fonts")).unwrap().catalog().to_vec()
}

fn stack_code(cat: &[CatalogEntry]) -> String {
    cat.iter().find(|e| e.kind == SignKind::StackType).expect("the UK pack has a stack-type sign").key.clone()
}

fn warning_code(cat: &[CatalogEntry]) -> String {
    cat.iter().find(|e| e.kind == SignKind::WarningSign).expect("the UK pack has a warning sign").key.clone()
}

/// A straight alignment east from (0, 0), 2000 m long, with no station equations.
fn plain() -> Alignment {
    Alignment {
        name: "Main Road".into(),
        start_station_m: 0.0,
        elements: vec![AlignmentElement::line(0.0, 0.0, 0.0, 2000.0)],
        profile: None,
        superelevation: None,
        station_equations: StationEquations::default(),
    }
}

/// The same, with the displayed station going back to 500 at internal 1000: displayed 500 to 1000
/// occurs twice (internal 500 to 1000, region 0, and internal 1000 to 1500, region 1).
fn equated() -> Alignment {
    let mut a = plain();
    a.name = "Equated".into();
    a.station_equations = StationEquations::new(vec![StationEquation::new(1000.0, 500.0)]).unwrap();
    a
}

struct Fake {
    measured: usize,
    placed: Vec<ElementId>,
}

impl SignBuilder for Fake {
    fn measure(&mut self, d: &SignDefinition) -> Result<(f64, f64), String> {
        if d.code == "REFUSE" {
            return Err("the engine refused it".into());
        }
        self.measured += 1;
        Ok((1000.0, 800.0))
    }
    fn placed(&mut self, _doc: &mut Document, id: ElementId) {
        self.placed.push(id);
    }
}

fn fake() -> Fake {
    Fake { measured: 0, placed: vec![] }
}

fn plan_of(text: &str, cat: &[CatalogEntry], aligns: &[Alignment], existing: &[ExistingSign]) -> ImportPlan {
    let table = parse_csv(text).unwrap();
    let mapping = propose_mapping(&table.headers).mapping;
    let ctx = PlanContext { catalog: cat, alignments: aligns, existing, traffic: afrobim_alignment::TrafficSide::Left };
    plan_import(&table, &mapping, &ctx).unwrap()
}

fn issues_of(plan: &ImportPlan, row: usize) -> Vec<IssueKind> {
    let r = plan.rows.iter().find(|r| r.row == row).unwrap_or_else(|| panic!("no row {row}"));
    match &r.outcome {
        RowOutcome::Invalid(i) => i.iter().map(|i| i.kind).collect(),
        RowOutcome::Valid(_) => vec![],
    }
}

const HEAD: &str = "mark,code,description,alignment,station,region,side,offset_m,mounting_height_mm,size_mm,legend,rotation_deg,notes\n";

#[test]
fn the_template_is_a_header_and_a_comment_that_an_import_skips() {
    let t = template_csv(&CsvStyle::default());
    let table = parse_csv(&t).unwrap();
    assert!(table.rows.is_empty());
    assert_eq!(table.comment_lines, [2]);
    let p = propose_mapping(&table.headers);
    assert!(p.missing_required.is_empty() && p.unmapped_headers.is_empty() && p.duplicate_headers.is_empty() && p.unknown_params.is_empty());
    for f in Field::FIXED {
        assert!(p.mapping.has(&f));
    }
    assert!(p.mapping.has(&Field::Param("speed_kph".into())));
    // Semicolon and BOM variants read the same.
    let semi = template_csv(&CsvStyle { delimiter: ';', bom: true });
    assert!(semi.starts_with('\u{feff}'));
    let t2 = parse_csv(&semi).unwrap();
    assert_eq!((t2.delimiter, t2.headers.clone()), (';', table.headers.clone()));
    assert_eq!(t2.comment_lines, [2]);
    let plan = plan_of(&t, &catalog(), &[plain()], &[]);
    assert!(plan.rows.is_empty() && plan.comment_lines == [2]);
}

#[test]
fn import_apply_export_import_round_trips() {
    let cat = catalog();
    let (stack, warn) = (stack_code(&cat), warning_code(&cat));
    let aligns = [plain(), equated()];
    let csv = format!(
        "{HEAD}\
         ADS-1,{stack},Advance direction,Main Road,1+234.500,,right,3.25,2000,,\"Kasese@Ahead#12 km | Masheruka@Right\",,Check on site\n\
         ADS-2,{stack},,Main Road,0+500,,left,4,,,\"A@Left | B@Left | C@Ahead~A11\",,\n\
         W-1,{warn},A warning,Equated,0+700,1,right,3,2.4 m,,,45.5,\n"
    );
    // The warning row may or may not need a legend, depending on the sign: skip it if it is not a stack.
    let plan = plan_of(&csv, &cat, &aligns, &[]);
    assert_eq!(plan.rows.len(), 3);
    assert!(matches!(plan.rows[0].outcome, RowOutcome::Valid(_)), "{:?}", plan.error_lines());
    assert!(matches!(plan.rows[1].outcome, RowOutcome::Valid(_)), "{:?}", plan.error_lines());
    assert!(matches!(plan.rows[2].outcome, RowOutcome::Valid(_)), "{:?}", plan.error_lines());

    // Hand-worked placement of ADS-1: east alignment, station 1234.5 m, 3.25 m to the right is
    // 3.25 m south: x = 1 234 500 mm, y = -3250 mm; right side with traffic on the left faces
    // the traffic coming down-station, rotation 90 degrees.
    let a1 = plan.valid().next().unwrap();
    assert!((a1.site.x - 1_234_500.0).abs() < 1e-6 && (a1.site.y + 3250.0).abs() < 1e-6, "{:?}", a1.site);
    assert!((a1.site.rotation_deg - 90.0).abs() < 1e-9);
    assert_eq!(a1.definition.destinations.len(), 2);
    assert_eq!(a1.properties["Mounting height (mm)"], "2000");
    assert_eq!(a1.properties["Description"], "Advance direction");
    assert_eq!(a1.properties["Notes"], "Check on site");
    assert_eq!(a1.definition.code, "ADS-1");
    // W-1: displayed 700 in region 1 of Equated is internal 1200; 2.4 m is 2400 mm; rotation override.
    let w = plan.valid().nth(2).unwrap();
    assert_eq!(w.placement.internal_station_m, 1200.0);
    assert_eq!(w.properties["Mounting height (mm)"], "2400");
    assert_eq!(w.site.rotation_deg, 45.5);
    assert!(w.warnings.iter().any(|m| m.contains("rotation")));

    // Apply.
    let mut doc = Document::default();
    let mut b = fake();
    let report = apply_plan(&mut doc, &plan, &mut b, DuplicatePolicy::Skip);
    assert_eq!((report.created.len(), report.updated.len(), report.skipped.len(), report.failed.len()), (3, 0, 0, 0), "{report:?}");
    assert_eq!(b.placed.len(), 3);
    let e = doc.find(report.created[0].1).unwrap();
    assert_eq!((e.name.as_str(), e.category.as_str(), e.layer.as_str(), e.width, e.height), ("ADS-1", "sign", "Signs", 1000.0, 800.0));
    assert_eq!(e.site.as_ref().unwrap().alignment.as_deref(), Some("Main Road"));

    // Export, then import into a fresh document: the plans agree.
    let ex = export_csv(&doc, &aligns, &cat, &CsvStyle::default());
    assert_eq!(ex.signs, 3);
    assert!(ex.warnings.is_empty(), "{:?}", ex.warnings);
    let plan2 = plan_of(&ex.csv, &cat, &aligns, &[]);
    assert_eq!(plan2.rows.len(), 3, "{:?}", plan2.error_lines());
    for (a, b) in plan.valid().zip(plan2.valid()) {
        assert_eq!(a.mark, b.mark);
        assert_eq!(a.definition, b.definition);
        assert_eq!(a.properties, b.properties);
        assert_eq!(a.placement.internal_station_m, b.placement.internal_station_m);
        assert!((a.site.x - b.site.x).abs() < 1e-6 && (a.site.y - b.site.y).abs() < 1e-6);
        assert!((a.site.rotation_deg - b.site.rotation_deg).abs() < 1e-6);
    }
    // A second export of the re-imported document is the same text.
    let mut doc2 = Document::default();
    apply_plan(&mut doc2, &plan2, &mut fake(), DuplicatePolicy::Skip);
    assert_eq!(export_csv(&doc2, &aligns, &cat, &CsvStyle::default()).csv, ex.csv);
    // Semicolons and a BOM round-trip too.
    let semi = export_csv(&doc, &aligns, &cat, &CsvStyle { delimiter: ';', bom: true });
    let plan3 = plan_of(&semi.csv, &cat, &aligns, &[]);
    assert_eq!(plan3.valid_count(), 3);
}

#[test]
fn a_foreign_layout_maps_by_synonym_and_by_an_explicit_mapping() {
    let cat = catalog();
    let stack = stack_code(&cat);
    let text = format!(
        "\u{feff}Sign No.;Diagram;Road Name;Chainage;L/R;Offset (m);Mounting Height (m);Remarks;Legend Text;Contractor\n\
         S1;{stack};main road;2+000;R;3.5;2.2;note;Kasese@Ahead;ACME\n"
    );
    let table = parse_csv(&text).unwrap();
    assert_eq!(table.delimiter, ';');
    let p = propose_mapping(&table.headers);
    assert!(p.missing_required.is_empty(), "{:?}", p.missing_required);
    assert_eq!(p.unmapped_headers, ["Contractor"]);
    let aligns = [plain()];
    let ctx = PlanContext { catalog: &cat, alignments: &aligns, existing: &[], traffic: afrobim_alignment::TrafficSide::Left };
    let plan = plan_import(&table, &p.mapping, &ctx).unwrap();
    assert_eq!(plan.unread_headers, ["Contractor"]);
    let s = plan.valid().next().expect("valid row");
    // Offset (m) read as metres, Mounting Height (m) as metres: 2.2 m is 2200 mm.
    assert_eq!(s.properties["Mounting height (mm)"], "2200");
    assert_eq!(s.placement.alignment, "Main Road");
    assert_eq!(s.placement.offset_m, 3.5);
    assert_eq!(s.placement.station_m, 2000.0);

    // An explicit mapping for headers the synonyms do not cover, saved and loaded as JSON.
    let weird = format!("Wibble;Hmm;Where;Along;Which side;How far;What\nS2;{stack};Main Road;1+000;left;2;Kasese@Ahead\n");
    let t2 = parse_csv(&weird).unwrap();
    let guess = propose_mapping(&t2.headers);
    assert_eq!(guess.missing_required.len(), 6);
    assert!(matches!(plan_import(&t2, &guess.mapping, &ctx), Err(PlanError::MissingColumns(m)) if m.len() == 6));
    let mut mapping = ColumnMapping::default();
    for (h, f) in [
        ("Wibble", Field::Mark),
        ("Hmm", Field::Code),
        ("Where", Field::Alignment),
        ("Along", Field::Station),
        ("Which side", Field::Side),
        ("How far", Field::Offset),
        ("What", Field::Legend),
    ] {
        mapping.set(h, f, None);
    }
    let saved = mapping.to_json();
    let loaded = ColumnMapping::from_json(&saved).unwrap();
    assert_eq!(loaded, mapping);
    let plan2 = plan_import(&t2, &loaded, &ctx).unwrap();
    assert_eq!(plan2.valid_count(), 1, "{:?}", plan2.error_lines());
    assert_eq!(plan2.valid().next().unwrap().placement.offset_m, 2.0);
}

#[test]
fn every_error_class_is_reported_with_its_row_number() {
    let cat = catalog();
    let (stack, warn) = (stack_code(&cat), warning_code(&cat));
    let warn_takes_legend = cat.iter().find(|e| e.key == warn).map(|e| e.kind).is_some_and(takes_legend);
    let aligns = [plain(), equated()];
    let existing = [ExistingSign { id: ElementId(7), mark: "EXISTS".into() }];
    let csv = format!(
        "{HEAD}\
         OK,{stack},,Main Road,1+000,,left,2,,,Kasese@Ahead,,\n\
         BADCODE,NOPE-1,,Main Road,1+000,,left,2,,,Kasese@Ahead,,\n\
         NOALIGN,{stack},,Nowhere,1+000,,left,2,,,Kasese@Ahead,,\n\
         FAR,{stack},,Main Road,9+000,,left,2,,,Kasese@Ahead,,\n\
         AMBIG,{stack},,Equated,0+700,,left,2,,,Kasese@Ahead,,\n\
         BADREGION,{stack},,Equated,0+700,7,left,2,,,Kasese@Ahead,,\n\
         NOLEGEND,{stack},,Main Road,1+000,,left,2,,,,,\n\
         BADNUM,{stack},,Main Road,1+000,,left,2 metres,,,Kasese@Ahead,,\n\
         BADSTA,{stack},,Main Road,1+2000,,left,2,,,Kasese@Ahead,,\n\
         BADSIDE,{stack},,Main Road,1+000,,middle,2,,,Kasese@Ahead,,\n\
         NEGOFF,{stack},,Main Road,1+000,,left,-2,,,Kasese@Ahead,,\n\
         BADLEG,{stack},,Main Road,1+000,,left,2,,,Kasese@Up,,\n\
         ,{stack},,Main Road,1+000,,left,2,,,Kasese@Ahead,,\n\
         OK,{stack},,Main Road,1+100,,left,2,,,Kasese@Ahead,,\n\
         EXISTS,{stack},,Main Road,1+200,,left,2,,,Kasese@Ahead,,\n\
         EXTRA,{stack},,Main Road,1+300,,left,2,,,Kasese@Ahead,,,surplus\n"
    );
    let plan = plan_of(&csv, &cat, &aligns, &existing);
    // Header is line 1; the first record is line 2.
    assert_eq!(plan.rows.len(), 16);
    assert!(issues_of(&plan, 2).is_empty());
    assert_eq!(issues_of(&plan, 3), [IssueKind::UnknownCode]);
    assert_eq!(issues_of(&plan, 4), [IssueKind::AlignmentNotFound]);
    assert_eq!(issues_of(&plan, 5), [IssueKind::StationOutOfRange]);
    assert_eq!(issues_of(&plan, 6), [IssueKind::StationAmbiguous]);
    assert_eq!(issues_of(&plan, 7), [IssueKind::RegionInvalid]);
    assert_eq!(issues_of(&plan, 8), [IssueKind::MissingRequiredParam]);
    assert_eq!(issues_of(&plan, 9), [IssueKind::BadNumber]);
    assert_eq!(issues_of(&plan, 10), [IssueKind::BadStation]);
    assert_eq!(issues_of(&plan, 11), [IssueKind::BadSide]);
    assert_eq!(issues_of(&plan, 12), [IssueKind::BadNumber]);
    assert_eq!(issues_of(&plan, 13), [IssueKind::BadValue]);
    assert_eq!(issues_of(&plan, 14), [IssueKind::MissingValue]);
    assert_eq!(issues_of(&plan, 15), [IssueKind::DuplicateMarkInFile]);
    assert!(issues_of(&plan, 16).is_empty());
    assert_eq!(issues_of(&plan, 17), [IssueKind::ExtraCells]);
    // The ambiguity message names the regions.
    let amb = plan.invalid().find(|(r, _)| r.row == 6).unwrap().1;
    assert!(amb[0].message.contains("regions 0, 1"), "{}", amb[0].message);
    // Every record is accounted for, and the report lines carry row numbers.
    assert_eq!(plan.valid_count() + plan.invalid_count(), 16);
    assert!(plan.error_lines().iter().all(|l| l.starts_with("row ")));
    // The duplicate of an existing mark is valid and flagged.
    let dup: Vec<&PlannedSign> = plan.duplicates().collect();
    assert_eq!((dup.len(), dup[0].mark.as_str(), dup[0].existing), (1, "EXISTS", Some(ElementId(7))));
    // A legend on a sign that has none, or a made-up parameter, is refused rather than dropped.
    let more = format!(
        "{HEAD}L,{warn},,Main Road,1+000,,left,2,,,Kasese@Ahead,,\n"
    );
    let p2 = plan_of(&more, &cat, &aligns, &[]);
    if !warn_takes_legend {
        assert_eq!(issues_of(&p2, 2), [IssueKind::LegendNotAccepted]);
    }
    let param = format!("mark,code,alignment,station,side,offset,legend,param:colour,param:speed_kph\nP,{stack},Main Road,1+000,left,2,A@Ahead,red,fast\n");
    let p3 = plan_of(&param, &cat, &aligns, &[]);
    assert_eq!(issues_of(&p3, 2), [IssueKind::UnknownParam, IssueKind::BadValue]);
}

#[test]
fn a_parameter_column_sets_the_definition() {
    let cat = catalog();
    let stack = stack_code(&cat);
    let text = format!("mark,code,alignment,station,side,offset,legend,param:speed_kph,param:scheme\nP,{stack},Main Road,1+000,left,2,A@Ahead,80,Primary\n");
    let plan = plan_of(&text, &cat, &[plain()], &[]);
    let p = plan.valid().next().expect("valid");
    assert_eq!(p.definition.speed_kph, 80);
    assert_eq!(p.definition.scheme, afrobim_signs::ColourScheme::Primary);
}

#[test]
fn station_formats_name_the_same_place() {
    let cat = catalog();
    let stack = stack_code(&cat);
    let rows = ["1+234.567", "1234.567", "1234.567 m", " 1+234.567 "];
    let csv: String = rows.iter().enumerate().map(|(i, s)| format!("S{i},{stack},Main Road,\"{s}\",left,2,A@Ahead\n")).collect();
    let plan = plan_of(&format!("mark,code,alignment,station,side,offset,legend\n{csv}"), &cat, &[plain()], &[]);
    let xs: Vec<f64> = plan.valid().map(|p| p.site.x).collect();
    assert_eq!(xs.len(), 4, "{:?}", plan.error_lines());
    assert!(xs.iter().all(|x| (x - 1_234_567.0).abs() < 1e-6), "{xs:?}");
}

#[test]
fn a_tab_separated_file_with_crlf_reads() {
    let cat = catalog();
    let stack = stack_code(&cat);
    let text = format!("mark\tcode\talignment\tstation\tside\toffset\tlegend\r\nT1\t{stack}\tMain Road\t0+100\tleft\t2\t\"A@Ahead | B@Left\"\r\n");
    let plan = plan_of(&text, &cat, &[plain()], &[]);
    assert_eq!(plan.delimiter, '\t');
    assert_eq!(plan.valid_count(), 1, "{:?}", plan.error_lines());
}

#[test]
fn duplicates_of_an_existing_mark_are_skipped_or_updated() {
    let cat = catalog();
    let stack = stack_code(&cat);
    let aligns = [plain()];
    let first = format!("{HEAD}D1,{stack},old,Main Road,1+000,,left,2,1500,,A@Ahead,,keep me\n");
    let mut doc = Document::default();
    let plan = plan_of(&first, &cat, &aligns, &[]);
    apply_plan(&mut doc, &plan, &mut fake(), DuplicatePolicy::Skip);
    let id = doc.elements().iter().find(|e| e.name == "D1").unwrap().id;
    let origin_before = doc.find(id).unwrap().origin;
    doc.find_mut(id).unwrap().properties.insert("Other".into(), "stays".into());
    assert_eq!(doc.find(id).unwrap().origin, Pt::new(0.0, 0.0));

    let again = format!("{HEAD}d1,{stack},new,Main Road,1+500,,right,4,,,B@Left,,\n");
    let plan = plan_of(&again, &cat, &aligns, &existing_signs(&doc));
    assert_eq!(plan.duplicates().count(), 1);
    // Skip leaves it alone and says so.
    let r = apply_plan(&mut doc, &plan, &mut fake(), DuplicatePolicy::Skip);
    assert_eq!((r.created.len(), r.updated.len(), r.skipped.len()), (0, 0, 1));
    assert_eq!(doc.find(id).unwrap().properties["Description"], "old");
    // Update replaces the definition, placement and schedule properties; the rest stays.
    let r = apply_plan(&mut doc, &plan, &mut fake(), DuplicatePolicy::Update);
    assert_eq!((r.created.len(), r.updated.len(), r.skipped.len()), (0, 1, 0));
    let e = doc.find(id).unwrap();
    assert_eq!(e.properties["Description"], "new");
    assert!(!e.properties.contains_key("Notes") && !e.properties.contains_key("Mounting height (mm)"));
    assert_eq!(e.properties["Other"], "stays");
    assert_eq!(e.origin, origin_before);
    assert_eq!(e.site.as_ref().unwrap().station_m, Some(1500.0));
    let def: SignDefinition = serde_json::from_value(e.data.clone()).unwrap();
    assert_eq!(def.destinations[0].name, "B");
    assert_eq!(doc.elements().iter().filter(|e| e.category == "sign").count(), 1);
}

#[test]
fn invalid_rows_and_builder_failures_are_reported_not_applied() {
    let cat = catalog();
    let stack = stack_code(&cat);
    let csv = format!("{HEAD}REFUSE,{stack},,Main Road,1+000,,left,2,,,A@Ahead,,\nBAD,NOPE,,Main Road,1+000,,left,2,,,A@Ahead,,\n");
    let plan = plan_of(&csv, &cat, &[plain()], &[]);
    let mut doc = Document::default();
    let r = apply_plan(&mut doc, &plan, &mut fake(), DuplicatePolicy::Skip);
    assert_eq!((r.created.len(), r.failed.len(), r.skipped.len()), (0, 1, 1));
    assert!(r.failed[0].2.contains("refused") && r.skipped[0].2.contains("not valid"));
    assert!(doc.elements().is_empty());
}

#[test]
fn export_says_what_it_cannot_carry() {
    let cat = catalog();
    let mut doc = Document::default();
    // A sign with no catalog code, not placed, and one with a destination symbol.
    let mut def = SignDefinition::stack("X", 60, vec![afrobim_signs::Destination::new("Kasese", afrobim_signs::Direction::Ahead)]);
    def.destinations[0].symbol_id = Some("airport".into());
    doc.add("sign", "X-1", Pt::new(0.0, 0.0), 1.0, 1.0, "Signs", serde_json::to_value(&def).unwrap());
    doc.add("sign", "X-2", Pt::new(0.0, 0.0), 1.0, 1.0, "Signs", serde_json::json!({"nonsense": true}));
    let ex = export_csv(&doc, &[plain()], &cat, &CsvStyle::default());
    assert_eq!(ex.signs, 1);
    let all = ex.warnings.join("\n");
    assert!(all.contains("X-1") && all.contains("no catalog code"), "{all}");
    assert!(all.contains("not placed"), "{all}");
    assert!(all.contains("symbol of 'Kasese'"), "{all}");
    assert!(all.contains("X-2") && all.contains("cannot be read"), "{all}");
}

#[test]
fn a_file_with_a_stray_open_quote_is_an_error_not_a_guess() {
    assert!(matches!(parse_csv("a,b\n\"x,1\n"), Err(CsvError::UnterminatedQuote { line: 2 })));
}

#[test]
fn alignments_and_signs_are_read_from_a_document() {
    let mut doc = Document::default();
    let a = plain();
    doc.add(afrobim_alignment::CATEGORY, &a.name, Pt::new(0.0, 0.0), 1.0, 1.0, "Alignments", serde_json::to_value(&a).unwrap());
    doc.add("sign", "S", Pt::new(0.0, 0.0), 1.0, 1.0, "Signs", serde_json::json!({}));
    assert_eq!(alignments_in(&doc).len(), 1);
    assert_eq!(existing_signs(&doc).iter().map(|s| s.mark.as_str()).collect::<Vec<_>>(), ["S"]);
}
