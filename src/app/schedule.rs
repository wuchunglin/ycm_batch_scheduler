//! 排程規則產生器：動作與畫面

use super::*;
use super::helpers::{labeled_input, mode_pill};

impl BatchSchedulerApp {
    // --- Actions: Schedule Generator ---
    pub(super) fn sync_schedule_inputs_from_rule(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        for &y in &self.schedule_rule.years {
            if !self.available_years.contains(&y) {
                self.available_years.push(y);
            }
        }
        self.available_years.sort();

        self.sched_default_time_input.update(cx, |s, cx| {
            s.set_value(self.schedule_rule.default_time.clone(), window, cx)
        });
        if let DateRangeMode::CustomRange { start, end } = &self.schedule_rule.range_mode {
            self.sched_range_start_input
                .update(cx, |s, cx| s.set_value(start.clone(), window, cx));
            self.sched_range_end_input
                .update(cx, |s, cx| s.set_value(end.clone(), window, cx));
        }
        match &self.schedule_rule.schedule_mode {
            ScheduleMode::Monthly { days, .. } => {
                let text = days
                    .iter()
                    .map(|d| d.to_string())
                    .collect::<Vec<_>>()
                    .join(", ");
                self.sched_monthly_days_input
                    .update(cx, |s, cx| s.set_value(text, window, cx));
            }
            ScheduleMode::SpecificDates { dates } => {
                let text = dates.join(", ");
                self.sched_specific_dates_input
                    .update(cx, |s, cx| s.set_value(text, window, cx));
            }
            _ => {}
        }
    }

    pub(super) fn apply_schedule_inputs_and_save(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let default_time = self
            .sched_default_time_input
            .read(cx)
            .value()
            .trim()
            .to_string();
        self.schedule_rule.default_time = if default_time.is_empty() {
            "13:00".to_string()
        } else {
            default_time
        };

        if let DateRangeMode::CustomRange { .. } = &self.schedule_rule.range_mode {
            let start = self
                .sched_range_start_input
                .read(cx)
                .value()
                .trim()
                .to_string();
            let end = self
                .sched_range_end_input
                .read(cx)
                .value()
                .trim()
                .to_string();
            self.schedule_rule.range_mode = DateRangeMode::CustomRange { start, end };
        }

        match &mut self.schedule_rule.schedule_mode {
            ScheduleMode::Monthly { days, .. } => {
                let raw = self.sched_monthly_days_input.read(cx).value().to_string();
                let parsed: Vec<u32> = raw
                    .split([',', ' ', '、'])
                    .filter_map(|t| t.trim().parse::<u32>().ok())
                    .filter(|&d| (1..=31).contains(&d))
                    .take(3)
                    .collect();
                *days = parsed;
            }
            ScheduleMode::SpecificDates { dates } => {
                let raw = self.sched_specific_dates_input.read(cx).value().to_string();
                let parsed: Vec<String> = raw
                    .split([',', ';', '、'])
                    .map(|t| t.trim().to_string())
                    .filter(|t| !t.is_empty())
                    .take(5)
                    .collect();
                *dates = parsed;
            }
            _ => {}
        }

        self.preview_items = ScheduleGenerator::generate(&self.schedule_rule);

        if let Some(gid) = self.selected_group.as_ref().and_then(|g| g.id) {
            if let Ok(json) = serde_json::to_string(&self.schedule_rule) {
                let guard = self.conn.lock().unwrap();
                let repo = Repository::new(&guard);
                let _ = repo.update_group_schedule_rule(gid, &json);
            }
            self.reload_groups(window, cx);
            window.push_notification(
                Notification::success(format!(
                    "已儲存排程規則至群組，共展開 {} 筆排程日",
                    self.preview_items.len()
                )),
                cx,
            );
        } else {
            cx.notify();
        }
    }

