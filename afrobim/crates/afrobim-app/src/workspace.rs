// SPDX-License-Identifier: LicenseRef-AfroBIM-Proprietary
// Copyright (c) 2026 AfroBIM. All rights reserved.
//! Workspaces: which ribbon tabs (and which panels of a shared tab) and which tool palettes show,
//! by module. "All" is everything, as before. Home and Annotate show in every workspace, and the
//! project tabs (Manage, View, Help) follow them.
//!
//! A command that belongs to another workspace still runs from the palette, the command line and
//! the menus: a workspace only chooses what the ribbon shows.
//!
//! Adding a tab to a workspace is a one-line change: put `WsTab::all(Tab::X)` (every panel of the
//! tab) or `WsTab::only(Tab::X, &["Panel title"])` (some of its panels) in that workspace's `tabs`
//! list below. A new workspace is one more entry in `WORKSPACES`.

use crate::ribbon::Tab;

/// A tab of a workspace, with the panels of it that show (all when `panels` is None).
#[derive(Clone, Copy, Debug)]
pub struct WsTab {
    pub tab: Tab,
    pub panels: Option<&'static [&'static str]>,
}

impl WsTab {
    pub const fn all(tab: Tab) -> WsTab {
        WsTab { tab, panels: None }
    }
    pub const fn only(tab: Tab, panels: &'static [&'static str]) -> WsTab {
        WsTab { tab, panels: Some(panels) }
    }
}

/// The tool palettes of the left panel a workspace shows.
#[derive(Clone, Copy, Debug)]
pub struct Palettes {
    /// The sign templates and the standard-sign library.
    pub signs: bool,
    /// Move, copy, erase, arrange, and the zoom buttons.
    pub modify: bool,
}

pub struct Workspace {
    pub name: &'static str,
    pub glyph: &'static str,
    pub blurb: &'static str,
    /// The tabs after Home and Annotate; the project tabs (`COMMON_TAIL`) follow.
    pub tabs: &'static [WsTab],
    pub palettes: Palettes,
    /// Panel visibility the workspace starts with: the left tool panel and the right properties panel.
    pub show_left: bool,
    pub show_right: bool,
    /// Everything: no filtering at all (today's behaviour).
    pub everything: bool,
    /// The toolspace tab the workspace starts on: 0 Prospector, 1 Survey, 2 Settings.
    pub toolspace_tab: usize,
}

use egui_phosphor::regular as ph;

/// Shown first in every workspace.
pub const COMMON_HEAD: &[WsTab] = &[WsTab::all(Tab::Home), WsTab::all(Tab::Annotate)];
/// Shown last in every workspace but All (All has them where they always were).
pub const COMMON_TAIL: &[WsTab] = &[WsTab::all(Tab::Edit), WsTab::all(Tab::Manage), WsTab::all(Tab::View), WsTab::all(Tab::Help)];

const BOTH: Palettes = Palettes { signs: false, modify: true };

