// SPDX-License-Identifier: LicenseRef-AfroBIM-Proprietary AND (MIT OR Apache-2.0)
// Copyright (c) 2026 AfroBIM. All rights reserved.
// Portions ported from BIMCraft (Copyright 2026 IPedrax and the BIMCraft contributors,
// MIT OR Apache-2.0) keep that licence; see the notice below and NOTICE.
//! The built-in DWG reader and writer, run in a child process of AfroBIM itself.
//!
//! Portions of this file are ported from BIMCraft's DWG bridge (crates/dwg/src/lib.rs):
//! Copyright 2026 IPedrax and the BIMCraft contributors, licensed MIT OR Apache-2.0.
//! See NOTICE at the repository root.
//!
//! `afrobim-dwg` turns a DWG into ASCII DXF text and DXF text into a DWG, through the `acadrust`
//! library. A DWG from outside is untrusted and the decoder cannot be interrupted: a file can make
//! it run for ever, or allocate until the system refuses and the process aborts, which nothing in
//! the process can catch. So the window never runs it. It starts this same program again as a
//! child with a hidden flag, handled at the very top of `main` before any window opens:
//!
//! * `afrobim --dwg-to-dxf <in.dwg> <out.dxf>` reads the DWG and writes the DXF, each note on what
//!   the read left out on its error stream as `note: ` and a line, and why on its error stream
//!   when it cannot.
//! * `afrobim --dxf-to-dwg <out.dwg> <version>` reads DXF text on its standard input and writes a
//!   DWG of `version` (`AC1018` … `AC1032`, or a year such as `2004`).
//!
//! The parent holds the child to a wall-clock limit and stops it when it runs over, caps the
//! sizes going in and coming out, and turns a crash, a stop or a refusal into a message. Reading
//! goes through the reader's DWG order (built-in first, then ODA File Converter; see
//! `afrobim_dxf::reader::dwg`), which also keeps the DXF in the cache folder so a repeat open is
//! not converted again.

use std::ffi::{OsStr, OsString};
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::process::{Command, ExitStatus, Stdio};
use std::time::{Duration, Instant};

use afrobim_dwg::{DwgVersion, Limits};
use afrobim_dxf::reader::{self, ReadError, Underlay};

/// The hidden flag that makes AfroBIM the child that reads one DWG.
pub const READ_ARG: &str = "--dwg-to-dxf";
/// The hidden flag that makes AfroBIM the child that writes one DWG.
pub const WRITE_ARG: &str = "--dxf-to-dwg";
/// The longest a child may take to read one DWG.
pub const READ_TIME: Duration = Duration::from_secs(300);
/// The longest a child may take to write one DWG.
pub const WRITE_TIME: Duration = Duration::from_secs(120);
/// The most a child may say on its error stream.
const MAX_SAID: u64 = 64 << 10;

// ---------- The child ----------

/// What AfroBIM does when started as a DWG child (`args`: its arguments after its own name): the
/// exit code to end with. `None` when the arguments are not a child's, and AfroBIM goes on as
/// itself.
pub fn child_main(args: &[OsString]) -> Option<i32> {
    match args {
        [flag, input, output] if flag == READ_ARG => Some(read_child(Path::new(input), Path::new(output))),
        [flag, output, version] if flag == WRITE_ARG => Some(write_child(Path::new(output), &version.to_string_lossy())),
        _ => None,
    }
}

fn read_child(input: &Path, output: &Path) -> i32 {
    let mut err = std::io::stderr();
    let limits = Limits::DEFAULT;
    let converted = std::fs::metadata(input)
        .map_err(|e| format!("{}: {e}", input.display()))
        .and_then(|m| {
            if m.len() > limits.dwg_bytes as u64 {
                Err(format!("the file is {}, larger than the {} of DWG AfroBIM reads", mb(m.len()), mb(limits.dwg_bytes as u64)))
            } else {
                std::fs::read(input).map_err(|e| format!("{}: {e}", input.display()))
            }
        })
        .and_then(|bytes| afrobim_dwg::dwg_to_dxf_with(&bytes, limits));
    match converted.and_then(|c| std::fs::write(output, c.dxf.as_bytes()).map(|()| c.notes).map_err(|e| format!("{}: {e}", output.display()))) {
        Ok(notes) => {
            for note in notes {
                let _ = writeln!(err, "note: {}", note.replace(['\r', '\n'], " "));
            }
            0
        }
        Err(e) => {
            let _ = writeln!(err, "{}", e.replace(['\r', '\n'], " "));
            1
        }
    }
}

