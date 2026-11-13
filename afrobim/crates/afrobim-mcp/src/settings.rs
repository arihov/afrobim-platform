//! Where the server finds its rule pack and the Transport fonts.
//!
//! Resolved the way the desktop app resolves them: the `AFROBIM_PACK` (pack file) and
//! `AFROBIM_FONTS` (fonts folder) environment variables when they name something that exists,
//! otherwise `packs/UK-TSM-2019.json` and `fonts/` in a folder above the executable or the
//! working directory. The command-line flags `--pack` and `--fonts` override both.

use std::path::{Path, PathBuf};

pub const PACK_ENV: &str = "AFROBIM_PACK";
pub const FONTS_ENV: &str = "AFROBIM_FONTS";
pub const DEFAULT_PACK_FILE: &str = "UK-TSM-2019.json";
/// The manifest of a folder pack (C# `RulePackLoader.ManifestFileName`).
pub const FOLDER_MANIFEST: &str = "pack.xml";

/// Command-line options.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Options {
    pub pack: Option<PathBuf>,
    pub fonts: Option<PathBuf>,
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

Usage: afrobim-mcp [--pack <rule-pack.json>] [--fonts <folder>]

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
    /// Relative `csv_path`, `out_dir` and document paths resolve against this folder: the one
    /// holding `packs/` when the pack file sits in a folder of that name, otherwise the working
    /// directory. It does not depend on where the MCP client launched the server from when the
    /// pack is found above the executable.
    pub root: PathBuf,
    /// The default rule pack file, or why there is none.
    pub pack_file: Result<PathBuf, String>,
    /// The fonts folder, or why there is none.
    pub fonts_dir: Result<PathBuf, String>,
}

impl Settings {
    pub fn new(
        pack_file: Result<PathBuf, String>,
        fonts_dir: Result<PathBuf, String>,
        cwd: &Path,
    ) -> Settings {
        let root = pack_file
            .as_ref()
            .ok()
            .and_then(|p| packs_dir_of(p))
            .filter(|dir| {
                dir.file_name()
                    .is_some_and(|n| n.eq_ignore_ascii_case("packs"))
            })
            .and_then(|dir| dir.parent().map(Path::to_path_buf))
            .unwrap_or_else(|| cwd.to_path_buf());
        Settings {
            root,
            pack_file,
            fonts_dir,
        }
    }

    /// Flags first, then the environment, then the search above the executable and the working
    /// directory.
    pub fn from_environment(options: &Options) -> Settings {
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
        Settings::new(pack, fonts, &cwd)
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
                fonts: Some("f".into())
            }))
        );
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
    fn the_root_is_the_folder_holding_packs() {
        let cwd = Path::new("/work");
        let s = Settings::new(
            Ok("/repo/packs/UK-TSM-2019.json".into()),
            Ok("/repo/fonts".into()),
            cwd,
        );
        assert_eq!(s.root, PathBuf::from("/repo"));
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
        assert_eq!(s.root, PathBuf::from("/repo"));
        assert_eq!(s.default_pack_id().as_deref(), Some("UG-TSM-2004"));

        let s = Settings::new(Err("no pack".into()), Err("no fonts".into()), cwd);
        assert_eq!(
            (s.packs_dir(), s.default_pack_id(), s.root),
            (None, None, PathBuf::from("/work"))
        );
    }
}
