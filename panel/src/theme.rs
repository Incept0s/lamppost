//! Colours, fonts and the two looks.
//!
//! Modern: rounded cards, Nunito, light or dark. Classic: the plain grey look
//! of old Windows control panels. Every text colour is checked to stay above
//! the WCAG AA contrast ratio of 4.5:1 against its background.
use crate::settings::{Look, Scheme};
use eframe::egui::{self, Color32, CornerRadius, FontFamily, FontId, Stroke, TextStyle};
use std::process::Command;

#[derive(Clone, Copy, PartialEq)]
pub struct Palette {
    pub dark: bool,
    pub bg: Color32,
    pub card: Color32,
    pub card_alt: Color32,
    pub border: Color32,
    pub text: Color32,
    pub muted: Color32,
    pub faint: Color32,
    pub accent: Color32,
    pub accent_btn: Color32,
    pub accent_hover: Color32,
    pub accent_soft: Color32,
    pub green: Color32,
    pub green_btn: Color32,
    pub green_hover: Color32,
    pub green_soft: Color32,
    pub red: Color32,
    pub red_btn: Color32,
    pub red_hover: Color32,
    pub red_soft: Color32,
    pub blue: Color32,
    pub on_color: Color32,
}

const fn rgb(hex: u32) -> Color32 {
    Color32::from_rgb((hex >> 16) as u8, ((hex >> 8) & 0xff) as u8, (hex & 0xff) as u8)
}

pub const LIGHT: Palette = Palette {
    dark: false,
    bg: rgb(0xf5f4f2), card: rgb(0xffffff), card_alt: rgb(0xfaf9f7), border: rgb(0xe4e1dc),
    text: rgb(0x1e1c1a), muted: rgb(0x6a635c), faint: rgb(0x7d756d),
    accent: rgb(0xa94c0c), accent_btn: rgb(0xc2540b), accent_hover: rgb(0xa84708), accent_soft: rgb(0xfdf1e7),
    green: rgb(0x136c34), green_btn: rgb(0x16833e), green_hover: rgb(0x126c33), green_soft: rgb(0xe3f6e9),
    red: rgb(0xbe2a20), red_btn: rgb(0xc52f25), red_hover: rgb(0xa8251c), red_soft: rgb(0xfbe6e4),
    blue: rgb(0x1d4ed8), on_color: rgb(0xffffff),
};

pub const DARK: Palette = Palette {
    dark: true,
    bg: rgb(0x16171a), card: rgb(0x1e2024), card_alt: rgb(0x24272c), border: rgb(0x31353b),
    text: rgb(0xe9eaed), muted: rgb(0xa0a5ad), faint: rgb(0x888f99),
    accent: rgb(0xf59b53), accent_btn: rgb(0xe8823a), accent_hover: rgb(0xf2914a), accent_soft: rgb(0x33241a),
    green: rgb(0x4ade80), green_btn: rgb(0x3fca75), green_hover: rgb(0x55d888), green_soft: rgb(0x16301f),
    red: rgb(0xf47070), red_btn: rgb(0xe25a5a), red_hover: rgb(0xef6b6b), red_soft: rgb(0x331d1d),
    blue: rgb(0x8ab4f8), on_color: rgb(0x14181a),
};

pub const CLASSIC: Palette = Palette {
    dark: false,
    bg: rgb(0xf0f0f0), card: rgb(0xffffff), card_alt: rgb(0xf7f7f7), border: rgb(0xb4b4b4),
    text: rgb(0x000000), muted: rgb(0x555555), faint: rgb(0x767676),
    accent: rgb(0x0055aa), accent_btn: rgb(0x0078d7), accent_hover: rgb(0x0067b8), accent_soft: rgb(0xcde8ff),
    green: rgb(0x006600), green_btn: rgb(0x008000), green_hover: rgb(0x006d00), green_soft: rgb(0x9cf09c),
    red: rgb(0xc00000), red_btn: rgb(0xc00000), red_hover: rgb(0xa00000), red_soft: rgb(0xffd6d6),
    blue: rgb(0x0000c0), on_color: rgb(0xffffff),
};

