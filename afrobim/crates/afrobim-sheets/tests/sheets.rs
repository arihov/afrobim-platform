mod common;

use std::io::Read;
use std::path::PathBuf;
use std::process::Command;

use afrobim_draw::Pt2;
use afrobim_kernel::Document;
use afrobim_sheets::*;
use common::{fixture, project};

fn page(size: PaperSize, o: Orientation) -> Page {
    Page::new(size, o)
}

/// A plan series on A3 at 1:1000, 300 m a sheet with 30 m overlap, over the test road.
fn plan_series() -> SheetSet {
    let mut view = PlanView::new("MC1", 0.0, 1.0, Scale(1000.0));
    view.tick_interval_m = 20.0;
    view.label_interval_m = 100.0;
    let template = SheetSpec::new("PLAN", page(PaperSize::A3, Orientation::Landscape), vec![ViewDef::full(ViewKind::Plan(view))]);
    let mut set = SheetSet::new("Test set");
    set.numbering = Numbering { prefix: "TR-PL-".into(), digits: 3, start: 1, step: 1, suffix: String::new() };
    set.sources.push(SheetSource::PlanSeries(PlanSeries { template, plan_view: 0, from_station_m: None, to_station_m: None, length_m: 300.0, overlap_m: 30.0 }));
    set
}

fn texts(sheet: &ComposedSheet) -> Vec<String> {
    sheet.texts().map(|t| t.text.clone()).collect()
}

#[test]
fn a_plan_series_covers_the_alignment_with_the_entered_overlap() {
    let f = fixture();
    let model = Model { doc: &f.doc, registry: &f.registry };
    let set = plan_series();
    let (expanded, warnings) = expand(&model, &set);
    assert!(warnings.is_empty(), "{warnings:?}");
    // 1500 m, 300 m sheets, starts step 270: 0, 270, 540, 810, 1080, 1350 (the last ends at 1500).
    assert_eq!(expanded.len(), 6);
    let ranges: Vec<(f64, f64)> = expanded
        .iter()
        .map(|e| match &e.spec.views[0].kind {
            ViewKind::Plan(p) => (p.from_station_m, p.to_station_m),
            _ => unreachable!(),
        })
        .collect();
    assert_eq!(ranges[0], (0.0, 300.0));
    assert_eq!(ranges[5], (1350.0, 1500.0));
    for w in ranges.windows(2) {
        assert!((w[0].1 - w[1].0 - 30.0).abs() < 1e-9, "overlap {:?}", w);
    }
    let numbers: Vec<&str> = expanded.iter().map(|e| e.number.as_str()).collect();
    assert_eq!(numbers, ["TR-PL-001", "TR-PL-002", "TR-PL-003", "TR-PL-004", "TR-PL-005", "TR-PL-006"]);
    // Match lines name the neighbours.
    let ViewKind::Plan(second) = &expanded[1].spec.views[0].kind else { unreachable!() };
    assert_eq!(second.match_lines.len(), 2);
    assert_eq!(second.match_lines[0].text, "CONTINUED ON SHEET TR-PL-001");
    assert_eq!(second.match_lines[1].text, "CONTINUED ON SHEET TR-PL-003");
    assert_eq!(second.match_lines[0].station_m, 285.0);
    // The title says which stations; displayed stations are used (0+000 to 0+300).
    assert_eq!(expanded[0].spec.title, "PLAN STA 0+000 TO 0+300");
    // After the equation at internal 1000 (displayed 1050) the last sheet reads 1+400 to 1+550.
    assert_eq!(expanded[5].spec.title, "PLAN STA 1+400 TO 1+550");
}