    pub(super) fn render_schedule_view(&self, viewport_w: gpui::Pixels, cx: &mut Context<Self>) -> impl IntoElement {
        let is_stacked = viewport_w < px(1080.);
        let weekday_names = ["週一", "週二", "週三", "週四", "週五", "週六", "週日"];

        let controls_panel = v_flex()
            .w_full()
            .bg(CARD)
            .p_4()
            .rounded_lg()
            .border_1()
            .border_color(BORDER)
            .gap_4()
            .child(div().font_weight(FontWeight::BOLD).child(format!(
                "圖 2 控制項設定（群組：{}）",
                self.selected_group
                    .as_ref()
                    .map(|g| g.group_name.as_str())
                    .unwrap_or("未選取")
            )));

        let controls_rest = controls_panel
                            // 1. Years
                            .child(
                                v_flex()
                                    .gap_2()
                                    .child(div().text_sm().font_weight(FontWeight::SEMIBOLD).child("1. 年度別（民國年）"))
                                    .child({
                                        let mut row = h_flex().gap_2().flex_wrap();
                                        for &y in &self.available_years {
                                            let checked = self.schedule_rule.years.contains(&y);
                                            row = row.child(
                                                Checkbox::new(SharedString::from(format!("yr-{y}")))
                                                    .label(format!("{y} ({})", y + 1911))
                                                    .checked(checked)
                                                    .on_click(cx.listener(move |this, is_chk, _, cx| {
                                                        if *is_chk {
                                                            if !this.schedule_rule.years.contains(&y) {
                                                                this.schedule_rule.years.push(y);
                                                                this.schedule_rule.years.sort();
                                                            }
                                                        } else {
                                                            this.schedule_rule.years.retain(|&x| x != y);
                                                        }
                                                        this.preview_items = ScheduleGenerator::generate(&this.schedule_rule);
                                                        cx.notify();
                                                    })),
                                            );
                                        }
                                        row
                                    })
                                    .child(
                                        h_flex()
                                            .gap_2()
                                            .child(div().flex_1().child(Input::new(&self.sched_add_year_input)))
                                            .child(
                                                Button::new("btn-add-yr")
                                                    .ghost()
                                                    .label("+ 新增年度")
                                                    .on_click(cx.listener(|this, _: &ClickEvent, window, cx| {
                                                        if let Ok(y) = this.sched_add_year_input.read(cx).value().trim().parse::<i32>() {
                                                            if !this.available_years.contains(&y) {
                                                                this.available_years.push(y);
                                                                this.available_years.sort();
                                                            }
                                                            if !this.schedule_rule.years.contains(&y) {
                                                                this.schedule_rule.years.push(y);
                                                                this.schedule_rule.years.sort();
                                                            }
                                                            this.sched_add_year_input.update(cx, |s, cx| s.set_value("", window, cx));
                                                            this.preview_items = ScheduleGenerator::generate(&this.schedule_rule);
                                                            cx.notify();
                                                        }
                                                    })),
                                            ),
                                    ),
                            )
                            // 2. Date Range
                            .child(
                                v_flex()
                                    .gap_2()
                                    .child(div().text_sm().font_weight(FontWeight::SEMIBOLD).child("2. 時間區間"))
                                    .child(
                                        h_flex()
                                            .gap_2()
                                            .child(mode_pill(
                                                "range-all",
                                                "整年 (01/01~12/31)",
                                                matches!(self.schedule_rule.range_mode, DateRangeMode::AllYear),
                                                cx.listener(|this, _, _, cx| {
                                                    this.schedule_rule.range_mode = DateRangeMode::AllYear;
                                                    this.preview_items = ScheduleGenerator::generate(&this.schedule_rule);
                                                    cx.notify();
                                                }),
                                            ))
                                            .child(mode_pill(
                                                "range-custom",
                                                "自訂區間",
                                                matches!(self.schedule_rule.range_mode, DateRangeMode::CustomRange { .. }),
                                                cx.listener(|this, _, _, cx| {
                                                    let start = this.sched_range_start_input.read(cx).value().to_string();
                                                    let end = this.sched_range_end_input.read(cx).value().to_string();
                                                    this.schedule_rule.range_mode = DateRangeMode::CustomRange { start, end };
                                                    this.preview_items = ScheduleGenerator::generate(&this.schedule_rule);
                                                    cx.notify();
                                                }),
                                            )),
                                    )
                                    .child(if matches!(self.schedule_rule.range_mode, DateRangeMode::CustomRange { .. }) {
                                        h_flex()
                                            .gap_2()
                                            .child(div().flex_1().child(Input::new(&self.sched_range_start_input)))
                                            .child(div().flex_1().child(Input::new(&self.sched_range_end_input)))
                                            .into_any_element()
                                    } else {
                                        div().into_any_element()
                                    }),
                            )
                            // 3. Schedule Mode
                            .child(
                                v_flex()
                                    .gap_2()
                                    .child(div().text_sm().font_weight(FontWeight::SEMIBOLD).child("3. 預定排程模式"))
                                    .child(
                                        h_flex()
                                            .gap_2()
                                            .flex_wrap()
                                            .child(mode_pill(
                                                "sm-monthly",
                                                "每月",
                                                matches!(self.schedule_rule.schedule_mode, ScheduleMode::Monthly { .. }),
                                                cx.listener(|this, _, _, cx| {
                                                    this.schedule_rule.schedule_mode = ScheduleMode::Monthly {
                                                        days: vec![1, 15],
                                                        last_day: true,
                                                    };
                                                    this.preview_items = ScheduleGenerator::generate(&this.schedule_rule);
                                                    cx.notify();
                                                }),
                                            ))
                                            .child(mode_pill(
                                                "sm-weekly",
                                                "每週",
                                                matches!(self.schedule_rule.schedule_mode, ScheduleMode::Weekly { .. }),
                                                cx.listener(|this, _, _, cx| {
                                                    this.schedule_rule.schedule_mode = ScheduleMode::Weekly { weekdays: vec![5] };
                                                    this.preview_items = ScheduleGenerator::generate(&this.schedule_rule);
                                                    cx.notify();
                                                }),
                                            ))
                                            .child(mode_pill(
                                                "sm-specific",
                                                "指定日期",
                                                matches!(self.schedule_rule.schedule_mode, ScheduleMode::SpecificDates { .. }),
                                                cx.listener(|this, _, _, cx| {
                                                    this.schedule_rule.schedule_mode = ScheduleMode::SpecificDates { dates: vec![] };
                                                    this.preview_items = ScheduleGenerator::generate(&this.schedule_rule);
                                                    cx.notify();
                                                }),
                                            ))
                                            .child(mode_pill(
                                                "sm-single",
                                                "單次執行",
                                                matches!(self.schedule_rule.schedule_mode, ScheduleMode::SingleRun { .. }),
                                                cx.listener(|this, _, _, cx| {
                                                    this.schedule_rule.schedule_mode = ScheduleMode::SingleRun { date: None };
                                                    this.preview_items = ScheduleGenerator::generate(&this.schedule_rule);
                                                    cx.notify();
                                                }),
                                            )),
                                    )
                                    .child(match &self.schedule_rule.schedule_mode {
                                        ScheduleMode::Monthly { last_day, .. } => v_flex()
                                            .gap_2()
                                            .child(Input::new(&self.sched_monthly_days_input))
                                            .child(
                                                Checkbox::new("chk-last-day")
                                                    .label("包含每月最後一天")
                                                    .checked(*last_day)
                                                    .on_click(cx.listener(|this, chk, _, cx| {
                                                        if let ScheduleMode::Monthly { last_day, .. } = &mut this.schedule_rule.schedule_mode {
                                                            *last_day = *chk;
                                                        }
                                                        this.preview_items = ScheduleGenerator::generate(&this.schedule_rule);
                                                        cx.notify();
                                                    })),
                                            )
                                            .into_any_element(),
                                        ScheduleMode::Weekly { weekdays } => {
                                            let mut row = h_flex().gap_1().flex_wrap();
                                            for (idx, wname) in weekday_names.iter().enumerate() {
                                                let dnum = (idx + 1) as u32;
                                                let active = weekdays.contains(&dnum);
                                                row = row.child(mode_pill(
                                                    SharedString::from(format!("wd-{dnum}")),
                                                    wname,
                                                    active,
                                                    cx.listener(move |this, _, _, cx| {
                                                        if let ScheduleMode::Weekly { weekdays } = &mut this.schedule_rule.schedule_mode {
                                                            if weekdays.contains(&dnum) {
                                                                weekdays.retain(|&x| x != dnum);
                                                            } else {
                                                                weekdays.push(dnum);
                                                                weekdays.sort();
                                                            }
                                                        }
                                                        this.preview_items = ScheduleGenerator::generate(&this.schedule_rule);
                                                        cx.notify();
                                                    }),
                                                ));
                                            }
                                            row.into_any_element()
                                        }
                                        ScheduleMode::SpecificDates { .. } => {
                                            Input::new(&self.sched_specific_dates_input).into_any_element()
                                        }
                                        ScheduleMode::SingleRun { .. } => div()
                                            .text_xs()
                                            .text_color(MUTED_FOREGROUND)
                                            .child("以當日或執行監控台指定之單日執行一輪")
                                            .into_any_element(),
                                    }),
                            )
                            // 4. Default Time
                            .child(labeled_input("4. 預計執行時間 (HH:mm)", &self.sched_default_time_input));

        let preview_panel = v_flex()
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
                    .child(div().font_weight(FontWeight::SEMIBOLD).child("展開後排程清單預覽"))
                    .child(
                        Tag::success().child(format!(
                            "共展開 {} 筆排程日",
                            self.preview_items.len()
                        )),
                    ),
            )
            .child(
                div()
                    .id("schedule-preview-table")
                    .w_full()
                    .max_h(px(480.))
                    .overflow_y_scroll()
                    .child({
                        let mut preview_rows = v_flex().w_full().gap_1();
                        let vars: HashMap<String, String> = self
                            .custom_variables
                            .iter()
                            .map(|v| (v.var_key.clone(), v.var_value.clone()))
                            .collect();

                        for (idx, item) in self.preview_items.iter().enumerate().take(25) {
                            let resolved_summary = if let Some(d) = RocDateUtils::parse_roc_date(&item.roc_date_raw) {
                                let r = ParamResolver::new(d).with_custom_vars(vars.clone());
                                if let Some(ref sel_item) = self.selected_item {
                                    let vals: Vec<String> = self
                                        .selected_item_params
                                        .iter()
                                        .filter(|p| p.is_configured)
                                        .map(|p| format!("{}[{}]={}", p.label, p.html_id, r.resolve(&p.rule_code, &p.rule_value)))
                                        .collect();
                                    if vals.is_empty() {
                                        format!("步驟 {} 無專屬參數 (E1={})", sel_item.step_order, r.resolve("E1", ""))
                                    } else {
                                        vals.join(" | ")
                                    }
                                } else {
                                    format!("E1={}, ED={}", r.resolve("E1", ""), r.resolve("ED", ""))
                                }
                            } else {
                                String::new()
                            };

                            preview_rows = preview_rows.child(
                                h_flex()
                                    .w_full()
                                    .flex_wrap()
                                    .p_2()
                                    .bg(BACKGROUND)
                                    .rounded_md()
                                    .text_xs()
                                    .items_center()
                                    .gap_3()
                                    .child(div().w(px(36.)).child(format!("#{}", idx + 1)))
                                    .child(
                                        div()
                                            .w(px(90.))
                                            .font_weight(FontWeight::BOLD)
                                            .child(item.roc_date.clone()),
                                    )
                                    .child(div().w(px(75.)).child(item.exe_time.clone()))
                                    .child(
                                        div()
                                            .flex_1()
                                            .min_w(px(160.))
                                            .text_color(MUTED_FOREGROUND)
                                            .child(resolved_summary),
                                    ),
                            );
                        }
                        if self.preview_items.len() > 25 {
                            preview_rows = preview_rows.child(
                                div()
                                    .text_xs()
                                    .text_color(MUTED_FOREGROUND)
                                    .p_2()
                                    .child(format!("... 還有 {} 筆排程日", self.preview_items.len() - 25)),
                            );
                        }
                        preview_rows
                    }),
            );

