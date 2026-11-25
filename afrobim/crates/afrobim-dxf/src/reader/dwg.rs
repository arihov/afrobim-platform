// SPDX-License-Identifier: LicenseRef-AfroBIM-Proprietary
// Copyright (c) 2026 AfroBIM. All rights reserved.
//! Opening a DWG: AfroBIM's built-in reader first, ODA File Converter as the fallback.
//!
//! The order, when a DWG is attached or dropped on the window (see [`super::read_file_with`]):
//!
//! 1. **The built-in reader.** The application hands the reader a converter ([`Builtin`]) that turns
//!    the DWG into an ASCII DXF without any other program: the `afrobim-dwg` crate, through the
//!    `acadrust` library (MPL-2.0, pure Rust), run in a child process of AfroBIM itself with a time
//!    limit (`afrobim-app/src/dwg_bridge.rs`), so a hostile or broken file cannot take the window
//!    down. Its DXF goes to `<cache>/builtin/drawing.dxf`. It counts as failed when the child fails,
//!    when the DXF it wrote cannot be read, or when it holds no entities and ODA File Converter is
//!    there to try. Set `AFROBIM_DWG_BUILTIN=0` to switch it off.
//! 2. **ODA File Converter**, when the built-in reader is off or failed and the converter is
//!    installed. Its DXF goes to `<cache>/out/drawing.dxf`.
//!
//! The underlay's report says which route was used, and when both fail the error gives both
//! reasons. A program that calls [`super::read_file`] (no built-in converter) gets ODA File
//! Converter alone, as before.
//!
//! ODA File Converter (free, from the Open Design Alliance,
//! <https://www.opendesign.com/guestfiles/oda_file_converter>) is never bundled with AfroBIM: it is
//! found where the user installed it. Where it is looked for, in order:
//!
//! 1. `AFROBIM_ODA_CONVERTER`: the program, or on macOS the `.app` folder. Set it to an empty
//!    value to switch conversion off.
//! 2. The usual install places: `/Applications/ODAFileConverter.app` and
//!    `~/Applications/ODAFileConverter.app` on macOS; `C:\Program Files\ODA\ODAFileConverter*`
//!    on Windows; `/usr/bin/ODAFileConverter` and `/opt/ODAFileConverter*` on Linux.
//! 3. `ODAFileConverter` on the `PATH`.
//!
//! The converter's command line, as its download page describes it: source folder, target
//! folder, output version, output type, recursive flag, audit flag, file filter. It converts
//! every matching file in the source folder, so the DWG is copied alone into a folder of its own
//! first. The arguments used are `ACAD2018 DXF 0 1 *.DWG`: DXF of the 2018 format, not recursive,
//! with ODA's audit and repair on loading.
//!
//! The cache is `<temp>/afrobim-dwg/<key>/`, the key made from the DWG's full path, size and
//! modification time, so a changed drawing is converted again and an unchanged one is not. Each
//! route has its own file in the key's folder, so a cached DXF is always reported by the route
//! that made it. The built-in reader writes to a `.part` file and renames it only when the child
//! finished, so a child that was stopped never leaves a half DXF to be taken for a whole one. The
//! source DWG is only read. A converted DXF is several times the size of its DWG (a 17 MB Civil 3D
//! drawing gave 142 MB through ODA).
//!
//! What a DXF from ODA holds: everything AutoCAD itself draws, and the objects of other programs
//! (Civil 3D alignments, surfaces, corridors, points, labels …) as proxy entities with the picture
//! their program saved, which the reader draws (see [`super::proxy`]). What the built-in reader
//! keeps of such objects depends on acadrust; see the module notes in `afrobim-dwg`.

use std::path::{Path, PathBuf};
use std::process::Command;

/// Why a DWG could not be opened.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum DwgError {
    #[error("{0}")]
    NoConverter(String),
    #[error("ODA File Converter could not convert {path}: {why}")]
    Failed { path: String, why: String },
    #[error("{0}")]
    Io(String),
}

const MACOS_EXECUTABLE: &str = "Contents/MacOS/ODAFileConverter";

