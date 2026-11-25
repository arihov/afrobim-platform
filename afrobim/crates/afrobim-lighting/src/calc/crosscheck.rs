// SPDX-License-Identifier: LicenseRef-AfroBIM-Proprietary
// Copyright (c) 2026 AfroBIM. All rights reserved.
//! Cross-check against an independent open-source reader, run only when it is available.
//!
//! The Python package `luxpy` (pip, GPL, https://github.com/ksmet1977/luxpy) has its own IES and
//! EULUMDAT readers and completes the symmetries itself. This test parses the same real files
//! with luxpy, compares luminous intensities at pseudo-random (C, gamma) directions with this
//! crate's, and recomputes the horizontal illuminance at grid points with numpy
//! (an independent re-implementation of the geometry written in the module documentation of
//! `frame.rs` and `engine.rs`, reading the intensity through luxpy's map and scipy's linear
//! interpolation) for comparison with `calculate`.
//!
//! Needs `AFROBIM_PHOTOMETRY_SAMPLES` (folder of real files) and `AFROBIM_PYTHONPATH` (a folder
//! holding luxpy, numpy, scipy, pandas, matplotlib). Skips with a message otherwise.

use std::io::Write;
use std::process::{Command, Stdio};

use super::*;
use crate::layout::tests::spec;
use crate::photometry::load_photometry_file;
use crate::{layout_single_column, BracketAim, LightingData, LightingDefinition, SingleColumn};

const SCRIPT: &str = r#"
import sys, json
inp = json.load(sys.stdin)
sys.path.insert(0, inp["pylib"])
import numpy as np
from luxpy.toolboxes import iolidfiles as io
from scipy.interpolate import RegularGridInterpolator

