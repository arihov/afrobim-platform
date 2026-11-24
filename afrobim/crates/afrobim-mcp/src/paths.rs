//! Resolves and checks every path a tool touches (port of C# `PathGuard`). Schedule reads are
//! limited to existing `.csv` files, `export_dxf` writes only directly inside the `out_dir` the
//! caller named, and the document tools only touch `.afrobim` and `.dxf` files.
//!
//! On top of that, every path a tool reads or writes goes through a [`Guard`]: it must lie inside
//! the project root the server was started with, after `..` is folded away and every symbolic
//! link on the way is resolved. A write may not pass through a link at all, and none is allowed
//! when the server runs read-only.

use std::ffi::OsString;
use std::io::ErrorKind;
use std::path::{Component, Path, PathBuf};

use crate::tools::ToolError;

const RESERVED_WINDOWS_NAMES: [&str; 22] = [
    "CON", "PRN", "AUX", "NUL", "COM1", "COM2", "COM3", "COM4", "COM5", "COM6", "COM7", "COM8",
    "COM9", "LPT1", "LPT2", "LPT3", "LPT4", "LPT5", "LPT6", "LPT7", "LPT8", "LPT9",
];

/// `path` with `.` and `..` folded away, without touching the file system (so it works for a
/// file that does not exist yet). `..` at the root stays at the root.
pub fn normalise(path: &Path) -> PathBuf {
    let mut out = PathBuf::new();
    for c in path.components() {
        match c {
            Component::CurDir => {}
            Component::ParentDir => {
                if matches!(out.components().next_back(), Some(Component::Normal(_))) {
                    out.pop();
                }
            }
            other => out.push(other.as_os_str()),
        }
    }
    out
}

/// A full path for `path`; a relative path resolves against `base_dir`.
pub fn resolve(path: &str, base_dir: &Path) -> Result<PathBuf, ToolError> {
    if path.trim().is_empty() {
        return Err(ToolError::new("path is empty"));
    }
    if path.contains('\0') {
        return Err(ToolError::new("path contains a NUL character"));
    }
    let p = Path::new(path);
    Ok(normalise(&if p.is_absolute() {
        p.to_path_buf()
    } else {
        base_dir.join(p)
    }))
}

/// What a tool is about to do with a path.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Access {
    Read,
    Write,
}

/// The project root every path must stay inside (and the folder relative paths resolve
/// against), plus the server-wide read-only switch.
#[derive(Debug, Clone, PartialEq)]
pub struct Guard {
    /// The root as given: absolute, with `.` and `..` folded away, links not resolved.
    given: PathBuf,
    /// The root with every link resolved (the same as `given` if it cannot be resolved).
    real: PathBuf,
    read_only: bool,
    /// False only for the guard the unit tests of the tools use (see `testing`).
    confined: bool,
}

impl Guard {
    pub fn new(root: &Path, read_only: bool) -> Guard {
        let given = normalise(root);
        let real = std::fs::canonicalize(&given).unwrap_or_else(|_| given.clone());
        Guard {
            given,
            real,
            read_only,
            confined: true,
        }
    }

    /// A guard that checks nothing, for the unit tests of the tools, which write into system
    /// temporary folders and read the checkout's examples. The confinement has its own tests.
    #[cfg(test)]
    pub fn unconfined(root: &Path) -> Guard {
        Guard {
            confined: false,
            ..Guard::new(root, false)
        }
    }

    pub fn read_only(&self) -> bool {
        self.read_only
    }