pub const WORKSPACES: &[Workspace] = &[
    Workspace {
        name: "All",
        glyph: ph::SQUARES_FOUR,
        blurb: "Every tab and tool, as before",
        tabs: &[],
        palettes: Palettes { signs: true, modify: true },
        show_left: true,
        show_right: true,
        everything: true,
        toolspace_tab: 0,
    },
    Workspace {
        name: "Civil",
        glyph: ph::TREE_VIEW,
        blurb: "Points, surfaces, alignments, profiles, sections, corridors, parcels and quantities",
        tabs: &[WsTab::all(Tab::Civil), WsTab::all(Tab::Insert)],
        // The toolspace tree gets the whole left panel.
        palettes: Palettes { signs: false, modify: false },
        show_left: true,
        show_right: true,
        everything: false,
        toolspace_tab: 0,
    },
    Workspace {
        name: "Survey, Clouds and Data",
        glyph: ph::CHART_SCATTER,
        blurb: "Survey database, adjustment, point clouds, data shortcuts and reprojection",
        tabs: &[WsTab::only(Tab::Civil, &["Points", "Surfaces", "Survey database", "Point clouds", "Shortcuts", "Reprojection"]), WsTab::all(Tab::Insert)],
        palettes: Palettes { signs: false, modify: false },
        show_left: true,
        show_right: true,
        everything: false,
        toolspace_tab: 1,
    },
    Workspace {
        name: "Pipes, Grading and Junctions",
        glyph: ph::PIPE,
        blurb: "Pipe networks, grading, intersections and roundabouts, with surfaces and quantities",
        tabs: &[WsTab::only(Tab::Civil, &["Surfaces", "Alignments", "Pipe networks", "Grading", "Intersections", "Quantities"]), WsTab::all(Tab::Insert)],
        palettes: Palettes { signs: false, modify: false },
        show_left: true,
        show_right: true,
        everything: false,
        toolspace_tab: 0,
    },
    Workspace {
        name: "Survey and Terrain",
        glyph: ph::MOUNTAINS,
        blurb: "Survey points, ground surfaces, reference drawings",
        tabs: &[WsTab::all(Tab::Insert), WsTab::only(Tab::Roads, &["Earthworks"])],
        palettes: BOTH,
        show_left: true,
        show_right: true,
        everything: false,
        toolspace_tab: 0,
    },
    Workspace {
        name: "Road Design",
        glyph: ph::ROAD_HORIZON,
        blurb: "Alignment, profile, corridor, pavement",
        tabs: &[WsTab::only(Tab::Roads, &["Alignment", "Pavement", "Earthworks"]), WsTab::all(Tab::Design), WsTab::all(Tab::Insert)],
        palettes: BOTH,
        show_left: true,
        show_right: true,
        everything: false,
        toolspace_tab: 0,
    },
    Workspace {
        name: "Signs and Markings",
        glyph: ph::TRAFFIC_SIGN,
        blurb: "Direction signs, library, road markings, schedules",
        tabs: &[WsTab::all(Tab::Signs), WsTab::only(Tab::Roads, &["Alignment", "Markings"]), WsTab::all(Tab::Insert), WsTab::all(Tab::Modify)],
        palettes: Palettes { signs: true, modify: true },
        show_left: true,
        show_right: true,
        everything: false,
        toolspace_tab: 0,
    },
    Workspace {
        name: "Drainage",
        glyph: ph::DROP,
        blurb: "Side drains, culverts, rational flow",
        tabs: &[WsTab::only(Tab::Roads, &["Alignment", "Drainage"])],
        palettes: BOTH,
        show_left: true,
        show_right: true,
        everything: false,
        toolspace_tab: 0,
    },
    Workspace {
        name: "Lighting and Restraint",
        glyph: ph::LIGHTBULB_FILAMENT,
        blurb: "Street lighting, photometry, safety barriers",
        tabs: &[WsTab::only(Tab::Roads, &["Alignment", "Lighting", "Restraint"])],
        palettes: BOTH,
        show_left: true,
        show_right: true,
        everything: false,
        toolspace_tab: 0,
    },
    Workspace {
        name: "Structures and Supports",
        glyph: ph::BRIDGE,
        blurb: "Posts, foundations, gantries",
        tabs: &[WsTab::all(Tab::Structures), WsTab::all(Tab::Modify)],
        palettes: BOTH,
        show_left: true,
        show_right: true,
        everything: false,
        toolspace_tab: 0,
    },
    Workspace {
        name: "Coordination",
        glyph: ph::CUBE_FOCUS,
        blurb: "Clash detection, issues, IFC reference models",
        tabs: &[WsTab::all(Tab::Coordination)],
        palettes: BOTH,
        show_left: false,
        show_right: true,
        everything: false,
        toolspace_tab: 0,
    },
    Workspace {
        name: "Construction",
        glyph: ph::CALENDAR,
        blurb: "Programme (4D), cost (5D), site progress",
        tabs: &[WsTab::all(Tab::FourD), WsTab::all(Tab::FiveD), WsTab::all(Tab::Construction)],
        palettes: BOTH,
        show_left: false,
        show_right: true,
        everything: false,
        toolspace_tab: 0,
    },
    Workspace {
        name: "Documentation",
        glyph: ph::FILES,
        blurb: "Sheets, annotation, reports, exports",
        tabs: &[WsTab::all(Tab::Output), WsTab::all(Tab::Insert)],
        palettes: BOTH,
        show_left: false,
        show_right: false,
        everything: false,
        toolspace_tab: 0,
    },
];