#[test]
fn composed_plan_sheets_carry_the_title_block_and_the_plan() {
    let f = fixture();
    let model = Model { doc: &f.doc, registry: &f.registry };
    let set = plan_series();
    let composed = compose_set(&model, &set);
    assert_eq!(composed.sheets.len(), 6);
    assert!(composed.warnings.is_empty(), "{:?}", composed.warnings);
    let s = &composed.sheets[2];
    assert_eq!((s.width_mm, s.height_mm), (PaperSize::A3.dimensions_mm().1, PaperSize::A3.dimensions_mm().0));
    let t = texts(s);
    for want in ["TR-PL-003", "3 of 6", "1:1000", "Test road, rehabilitation and upgrading", "TEST/2026/001", "B", "Alignment revised at cur...", "CONTINUED ON SHEET TR-PL-002", "CONTINUED ON SHEET TR-PL-004", "N"] {
        assert!(t.iter().any(|x| x == want), "missing {want:?} in {t:?}");
    }
    // Station labels at the label interval, displayed stations: 600 to 840 on this sheet.
    assert!(t.iter().any(|x| x == "0+600"));
    assert!(t.iter().any(|x| x == "0+800"));
    // Marks of the two markings, drawn with their own layer name and clipped to the viewport.
    assert!(s.items.iter().any(|i| i.layer() == afrobim_markings::LAYER));
    assert!(s.items.iter().any(|i| i.layer() == layers::PLAN_ALIGNMENT));
    let area = Page::default().frame_rect();
    for i in &s.items {
        if let Item::Stroke { points, layer, .. } = i {
            if layer == afrobim_markings::LAYER {
                for p in points {
                    assert!(p.x >= area.x0 - 1e-6 && p.x <= area.x1 + 1e-6 && p.y >= area.y0 - 1e-6 && p.y <= area.y1 + 1e-6, "{p:?}");
                }
            }
        }
    }
}

#[test]
fn the_station_ticks_use_displayed_stations_across_the_equation() {
    let f = fixture();
    let model = Model { doc: &f.doc, registry: &f.registry };
    let mut view = PlanView::new("MC1", 900.0, 1200.0, Scale(1000.0));
    view.label_interval_m = 100.0;
    let spec = SheetSpec::new("P", page(PaperSize::A3, Orientation::Landscape), vec![ViewDef::full(ViewKind::Plan(view))]);
    let mut set = SheetSet::new("s");
    set.sources.push(SheetSource::Single(spec));
    let composed = compose_set(&model, &set);
    let t = texts(&composed.sheets[0]);
    // Internal 900 reads 0+900; internal 1000 is the equation (displayed 1+050 ahead); internal
    // 1050 reads 1+100 and 1150 reads 1+200. No label 1+000 exists ahead of the equation.
    assert!(t.contains(&"0+900".to_string()), "{t:?}");
    assert!(t.contains(&"1+100".to_string()) && t.contains(&"1+200".to_string()), "{t:?}");
}

#[test]
fn a_profile_view_shows_levels_a_datum_and_a_data_band() {
    let f = fixture();
    let model = Model { doc: &f.doc, registry: &f.registry };
    let mut v = ProfileView::new("MC1", 0.0, 600.0, Scale(2000.0), 10.0);
    v.surface = Some("Ground".into());
    v.grid_level_interval_m = 2.0;
    let spec = SheetSpec::new("PROFILE", page(PaperSize::A3, Orientation::Landscape), vec![ViewDef::full(ViewKind::Profile(v))]);
    let mut set = SheetSet::new("s");
    set.sources.push(SheetSource::Single(spec));
    let composed = compose_set(&model, &set);
    assert!(composed.warnings.is_empty(), "{:?}", composed.warnings);
    let s = &composed.sheets[0];
    let t = texts(s);
    assert!(t.iter().any(|x| x.starts_with("PROFILE  H 1:2000  V 1:200  DATUM")), "{t:?}");
    for want in ["STATION", "DESIGN LEVEL", "GROUND LEVEL", "0+000", "0+500", "1100.000"] {
        assert!(t.iter().any(|x| x == want), "missing {want}: {t:?}");
    }
    assert!(s.items.iter().any(|i| i.layer() == layers::PROFILE_GROUND));
    // Hand-worked: the design level at station 0 is the first PVI, 1100.000, and at 500 on the
    // grade between the PVIs at 400 (1106, curve 120 m: 340 to 460) and 900 (1098, curve 160 m: 820
    // to 980): 1106 + (500 - 400) * (1098 - 1106) / 500 = 1104.4.
    assert!(t.iter().any(|x| x == "1104.400"), "{t:?}");
}