fn write_child(output: &Path, version: &str) -> i32 {
    let mut err = std::io::stderr();
    let limits = Limits::DEFAULT;
    let written = DwgVersion::parse(version)
        .ok_or_else(|| format!("{version} is not a DWG version AfroBIM writes (AC1018 to AC1032)"))
        .and_then(|v| {
            let mut dxf = Vec::new();
            std::io::stdin().lock().take(limits.dxf_bytes as u64 + 1).read_to_end(&mut dxf).map_err(|e| e.to_string())?;
            let text = String::from_utf8(dxf).map_err(|_| "the drawing's DXF is not UTF-8 text".to_string())?;
            afrobim_dwg::dxf_to_dwg_with(&text, v, limits)
        })
        .and_then(|dwg| std::fs::write(output, dwg).map_err(|e| format!("{}: {e}", output.display())));
    match written {
        Ok(()) => 0,
        Err(e) => {
            let _ = writeln!(err, "{}", e.replace(['\r', '\n'], " "));
            1
        }
    }
}

// ---------- The parent ----------

/// The program started as the child, and how.
#[derive(Clone, Debug)]
pub struct Helper {
    exe: PathBuf,
    /// Arguments before the child's own (none for AfroBIM itself).
    lead: Vec<OsString>,
    /// Extra environment for the child.
    envs: Vec<(OsString, OsString)>,
    /// When set, the child's arguments go in this environment variable instead of on its command
    /// line (the test harness takes no arguments of ours).
    args_env: Option<&'static str>,
}

impl Helper {
    /// This program itself.
    #[cfg(not(test))]
    pub fn this_program() -> Result<Helper, String> {
        let exe = std::env::current_exe().map_err(|e| format!("AfroBIM could not find its own program to start the DWG reader ({e})"))?;
        Ok(Helper { exe, lead: Vec::new(), envs: Vec::new(), args_env: None })
    }

    /// Under `cargo test` this program is the test harness, which knows no `--dwg-to-dxf`: it is
    /// started on [`tests::child_entry`] instead, with the child's arguments in the environment.
    #[cfg(test)]
    pub fn this_program() -> Result<Helper, String> {
        let exe = std::env::current_exe().map_err(|e| e.to_string())?;
        let lead = ["dwg_bridge::tests::child_entry", "--exact", "--ignored", "--nocapture", "--test-threads=1", "-q"].map(OsString::from).to_vec();
        Ok(Helper { exe, lead, envs: Vec::new(), args_env: Some(tests::CHILD_ARGS_ENV) })
    }

    fn command(&self, args: &[&OsStr]) -> Command {
        let mut c = Command::new(&self.exe);
        c.args(&self.lead);
        match self.args_env {
            Some(var) => {
                let joined: Vec<String> = args.iter().map(|a| a.to_string_lossy().into_owned()).collect();
                c.env(var, joined.join("\u{1f}"));
            }
            None => {
                c.args(args);
            }
        }
        c.envs(self.envs.iter().map(|(k, v)| (k, v)));
        c
    }
}

/// How a child ended: its status (none when it was stopped), and what it said on its error stream.
struct Ran {
    status: Option<ExitStatus>,
    said: Vec<u8>,
}

impl Ran {
    /// Its notes (`note: ` lines) and the rest of what it said: why it failed.
    fn said(&self) -> (Vec<String>, Vec<String>) {
        let said = String::from_utf8_lossy(&self.said);
        let (notes, why): (Vec<&str>, Vec<&str>) = said.lines().map(str::trim).filter(|l| !l.is_empty()).partition(|l| l.starts_with("note: "));
        (notes.iter().map(|l| l.trim_start_matches("note: ").to_string()).collect(), why.iter().map(|l| l.to_string()).collect())
    }
}

