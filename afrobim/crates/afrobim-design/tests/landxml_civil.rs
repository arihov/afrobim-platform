// SPDX-License-Identifier: LicenseRef-AfroBIM-Proprietary
// Copyright (c) 2026 AfroBIM. All rights reserved.
//! Round trips of what a Civil 3D file holds: `parse_landxml_file`, `write_landxml_file`,
//! `parse_landxml_file` again, and the two readings compared for exact equality.
//!
//! The public test uses the synthetic file with Civil 3D's structure in `afrobim-alignment`
//! (`civil3d_like.xml`, made-up numbers). The private test does the same with the file named by
//! `AFROBIM_PRIVATE_LANDXML` (a real Civil 3D export that is not in this repository) and is
//! skipped, passing with a printed note, when that is unset or missing.
//!
//! `AFROBIM_LANDXML_XSD` (optional): the path of `LandXML-1.2.xsd`, which is kept outside the
//! repository. When set and `xmllint` is installed, what the writer wrote is validated against it
//! (`xmllint --noout --schema`). `AFROBIM_LANDXML_OUT`: a directory to leave the written files in.

use std::path::{Path, PathBuf};
use std::time::Instant;

use afrobim_alignment::{parse_landxml_file, read_landxml_file, LandXmlFile};
use afrobim_design::{write_landxml_file, LandXmlOptions};

fn options() -> LandXmlOptions {
    LandXmlOptions { date: "2026-11-16".into(), time: "09:00:00".into(), project: String::new() }
}

fn synthetic() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("..").join("afrobim-alignment").join("tests").join("landxml").join("civil3d_like.xml")
}

/// Validates with xmllint when the schema is at hand; returns whether it was checked.
fn validate(label: &str, xml: &Path) -> bool {
    let Ok(xsd) = std::env::var("AFROBIM_LANDXML_XSD") else {
        println!("{label}: AFROBIM_LANDXML_XSD is not set; the output was not validated against the schema.");
        return false;
    };
    let out = match std::process::Command::new("xmllint").arg("--noout").arg("--schema").arg(&xsd).arg(xml).output() {
        Ok(o) => o,
        Err(e) => {
            println!("{label}: xmllint is not available ({e}); the output was not validated against the schema.");
            return false;
        }
    };
    let text = format!("{}{}", String::from_utf8_lossy(&out.stdout), String::from_utf8_lossy(&out.stderr));
    println!("{label}: xmllint: {}", text.lines().last().unwrap_or(""));
    assert!(out.status.success(), "{label}: the output does not validate against the schema:\n{}", text.lines().take(10).collect::<Vec<_>>().join("\n"));
    true
}

/// Everything the writer is meant to carry, compared part by part.
fn assert_survives(label: &str, a: &LandXmlFile, b: &LandXmlFile) {
    assert_eq!(a.alignments, b.alignments, "{label}: alignments (geometry, engine profile, superelevation, station equations)");
    assert_eq!(a.alignment_infos, b.alignment_infos, "{label}: alignment attributes and the stated geometry");
    assert_eq!(a.design_profiles, b.design_profiles, "{label}: design profiles");
    assert_eq!(a.ground_profiles, b.ground_profiles, "{label}: ground profiles");
    assert_eq!(a.cross_sections, b.cross_sections, "{label}: cross-sections");
    assert_eq!(a.cross_sect_groups, b.cross_sect_groups, "{label}: CrossSects attributes");
    assert_eq!(a.cross_sect_details, b.cross_sect_details, "{label}: CrossSect attributes");
    assert_eq!(a.superelevation_tables, b.superelevation_tables, "{label}: superelevation tables");
    assert_eq!(a.object_features, b.object_features, "{label}: features");
    assert_eq!(a.field_notes, b.field_notes, "{label}: field notes");
    assert_eq!(a.surfaces, b.surfaces, "{label}: surfaces");
    assert_eq!(a.coordinate_system, b.coordinate_system, "{label}: coordinate system");
    assert_eq!(a.project, b.project, "{label}: project");
    assert_eq!((&a.units.linear_unit, &a.units.angular_unit, &a.units.direction_unit), (&b.units.linear_unit, &b.units.angular_unit, &b.units.direction_unit), "{label}: units");
    assert_eq!(a.summary.counts, b.summary.counts, "{label}: counts");
    assert_eq!(a.summary.alignments, b.summary.alignments, "{label}: alignment summary");
    assert_eq!(a.summary.surfaces, b.summary.surfaces, "{label}: surface summary");
}

fn round_trip(label: &str, f: &LandXmlFile) -> (String, Vec<String>, LandXmlFile) {
    let t = Instant::now();
    let out = write_landxml_file(f, &options());
    println!("{label}: written in {:?}: {} bytes, {} warning(s)", t.elapsed(), out.text.len(), out.warnings.len());
    let dir = std::env::var("AFROBIM_LANDXML_OUT").map(PathBuf::from).unwrap_or_else(|_| std::env::temp_dir().join(format!("afrobim-roundtrip-{}", std::process::id())));
    std::fs::create_dir_all(&dir).unwrap();
    let path = dir.join(format!("{label}.xml"));
    std::fs::write(&path, &out.text).unwrap();
    validate(label, &path);
    if std::env::var("AFROBIM_LANDXML_OUT").is_err() {
        let _ = std::fs::remove_dir_all(&dir);
    }
    let t = Instant::now();
    let back = parse_landxml_file(&out.text).unwrap_or_else(|e| panic!("{label}: {e}"));
    println!("{label}: read again in {:?}", t.elapsed());
    (out.text, out.warnings, back)
}