/// The program inside a path that names the converter: the path itself, or for a macOS `.app`
/// folder the executable inside it.
fn program_in(path: &Path) -> PathBuf {
    if path.is_dir() {
        path.join(MACOS_EXECUTABLE)
    } else {
        path.to_path_buf()
    }
}

/// The places looked at after the environment variable, for this system.
fn usual_places() -> Vec<PathBuf> {
    let mut out = Vec::new();
    if cfg!(target_os = "macos") {
        out.push(PathBuf::from("/Applications/ODAFileConverter.app").join(MACOS_EXECUTABLE));
        if let Some(home) = std::env::var_os("HOME") {
            out.push(PathBuf::from(home).join("Applications/ODAFileConverter.app").join(MACOS_EXECUTABLE));
        }
    } else if cfg!(windows) {
        for root in [std::env::var_os("ProgramFiles"), Some("C:\\Program Files".into())].into_iter().flatten() {
            let oda = PathBuf::from(root).join("ODA");
            if let Ok(dirs) = std::fs::read_dir(&oda) {
                let mut found: Vec<PathBuf> = dirs.flatten().map(|d| d.path().join("ODAFileConverter.exe")).collect();
                // The newest version folder first.
                found.sort();
                found.reverse();
                out.extend(found);
            }
        }
    } else {
        out.push(PathBuf::from("/usr/bin/ODAFileConverter"));
        if let Ok(dirs) = std::fs::read_dir("/opt") {
            let mut found: Vec<PathBuf> =
                dirs.flatten().filter(|d| d.file_name().to_string_lossy().starts_with("ODAFileConverter")).map(|d| d.path().join("ODAFileConverter")).collect();
            found.sort();
            found.reverse();
            out.extend(found);
        }
    }
    out
}

fn on_path(name: &str) -> Option<PathBuf> {
    let path = std::env::var_os("PATH")?;
    std::env::split_paths(&path).map(|dir| dir.join(name)).find(|p| p.is_file())
}

/// Where ODA File Converter is, or `None`. See the module documentation for the order.
pub fn find_converter() -> Option<PathBuf> {
    find_converter_with(std::env::var_os("AFROBIM_ODA_CONVERTER").map(PathBuf::from))
}

/// [`find_converter`] with the environment variable's value given (for tests).
pub fn find_converter_with(setting: Option<PathBuf>) -> Option<PathBuf> {
    match setting {
        Some(p) if p.as_os_str().is_empty() => return None,
        Some(p) => {
            let program = program_in(&p);
            return program.is_file().then_some(program);
        }
        None => {}
    }
    usual_places().into_iter().find(|p| p.is_file()).or_else(|| on_path(if cfg!(windows) { "ODAFileConverter.exe" } else { "ODAFileConverter" }))
}

/// What to tell the user about opening a DWG: how it is done and what to do without a converter.
pub fn install_hint() -> String {
    let found = if find_converter().is_some() { "" } else { " It was not found on this computer." };
    format!(
        "AfroBIM opens a DWG from its file with its built-in DWG reader, and otherwise by converting it to DXF with ODA File \
         Converter.{found} Install it free from https://www.opendesign.com/guestfiles/oda_file_converter, or set \
         AFROBIM_ODA_CONVERTER to where it is. Otherwise convert it to DXF (save as DXF from AutoCAD or Civil 3D) and open the DXF."
    )
}

/// A built-in DWG converter, handed in by the application: it reads the DWG at the first path and
/// writes an ASCII DXF to the second, returning notes on what it left out, or why it could not.
pub type Builtin<'a> = &'a dyn Fn(&Path, &Path) -> Result<Vec<String>, String>;

/// Whether the built-in DWG reader is on: it is unless `AFROBIM_DWG_BUILTIN` is `0`, `off`,
/// `false` or `no`.
pub fn builtin_enabled() -> bool {
    builtin_enabled_with(std::env::var("AFROBIM_DWG_BUILTIN").ok().as_deref())
}

/// [`builtin_enabled`] with the environment variable's value given (for tests).
pub fn builtin_enabled_with(setting: Option<&str>) -> bool {
    !matches!(setting.map(|s| s.trim().to_ascii_lowercase()).as_deref(), Some("0" | "off" | "false" | "no"))
}