#[test]
fn a_profile_that_does_not_fit_says_so() {
    let f = fixture();
    let model = Model { doc: &f.doc, registry: &f.registry };
    let v = ProfileView::new("MC1", 0.0, 1500.0, Scale(1000.0), 10.0); // 1500 mm long at 1:1000
    let mut set = SheetSet::new("s");
    set.sources.push(SheetSource::Single(SheetSpec::new("P", Page::default(), vec![ViewDef::full(ViewKind::Profile(v))])));
    let composed = compose_set(&model, &set);
    assert!(composed.warnings.iter().any(|w| w.contains("the end is cut off")), "{:?}", composed.warnings);
}

#[test]
fn sections_are_arranged_in_a_grid_with_their_labels() {
    let f = fixture();
    let model = Model { doc: &f.doc, registry: &f.registry };
    let mut v = SectionsView::new("Road", Scale(200.0), 3);
    v.from_station_m = Some(400.0);
    v.to_station_m = Some(550.0);
    v.vertical_exaggeration = 1.0;
    let spec = SheetSpec::new("SECTIONS", page(PaperSize::A3, Orientation::Landscape), vec![ViewDef::full(ViewKind::Sections(v))]);
    let mut set = SheetSet::new("s");
    set.sources.push(SheetSource::Single(spec));
    let composed = compose_set(&model, &set);
    let t = texts(&composed.sheets[0]);
    // Sections every 50 m between 400 and 550 (and at the curve starts): at least 4 stations.
    let titles = t.iter().filter(|x| x.starts_with("Station ")).count();
    assert!(titles >= 4, "{t:?}");
    assert!(t.iter().any(|x| x.starts_with("Cut ") || x.contains("daylight")));
    assert!(t.iter().any(|x| x.starts_with("CROSS-SECTIONS  1:200")));
}

#[test]
fn a_long_table_continues_over_sheets_with_wrapped_cells_and_a_repeated_header() {
    let f = fixture();
    let model = Model { doc: &f.doc, registry: &f.registry };
    let mut csv = String::from("Item,Description,Unit,Quantity,Rate,Amount\n");
    for i in 1..=80 {
        csv.push_str(&format!(
            "B{i:03},\"Granular sub-base of crushed stone placed and compacted in layers, tested to the specified density, item {i}\",m3,{}.5,{}.25,{}.0\n",
            100 + i,
            20 + i % 7,
            (100 + i) * 20
        ));
    }
    let table = priced_bill_table(&csv);
    assert_eq!(table.rows.len(), 80);
    assert_eq!(table.align[3], Align::Right);
    let spec = SheetSpec::new("BILL OF QUANTITIES", page(PaperSize::A4, Orientation::Landscape), vec![ViewDef::full(ViewKind::Table(table))]);
    let mut set = SheetSet::new("s");
    set.sources.push(SheetSource::TableSeries(TableSeries { template: spec, table_view: 0 }));
    let (expanded, warnings) = expand(&model, &set);
    assert!(warnings.is_empty());
    assert!(expanded.len() >= 3, "{}", expanded.len());
    let mut rows = 0;
    for e in &expanded {
        let ViewKind::Table(t) = &e.spec.views[0].kind else { unreachable!() };
        rows += t.rows.len();
        assert_eq!(t.headers[0], "Item");
        assert_eq!(t.col_widths_mm.len(), 6);
    }
    assert_eq!(rows, 80);
    let composed = compose_set(&model, &set);
    assert!(composed.warnings.is_empty(), "{:?}", composed.warnings);
    assert!(texts(&composed.sheets[1]).iter().any(|x| x == "2 of 3" || x.starts_with("2 of ")));
    assert!(expanded[0].spec.title.ends_with(&format!("(1 of {})", expanded.len())));
}