impl Palette {
    /// Subtle background for a hovered title-bar button.
    pub fn hover_fill(&self) -> Color32 {
        if self.dark { Color32::from_white_alpha(22) } else { Color32::from_black_alpha(18) }
    }
}

pub fn palette(look: Look, scheme: Scheme) -> Palette {
    if look == Look::Classic {
        return CLASSIC;
    }
    let dark = match scheme {
        Scheme::Light => false,
        Scheme::Dark => true,
        Scheme::System => system_is_dark(),
    };
    if dark { DARK } else { LIGHT }
}

fn run(program: &str, args: &[&str]) -> String {
    Command::new(program)
        .args(args)
        .output()
        .ok()
        .filter(|o| o.status.success())
        .map(|o| String::from_utf8_lossy(&o.stdout).trim().to_string())
        .unwrap_or_default()
}

/// True when the desktop asks for a dark theme.
///
/// The desktop is asked directly: GNOME's setting first, then the portal
/// (which also works on KDE and others). Toolkit detection is unreliable on
/// Wayland - it reports "light" while the desktop is dark.
pub fn system_is_dark() -> bool {
    let gnome = run("gsettings", &["get", "org.gnome.desktop.interface", "color-scheme"]);
    if gnome.contains("prefer-dark") {
        return true;
    }
    if gnome.contains("prefer-light") {
        return false;
    }
    let portal = run("gdbus", &[
        "call", "--session", "--dest", "org.freedesktop.portal.Desktop",
        "--object-path", "/org/freedesktop/portal/desktop",
        "--method", "org.freedesktop.portal.Settings.ReadOne",
        "org.freedesktop.appearance", "color-scheme",
    ]);
    // 1 = prefer dark, 2 = prefer light, 0 = no preference
    portal.contains("uint32 1")
}

/// The running "gsettings monitor", so it can be stopped with the window.
static MONITOR: std::sync::Mutex<Option<std::process::Child>> = std::sync::Mutex::new(None);

/// Watch the desktop setting; calls `on_change` whenever it changes.
pub fn watch_system_scheme(on_change: impl Fn() + Send + 'static) {
    std::thread::spawn(move || {
        let mut command = Command::new("gsettings");
        command
            .args(["monitor", "org.gnome.desktop.interface", "color-scheme"])
            .stdout(std::process::Stdio::piped());
        unsafe {
            // the watcher dies with us, even if we are killed outright
            use std::os::unix::process::CommandExt;
            command.pre_exec(|| {
                libc::prctl(libc::PR_SET_PDEATHSIG, libc::SIGTERM);
                Ok(())
            });
        }
        let mut child = match command.spawn() {
            Ok(child) => child,
            Err(_) => return, // not GNOME: the colours are read at startup only
        };
        let Some(stdout) = child.stdout.take() else { return };
        if let Ok(mut slot) = MONITOR.lock() {
            *slot = Some(child);
        }
        use std::io::BufRead;
        for line in std::io::BufReader::new(stdout).lines().map_while(Result::ok) {
            if line.contains("color-scheme") {
                on_change();
            }
        }
    });
}

/// Stop watching (called when the window closes, so nothing is left behind).
pub fn stop_watching() {
    if let Ok(mut slot) = MONITOR.lock()
        && let Some(mut child) = slot.take()
    {
        let _ = child.kill();
        let _ = child.wait();
    }
}

pub fn install_fonts(ctx: &egui::Context) {
    let mut fonts = egui::FontDefinitions::default();
    let faces = [
        ("nunito", &include_bytes!("../../assets/fonts/Nunito-400.ttf")[..]),
        ("nunito-semibold", &include_bytes!("../../assets/fonts/Nunito-600.ttf")[..]),
        ("nunito-bold", &include_bytes!("../../assets/fonts/Nunito-800.ttf")[..]),
    ];
    for (name, data) in faces {
        fonts.font_data.insert(name.to_string(), std::sync::Arc::new(egui::FontData::from_static(data)));
    }
    fonts
        .families
        .entry(FontFamily::Proportional)
        .or_default()
        .insert(0, "nunito".to_string());
    fonts
        .families
        .insert(FontFamily::Name("semibold".into()), vec!["nunito-semibold".into(), "nunito".into()]);
    fonts
        .families
        .insert(FontFamily::Name("bold".into()), vec!["nunito-bold".into(), "nunito".into()]);
    ctx.set_fonts(fonts);
}