/// A tab as the ribbon shows it in a workspace.
#[derive(Clone, Debug, PartialEq)]
pub struct VisTab {
    pub tab: Tab,
    /// The panels that show; all when None.
    pub panels: Option<Vec<&'static str>>,
}

impl VisTab {
    pub fn allows(&self, title: &str) -> bool {
        self.panels.as_ref().is_none_or(|p| p.contains(&title))
    }
}

impl Workspace {
    /// The tabs of this workspace in ribbon order.
    pub fn visible_tabs(&self) -> Vec<VisTab> {
        if self.everything {
            return crate::ribbon::TABS.iter().map(|(t, _)| VisTab { tab: *t, panels: None }).collect();
        }
        let mut out: Vec<VisTab> = Vec::new();
        for wt in COMMON_HEAD.iter().chain(self.tabs).chain(COMMON_TAIL) {
            // A tab named twice keeps its first place and the union of its panels.
            if let Some(existing) = out.iter_mut().find(|e| e.tab == wt.tab) {
                existing.panels = match (existing.panels.take(), wt.panels) {
                    (None, _) | (_, None) => None,
                    (Some(mut a), Some(b)) => {
                        for p in b {
                            if !a.contains(p) {
                                a.push(p);
                            }
                        }
                        Some(a)
                    }
                };
            } else {
                out.push(VisTab { tab: wt.tab, panels: wt.panels.map(|p| p.to_vec()) });
            }
        }
        out
    }

    pub fn shows_tab(&self, tab: Tab) -> bool {
        self.visible_tabs().iter().any(|t| t.tab == tab)
    }

    /// Whether a panel of a tab shows in this workspace.
    #[allow(dead_code)]
    pub fn shows_panel(&self, tab: Tab, title: &str) -> bool {
        self.visible_tabs().iter().find(|t| t.tab == tab).is_some_and(|t| t.allows(title))
    }
}

/// The workspace switcher: a prominent button with the current workspace's icon and name, and a
/// menu of all of them. Returns true when another one was picked.
pub fn switcher(ui: &mut egui::Ui, current: &mut usize) -> bool {
    use crate::theme;
    let t = theme::tokens_ui(ui);
    let mut picked = None;
    let ws = &WORKSPACES[(*current).min(WORKSPACES.len() - 1)];
    ui.scope(|ui| {
        let v = ui.visuals_mut();
        for w in [&mut v.widgets.inactive, &mut v.widgets.hovered, &mut v.widgets.active, &mut v.widgets.open] {
            w.weak_bg_fill = t.flat(t.accent.gamma_multiply(if t.dark { 0.55 } else { 0.16 }));
            w.bg_stroke = egui::Stroke::new(1.0_f32, t.accent_text.gamma_multiply(0.7));
        }
        let label = theme::icon_text(ws.glyph, ws.name, theme::FONT_BODY, t.text);
        let resp = ui.menu_button(label, |ui| {
            ui.set_min_width(300.0);
            ui.label(egui::RichText::new("WORKSPACE").size(theme::FONT_SMALL).strong().color(t.text_muted));
            ui.add_space(2.0);
            for (i, w) in WORKSPACES.iter().enumerate() {
                let on = i == *current;
                let r = ui.add(egui::Button::new(theme::icon_text(w.glyph, w.name, theme::FONT_BODY, t.text)).selected(on).min_size(egui::vec2(ui.available_width(), 26.0)).frame(on));
                ui.label(egui::RichText::new(w.blurb).size(theme::FONT_SMALL).color(t.text_muted));
                ui.add_space(2.0);
                if r.clicked() {
                    picked = Some(i);
                    ui.close_menu();
                }
            }
        });
        resp.response.on_hover_text("Workspace: chooses which tabs and tools show. Every command still runs from the palette (Ctrl+K) and the command line.");
    });
    if let Some(i) = picked {
        if i != *current {
            *current = i;
            return true;
        }
    }
    false
}