#[test]
fn a_detail_view_draws_a_chart_at_a_scale_with_its_caption() {
    use afrobim_schedule::*;
    let f = fixture();
    let model = Model { doc: &f.doc, registry: &f.registry };
    let d = |m, day| Date::from_ymd(2026, m, day).unwrap();
    let mut p = Programme::new("Works", d(3, 2), Calendar::new("Five-day", &[Weekday::Mon, Weekday::Tue, Weekday::Wed, Weekday::Thu, Weekday::Fri]));
    p.add_activity(Activity::new("A", "1.1", "Site clearance", 5));
    p.add_activity(Activity::new("B", "1.2", "Earthworks", 15));
    p.link("A", "B", LinkType::FS, 0);
    let s = schedule(&p).unwrap();
    let g = gantt_drawing(&p, &s, &GanttOptions::default());
    let labels: Vec<DetailLabel> = g.labels.iter().map(|l| DetailLabel::new(l.at, &l.text)).collect();
    let mut view = DetailView::inline(g.drawing.clone(), labels, Scale(1.0), 1.0, "CONSTRUCTION PROGRAMME");
    view.fit = true;
    let spec = SheetSpec::new("PROGRAMME", page(PaperSize::A3, Orientation::Landscape), vec![ViewDef::full(ViewKind::Detail(view))]);
    let mut set = SheetSet::new("s");
    set.sources.push(SheetSource::Single(spec));
    let composed = compose_set(&model, &set);
    let t = texts(&composed.sheets[0]);
    assert!(t.iter().any(|x| x == "Site clearance"), "{t:?}");
    assert!(t.iter().any(|x| x.starts_with("CONSTRUCTION PROGRAMME  1:")));
    assert!(composed.warnings.is_empty(), "{:?}", composed.warnings);
}

#[test]
fn the_set_is_stored_in_the_document_and_the_sheets_regenerate_from_the_live_model() {
    let mut f = fixture();
    let set = plan_series();
    let id = add_sheet_set(&mut f.doc, "Plan sheets", &set);
    // Save and load the whole document.
    let loaded = Document::from_json(&f.doc.to_json()).unwrap();
    assert_eq!(load_sheet_set(&loaded, id).unwrap(), set);
    assert_eq!(sheet_sets(&loaded).len(), 1);
    let model = Model { doc: &loaded, registry: &f.registry };
    assert_eq!(compose_set(&model, &set).sheets.len(), 6);

    // Change the model: lengthen the last straight by 600 m. The same stored set now makes two more sheets.
    let mut doc = loaded;
    let e = doc.find_mut(f.alignment).unwrap();
    let mut a: afrobim_alignment::Alignment = serde_json::from_value(e.data.clone()).unwrap();
    let last = a.elements.len() - 1;
    a.elements[last] = afrobim_alignment::AlignmentElement::line(a.elements[last].x, a.elements[last].y, a.elements[last].heading_rad, 950.0);
    a.profile = None;
    a.station_equations = Default::default();
    e.data = serde_json::to_value(&a).unwrap();
    let stored = load_sheet_set(&doc, id).unwrap();
    let model = Model { doc: &doc, registry: &f.registry };
    let composed = compose_set(&model, &stored);
    assert_eq!(composed.sheets.len(), 8, "{:?}", composed.warnings);
    // The module reports its findings and a placeholder drawing.
    let g = f.registry.generate(doc.find(id).unwrap()).unwrap();
    assert_eq!(g.derived["First number"], "TR-PL-001");
    assert!(g.notes.is_empty());
    // A bad series is reported by the module.
    let mut bad = set.clone();
    if let SheetSource::PlanSeries(p) = &mut bad.sources[0] {
        p.overlap_m = 400.0;
    }
    store_sheet_set(&mut doc, id, &bad).unwrap();
    assert_eq!(f.registry.generate(doc.find(id).unwrap()).unwrap().notes.len(), 1);
}

#[test]
fn project_information_is_stored_in_the_document() {
    let f = fixture();
    let loaded = Document::from_json(&f.doc.to_json()).unwrap();
    assert_eq!(project_info(&loaded), project());
}

