//! 群組編輯畫面與步驟詳細編輯器

use super::*;
use super::helpers::{flex_labeled_input, scope_pill, mode_pill, rule_badge};

impl BatchSchedulerApp {
    pub(super) fn render_group_view(&self, viewport_w: gpui::Pixels, cx: &mut Context<Self>) -> impl IntoElement {
        let is_stacked = viewport_w < px(1160.);
        let resolver = self.build_resolver(cx);
        let test_expr = self.tester_expr_input.read(cx).value().to_string();
        let test_result = resolver.resolve_template(&test_expr);

        let prog_map: HashMap<i64, &Program> = self
            .programs
            .iter()
            .filter_map(|p| p.id.map(|id| (id, p)))
            .collect();

        v_flex()
            .w_full()
            .items_start()
            .gap_5()
            .child(
                v_flex()
                    .w_full()
                    .items_start()
                    .gap_1()
                    .child(
                        div()
                            .text_xl()
                            .font_weight(FontWeight::BOLD)
                            .child("排程群組與輸入參數設定"),
                    )
                    .child(
                        div()
                            .text_sm()
                            .text_color(MUTED_FOREGROUND)
                            .child("自由組合多程式步驟或單程式多批次，設定共同項目與各欄位輸入參數（包含欄位名稱、欄位 ID 及動態代碼）"),
                    ),
            )
            // Create / Select Group Bar
            .child(
                v_flex()
                    .w_full()
                    .bg(CARD)
                    .p_4()
                    .rounded_lg()
                    .border_1()
                    .border_color(BORDER)
                    .gap_3()
                    .child(
                        h_flex()
                            .w_full()
                            .flex_wrap()
                            .gap_2()
                            .items_center()
                            .child(div().w(px(220.)).min_w(px(160.)).child(Input::new(&self.new_group_name_input)))
                            .child(div().flex_1().min_w(px(200.)).child(Input::new(&self.new_group_desc_input)))
                            .child(
                                Button::new("btn-create-group")
                                    .primary()
                                    .label("新增排程群組")
                                    .on_click(cx.listener(|this, _: &ClickEvent, window, cx| {
                                        this.create_group(window, cx);
                                    })),
                            ),
                    )
                    .child({
                        let mut grp_tabs = h_flex().w_full().gap_2().flex_wrap().items_center();
                        if self.groups.is_empty() {
                            grp_tabs = grp_tabs.child(
                                div()
                                    .text_xs()
                                    .text_color(MUTED_FOREGROUND)
                                    .child("尚無群組，請於上方建立第一個排程群組"),
                            );
                        } else {
                            for g in &self.groups {
                                let is_sel = self
                                    .selected_group
                                    .as_ref()
                                    .map(|sg| sg.id == g.id)
                                    .unwrap_or(false);
                                let g_clone = g.clone();
                                let gid = g.id.unwrap_or(0);
                                grp_tabs = grp_tabs.child(
                                    div()
                                        .id(SharedString::from(format!("grp-tab-{gid}")))
                                        .px_3()
                                        .py_1()
                                        .rounded_md()
                                        .bg(if is_sel { PRIMARY } else { BACKGROUND })
                                        .border_1()
                                        .border_color(if is_sel { PRIMARY } else { BORDER })
                                        .text_xs()
                                        .font_weight(FontWeight::SEMIBOLD)
                                        .cursor_pointer()
                                        .on_click(cx.listener(move |this, _: &ClickEvent, window, cx| {
                                            this.selected_group = Some(g_clone.clone());
                                            this.reload_groups(window, cx);
                                        }))
                                        .child(format!("{} ({})", g.group_name, g.group_mode)),
                                );
                            }
                        }
                        grp_tabs
                    }),
            )
            // Group Body
            .child(if let Some(ref group) = self.selected_group {
                let left_panel = v_flex()
                    .w_full()
                    .gap_4()
                    // Mode + Add Program Step
                    .child(
                        v_flex()
                            .w_full()
                            .bg(CARD)
                            .p_4()
                            .rounded_lg()
                            .border_1()
                            .border_color(BORDER)
                            .gap_3()
                            .child(
                                h_flex()
                                    .w_full()
                                    .flex_wrap()
                                    .justify_between()
                                    .items_center()
                                    .gap_2()
                                    .child(
                                        div()
                                            .font_weight(FontWeight::SEMIBOLD)
                                            .child(format!("群組步驟編排：{}", group.group_name)),
                                    )
                                    .child(
                                        h_flex()
                                            .flex_wrap()
                                            .gap_2()
                                            .child(mode_pill(
                                                "MULTI",
                                                "MULTI 多程式串接",
                                                group.group_mode == "MULTI",
                                                cx.listener(|this, _, window, cx| {
                                                    this.set_group_mode("MULTI", window, cx);
                                                }),
                                            ))
                                            .child(mode_pill(
                                                "SINGLE",
                                                "SINGLE 單程式多批次",
                                                group.group_mode == "SINGLE",
                                                cx.listener(|this, _, window, cx| {
                                                    this.set_group_mode("SINGLE", window, cx);
                                                }),
                                            ))
                                            .child(
                                                Button::new("btn-del-group")
                                                    .danger()
                                                    .label("刪除群組")
                                                    .on_click(cx.listener(|this, _: &ClickEvent, window, cx| {
                                                        this.delete_selected_group(window, cx);
                                                    })),
                                            ),
                                    ),
                            )
                                    // Quick Add Program to Group
                                    .child({
                                        let mut add_bar = h_flex().gap_2().flex_wrap().items_center();
                                        add_bar = add_bar.child(
                                            div()
                                                .text_xs()
                                                .text_color(MUTED_FOREGROUND)
                                                .child("加入程式至步驟："),
                                        );
                                        for p in &self.programs {
                                            let pid = p.id.unwrap_or(0);
                                            add_bar = add_bar.child(
                                                Button::new(SharedString::from(format!("add-step-prog-{pid}")))
                                                    .ghost()
                                                    .label(format!("+ {}", p.job_code))
                                                    .on_click(cx.listener(move |this, _: &ClickEvent, window, cx| {
                                                        this.add_program_to_group(pid, window, cx);
                                                    })),
                                            );
                                        }
                                        add_bar
                                    })
                                    // Step List
                                    .child(if self.group_items.is_empty() {
                                        div()
                                            .p_4()
                                            .bg(BACKGROUND)
                                            .rounded_md()
                                            .text_xs()
                                            .text_color(MUTED_FOREGROUND)
                                            .child("此群組尚無步驟，請點選上方「+ 程式代號」加入執行步驟")
                                    } else {
                                        let mut rows = v_flex().gap_2();
                                        for item in &self.group_items {
                                            let iid = item.id.unwrap_or(0);
                                            let gid = group.id.unwrap_or(0);
                                            let is_sel = self
                                                .selected_item
                                                .as_ref()
                                                .map(|si| si.id == item.id)
                                                .unwrap_or(false);
                                            let prog = prog_map.get(&item.program_id).copied();
                                            let code = prog
                                                .map(|p| p.job_code.as_str())
                                                .unwrap_or("UNKNOWN");
                                            let name =
                                                prog.map(|p| p.job_name.as_str()).unwrap_or("");
                                            let item_clone = item.clone();

                                            rows = rows.child(
                                                h_flex()
                                                    .id(SharedString::from(format!("step-row-{iid}")))
                                                    .p_2()
                                                    .bg(if is_sel { SECONDARY } else { BACKGROUND })
                                                    .rounded_md()
                                                    .border_1()
                                                    .border_color(if is_sel { PRIMARY } else { BORDER })
                                                    .justify_between()
                                                    .items_center()
                                                    .cursor_pointer()
                                                    .on_click(cx.listener(move |this, _: &ClickEvent, window, cx| {
                                                        this.select_group_item(item_clone.clone(), window, cx);
                                                    }))
                                                    .child(
                                                        h_flex()
                                                            .gap_3()
                                                            .items_center()
                                                            .child(
                                                                div()
                                                                    .w(px(28.))
                                                                    .font_weight(FontWeight::BOLD)
                                                                    .child(format!("{}.", item.step_order)),
                                                            )
                                                            .child(
                                                                div()
                                                                    .w(px(90.))
                                                                    .font_weight(FontWeight::BOLD)
                                                                    .child(code.to_string()),
                                                            )
                                                            .child(
                                                                div()
                                                                    .text_xs()
                                                                    .child(name.to_string()),
                                                            ),
                                                    )
                                                    .child(
                                                        h_flex()
                                                            .gap_1()
                                                            .items_center()
                                                            .child(Tag::secondary().child(
                                                                if item.expect_exe_time.is_empty() {
                                                                    "即時".to_string()
                                                                } else {
                                                                    item.expect_exe_time.clone()
                                                                },
                                                            ))
                                                            .child(if item.link_prev_sn {
                                                                Tag::primary().child("串接上步序號")
                                                            } else {
                                                                Tag::secondary().child("獨立")
                                                            })
                                                            .child(
                                                                Button::new(SharedString::from(format!("up-step-{iid}")))
                                                                    .ghost()
                                                                    .label("↑")
                                                                    .on_click(cx.listener(move |this, _: &ClickEvent, window, cx| {
                                                                        {
                                                                            let guard = this.conn.lock().unwrap();
                                                                            let repo = Repository::new(&guard);
                                                                            let _ = repo.swap_group_item_order(gid, iid, true);
                                                                        }
                                                                        this.reload_groups(window, cx);
                                                                    })),
                                                            )
                                                            .child(
                                                                Button::new(SharedString::from(format!("dn-step-{iid}")))
                                                                    .ghost()
                                                                    .label("↓")
                                                                    .on_click(cx.listener(move |this, _: &ClickEvent, window, cx| {
                                                                        {
                                                                            let guard = this.conn.lock().unwrap();
                                                                            let repo = Repository::new(&guard);
                                                                            let _ = repo.swap_group_item_order(gid, iid, false);
                                                                        }
                                                                        this.reload_groups(window, cx);
                                                                    })),
                                                            )
                                                            .child(
                                                                Button::new(SharedString::from(format!("rm-step-{iid}")))
                                                                    .danger()
                                                                    .label("移")
                                                                    .on_click(cx.listener(move |this, _: &ClickEvent, window, cx| {
                                                                        {
                                                                            let guard = this.conn.lock().unwrap();
                                                                            let repo = Repository::new(&guard);
                                                                            let _ = repo.delete_group_item(gid, iid);
                                                                        }
                                                                        this.reload_groups(window, cx);
                                                                    })),
                                                            ),
                                                    ),
                                            );
                                        }
                                        rows
                                    }),
                            )
                            // Selected Step Common & Input Param Editor
                            .child(if let Some(ref sel_item) = self.selected_item {
                                let step_code = prog_map
                                    .get(&sel_item.program_id)
                                    .map(|p| p.job_code.as_str())
                                    .unwrap_or("");
                                self.render_step_detail_editor(step_code, &resolver, cx)
                                    .into_any_element()
                            } else {
                                div().into_any_element()
                            });

                let right_panel = v_flex()
                    .w_full()
                    .gap_4()
                    .child(
                        v_flex()
                            .w_full()
                            .bg(CARD)
                            .p_4()
                            .rounded_lg()
                            .border_1()
                            .border_color(BORDER)
                            .gap_2()
                            .child(
                                div()
                                    .font_weight(FontWeight::SEMIBOLD)
                                    .child("圖 1 動態代碼速查表"),
                            )
                            .child(rule_badge("E1", "執行日期當日 (YYYMMDD)"))
                            .child(rule_badge("E2~E8", "執行日期前 1~7 天"))
                            .child(rule_badge("E9", "系統/執行日期當月 (MM)"))
                            .child(rule_badge("EA~EC", "前一至三月份 (MM)"))
                            .child(rule_badge("ED", "系統/執行日期年月 (YYYMM)"))
                            .child(rule_badge("EE~EG", "前一至三年月 (YYYMM)"))
                            .child(rule_badge("WEEK_1~7", "本週星期一至日 (YYYMMDD)"))
                            .child(rule_badge("TEMPLATE", "樣板插值 ({代碼} 與自訂變數)")),
                    )
                    .child(
                        v_flex()
                            .w_full()
                            .bg(CARD)
                            .p_4()
                            .rounded_lg()
                            .border_1()
                            .border_color(BORDER)
                            .gap_2()
                            .child(
                                div()
                                    .font_weight(FontWeight::SEMIBOLD)
                                    .child("動態參數即時試算器"),
                            )
                            .child(div().text_xs().text_color(MUTED_FOREGROUND).child("基準日期 (YYY/MM/DD)"))
                            .child(Input::new(&self.tester_date_input))
                            .child(div().text_xs().text_color(MUTED_FOREGROUND).child("樣板 / 代碼表達式"))
                            .child(Input::new(&self.tester_expr_input))
                            .child(
                                div()
                                    .p_2()
                                    .bg(BACKGROUND)
                                    .rounded_md()
                                    .border_1()
                                    .border_color(BORDER)
                                    .text_xs()
                                    .text_color(SUCCESS)
                                    .child(format!("換算結果：{test_result}")),
                            ),
                    );

                if is_stacked {
                    v_flex()
                        .w_full()
                        .gap_4()
                        .items_start()
                        .child(left_panel)
                        .child(right_panel)
                        .into_any_element()
                } else {
                    h_flex()
                        .w_full()
                        .gap_5()
                        .items_start()
                        .child(div().flex_1().min_w_0().child(left_panel))
                        .child(div().w(px(330.)).flex_shrink_0().child(right_panel))
                        .into_any_element()
                }
            } else {
                div().into_any_element()
            })
    }

