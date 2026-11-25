// SPDX-License-Identifier: LicenseRef-AfroBIM-Proprietary
// Copyright (c) 2026 AfroBIM. All rights reserved.
//! Georeferenced export: `IfcProjectedCRS` and `IfcMapConversion`, with the geometry written near a
//! local origin. The site fixture is moved to UTM-sized coordinates (easting 450 km, northing
//! 9 960 km) and exported with and without a georeference.

mod common;

use afrobim_alignment::Alignment;
use afrobim_ifc::{export_ifc_model, AlignmentExport, IfcGeoreference, IfcOptions};
use common::site::site_fixture;
use common::{options, python_with_ifcopenshell};

const DE: f64 = 450_000.0;
const DN: f64 = 9_960_000.0;

fn moved() -> common::site::SiteFixture {
    let mut f = site_fixture();
    let ids: Vec<_> = f.document.elements().iter().map(|e| e.id).collect();
    for id in ids {
        let e = f.document.find_mut(id).unwrap();
        if let Some(s) = e.site.as_mut() {
            s.x += DE * 1000.0;
            s.y += DN * 1000.0;
        }
        if e.category == "alignment" {
            let mut a: Alignment = serde_json::from_value(e.data.clone()).unwrap();
            for el in &mut a.elements {
                el.x += DE;
                el.y += DN;
            }
            e.data = serde_json::to_value(&a).unwrap();
        }
    }
    f
}

fn georef() -> IfcGeoreference {
    IfcGeoreference {
        name: "EPSG:32736".into(),
        description: Some("EPSG:32736 - WGS 84 / UTM zone 36S".into()),
        geodetic_datum: Some("WGS 84".into()),
        vertical_datum: Some("Mean sea level (test text)".into()),
        map_projection: Some("UTM".into()),
        map_zone: Some("36S".into()),
        origin: None,
    }
}

fn largest_point(text: &str) -> f64 {
    let mut max: f64 = 0.0;
    for line in text.lines().filter(|l| l.contains("IFCCARTESIANPOINT(") || l.contains("IFCCARTESIANPOINTLIST")) {
        for token in line.split(|c: char| !(c.is_ascii_digit() || c == '.' || c == '-' || c == 'E')).filter(|t| t.chars().any(|c| c.is_ascii_digit())) {
            if let Ok(v) = token.parse::<f64>() {
                max = max.max(v.abs());
            }
        }
    }
    max
}

fn export(f: &common::site::SiteFixture, g: Option<IfcGeoreference>, mode: AlignmentExport) -> String {
    export_ifc_model(&f.document, &f.inputs(), &IfcOptions { alignment: mode, georeference: g, ..options() }).text
}

#[test]
fn a_georeferenced_export_writes_the_crs_and_the_map_conversion_and_keeps_numbers_small() {
    let f = moved();
    for mode in [AlignmentExport::Simple, AlignmentExport::Linear] {
        let plain = export(&f, None, mode);
        assert!(!plain.contains("IFCMAPCONVERSION") && !plain.contains("IFCPROJECTEDCRS"), "no CRS, no entities");
        assert!(largest_point(&plain) > 4.0e8, "without a georeference the numbers are the site's: {}", largest_point(&plain));

        let text = export(&f, Some(georef()), mode);
        assert!(text.contains("IFCPROJECTEDCRS('EPSG:32736','EPSG:32736 - WGS 84 / UTM zone 36S','WGS 84','Mean sea level (test text)','UTM','36S',#"), "{mode:?}");
        // The rule: least easting 450 000 - 80 = 449 920 m rounds down to 449 000; least northing 9 960 000 - 22.25 rounds down to 9 959 000.
        assert!(text.contains("IFCMAPCONVERSION(#") && text.contains(",449000.,9959000.,0.,1.,0.,0.001)"), "{mode:?}: {}", text.lines().find(|l| l.contains("IFCMAPCONVERSION")).unwrap_or("none"));
        let big = largest_point(&text);
        assert!(big < 5.0e6, "{mode:?}: geometry stays within a few kilometres of the local origin, largest {big}");
    }
}

