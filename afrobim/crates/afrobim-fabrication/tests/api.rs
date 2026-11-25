// SPDX-License-Identifier: LicenseRef-AfroBIM-Proprietary
// Copyright (c) 2026 AfroBIM. All rights reserved.
//! The public API as the desktop app uses it, on things the C# reference files do not cover:
//! measuring options, the batch's edge cases and the default PDF stamp.

use std::path::{Path, PathBuf};

use afrobim_fabrication::*;
use afrobim_signs::{Colour, Destination, Direction, SignDefinition, SignEngine};

fn repo_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../..")
}

fn engine() -> SignEngine {
    SignEngine::load(&repo_root().join("packs/UK-TSM-2019.json"), &repo_root().join("fonts")).unwrap()
}

fn seed_sign() -> SignDefinition {
    SignDefinition::stack("ADS-001", 60, vec![Destination::new("Kasese", Direction::Ahead), Destination::new("Masheruka", Direction::Right)])
}

fn out_dir(name: &str) -> PathBuf {
    let dir = Path::new(env!("CARGO_TARGET_TMPDIR")).join(name);
    let _ = std::fs::remove_dir_all(&dir);
    dir
}

#[test]
fn measure_reports_the_panel_at_the_signs_own_size() {
    let e = engine();
    let sign = seed_sign();
    let layout = e.layout(&sign).unwrap();
    let q = measure(&e, &sign, &layout);
    assert_eq!((q.code.as_str(), q.ground, q.speed_kph), ("ADS-001", Colour::Green, 60));
    assert_eq!((q.sw_mm, q.x_height_mm), (layout.sw_mm, layout.x_height_mm));
    assert_eq!(q.width_mm, layout.drawing.extents.width() * layout.sw_mm);
    assert_eq!(q.height_mm, layout.drawing.extents.height() * layout.sw_mm);
    assert_eq!(q.legend_characters, "KaseseMasheruka".len());
    assert_eq!((q.symbols, q.warnings, q.errors), (0, 0, 0));
    assert!((q.area_m2() - q.width_mm * q.height_mm / 1e6).abs() < 1e-12);
}

#[test]
fn a_stroke_width_override_scales_the_panel_and_the_x_height() {
    let e = engine();
    let sign = seed_sign();
    let layout = e.layout(&sign).unwrap();
    let q = measure_with(&e, &sign, &layout, MeasureOptions { sw_mm_override: Some(10.0), round_up_mm: 0.0 }).unwrap();
    assert_eq!((q.sw_mm, q.x_height_mm), (10.0, 40.0));
    assert_eq!(q.width_mm, layout.drawing.extents.width() * 10.0);

    let rounded = measure_with(&e, &sign, &layout, MeasureOptions { sw_mm_override: Some(10.0), round_up_mm: 25.0 }).unwrap();
    assert!(rounded.width_mm >= q.width_mm && rounded.width_mm < q.width_mm + 25.0);
    assert_eq!(rounded.width_mm % 25.0, 0.0);
    assert_eq!(rounded.height_mm % 25.0, 0.0);
}

#[test]
fn a_negative_module_is_refused() {
    let e = engine();
    let sign = seed_sign();
    let layout = e.layout(&sign).unwrap();
    let err = measure_with(&e, &sign, &layout, MeasureOptions { sw_mm_override: None, round_up_mm: -1.0 }).unwrap_err();
    assert!(matches!(err, FabricationError::InvalidOption(_)));
}

#[test]
fn symbols_are_counted_on_the_sign_the_destinations_and_the_embedded_list() {
    let e = engine();
    let mut sign = seed_sign();
    sign.destinations[0].symbol_id = Some("parking-p".into());
    sign.destinations[1].symbol_id = Some("  ".into());
    let layout = e.layout(&sign).unwrap();
    assert_eq!(measure(&e, &sign, &layout).symbols, 1);
}

#[test]
fn the_sheet_holds_the_face_between_its_dimensions() {
    let e = engine();
    let sign = seed_sign();
    let layout = e.layout(&sign).unwrap();
    let sheet = compose(&e, &sign, &layout, &SheetInfo { project: "P".into(), ..SheetInfo::default() });

    assert!(composer::SCALE_LADDER.contains(&sheet.scale));
    let (w, h) = (sheet.quantity.width_mm / f64::from(sheet.scale), sheet.quantity.height_mm / f64::from(sheet.scale));
    assert!((sheet.face_box.width() - w).abs() < 1e-9 && (sheet.face_box.height() - h).abs() < 1e-9);
    assert!(sheet.face_box.min_x > composer::DRAWING_AREA.min_x && sheet.face_box.max_x < composer::DRAWING_AREA.max_x);
    assert!(sheet.face_box.min_y > composer::DRAWING_AREA.min_y && sheet.face_box.max_y < composer::DRAWING_AREA.max_y);

    let overall = &sheet.dimensions[0];
    assert_eq!((overall.name.as_str(), overall.orientation), ("overall-width", DimensionOrientation::Horizontal));
    assert_eq!((overall.start, overall.end), (sheet.face_box.min_x, sheet.face_box.max_x));
    assert_eq!(overall.label(), composer::format_mm(sheet.quantity.width_mm));
    assert_eq!(sheet.dimensions[1].name, "overall-height");

    let labels: Vec<&str> = sheet.title_block.iter().map(|f| f.label.as_str()).collect();
    assert_eq!(labels, ["PROJECT", "SIGN CODE", "DRAWING NO.", "SCALE", "DATE", "DRAWN BY", "REVISION"]);
    assert_eq!(sheet.title_block[3].value, format!("1:{}", sheet.scale));
    assert_eq!(sheet.title_block[6].value, "A", "the default revision");
    assert_eq!(sheet.colours[0].role, "Ground");
    assert_eq!(sheet.colours.len(), sheet.swatches.len());
    assert!(sheet.reference.ends_with('.'));
    assert!(sheet.texts.iter().any(|t| t.text == "SIGN DATA" && t.bold));
    assert!(sheet.lines.iter().all(|l| [composer::FRAME_LAYER, composer::DIM_LAYER].contains(&l.layer)));
    // Everything drawn stays on the A3 sheet.
    for l in &sheet.lines {
        for (x, y) in [(l.x1, l.y1), (l.x2, l.y2)] {
            assert!((0.0..=FabricationSheet::WIDTH_MM).contains(&x) && (0.0..=FabricationSheet::HEIGHT_MM).contains(&y));
        }
    }
}

