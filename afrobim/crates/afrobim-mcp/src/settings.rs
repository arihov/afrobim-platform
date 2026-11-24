//! Where the server finds its rule pack and the Transport fonts.
//!
//! Resolved the way the desktop app resolves them: the `AFROBIM_PACK` (pack file) and
//! `AFROBIM_FONTS` (fonts folder) environment variables when they name something that exists,
//! otherwise `packs/UK-TSM-2019.json` and `fonts/` in a folder above the executable or the
//! working directory. The command-line flags `--pack` and `--fonts` override both.
//!
//! The project root every tool path is confined to, and the read-only switch, are settled here
//! too: `--root` or `AFROBIM_ROOT` (otherwise the working directory), and `--read-only` or
//! `AFROBIM_READ_ONLY`.

use std::path::{Path, PathBuf};

use crate::paths::Guard;

pub const PACK_ENV: &str = "AFROBIM_PACK";
pub const FONTS_ENV: &str = "AFROBIM_FONTS";
pub const ROOT_ENV: &str = "AFROBIM_ROOT";
pub const READ_ONLY_ENV: &str = "AFROBIM_READ_ONLY";
pub const DEFAULT_PACK_FILE: &str = "UK-TSM-2019.json";
/// The manifest of a folder pack (C# `RulePackLoader.ManifestFileName`).
pub const FOLDER_MANIFEST: &str = "pack.xml";

/// Command-line options.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Options {
    pub pack: Option<PathBuf>,
    pub fonts: Option<PathBuf>,
    /// The project root (`--root`).
    pub root: Option<PathBuf>,
    /// `--read-only`.
    pub read_only: bool,
}

/// What the command line asked for.
#[derive(Debug, Clone, PartialEq)]
pub enum Invocation {
    Serve(Options),
    Help,
    Version,
}

pub const USAGE: &str =
    "afrobim-mcp: AfroBIM MCP server (JSON-RPC 2.0 over stdio, one message per line)

Usage: afrobim-mcp [--root <folder>] [--read-only] [--pack <rule-pack.json>] [--fonts <folder>]

  --root <dir>     project root: every path a tool reads or writes must lie inside it (links
                   and .. are resolved first), and relative paths resolve against it (otherwise
                   AFROBIM_ROOT, otherwise the working directory, which the start-up log says)
  --read-only      refuse every tool that writes a file (otherwise AFROBIM_READ_ONLY=1)
  --pack <file>    rule pack to use as the default (otherwise AFROBIM_PACK, otherwise
                   packs/UK-TSM-2019.json above the executable or the working directory)
  --fonts <dir>    folder holding the Transport fonts (otherwise AFROBIM_FONTS, otherwise
                   fonts/ above the executable or the working directory)
  --help           show this text
  --version        show the version";

pub fn parse_args<I: IntoIterator<Item = String>>(args: I) -> Result<Invocation, String> {
    let mut options = Options::default();
    let mut args = args.into_iter();
    while let Some(a) = args.next() {
        let (flag, inline) = match a.split_once('=') {
            Some((f, v)) if f.starts_with("--") => (f.to_string(), Some(v.to_string())),
            _ => (a.clone(), None),
        };
        let mut value = |name: &str| {
            inline
                .clone()
                .or_else(|| args.next())
                .ok_or_else(|| format!("{name} needs a value"))
        };
        match flag.as_str() {
            "--pack" => options.pack = Some(PathBuf::from(value("--pack")?)),
            "--fonts" => options.fonts = Some(PathBuf::from(value("--fonts")?)),
            "--root" => options.root = Some(PathBuf::from(value("--root")?)),
            "--read-only" if inline.is_none() => options.read_only = true,
            "--help" | "-h" => return Ok(Invocation::Help),
            "--version" | "-V" => return Ok(Invocation::Version),
            other => return Err(format!("unknown argument '{other}'")),
        }
    }
    Ok(Invocation::Serve(options))
}