/// Runs the child with `args`, `input` on its standard input, for at most `time`: past that it is
/// killed. Its error stream is read while it runs so it never fills and stalls the child.
fn run(helper: &Helper, args: &[&OsStr], input: Option<Vec<u8>>, time: Duration) -> std::io::Result<Ran> {
    let mut child = helper
        .command(args)
        .stdin(if input.is_some() { Stdio::piped() } else { Stdio::null() })
        .stdout(Stdio::null())
        .stderr(Stdio::piped())
        .spawn()?;
    // Fed on a thread of its own, so a child that stops reading cannot hold the parent.
    let feed = child.stdin.take().zip(input).map(|(mut stdin, bytes)| {
        std::thread::spawn(move || {
            let _ = stdin.write_all(&bytes);
        })
    });
    let said = child.stderr.take().map(|s| {
        std::thread::spawn(move || {
            let mut buf = Vec::new();
            let mut s = s.take(MAX_SAID);
            let _ = s.read_to_end(&mut buf);
            // Keep draining past the cap so the child is never blocked on a full pipe.
            let _ = std::io::copy(&mut s.into_inner(), &mut std::io::sink());
            buf
        })
    });
    let start = Instant::now();
    let status = loop {
        match child.try_wait() {
            Ok(Some(status)) => break Some(status),
            Ok(None) if start.elapsed() < time => std::thread::sleep(Duration::from_millis(20)),
            Ok(None) | Err(_) => {
                let _ = child.kill();
                let _ = child.wait();
                break None;
            }
        }
    };
    if let Some(f) = feed {
        let _ = f.join();
    }
    let said = said.and_then(|t| t.join().ok()).unwrap_or_default();
    Ok(Ran { status, said })
}

fn seconds(time: Duration) -> String {
    if time >= Duration::from_secs(1) {
        format!("{} s", time.as_secs())
    } else {
        format!("{:.1} s", time.as_secs_f64())
    }
}

/// Converts the DWG at `dwg` to an ASCII DXF at `out` in a child, within `time`, and returns the
/// child's notes on what it left out. `out` is removed again when the conversion fails.
pub fn dwg_to_dxf_file(helper: &Helper, dwg: &Path, out: &Path, time: Duration) -> Result<Vec<String>, String> {
    let limits = Limits::DEFAULT;
    let len = std::fs::metadata(dwg).map_err(|e| format!("{}: {e}", dwg.display()))?.len();
    if len > limits.dwg_bytes as u64 {
        return Err(format!("the file is {}, larger than the {} of DWG AfroBIM reads", mb(len), mb(limits.dwg_bytes as u64)));
    }
    let _ = std::fs::remove_file(out);
    let ran = run(helper, &[READ_ARG.as_ref(), dwg.as_os_str(), out.as_os_str()], None, time).map_err(|e| format!("the DWG reader could not be started ({e})"))?;
    let (notes, why) = ran.said();
    let result = match ran.status {
        None => Err(format!("the DWG reader took longer than {} on this file and was stopped", seconds(time))),
        Some(s) if s.success() => match std::fs::metadata(out) {
            Ok(m) if m.len() > limits.dxf_bytes as u64 => Err(format!("this DWG became more than {} of DXF, the most AfroBIM reads", mb(limits.dxf_bytes as u64))),
            Ok(m) if m.len() > 0 => Ok(notes),
            _ => Err("the DWG reader finished but wrote no DXF".to_string()),
        },
        Some(_) if !why.is_empty() => Err(why.join("; ")),
        Some(s) => Err(format!("the DWG reader stopped on this file ({s}); the file may be damaged")),
    };
    if result.is_err() {
        let _ = std::fs::remove_file(out);
    }
    result
}

/// The DWG of `version` that DXF text becomes, written by a child within `time`.
pub fn dxf_to_dwg_bytes(helper: &Helper, dxf: &str, version: DwgVersion, time: Duration) -> Result<Vec<u8>, String> {
    let limits = Limits::DEFAULT;
    if dxf.len() > limits.dxf_bytes {
        return Err(format!("the drawing is {} of DXF, more than the {} AfroBIM writes as DWG", mb(dxf.len() as u64), mb(limits.dxf_bytes as u64)));
    }
    static NEXT: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
    let n = NEXT.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    let out = std::env::temp_dir().join(format!("afrobim-dwg-write-{}-{n}.dwg", std::process::id()));
    let _ = std::fs::remove_file(&out);
    let ran = run(helper, &[WRITE_ARG.as_ref(), out.as_os_str(), version.code().as_ref()], Some(dxf.as_bytes().to_vec()), time).map_err(|e| format!("the DWG writer could not be started ({e})"));
    let result = ran.and_then(|ran| {
        let (_, why) = ran.said();
        match ran.status {
            None => Err(format!("the DWG writer took longer than {} and was stopped", seconds(time))),
            Some(s) if s.success() => {
                let len = std::fs::metadata(&out).map(|m| m.len()).unwrap_or(0);
                if len > limits.dwg_bytes as u64 {
                    return Err(format!("the DWG came out larger than the {} AfroBIM writes", mb(limits.dwg_bytes as u64)));
                }
                let bytes = std::fs::read(&out).map_err(|e| format!("the DWG writer's file could not be read back ({e})"))?;
                if bytes.starts_with(version.code().as_bytes()) {
                    Ok(bytes)
                } else {
                    Err("the DWG writer gave back no DWG".to_string())
                }
            }
            Some(_) if !why.is_empty() => Err(why.join("; ")),
            Some(s) => Err(format!("the DWG writer stopped on this drawing ({s})")),
        }
    });
    let _ = std::fs::remove_file(&out);
    result
}

