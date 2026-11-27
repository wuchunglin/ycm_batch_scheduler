//! 主視窗 Render：導覽列與分頁切換

use super::*;
use super::helpers::{nav_vertical};

impl Render for BatchSchedulerApp {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        if self.field_rows_dirty {
            self.rebuild_field_rows(window, cx);
            self.field_rows_dirty = false;
        }
        if self.active_tab == NavTab::ExecPreview && !self.exec_console_mode && self.exec_param_dirty {
            self.rebuild_exec_param_rows(window, cx);
            self.exec_param_dirty = false;
        }
        let active = self.active_tab;
        let viewport_w = window.viewport_size().width;
        let is_narrow = viewport_w < px(860.);
        // 執行排程期間鎖住所有頁籤，必須執行完成才能切換
        let locked = self.runner_state.is_running;
        let pad = if is_narrow { px(16.) } else { px(24.) };

        v_flex()
            .size_full()
            .relative()
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
                                        self.default_values.iter().filter(|d| !is_custom_default(d)).count()
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
                                "nav-exec",
                                "排程設定及執行",
                                matches!(active, NavTab::ScheduleExec | NavTab::ExecPreview | NavTab::GroupSetup),
                                locked,
                                cx.listener(|this, _: &ClickEvent, _, cx| {
                                    this.request_nav(NavTab::ScheduleExec, cx);
                                }),
                            ))
                            .child(nav_vertical(
                                "nav-prog",
                                "批次程式新增",
                                active == NavTab::ProgramManager,
                                locked,
                                cx.listener(|this, _: &ClickEvent, _, cx| {
                                    this.request_nav(NavTab::ProgramManager, cx);
                                }),
                            ))
                            .child(nav_vertical(
                                "nav-settings",
                                "參數設定",
                                active == NavTab::Settings,
                                locked,
                                cx.listener(|this, _: &ClickEvent, _, cx| {
                                    this.request_nav(NavTab::Settings, cx);
                                }),
                            )),
                    )
                    .child(if self.active_tab == NavTab::ExecPreview {
                        // 預覽頁：標題列固定在上方（含「開始執行」鍵），只有內容區捲動
                        v_flex()
                            .flex_1()
                            .h_full()
                            .min_w_0()
                            .min_h_0()
                            .p(pad)
                            .child(self.render_exec_preview_view(cx))
                            .into_any_element()
                    } else {
                        v_flex()
                            .id("workspace-scroll-area")
                            .flex_1()
                            .h_full()
                            .min_w_0()
                            .min_h_0()
                            .justify_start()
                            .items_start()
                            .overflow_y_scroll()
                            .p(pad)
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
                                NavTab::ExecPreview => div().into_any_element(),
                                NavTab::GroupSetup => {
                                    self.render_group_setup_view(cx).into_any_element()
                                }
                                NavTab::Settings => {
                                    self.render_settings_view(cx).into_any_element()
                                }
                            }))
                            .into_any_element()
                    }),
            )
            .children(self.render_leave_prompt(cx))
            .children(self.render_interrupt_prompt(cx))
            .children(Root::render_notification_layer(window, cx))
    }
}

impl BatchSchedulerApp {
    /// 執行中按「中斷」：詢問「確定中斷 / 取消」
    fn render_interrupt_prompt(&self, cx: &mut Context<Self>) -> Option<gpui::AnyElement> {
        if !(self.interrupt_prompt && self.runner_state.is_running) {
            return None;
        }
        Some(
            div()
                .id("interrupt-prompt-overlay")
                .absolute()
                .top_0()
                .left_0()
                .size_full()
                .flex()
                .items_center()
                .justify_center()
                .bg(hsl(0.0, 0.0, 0.0).opacity(0.45))
                .occlude()
                .child(
                    v_flex()
                        .w(px(420.))
                        .p_5()
                        .gap_4()
                        .bg(CARD)
                        .rounded_lg()
                        .border_1()
                        .border_color(BORDER)
                        .child(
                            div()
                                .text_lg()
                                .font_weight(FontWeight::BOLD)
                                .child("確定要中斷排程執行嗎？"),
                        )
                        .child(
                            div()
                                .text_size(px(14.))
                                .text_color(MUTED_FOREGROUND)
                                .child("中斷後，尚未執行的排程將不再執行；已送出（F8）的作業無法復原。"),
                        )
                        .child(
                            h_flex()
                                .w_full()
                                .justify_end()
                                .gap_2()
                                .child(
                                    Button::new("interrupt-cancel")
                                        .ghost()
                                        .label("取消")
                                        .on_click(cx.listener(|this, _: &ClickEvent, _, cx| {
                                            this.interrupt_prompt = false;
                                            cx.notify();
                                        })),
                                )
                                .child(
                                    Button::new("interrupt-confirm")
                                        .danger()
                                        .label("確定中斷")
                                        .on_click(cx.listener(|this, _: &ClickEvent, _, cx| {
                                            if this.runner_state.is_running {
                                                request_cancel();
                                                this.interrupt_requested = true;
                                                this.runner_state.current_step = "中斷中...".to_string();
                                            }
                                            this.interrupt_prompt = false;
                                            cx.notify();
                                        })),
                                ),
                        ),
                )
                .into_any_element(),
        )
    }

    /// 離開群組設定頁時，若有未儲存的變更：顯示「儲存變更並離開 / 不儲存離開 / 取消」確認視窗
    fn render_leave_prompt(&self, cx: &mut Context<Self>) -> Option<gpui::AnyElement> {
        let target = self.leave_prompt?;
        Some(
            div()
                .id("leave-prompt-overlay")
                .absolute()
                .top_0()
                .left_0()
                .size_full()
                .flex()
                .items_center()
                .justify_center()
                .bg(hsl(0.0, 0.0, 0.0).opacity(0.45))
                .occlude()
                .child(
                    v_flex()
                        .w(px(420.))
                        .p_5()
                        .gap_4()
                        .bg(CARD)
                        .rounded_lg()
                        .border_1()
                        .border_color(BORDER)
                        .child(
                            div()
                                .text_lg()
                                .font_weight(FontWeight::BOLD)
                                .child("群組內的批次程式有未儲存的變更"),
                        )
                        .child(
                            div()
                                .text_size(px(14.))
                                .text_color(MUTED_FOREGROUND)
                                .child("要先儲存變更再離開，還是直接離開（變更將不會被儲存）？"),
                        )
                        .child(
                            h_flex()
                                .w_full()
                                .justify_end()
                                .gap_2()
                                .child(
                                    Button::new("leave-cancel")
                                        .ghost()
                                        .label("取消")
                                        .on_click(cx.listener(|this, _: &ClickEvent, _, cx| {
                                            this.leave_prompt = None;
                                            cx.notify();
                                        })),
                                )
                                .child(
                                    Button::new("leave-discard")
                                        .danger()
                                        .label("離開")
                                        .on_click(cx.listener(move |this, _: &ClickEvent, _, cx| {
                                            this.finish_nav(target, cx);
                                        })),
                                )
                                .child(
                                    Button::new("leave-save")
                                        .primary()
                                        .label("儲存變更並離開")
                                        .on_click(cx.listener(move |this, _: &ClickEvent, window, cx| {
                                            if this.save_group_item_changes(window, cx) {
                                                this.finish_nav(target, cx);
                                            }
                                        })),
                                ),
                        ),
                )
                .into_any_element(),
        )
    }
}
