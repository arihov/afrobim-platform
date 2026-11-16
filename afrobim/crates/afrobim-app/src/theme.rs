//! The design tokens of the interface: one source for colours, spacing, radii, shadow and font
//! sizes, in a dark glass, a light glass and a solid variant, applied through `egui::Style` and
//! `egui::Visuals`.
//!
//! "Glass" here means translucent tinted panels and floating windows (an alpha fill, so the model
//! shows faintly through a window that floats over it), rounded corners, a soft shadow and a thin
//! light border. egui's glow renderer has no live background blur, so there is none: the
//! translucency is plain alpha, and the panels tint a soft gradient painted behind the whole
//! window. "Solid" turns the translucency off.
//!
//! Engineering drawing colours (layers, signs, analysis results) are not tokens and are left alone.

use egui::{epaint::Shadow, Color32, Context, CornerRadius, FontFamily, FontId, Id, Margin, Stroke, TextStyle, Ui};

/// Which look the interface has.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Look {
    DarkGlass,
    LightGlass,
    /// No translucency; dark or light as `Theme::solid_dark` says.
    Solid,
}

impl Look {
    pub const ALL: [Look; 3] = [Look::DarkGlass, Look::LightGlass, Look::Solid];
    pub fn label(self) -> &'static str {
        match self {
            Look::DarkGlass => "Dark glass",
            Look::LightGlass => "Light glass",
            Look::Solid => "Solid",
        }
    }
    pub fn key(self) -> &'static str {
        match self {
            Look::DarkGlass => "dark-glass",
            Look::LightGlass => "light-glass",
            Look::Solid => "solid",
        }
    }
    pub fn from_key(k: &str) -> Option<Look> {
        Look::ALL.into_iter().find(|l| l.key() == k)
    }
}

/// The interface settings the engineer chooses.
#[derive(Clone, Copy, PartialEq, Debug)]
pub struct Theme {
    pub look: Look,
    /// For `Look::Solid`: a dark rather than a light solid palette.
    pub solid_dark: bool,
    /// The interface scale, 1.0 being normal.
    pub scale: f32,
}

impl Default for Theme {
    fn default() -> Self {
        Theme { look: Look::DarkGlass, solid_dark: true, scale: 1.0 }
    }
}

/// `dark-glass`, `light-glass`, `solid-dark` or `solid-light`.
pub fn parse_theme(text: &str) -> Option<Theme> {
    let base = Theme::default();
    match text.trim().to_ascii_lowercase().as_str() {
        "dark-glass" => Some(Theme { look: Look::DarkGlass, ..base }),
        "light-glass" => Some(Theme { look: Look::LightGlass, ..base }),
        "solid-dark" | "solid" => Some(Theme { look: Look::Solid, solid_dark: true, ..base }),
        "solid-light" => Some(Theme { look: Look::Solid, solid_dark: false, ..base }),
        _ => None,
    }
}

pub const SCALES: [f32; 6] = [0.85, 1.0, 1.15, 1.3, 1.5, 1.75];

impl Theme {
    pub fn is_dark(&self) -> bool {
        match self.look {
            Look::DarkGlass => true,
            Look::LightGlass => false,
            Look::Solid => self.solid_dark,
        }
    }
    pub fn glass(&self) -> bool {
        self.look != Look::Solid
    }
    /// Switches between the dark and the light variant of the current look.
    pub fn toggled(mut self) -> Theme {
        match self.look {
            Look::DarkGlass => self.look = Look::LightGlass,
            Look::LightGlass => self.look = Look::DarkGlass,
            Look::Solid => self.solid_dark = !self.solid_dark,
        }
        self
    }
}