    pub(super) fn render_step_detail_editor(
        &self,
        step_code: &str,
        resolver: &ParamResolver,
        cx: &mut Context<Self>,
    ) -> impl IntoElement {
        let param_val_str = self.param_rule_value_input.read(cx).value().to_string();
        let preview_param_val = resolver.resolve(&self.param_rule_code, &param_val_str);

        v_flex()
            .w_full()
            .bg(CARD)
            .p_4()
            .rounded_lg()
            .border_1()
            .border_color(BORDER)
            .gap_4()
            .child(
                h_flex()
                    .w_full()
                    .flex_wrap()
                    .justify_between()
                    .items_center()
                    .gap_2()
                    .child(
                        div()
                            .font_weight(FontWeight::BOLD)
                            .child(format!("步驟設定：{step_code}")),
                    )
                    .child(
                        Button::new("btn-save-item-common")
                            .primary()
                            .label("儲存共同項目")
                            .on_click(cx.listener(|this, _: &ClickEvent, window, cx| {
                                this.save_selected_item_common(window, cx);
                            })),
                    ),
            )
            // Common Fields Grid
            .child(
                h_flex()
                    .w_full()
                    .flex_wrap()
                    .gap_3()
                    .child(flex_labeled_input("預計執行時間 (HH:mm)", &self.item_exe_time_input, px(180.)))
                    .child(flex_labeled_input("日期規則 (GROUP_DATE/WEEK_5)", &self.item_exe_date_rule_input, px(180.)))
                    .child(flex_labeled_input("Mail 收件者 (#mailRecipients)", &self.item_mail_input, px(180.))),
            )
            .child(
                h_flex()
                    .w_full()
                    .flex_wrap()
                    .gap_3()
                    .child(flex_labeled_input("前置序號 1 (#preJobSn1)", &self.item_pre_sn1_input, px(140.)))
                    .child(flex_labeled_input("前置序號 2 (#preJobSn2)", &self.item_pre_sn2_input, px(140.)))
                    .child(flex_labeled_input("前置序號 3 (#preJobSn3)", &self.item_pre_sn3_input, px(140.)))
                    .child(flex_labeled_input("重作序號 (#reWorkSn)", &self.item_rework_sn_input, px(140.))),
            )
            .child(
                h_flex()
                    .w_full()
                    .flex_wrap()
                    .gap_3()
                    .child(flex_labeled_input("前置備註 (#preWork)", &self.item_pre_note_input, px(200.)))
                    .child(flex_labeled_input("後置備註 (#rearWork)", &self.item_rear_note_input, px(200.))),
            )
            .child(
                h_flex()
                    .w_full()
                    .flex_wrap()
                    .gap_4()
                    .child(
                        Checkbox::new("chk-link-prev")
                            .label("自動帶入上一步序號 (#preJobSn1)")
                            .checked(self.item_link_prev)
                            .on_click(cx.listener(|this, checked, _, cx| {
                                this.item_link_prev = *checked;
                                cx.notify();
                            })),
                    )
                    .child(
                        Checkbox::new("chk-req-op")
                            .label("需要 OP 確認 (#reqOPConfirm)")
                            .checked(self.item_req_op)
                            .on_click(cx.listener(|this, checked, _, cx| {
                                this.item_req_op = *checked;
                                cx.notify();
                            })),
                    )
                    .child(
                        Checkbox::new("chk-req-email")
                            .label("發送 Mail 通知 (#reqEmail)")
                            .checked(self.item_req_email)
                            .on_click(cx.listener(|this, checked, _, cx| {
                                this.item_req_email = *checked;
                                cx.notify();
                            })),
                    ),
            )
            // Input Parameter Settings (欄位名稱 + 欄位 ID + 動態規則)
            .child(
                v_flex()
                    .w_full()
                    .pt_3()
                    .border_t_1()
                    .border_color(BORDER)
                    .gap_3()
                    .child(
                        h_flex()
                            .w_full()
                            .flex_wrap()
                            .justify_between()
                            .items_center()
                            .gap_2()
                            .child(
                                div()
                                    .font_weight(FontWeight::SEMIBOLD)
                                    .child("輸入參數設定（包含欄位名稱及欄位 ID）"),
                            )
                            .child(
                                Button::new("btn-new-param")
                                    .ghost()
                                    .label("+ 清空切換為新增參數")
                                    .on_click(cx.listener(|this, _: &ClickEvent, window, cx| {
                                        this.clear_param_editor(window, cx);
                                    })),
                            ),
                    )
                    // Editor Form for 欄位名稱 + 欄位 ID + Rule
                    .child(
                        v_flex()
                            .w_full()
                            .p_3()
                            .bg(BACKGROUND)
                            .rounded_md()
                            .border_1()
                            .border_color(BORDER)
                            .gap_2()
                            .child(
                                h_flex()
                                    .w_full()
                                    .flex_wrap()
                                    .gap_2()
                                    .items_end()
                                    .child(flex_labeled_input("欄位名稱 (Label)", &self.param_label_input, px(160.)))
                                    .child(flex_labeled_input("欄位 ID (HTML ID)", &self.param_html_id_input, px(160.)))
                                    .child(flex_labeled_input("規則值 / 樣板", &self.param_rule_value_input, px(160.))),
                            )
                            .child({
                                let mut rc_bar = h_flex().gap_1().flex_wrap().items_center();
                                rc_bar = rc_bar.child(
                                    div()
                                        .text_xs()
                                        .text_color(MUTED_FOREGROUND)
                                        .child("規則代碼："),
                                );
                                for &rc in RULE_CODES {
                                    let is_active = self.param_rule_code == rc;
                                    rc_bar = rc_bar.child(
                                        div()
                                            .id(SharedString::from(format!("rc-{rc}")))
                                            .px_2()
                                            .py_1()
                                            .rounded_md()
                                            .text_xs()
                                            .bg(if is_active { PRIMARY } else { SECONDARY })
                                            .border_1()
                                            .border_color(if is_active { PRIMARY } else { BORDER })
                                            .cursor_pointer()
                                            .on_click(cx.listener(move |this, _: &ClickEvent, _, cx| {
                                                this.param_rule_code = rc.to_string();
                                                cx.notify();
                                            }))
                                            .child(rc),
                                    );
                                }
                                rc_bar
                            })
                            .child(
                                h_flex()
                                    .justify_between()
                                    .items_center()
                                    .child(
                                        h_flex()
                                            .gap_2()
                                            .items_center()
                                            .child(scope_pill("PROGRAM", &self.param_field_scope, cx.listener(|this, _, _, cx| {
                                                this.param_field_scope = "PROGRAM".to_string();
                                                cx.notify();
                                            })))
                                            .child(scope_pill("JOB", &self.param_field_scope, cx.listener(|this, _, _, cx| {
                                                this.param_field_scope = "JOB".to_string();
                                                cx.notify();
                                            })))
                                            .child(
                                                div()
                                                    .text_xs()
                                                    .text_color(SUCCESS)
                                                    .child(format!("試算預覽 ➜ {preview_param_val}")),
                                            ),
                                    )
                                    .child(
                                        Button::new("btn-save-param")
                                            .primary()
                                            .label("儲存輸入參數設定")
                                            .on_click(cx.listener(|this, _: &ClickEvent, window, cx| {
                                                this.save_item_param_with_field(window, cx);
                                            })),
                                    ),
                            ),
                    )
                    // Table of all fields with 欄位名稱 + 欄位 ID + Rule
                    .child(if self.selected_item_params.is_empty() {
                        div()
                            .text_xs()
                            .text_color(MUTED_FOREGROUND)
                            .p_2()
                            .child("此程式目前無欄位定義，請直接於上方輸入「欄位名稱」與「欄位 ID」新增參數")
                    } else {
                        let mut table = v_flex().gap_1();
                        table = table.child(
                            h_flex()
                                .p_2()
                                .bg(BACKGROUND)
                                .rounded_md()
                                .text_xs()
                                .font_weight(FontWeight::BOLD)
                                .text_color(MUTED_FOREGROUND)
                                .child(div().flex_1().child("欄位名稱 (Label)"))
                                .child(div().w(px(160.)).child("欄位 ID (HTML ID)"))
                                .child(div().w(px(120.)).child("規則代碼"))
                                .child(div().w(px(160.)).child("規則值 / 試算"))
                                .child(div().w(px(110.)).child("操作")),
                        );

                        for p in &self.selected_item_params {
                            let p_clone = p.clone();
                            let fid = p.field_id;
                            let preview = resolver.resolve(&p.rule_code, &p.rule_value);
                            table = table.child(
                                h_flex()
                                    .p_2()
                                    .bg(BACKGROUND)
                                    .rounded_md()
                                    .border_1()
                                    .border_color(BORDER)
                                    .text_xs()
                                    .items_center()
                                    .gap_2()
                                    .child(div().flex_1().child(p.label.clone()))
                                    .child(
                                        div()
                                            .w(px(160.))
                                            .font_weight(FontWeight::MEDIUM)
                                            .child(p.html_id.clone()),
                                    )
                                    .child(div().w(px(120.)).child(if p.is_configured {
                                        Tag::primary().child(p.rule_code.clone())
                                    } else {
                                        Tag::secondary().child("未設定")
                                    }))
                                    .child(
                                        div()
                                            .w(px(160.))
                                            .text_color(if p.is_configured {
                                                SUCCESS
                                            } else {
                                                MUTED_FOREGROUND
                                            })
                                            .child(if p.is_configured {
                                                format!("{} ➜ {}", p.rule_value, preview)
                                            } else {
                                                "(尚未設定)".to_string()
                                            }),
                                    )
                                    .child(
                                        h_flex()
                                            .w(px(110.))
                                            .gap_1()
                                            .child(
                                                Button::new(SharedString::from(format!("pick-param-{fid}")))
                                                    .ghost()
                                                    .label("編輯")
                                                    .on_click(cx.listener(move |this, _: &ClickEvent, window, cx| {
                                                        this.load_param_into_editor(&p_clone, window, cx);
                                                    })),
                                            )
                                            .child(
                                                Button::new(SharedString::from(format!("clr-param-{fid}")))
                                                    .danger()
                                                    .label("清")
                                                    .on_click(cx.listener(move |this, _: &ClickEvent, window, cx| {
                                                        this.delete_item_param(fid, window, cx);
                                                    })),
                                            ),
                                    ),
                            );
                        }
                        table
                    }),
            )
    }
}