#[test]
fn composed_sheets_preview_as_a_drawing_with_labels() {
    let f = fixture();
    let model = Model { doc: &f.doc, registry: &f.registry };
    let composed = compose_set(&model, &plan_series());
    let s = &composed.sheets[0];
    let d = s.to_drawing();
    assert_eq!(d.drawing.extents.max, Pt2::new(s.width_mm, s.height_mm));
    assert_eq!(d.labels.len(), s.texts().count());
    assert!(!d.drawing.primitives.is_empty());
}

// PDF -----------------------------------------------------------------------------------------

/// Checks the PDF's structure by reading it back: header, trailer, a cross-reference table whose
/// offsets each point at their object, the page count and the media box of each page. Returns
/// the inflated page streams.
fn read_back(pdf: &[u8]) -> (usize, Vec<String>) {
    assert!(pdf.starts_with(b"%PDF-1.4\n"));
    assert!(pdf.ends_with(b"%%EOF\n"));
    // One char per byte (binary bytes as ?), so that string offsets are file offsets.
    let text: String = pdf.iter().map(|&b| if b < 128 { b as char } else { '?' }).collect();
    let start: usize = text.rsplit("startxref\n").next().unwrap().lines().next().unwrap().parse().unwrap();
    assert!(text[start..].starts_with("xref\n0 "));
    let header = text[start..].lines().nth(1).unwrap();
    let count: usize = header.split(' ').nth(1).unwrap().parse().unwrap();
    for (i, line) in text[start..].lines().skip(3).take(count - 1).enumerate() {
        let off: usize = line[..10].parse().unwrap();
        assert!(text[off..].starts_with(&format!("{} 0 obj\n", i + 1)), "object {} is not at offset {off}", i + 1);
    }
    let pages = text.matches("/Type /Page ").count();
    assert!(text.contains(&format!("/Count {pages}")));
    // Inflate every stream.
    let mut streams = Vec::new();
    let mut at = 0;
    while let Some(p) = find(pdf, b"\nstream\n", at) {
        let begin = p + 8;
        let end = find(pdf, b"\nendstream", begin).unwrap();
        let mut out = String::new();
        flate2::read::ZlibDecoder::new(&pdf[begin..end]).read_to_string(&mut out).unwrap();
        streams.push(out);
        at = end;
    }
    assert_eq!(streams.len(), pages);
    (pages, streams)
}

fn find(h: &[u8], n: &[u8], from: usize) -> Option<usize> {
    h[from..].windows(n.len()).position(|w| w == n).map(|p| p + from)
}

fn have(tool: &str, arg: &str) -> bool {
    Command::new(tool).arg(arg).output().is_ok()
}

fn scratch(name: &str) -> PathBuf {
    let dir = std::env::var("AFROBIM_SHEETS_SHOTS").map(PathBuf::from).unwrap_or_else(|_| std::env::temp_dir().join("afrobim-sheets-tests"));
    std::fs::create_dir_all(&dir).unwrap();
    dir.join(name)
}