/// Every colour and size of one look. Fills may be translucent (premultiplied `Color32`).
#[derive(Clone, Copy, Debug)]
pub struct Tokens {
    pub dark: bool,
    pub glass: bool,
    /// The opaque base behind everything (the window clear colour, and the top of the gradient).
    pub surface: Color32,
    /// The bottom of the soft gradient behind the panels.
    pub surface_low: Color32,
    /// Side, top and bottom panels.
    pub panel: Color32,
    /// Floating windows and menus.
    pub window: Color32,
    /// Buttons and fields at rest, and a header strip.
    pub raised: Color32,
    pub raised_hover: Color32,
    pub raised_active: Color32,
    /// A field's inside.
    pub field: Color32,
    pub border: Color32,
    /// The thin light line along the top edge of a window or panel.
    pub highlight: Color32,
    pub text: Color32,
    pub text_muted: Color32,
    /// Fill of the primary button, and the text on it.
    pub accent: Color32,
    pub text_on_accent: Color32,
    /// The accent as text or a line on a panel.
    pub accent_text: Color32,
    pub success: Color32,
    pub warning: Color32,
    pub danger: Color32,
    pub selection: Color32,
    pub shadow: Color32,
}

fn rgba(r: u8, g: u8, b: u8, a: u8) -> Color32 {
    Color32::from_rgba_unmultiplied(r, g, b, a)
}

impl Tokens {
    pub fn of(theme: &Theme) -> Tokens {
        let dark = theme.is_dark();
        let glass = theme.glass();
        if dark {
            Tokens {
                dark,
                glass,
                surface: Color32::from_rgb(0x0F, 0x14, 0x1C),
                surface_low: Color32::from_rgb(0x08, 0x0B, 0x10),
                panel: if glass { rgba(0x16, 0x1C, 0x26, 214) } else { Color32::from_rgb(0x17, 0x1C, 0x25) },
                window: if glass { rgba(0x18, 0x1F, 0x2B, 232) } else { Color32::from_rgb(0x1C, 0x23, 0x2E) },
                raised: rgba(255, 255, 255, 20),
                raised_hover: rgba(255, 255, 255, 36),
                raised_active: rgba(255, 255, 255, 52),
                field: rgba(0, 0, 0, 90),
                border: rgba(255, 255, 255, if glass { 38 } else { 30 }),
                highlight: rgba(255, 255, 255, if glass { 46 } else { 0 }),
                text: Color32::from_rgb(0xE7, 0xEC, 0xF3),
                text_muted: Color32::from_rgb(0xB1, 0xBB, 0xC9),
                accent: Color32::from_rgb(0x2F, 0x6F, 0xDB),
                text_on_accent: Color32::WHITE,
                accent_text: Color32::from_rgb(0x72, 0xB2, 0xFF),
                success: Color32::from_rgb(0x5B, 0xD0, 0x98),
                warning: Color32::from_rgb(0xEE, 0xB8, 0x5E),
                danger: Color32::from_rgb(0xFF, 0x9C, 0x93),
                selection: rgba(0x2F, 0x6F, 0xDB, 140),
                shadow: rgba(0, 0, 0, 120),
            }
        } else {
            Tokens {
                dark,
                glass,
                surface: Color32::from_rgb(0xEE, 0xF1, 0xF6),
                surface_low: Color32::from_rgb(0xDC, 0xE2, 0xEB),
                panel: if glass { rgba(0xFA, 0xFB, 0xFD, 204) } else { Color32::from_rgb(0xF5, 0xF7, 0xFA) },
                window: if glass { rgba(0xFC, 0xFD, 0xFF, 238) } else { Color32::from_rgb(0xFF, 0xFF, 0xFF) },
                raised: rgba(0x1B, 0x28, 0x3C, 14),
                raised_hover: rgba(0x1B, 0x28, 0x3C, 28),
                raised_active: rgba(0x1B, 0x28, 0x3C, 44),
                field: rgba(255, 255, 255, 190),
                border: rgba(0x1B, 0x28, 0x3C, if glass { 46 } else { 52 }),
                highlight: rgba(255, 255, 255, if glass { 230 } else { 0 }),
                text: Color32::from_rgb(0x18, 0x20, 0x2C),
                text_muted: Color32::from_rgb(0x4B, 0x58, 0x6A),
                accent: Color32::from_rgb(0x1D, 0x5F, 0xD0),
                text_on_accent: Color32::WHITE,
                accent_text: Color32::from_rgb(0x17, 0x52, 0xB8),
                success: Color32::from_rgb(0x0E, 0x66, 0x40),
                warning: Color32::from_rgb(0x7D, 0x4C, 0x00),
                danger: Color32::from_rgb(0xA3, 0x1F, 0x18),
                selection: rgba(0x1D, 0x5F, 0xD0, 58),
                shadow: rgba(0x10, 0x20, 0x40, 64),
            }
        }
    }
}