/// The resolved locations. A missing pack or fonts folder does not stop the server: the tools
/// that need it report the problem, so an assistant can tell the user what to fix.
#[derive(Debug, Clone, PartialEq)]
pub struct Settings {
    /// The project root: relative `csv_path`, `out_dir` and document paths resolve against it,
    /// and no tool reads or writes outside it.
    pub root: PathBuf,
    /// Where the root came from, for the start-up log.
    pub root_source: RootSource,
    /// Confines every tool path to `root` and carries the read-only switch.
    pub guard: Guard,
    /// The default rule pack file, or why there is none.
    pub pack_file: Result<PathBuf, String>,
    /// The fonts folder, or why there is none.
    pub fonts_dir: Result<PathBuf, String>,
}

/// Where the project root came from.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RootSource {
    Flag,
    Environment,
    /// Neither was given: the working directory.
    WorkingDirectory,
}

impl Settings {
    /// Settings whose project root is `root` (the working directory when nothing else was
    /// asked for), not read-only.
    pub fn new(
        pack_file: Result<PathBuf, String>,
        fonts_dir: Result<PathBuf, String>,
        root: &Path,
    ) -> Settings {
        Settings {
            root: root.to_path_buf(),
            root_source: RootSource::WorkingDirectory,
            guard: Guard::new(root, false),
            pack_file,
            fonts_dir,
        }
    }

    /// The same settings with the read-only switch set.
    pub fn with_read_only(mut self, read_only: bool) -> Settings {
        self.guard = Guard::new(&self.root, read_only);
        self
    }

    pub fn read_only(&self) -> bool {
        self.guard.read_only()
    }

    /// The lines the server logs to standard error at start-up: the project root and where it
    /// came from, and the read-only state.
    pub fn startup_log(&self) -> Vec<String> {
        let mut lines = Vec::new();
        let root = self.root.display();
        lines.push(match self.root_source {
            RootSource::Flag => format!("project root: {root} (from --root)"),
            RootSource::Environment => format!("project root: {root} (from {ROOT_ENV})"),
            RootSource::WorkingDirectory => format!(
                "project root: {root} (no --root or {ROOT_ENV} given, so the working directory is used)"
            ),
        });
        if self.root.parent().is_none() {
            lines.push(format!(
                "warning: the project root is a whole drive or file system, so paths are not confined in practice; pass --root or set {ROOT_ENV}"
            ));
        }
        lines.push(if self.read_only() {
            "read-only mode: on (every tool that writes a file is refused)".to_string()
        } else {
            "read-only mode: off".to_string()
        });
        lines
    }

    /// Flags first, then the environment, then the search above the executable and the working
    /// directory.
    pub fn from_environment(options: &Options) -> Result<Settings, String> {
        let cwd = std::env::current_dir().unwrap_or_else(|_| PathBuf::from("."));
        let exe = std::env::current_exe().ok();
        let absolute = |p: &Path| {
            crate::paths::normalise(&if p.is_absolute() {
                p.to_path_buf()
            } else {
                cwd.join(p)
            })
        };

        let pack = match &options.pack {
            Some(p) if absolute(p).is_file() => Ok(absolute(p)),
            Some(p) => Err(format!("rule pack not found: {} (given with --pack)", absolute(p).display())),
            None => find(std::env::var_os(PACK_ENV).map(PathBuf::from), &Path::new("packs").join(DEFAULT_PACK_FILE), exe.as_deref(), &cwd)
                .map(|p| absolute(&p))
                .ok_or_else(|| {
                    format!("rule pack packs/{DEFAULT_PACK_FILE} was not found (set {PACK_ENV} to its path or start the server with --pack)")
                }),
        };
        let fonts = match &options.fonts {
            Some(p) if absolute(p).is_dir() => Ok(absolute(p)),
            Some(p) => Err(format!("fonts folder not found: {} (given with --fonts)", absolute(p).display())),
            None => find(std::env::var_os(FONTS_ENV).map(PathBuf::from), Path::new("fonts"), exe.as_deref(), &cwd)
                .map(|p| absolute(&p))
                .ok_or_else(|| {
                    format!(
                        "the fonts folder was not found (set {FONTS_ENV} to the folder holding the Transport fonts or start the server with --fonts)"
                    )
                }),
        };

        let (root, root_source) = match (&options.root, env_text(ROOT_ENV)) {
            (Some(p), _) => (absolute(p), RootSource::Flag),
            (None, Some(p)) => (absolute(Path::new(&p)), RootSource::Environment),
            (None, None) => (absolute(&cwd), RootSource::WorkingDirectory),
        };
        if !root.is_dir() {
            let given = match root_source {
                RootSource::Flag => "given with --root",
                RootSource::Environment => "given in AFROBIM_ROOT",
                RootSource::WorkingDirectory => "the working directory",
            };
            return Err(format!(
                "project root is not a folder that exists: {} ({given})",
                root.display()
            ));
        }
        let read_only = options.read_only
            || match env_text(READ_ONLY_ENV) {
                None => false,
                Some(v) => parse_switch(&v).ok_or_else(|| {
                    format!("{READ_ONLY_ENV} must be 1, true, yes or on (or 0, false, no or off), not '{v}'")
                })?,
            };
        let mut settings = Settings::new(pack, fonts, &root).with_read_only(read_only);
        settings.root_source = root_source;
        Ok(settings)
    }