/// Where the built-in reader's DXF of a DWG is kept, inside the DWG's cache folder.
pub fn builtin_dxf(dir: &Path) -> PathBuf {
    dir.join("builtin").join("drawing.dxf")
}

/// Converts a DWG with the built-in converter into its cache folder under `cache_root` and returns
/// the DXF with the converter's notes. A DXF already in the cache is returned without converting
/// again, with the notes kept from that conversion.
pub fn convert_builtin_with(convert: Builtin<'_>, dwg: &Path, cache_root: &Path) -> Result<(PathBuf, Vec<String>), String> {
    let dir = cache_dir(dwg, cache_root).map_err(|e| e.to_string())?;
    let dxf = builtin_dxf(&dir);
    let notes_file = dxf.with_file_name("notes.txt");
    let read_notes = || std::fs::read_to_string(&notes_file).map(|s| s.lines().filter(|l| !l.trim().is_empty()).map(str::to_string).collect()).unwrap_or_default();
    if std::fs::metadata(&dxf).is_ok_and(|m| m.len() > 0) {
        return Ok((dxf, read_notes()));
    }
    let parent = dxf.parent().unwrap_or(&dir);
    std::fs::create_dir_all(parent).map_err(|e| format!("{}: {e}", parent.display()))?;
    let part = dxf.with_extension("dxf.part");
    let _ = std::fs::remove_file(&part);
    let result = convert(dwg, &part);
    let fail = |why: String| {
        let _ = std::fs::remove_file(&part);
        Err(why)
    };
    let notes = match result {
        Ok(notes) => notes,
        Err(why) => return fail(why),
    };
    if !std::fs::metadata(&part).is_ok_and(|m| m.len() > 0) {
        return fail("it wrote no DXF".into());
    }
    if let Err(e) = std::fs::rename(&part, &dxf) {
        return fail(format!("{}: {e}", dxf.display()));
    }
    let _ = std::fs::write(&notes_file, notes.join("\n"));
    Ok((dxf, notes))
}

/// Removes the built-in reader's cached DXF of a DWG (when the reader could not use it), so the
/// next open converts again.
pub fn forget_builtin(dwg: &Path, cache_root: &Path) {
    if let Ok(dir) = cache_dir(dwg, cache_root) {
        let _ = std::fs::remove_dir_all(builtin_dxf(&dir).parent().unwrap_or(&dir));
    }
}

/// FNV-1a 64 of some bytes.
fn fnv1a(bytes: &[u8]) -> u64 {
    bytes.iter().fold(0xcbf2_9ce4_8422_2325, |h, b| (h ^ u64::from(*b)).wrapping_mul(0x0100_0000_01b3))
}

/// The cache folder of a DWG under `cache_root`: keyed by its full path, size and modification time.
pub fn cache_dir(dwg: &Path, cache_root: &Path) -> Result<PathBuf, DwgError> {
    let meta = std::fs::metadata(dwg).map_err(|e| DwgError::Io(format!("{}: {e}", dwg.display())))?;
    let full = dwg.canonicalize().unwrap_or_else(|_| dwg.to_path_buf());
    let modified = meta.modified().ok().and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok()).map(|d| d.as_nanos()).unwrap_or(0);
    let key = format!("{}|{}|{modified}", full.display(), meta.len());
    Ok(cache_root.join(format!("{:016x}", fnv1a(key.as_bytes()))))
}

/// The default cache root, `<temp>/afrobim-dwg`.
pub fn default_cache_root() -> PathBuf {
    std::env::temp_dir().join("afrobim-dwg")
}