// Sizes: one place for spacing, radii and type.
pub const RADIUS_CONTROL: u8 = 6;
pub const RADIUS_WINDOW: u8 = 12;
pub const RADIUS_MENU: u8 = 8;
pub const SP_XS: f32 = 4.0;
pub const SP_SM: f32 = 8.0;
pub const SP_MD: f32 = 12.0;
pub const SP_LG: f32 = 16.0;
pub const FONT_SMALL: f32 = 11.5;
pub const FONT_BODY: f32 = 13.5;
pub const FONT_HEADING: f32 = 16.0;
pub const FONT_TITLE: f32 = 14.5;
pub const FONT_MONO: f32 = 12.5;

static GLOBAL: std::sync::RwLock<Option<Tokens>> = std::sync::RwLock::new(None);

fn global() -> Tokens {
    GLOBAL.read().ok().and_then(|g| *g).unwrap_or_else(|| Tokens::of(&Theme::default()))
}

/// The semantic colours of the look in force, for code that has no context at hand.
pub fn success() -> Color32 {
    global().success
}
pub fn warn() -> Color32 {
    global().warning
}
pub fn danger() -> Color32 {
    global().danger
}
pub fn muted() -> Color32 {
    global().text_muted
}
pub fn accent_text() -> Color32 {
    global().accent_text
}

const TOKENS_ID: &str = "afrobim-theme-tokens";
const THEME_ID: &str = "afrobim-theme";

/// The tokens in force (the last `apply`), for any code that has a context.
pub fn tokens(ctx: &Context) -> Tokens {
    ctx.data(|d| d.get_temp::<Tokens>(Id::new(TOKENS_ID))).unwrap_or_else(|| Tokens::of(&Theme::default()))
}

pub fn tokens_ui(ui: &Ui) -> Tokens {
    tokens(ui.ctx())
}

/// The theme in force.
pub fn current(ctx: &Context) -> Theme {
    ctx.data(|d| d.get_temp::<Theme>(Id::new(THEME_ID))).unwrap_or_default()
}

/// The font family of the icon glyphs.
pub fn icon_family() -> FontFamily {
    FontFamily::Name("icons".into())
}

pub fn icon_font(size: f32) -> FontId {
    FontId::new(size, icon_family())
}

/// A layout of an icon glyph followed by text, for buttons, menu items and titles.
pub fn icon_text(glyph: &str, text: &str, size: f32, colour: Color32) -> egui::text::LayoutJob {
    let mut job = egui::text::LayoutJob::default();
    job.append(glyph, 0.0, egui::TextFormat { font_id: icon_font(size + 1.0), color: colour, valign: egui::Align::Center, ..Default::default() });
    if !text.is_empty() {
        job.append(text, 7.0, egui::TextFormat { font_id: FontId::new(size, FontFamily::Proportional), color: colour, valign: egui::Align::Center, ..Default::default() });
    }
    job
}

/// The user-interface fonts: Inter for text (SIL OFL 1.1) and the Phosphor icon font (MIT), both
/// bundled in the binary. The egui default fonts stay behind them for any glyph they lack.
pub fn install_fonts(ctx: &Context) {
    let mut fonts = egui::FontDefinitions::default();
    fonts.font_data.insert("inter".into(), std::sync::Arc::new(egui::FontData::from_static(include_bytes!("../assets/fonts/Inter-Variable.ttf"))));
    if let Some(list) = fonts.families.get_mut(&FontFamily::Proportional) {
        list.insert(0, "inter".into());
    }
    // The icons live in a family of their own: Inter has glyphs in the same private-use range,
    // and the icon font is made of ligatures that would otherwise catch ordinary letters.
    fonts.font_data.insert("phosphor".into(), egui_phosphor::Variant::Regular.font_data().into());
    fonts.families.insert(icon_family(), vec!["phosphor".into()]);
    ctx.set_fonts(fonts);
}