def own_ldt(path):
    # Used only for EULUMDAT files with non-equidistant C-planes (Dc = 0), where luxpy's symmetry
    # completion assumes equidistant planes. Written separately from the Rust reader, from the
    # DIALux evo knowledge base table of Mc1 and Mc2.
    T = open(path, encoding="utf-8-sig", errors="replace").read().splitlines()
    isym = int(T[2]); mc = int(T[3]); ng = int(T[5]); n = int(T[25])
    conv = float(T[23]); flux = sum(float(T[26 + 6 * k + 2]) for k in range(n))
    tk = " ".join(T[26 + 6 * n:]).split()
    cs = [float(x) for x in tk[10:10 + mc]]; gs = [float(x) for x in tk[10 + mc:10 + mc + ng]]
    npl = {0: mc, 1: 1, 2: mc // 2 + 1, 3: mc // 2 + 1, 4: mc // 4 + 1}[isym]
    v = np.array(tk[10 + mc + ng:10 + mc + ng + npl * ng], float).reshape(npl, ng) * conv * flux / 1000.0
    idx = [(3 * mc // 4 + k) % mc for k in range(npl)] if isym == 3 else list(range(npl))
    planes = {}
    for k, i in enumerate(idx):
        a = cs[i] if isym != 1 else 0.0
        imgs = [a]
        if isym == 2: imgs.append(360.0 - a)
        if isym == 3: imgs.append(180.0 - a)
        if isym == 4: imgs += [180.0 - a, 180.0 + a, 360.0 - a]
        for im in imgs:
            planes.setdefault(round(im % 360.0, 6), v[k])
    keys = sorted(planes)
    return np.array(gs), np.array(keys), np.array([planes[k] for k in keys])

def own_ies(path):
    # Used for IES files whose horizontal angles are not evenly spaced: luxpy completes the
    # symmetry by position in the list and misplaces planes there. Angle-based completion,
    # written separately from the Rust reader, after the Autodesk 3ds Max help page.
    T = open(path, encoding="latin1").read()
    body = T[T.upper().index("TILT="):].split("\n", 1)[1]
    tk = body.replace(",", " ").split()
    off = 0
    if T[T.upper().index("TILT="):].upper().startswith("TILT=INCLUDE"):
        off = 2 + 2 * int(float(tk[1]))
    tk = tk[off:]
    mult = float(tk[2]); nv = int(tk[3]); nh = int(tk[4])
    v = np.array(tk[13:13 + nv], float); h = np.array(tk[13 + nv:13 + nv + nh], float)
    c = np.array(tk[13 + nv + nh:13 + nv + nh + nv * nh], float).reshape(nh, nv) * mult
    planes = {}
    for k, a in enumerate(h):
        imgs = [a]
        if nh == 1: imgs = [0.0]
        elif h[0] == 0 and h[-1] == 90: imgs += [180.0 - a, 180.0 + a, 360.0 - a]
        elif h[0] == 0 and h[-1] == 180: imgs.append(360.0 - a)
        elif h[0] == 90 and h[-1] == 270: imgs.append(180.0 - a)
        for im in imgs:
            planes.setdefault(round(im % 360.0, 6), c[k])
    keys = sorted(planes)
    return v, np.array(keys), np.array([planes[k] for k in keys])

first = open(inp["file"], encoding="utf-8-sig", errors="replace").read().splitlines()
reader = "luxpy"
LX = None
if not inp["file"].lower().endswith(".ldt"):
    LX = io.read_lamp_data(inp["file"], normalize=None)
if inp["file"].lower().endswith(".ldt") and float(first[4]) == 0.0:
    reader = "own numpy reader (luxpy assumes equidistant C-planes)"
    th, ph, vals = own_ldt(inp["file"])
    is_ldt = True; scale = 1.0
elif LX is not None and np.ptp(np.diff(np.asarray(LX["h_angs"], float))) > 1e-6 if LX is not None and len(LX["h_angs"]) > 1 else False:
    reader = "own numpy reader (luxpy completes unevenly spaced planes by position)"
    th, ph, vals = own_ies(inp["file"])
    is_ldt = False; scale = 1.0
else:
    L = io.read_lamp_data(inp["file"], normalize=None)
    th = np.asarray(L["map"]["thetas"], float)
    ph = np.asarray(L["map"]["phis"], float)
    vals = np.asarray(L["map"]["values"], float)          # (phi, theta), file units
    is_ldt = inp["file"].lower().endswith(".ldt")
    scale = float(L["intensity"])                           # multiplier (norm = 1)
    if is_ldt:
        scale *= float(L["tflux"]) / 1000.0                 # cd/klm -> cd
order = np.argsort(ph, kind="stable")
ph = ph[order]; vals = vals[order]
keep = np.concatenate([[True], np.diff(ph) > 1e-9])
ph = ph[keep]; vals = vals[keep]
if ph[-1] < 360.0 - 1e-9:                                # close the circle: the last plane meets the first
    ph = np.append(ph, ph[0] + 360.0); vals = np.vstack([vals, vals[:1]])
f = RegularGridInterpolator((ph, th), vals * scale, method="linear", bounds_error=False, fill_value=0.0)

def intensity(c, g):
    c = np.mod(c, 360.0)
    return f(np.stack([c, g], axis=-1))

rng = np.random.default_rng(12345)
c = rng.uniform(0.0, 360.0, 300)
g = rng.uniform(0.0, 180.0, 300)
sample = intensity(c, g)

# Illuminance at the points from one luminaire: own re-implementation of the geometry.
lx, ly, lz, az, tilt, meas, c_toward, ccw = inp["lum"]
P = np.asarray(inp["points"], float)
d = np.stack([P[:, 0] - lx, P[:, 1] - ly, 0.0 - lz * np.ones(len(P))], axis=1)
dist = np.linalg.norm(d, axis=1)
u = d / dist[:, None]
street = np.array([np.cos(az), np.sin(az), 0.0])
left = np.array([-np.sin(az), np.cos(az), 0.0])
a = u @ street; b = u @ left; cz = u[:, 2]
delta = np.radians(tilt - meas)
ap = a * np.cos(delta) + cz * np.sin(delta)
cp = -a * np.sin(delta) + cz * np.cos(delta)
gamma = np.degrees(np.arccos(np.clip(-cp, -1, 1)))
alpha = np.degrees(np.arctan2(b, ap))
C = np.mod(c_toward + (1.0 if ccw else -1.0) * alpha, 360.0)
I = intensity(C, gamma)
E = I * (lz / dist) / dist**2
print(json.dumps({"c": c.tolist(), "g": g.tolist(), "i": sample.tolist(), "e": E.tolist(), "reader": reader}))
"#;

fn run_python(pylib: &str, file: &str, lum: [f64; 8], points: &[(f64, f64)]) -> Option<serde_json::Value> {
    let input = serde_json::json!({ "pylib": pylib, "file": file, "lum": lum, "points": points.iter().map(|p| [p.0, p.1]).collect::<Vec<_>>() });
    let mut child = Command::new("python3").args(["-I", "-c", SCRIPT]).stdin(Stdio::piped()).stdout(Stdio::piped()).stderr(Stdio::piped()).spawn().ok()?;
    child.stdin.take()?.write_all(input.to_string().as_bytes()).ok()?;
    let out = child.wait_with_output().ok()?;
    if !out.status.success() {
        eprintln!("python failed for {file}: {}", String::from_utf8_lossy(&out.stderr).lines().last().unwrap_or(""));
        return None;
    }
    serde_json::from_slice(&out.stdout).ok()
}

#[test]
fn intensities_and_illuminance_agree_with_luxpy_and_numpy_when_available() {
    let (Ok(dir), Ok(pylib)) = (std::env::var("AFROBIM_PHOTOMETRY_SAMPLES"), std::env::var("AFROBIM_PYTHONPATH")) else {
        eprintln!("skipped: AFROBIM_PHOTOMETRY_SAMPLES or AFROBIM_PYTHONPATH is not set");
        return;
    };
    let mut compared = 0;
    for name in ["luxpy_isym3.ldt", "luxpy_isym2.ldt", "eul_road.ldt", "rtappl_mirel.ldt", "luxpy_test_lid_file.ies", "sdl_t2u.ies", "nav_t3r.ies", "taman_street.ies", "gat080.ies"] {
        let path = std::path::Path::new(&dir).join(name);
        if !path.exists() {
            eprintln!("{name}: not present, skipped");
            continue;
        }
        let loaded = load_photometry_file(&path).unwrap();
        let phot = &loaded.photometry;

        // A single luminaire: column 5 m right of the alignment, bracket towards it, 10 m high,
        // tilt 5 degrees (so the tilt transform is exercised too).
        let a = crate::layout::tests::straight();
        let mut column = spec();
        column.tilt_deg = 5.0;
        let def = SingleColumn { mark: "J".to_string(), alignment: "A".to_string(), station_m: 150.0, offset_m: 5.0, aim: BracketAim::TowardAlignment, column, lighting_class: String::new() };
        let layout = layout_single_column(&a, &def).unwrap();
        let data = LightingData { definition: LightingDefinition::Column(def), layout };
        let mut s = CalcSettings {
            maintenance_factor: 1.0,
            contribution_distance_m: 1000.0,
            orientation: Orientation::default(),
            areas: vec![CalcArea {
                name: "G".to_string(),
                kind: AreaKind::Other,
                station_from_m: 110.0,
                station_to_m: 190.0,
                offset_from_m: -12.0,
                offset_to_m: 12.0,
                along: Division::Count(16),
                across: Division::Count(8),
            }],
            plane_level_m: None,
            vertical: None,
            observers: Vec::new(),
            eir_lane_width_m: None,
        };
        s.orientation.c_toward_carriageway_deg = 90.0;
        let (frame, _) = GridFrame::from_layout(&data.layout).unwrap();
        let result = calculate(phot, &data, &frame, &s, None).unwrap();
        let pts: Vec<(f64, f64)> = result.areas[0].points.iter().map(|p| (p.east_m, p.north_m)).collect();
        let inst = &luminaire_instances(&data)[0];
        let lum = [inst.x_m, inst.y_m, inst.z_m, inst.bracket_azimuth_rad, inst.tilt_deg, phot.measured_tilt_deg, 90.0, 1.0];
        let Some(py) = run_python(&pylib, path.to_str().unwrap(), lum, &pts) else {
            eprintln!("{name}: python/luxpy not usable, skipped");
            return;
        };
        let get = |k: &str| py[k].as_array().unwrap().iter().map(|v| v.as_f64().unwrap()).collect::<Vec<f64>>();
        let (c, g, i_py, e_py) = (get("c"), get("g"), get("i"), get("e"));
        let peak = phot.peak_intensity().candela;
        let mut worst_i = 0.0f64;
        for k in 0..c.len() {
            worst_i = worst_i.max((phot.intensity(c[k], g[k]) - i_py[k]).abs() / peak);
        }
        let e_rust = &result.areas[0].e_h_lx;
        let e_max = e_rust.iter().cloned().fold(0.0, f64::max);
        let worst_e = e_rust.iter().zip(&e_py).map(|(a, b)| (a - b).abs() / e_max).fold(0.0, f64::max);
        eprintln!(
            "{name:26} [{:5}] intensity: {} directions, worst difference {:.2e} of the peak; illuminance: {} points, worst difference {:.2e} of the maximum ({:.1} lx)",
            py["reader"].as_str().unwrap_or("?").split(' ').next().unwrap_or("?"),
            c.len(),
            worst_i,
            e_rust.len(),
            worst_e,
            e_max
        );
        assert!(worst_i < 1e-6, "{name}: intensity differs from luxpy by {worst_i} of the peak");
        assert!(worst_e < 1e-6, "{name}: illuminance differs from the numpy recomputation by {worst_e} of the maximum");
        compared += 1;
    }
    assert!(compared > 0, "no sample file was present");
}
