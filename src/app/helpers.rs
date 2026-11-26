//! 共用的小型 UI 元件與工具函式

use super::*;

/// 解析 HH:MM（容許 9:30），回傳標準化的 "09:30"
pub(super) fn parse_hhmm(raw: &str) -> Option<String> {
    let (h, m) = raw.trim().split_once(':')?;
    let h: u32 = h.trim().parse().ok()?;
    let m: u32 = m.trim().parse().ok()?;
    if h < 24 && m < 60 {
        Some(format!("{:02}:{:02}", h, m))
    } else {
        None
    }
}

// ---------------- Helper Components ----------------

pub(super) fn nav_vertical(
    id: &'static str,
    title: &'static str,
    is_active: bool,
    disabled: bool,
    on_click: impl Fn(&ClickEvent, &mut Window, &mut gpui::App) + 'static,
) -> impl IntoElement {
    let mut col = v_flex().items_center().gap_0();
    for ch in title.chars() {
        col = col.child(
            div()
                .text_size(px(15.))
                .line_height(gpui::relative(1.0))
                .font_weight(FontWeight::SEMIBOLD)
                .child(ch.to_string()),
        );
    }
    let base = div()
        .id(SharedString::from(id))
        .px_3()
        .py_4()
        .rounded_md()
        .bg(if is_active { PRIMARY } else { CARD })
        .text_color(if is_active {
            hsl(0.0, 0.0, 100.0)
        } else {
            FOREGROUND
        });
    if disabled {
        // 執行中：頁籤鎖住，變灰且不可點
        base.opacity(0.4).child(col).into_any_element()
    } else {
        base.cursor_pointer().on_click(on_click).child(col).into_any_element()
    }
}

#[allow(dead_code)]
pub(super) fn nav_button(
    id: &'static str,
    title: &'static str,
    subtitle: &'static str,
    is_active: bool,
    on_click: impl Fn(&ClickEvent, &mut Window, &mut gpui::App) + 'static,
) -> impl IntoElement {
    div()
        .id(SharedString::from(id))
        .p_3()
        .rounded_md()
        .bg(if is_active { PRIMARY } else { CARD })
        .text_color(if is_active {
            hsl(0.0, 0.0, 100.0)
        } else {
            FOREGROUND
        })
        .cursor_pointer()
        .on_click(on_click)
        .child(
            v_flex()
                .gap_1()
                .child(div().font_weight(FontWeight::SEMIBOLD).text_sm().child(title))
                .child(
                    div()
                        .text_xs()
                        .text_color(if is_active {
                            hsl(0.0, 0.0, 90.0)
                        } else {
                            MUTED_FOREGROUND
                        })
                        .child(subtitle),
                ),
        )
}

pub(super) fn labeled_input(label: &'static str, state: &Entity<InputState>) -> impl IntoElement {
    v_flex()
        .w_full()
        .min_w_0()
        .gap_1()
        .child(
            div()
                .text_xs()
                .text_color(MUTED_FOREGROUND)
                .child(label),
        )
        .child(Input::new(state))
}

pub(super) fn flex_labeled_input(
    label: &'static str,
    state: &Entity<InputState>,
    min_w: gpui::Pixels,
) -> impl IntoElement {
    div()
        .flex_1()
        .min_w(min_w)
        .child(labeled_input(label, state))
}

pub(super) fn scope_pill(
    label: &'static str,
    current: &str,
    on_click: impl Fn(&ClickEvent, &mut Window, &mut gpui::App) + 'static,
) -> impl IntoElement {
    let active = current == label;
    div()
        .id(SharedString::from(format!("scope-{label}")))
        .px_2()
        .py_1()
        .rounded_md()
        .bg(if active { PRIMARY } else { SECONDARY })
        .border_1()
        .border_color(if active { PRIMARY } else { BORDER })
        .text_xs()
        .cursor_pointer()
        .on_click(on_click)
        .child(label)
}

pub(super) fn mode_pill(
    id: impl Into<SharedString>,
    label: &str,
    active: bool,
    on_click: impl Fn(&ClickEvent, &mut Window, &mut gpui::App) + 'static,
) -> impl IntoElement {
    div()
        .id(id.into())
        .px_2()
        .py_1()
        .rounded_md()
        .bg(if active { PRIMARY } else { SECONDARY })
        .border_1()
        .border_color(if active { PRIMARY } else { BORDER })
        .text_xs()
        .cursor_pointer()
        .on_click(on_click)
        .child(label.to_string())
}

pub(super) fn dropdown_item(
    id: impl Into<SharedString>,
    label: &str,
    active: bool,
    on_click: impl Fn(&ClickEvent, &mut Window, &mut gpui::App) + 'static,
) -> impl IntoElement {
    div()
        .id(id.into())
        .w_full()
        .px_2()
        .py_1()
        .rounded_md()
        .text_sm()
        .cursor_pointer()
        .bg(if active { PRIMARY } else { CARD })
        .text_color(if active { hsl(0.0, 0.0, 100.0) } else { FOREGROUND })
        .hover(|s| s.bg(SECONDARY))
        .on_click(on_click)
        .child(label.to_string())
}

pub(super) fn rule_badge(code: &'static str, desc: &'static str) -> impl IntoElement {
    h_flex()
        .p_2()
        .bg(BACKGROUND)
        .rounded_md()
        .items_center()
        .gap_2()
        .text_xs()
        .child(Tag::primary().child(code))
        .child(div().child(desc))
}

/// 單選圓點 + 文字（14px），點擊文字或圓點皆可選取
pub(super) fn radio_item(
    id: impl Into<SharedString>,
    label: impl Into<SharedString>,
    active: bool,
    on_click: impl Fn(&ClickEvent, &mut Window, &mut gpui::App) + 'static,
) -> impl IntoElement {
    h_flex()
        .id(id.into())
        .gap_1()
        .items_center()
        .text_size(px(14.))
        .cursor_pointer()
        .on_click(on_click)
        .child(
            div()
                .w(px(16.))
                .h(px(16.))
                .flex_shrink_0()
                .rounded_full()
                .border_1()
                .border_color(if active { PRIMARY } else { BORDER })
                .bg(CARD)
                .flex()
                .items_center()
                .justify_center()
                .child(div().w(px(8.)).h(px(8.)).rounded_full().bg(if active {
                    PRIMARY
                } else {
                    CARD
                })),
        )
        .child(label.into())
}