/// Reads an underlay file: a DXF directly, a DWG with the built-in reader first and ODA File
/// Converter as the fallback (see the module documentation).
pub fn read_underlay(path: &Path) -> Result<Underlay, ReadError> {
    read_underlay_with(path, Helper::this_program(), READ_TIME)
}

fn read_underlay_with(path: &Path, helper: Result<Helper, String>, time: Duration) -> Result<Underlay, ReadError> {
    let convert = |dwg: &Path, out: &Path| helper.clone().and_then(|h| dwg_to_dxf_file(&h, dwg, out, time));
    reader::read_file_with(path, Some(&convert))
}

/// The DWG version exported: AutoCAD 2018 (AC1032), or the one `AFROBIM_DWG_EXPORT_VERSION` names
/// (`2004`, `AC1018` …).
pub fn export_version() -> DwgVersion {
    std::env::var("AFROBIM_DWG_EXPORT_VERSION").ok().and_then(|v| DwgVersion::parse(&v)).unwrap_or(DwgVersion::R2018)
}

/// The DWG that the exporter's DXF text becomes, written by a child of this program.
pub fn export_dwg(dxf: &str, version: DwgVersion) -> Result<Vec<u8>, String> {
    dxf_to_dwg_bytes(&Helper::this_program()?, dxf, version, WRITE_TIME)
}