/// Converts a DWG to DXF with the converter at `program`, into the DWG's cache folder under
/// `cache_root`, and returns the DXF. A DXF already in the cache for the same file is returned
/// without converting again.
pub fn convert_with(program: &Path, dwg: &Path, cache_root: &Path) -> Result<PathBuf, DwgError> {
    let dir = cache_dir(dwg, cache_root)?;
    let (input, output) = (dir.join("in"), dir.join("out"));
    let dxf = output.join("drawing.dxf");
    if std::fs::metadata(&dxf).is_ok_and(|m| m.len() > 0) {
        return Ok(dxf);
    }
    let io = |e: std::io::Error| DwgError::Io(format!("{}: {e}", dir.display()));
    std::fs::create_dir_all(&input).map_err(io)?;
    std::fs::create_dir_all(&output).map_err(io)?;
    // Alone in its folder, under a plain name: the converter takes every match in the folder.
    let copy = input.join("drawing.dwg");
    std::fs::copy(dwg, &copy).map_err(|e| DwgError::Io(format!("copying {}: {e}", dwg.display())))?;
    let run = Command::new(program).arg(&input).arg(&output).args(["ACAD2018", "DXF", "0", "1", "*.DWG"]).output();
    let _ = std::fs::remove_file(&copy);
    let _ = std::fs::remove_dir(&input);
    let failed = |why: String| DwgError::Failed { path: dwg.display().to_string(), why };
    let run = run.map_err(|e| failed(format!("it could not be started ({e})")))?;
    if std::fs::metadata(&dxf).is_ok_and(|m| m.len() > 0) {
        return Ok(dxf);
    }
    // The converter writes a .err file next to the output when a drawing fails.
    let err_file = std::fs::read_dir(&output)
        .ok()
        .and_then(|d| d.flatten().find(|e| e.path().extension().is_some_and(|x| x.eq_ignore_ascii_case("err"))))
        .and_then(|e| std::fs::read_to_string(e.path()).ok());
    let said = [err_file.unwrap_or_default(), String::from_utf8_lossy(&run.stderr).into_owned(), String::from_utf8_lossy(&run.stdout).into_owned()]
        .into_iter()
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
        .collect::<Vec<_>>()
        .join(" ");
    let _ = std::fs::remove_dir_all(&dir);
    Err(failed(if said.is_empty() { format!("it wrote no DXF (exit status {})", run.status) } else { said.chars().take(400).collect() }))
}

/// Converts a DWG with the converter found by [`find_converter`], into the default cache.
pub fn convert(dwg: &Path) -> Result<PathBuf, DwgError> {
    let program = find_converter().ok_or_else(|| DwgError::NoConverter(install_hint()))?;
    convert_with(&program, dwg, &default_cache_root())
}