#[test]
fn a_run_without_signs_writes_nothing_and_says_so() {
    let e = engine();
    let fonts = PdfFonts::from_folder(&repo_root().join("fonts"), &e).unwrap();
    let dir = out_dir("api-empty");
    let result = run(&e, &fonts, &[], &FabricationOptions::new(&dir));
    assert_eq!(result.exit_code, 5);
    assert!(result.files_written.is_empty() && result.rows.is_empty() && result.errors.is_empty());
    assert!(dir.is_dir());
    assert_eq!(std::fs::read_dir(&dir).unwrap().count(), 0);
}

#[test]
fn a_run_with_defaults_dates_and_numbers_the_sheets() {
    let e = engine();
    let fonts = PdfFonts::from_folder(&repo_root().join("fonts"), &e).unwrap();
    let dir = out_dir("api-defaults");
    let mut second = seed_sign();
    second.code = "ADS:002".into();
    let signs = [SheetSign::new(seed_sign()), SheetSign { quantity: Some(2), ..SheetSign::new(second) }];
    let result = run(&e, &fonts, &signs, &FabricationOptions::new(&dir));
    assert_eq!(result.exit_code, 0, "{:?}", result.errors);
    let names: Vec<String> = result.files_written.iter().map(|p| p.file_name().unwrap().to_string_lossy().into_owned()).collect();
    assert_eq!(names, ["ADS-001-sheet.dxf", "ADS-001-sheet.pdf", "ADS_002-sheet.dxf", "ADS_002-sheet.pdf", SCHEDULE_FILE_NAME]);
    assert_eq!(result.rows.len(), 2);
    assert_eq!(schedule_totals(&result.rows).count, 3);

    // A blank date becomes today's, and the drawing numbers count the signs.
    let dxf = std::fs::read_to_string(dir.join("ADS_002-sheet.dxf")).unwrap();
    assert!(dxf.contains("\nFS-002\n"));
    let after_label = dxf.split("\nDATE\n").nth(1).unwrap();
    let date = after_label.lines().find(|l| l.len() == 10 && l.as_bytes()[4] == b'-' && l.as_bytes()[7] == b'-').unwrap();
    assert!(date.starts_with("20"), "{date}");

    // The default stamp names this program, not the C# one.
    let pdf = std::fs::read(dir.join("ADS-001-sheet.pdf")).unwrap();
    let text = String::from_utf8_lossy(&pdf);
    assert!(text.starts_with("%PDF-1.7\n"));
    assert!(text.contains("/Title(ADS-001 fabrication sheet)/Creator(AfroBIM)/Producer(AfroBIM afrobim-fabrication "));
    assert!(text.contains(&format!("<xmp:CreateDate>{date}T")));
    assert!(text.ends_with("\n%%EOF\n"));
}

#[test]
fn a_folder_that_cannot_be_made_is_an_error_not_a_panic() {
    let e = engine();
    let fonts = PdfFonts::from_folder(&repo_root().join("fonts"), &e).unwrap();
    let dir = out_dir("api-blocked");
    std::fs::create_dir_all(&dir).unwrap();
    let file = dir.join("not-a-folder");
    std::fs::write(&file, b"x").unwrap();
    let result = run(&e, &fonts, &[SheetSign::new(seed_sign())], &FabricationOptions::new(file.join("sheets")));
    assert_eq!(result.exit_code, 5);
    assert_eq!(result.errors.len(), 1);
    assert!(result.files_written.is_empty());
}

#[test]
fn fonts_that_are_missing_or_not_fonts_are_reported() {
    let dir = out_dir("api-fonts");
    std::fs::create_dir_all(&dir).unwrap();
    let junk = dir.join("junk.ttf");
    std::fs::write(&junk, b"this is not a font").unwrap();
    assert!(matches!(PdfFonts::load(&junk, &junk), Err(FabricationError::Font(_))));
    assert!(matches!(PdfFonts::load(&dir.join("absent.ttf"), &junk), Err(FabricationError::Font(_))));
    assert!(PdfFonts::from_bytes(vec![1, 2, 3], vec![]).is_err());
}

#[test]
fn a_title_outside_latin_1_is_written_as_utf_16() {
    let e = engine();
    let fonts = PdfFonts::from_folder(&repo_root().join("fonts"), &e).unwrap();
    let mut sign = seed_sign();
    sign.code = "道-1".into();
    let layout = e.layout(&sign).unwrap();
    let sheet = compose(&e, &sign, &layout, &SheetInfo::default());
    let pdf = write_sheet_pdf(&sheet, &fonts);
    let text = String::from_utf8_lossy(&pdf);
    assert!(text.contains("/Title<FEFF9053002D0031002000660061"), "the title is hex UTF-16BE with a byte-order mark");
    // In the sheet itself the Transport faces cannot print it: ASCII, as in the DXF.
    assert!(write_sheet_dxf(&sheet).contains("\n?-1\n"));
}
