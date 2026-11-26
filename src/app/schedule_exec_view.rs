//! 排程設定及執行畫面

use super::helpers::radio_item;
use super::*;

impl BatchSchedulerApp {
    /// 排程設定及執行：先顯示群組清單，再顯示批次程式清單與「新增群組」鍵
    pub(super) fn render_schedule_exec_view(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let prog_map: HashMap<i64, &Program> = self
            .programs
            .iter()
            .filter_map(|p| p.id.map(|id| (id, p)))
            .collect();

        // 每個群組內的步驟（顯示摘要用）
        let group_items_map: HashMap<i64, Vec<GroupItem>> = {
            let guard = self.conn.lock().unwrap();
            let repo = Repository::new(&guard);
            self.groups
                .iter()
                .filter_map(|g| g.id)
                .map(|gid| (gid, repo.list_group_items(gid).unwrap_or_default()))
                .collect()
        };

        // ---- 群組清單 ----
        let mut group_rows = v_flex().w_full().gap_2();
        if self.groups.is_empty() {
            group_rows = group_rows.child(
                div()
                    .text_size(px(14.))
                    .text_color(MUTED_FOREGROUND)
                    .child("尚無群組，請按下方「新增群組」建立"),
            );
        }
        for g in &self.groups {
            let gid = g.id.unwrap_or(0);
            let items = group_items_map.get(&gid).cloned().unwrap_or_default();
            let summary = if items.is_empty() {
                "（尚未加入批次程式）".to_string()
            } else {
                items
                    .iter()
                    .map(|it| {
                        let code = prog_map
                            .get(&it.program_id)
                            .map(|p| p.job_code.clone())
                            .unwrap_or_else(|| "?".to_string());
                        format!("{}. {} {}", it.step_order, code, it.expect_exe_time)
                    })
                    .collect::<Vec<_>>()
                    .join("  →  ")
            };
            let mut info = v_flex()
                .flex_1()
                .min_w_0()
                .gap_0p5()
                .child(
                    div()
                        .text_size(px(14.))
                        .font_weight(FontWeight::BOLD)
                        .child(if g.description.is_empty() {
                            g.group_name.clone()
                        } else {
                            g.description.clone()
                        }),
                );
            info = info.child(div().text_size(px(14.)).text_color(MUTED_FOREGROUND).child(summary));

            let g_edit = g.clone();
            let g_del = g.clone();
            group_rows = group_rows.child(
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
                        Checkbox::new(SharedString::from(format!("sx-gchk-{gid}")))
                            .checked(self.exec_sel_groups.contains(&gid))
                            .on_click(cx.listener(move |this, checked, window, cx| {
                                this.toggle_exec_group(gid, *checked, window, cx);
                            })),
                    )
                    .child(info)
                    .child(
                        div()
                            .flex_shrink_0()
                            .text_size(px(14.))
                            .text_color(MUTED_FOREGROUND)
                            .child(format!("{} 支程式", items.len())),
                    )
                    .child(
                        Button::new(SharedString::from(format!("sx-edit-{gid}")))
                            .label("設定群組")
                            .on_click(cx.listener(move |this, _: &ClickEvent, window, cx| {
                                this.selected_group = Some(g_edit.clone());
                                this.reload_groups(window, cx);
                                this.active_tab = NavTab::GroupSetup;
                                cx.notify();
                            })),
                    )
                    .child(
                        Button::new(SharedString::from(format!("sx-del-{gid}")))
                            .danger()
                            .label("刪除")
                            .on_click(cx.listener(move |this, _: &ClickEvent, window, cx| {
                                this.selected_group = Some(g_del.clone());
                                this.delete_selected_group(window, cx);
                                if let Some(did) = g_del.id {
                                    this.exec_sel_groups.remove(&did);
                                    this.exec_entries.retain(|e| e.source_group != Some(did));
                                }
                            })),
                    ),
            );
        }