    /// The full path for `path` (relative paths resolve against the root), checked for `access`:
    ///
    /// * `..` is folded away first, and the path the file system is then given is exactly the
    ///   one returned, so what was checked is what is used;
    /// * the deepest part that exists is resolved through every link, and the result must lie
    ///   inside the root;
    /// * a write is refused in read-only mode, and refused if any existing part of the path
    ///   below the root is a link (so a write never goes through one).
    pub fn check(&self, path: &str, access: Access) -> Result<PathBuf, ToolError> {
        let full = resolve(path, &self.given)?;
        if access == Access::Write && self.read_only {
            return Err(ToolError::new(format!(
                "refused: the server is in read-only mode, so {} cannot be written",
                full.display()
            )));
        }
        if !self.confined {
            return Ok(full);
        }
        let real = self.real_path(&full)?;
        if !real.starts_with(&self.real) {
            return Err(self.outside(&full, Some(&real)));
        }
        if access == Access::Write {
            self.refuse_links(&full)?;
        }
        Ok(full)
    }

    fn outside(&self, full: &Path, real: Option<&Path>) -> ToolError {
        let resolved = match real {
            Some(r) if r != full => format!(" (it resolves to {})", r.display()),
            _ => String::new(),
        };
        ToolError::new(format!(
            "refused: {} is outside the project root {}{resolved}; every path must lie inside the root",
            full.display(),
            self.given.display()
        ))
    }

    /// `full` with the deepest existing part resolved through every link; the part that does
    /// not exist yet is appended unchanged (it has no links to follow).
    fn real_path(&self, full: &Path) -> Result<PathBuf, ToolError> {
        let mut existing = full.to_path_buf();
        let mut tail: Vec<OsString> = Vec::new();
        loop {
            match std::fs::symlink_metadata(&existing) {
                Ok(_) => break,
                Err(e) if e.kind() == ErrorKind::NotFound => {
                    let (Some(name), Some(parent)) = (existing.file_name(), existing.parent())
                    else {
                        return Err(ToolError::new(format!(
                            "refused: cannot resolve {}",
                            full.display()
                        )));
                    };
                    tail.push(name.to_os_string());
                    existing = parent.to_path_buf();
                }
                Err(e) => {
                    return Err(ToolError::new(format!(
                        "refused: cannot resolve {}: {e}",
                        full.display()
                    )))
                }
            }
        }
        let mut real = std::fs::canonicalize(&existing).map_err(|e| {
            if e.kind() == ErrorKind::NotFound {
                ToolError::new(format!(
                    "refused: {} is a link to something that does not exist",
                    existing.display()
                ))
            } else {
                ToolError::new(format!(
                    "refused: cannot resolve {}: {e}",
                    existing.display()
                ))
            }
        })?;
        real.extend(tail.iter().rev());
        Ok(real)
    }

    /// Refuses a write whose path passes through a link anywhere below the root. The path must
    /// be written starting from the root as given or as resolved.
    fn refuse_links(&self, full: &Path) -> Result<(), ToolError> {
        let base = [&self.given, &self.real]
            .into_iter()
            .find(|b| full.starts_with(b))
            .ok_or_else(|| {
                ToolError::new(format!(
                    "refused: {} names the project root differently from {}; write the path under {} so it can be checked for links",
                    full.display(),
                    self.given.display(),
                    self.given.display()
                ))
            })?;
        let mut current = base.to_path_buf();
        for part in full.strip_prefix(base).unwrap_or(full).components() {
            current.push(part);
            let is_link = match std::fs::symlink_metadata(&current) {
                Ok(m) => m.file_type().is_symlink(),
                Err(e) if e.kind() == ErrorKind::NotFound => break,
                Err(e) => {
                    let message = format!("refused: cannot check {}: {e}", current.display());
                    return Err(ToolError::new(message));
                }
            };
            if is_link {
                let message = format!(
                    "refused: {} is a link; nothing is written through a link (project root {})",
                    current.display(),
                    self.given.display()
                );
                return Err(ToolError::new(message));
            }
        }
        Ok(())
    }
}

fn has_extension(path: &Path, ext: &str) -> bool {
    path.extension()
        .and_then(|e| e.to_str())
        .is_some_and(|e| e.eq_ignore_ascii_case(ext))
}

