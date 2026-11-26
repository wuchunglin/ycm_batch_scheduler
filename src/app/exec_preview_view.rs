//! 排程設定及執行 → 下一步：執行清單預覽（所有欲執行程式及其參數）

use super::*;
use chrono::Datelike;

/// 預覽頁一列參數
struct ParamPreviewRow {
    label: String,
    html_id: String,
    /// 「批次程式新增」下拉式選單選的預設輸入值名稱
    default_name: String,
    resolved: String,
    configured: bool,
}

impl BatchSchedulerApp {
    fn exec_schedule_summary(&self, cx: &Context<Self>) -> (String, String) {
        let year = match self.exec_year {
            Some(y) => format!("{y} 年度"),
            None => format!(
                "時間區間 {} ~ {}",
                self.exec_range_start_input.read(cx).value().trim(),
                self.exec_range_end_input.read(cx).value().trim()
            ),
        };
        let date = match self.exec_date_mode {
            0 => format!("每月 {}", self.exec_monthly_input.read(cx).value().trim()),
            1 => "每月最後一天".to_string(),
            2 => {
                let names = ["一", "二", "三", "四", "五", "六", "日"];
                let picked: Vec<&str> = names
                    .iter()
                    .enumerate()
                    .filter(|(i, _)| self.exec_weekdays[*i])
                    .map(|(_, n)| *n)
                    .collect();
                format!("每周 {}", if picked.is_empty() { "（未勾選）".to_string() } else { picked.join("、") })
            }
            _ => {
                let dates: Vec<String> = self
                    .exec_specific_inputs
                    .iter()
                    .map(|s| s.read(cx).value().trim().to_string())
                    .filter(|s| !s.is_empty())
                    .collect();
                format!("指定日期 {}", if dates.is_empty() { "（未填）".to_string() } else { dates.join("、") })
            }
        };
        (year, date)
    }

    /// 由畫面上的排程條件組出 ScheduleRule，並展開成實際排程日期（民國年）
    fn exec_generated_dates(&self, cx: &Context<Self>) -> Vec<GeneratedScheduleItem> {
        let (years, range_mode) = match self.exec_year {
            Some(y) => (vec![y], DateRangeMode::AllYear),
            None => {
                let start = self.exec_range_start_input.read(cx).value().trim().to_string();
                let end = self.exec_range_end_input.read(cx).value().trim().to_string();
                let mut years = Vec::new();
                for s in [&start, &end] {
                    if let Some(d) = RocDateUtils::parse_roc_date(s) {
                        let y = d.year() - 1911;
                        if !years.contains(&y) {
                            years.push(y);
                        }
                    }
                }
                if let (Some(a), Some(b)) = (years.first().copied(), years.last().copied()) {
                    years = (a.min(b)..=a.max(b)).collect();
                }
                (years, DateRangeMode::CustomRange { start, end })
            }
        };
        let schedule_mode = match self.exec_date_mode {
            0 => ScheduleMode::Monthly {
                days: self
                    .exec_monthly_input
                    .read(cx)
                    .value()
                    .split(|c: char| c == ',' || c == '，' || c == '、' || c.is_whitespace())
                    .filter_map(|t| t.trim().parse::<u32>().ok())
                    .collect(),
                last_day: false,
            },
            1 => ScheduleMode::Monthly { days: vec![], last_day: true },
            2 => ScheduleMode::Weekly {
                weekdays: (0..7)
                    .filter(|i| self.exec_weekdays[*i])
                    .map(|i| i as u32 + 1)
                    .collect(),
            },
            _ => ScheduleMode::SpecificDates {
                dates: self
                    .exec_specific_inputs
                    .iter()
                    .map(|s| s.read(cx).value().trim().to_string())
                    .filter(|s| !s.is_empty())
                    .collect(),
            },
        };
        // 指定日期不受年度限制
        let years = if self.exec_date_mode == 3 { vec![] } else { years };
        ScheduleGenerator::generate(&ScheduleRule {
            years,
            range_mode,
            schedule_mode,
            default_time: "13:00".to_string(),
        })
    }