        // ---- 批次程式清單 ----
        let mut prog_rows = v_flex().w_full().gap_1();
        if self.programs.is_empty() {
            prog_rows = prog_rows.child(
                div()
                    .text_size(px(14.))
                    .text_color(MUTED_FOREGROUND)
                    .child("尚無批次程式，請先到「批次程式新增」建立"),
            );
        }
        for p in &self.programs {
            let pid = p.id.unwrap_or(0);
            prog_rows = prog_rows.child(
                h_flex()
                    .w_full()
                    .px_3()
                    .py_1()
                    .gap_2()
                    .text_size(px(14.))
                    .items_center()
                    .bg(BACKGROUND)
                    .rounded_md()
                    .border_1()
                    .border_color(BORDER)
                    .child(
                        Checkbox::new(SharedString::from(format!("sx-pchk-{pid}")))
                            .checked(self.exec_sel_programs.contains(&pid))
                            .on_click(cx.listener(move |this, checked, window, cx| {
                                this.toggle_exec_program(pid, *checked, window, cx);
                            })),
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

        // ---- 執行清單（由上方勾選的群組 / 批次程式帶入，可調整順序） ----
        let mut exec_rows = v_flex().w_full().gap_1();
        if self.exec_entries.is_empty() {
            exec_rows = exec_rows.child(
                div()
                    .text_size(px(14.))
                    .text_color(MUTED_FOREGROUND)
                    .child("請勾選上方的群組或批次程式，會列示在此"),
            );
        }
        for (idx, e) in self.exec_entries.iter().enumerate() {
            let (code, name) = prog_map
                .get(&e.program_id)
                .map(|p| (p.job_code.clone(), p.job_name.clone()))
                .unwrap_or_else(|| ("?".to_string(), "（程式已刪除）".to_string()));
            let source = match e.source_group {
                Some(gid) => {
                    let gname = self
                        .groups
                        .iter()
                        .find(|g| g.id == Some(gid))
                        .map(|g| g.group_name.clone())
                        .unwrap_or_default();
                    format!("來自群組：{gname}")
                }
                None => "單獨選取".to_string(),
            };
            exec_rows = exec_rows.child(
                h_flex()
                    .w_full()
                    .px_3()
                    .py_2()
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
                            .child(format!("{}", idx + 1)),
                    )
                    .child(
                        v_flex()
                            .flex_1()
                            .min_w(px(180.))
                            .gap_0p5()
                            .child(
                                div()
                                    .font_weight(FontWeight::SEMIBOLD)
                                    .child(format!("{}　{}", code, name)),
                            )
                            .child(div().text_size(px(14.)).text_color(MUTED_FOREGROUND).child(source)),
                    )
                    .child(
                        h_flex()
                            .gap_1()
                            .items_center()
                            .child(
                                div()
                                    .text_size(px(14.))
                                    .text_color(MUTED_FOREGROUND)
                                    .child("預計執行時間"),
                            )
                            .child(div().w(px(80.)).text_size(px(14.)).child(Input::new(&e.time_input))),
                    )
                    .child(
                        h_flex()
                            .gap_1()
                            .items_center()
                            .child(
                                Checkbox::new(SharedString::from(format!("ex-link-{idx}")))
                                    .checked(e.link_prev)
                                    .on_click(cx.listener(move |this, checked, _, cx| {
                                        if let Some(en) = this.exec_entries.get_mut(idx) {
                                            en.link_prev = *checked;
                                        }
                                        cx.notify();
                                    })),
                            )
                            .child(
                                div()
                                    .id(SharedString::from(format!("ex-link-lbl-{idx}")))
                                    .text_size(px(14.))
                                    .cursor_pointer()
                                    .on_click(cx.listener(move |this, _: &ClickEvent, _, cx| {
                                        if let Some(en) = this.exec_entries.get_mut(idx) {
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
                                Checkbox::new(SharedString::from(format!("ex-mail-{idx}")))
                                    .checked(e.req_email)
                                    .on_click(cx.listener(move |this, checked, _, cx| {
                                        if let Some(en) = this.exec_entries.get_mut(idx) {
                                            en.req_email = *checked;
                                        }
                                        cx.notify();
                                    })),
                            )
                            .child(
                                div()
                                    .id(SharedString::from(format!("ex-mail-lbl-{idx}")))
                                    .text_size(px(14.))
                                    .cursor_pointer()
                                    .on_click(cx.listener(move |this, _: &ClickEvent, _, cx| {
                                        if let Some(en) = this.exec_entries.get_mut(idx) {
                                            en.req_email = !en.req_email;
                                        }
                                        cx.notify();
                                    }))
                                    .child("發送mail通知"),
                            ),
                    )
                    .child(
                        h_flex()
                            .flex_shrink_0()
                            .gap_0()
                            .child(
                                Button::new(SharedString::from(format!("ex-up-{idx}")))
                                    .ghost()
                                    .w(px(30.))
                                    .label("▲")
                                    .on_click(cx.listener(move |this, _: &ClickEvent, _, cx| {
                                        this.move_exec_entry(idx, true, cx);
                                    })),
                            )
                            .child(
                                Button::new(SharedString::from(format!("ex-down-{idx}")))
                                    .ghost()
                                    .w(px(30.))
                                    .label("▼")
                                    .on_click(cx.listener(move |this, _: &ClickEvent, _, cx| {
                                        this.move_exec_entry(idx, false, cx);
                                    })),
                            ),
                    )
                    .child(
                        Button::new(SharedString::from(format!("ex-rm-{idx}")))
                            .danger()
                            .label("移除")
                            .on_click(cx.listener(move |this, _: &ClickEvent, _, cx| {
                                this.remove_exec_entry(idx, cx);
                            })),
                    ),
            );
        }

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

        v_flex()
            .w_full()
            .items_start()
            .gap_4()
            .child(
                v_flex()
                    .w_full()
                    .items_start()
                    .gap_1()
                    .child(
                        div()
                            .text_xl()
                            .font_weight(FontWeight::BOLD)
                            .child("排程設定及執行"),
                    )
                    .child(
                        div()
                            .text_sm()
                            .text_color(MUTED_FOREGROUND)
                            .child("群組清單與批次程式清單；按「新增群組」或群組的「設定群組」進入群組設定畫面。"),
                    ),
            )
            .child(
                card()
                    .child(
                        h_flex()
                            .w_full()
                            .items_center()
                            .justify_between()
                            .child(
                                div()
                                    .font_weight(FontWeight::SEMIBOLD)
                                    .child(format!("群組清單（{}）", self.groups.len())),
                            )
                            .child(
                                Button::new("sx-new-group")
                                    .primary()
                                    .label("新增群組")
                                    .on_click(cx.listener(|this, _: &ClickEvent, _, cx| {
                                        this.selected_group = None;
                                        this.group_items.clear();
                                        this.gs_selected_program = None;
                                        this.active_tab = NavTab::GroupSetup;
                                        cx.notify();
                                    })),
                            ),
                    )
                    .child(group_rows),
            )
            .child(
                card()
                    .child(
                        div()
                            .font_weight(FontWeight::SEMIBOLD)
                            .child(format!("批次程式清單（{}）", self.programs.len())),
                    )
                    .child(prog_rows),
            )
            .child(
                card()
                    .child(
                        div()
                            .font_weight(FontWeight::SEMIBOLD)
                            .child(format!("執行清單（{}）", self.exec_entries.len())),
                    )
                    .child(exec_rows)
                    .child(self.render_exec_schedule_form(cx))
                    .child(
                        h_flex().w_full().justify_end().child(
                            Button::new("sx-next")
                                .primary()
                                .label("下一步")
                                .on_click(cx.listener(|this, _: &ClickEvent, window, cx| {
                                    if this.exec_entries.is_empty() {
                                        window.push_notification(
                                            Notification::warning("執行清單是空的，請先勾選群組或批次程式"),
                                            cx,
                                        );
                                        return;
                                    }
                                    this.exec_preview_date_idx = 0;
                                    this.active_tab = NavTab::ExecPreview;
                                    cx.notify();
                                })),
                        ),
                    ),
            )
    }

    /// 執行清單下方的排程條件：年度別 + 排程日期
    fn render_exec_schedule_form(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let row_label = |t: &'static str| {
            div()
                .w(px(80.))
                .flex_shrink_0()
                .text_size(px(14.))
                .font_weight(FontWeight::SEMIBOLD)
                .child(t)
        };
        let date_input = |st: &Entity<InputState>| {
            div().w(px(150.)).text_size(px(14.)).child(Input::new(st))
        };
        let mode = self.exec_date_mode;

        // 年度別
        let year_row = h_flex()
            .w_full()
            .flex_wrap()
            .gap_4()
            .items_center()
            .child(row_label("年度"))
            .child(radio_item(
                "sx-y115",
                "115",
                self.exec_year == Some(115),
                cx.listener(|this, _: &ClickEvent, _, cx| {
                    this.exec_year = Some(115);
                    cx.notify();
                }),
            ))
            .child(radio_item(
                "sx-y116",
                "116",
                self.exec_year == Some(116),
                cx.listener(|this, _: &ClickEvent, _, cx| {
                    this.exec_year = Some(116);
                    cx.notify();
                }),
            ))
            .child(radio_item(
                "sx-yrange",
                "時間區間",
                self.exec_year.is_none(),
                cx.listener(|this, _: &ClickEvent, _, cx| {
                    this.exec_year = None;
                    cx.notify();
                }),
            ))
            .child(date_input(&self.exec_range_start_input))
            .child(date_input(&self.exec_range_end_input));

        // 每周
        let weekday_names = ["一", "二", "三", "四", "五", "六", "日"];
        let mut week = h_flex().gap_3().items_center().flex_wrap().child(radio_item(
            "sx-dm-week",
            "每周",
            mode == 2,
            cx.listener(|this, _: &ClickEvent, _, cx| {
                this.exec_date_mode = 2;
                cx.notify();
            }),
        ));
        for (i, n) in weekday_names.iter().enumerate() {
            week = week.child(
                h_flex()
                    .gap_1()
                    .items_center()
                    .child(
                        Checkbox::new(SharedString::from(format!("sx-wd-{i}")))
                            .checked(self.exec_weekdays[i])
                            .on_click(cx.listener(move |this, checked, _, cx| {
                                this.exec_weekdays[i] = *checked;
                                this.exec_date_mode = 2;
                                cx.notify();
                            })),
                    )
                    .child(div().text_size(px(14.)).child(*n)),
            );
        }

        // 指定日期
        let mut specific = h_flex().gap_2().items_center().flex_wrap().child(radio_item(
            "sx-dm-spec",
            "指定日期",
            mode == 3,
            cx.listener(|this, _: &ClickEvent, _, cx| {
                this.exec_date_mode = 3;
                cx.notify();
            }),
        ));
        for st in &self.exec_specific_inputs {
            specific = specific.child(date_input(st));
        }

        let date_row = h_flex()
            .w_full()
            .items_start()
            .gap_4()
            .child(row_label("排程日期"))
            .child(
                v_flex()
                    .flex_1()
                    .min_w_0()
                    .gap_2()
                    .child(
                        h_flex()
                            .gap_4()
                            .items_center()
                            .flex_wrap()
                            .child(
                                h_flex()
                                    .gap_2()
                                    .items_center()
                                    .child(radio_item(
                                        "sx-dm-month",
                                        "每月",
                                        mode == 0,
                                        cx.listener(|this, _: &ClickEvent, _, cx| {
                                            this.exec_date_mode = 0;
                                            cx.notify();
                                        }),
                                    ))
                                    .child(date_input(&self.exec_monthly_input)),
                            )
                            .child(radio_item(
                                "sx-dm-last",
                                "最後一天",
                                mode == 1,
                                cx.listener(|this, _: &ClickEvent, _, cx| {
                                    this.exec_date_mode = 1;
                                    cx.notify();
                                }),
                            )),
                    )
                    .child(week)
                    .child(specific),
            );

        v_flex()
            .w_full()
            .gap_3()
            .p_3()
            .rounded_md()
            .border_1()
            .border_color(BORDER)
            .bg(BACKGROUND)
            .child(year_row)
            .child(date_row)
    }
}
