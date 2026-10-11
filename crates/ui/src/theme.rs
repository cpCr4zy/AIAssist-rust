//! Themes: Dark is Catppuccin Mocha, Light is Catppuccin Latte, System is egui's own light and dark
//! looks, following the operating system.

use aiassist_core::Theme;
use egui::{Color32, Stroke, Visuals};

struct Palette {
    dark: bool,
    base: Color32,
    mantle: Color32,
    crust: Color32,
    surface0: Color32,
    surface1: Color32,
    surface2: Color32,
    overlay0: Color32,
    text: Color32,
    blue: Color32,
    lavender: Color32,
    red: Color32,
    yellow: Color32,
}

const MOCHA: Palette = Palette {
    dark: true,
    base: Color32::from_rgb(0x1e, 0x1e, 0x2e),
    mantle: Color32::from_rgb(0x18, 0x18, 0x25),
    crust: Color32::from_rgb(0x11, 0x11, 0x1b),
    surface0: Color32::from_rgb(0x31, 0x32, 0x44),
    surface1: Color32::from_rgb(0x45, 0x47, 0x5a),
    surface2: Color32::from_rgb(0x58, 0x5b, 0x70),
    overlay0: Color32::from_rgb(0x6c, 0x70, 0x86),
    text: Color32::from_rgb(0xcd, 0xd6, 0xf4),
    blue: Color32::from_rgb(0x89, 0xb4, 0xfa),
    lavender: Color32::from_rgb(0xb4, 0xbe, 0xfe),
    red: Color32::from_rgb(0xf3, 0x8b, 0xa8),
    yellow: Color32::from_rgb(0xf9, 0xe2, 0xaf),
};

const LATTE: Palette = Palette {
    dark: false,
    base: Color32::from_rgb(0xef, 0xf1, 0xf5),
    mantle: Color32::from_rgb(0xe6, 0xe9, 0xef),
    crust: Color32::from_rgb(0xdc, 0xe0, 0xe8),
    surface0: Color32::from_rgb(0xcc, 0xd0, 0xda),
    surface1: Color32::from_rgb(0xbc, 0xc0, 0xcc),
    surface2: Color32::from_rgb(0xac, 0xb0, 0xbe),
    overlay0: Color32::from_rgb(0x9c, 0xa0, 0xb0),
    text: Color32::from_rgb(0x4c, 0x4f, 0x69),
    blue: Color32::from_rgb(0x1e, 0x66, 0xf5),
    lavender: Color32::from_rgb(0x72, 0x87, 0xfd),
    red: Color32::from_rgb(0xd2, 0x0f, 0x39),
    yellow: Color32::from_rgb(0xdf, 0x8e, 0x1d),
};

fn catppuccin(p: &Palette) -> Visuals {
    let mut v = if p.dark { Visuals::dark() } else { Visuals::light() };
    v.panel_fill = p.base;
    v.window_fill = p.base;
    v.window_stroke = Stroke::new(1.0, p.surface1);
    v.extreme_bg_color = p.mantle;
    v.faint_bg_color = p.mantle;
    v.code_bg_color = p.mantle;
    v.hyperlink_color = p.lavender;
    v.warn_fg_color = p.yellow;
    v.error_fg_color = p.red;
    v.selection.bg_fill = p.blue;
    v.selection.stroke = Stroke::new(1.0, p.crust);

    let w = &mut v.widgets;
    w.noninteractive.bg_fill = p.base;
    w.noninteractive.weak_bg_fill = p.base;
    w.noninteractive.bg_stroke = Stroke::new(1.0, p.surface0);
    w.noninteractive.fg_stroke = Stroke::new(1.0, p.text);
    for (state, fill, stroke) in [
        (&mut w.inactive, p.surface0, Stroke::NONE),
        (&mut w.hovered, p.surface1, Stroke::new(1.0, p.overlay0)),
        (&mut w.active, p.surface2, Stroke::new(1.0, p.blue)),
        (&mut w.open, p.surface0, Stroke::new(1.0, p.surface1)),
    ] {
        state.bg_fill = fill;
        state.weak_bg_fill = fill;
        state.bg_stroke = stroke;
        state.fg_stroke = Stroke::new(1.0, p.text);
    }
    v
}

pub fn apply(ctx: &egui::Context, theme: Theme) {
    match theme {
        Theme::Dark => {
            ctx.set_visuals_of(egui::Theme::Dark, catppuccin(&MOCHA));
            ctx.set_theme(egui::ThemePreference::Dark);
        }
        Theme::Light => {
            ctx.set_visuals_of(egui::Theme::Light, catppuccin(&LATTE));
            ctx.set_theme(egui::ThemePreference::Light);
        }
        Theme::System => {
            ctx.set_visuals_of(egui::Theme::Dark, Visuals::dark());
            ctx.set_visuals_of(egui::Theme::Light, Visuals::light());
            ctx.set_theme(egui::ThemePreference::System);
        }
    }
    log::debug!("Applied theme: {}", theme.as_str());
}