fn mb(n: u64) -> String {
    format!("{:.1} MB", n as f64 / f64::from(1u32 << 20))
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;

    /// The environment variable that carries a test child's arguments.
    pub const CHILD_ARGS_ENV: &str = "AFROBIM_DWG_TEST_CHILD";
    /// Makes a test child sleep instead of working, for the timeout test.
    const SLEEP_ENV: &str = "AFROBIM_DWG_TEST_SLEEP";
    /// Makes a test child abort, as a decoder brought down by a hostile file would.
    const ABORT_ENV: &str = "AFROBIM_DWG_TEST_ABORT";

    /// The child, when the test harness is started as one (see [`Helper::this_program`]). Run on
    /// its own it does nothing.
    #[test]
    #[ignore = "the DWG child process entry; runs only when started as a child"]
    fn child_entry() {
        let Ok(joined) = std::env::var(CHILD_ARGS_ENV) else { return };
        if std::env::var_os(SLEEP_ENV).is_some() {
            std::thread::sleep(Duration::from_secs(120));
        }
        if std::env::var_os(ABORT_ENV).is_some() {
            std::process::abort();
        }
        let args: Vec<OsString> = joined.split('\u{1f}').map(OsString::from).collect();
        std::process::exit(child_main(&args).unwrap_or(2));
    }

    fn scratch(name: &str) -> PathBuf {
        let d = std::env::temp_dir().join(format!("afrobim-dwg-bridge-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&d);
        std::fs::create_dir_all(&d).unwrap();
        d
    }

    fn small_dxf() -> String {
        let mut d = afrobim_dxf::DxfDocument::new();
        d.add_line("ROAD-EDGE", 1, afrobim_draw::Pt2::new(0.0, 0.0), afrobim_draw::Pt2::new(10_000.0, 0.0));
        d.add_arc("KERB", 3, afrobim_draw::Pt2::new(5_000.0, 2_000.0), 1_500.0, 0.0, 90.0);
        d.add_polyline("CL", 2, &[afrobim_draw::Pt2::new(0.0, 1.0), afrobim_draw::Pt2::new(5.0, 1.0), afrobim_draw::Pt2::new(9.0, 3.0)], false, 0.0);
        d.add_text(&afrobim_dxf::DxfText::new("CH 0+000", afrobim_draw::Pt2::new(0.0, -500.0), 250.0, "TEXT"));
        d.finish()
    }

    #[test]
    fn the_hidden_flags_are_recognised_and_anything_else_is_left_to_the_app() {
        let a = |v: &[&str]| v.iter().map(OsString::from).collect::<Vec<_>>();
        assert_eq!(child_main(&a(&[])), None);
        assert_eq!(child_main(&a(&["--open", "x.afrobim"])), None);
        assert_eq!(child_main(&a(&[READ_ARG, "only-one"])), None);
        // A write child with a version it does not know says so and fails.
        assert_eq!(child_main(&a(&[WRITE_ARG, "/nowhere/x.dwg", "1999"])), Some(1));
        // A read child on a missing file fails rather than opening a window.
        assert_eq!(child_main(&a(&[READ_ARG, "/no/such/file.dwg", "/nowhere/out.dxf"])), Some(1));
    }

    /// The child writes a DWG from the exporter's DXF and reads it back as DXF, and the underlay
    /// reader takes the built-in route and says so.
    #[test]
    fn the_child_bridge_converts_both_ways() {
        let d = scratch("both-ways");
        let helper = Helper::this_program().unwrap();
        let dwg = dxf_to_dwg_bytes(&helper, &small_dxf(), DwgVersion::R2018, WRITE_TIME).unwrap();
        assert!(dwg.starts_with(b"AC1032"));
        let path = d.join("site.dwg");
        std::fs::write(&path, &dwg).unwrap();
        let out = d.join("site.dxf");
        let notes = dwg_to_dxf_file(&helper, &path, &out, READ_TIME).unwrap();
        assert!(notes.is_empty(), "{notes:?}");
        let u = reader::read_file(&out).unwrap();
        assert_eq!(u.report.read.get("LINE"), Some(&1), "{:?}", u.report.read);
        assert_eq!(u.report.read.get("ARC"), Some(&1), "{:?}", u.report.read);
        // Through the underlay reader: built-in first, and the report says so. A fresh DWG path
        // per run keeps the shared cache from answering for an earlier one.
        let u = read_underlay_with(&path, Ok(helper.clone()), READ_TIME).unwrap();
        assert!(u.report.warnings.iter().any(|w| w.contains("built-in DWG reader")), "{:?}", u.report.warnings);
        assert_eq!(u.report.total_read(), 4, "{:?}", u.report.read);
        // And a 2004 DWG for the older programs.
        assert!(dxf_to_dwg_bytes(&helper, &small_dxf(), DwgVersion::R2004, WRITE_TIME).unwrap().starts_with(b"AC1018"));
        let _ = std::fs::remove_dir_all(&d);
    }

    #[test]
    fn a_child_that_runs_over_its_time_is_killed() {
        let d = scratch("timeout");
        let path = d.join("slow.dwg");
        std::fs::write(&path, afrobim_dwg::dxf_to_dwg(&small_dxf(), DwgVersion::R2018).unwrap()).unwrap();
        let mut helper = Helper::this_program().unwrap();
        helper.envs.push((SLEEP_ENV.into(), "1".into()));
        let out = d.join("slow.dxf");
        let start = Instant::now();
        let e = dwg_to_dxf_file(&helper, &path, &out, Duration::from_millis(300)).unwrap_err();
        assert!(e.contains("took longer than 0.3 s") && e.contains("stopped"), "{e}");
        assert!(start.elapsed() < Duration::from_secs(30), "the parent waited {:?}", start.elapsed());
        assert!(!out.exists());
        let e = dxf_to_dwg_bytes(&helper, &small_dxf(), DwgVersion::R2018, Duration::from_millis(300)).unwrap_err();
        assert!(e.contains("stopped"), "{e}");
        let _ = std::fs::remove_dir_all(&d);
    }

    #[test]
    fn a_child_that_crashes_is_reported_as_a_damaged_file() {
        let d = scratch("crash");
        let path = d.join("hostile.dwg");
        std::fs::write(&path, b"AC1032 not really").unwrap();
        let mut helper = Helper::this_program().unwrap();
        helper.envs.push((ABORT_ENV.into(), "1".into()));
        let out = d.join("hostile.dxf");
        let e = dwg_to_dxf_file(&helper, &path, &out, READ_TIME).unwrap_err();
        assert!(e.contains("stopped on this file") || e.contains("abort"), "{e}");
        assert!(!out.exists());
        let _ = std::fs::remove_dir_all(&d);
    }

    #[test]
    fn a_dwg_the_child_refuses_says_why() {
        let d = scratch("refused");
        let path = d.join("junk.dwg");
        std::fs::write(&path, b"AC1032\0\0\0\0\0\0\0\0\0\0").unwrap();
        let e = dwg_to_dxf_file(&Helper::this_program().unwrap(), &path, &d.join("junk.dxf"), READ_TIME).unwrap_err();
        assert!(e.contains("DWG"), "{e}");
        let _ = std::fs::remove_dir_all(&d);
    }
}