    /// 取得某一執行項目的參數：一律讀「批次程式新增」儲存的欄位預設值（不使用群組步驟舊有規則）
    fn exec_entry_params(&self, e: &ExecEntry, resolver: &ParamResolver) -> Vec<ParamPreviewRow> {
        let guard = self.conn.lock().unwrap();
        let repo = Repository::new(&guard);
        let mut rows = Vec::new();
        let fields = repo.list_program_fields(e.program_id).unwrap_or_default();
        let defaults = repo.field_defaults_for_program(e.program_id).unwrap_or_default();
        for f in fields {
            let fid = f.id.unwrap_or(0);
            let dv = defaults
                .get(&fid)
                .and_then(|did| self.default_values.iter().find(|d| d.id == Some(*did)));
            rows.push(match dv {
                Some(d) => ParamPreviewRow {
                    label: f.label,
                    html_id: f.html_id,
                    default_name: d.name.clone(),
                    resolved: {
                        let v = resolver.resolve(&d.rule_code, &d.rule_value);
                        if v.is_empty() { "（空白）".to_string() } else { v }
                    },
                    configured: true,
                },
                None => ParamPreviewRow {
                    label: f.label,
                    html_id: f.html_id,
                    default_name: "（未設定）".into(),
                    resolved: "（尚未設定）".into(),
                    configured: false,
                },
            });
        }
        rows
    }

