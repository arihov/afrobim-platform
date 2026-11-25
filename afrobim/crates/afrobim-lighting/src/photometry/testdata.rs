// SPDX-License-Identifier: LicenseRef-AfroBIM-Proprietary
// Copyright (c) 2026 AfroBIM. All rights reserved.
//! Small synthetic photometric files for the committed tests. They are written by hand for
//! these tests (no manufacturer data), in the layouts the cited descriptions give.

use std::fmt::Write;

/// An IES file in the 2002 layout. `rows[k]` are the candela values of horizontal angle `horiz[k]`.
#[allow(clippy::too_many_arguments)]
pub fn ies(version: &str, tilt: &str, lamps: f64, lumens: f64, multiplier: f64, ptype: i32, units: i32, dims: [f64; 3], ballast: [f64; 3], vert: &[f64], horiz: &[f64], rows: &[Vec<f64>]) -> String {
    let mut s = String::new();
    if !version.is_empty() {
        writeln!(s, "{version}").unwrap();
    }
    writeln!(s, "[TEST] T-1\n[MANUFAC] Test maker\n[LUMCAT] CAT-1\n[LUMINAIRE] Test luminaire").unwrap();
    writeln!(s, "{tilt}").unwrap();
    if tilt.ends_with("INCLUDE") {
        writeln!(s, "1\n2\n0 90\n1.0 0.9").unwrap();
    }
    writeln!(s, "{lamps} {lumens} {multiplier} {} {} {ptype} {units}", vert.len(), horiz.len()).unwrap();
    writeln!(s, "{} {} {}", dims[0], dims[1], dims[2]).unwrap();
    writeln!(s, "{} {} {}", ballast[0], ballast[1], ballast[2]).unwrap();
    writeln!(s, "{}", vert.iter().map(|v| v.to_string()).collect::<Vec<_>>().join(" ")).unwrap();
    writeln!(s, "{}", horiz.iter().map(|v| v.to_string()).collect::<Vec<_>>().join(" ")).unwrap();
    for r in rows {
        writeln!(s, "{}", r.iter().map(|v| v.to_string()).collect::<Vec<_>>().join(" ")).unwrap();
    }
    s
}

/// A plain IES type C file with defaults: 1 lamp of 1000 lm, multiplier 1, metres.
pub fn ies_simple(horiz: &[f64], vert: &[f64], rows: &[Vec<f64>]) -> String {
    ies("IESNA:LM-63-2002", "TILT=NONE", 1.0, 1000.0, 1.0, 1, 2, [0.2, 0.4, 0.1], [1.0, 1.0, 50.0], vert, horiz, rows)
}

/// A EULUMDAT file. `c` is the whole angle list (Mc entries), `values` the stored planes in order.
#[allow(clippy::too_many_arguments)]
pub fn ldt(isym: u32, mc: usize, c: &[f64], gamma: &[f64], values: &[Vec<f64>], lamps: i32, flux: f64, conversion: f64, lorl: f64, tilt: f64) -> String {
    let mut s = String::new();
    writeln!(s, "Test maker\n3\n{isym}\n{mc}\n0\n{}\n0\nREP-1\nTest luminaire\nL-1\nfile.ldt\ndate", gamma.len()).unwrap();
    writeln!(s, "600\n300\n100\n500\n250\n0\n0\n0\n0\n95\n{lorl}\n{conversion}\n{tilt}\n1").unwrap();
    writeln!(s, "{lamps}\nLED\n{flux}\n4000\n80\n50").unwrap();
    for _ in 0..10 {
        writeln!(s, "0.5").unwrap();
    }
    for a in c {
        writeln!(s, "{a}").unwrap();
    }
    for g in gamma {
        writeln!(s, "{g}").unwrap();
    }
    for r in values {
        for v in r {
            writeln!(s, "{v}").unwrap();
        }
    }
    s
}
