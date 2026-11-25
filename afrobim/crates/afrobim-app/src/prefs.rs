// SPDX-License-Identifier: LicenseRef-AfroBIM-Proprietary
// Copyright (c) 2026 AfroBIM. All rights reserved.
//! The engineer's interface preferences, kept in one small JSON file in the user's configuration
//! folder: the look, the interface scale, the last workspace, the recent files and where the
//! dialogs were left. A missing or damaged file gives the defaults; nothing here touches a drawing.

use std::path::PathBuf;

use serde::{Deserialize, Serialize};

use crate::theme::{Look, Theme};

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Prefs {
    pub look: String,
    pub solid_dark: bool,
    pub scale: f32,
    pub workspace: String,
    pub recent: Vec<String>,
    /// Remembered window positions by title: x, y.
    pub window_pos: std::collections::BTreeMap<String, [f32; 2]>,
    /// Minutes between autosaves of unsaved changes; 0 switches autosave off.
    pub autosave_minutes: u32,
}

impl Default for Prefs {
    fn default() -> Self {
        let t = Theme::default();
        Prefs { look: t.look.key().into(), solid_dark: t.solid_dark, scale: t.scale, workspace: "All".into(), recent: Vec::new(), window_pos: Default::default(), autosave_minutes: crate::recovery::DEFAULT_AUTOSAVE_MINUTES }
    }
}

pub const MAX_RECENT: usize = 10;

impl Prefs {
    pub fn theme(&self) -> Theme {
        Theme { look: Look::from_key(&self.look).unwrap_or(Look::DarkGlass), solid_dark: self.solid_dark, scale: if self.scale.is_finite() { self.scale.clamp(0.7, 2.0) } else { 1.0 } }
    }

    pub fn set_theme(&mut self, t: &Theme) {
        self.look = t.look.key().into();
        self.solid_dark = t.solid_dark;
        self.scale = t.scale;
    }

    /// Puts `path` first in the recent files, once.
    pub fn add_recent(&mut self, path: &str) {
        self.recent.retain(|p| p != path);
        self.recent.insert(0, path.to_string());
        self.recent.truncate(MAX_RECENT);
    }

    pub fn from_json(text: &str) -> Prefs {
        serde_json::from_str(text).unwrap_or_default()
    }

    pub fn to_json(&self) -> String {
        serde_json::to_string_pretty(self).unwrap_or_default()
    }
}

/// The preferences file: `AFROBIM_PREFS` when set, else the platform's configuration folder.
pub fn path() -> Option<PathBuf> {
    if let Some(p) = std::env::var_os("AFROBIM_PREFS") {
        return Some(PathBuf::from(p));
    }
    let var = |name: &str| std::env::var_os(name).filter(|v| !v.is_empty()).map(PathBuf::from);
    let dir = if cfg!(windows) {
        var("APPDATA")?.join("AfroBIM")
    } else if cfg!(target_os = "macos") {
        var("HOME")?.join("Library").join("Application Support").join("AfroBIM")
    } else {
        var("XDG_CONFIG_HOME").or_else(|| var("HOME").map(|h| h.join(".config")))?.join("afrobim")
    };
    Some(dir.join("prefs.json"))
}

pub fn load() -> Prefs {
    path().and_then(|p| std::fs::read_to_string(p).ok()).map(|t| Prefs::from_json(&t)).unwrap_or_default()
}

/// Writes the preferences; a failure is ignored (they are a convenience).
pub fn save(p: &Prefs) {
    if let Some(path) = path() {
        if let Some(dir) = path.parent() {
            let _ = std::fs::create_dir_all(dir);
        }
        let _ = std::fs::write(path, p.to_json());
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn preferences_round_trip_and_a_damaged_file_gives_the_defaults() {
        let mut p = Prefs::default();
        p.set_theme(&Theme { look: Look::Solid, solid_dark: false, scale: 1.3 });
        p.workspace = "Road Design".into();
        p.add_recent("a.afrobim");
        p.add_recent("b.afrobim");
        p.add_recent("a.afrobim");
        assert_eq!(p.recent, vec!["a.afrobim", "b.afrobim"]);
        let back = Prefs::from_json(&p.to_json());
        assert_eq!(back, p);
        assert_eq!(back.theme(), Theme { look: Look::Solid, solid_dark: false, scale: 1.3 });
        assert_eq!(Prefs::from_json("not json"), Prefs::default());
        // A scale that is nonsense is brought back into range.
        let mut q = Prefs::default();
        q.scale = f32::NAN;
        assert_eq!(q.theme().scale, 1.0);
        q.scale = 40.0;
        assert_eq!(q.theme().scale, 2.0);
    }

    #[test]
    fn the_recent_list_is_capped() {
        let mut p = Prefs::default();
        for i in 0..30 {
            p.add_recent(&format!("{i}.afrobim"));
        }
        assert_eq!(p.recent.len(), MAX_RECENT);
        assert_eq!(p.recent[0], "29.afrobim");
    }
}
