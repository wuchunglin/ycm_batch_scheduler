//! 群組設定畫面

use super::*;

impl BatchSchedulerApp {
    /// 群組設定畫面：建立群組、選擇批次程式加入群組、調整群組內順序
    pub(super) fn render_group_setup_view(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let prog_map: HashMap<i64, &Program> = self
            .programs
            .iter()
            .filter_map(|p| p.id.map(|id| (id, p)))
            .collect();

        let card = || {
            v_flex()
                .w_full()
                .bg(CARD)
                .p_4()
                .rounded_lg()
                .border_1()
                .border_color(BORDER)
                .gap_3()
        };

        let mut root = v_flex()
            .w_full()
            .items_start()
            .gap_4()
            .child(
                div().child(
                    Button::new("gs-back")
                        .ghost()
                        .label("← 返回排程設定及執行")
                        .on_click(cx.listener(|this, _: &ClickEvent, _, cx| {
                            this.active_tab = NavTab::ScheduleExec;
                            cx.notify();
                        })),
                ),
            )
            .child(
                div()
                    .text_xl()
                    .font_weight(FontWeight::BOLD)
                    .child("群組設定"),
            );

        let Some(group) = self.selected_group.as_ref() else {
            // 尚未建立群組：先輸入群組名稱
            return root
                .child(
                    card()
                        .child(div().font_weight(FontWeight::SEMIBOLD).child("新增群組"))
                        .child(
                            h_flex()
                                .w_full()
                                .flex_wrap()
                                .gap_2()
                                .items_center()
                                .child(
                                    div()
                                        .w(px(240.))
                                        .min_w(px(160.))
                                        .child(Input::new(&self.new_group_name_input)),
                                )
                                .child(
                                    div()
                                        .flex_1()
                                        .min_w(px(200.))
                                        .child(Input::new(&self.new_group_desc_input)),
                                )
                                .child(
                                    Button::new("gs-create-group")
                                        .primary()
                                        .label("建立群組")
                                        .on_click(cx.listener(|this, _: &ClickEvent, window, cx| {
                                            this.create_group(window, cx);
                                        })),
                                ),
                        )
                        .child(
                            div()
                                .text_xs()
                                .text_color(MUTED_FOREGROUND)
                                .child("建立後即可在此選擇批次程式加入群組。"),
                        ),
                )
                .into_any_element();
        };

        // ---- 目前群組 ----
        let mut head = card().child(
            div()
                .font_weight(FontWeight::SEMIBOLD)
                .child(format!("目前群組：{}", group.group_name)),
        );
        if !group.description.is_empty() {
            head = head.child(
                div()
                    .text_xs()
                    .text_color(MUTED_FOREGROUND)
                    .child(group.description.clone()),
            );
        }
        root = root.child(head);

        // ---- 加入批次程式 ----
        let mut picker = v_flex()
            .id("gs-prog-list")
            .w_full()
            .max_h(px(240.))
            .overflow_y_scroll()
            .gap_1();
        if self.programs.is_empty() {
            picker = picker.child(
                div()
                    .text_xs()
                    .text_color(MUTED_FOREGROUND)
                    .child("尚無批次程式，請先到「批次程式新增」建立"),
            );
        }
        for p in &self.programs {
            let pid = p.id.unwrap_or(0);
            let is_sel = self.gs_selected_program == Some(pid);
            picker = picker.child(
                h_flex()
                    .id(SharedString::from(format!("gs-pick-{pid}")))
                    .w_full()
                    .px_3()
                    .py_1()
                    .gap_2()
                    .text_size(px(14.))
                    .items_center()
                    .rounded_md()
                    .border_1()
                    .border_color(if is_sel { PRIMARY } else { BORDER })
                    .bg(if is_sel { SECONDARY } else { BACKGROUND })
                    .cursor_pointer()
                    .on_click(cx.listener(move |this, _: &ClickEvent, _, cx| {
                        this.gs_selected_program = Some(pid);
                        cx.notify();
                    }))
                    .child(
                        div()
                            .w(px(120.))
                            .flex_shrink_0()
                            .font_weight(FontWeight::SEMIBOLD)
                            .child(p.job_code.clone()),
                    )
                    .child(div().flex_1().min_w_0().child(p.job_name.clone())),
            );
        }
        let picked_text = self
            .gs_selected_program
            .and_then(|id| prog_map.get(&id))
            .map(|p| format!("已選擇：{}　{}", p.job_code, p.job_name))
            .unwrap_or_else(|| "請從上方清單點選要加入的批次程式".to_string());

        root = root.child(
            card()
                .child(
                    div()
                        .font_weight(FontWeight::SEMIBOLD)
                        .child("新增批次程式到群組"),
                )
                .child(picker)
                .child(div().text_xs().text_color(MUTED_FOREGROUND).child(picked_text))
                .child(
                    h_flex()
                        .w_full()
                        .flex_wrap()
                        .gap_4()
                        .items_center()
                        .child(
                            h_flex()
                                .gap_2()
                                .items_center()
                                .text_size(px(14.))
                                .child(div().flex_shrink_0().child("預計執行時間 HH:MM"))
                                .child(
                                    div()
                                        .w(px(80.))
                                        .text_size(px(14.))
                                        .child(Input::new(&self.gs_time_input)),
                                ),
                        )
                        .child(
                            h_flex()
                                .gap_1()
                                .items_center()
                                .child(
                                    Checkbox::new("gs-chk-prev")
                                        .checked(self.gs_link_prev)
                                        .on_click(cx.listener(|this, checked, _, cx| {
                                            this.gs_link_prev = *checked;
                                            cx.notify();
                                        })),
                                )
                                .child(
                                    div()
                                        .id("gs-chk-prev-lbl")
                                        .text_size(px(14.))
                                        .cursor_pointer()
                                        .on_click(cx.listener(|this, _: &ClickEvent, _, cx| {
                                            this.gs_link_prev = !this.gs_link_prev;
                                            cx.notify();
                                        }))
                                        .child("前置工作序號"),
                                ),
                        )
                        .child(
                            h_flex()
                                .gap_1()
                                .items_center()
                                .child(
                                    Checkbox::new("gs-chk-mail")
                                        .checked(self.gs_req_email)
                                        .on_click(cx.listener(|this, checked, _, cx| {
                                            this.gs_req_email = *checked;
                                            cx.notify();
                                        })),
                                )
                                .child(
                                    div()
                                        .id("gs-chk-mail-lbl")
                                        .text_size(px(14.))
                                        .cursor_pointer()
                                        .on_click(cx.listener(|this, _: &ClickEvent, _, cx| {
                                            this.gs_req_email = !this.gs_req_email;
                                            cx.notify();
                                        }))
                                        .child("發送mail通知"),
                                ),
                        )
                        .child(
                            Button::new("gs-add-to-group")
                                .primary()
                                .label("加入群組")
                                .on_click(cx.listener(|this, _: &ClickEvent, window, cx| {
                                    this.add_program_with_options(window, cx);
                                })),
                        ),
                ),
        );

        // ---- 群組內的批次程式（可調整順序） ----
        let mut step_rows = v_flex().w_full().gap_1();
        if self.group_items.is_empty() {
            step_rows = step_rows.child(
                div()
                    .text_xs()
                    .text_color(MUTED_FOREGROUND)
                    .child("此群組尚未加入批次程式"),
            );
        }
        for it in &self.group_items {
            let item_id = it.id.unwrap_or(0);
            let (code, name) = prog_map
                .get(&it.program_id)
                .map(|p| (p.job_code.clone(), p.job_name.clone()))
                .unwrap_or_else(|| ("?".to_string(), "（程式已刪除）".to_string()));
            let time = if it.expect_exe_time.is_empty() {
                "當下時間".to_string()
            } else {
                it.expect_exe_time.clone()
            };
            let detail = format!(
                "預計執行 {}　｜　前置工作序號：{}　｜　發送mail通知：{}",
                time,
                if it.link_prev_sn { "是" } else { "否" },
                if it.req_email { "是" } else { "否" },
            );
            step_rows = step_rows.child(
                h_flex()
                    .w_full()
                    .px_3()
                    .py_2()
                    .gap_2()
                    .text_size(px(14.))
                    .items_center()
                    .bg(BACKGROUND)
                    .rounded_md()
                    .border_1()
                    .border_color(BORDER)
                    .child(
                        div()
                            .w(px(28.))
                            .flex_shrink_0()
                            .text_color(MUTED_FOREGROUND)
                            .child(format!("{}", it.step_order)),
                    )
                    .child(
                        v_flex()
                            .flex_1()
                            .min_w_0()
                            .gap_0p5()
                            .child(
                                div()
                                    .font_weight(FontWeight::SEMIBOLD)
                                    .child(format!("{}　{}", code, name)),
                            )
                            .child(div().text_size(px(14.)).text_color(MUTED_FOREGROUND).child(detail)),
                    )
                    .child(
                        h_flex()
                            .flex_shrink_0()
                            .gap_0()
                            .child(
                                Button::new(SharedString::from(format!("gs-up-{item_id}")))
                                    .ghost()
                                    .w(px(30.))
                                    .label("▲")
                                    .on_click(cx.listener(move |this, _: &ClickEvent, window, cx| {
                                        this.move_group_item(item_id, true, window, cx);
                                    })),
                            )
                            .child(
                                Button::new(SharedString::from(format!("gs-down-{item_id}")))
                                    .ghost()
                                    .w(px(30.))
                                    .label("▼")
                                    .on_click(cx.listener(move |this, _: &ClickEvent, window, cx| {
                                        this.move_group_item(item_id, false, window, cx);
                                    })),
                            ),
                    )
                    .child(
                        Button::new(SharedString::from(format!("gs-rm-{item_id}")))
                            .danger()
                            .label("移除")
                            .on_click(cx.listener(move |this, _: &ClickEvent, window, cx| {
                                this.remove_group_item(item_id, window, cx);
                            })),
                    ),
            );
        }
        root = root.child(
            card()
                .child(
                    div()
                        .font_weight(FontWeight::SEMIBOLD)
                        .child(format!("群組內的批次程式（{}）", self.group_items.len())),
                )
                .child(step_rows),
        );

        root.into_any_element()
    }
}