fn build_visuals(t: &Tokens) -> egui::Visuals {
    let mut v = if t.dark { egui::Visuals::dark() } else { egui::Visuals::light() };
    let cr = CornerRadius::same(RADIUS_CONTROL);
    v.panel_fill = t.panel;
    v.window_fill = t.window;
    v.window_stroke = Stroke::new(1.0, t.border);
    v.window_corner_radius = CornerRadius::same(RADIUS_WINDOW);
    v.menu_corner_radius = CornerRadius::same(RADIUS_MENU);
    v.window_shadow = Shadow { offset: [0, 10], blur: 30, spread: 0, color: t.shadow };
    v.popup_shadow = Shadow { offset: [0, 6], blur: 18, spread: 0, color: t.shadow };
    v.extreme_bg_color = t.field;
    v.faint_bg_color = if t.dark { rgba(255, 255, 255, 9) } else { rgba(0x1B, 0x28, 0x3C, 9) };
    v.code_bg_color = t.raised;
    v.hyperlink_color = t.accent_text;
    v.warn_fg_color = t.warning;
    v.error_fg_color = t.danger;
    v.override_text_color = None;
    v.selection.bg_fill = t.selection;
    v.selection.stroke = Stroke::new(1.0, t.text);
    v.striped = false;
    v.window_highlight_topmost = false;

    let w = &mut v.widgets;
    w.noninteractive.bg_fill = t.panel;
    w.noninteractive.weak_bg_fill = t.panel;
    w.noninteractive.bg_stroke = Stroke::new(1.0, t.border);
    w.noninteractive.fg_stroke = Stroke::new(1.0, t.text);
    w.noninteractive.corner_radius = cr;
    for (wv, fill, stroke) in [
        (&mut w.inactive, t.raised, Stroke::new(1.0, t.border)),
        (&mut w.hovered, t.raised_hover, Stroke::new(1.0, t.accent_text)),
        (&mut w.active, t.raised_active, Stroke::new(1.0, t.accent_text)),
        (&mut w.open, t.raised_hover, Stroke::new(1.0, t.border)),
    ] {
        wv.bg_fill = fill;
        wv.weak_bg_fill = fill;
        wv.bg_stroke = stroke;
        wv.fg_stroke = Stroke::new(1.0, t.text);
        wv.corner_radius = cr;
        wv.expansion = 0.0;
    }
    w.hovered.fg_stroke = Stroke::new(1.5, t.text);
    w.active.fg_stroke = Stroke::new(1.5, t.text);
    // `RichText::weak` mixes the text colour half way towards this fill, so choose the fill that
    // lands on the muted text colour.
    let mix = |m: u8, x: u8| (2 * m as i32 - x as i32).clamp(0, 255) as u8;
    w.noninteractive.weak_bg_fill = Color32::from_rgb(mix(t.text_muted.r(), t.text.r()), mix(t.text_muted.g(), t.text.g()), mix(t.text_muted.b(), t.text.b()));
    v
}

/// Applies the theme: visuals, style and scale. Cheap enough to call when anything changes.
pub fn apply(ctx: &Context, theme: &Theme) {
    let t = Tokens::of(theme);
    let v = build_visuals(&t);
    if let Ok(mut g) = GLOBAL.write() {
        *g = Some(t);
    }
    ctx.data_mut(|d| {
        d.insert_temp(Id::new(TOKENS_ID), t);
        d.insert_temp(Id::new(THEME_ID), *theme);
    });
    ctx.set_zoom_factor(theme.scale.clamp(0.7, 2.0));

    ctx.set_visuals(v);

    ctx.style_mut(|s| {
        s.spacing.item_spacing = egui::vec2(SP_SM, 6.0);
        s.spacing.button_padding = egui::vec2(10.0, 4.0);
        s.spacing.interact_size = egui::vec2(36.0, 26.0);
        s.spacing.window_margin = Margin::same(SP_MD as i8);
        s.spacing.menu_margin = Margin::same(6);
        s.spacing.indent = 16.0;
        s.spacing.combo_width = 150.0;
        s.spacing.text_edit_width = 160.0;
        s.spacing.slider_width = 150.0;
        s.spacing.scroll.bar_width = 8.0;
        s.spacing.scroll.floating = true;
        s.interaction.tooltip_delay = 0.35;
        s.text_styles = [
            (TextStyle::Heading, FontId::new(FONT_HEADING, FontFamily::Proportional)),
            (TextStyle::Body, FontId::new(FONT_BODY, FontFamily::Proportional)),
            (TextStyle::Button, FontId::new(FONT_BODY, FontFamily::Proportional)),
            (TextStyle::Small, FontId::new(FONT_SMALL, FontFamily::Proportional)),
            (TextStyle::Monospace, FontId::new(FONT_MONO, FontFamily::Monospace)),
        ]
        .into();
    });
}