    pub(super) fn render_exec_preview_view(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let dates = self.exec_generated_dates(cx);
        let sel_idx = self.exec_preview_date_idx.min(dates.len().saturating_sub(1));
        let base_date = dates
            .get(sel_idx)
            .and_then(|d| RocDateUtils::parse_roc_date(&d.roc_date))
            .unwrap_or_else(|| Local::now().date_naive());
        let resolver = ParamResolver::new(base_date).with_custom_vars(
            self.custom_variables
                .iter()
                .map(|v| (v.var_key.clone(), v.var_value.clone()))
                .collect(),
        );
        let base_txt = RocDateUtils::to_roc_yyymmdd(&base_date, "/");
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
        let (year_txt, date_txt) = self.exec_schedule_summary(cx);

        let mut root = v_flex()
            .w_full()
            .items_start()
            .gap_4()
            .child(
                h_flex()
                    .w_full()
                    .items_center()
                    .justify_between()
                    .child(
                        v_flex()
                            .gap_1()
                            .child(
                                div()
                                    .text_xl()
                                    .font_weight(FontWeight::BOLD)
                                    .child("執行清單預覽"),
                            )
                            .child(
                                div()
                                    .text_sm()
                                    .text_color(MUTED_FOREGROUND)
                                    .child("確認所有欲執行的批次程式與其參數"),
                            ),
                    )
                    .child(
                        Button::new("ep-back")
                            .label("上一步")
                            .on_click(cx.listener(|this, _: &ClickEvent, _, cx| {
                                this.active_tab = NavTab::ScheduleExec;
                                cx.notify();
                            })),
                    ),
            )
            .child(
                card()
                    .child(div().font_weight(FontWeight::SEMIBOLD).child("排程條件"))
                    .child(
                        v_flex()
                            .gap_1()
                            .text_size(px(14.))
                            .child(format!("年度：{year_txt}"))
                            .child(format!("排程日期：{date_txt}")),
                    ),
            );

        // 轉換後的排程日期（點選其一作為參數轉換的基準日）
        let mut chips = h_flex().w_full().flex_wrap().gap_2();
        if dates.is_empty() {
            chips = chips.child(
                div()
                    .text_size(px(14.))
                    .text_color(MUTED_FOREGROUND)
                    .child("依目前條件沒有產生任何日期，參數以今天作為基準日轉換"),
            );
        }
        for (i, d) in dates.iter().enumerate() {
            let active = i == sel_idx;
            chips = chips.child(
                div()
                    .id(SharedString::from(format!("ep-date-{i}")))
                    .px_2()
                    .py_1()
                    .rounded_md()
                    .text_size(px(14.))
                    .border_1()
                    .border_color(if active { PRIMARY } else { BORDER })
                    .bg(if active { PRIMARY } else { BACKGROUND })
                    .text_color(if active { hsl(0.0, 0.0, 100.0) } else { FOREGROUND })
                    .cursor_pointer()
                    .on_click(cx.listener(move |this, _: &ClickEvent, _, cx| {
                        this.exec_preview_date_idx = i;
                        cx.notify();
                    }))
                    .child(d.roc_date.clone()),
            );
        }
        root = root.child(
            card()
                .child(
                    div()
                        .font_weight(FontWeight::SEMIBOLD)
                        .child(format!("轉換後的排程日期（{} 天）", dates.len())),
                )
                .child(
                    div()
                        .id("ep-dates-scroll")
                        .w_full()
                        .max_h(px(160.))
                        .overflow_y_scroll()
                        .child(chips),
                )
                .child(
                    div()
                        .text_size(px(14.))
                        .text_color(MUTED_FOREGROUND)
                        .child(format!("下方參數以 {base_txt} 為基準日轉換（點選日期可切換）")),
                ),
        );

        if self.exec_entries.is_empty() {
            root = root.child(
                card().child(
                    div()
                        .text_size(px(14.))
                        .text_color(MUTED_FOREGROUND)
                        .child("執行清單是空的，請按「上一步」回去勾選"),
                ),
            );
        }

        for (idx, e) in self.exec_entries.iter().enumerate() {
            let (code, name) = prog_map
                .get(&e.program_id)
                .map(|p| (p.job_code.clone(), p.job_name.clone()))
                .unwrap_or_else(|| ("?".to_string(), "（程式已刪除）".to_string()));
            let time = e.time_input.read(cx).value().trim().to_string();
            let params = self.exec_entry_params(e, &resolver);

            let mut table = v_flex().w_full().gap_1().child(
                h_flex()
                    .w_full()
                    .px_2()
                    .py_1()
                    .gap_2()
                    .text_size(px(14.))
                    .font_weight(FontWeight::BOLD)
                    .text_color(MUTED_FOREGROUND)
                    .child(div().flex_1().min_w(px(120.)).child("欄位名稱"))
                    .child(div().w(px(160.)).flex_shrink_0().child("欄位 ID"))
                    .child(div().w(px(280.)).flex_shrink_0().child("預設輸入值"))
                    .child(div().flex_1().min_w(px(120.)).child("轉換後參數值")),
            );
            if params.is_empty() {
                table = table.child(
                    div()
                        .px_2()
                        .text_size(px(14.))
                        .text_color(MUTED_FOREGROUND)
                        .child("此程式沒有輸入欄位"),
                );
            }
            for p in params {
                table = table.child(
                    h_flex()
                        .w_full()
                        .px_2()
                        .py_1()
                        .gap_2()
                        .items_center()
                        .text_size(px(14.))
                        .bg(BACKGROUND)
                        .rounded_md()
                        .border_1()
                        .border_color(BORDER)
                        .child(div().flex_1().min_w(px(120.)).child(p.label))
                        .child(div().w(px(160.)).flex_shrink_0().child(p.html_id))
                        .child(
                            div()
                                .w(px(280.))
                                .flex_shrink_0()
                                .text_color(if p.configured { FOREGROUND } else { MUTED_FOREGROUND })
                                .child(p.default_name),
                        )
                        .child(
                            div()
                                .flex_1()
                                .min_w(px(120.))
                                .text_color(if p.configured { SUCCESS } else { MUTED_FOREGROUND })
                                .child(p.resolved),
                        ),
                );
            }

            root = root.child(
                card()
                    .child(
                        h_flex()
                            .w_full()
                            .flex_wrap()
                            .items_center()
                            .gap_3()
                            .text_size(px(14.))
                            .child(
                                div()
                                    .text_color(MUTED_FOREGROUND)
                                    .child(format!("{}", idx + 1)),
                            )
                            .child(
                                div()
                                    .font_weight(FontWeight::BOLD)
                                    .child(format!("{}　{}", code, name)),
                            )
                            .child(Tag::secondary().child(if time.is_empty() {
                                "即時".to_string()
                            } else {
                                time
                            }))
                            .child(div().text_color(MUTED_FOREGROUND).child(format!(
                                "前置工作序號：{}　｜　發送mail通知：{}",
                                if e.link_prev { "是" } else { "否" },
                                if e.req_email { "是" } else { "否" },
                            ))),
                    )
                    .child(table),
            );
        }
        root
    }
}