#[test]
fn a_civil_3d_style_file_survives_a_round_trip_exactly() {
    let f = read_landxml_file(&synthetic()).unwrap();
    let (text, warnings, back) = round_trip("civil3d_like", &f);
    assert!(warnings.is_empty(), "{warnings:?}");
    assert_survives("civil3d_like", &f, &back);
    // Spot checks: nothing was recomputed from the engine geometry.
    assert_eq!((back.alignments[0].elements[0].x, back.alignments[0].elements[0].y), (358000.25, 9964000.5));
    assert!(text.contains("<Spiral length=\"40\"") && text.contains("spiType=\"clothoid\""));
    assert!(text.contains("<FullSuperelev>-3.5</FullSuperelev>") && text.contains("<Superelevation staStart=\"50\" staEnd=\"60\">"), "{text}");
    assert!(text.contains("<Feature name=\"SpeedStation\" code=\"0\" source=\"Autodesk Civil 3D\">") && text.contains("<Property label=\"speed\" value=\"60.00000000\"/>"));
    assert!(text.contains("<FieldNote>Transformation:LocalPoint=(1000.5 2000.25 0.0);"));
    assert!(text.contains("<PointFile fileName=") && text.contains("area2DSurf=\"600\""));
    assert!(text.contains("<CrossSect sta=\"20\" name=\"0+020\" areaCut=\"12.5\" areaFill=\"3.25\" sectType=\"made-up\" volumeCut=\"200\" volumeFill=\"40\">"), "{text}");
    // The vertical step of the second profile is written as it was read, two PVIs at one station.
    assert!(text.contains("<PVI>31.5 1210.5</PVI>") && text.contains("<PVI>31.5 1209.5</PVI>"));
    // Written by this crate, not by Civil 3D.
    assert!(!back.is_civil_3d());
}

#[test]
fn what_was_changed_since_the_read_is_written_from_the_engine_and_said() {
    let mut f = read_landxml_file(&synthetic()).unwrap();
    // Move the second alignment: the stated points are stale and are not written.
    f.alignments[1].elements[0].x += 1.0;
    // Replace the superelevation of the first: the records of the file are stale.
    f.alignments[0].superelevation = None;
    let out = write_landxml_file(&f, &options());
    assert!(out.warnings.iter().any(|w| w.contains("Alignment 'Beta Spur': its geometry was changed since the file was read")), "{:?}", out.warnings);
    assert!(out.warnings.iter().any(|w| w.contains("Alignment 'Alpha Road': its superelevation was changed since the file was read")), "{:?}", out.warnings);
    let back = parse_landxml_file(&out.text).unwrap();
    assert_eq!(back.alignments[1].elements[0].x, f.alignments[1].elements[0].x);
}

/// Parts of the first reading that the second does not have, and why: printed for the report.
fn print_losses(f: &LandXmlFile, back: &LandXmlFile) {
    println!("not carried: the source's Application ({:?}), LandXML@readOnly ({:?}), the date and time (replaced by the options)", f.applications.iter().map(|a| a.name.as_str()).collect::<Vec<_>>(), f.header.read_only);
    println!("redundant attributes not read, so not written ({} values of {} kinds): the second reading has {} values of {} kinds", f.summary.ignored_attributes.values().sum::<usize>(), f.summary.ignored_attributes.len(), back.summary.ignored_attributes.values().sum::<usize>(), back.summary.ignored_attributes.len());
    println!("warnings: first reading {}, second {}", f.warnings.len(), back.warnings.len());
}

#[test]
fn the_private_file_survives_a_round_trip_exactly() {
    let Ok(p) = std::env::var("AFROBIM_PRIVATE_LANDXML") else {
        println!("AFROBIM_PRIVATE_LANDXML is not set: private test skipped.");
        return;
    };
    let path = PathBuf::from(&p);
    if !path.is_file() {
        println!("AFROBIM_PRIVATE_LANDXML is set to '{p}', which is not a file: private test skipped.");
        return;
    }
    let t = Instant::now();
    let f = read_landxml_file(&path).unwrap();
    println!("first reading: {:?}", t.elapsed());
    let (text, warnings, back) = round_trip("private", &f);
    for w in &warnings {
        println!("writer warning: {}", &w[..w.len().min(300)]);
    }
    print_losses(&f, &back);
    assert!(warnings.is_empty(), "{warnings:?}");
    assert_survives("private", &f, &back);
    // Counts, and the coordinates of the first point of each surface and alignment, bit for bit.
    assert_eq!((back.alignments.len(), back.surfaces.len(), back.cross_sections.len(), back.design_profiles.len(), back.ground_profiles.len()), (17, 4, 1055, 17, 17));
    for (a, b) in f.alignments.iter().zip(&back.alignments) {
        assert_eq!(a.elements.len(), b.elements.len());
        for (x, y) in a.elements.iter().zip(&b.elements) {
            assert_eq!((x.x.to_bits(), x.y.to_bits(), x.heading_rad.to_bits(), x.length.to_bits()), (y.x.to_bits(), y.y.to_bits(), y.heading_rad.to_bits(), y.length.to_bits()));
        }
    }
    for (a, b) in f.surfaces.iter().zip(&back.surfaces) {
        assert_eq!(a.points.len(), b.points.len());
        assert!(a.points.iter().zip(&b.points).all(|(p, q)| p.id == q.id && p.easting_m.to_bits() == q.easting_m.to_bits() && p.northing_m.to_bits() == q.northing_m.to_bits() && p.elevation_m.to_bits() == q.elevation_m.to_bits()));
        assert_eq!(a.faces, b.faces);
    }
    assert!(!text.contains("Application name=\"Autodesk Civil 3D\""));
    // Writing what was read again gives the same text: the writer is a fixed point.
    let again = write_landxml_file(&back, &options());
    assert_eq!(again.text, text);
}