        v_flex()
            .w_full()
            .items_start()
            .gap_5()
            .child(
                h_flex()
                    .w_full()
                    .flex_wrap()
                    .justify_between()
                    .items_center()
                    .gap_3()
                    .child(
                        v_flex()
                            .gap_1()
                            .child(
                                div()
                                    .text_xl()
                                    .font_weight(FontWeight::BOLD)
                                    .child("排程時間生成器（整合圖 2 規格）"),
                            )
                            .child(
                                div()
                                    .text_sm()
                                    .text_color(MUTED_FOREGROUND)
                                    .child("設定選定群組之年度別、時間區間與每月/每週/指定日期規則，即時展開排程清單"),
                            ),
                    )
                    .child(
                        Button::new("btn-gen-schedule")
                            .primary()
                            .label("套用規則至群組並展開")
                            .on_click(cx.listener(|this, _: &ClickEvent, window, cx| {
                                this.apply_schedule_inputs_and_save(window, cx);
                            })),
                    ),
            )
            .child(if is_stacked {
                v_flex()
                    .w_full()
                    .gap_4()
                    .items_start()
                    .child(controls_rest)
                    .child(preview_panel)
                    .into_any_element()
            } else {
                h_flex()
                    .w_full()
                    .gap_5()
                    .items_start()
                    .child(div().w(px(380.)).flex_shrink_0().child(controls_rest))
                    .child(div().flex_1().min_w_0().child(preview_panel))
                    .into_any_element()
            })
    }
}