/// An existing `.csv` schedule file inside the project root.
pub fn resolve_csv(path: &str, guard: &Guard) -> Result<PathBuf, ToolError> {
    let full = guard.check(path, Access::Read)?;
    let shown = full.display();
    if !has_extension(&full, "csv") {
        return Err(ToolError::new(format!(
            "csv_path must name a .csv schedule file: {shown}"
        )));
    }
    if full.is_dir() {
        return Err(ToolError::new(format!(
            "csv_path is a folder, not a file: {shown}"
        )));
    }
    if !full.is_file() {
        return Err(ToolError::new(format!("schedule not found: {shown}")));
    }
    Ok(full)
}

/// A path that must carry the extension `ext` (no dot) and must not be a folder. `param` names
/// the argument in error messages. The file itself may or may not exist. The path must lie inside
/// the project root, and a write must not go through a link.
pub fn resolve_file(
    path: &str,
    guard: &Guard,
    access: Access,
    ext: &str,
    param: &str,
) -> Result<PathBuf, ToolError> {
    let full = guard.check(path, access)?;
    let shown = full.display();
    if !has_extension(&full, ext) {
        return Err(ToolError::new(format!(
            "{param} must name a .{ext} file: {shown}"
        )));
    }
    if full.is_dir() {
        return Err(ToolError::new(format!(
            "{param} is a folder, not a file: {shown}"
        )));
    }
    Ok(full)
}

/// Creates the folder a file is about to be written into.
pub fn prepare_parent(file: &Path) -> Result<(), ToolError> {
    match file.parent() {
        Some(dir) if !dir.as_os_str().is_empty() => std::fs::create_dir_all(dir)
            .map_err(|e| ToolError::new(format!("could not create folder {}: {e}", dir.display()))),
        _ => Ok(()),
    }
}

/// The output folder, created if needed. Refuses a path that is an existing file, that lies
/// outside the project root or that goes through a link; nothing is created before the check.
pub fn prepare_out_dir(path: &str, guard: &Guard) -> Result<PathBuf, ToolError> {
    let full = guard.check(path, Access::Write)?;
    if full.is_file() {
        return Err(ToolError::new(format!(
            "out_dir is a file, not a folder: {}",
            full.display()
        )));
    }
    std::fs::create_dir_all(&full)
        .map_err(|e| ToolError::new(format!("could not create out_dir {}: {e}", full.display())))?;
    Ok(full)
}

/// The full path for `file_name` inside `out_dir`, after checking that it resolves to a file
/// directly in that folder and is not a link that would redirect the write elsewhere.
pub fn file_in_out_dir(out_dir: &Path, file_name: &str) -> Result<PathBuf, ToolError> {
    let full = normalise(&out_dir.join(file_name));
    if !is_directly_inside(out_dir, &full) {
        return Err(ToolError::new(format!(
            "refused: '{file_name}' resolves outside out_dir ({})",
            full.display()
        )));
    }
    if let Ok(meta) = std::fs::symlink_metadata(&full) {
        if meta.file_type().is_symlink() {
            return Err(ToolError::new(format!(
                "refused: {} is a link; writes must stay inside out_dir",
                full.display()
            )));
        }
        if meta.is_dir() {
            return Err(ToolError::new(format!(
                "refused: {} is a folder",
                full.display()
            )));
        }
    }
    Ok(full)
}

/// True when `candidate` is a direct child of `dir`.
pub fn is_directly_inside(dir: &Path, candidate: &Path) -> bool {
    let (dir, candidate) = (normalise(dir), normalise(candidate));
    candidate.file_name().is_some() && candidate.parent() == Some(dir.as_path())
}