/// A soft gradient behind every panel, so translucent panels have something to tint. Painted on
/// the background layer, once a frame, before any panel.
pub fn paint_backdrop(ctx: &Context) {
    let t = tokens(ctx);
    let rect = ctx.screen_rect();
    let painter = ctx.layer_painter(egui::LayerId::background());
    let mut mesh = egui::Mesh::default();
    let (top, bottom) = (t.surface, t.surface_low);
    let i = mesh.vertices.len() as u32;
    for (p, c) in [(rect.left_top(), top), (rect.right_top(), top), (rect.right_bottom(), bottom), (rect.left_bottom(), bottom)] {
        mesh.vertices.push(egui::epaint::Vertex { pos: p, uv: egui::epaint::WHITE_UV, color: c });
    }
    mesh.indices.extend_from_slice(&[i, i + 1, i + 2, i, i + 2, i + 3]);
    painter.add(egui::Shape::mesh(mesh));
}

/// The frame of a floating window or a dialog: translucent fill, rounded, soft shadow, thin border.
pub fn window_frame(ctx: &Context) -> egui::Frame {
    let t = tokens(ctx);
    egui::Frame::new()
        .fill(t.window)
        .stroke(Stroke::new(1.0, t.border))
        .corner_radius(CornerRadius::same(RADIUS_WINDOW))
        .shadow(Shadow { offset: [0, 10], blur: 30, spread: 0, color: t.shadow })
        .inner_margin(Margin::same(SP_MD as i8))
}

/// Paints the subtle highlight along the top edge of `rect` (a window, a header or a panel).
pub fn paint_top_highlight(painter: &egui::Painter, rect: egui::Rect, tokens: &Tokens, radius: f32) {
    if tokens.highlight.a() == 0 {
        return;
    }
    let y = rect.top() + 0.5;
    painter.hline((rect.left() + radius)..=(rect.right() - radius), y, Stroke::new(1.0, tokens.highlight));
}

/// `fg` over `bg`, as the colour a viewer sees.
pub fn over(fg: Color32, bg: Color32) -> Color32 {
    let a = fg.a() as f32 / 255.0;
    let mix = |f: u8, b: u8| (f as f32 * a + b as f32 * (1.0 - a)).round() as u8;
    let [fr, fg_, fb, _] = fg.to_srgba_unmultiplied();
    Color32::from_rgb(mix(fr, bg.r()), mix(fg_, bg.g()), mix(fb, bg.b()))
}

fn luminance(c: Color32) -> f64 {
    let lin = |v: u8| {
        let s = v as f64 / 255.0;
        if s <= 0.03928 {
            s / 12.92
        } else {
            ((s + 0.055) / 1.055).powf(2.4)
        }
    };
    0.2126 * lin(c.r()) + 0.7152 * lin(c.g()) + 0.0722 * lin(c.b())
}