/// The workspace with this name: the name as written, or its letters alone ("road design",
/// "roaddesign", "signs"), or the start of one.
pub fn index_of(name: &str) -> Option<usize> {
    let norm = |s: &str| s.chars().filter(|c| c.is_alphanumeric()).collect::<String>().to_lowercase();
    let n = norm(name);
    if n.is_empty() {
        return None;
    }
    WORKSPACES.iter().position(|w| norm(w.name) == n).or_else(|| WORKSPACES.iter().position(|w| norm(w.name).starts_with(&n)))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ribbon::TABS;

    #[test]
    fn all_shows_every_tab_and_every_panel() {
        let all = &WORKSPACES[0];
        for (t, _) in TABS {
            assert!(all.shows_tab(t));
        }
        assert!(all.shows_panel(Tab::Roads, "Drainage"));
    }

    #[test]
    fn home_and_annotate_are_in_every_workspace_and_the_project_tabs_follow() {
        for w in WORKSPACES {
            assert!(w.shows_tab(Tab::Home) && w.shows_tab(Tab::Annotate), "{}", w.name);
            assert!(w.shows_tab(Tab::Manage) && w.shows_tab(Tab::Help), "{}", w.name);
            let tabs = w.visible_tabs();
            assert_eq!(tabs[0].tab, Tab::Home);
        }
    }

    #[test]
    fn every_ribbon_panel_is_in_some_workspace_that_is_not_all() {
        // A panel that no workspace shows would be reachable only through All.
        let mut seen = std::collections::HashSet::new();
        for w in WORKSPACES.iter().filter(|w| !w.everything) {
            for t in w.visible_tabs() {
                for p in crate::ribbon::panel_titles(t.tab) {
                    if t.allows(p) {
                        seen.insert((t.tab as u8, p));
                    }
                }
            }
        }
        for (tab, _) in TABS {
            for p in crate::ribbon::panel_titles(tab) {
                assert!(seen.contains(&(tab as u8, p)), "{tab:?} / {p} is in no workspace");
            }
        }
    }

    #[test]
    fn panel_filters_name_panels_that_exist() {
        for w in WORKSPACES {
            for wt in w.tabs {
                if let Some(ps) = wt.panels {
                    let titles = crate::ribbon::panel_titles(wt.tab);
                    for p in ps {
                        assert!(titles.contains(p), "{}: {:?} has no panel {p}", w.name, wt.tab);
                    }
                }
            }
        }
    }

    #[test]
    fn a_shared_tab_shows_only_the_panels_the_workspace_names() {
        let d = &WORKSPACES[index_of("drainage").unwrap()];
        assert!(d.shows_panel(Tab::Roads, "Drainage") && d.shows_panel(Tab::Roads, "Alignment"));
        assert!(!d.shows_panel(Tab::Roads, "Lighting") && !d.shows_panel(Tab::Roads, "Markings"));
        assert!(!d.shows_tab(Tab::FourD));
    }

    #[test]
    fn names_resolve_loosely() {
        assert_eq!(index_of("All"), Some(0));
        assert_eq!(WORKSPACES[index_of("road design").unwrap()].name, "Road Design");
        assert_eq!(WORKSPACES[index_of("RoadDesign").unwrap()].name, "Road Design");
        assert_eq!(WORKSPACES[index_of("signs").unwrap()].name, "Signs and Markings");
        assert_eq!(WORKSPACES[index_of("survey and terrain").unwrap()].name, "Survey and Terrain");
        assert!(index_of("nonsense").is_none() && index_of("").is_none());
        let mut names = std::collections::HashSet::new();
        for w in WORKSPACES {
            assert!(names.insert(w.name));
        }
    }
}