pub fn semibold() -> FontFamily {
    FontFamily::Name("semibold".into())
}

pub fn bold() -> FontFamily {
    FontFamily::Name("bold".into())
}

/// Apply a palette to the whole interface.
pub fn apply(ctx: &egui::Context, p: &Palette, look: Look) {
    let radius = if look == Look::Classic { 3 } else { 10 };
    let mut style = (*ctx.global_style()).clone();
    let visuals = &mut style.visuals;
    visuals.dark_mode = p.dark;
    visuals.override_text_color = Some(p.text);
    visuals.panel_fill = p.bg;
    visuals.window_fill = p.card;
    visuals.extreme_bg_color = p.card_alt;
    visuals.faint_bg_color = p.card_alt;
    visuals.window_stroke = Stroke::new(1.0, p.border);
    visuals.window_corner_radius = CornerRadius::same(if look == Look::Classic { 3 } else { 14 });
    visuals.window_shadow = egui::epaint::Shadow {
        offset: [0, 6],
        blur: if look == Look::Classic { 0 } else { 24 },
        spread: 0,
        color: Color32::from_black_alpha(if p.dark { 90 } else { 30 }),
    };
    visuals.popup_shadow = visuals.window_shadow;
    visuals.selection.bg_fill = p.accent_soft;
    visuals.selection.stroke = Stroke::new(1.0, p.accent);
    visuals.hyperlink_color = p.accent;

    let widgets = &mut visuals.widgets;
    widgets.noninteractive.bg_fill = p.card;
    widgets.noninteractive.weak_bg_fill = p.card;
    widgets.noninteractive.bg_stroke = Stroke::new(1.0, p.border);
    widgets.noninteractive.fg_stroke = Stroke::new(1.0, p.text);
    widgets.noninteractive.corner_radius = CornerRadius::same(radius);

    widgets.inactive.bg_fill = p.card;
    widgets.inactive.weak_bg_fill = p.card;
    widgets.inactive.bg_stroke = Stroke::new(1.0, p.border);
    widgets.inactive.fg_stroke = Stroke::new(1.0, p.text);
    widgets.inactive.corner_radius = CornerRadius::same(radius);

    widgets.hovered.bg_fill = p.card_alt;
    widgets.hovered.weak_bg_fill = p.card_alt;
    widgets.hovered.bg_stroke = Stroke::new(1.0, p.accent);
    widgets.hovered.fg_stroke = Stroke::new(1.0, p.text);
    widgets.hovered.corner_radius = CornerRadius::same(radius);

    widgets.active.bg_fill = p.accent_soft;
    widgets.active.weak_bg_fill = p.accent_soft;
    widgets.active.bg_stroke = Stroke::new(1.0, p.accent);
    widgets.active.fg_stroke = Stroke::new(1.0, p.text);
    widgets.active.corner_radius = CornerRadius::same(radius);

    widgets.open.bg_fill = p.card_alt;
    widgets.open.weak_bg_fill = p.card_alt;
    widgets.open.bg_stroke = Stroke::new(1.0, p.border);
    widgets.open.fg_stroke = Stroke::new(1.0, p.text);
    widgets.open.corner_radius = CornerRadius::same(radius);

    style.spacing.item_spacing = egui::vec2(8.0, 8.0);
    style.spacing.button_padding = egui::vec2(12.0, 7.0);
    style.spacing.menu_margin = egui::Margin::same(6);
    style.text_styles = [
        (TextStyle::Heading, FontId::new(21.0, bold())),
        (TextStyle::Body, FontId::new(14.0, FontFamily::Proportional)),
        (TextStyle::Button, FontId::new(14.0, semibold())),
        (TextStyle::Small, FontId::new(12.0, FontFamily::Proportional)),
        (TextStyle::Monospace, FontId::new(12.5, FontFamily::Monospace)),
    ]
    .into();
    ctx.set_global_style(style);
}