#[test]
fn the_pdf_is_a_valid_multi_page_file_with_real_text() {
    let f = fixture();
    let model = Model { doc: &f.doc, registry: &f.registry };
    let composed = compose_set(&model, &plan_series());
    let pdf = composed.pdf("Test set");
    let (pages, streams) = read_back(&pdf);
    assert_eq!(pages, 6);
    // Text is real text: shown with Tj in the standard faces.
    assert!(streams[0].contains("/F1 ") && streams[0].contains(" Tj ET"));
    assert!(streams[0].contains("(TR-PL-001)"));
    assert!(streams[2].contains("(3 of 6)"));
    // Deterministic: the same sheets give the same bytes.
    assert_eq!(pdf, composed.pdf("Test set"));
    // Poppler, when it is on the machine.
    if have("pdfinfo", "-v") && have("pdftotext", "-v") {
        let path = scratch("d1_test_set.pdf");
        std::fs::write(&path, &pdf).unwrap();
        let info = String::from_utf8(Command::new("pdfinfo").arg(&path).output().unwrap().stdout).unwrap();
        eprintln!("pdfinfo said:\n{info}");
        assert!(info.contains("Pages:           6"), "{info}");
        assert!(info.contains("Page size:"), "{info}");
        let txt = String::from_utf8(Command::new("pdftotext").arg("-layout").arg(&path).arg("-").output().unwrap().stdout).unwrap();
        assert!(txt.contains("TR-PL-004"));
        assert!(txt.contains("1+400"), "station labels are text");
        assert!(txt.contains("Test road, rehabilitation and upgrading"));
        assert!(txt.contains("MATCH LINE STA"));
    } else {
        eprintln!("pdfinfo or pdftotext not found: the poppler checks were skipped");
    }
    if have("qpdf", "--version") {
        let path = scratch("d1_test_set.pdf");
        std::fs::write(&path, &pdf).unwrap();
        let out = Command::new("qpdf").arg("--check").arg(&path).output().unwrap();
        eprintln!("qpdf said:\n{}", String::from_utf8_lossy(&out.stdout));
        assert!(out.status.success());
    }
}

// DXF -----------------------------------------------------------------------------------------

#[test]
fn the_dxf_reads_back_in_ezdxf_with_text_entities() {
    let f = fixture();
    let model = Model { doc: &f.doc, registry: &f.registry };
    let mut set = plan_series();
    // Three sheets are enough here.
    if let SheetSource::PlanSeries(p) = &mut set.sources[0] {
        p.to_station_m = Some(750.0);
    }
    let composed = compose_set(&model, &set);
    assert_eq!(composed.sheets.len(), 3);
    let dxf = composed.dxf(&set.styles, 50.0);
    let want_text = composed.sheets.iter().map(|s| s.texts().count()).sum::<usize>();
    let want_poly = composed.sheets.iter().map(|s| s.items.iter().filter(|i| !matches!(i, Item::Text(_))).count() + 1).sum::<usize>();
    assert!(dxf.starts_with("0\nSECTION\n2\nHEADER\n9\n$ACADVER\n1\nAC1009\n"));
    assert_eq!(dxf.matches("\n0\nTEXT\n").count(), want_text);
    let path = scratch("d1_test_set.dxf");
    std::fs::write(&path, &dxf).unwrap();
    let script = r#"
import sys
try:
    import ezdxf
except Exception:
    print("NOEZDXF"); sys.exit(0)
doc = ezdxf.readfile(sys.argv[1])
msp = doc.modelspace()
texts = [e for e in msp if e.dxftype() == "TEXT"]
polys = [e for e in msp if e.dxftype() == "POLYLINE"]
print("TEXT", len(texts))
print("POLYLINE", len(polys))
print("LAYERS", len(doc.layers))
print("SAMPLE", sorted({t.dxf.text for t in texts if t.dxf.text.startswith("TR-PL")}))
print("HEIGHT", sorted({round(t.dxf.height, 3) for t in texts})[:3])
print("ALIGNED", any(t.dxf.halign == 1 for t in texts))
"#;
    let py = Command::new("python3").arg("-I").arg("-c").arg(script).arg(&path).output();
    let Ok(out) = py else {
        eprintln!("python3 not found: the ezdxf check was skipped");
        return;
    };
    let said = String::from_utf8_lossy(&out.stdout).into_owned();
    eprintln!("ezdxf said:\n{said}{}", String::from_utf8_lossy(&out.stderr));
    if said.contains("NOEZDXF") {
        eprintln!("ezdxf is not installed: the ezdxf check was skipped");
        return;
    }
    assert!(out.status.success(), "{}", String::from_utf8_lossy(&out.stderr));
    let get = |key: &str| -> usize { said.lines().find_map(|l| l.strip_prefix(key)).map(|v| v.trim().parse().unwrap()).unwrap() };
    assert_eq!(get("TEXT "), want_text);
    assert!(get("TEXT ") > 100);
    assert_eq!(get("POLYLINE "), want_poly);
    assert!(said.contains("TR-PL-001") && said.contains("TR-PL-003"));
    assert!(said.contains("ALIGNED True"));
}

