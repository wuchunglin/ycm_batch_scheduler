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
                            this.request_nav(NavTab::ScheduleExec, cx);
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

        // ---- 新增批次程式到群組 ----
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
                    // 雙擊：直接加入群組
                    .on_mouse_down(
                        MouseButton::Left,
                        cx.listener(move |this, ev: &MouseDownEvent, window, cx| {
                            if ev.click_count == 2 {
                                this.gs_selected_program = Some(pid);
                                this.add_program_with_options(pid, window, cx);
                            }
                        }),
                    )
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

        root = root.child(
            card()
                .child(
                    h_flex()
                        .w_full()
                        .items_center()
                        .justify_between()
                        .child(
                            div()
                                .font_weight(FontWeight::SEMIBOLD)
                                .child("新增批次程式到群組"),
                        )
                        .child(
                            Button::new("gs-add-to-group")
                                .primary()
                                .label("加入群組")
                                .on_click(cx.listener(|this, _: &ClickEvent, window, cx| {
                                    match this.gs_selected_program {
                                        Some(pid) => this.add_program_with_options(pid, window, cx),
                                        None => window.push_notification(
                                            Notification::error("請先選擇批次程式"),
                                            cx,
                                        ),
                                    }
                                })),
                        ),
                )
                .child(picker),
        );

        // ---- 群組內的批次程式（可調整順序、修改時間 / 前置工作序號 / mail 通知） ----
        let dirty = self.gs_dirty(cx);
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
            let edit = self.gs_edits.iter().find(|e| e.item_id == item_id);
            let mut row = h_flex()
                .w_full()
                .px_3()
                .py_1()
                .gap_3()
                .text_size(px(14.))
                .flex_wrap()
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
                    div()
                        .flex_1()
                        .min_w(px(180.))
                        .font_weight(FontWeight::SEMIBOLD)
                        .child(format!("{}　{}", code, name)),
                );
            if let Some(e) = edit {
                let link_prev = e.link_prev;
                let req_email = e.req_email;
                row = row
                    .child(
                        h_flex()
                            .gap_1()
                            .items_center()
                            .child(
                                div()
                                    .flex_shrink_0()
                                    .text_color(MUTED_FOREGROUND)
                                    .child("預計執行時間"),
                            )
                            .child(
                                div()
                                    .w(px(80.))
                                    .text_size(px(14.))
                                    .child(Input::new(&e.time_input)),
                            ),
                    )
                    .child(
                        h_flex()
                            .gap_1()
                            .items_center()
                            .child(
                                Checkbox::new(SharedString::from(format!("gs-link-{item_id}")))
                                    .checked(link_prev)
                                    .on_click(cx.listener(move |this, checked, _, cx| {
                                        if let Some(en) =
                                            this.gs_edits.iter_mut().find(|x| x.item_id == item_id)
                                        {
                                            en.link_prev = *checked;
                                        }
                                        cx.notify();
                                    })),
                            )
                            .child(
                                div()
                                    .id(SharedString::from(format!("gs-link-lbl-{item_id}")))
                                    .text_size(px(14.))
                                    .cursor_pointer()
                                    .on_click(cx.listener(move |this, _: &ClickEvent, _, cx| {
                                        if let Some(en) =
                                            this.gs_edits.iter_mut().find(|x| x.item_id == item_id)
                                        {
                                            en.link_prev = !en.link_prev;
                                        }
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
                                Checkbox::new(SharedString::from(format!("gs-mail-{item_id}")))
                                    .checked(req_email)
                                    .on_click(cx.listener(move |this, checked, _, cx| {
                                        if let Some(en) =
                                            this.gs_edits.iter_mut().find(|x| x.item_id == item_id)
                                        {
                                            en.req_email = *checked;
                                        }
                                        cx.notify();
                                    })),
                            )
                            .child(
                                div()
                                    .id(SharedString::from(format!("gs-mail-lbl-{item_id}")))
                                    .text_size(px(14.))
                                    .cursor_pointer()
                                    .on_click(cx.listener(move |this, _: &ClickEvent, _, cx| {
                                        if let Some(en) =
                                            this.gs_edits.iter_mut().find(|x| x.item_id == item_id)
                                        {
                                            en.req_email = !en.req_email;
                                        }
                                        cx.notify();
                                    }))
                                    .child("發送mail通知"),
                            ),
                    );
            }
            row = row
                .child(
                    h_flex()
                        .flex_shrink_0()
                        .gap_0()
                        .child(
                            Button::new(SharedString::from(format!("gs-up-{item_id}")))
                                .small()
                                .ghost()
                                .w(px(30.))
                                .label("▲")
                                .on_click(cx.listener(move |this, _: &ClickEvent, window, cx| {
                                    this.move_group_item(item_id, true, window, cx);
                                })),
                        )
                        .child(
                            Button::new(SharedString::from(format!("gs-down-{item_id}")))
                                .small()
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
                        .small()
                        .danger()
                        .label("移除")
                        .on_click(cx.listener(move |this, _: &ClickEvent, window, cx| {
                            this.remove_group_item(item_id, window, cx);
                        })),
                );
            step_rows = step_rows.child(row);
        }
        root = root.child(
            card()
                .child(
                    h_flex()
                        .w_full()
                        .items_center()
                        .justify_between()
                        .child(
                            h_flex()
                                .gap_2()
                                .items_center()
                                .child(
                                    div()
                                        .font_weight(FontWeight::SEMIBOLD)
                                        .child(format!("群組內的批次程式（{}）", self.group_items.len())),
                                )
                                .child(if dirty {
                                    Tag::warning().child("有未儲存的變更").into_any_element()
                                } else {
                                    div().into_any_element()
                                }),
                        )
                        .child(
                            Button::new("gs-save-changes")
                                .primary()
                                .label("儲存變更")
                                .disabled(!dirty)
                                .on_click(cx.listener(|this, _: &ClickEvent, window, cx| {
                                    this.save_group_item_changes(window, cx);
                                })),
                        ),
                )
                .child(step_rows),
        );

        root.into_any_element()
    }
}