/// True when the first bytes are a DWG's version code (`AC1015`, `AC1032` …).
pub fn is_dwg(head: &[u8]) -> bool {
    head.len() >= 6 && head.starts_with(b"AC10") && head[4..6].iter().all(u8::is_ascii_digit)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn scratch(name: &str) -> PathBuf {
        let d = std::env::temp_dir().join(format!("afrobim-dwg-test-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&d);
        std::fs::create_dir_all(&d).unwrap();
        d
    }

    #[test]
    fn the_setting_wins_and_an_empty_one_switches_conversion_off() {
        let d = scratch("setting");
        let program = d.join("ODAFileConverter");
        std::fs::write(&program, b"").unwrap();
        assert_eq!(find_converter_with(Some(program.clone())), Some(program.clone()));
        // A macOS .app folder names the executable inside it.
        let app = d.join("ODAFileConverter.app");
        std::fs::create_dir_all(app.join("Contents/MacOS")).unwrap();
        std::fs::write(app.join(MACOS_EXECUTABLE), b"").unwrap();
        assert_eq!(find_converter_with(Some(app.clone())), Some(app.join(MACOS_EXECUTABLE)));
        assert_eq!(find_converter_with(Some(PathBuf::new())), None);
        assert_eq!(find_converter_with(Some(d.join("missing"))), None);
        let _ = std::fs::remove_dir_all(&d);
    }

    #[test]
    fn the_cache_key_follows_the_file_and_a_converted_dxf_is_reused() {
        let d = scratch("cache");
        let dwg = d.join("a.dwg");
        std::fs::write(&dwg, b"AC1032 first").unwrap();
        let root = d.join("cache");
        let first = cache_dir(&dwg, &root).unwrap();
        assert_eq!(cache_dir(&dwg, &root).unwrap(), first);
        std::fs::write(&dwg, b"AC1032 second, longer").unwrap();
        assert_ne!(cache_dir(&dwg, &root).unwrap(), first, "a changed size is a new key");
        // A DXF already in the cache is returned without running anything.
        let dir = cache_dir(&dwg, &root).unwrap();
        std::fs::create_dir_all(dir.join("out")).unwrap();
        std::fs::write(dir.join("out/drawing.dxf"), b"0\nEOF\n").unwrap();
        assert_eq!(convert_with(Path::new("/no/such/program"), &dwg, &root).unwrap(), dir.join("out/drawing.dxf"));
        let _ = std::fs::remove_dir_all(&d);
    }

    #[test]
    fn a_converter_that_cannot_start_is_reported_and_leaves_no_cache() {
        let d = scratch("nostart");
        let dwg = d.join("b.dwg");
        std::fs::write(&dwg, b"AC1032").unwrap();
        let root = d.join("cache");
        let e = convert_with(Path::new("/no/such/program"), &dwg, &root).unwrap_err();
        assert!(matches!(&e, DwgError::Failed { why, .. } if why.contains("could not be started")), "{e}");
        assert!(!cache_dir(&dwg, &root).unwrap().join("out/drawing.dxf").exists());
        assert!(dwg.exists(), "the source is untouched");
        let _ = std::fs::remove_dir_all(&d);
    }

    #[test]
    fn the_builtin_switch_reads_off_values_and_defaults_to_on() {
        assert!(builtin_enabled_with(None));
        assert!(builtin_enabled_with(Some("1")));
        assert!(builtin_enabled_with(Some("")));
        for off in ["0", " 0 ", "off", "FALSE", "no"] {
            assert!(!builtin_enabled_with(Some(off)), "{off}");
        }
    }

    #[test]
    fn a_builtin_conversion_is_cached_with_its_notes_and_a_failed_one_leaves_nothing() {
        let d = scratch("builtin");
        let dwg = d.join("c.dwg");
        std::fs::write(&dwg, b"AC1032 builtin").unwrap();
        let root = d.join("cache");
        let calls = std::cell::Cell::new(0);
        let ok = |_: &Path, out: &Path| {
            calls.set(calls.get() + 1);
            std::fs::write(out, b"0\nEOF\n").unwrap();
            Ok(vec!["1 of 9 records were left out.".to_string()])
        };
        let (dxf, notes) = convert_builtin_with(&ok, &dwg, &root).unwrap();
        assert_eq!(dxf, builtin_dxf(&cache_dir(&dwg, &root).unwrap()));
        assert_eq!(notes, vec!["1 of 9 records were left out.".to_string()]);
        // The second open is the cache, notes and all.
        assert_eq!(convert_builtin_with(&ok, &dwg, &root).unwrap(), (dxf.clone(), notes));
        assert_eq!(calls.get(), 1);
        // The ODA route does not take the built-in DXF for its own.
        assert!(!cache_dir(&dwg, &root).unwrap().join("out/drawing.dxf").exists());
        forget_builtin(&dwg, &root);
        assert!(!dxf.exists());
        // A converter that writes half a file and then fails leaves no DXF and no part file.
        let half = |_: &Path, out: &Path| {
            std::fs::write(out, b"0\nSECTION\n").unwrap();
            Err("stopped after 1 s".to_string())
        };
        assert_eq!(convert_builtin_with(&half, &dwg, &root).unwrap_err(), "stopped after 1 s");
        assert!(!dxf.exists() && !dxf.with_extension("dxf.part").exists());
        let nothing = |_: &Path, _: &Path| Ok(Vec::new());
        assert_eq!(convert_builtin_with(&nothing, &dwg, &root).unwrap_err(), "it wrote no DXF");
        assert!(dwg.exists(), "the source is untouched");
        let _ = std::fs::remove_dir_all(&d);
    }

    #[test]
    fn dwg_version_codes_are_recognised() {
        assert!(is_dwg(b"AC1032\0\0"));
        assert!(is_dwg(b"AC1015"));
        assert!(!is_dwg(b"  0\nSECTION"));
        assert!(!is_dwg(b"AC10"));
        assert!(!is_dwg(b"AC10XY"));
    }
}