    /// The folder the rule packs are discovered in: the default pack's own folder.
    pub fn packs_dir(&self) -> Option<PathBuf> {
        self.pack_file.as_ref().ok().and_then(|p| packs_dir_of(p))
    }

    /// The id of the default pack: its file name without extension, or its folder name for a
    /// folder pack.
    pub fn default_pack_id(&self) -> Option<String> {
        let file = self.pack_file.as_ref().ok()?;
        let name = if is_folder_manifest(file) {
            file.parent()?.file_name()?
        } else {
            file.file_stem()?
        };
        Some(name.to_string_lossy().into_owned())
    }
}

/// An environment variable's value, `None` when unset or blank.
fn env_text(name: &str) -> Option<String> {
    std::env::var(name)
        .ok()
        .map(|v| v.trim().to_string())
        .filter(|v| !v.is_empty())
}

/// A yes/no switch as people write it in an environment variable.
fn parse_switch(value: &str) -> Option<bool> {
    match value.to_ascii_lowercase().as_str() {
        "1" | "true" | "yes" | "on" => Some(true),
        "0" | "false" | "no" | "off" => Some(false),
        _ => None,
    }
}

fn is_folder_manifest(file: &Path) -> bool {
    file.file_name()
        .is_some_and(|n| n.eq_ignore_ascii_case(FOLDER_MANIFEST))
}

fn packs_dir_of(pack_file: &Path) -> Option<PathBuf> {
    let dir = pack_file.parent()?;
    Some(
        if is_folder_manifest(pack_file) {
            dir.parent()?
        } else {
            dir
        }
        .to_path_buf(),
    )
}

