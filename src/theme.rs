//! YCM Batch Scheduler Theme definitions

use gpui::{hsla, App, Hsla};
use gpui_component::{Theme, ThemeMode};

pub const fn hsl(h: f32, s: f32, l: f32) -> Hsla {
    Hsla {
        h: h / 360.0,
        s: s / 100.0,
        l: l / 100.0,
        a: 1.0,
    }
}

pub const BACKGROUND: Hsla = hsl(224.0, 71.0, 4.0);
pub const FOREGROUND: Hsla = hsl(213.0, 31.0, 91.0);
pub const CARD: Hsla = hsl(224.0, 71.0, 8.0);
pub const PRIMARY: Hsla = hsl(210.0, 100.0, 50.0);
pub const SECONDARY: Hsla = hsl(222.0, 47.0, 14.0);
pub const MUTED_FOREGROUND: Hsla = hsl(215.0, 20.0, 65.0);
pub const ACCENT: Hsla = hsl(216.0, 34.0, 17.0);
pub const BORDER: Hsla = hsl(216.0, 34.0, 20.0);
pub const SUCCESS: Hsla = hsl(142.0, 76.0, 42.0);
pub const WARNING: Hsla = hsl(38.0, 92.0, 50.0);
pub const DANGER: Hsla = hsl(0.0, 84.0, 60.0);

fn lighten(c: Hsla, by: f32) -> Hsla {
    hsla(c.h, c.s, (c.l + by).min(1.0), c.a)
}

fn darken(c: Hsla, by: f32) -> Hsla {
    hsla(c.h, c.s, (c.l - by).max(0.0), c.a)
}

pub fn apply(cx: &mut App) {
    Theme::change(ThemeMode::Dark, None, cx);
    let t = Theme::global_mut(cx);

    if cfg!(windows) {
        t.font_family = "Microsoft JhengHei UI".into();
    }
    t.radius = gpui::px(8.0);
    t.radius_lg = gpui::px(10.0);

    t.background = BACKGROUND;
    t.foreground = FOREGROUND;
    t.border = BORDER;
    t.input = BORDER;
    t.ring = PRIMARY;
    t.popover = CARD;
    t.popover_foreground = FOREGROUND;

    t.primary = PRIMARY;
    t.primary_hover = lighten(PRIMARY, 0.05);
    t.primary_active = darken(PRIMARY, 0.05);
    t.primary_foreground = hsl(0.0, 0.0, 100.0);

    t.secondary = SECONDARY;
    t.secondary_hover = ACCENT;
    t.secondary_active = lighten(ACCENT, 0.03);
    t.secondary_foreground = FOREGROUND;

    t.muted = SECONDARY;
    t.muted_foreground = MUTED_FOREGROUND;
    t.accent = ACCENT;
    t.accent_foreground = FOREGROUND;

    t.danger = DANGER;
    t.danger_hover = lighten(DANGER, 0.05);
    t.danger_active = darken(DANGER, 0.05);
    t.danger_foreground = hsl(0.0, 0.0, 100.0);

    t.success = SUCCESS;
    t.success_hover = lighten(SUCCESS, 0.05);
    t.success_active = darken(SUCCESS, 0.05);
    t.success_foreground = hsl(0.0, 0.0, 100.0);
}