/// A file name stem for a sign code: path separators and characters invalid on either Windows
/// or Linux become `_`, and a name that is empty, all dots or a Windows device name gets a
/// leading `_`.
pub fn safe_stem(code: &str) -> String {
    const INVALID: [char; 9] = ['/', '\\', ':', '*', '?', '"', '<', '>', '|'];
    let replaced: String = code
        .chars()
        .map(|c| {
            if INVALID.contains(&c) || c.is_control() {
                '_'
            } else {
                c
            }
        })
        .collect();
    let stem = replaced.trim().trim_end_matches('.');
    if stem.is_empty() || stem.chars().all(|c| c == '.') {
        return format!("_{}", stem.replace('.', "_"));
    }
    let first = stem.split('.').next().unwrap_or("");
    if RESERVED_WINDOWS_NAMES
        .iter()
        .any(|r| r.eq_ignore_ascii_case(first))
    {
        return format!("_{stem}");
    }
    stem.to_string()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testing::TempDir;

    #[test]
    fn normalise_folds_dots_without_touching_disk() {
        assert_eq!(
            normalise(Path::new("/a/b/../c/./d")),
            PathBuf::from("/a/c/d")
        );
        assert_eq!(normalise(Path::new("/../x")), PathBuf::from("/x"));
    }

    #[test]
    fn resolve_uses_the_base_for_relative_paths_only() {
        let base = Path::new("/base/dir");
        assert_eq!(
            resolve("out/x", base).unwrap(),
            PathBuf::from("/base/dir/out/x")
        );
        assert_eq!(resolve("/abs/x", base).unwrap(), PathBuf::from("/abs/x"));
        assert!(resolve("  ", base).is_err());
        assert!(resolve("a\0b", base).is_err());
    }

    #[test]
    fn safe_stem_never_leaves_the_folder() {
        let tmp = TempDir::new("stem");
        for code in [
            "../../evil",
            "..",
            ".",
            "",
            "a/b\\c",
            "CON",
            "C:x",
            "con.txt",
            "tab\there",
        ] {
            let name = format!("{}.dxf", safe_stem(code));
            let full = file_in_out_dir(tmp.path(), &name).unwrap_or_else(|e| panic!("{code}: {e}"));
            assert!(
                is_directly_inside(tmp.path(), &full),
                "{code} -> {}",
                full.display()
            );
        }
        assert_eq!(safe_stem("ADS-1"), "ADS-1");
        assert_eq!(safe_stem("CON"), "_CON");
        assert_eq!(safe_stem(".."), "_");
        assert_eq!(safe_stem("../../evil"), ".._.._evil");
    }

    #[test]
    fn file_in_out_dir_refuses_escapes_links_and_folders() {
        let tmp = TempDir::new("guard");
        let dir = tmp.path();
        assert!(file_in_out_dir(dir, "../x.dxf").is_err());
        assert!(file_in_out_dir(dir, "/tmp/x.dxf").is_err());
        assert!(file_in_out_dir(dir, "sub/x.dxf").is_err());
        assert!(file_in_out_dir(dir, "x.dxf").is_ok());

        std::fs::create_dir(dir.join("folder.dxf")).unwrap();
        assert!(file_in_out_dir(dir, "folder.dxf").is_err());

        #[cfg(unix)]
        {
            std::fs::write(dir.join("target.txt"), "x").unwrap();
            std::os::unix::fs::symlink(dir.join("target.txt"), dir.join("link.dxf")).unwrap();
            assert!(file_in_out_dir(dir, "link.dxf")
                .unwrap_err()
                .to_string()
                .contains("link"));
        }
    }

    #[test]
    fn csv_paths_must_be_existing_csv_files() {
        let tmp = TempDir::new("csv");
        let dir = tmp.path();
        std::fs::write(dir.join("schedule.txt"), "x").unwrap();
        std::fs::write(dir.join("ok.CSV"), "x").unwrap();
        std::fs::create_dir(dir.join("dir.csv")).unwrap();
        let guard = Guard::new(dir, false);
        assert!(resolve_csv("schedule.txt", &guard).is_err());
        assert!(resolve_csv("absent.csv", &guard).is_err());
        assert!(resolve_csv("dir.csv", &guard).is_err());
        assert_eq!(resolve_csv("ok.CSV", &guard).unwrap(), dir.join("ok.CSV"));
    }

    #[test]
    fn out_dir_is_created_and_must_not_be_a_file() {
        let tmp = TempDir::new("outdir");
        let guard = Guard::new(tmp.path(), false);
        let made = prepare_out_dir("a/b", &guard).unwrap();
        assert!(made.is_dir());
        std::fs::write(tmp.path().join("file"), "x").unwrap();
        assert!(prepare_out_dir("file", &guard).is_err());
    }

    // ─── confinement to the project root ─────────────────────────────────────────────────────

    /// A project root with a sibling folder `outside` that holds a file, in one temporary folder.
    fn project() -> (TempDir, PathBuf, PathBuf) {
        let tmp = TempDir::new("confine");
        // The system temporary folder may itself sit behind a link (macOS), so use the real one.
        let base = std::fs::canonicalize(tmp.path()).unwrap();
        let root = base.join("project");
        let outside = base.join("outside");
        std::fs::create_dir_all(&root).unwrap();
        std::fs::create_dir_all(&outside).unwrap();
        std::fs::write(outside.join("secret.csv"), "code,kind\n").unwrap();
        std::fs::write(root.join("inside.csv"), "code,kind\n").unwrap();
        (tmp, root, outside)
    }

    fn refusal(r: Result<PathBuf, ToolError>) -> String {
        r.expect_err("the path must be refused").to_string()
    }

    #[test]
    fn a_normal_path_inside_the_root_is_accepted_for_reading_and_writing() {
        let (_tmp, root, _) = project();
        let guard = Guard::new(&root, false);
        assert_eq!(
            guard.check("inside.csv", Access::Read).unwrap(),
            root.join("inside.csv")
        );
        // A file and folders that do not exist yet, and a path given absolutely.
        assert_eq!(
            guard.check("new/deeper/out.dxf", Access::Write).unwrap(),
            root.join("new/deeper/out.dxf")
        );
        assert_eq!(
            guard
                .check(root.join("inside.csv").to_str().unwrap(), Access::Write)
                .unwrap(),
            root.join("inside.csv")
        );
        // `..` that stays inside is fine.
        assert_eq!(
            guard.check("a/../inside.csv", Access::Read).unwrap(),
            root.join("inside.csv")
        );
        assert!(resolve_csv("inside.csv", &guard).is_ok());
        assert!(prepare_out_dir("made/here", &guard).unwrap().is_dir());
    }

    #[test]
    fn dot_dot_cannot_leave_the_root() {
        let (_tmp, root, outside) = project();
        let guard = Guard::new(&root, false);
        for access in [Access::Read, Access::Write] {
            let msg = refusal(guard.check("../outside/secret.csv", access));
            assert!(
                msg.contains("outside the project root") && msg.contains(root.to_str().unwrap()),
                "{msg}"
            );
            assert!(guard.check("sub/../../outside/x.dxf", access).is_err());
        }
        assert!(resolve_csv("../outside/secret.csv", &guard).is_err());
        assert!(!outside.join("x.dxf").exists());
    }

    #[test]
    fn an_absolute_path_outside_the_root_is_refused() {
        let (_tmp, root, outside) = project();
        let guard = Guard::new(&root, false);
        let target = outside.join("secret.csv");
        let msg = refusal(guard.check(target.to_str().unwrap(), Access::Read));
        assert!(msg.contains("outside the project root"), "{msg}");
        assert!(resolve_csv(target.to_str().unwrap(), &guard).is_err());
        assert!(prepare_out_dir(outside.join("made").to_str().unwrap(), &guard).is_err());
        assert!(
            !outside.join("made").exists(),
            "nothing is created outside the root"
        );
        // A sibling whose name merely starts with the root's name is outside too.
        let sibling = root.with_file_name("project-evil");
        assert!(guard
            .check(sibling.join("x").to_str().unwrap(), Access::Write)
            .is_err());
    }

    #[cfg(unix)]
    #[test]
    fn a_link_pointing_outside_the_root_is_refused() {
        use std::os::unix::fs::symlink;
        let (_tmp, root, outside) = project();
        let guard = Guard::new(&root, false);
        symlink(outside.join("secret.csv"), root.join("link.csv")).unwrap();
        symlink(&outside, root.join("linked-dir")).unwrap();
        symlink(root.join("nowhere.csv"), root.join("dangling.csv")).unwrap();

        // Reading through a file link and through a folder link.
        assert!(refusal(guard.check("link.csv", Access::Read)).contains("outside the project root"));
        assert!(resolve_csv("link.csv", &guard).is_err());
        assert!(guard.check("linked-dir/secret.csv", Access::Read).is_err());
        // Writing through either, and creating a folder beyond a linked folder.
        assert!(guard.check("link.csv", Access::Write).is_err());
        assert!(guard.check("linked-dir/new.dxf", Access::Write).is_err());
        assert!(prepare_out_dir("linked-dir/made", &guard).is_err());
        assert!(!outside.join("made").exists() && !outside.join("new.dxf").exists());
        // A link that leads nowhere cannot be shown to stay inside.
        assert!(guard.check("dangling.csv", Access::Write).is_err());
    }

    #[cfg(unix)]
    #[test]
    fn a_write_never_goes_through_a_link_even_one_that_stays_inside() {
        use std::os::unix::fs::symlink;
        let (_tmp, root, _) = project();
        let guard = Guard::new(&root, false);
        std::fs::create_dir(root.join("real")).unwrap();
        symlink(root.join("real"), root.join("alias")).unwrap();
        symlink(root.join("inside.csv"), root.join("alias.csv")).unwrap();

        let msg = refusal(guard.check("alias/out.dxf", Access::Write));
        assert!(
            msg.contains("is a link") && msg.contains("nothing is written through a link"),
            "{msg}"
        );
        assert!(guard.check("alias.csv", Access::Write).is_err());
        // Reading through a link that stays inside is allowed.
        assert!(guard.check("alias/anything.csv", Access::Read).is_ok());
        assert!(guard.check("alias.csv", Access::Read).is_ok());
        assert!(guard.check("real/out.dxf", Access::Write).is_ok());
    }

    #[cfg(unix)]
    #[test]
    fn a_root_reached_through_a_link_still_works() {
        use std::os::unix::fs::symlink;
        let (_tmp, root, _) = project();
        let via = root.with_file_name("via");
        symlink(&root, &via).unwrap();
        // The root is given by its linked name; paths under either name are inside it, and the
        // link in the root's own name is not held against a write.
        let guard = Guard::new(&via, false);
        assert!(guard.check("inside.csv", Access::Read).is_ok());
        assert!(guard.check("new/out.dxf", Access::Write).is_ok());
        assert!(guard
            .check(root.join("inside.csv").to_str().unwrap(), Access::Write)
            .is_ok());
        assert!(guard.check("../outside/secret.csv", Access::Read).is_err());
    }

    #[test]
    fn read_only_mode_refuses_every_write_but_not_a_read() {
        let (_tmp, root, _) = project();
        let guard = Guard::new(&root, true);
        assert!(guard.read_only());
        let msg = refusal(guard.check("inside.csv", Access::Write));
        assert!(msg.contains("read-only mode"), "{msg}");
        assert!(prepare_out_dir("made", &guard).is_err());
        assert!(!root.join("made").exists());
        assert!(resolve_file("m.afrobim", &guard, Access::Write, "afrobim", "path").is_err());
        assert!(guard.check("inside.csv", Access::Read).is_ok());
        assert!(resolve_file("m.afrobim", &guard, Access::Read, "afrobim", "path").is_ok());
    }
}