/// The WCAG contrast ratio of two opaque colours, 1 to 21.
pub fn contrast(a: Color32, b: Color32) -> f64 {
    let (la, lb) = (luminance(a), luminance(b));
    let (hi, lo) = if la > lb { (la, lb) } else { (lb, la) };
    (hi + 0.05) / (lo + 0.05)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn all_themes() -> Vec<Theme> {
        vec![
            Theme { look: Look::DarkGlass, ..Theme::default() },
            Theme { look: Look::LightGlass, ..Theme::default() },
            Theme { look: Look::Solid, solid_dark: true, scale: 1.0 },
            Theme { look: Look::Solid, solid_dark: false, scale: 1.0 },
        ]
    }

    #[test]
    fn contrast_of_black_on_white_is_21() {
        assert!((contrast(Color32::BLACK, Color32::WHITE) - 21.0).abs() < 1e-9);
        assert!((contrast(Color32::WHITE, Color32::WHITE) - 1.0).abs() < 1e-9);
    }

    /// The backgrounds text sits on, as seen: panels over the gradient, windows over the worst
    /// canvas behind them (white and black, the extremes a drawing can have), raised fills over those.
    fn backgrounds(t: &Tokens) -> Vec<(&'static str, Color32)> {
        let mut out = Vec::new();
        for (name, base) in [("surface", t.surface), ("surface_low", t.surface_low), ("white canvas", Color32::WHITE), ("black canvas", Color32::BLACK)] {
            let panel_or_window = if name.contains("canvas") { t.window } else { t.panel };
            let p = over(panel_or_window, base);
            out.push((name, p));
            out.push(("raised", over(t.raised, p)));
            out.push(("raised hover", over(t.raised_hover, p)));
            out.push(("raised active", over(t.raised_active, p)));
            out.push(("field", over(t.field, p)));
            out.push(("selection", over(t.selection, p)));
        }
        out
    }

    #[test]
    fn body_text_has_at_least_4_5_to_1_on_every_background() {
        for theme in all_themes() {
            let t = Tokens::of(&theme);
            for (name, bg) in backgrounds(&t) {
                let c = contrast(t.text, bg);
                assert!(c >= 4.5, "{:?} text on {name}: {c:.2}", theme.look);
            }
        }
    }

    #[test]
    fn muted_text_and_the_coloured_text_tokens_have_at_least_4_5_to_1_on_panels_and_windows() {
        for theme in all_themes() {
            let t = Tokens::of(&theme);
            for (name, bg) in backgrounds(&t).into_iter().filter(|(n, _)| !n.contains("selection") && !n.contains("hover") && !n.contains("active")) {
                for (what, colour) in [("muted", t.text_muted), ("accent text", t.accent_text), ("success", t.success), ("warning", t.warning), ("danger", t.danger)] {
                    let c = contrast(colour, bg);
                    assert!(c >= 4.5, "{:?} {what} on {name}: {c:.2}", theme.look);
                }
            }
        }
    }

    #[test]
    fn weak_text_as_egui_draws_it_is_readable_on_panels() {
        for theme in all_themes() {
            let t = Tokens::of(&theme);
            let weak = build_visuals(&t).weak_text_color();
            for (name, bg) in backgrounds(&t).into_iter().filter(|(n, _)| !n.contains("selection") && !n.contains("hover") && !n.contains("active")) {
                let c = contrast(weak, bg);
                assert!(c >= 4.5, "{:?} weak text on {name}: {c:.2}", theme.look);
            }
        }
    }

    #[test]
    fn the_primary_button_text_is_readable_on_the_accent() {
        for theme in all_themes() {
            let t = Tokens::of(&theme);
            let c = contrast(t.text_on_accent, t.accent);
            assert!(c >= 4.5, "{:?}: {c:.2}", theme.look);
        }
    }

    #[test]
    fn solid_has_no_translucent_fills_and_glass_has() {
        for theme in all_themes() {
            let t = Tokens::of(&theme);
            if theme.look == Look::Solid {
                assert!(t.panel.a() == 255 && t.window.a() == 255);
            } else {
                assert!(t.panel.a() < 255 && t.window.a() < 255);
            }
        }
    }

    #[test]
    fn toggling_switches_dark_and_light_and_keys_round_trip() {
        let t = Theme::default();
        assert!(t.is_dark() && !t.toggled().is_dark() && t.toggled().toggled() == t);
        let s = Theme { look: Look::Solid, solid_dark: true, scale: 1.0 };
        assert!(!s.toggled().is_dark());
        for l in Look::ALL {
            assert_eq!(Look::from_key(l.key()), Some(l));
        }
    }
}