// Review renders -------------------------------------------------------------------------------

/// Writes review sheets (plan, profile, sections, schedule, detail) as PDFs; converts to PNG when
/// pdftoppm is there. Where they go: `AFROBIM_SHEETS_SHOTS` or the system temp folder.
#[test]
fn review_sheets_render_without_warnings_about_layout() {
    let f = fixture();
    let model = Model { doc: &f.doc, registry: &f.registry };
    let mut set = plan_series();
    // Plan: only two sheets, so the review is quick.
    if let SheetSource::PlanSeries(p) = &mut set.sources[0] {
        p.to_station_m = Some(570.0);
    }
    // Profile with ground, sections, and a schedule.
    let mut prof = ProfileView::new("MC1", 0.0, 600.0, Scale(2000.0), 10.0);
    prof.surface = Some("Ground".into());
    prof.grid_level_interval_m = 2.0;
    let notes = Notes {
        heading: Some("GENERAL NOTES".into()),
        paragraphs: vec![
            "All dimensions are in metres unless stated otherwise.".into(),
            "Levels are to the project datum. Stations are displayed stations; the station equation at internal station 1+000 is shown on the plan sheets.".into(),
            "Do not scale from this drawing.".into(),
        ],
        numbered: true,
    };
    set.sources.push(SheetSource::Single(SheetSpec::new(
        "LONGITUDINAL PROFILE",
        page(PaperSize::A3, Orientation::Landscape),
        vec![ViewDef::in_area(0.0, 0.0, 1.0, 0.8, ViewKind::Profile(prof)), ViewDef::in_area(0.0, 0.8, 1.0, 1.0, ViewKind::Notes(notes))],
    )));
    let mut sec = SectionsView::new("Road", Scale(500.0), 3);
    sec.from_station_m = Some(300.0);
    sec.to_station_m = Some(550.0);
    set.sources.push(SheetSource::SectionSeries(SectionSeries {
        template: SheetSpec::new("CROSS-SECTIONS", page(PaperSize::A3, Orientation::Landscape), vec![ViewDef::full(ViewKind::Sections(sec))]),
        sections_view: 0,
        per_sheet: 9,
    }));
    let mut csv = String::from("Mark,Type,Description,Quantity,Unit,Location\n");
    for i in 1..=30 {
        csv.push_str(&format!("ADS-{i:03},Advance direction sign,\"Stack sign with destinations, face area 2.4 m2, on two posts, item {i}\",{},nr,\"Sta {}+{:03}, left\"\n", 1 + i % 3, i / 2, (i * 37) % 1000));
    }
    set.sources.push(SheetSource::TableSeries(TableSeries {
        template: SheetSpec::new("SIGN SCHEDULE", page(PaperSize::A4, Orientation::Landscape), vec![ViewDef::full(ViewKind::Table(sign_schedule_table(&csv)))]),
        table_view: 0,
    }));
    let composed = compose_set(&model, &set);
    eprintln!("sheets {} warnings {:?}", composed.sheets.len(), composed.warnings);
    assert!(composed.warnings.is_empty(), "{:?}", composed.warnings);
    let pdf = composed.pdf("Review");
    read_back(&pdf);
    let path = scratch("d1_review.pdf");
    std::fs::write(&path, &pdf).unwrap();
    std::fs::write(scratch("d1_review.dxf"), composed.dxf(&set.styles, 50.0)).unwrap();
    if have("pdftoppm", "-v") {
        let out = Command::new("pdftoppm").args(["-png", "-r", "70"]).arg(&path).arg(scratch("d1_review")).output().unwrap();
        assert!(out.status.success());
    }
}