#[test]
fn a_chosen_origin_is_used_as_given() {
    let f = moved();
    let g = IfcGeoreference { origin: Some([450_000.0, 9_960_000.0, 0.0]), ..georef() };
    let text = export(&f, Some(g), AlignmentExport::Simple);
    assert!(text.contains(",450000.,9960000.,0.,1.,0.,0.001)"));
}

#[test]
fn a_document_with_nothing_on_the_site_is_not_georeferenced() {
    let f = common::fixture();
    let text = export_ifc_model(&f.document, &f.inputs(), &IfcOptions { georeference: Some(georef()), ..options() }).text;
    assert!(!text.contains("IFCMAPCONVERSION"), "a sheet of signs has nothing to place on a map");
}

/// IfcOpenShell reads the file back: with the map conversion added, every product is where the
/// same product is in the file written without a georeference from the unmoved fixture.
#[test]
fn the_map_conversion_puts_every_product_back_at_its_site_position_for_ifcopenshell() {
    let python = match python_with_ifcopenshell() {
        Ok(p) => p,
        Err(why) => {
            assert!(std::env::var("AFROBIM_REQUIRE_IFCOPENSHELL").is_err(), "ifcopenshell is required: {why}");
            eprintln!("skipped: {why}");
            return;
        }
    };
    let dir = std::env::temp_dir().join(format!("afrobim-georef-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let (a, b) = (dir.join("plain.ifc"), dir.join("georef.ifc"));
    let g = IfcGeoreference { origin: Some([DE, DN, 0.0]), ..georef() };
    std::fs::write(&a, export(&site_fixture(), None, AlignmentExport::Simple)).unwrap();
    std::fs::write(&b, export(&moved(), Some(g), AlignmentExport::Simple)).unwrap();
    let script = r#"
import sys, ifcopenshell, ifcopenshell.util.placement as pl
plain = ifcopenshell.open(sys.argv[1]); geo = ifcopenshell.open(sys.argv[2])
mc = geo.by_type("IfcMapConversion")[0]
crs = mc.TargetCRS
assert crs.is_a("IfcProjectedCRS") and crs.Name == "EPSG:32736" and crs.MapZone == "36S", crs
assert mc.SourceCRS.is_a("IfcGeometricRepresentationContext")
e, n, h, scale = mc.Eastings, mc.Northings, mc.OrthogonalHeight, mc.Scale
assert (e, n, h) == (450000.0, 9960000.0, 0.0) and scale == 0.001, (e, n, h, scale)
worst = 0.0; count = 0
for p in plain.by_type("IfcProduct"):
    if p.ObjectPlacement is None: continue
    q = [x for x in geo.by_type("IfcProduct") if x.GlobalId == p.GlobalId]
    assert q, p.GlobalId
    m0 = pl.get_local_placement(p.ObjectPlacement); m1 = pl.get_local_placement(q[0].ObjectPlacement)
    # file millimetres -> map metres by Scale, then the origin: easting = e + x * scale
    dx = (e + m1[0][3] * scale) - (m0[0][3] * scale + 450000.0)
    dy = (n + m1[1][3] * scale) - (m0[1][3] * scale + 9960000.0)
    dz = m1[2][3] - m0[2][3]
    worst = max(worst, abs(dx) * 1000, abs(dy) * 1000, abs(dz)); count += 1
    assert abs(m1[0][3]) < 5e6 and abs(m1[1][3]) < 5e6, (p.GlobalId, m1[0][3], m1[1][3])
assert count > 10, count
print("ok", count, worst)
assert worst < 1e-3, worst
"#;
    let out = std::process::Command::new(python).arg("-c").arg(script).arg(&a).arg(&b).output().unwrap();
    assert!(out.status.success(), "{}\n{}", String::from_utf8_lossy(&out.stdout), String::from_utf8_lossy(&out.stderr));
}