/// The desktop app's search (`signs_glue::find`): the environment value when it exists, else
/// `relative` under every folder above the executable and then the working directory.
fn find(
    env_value: Option<PathBuf>,
    relative: &Path,
    exe: Option<&Path>,
    cwd: &Path,
) -> Option<PathBuf> {
    if let Some(p) = env_value.filter(|p| !p.as_os_str().is_empty() && p.exists()) {
        return Some(p);
    }
    let above_exe = exe.into_iter().flat_map(|e| e.ancestors().skip(1));
    above_exe
        .chain(cwd.ancestors())
        .map(|root| root.join(relative))
        .find(|p| p.exists())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testing::TempDir;

    fn args(list: &[&str]) -> Result<Invocation, String> {
        parse_args(list.iter().map(|s| s.to_string()))
    }

    #[test]
    fn flags_are_parsed_in_both_spellings() {
        assert_eq!(args(&[]), Ok(Invocation::Serve(Options::default())));
        assert_eq!(
            args(&["--pack", "p.json", "--fonts=f"]),
            Ok(Invocation::Serve(Options {
                pack: Some("p.json".into()),
                fonts: Some("f".into()),
                ..Options::default()
            }))
        );
        assert_eq!(
            args(&["--root", "proj", "--read-only"]),
            Ok(Invocation::Serve(Options {
                root: Some("proj".into()),
                read_only: true,
                ..Options::default()
            }))
        );
        assert!(args(&["--root"]).unwrap_err().contains("needs a value"));
        assert!(args(&["--read-only=yes"])
            .unwrap_err()
            .contains("unknown argument"));
        assert_eq!(args(&["--help"]), Ok(Invocation::Help));
        assert_eq!(args(&["--version"]), Ok(Invocation::Version));
        assert!(args(&["--pack"]).unwrap_err().contains("needs a value"));
        assert!(args(&["--nope"]).unwrap_err().contains("unknown argument"));
    }

    #[test]
    fn the_search_prefers_the_environment_then_walks_up() {
        let tmp = TempDir::new("settings");
        let root = tmp.path();
        std::fs::create_dir_all(root.join("packs")).unwrap();
        std::fs::create_dir_all(root.join("fonts")).unwrap();
        std::fs::create_dir_all(root.join("a/b")).unwrap();
        std::fs::write(root.join("packs").join(DEFAULT_PACK_FILE), "{}").unwrap();
        std::fs::write(root.join("other.json"), "{}").unwrap();
        let relative = Path::new("packs").join(DEFAULT_PACK_FILE);

        // From the working directory, walking up.
        assert_eq!(
            find(None, &relative, None, &root.join("a/b")),
            Some(root.join(&relative))
        );
        // From the executable's folder, before the working directory.
        assert_eq!(
            find(
                None,
                &relative,
                Some(&root.join("a/b/server")),
                Path::new("/")
            ),
            Some(root.join(&relative))
        );
        // The environment wins when it names something that exists, and is ignored otherwise.
        assert_eq!(
            find(
                Some(root.join("other.json")),
                &relative,
                None,
                &root.join("a")
            ),
            Some(root.join("other.json"))
        );
        assert_eq!(
            find(
                Some(root.join("missing.json")),
                &relative,
                None,
                &root.join("a")
            ),
            Some(root.join(&relative))
        );
        assert_eq!(
            find(
                None,
                Path::new("no-such-folder-anywhere"),
                None,
                &root.join("a")
            ),
            None
        );
    }

    #[test]
    fn the_root_is_the_folder_given_and_the_packs_are_found_separately() {
        let cwd = Path::new("/work");
        let s = Settings::new(
            Ok("/repo/packs/UK-TSM-2019.json".into()),
            Ok("/repo/fonts".into()),
            cwd,
        );
        assert_eq!(s.root, PathBuf::from("/work"));
        assert_eq!(s.packs_dir(), Some(PathBuf::from("/repo/packs")));
        assert_eq!(s.default_pack_id().as_deref(), Some("UK-TSM-2019"));

        let s = Settings::new(
            Ok("/data/my-pack.json".into()),
            Ok("/repo/fonts".into()),
            cwd,
        );
        assert_eq!(s.root, PathBuf::from("/work"));
        assert_eq!(s.default_pack_id().as_deref(), Some("my-pack"));

        let s = Settings::new(
            Ok("/repo/packs/UG-TSM-2004/pack.xml".into()),
            Ok("/repo/fonts".into()),
            cwd,
        );
        assert_eq!(s.root, PathBuf::from("/work"));
        assert_eq!(s.default_pack_id().as_deref(), Some("UG-TSM-2004"));

        let s = Settings::new(Err("no pack".into()), Err("no fonts".into()), cwd);
        assert_eq!(
            (s.packs_dir(), s.default_pack_id(), s.root),
            (None, None, PathBuf::from("/work"))
        );
    }

    #[test]
    fn the_start_up_log_names_the_root_its_source_and_the_read_only_state() {
        let s = Settings::new(Err("x".into()), Err("y".into()), Path::new("/work/proj"));
        let log = s.startup_log().join("\n");
        assert!(log.contains("project root: /work/proj"), "{log}");
        assert!(log.contains("the working directory is used"), "{log}");
        assert!(log.contains("read-only mode: off"), "{log}");

        let mut s = s.with_read_only(true);
        s.root_source = RootSource::Flag;
        let log = s.startup_log().join("\n");
        assert!(
            log.contains("(from --root)") && log.contains("read-only mode: on"),
            "{log}"
        );

        let log = Settings::new(Err("x".into()), Err("y".into()), Path::new("/"))
            .startup_log()
            .join("\n");
        assert!(
            log.contains("warning: the project root is a whole"),
            "{log}"
        );
    }

    #[test]
    fn read_only_switch_values() {
        for yes in ["1", "true", "YES", "On"] {
            assert_eq!(parse_switch(yes), Some(true), "{yes}");
        }
        for no in ["0", "false", "No", "off"] {
            assert_eq!(parse_switch(no), Some(false), "{no}");
        }
        assert_eq!(parse_switch("ture"), None);
    }
}