#[test]
fn a_profile_view_annotates_pvis_grades_and_curves() {
    // The fixture profile: PVIs (0, 1100), (400, 1106 with a 120 m parabola), (900, 1098 with a
    // 160 m parabola), (1500, 1104). Grades: 6 / 400 = +1.50 %, -8 / 500 = -1.60 %, 6 / 600 =
    // +1.00 %. Curve at 400: A = -1.6 - 1.5 = -3.10 %, L = 120, K = 120 / 3.1 = 38.71, from 340
    // to 460. Curve at 900: A = 1.0 + 1.6 = 2.60 %, L = 160, K = 160 / 2.6 = 61.54, from 820 to 980.
    let f = fixture();
    let model = Model { doc: &f.doc, registry: &f.registry };
    let v = ProfileView::new("MC1", 0.0, 1500.0, Scale(8000.0), 20.0);
    let mut set = SheetSet::new("s");
    set.sources.push(SheetSource::Single(SheetSpec::new("P", Page::default(), vec![ViewDef::full(ViewKind::Profile(v))])));
    let composed = compose_set(&model, &set);
    let s = &composed.sheets[0];
    let t = texts(s);
    for want in ["+1.50 %", "-1.60 %", "+1.00 %"] {
        assert!(t.iter().any(|x| x == want), "missing grade {want}: {t:?}");
    }
    let has = |needle: &str| t.iter().any(|x| x.contains(needle));
    assert!(has("EL 1106.000") && has("EL 1098.000") && has("EL 1100.000") && has("EL 1104.000"), "{t:?}");
    assert!(has("L 120.000   A -3.10 %   K 38.71") && has("L 160.000   A +2.60 %   K 61.54"), "{t:?}");
    // Curve ends at 340 and 460, 820 and 980 are before the equation at 1000, so they read as they are.
    assert!(has("PVC 0+340.000   PVT 0+460.000") && has("PVC 0+820.000   PVT 0+980.000"), "{t:?}");
    // No limit and no pass or fail is applied anywhere in the sheet.
    assert!(!t.iter().any(|x| x.to_lowercase().contains("pass") || x.to_lowercase().contains("fail") || x.to_lowercase().contains("minimum")));
    // Annotation texts never overlap each other (staggered rows) and stay on the sheet.
    let boxes: Vec<(f64, f64, f64, f64)> = s
        .texts()
        .filter(|t| t.layer == layers::PROFILE_NOTES && t.rotation_deg == 0.0)
        .map(|t| (t.at.x, t.at.y, t.at.x + text_width(&t.text, t.height_mm, false), t.at.y + t.height_mm))
        .collect();
    assert!(boxes.len() >= 8);
    for (i, a) in boxes.iter().enumerate() {
        assert!(a.0 >= 0.0 && a.2 <= s.width_mm && a.3 <= s.height_mm);
        for b in &boxes[i + 1..] {
            assert!(!(a.0 < b.2 && b.0 < a.2 && a.1 < b.3 && b.1 < a.3), "annotation texts overlap: {a:?} {b:?}");
        }
    }
    // Switching it off removes every annotation.
    let mut off = ProfileView::new("MC1", 0.0, 1500.0, Scale(8000.0), 20.0);
    off.annotations = ProfileAnnotations::none();
    let mut set2 = SheetSet::new("s");
    set2.sources.push(SheetSource::Single(SheetSpec::new("P", Page::default(), vec![ViewDef::full(ViewKind::Profile(off))])));
    let plain = compose_set(&model, &set2);
    assert!(!plain.sheets[0].items.iter().any(|i| i.layer() == layers::PROFILE_NOTES));
}

#[test]
fn a_source_can_have_its_own_numbering_prefix() {
    let f = fixture();
    let model = Model { doc: &f.doc, registry: &f.registry };
    let mut set = plan_series();
    set.sources.push(SheetSource::Single(SheetSpec::new("NOTES", Page::default(), vec![ViewDef::full(ViewKind::Notes(Notes { heading: None, paragraphs: vec!["x".into()], numbered: false }))])));
    set.sources[1].template_mut().number_prefix = Some("TR-N-".into());
    let (expanded, _) = expand(&model, &set);
    let n = expanded.len();
    assert_eq!(expanded[n - 1].number, "TR-N-001");
    assert_eq!(expanded[0].number, "TR-PL-001");
}
