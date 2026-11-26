//! 主視窗 Render：導覽列與分頁切換

use super::*;
use super::helpers::{nav_vertical};

impl Render for BatchSchedulerApp {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        if self.field_rows_dirty {
            self.rebuild_field_rows(window, cx);
            self.field_rows_dirty = false;
        }
        let active = self.active_tab;
        let viewport_w = window.viewport_size().width;
        let is_narrow = viewport_w < px(860.);

        v_flex()
            .size_full()
            .bg(BACKGROUND)
            .text_color(FOREGROUND)
            // Top Header Bar
            .child(
                h_flex()
                    .min_h(px(54.))
                    .px(if is_narrow { px(16.) } else { px(24.) })
                    .py_2()
                    .bg(CARD)
                    .border_b_1()
                    .border_color(BORDER)
                    .flex_wrap()
                    .gap_2()
                    .items_center()
                    .justify_between()
                    .child(
                        h_flex()
                            .gap_2()
                            .flex_wrap()
                            .items_center()
                            .child(
                                div()
                                    .text_lg()
                                    .font_weight(FontWeight::BOLD)
                                    .child("YCM 共用批次排程自動化執行系統"),
                            )
                            .child(Tag::primary().child("GPUI 通用版"))
                            .child(Tag::secondary().child("v1.0.0")),
                    )
                    .child(
                        h_flex()
                            .gap_3()
                            .flex_wrap()
                            .items_center()
                            .child(
                                h_flex()
                                    .gap_1()
                                    .items_center()
                                    .child(div().w(px(8.)).h(px(8.)).rounded_full().bg(SUCCESS))
                                    .child(
                                        div()
                                            .text_xs()
                                            .text_color(MUTED_FOREGROUND)
                                            .child("SQLite 本機資料庫"),
                                    ),
                            )
                            .child(
                                div()
                                    .text_xs()
                                    .text_color(MUTED_FOREGROUND)
                                    .child(format!(
                                        "程式: {} 支 | 預設值: {} 項",
                                        self.programs.len(),
                                        self.default_values.len()
                                    )),
                            ),
                    ),
            )
            // Main Body
            .child(
                h_flex()
                    .flex_1()
                    .min_h_0()
                    .w_full()
                    .items_start()
                    .child(
                        v_flex()
                            .w(if is_narrow { px(60.) } else { px(72.) })
                            .flex_shrink_0()
                            .h_full()
                            .bg(CARD)
                            .border_r_1()
                            .border_color(BORDER)
                            .p_2()
                            .gap_2()
                            .justify_start()
                            .items_center()
                            .child(nav_vertical(
                                "nav-prog",
                                "批次程式新增",
                                active == NavTab::ProgramManager,
                                cx.listener(|this, _: &ClickEvent, _, cx| {
                                    this.active_tab = NavTab::ProgramManager;
                                    cx.notify();
                                }),
                            ))
                            .child(nav_vertical(
                                "nav-exec",
                                "排程設定及執行",
                                matches!(active, NavTab::ScheduleExec | NavTab::ExecPreview | NavTab::GroupSetup),
                                cx.listener(|this, _: &ClickEvent, _, cx| {
                                    this.active_tab = NavTab::ScheduleExec;
                                    cx.notify();
                                }),
                            ))
                            .child(nav_vertical(
                                "nav-settings",
                                "設定",
                                active == NavTab::Settings,
                                cx.listener(|this, _: &ClickEvent, _, cx| {
                                    this.active_tab = NavTab::Settings;
                                    cx.notify();
                                }),
                            )),
                    )
                    .child(
                        v_flex()
                            .id("workspace-scroll-area")
                            .flex_1()
                            .h_full()
                            .min_w_0()
                            .min_h_0()
                            .justify_start()
                            .items_start()
                            .overflow_y_scroll()
                            .p(if is_narrow { px(16.) } else { px(24.) })
                            .child(div().w_full().flex_none().child(match self.active_tab {
                                NavTab::ProgramManager => {
                                    self.render_program_view(viewport_w, cx).into_any_element()
                                }
                                NavTab::GroupEditor => {
                                    self.render_group_view(viewport_w, cx).into_any_element()
                                }
                                NavTab::ScheduleRule => {
                                    self.render_schedule_view(viewport_w, cx).into_any_element()
                                }
                                NavTab::ExecutionRunner => {
                                    self.render_runner_view(cx).into_any_element()
                                }
                                NavTab::ScheduleExec => {
                                    self.render_schedule_exec_view(cx).into_any_element()
                                }
                                NavTab::ExecPreview => {
                                    self.render_exec_preview_view(cx).into_any_element()
                                }
                                NavTab::GroupSetup => {
                                    self.render_group_setup_view(cx).into_any_element()
                                }
                                NavTab::Settings => {
                                    self.render_settings_view(cx).into_any_element()
                                }
                            })),
                    ),
            )
            .children(Root::render_notification_layer(window, cx))
    }
}
