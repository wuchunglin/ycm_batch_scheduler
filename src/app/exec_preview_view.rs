//! 排程設定及執行 → 下一步：排程預覽（依日期頁籤檢視）、開始執行與執行主控台

use super::helpers::parse_hhmm;
use super::*;
use chrono::{Datelike, NaiveDate};

/// 預覽頁一列參數（不顯示欄位 ID）
struct ParamPreviewRow {
    field_id: i64,
    label: String,
    /// 「批次程式新增」下拉式選單選的預設輸入值名稱
    default_name: String,
    resolved: String,
    configured: bool,
}

const WEEK_NAMES: [&str; 7] = ["一", "二", "三", "四", "五", "六", "日"];

fn weekday_name(d: &NaiveDate) -> &'static str {
    WEEK_NAMES[d.weekday().num_days_from_monday() as usize]
}

/// 日期的顯示文字，如「115/03/15(日)」
fn date_label(roc_date: &str) -> String {
    match RocDateUtils::parse_roc_date(roc_date) {
        Some(d) => format!("{}({})", roc_date, weekday_name(&d)),
        None => roc_date.to_string(),
    }
}

/// 頁籤小按鈕（總覽 / 日期 / 日誌）
fn tab_chip(
    id: SharedString,
    label: String,
    active: bool,
    accent: Option<gpui::Hsla>,
    on_click: impl Fn(&ClickEvent, &mut Window, &mut gpui::App) + 'static,
) -> impl IntoElement {
    div()
        .id(id)
        .flex_shrink_0()
        .whitespace_nowrap()
        .px_3()
        .py_1()
        .rounded_md()
        .text_size(px(14.))
        .border_1()
        .border_color(if active { PRIMARY } else { accent.unwrap_or(BORDER) })
        .bg(if active { PRIMARY } else { BACKGROUND })
        .text_color(if active {
            hsl(0.0, 0.0, 100.0)
        } else {
            accent.unwrap_or(FOREGROUND)
        })
        .cursor_pointer()
        .on_click(on_click)
        .child(label)
}

impl BatchSchedulerApp {
    #[allow(dead_code)]
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
    pub(super) fn exec_generated_dates(&self, cx: &Context<Self>) -> Vec<GeneratedScheduleItem> {
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
                    field_id: fid,
                    label: f.label,
                    default_name: d.name.clone(),
                    resolved: resolver.resolve(&d.rule_code, &d.rule_value),
                    configured: true,
                },
                None => ParamPreviewRow {
                    field_id: fid,
                    label: f.label,
                    default_name: "（未設定）".into(),
                    resolved: String::new(),
                    configured: false,
                },
            });
        }
        rows
    }

    fn entry_time_text(&self, e: &ExecEntry, cx: &Context<Self>) -> String {
        let t = e.time_input.read(cx).value().trim().to_string();
        if t.is_empty() { "即時".to_string() } else { t }
    }

    /// 「開始執行」是否可按：回傳 (可開始, 不可開始的原因；可開始時為空字串)
    fn exec_start_gate(&self, dates_len: usize, cx: &Context<Self>) -> (bool, String) {
        if self.runner_state.is_running {
            return (false, "執行中".to_string());
        }
        if self.exec_entries.is_empty() {
            return (false, "執行清單是空的".to_string());
        }
        if dates_len == 0 {
            return (false, "依目前條件沒有產生任何排程日期".to_string());
        }
        for e in &self.exec_entries {
            let Some(p) = self.programs.iter().find(|p| p.id == Some(e.program_id)) else {
                return (false, "執行清單內有已刪除的程式".to_string());
            };
            if self.program_pending(e.program_id) {
                return (false, format!("「{}」有欄位尚未設定預設輸入值", p.job_code));
            }
            let t = e.time_input.read(cx).value().trim().to_string();
            if !t.is_empty() && parse_hhmm(&t).is_none() {
                return (false, format!("「{}」預計執行時間格式錯誤（HH:MM）", p.job_code));
            }
        }
        let unreviewed = (0..dates_len).filter(|i| !self.exec_reviewed.contains(i)).count();
        if unreviewed > 0 {
            return (false, format!("尚有 {unreviewed} 天未檢視"));
        }
        (true, String::new())
    }

    /// 依目前選取的日期重建各執行項目的可修改參數輸入框（已修改過的值會帶回）
    pub(super) fn rebuild_exec_param_rows(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let dates = self.exec_generated_dates(cx);
        let sel = self.exec_preview_date_idx.min(dates.len().saturating_sub(1));
        let (date_key, base_date) = match dates.get(sel) {
            Some(d) => (
                d.roc_date_raw.clone(),
                RocDateUtils::parse_roc_date(&d.roc_date_raw)
                    .unwrap_or_else(|| Local::now().date_naive()),
            ),
            None => (String::new(), Local::now().date_naive()),
        };
        let resolver = ParamResolver::new(base_date).with_custom_vars(
            self.custom_variables
                .iter()
                .map(|v| (v.var_key.clone(), v.var_value.clone()))
                .collect(),
        );
        let mut all: Vec<Vec<ExecParamRow>> = Vec::new();
        for e in self.exec_entries.iter() {
            let mut rows = Vec::new();
            for p in self.exec_entry_params(e, &resolver) {
                let key = (date_key.clone(), e.program_id, p.field_id);
                let init = self
                    .exec_param_overrides
                    .get(&key)
                    .cloned()
                    .unwrap_or_else(|| p.resolved.clone());
                let input = cx.new(|cx| InputState::new(window, cx).default_value(init));
                rows.push(ExecParamRow {
                    field_id: p.field_id,
                    label: p.label,
                    default_name: p.default_name,
                    configured: p.configured,
                    resolved: p.resolved,
                    input,
                });
            }
            all.push(rows);
        }
        self.exec_param_rows = all;
        self.exec_param_date = date_key;
    }

    /// 把目前輸入框的內容與原轉換值比對，不同者記為該日期的手動修改
    pub(super) fn harvest_exec_param_overrides(&mut self, cx: &mut Context<Self>) {
        if self.exec_param_date.is_empty() {
            return;
        }
        let date = self.exec_param_date.clone();
        for (e, rows) in self.exec_entries.iter().zip(self.exec_param_rows.iter()) {
            for r in rows {
                if !r.configured {
                    continue;
                }
                let v = r.input.read(cx).value().to_string();
                let key = (date.clone(), e.program_id, r.field_id);
                if v == r.resolved {
                    self.exec_param_overrides.remove(&key);
                } else {
                    self.exec_param_overrides.insert(key, v);
                }
            }
        }
    }

    pub(super) fn exec_select_date(&mut self, idx: usize, cx: &mut Context<Self>) {
        self.harvest_exec_param_overrides(cx);
        self.exec_param_dirty = true;
        self.exec_preview_overview = false;
        self.exec_preview_date_idx = idx;
        self.exec_reviewed.insert(idx);
        cx.notify();
    }

    /// 頁籤左右箭頭：切到前一天 / 後一天（切過即算已檢視）
    fn exec_step_date(&mut self, delta: i32, cx: &mut Context<Self>) {
        let n = self.exec_generated_dates(cx).len();
        if n == 0 {
            return;
        }
        let cur = if self.exec_preview_overview {
            if delta > 0 { -1 } else { n as i32 }
        } else {
            self.exec_preview_date_idx as i32
        };
        let next = (cur + delta).clamp(0, n as i32 - 1) as usize;
        self.exec_select_date(next, cx);
    }

    /// 按「開始執行」：用「執行清單 × 排程日期」組出執行要求，沿用原有執行引擎，並切到主控台
    fn start_exec_from_preview(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.runner_state.is_running {
            return;
        }
        self.harvest_exec_param_overrides(cx);
        let dates = self.exec_generated_dates(cx);
        let (ok, msg) = self.exec_start_gate(dates.len(), cx);
        if !ok {
            window.push_notification(Notification::warning(msg), cx);
            return;
        }

        let (steps, custom_vars) = {
            let guard = self.conn.lock().unwrap();
            let repo = Repository::new(&guard);
            let mut steps = Vec::new();
            for (idx, e) in self.exec_entries.iter().enumerate() {
                let Ok(Some(program)) = repo.get_program_by_id(e.program_id) else {
                    continue;
                };
                let fields = repo.list_program_fields(e.program_id).unwrap_or_default();
                let defaults = repo.field_defaults_for_program(e.program_id).unwrap_or_default();
                let params: Vec<GroupItemParamDetail> = fields
                    .into_iter()
                    .map(|f| {
                        let fid = f.id.unwrap_or(0);
                        let dv = defaults
                            .get(&fid)
                            .and_then(|did| self.default_values.iter().find(|d| d.id == Some(*did)));
                        GroupItemParamDetail {
                            param_id: None,
                            group_item_id: 0,
                            field_id: fid,
                            field_scope: f.field_scope,
                            html_id: f.html_id,
                            html_name: f.html_name,
                            label: f.label,
                            tag_type: f.tag_type,
                            is_required: f.is_required,
                            rule_code: dv.map(|d| d.rule_code.clone()).unwrap_or_default(),
                            rule_value: dv.map(|d| d.rule_value.clone()).unwrap_or_default(),
                            is_configured: dv.is_some(),
                        }
                    })
                    .collect();
                let raw_time = e.time_input.read(cx).value().trim().to_string();
                let time = if raw_time.is_empty() {
                    String::new()
                } else {
                    parse_hhmm(&raw_time).unwrap_or(raw_time)
                };
                steps.push(ExecutionStepInput {
                    item: GroupItem {
                        id: None,
                        group_id: 0,
                        program_id: e.program_id,
                        step_order: idx as i32 + 1,
                        expect_exe_date_rule: "GROUP_DATE".to_string(),
                        expect_exe_time: time,
                        req_op_confirm: false,
                        link_prev_sn: e.link_prev,
                        pre_job_sn1: String::new(),
                        pre_job_sn2: String::new(),
                        pre_job_sn3: String::new(),
                        rework_sn: String::new(),
                        req_email: e.req_email,
                        mail_recipients: String::new(),
                        pre_work_note: String::new(),
                        rear_work_note: String::new(),
                    },
                    program,
                    params,
                });
            }
            (steps, repo.get_variables_map().unwrap_or_default())
        };
        if steps.is_empty() {
            window.push_notification(Notification::error("執行清單內沒有可執行的批次程式"), cx);
            return;
        }

        let config = self.build_runner_config(cx);
        let mode_txt = if config.dry_run { "預演" } else { "正式執行" };
        let req = ExecutionRequest {
            config,
            group: JobGroup {
                id: None,
                group_name: "排程執行（執行清單）".to_string(),
                description: String::new(),
                group_mode: "MULTI".to_string(),
                schedule_rule_json: String::new(),
                created_at: None,
            },
            steps,
            schedule_dates: dates.clone(),
            custom_vars,
            param_overrides: self.exec_param_overrides.clone(),
        };

        // 排程 log：開始時先記錄整份排程（日期、程式、時間），之後每步由執行引擎寫入同一個 log 檔
        let prog_codes: Vec<String> = req
            .steps
            .iter()
            .map(|s| format!("{}@{}", s.program.job_code, if s.item.expect_exe_time.is_empty() { "即時" } else { s.item.expect_exe_time.as_str() }))
            .collect();
        append_file_log(
            "INFO",
            &format!(
                "[排程] 按下開始執行（{mode_txt}）：{} 個日期 × {} 支程式｜程式：{}｜日期：{}",
                req.schedule_dates.len(),
                req.steps.len(),
                prog_codes.join("、"),
                req.schedule_dates
                    .iter()
                    .map(|d| d.roc_date.clone())
                    .collect::<Vec<_>>()
                    .join("、")
            ),
        );

        self.exec_run_dates = dates.iter().map(|d| date_label(&d.roc_date)).collect();
        self.exec_console_tab = None;
        self.exec_console_mode = true;
        self.exec_cal_open = None;
        self.launch_execution(req, cx);
    }

    /// 排程預覽頁（依日期頁籤檢視）；按「開始執行」後改顯示執行主控台
    pub(super) fn render_exec_preview_view(&self, cx: &mut Context<Self>) -> gpui::AnyElement {
        if self.exec_console_mode {
            return self.render_exec_console(cx);
        }
        let dates = self.exec_generated_dates(cx);
        let n = dates.len();
        let sel_idx = self.exec_preview_date_idx.min(n.saturating_sub(1));
        let overview = self.exec_preview_overview && n > 0;
        let reviewed_cnt = (0..n).filter(|i| self.exec_reviewed.contains(i)).count();
        let (can_start, gate_msg) = self.exec_start_gate(n, cx);
        // 排程日期：只顯示展開後的日期（不帶年度、每月/每周/指定日期等選擇項目）
        let dates_txt = {
            const MAX_SHOW: usize = 10;
            let shown: Vec<&str> = dates.iter().take(MAX_SHOW).map(|d| d.roc_date.as_str()).collect();
            if shown.is_empty() {
                "（無）".to_string()
            } else if n > MAX_SHOW {
                format!("{}…（共 {n} 天）", shown.join("、"))
            } else {
                shown.join("、")
            }
        };

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

        // ---- 固定在上方的標題列：上一步 / 預演 / 開始執行 ----
        let status_txt = if can_start {
            format!("已檢視 {reviewed_cnt}/{n} 天，可開始執行")
        } else if gate_msg.is_empty() {
            format!("已檢視 {reviewed_cnt}/{n} 天")
        } else if n > 0 && reviewed_cnt < n {
            format!("已檢視 {reviewed_cnt}/{n} 天｜{gate_msg}")
        } else {
            gate_msg.clone()
        };
        let header = h_flex()
            .w_full()
            .flex_none()
            .flex_wrap()
            .items_center()
            .justify_between()
            .gap_3()
            .pb_3()
            .child(
                v_flex()
                    .gap_1()
                    .child(div().text_xl().font_weight(FontWeight::BOLD).child("排程預覽"))
                    .child(
                        div()
                            .text_sm()
                            .text_color(MUTED_FOREGROUND)
                            .child("逐一點開每個日期頁籤檢視，全部檢視後才能「開始執行」"),
                    ),
            )
            .child(
                h_flex()
                    .flex_wrap()
                    .items_center()
                    .gap_3()
                    .child(
                        div()
                            .text_size(px(14.))
                            .text_color(if can_start { SUCCESS } else { WARNING })
                            .child(status_txt),
                    )
                    .child(
                        Checkbox::new("ep-dry-run")
                            .label("預演模式")
                            .checked(self.runner_dry_run)
                            .on_click(cx.listener(|this, chk, _, cx| {
                                this.runner_dry_run = *chk;
                                cx.notify();
                            })),
                    )
                    .child(
                        Button::new("ep-back")
                            .label("上一步")
                            .on_click(cx.listener(|this, _: &ClickEvent, _, cx| {
                                this.harvest_exec_param_overrides(cx);
                                this.active_tab = NavTab::ScheduleExec;
                                cx.notify();
                            })),
                    )
                    .child(
                        Button::new("ep-start")
                            .primary()
                            .label("開始執行")
                            .disabled(!can_start)
                            .on_click(cx.listener(|this, _: &ClickEvent, window, cx| {
                                this.start_exec_from_preview(window, cx);
                            })),
                    ),
            );

        // ---- 排程條件（一行） ----
        let cond = card().child(
            h_flex()
                .w_full()
                .flex_wrap()
                .gap_4()
                .text_size(px(14.))
                .child(div().child(format!("排程日期：{dates_txt}")))
                .child(
                    div()
                        .text_color(MUTED_FOREGROUND)
                        .child(format!("共 {n} 天 × {} 支程式", self.exec_entries.len())),
                ),
        );

        // ---- 頁籤列：總覽 + 每個日期一個頁籤（可左右捲動，◀ ▶ 切換前後一天） ----
        let mut tabs = h_flex().gap_1().items_center();
        tabs = tabs.child(tab_chip(
            SharedString::from("ep-tab-ov"),
            "總覽".to_string(),
            overview,
            None,
            cx.listener(|this, _: &ClickEvent, _, cx| {
                this.harvest_exec_param_overrides(cx);
                this.exec_param_dirty = true;
                this.exec_preview_overview = true;
                cx.notify();
            }),
        ));
        for (i, d) in dates.iter().enumerate() {
            let reviewed = self.exec_reviewed.contains(&i);
            tabs = tabs.child(tab_chip(
                SharedString::from(format!("ep-tab-{i}")),
                format!("{}{}", if reviewed { "✓ " } else { "" }, date_label(&d.roc_date)),
                !overview && i == sel_idx,
                if reviewed { Some(SUCCESS) } else { None },
                cx.listener(move |this, _: &ClickEvent, _, cx| {
                    this.exec_select_date(i, cx);
                }),
            ));
        }
        let tab_row = h_flex()
            .w_full()
            .items_center()
            .gap_1()
            .child(
                Button::new("ep-prev")
                    .ghost()
                    .label("◀")
                    .on_click(cx.listener(|this, _: &ClickEvent, _, cx| this.exec_step_date(-1, cx))),
            )
            .child(
                div()
                    .id("ep-tabs-scroll")
                    .flex_1()
                    .min_w_0()
                    .overflow_x_scroll()
                    .child(tabs),
            )
            .child(
                Button::new("ep-next")
                    .ghost()
                    .label("▶")
                    .on_click(cx.listener(|this, _: &ClickEvent, _, cx| this.exec_step_date(1, cx))),
            );

        let mut body = v_flex().w_full().gap_3().child(cond).child(tab_row);

        if self.exec_entries.is_empty() {
            body = body.child(
                card().child(
                    div()
                        .text_size(px(14.))
                        .text_color(MUTED_FOREGROUND)
                        .child("執行清單是空的，請按「上一步」回去勾選"),
                ),
            );
        } else if overview {
            // ---- 總覽：所有日期一覽，點一列跳到該日期頁籤 ----
            let times: Vec<String> = {
                let mut v: Vec<String> = self
                    .exec_entries
                    .iter()
                    .map(|e| e.time_input.read(cx).value().trim().to_string())
                    .filter(|t| !t.is_empty())
                    .collect();
                v.sort();
                v
            };
            let time_range = match (times.first(), times.last()) {
                (Some(a), Some(b)) if a != b => format!("{a} ~ {b}"),
                (Some(a), _) => a.clone(),
                _ => "即時".to_string(),
            };
            let mut tbl = v_flex().w_full().gap_1().child(
                h_flex()
                    .w_full()
                    .px_2()
                    .py_1()
                    .gap_2()
                    .text_size(px(14.))
                    .font_weight(FontWeight::BOLD)
                    .text_color(MUTED_FOREGROUND)
                    .child(div().w(px(40.)).flex_shrink_0().child("#"))
                    .child(div().flex_1().min_w(px(140.)).child("執行日期"))
                    .child(div().w(px(90.)).flex_shrink_0().child("程式數"))
                    .child(div().w(px(150.)).flex_shrink_0().child("預計執行時間"))
                    .child(div().w(px(80.)).flex_shrink_0().child("檢視")),
            );
            for (i, d) in dates.iter().enumerate() {
                let reviewed = self.exec_reviewed.contains(&i);
                tbl = tbl.child(
                    h_flex()
                        .id(SharedString::from(format!("ep-ov-{i}")))
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
                        .cursor_pointer()
                        .hover(|s| s.bg(SECONDARY))
                        .on_click(cx.listener(move |this, _: &ClickEvent, _, cx| {
                            this.exec_select_date(i, cx);
                        }))
                        .child(div().w(px(40.)).flex_shrink_0().text_color(MUTED_FOREGROUND).child(format!("{}", i + 1)))
                        .child(div().flex_1().min_w(px(140.)).child(date_label(&d.roc_date)))
                        .child(div().w(px(90.)).flex_shrink_0().child(format!("{} 支", self.exec_entries.len())))
                        .child(div().w(px(150.)).flex_shrink_0().child(time_range.clone()))
                        .child(
                            div()
                                .w(px(80.))
                                .flex_shrink_0()
                                .text_color(if reviewed { SUCCESS } else { WARNING })
                                .child(if reviewed { "✓ 已檢視" } else { "未檢視" }),
                        ),
                );
            }
            body = body.child(
                card()
                    .child(
                        div()
                            .font_weight(FontWeight::SEMIBOLD)
                            .child(format!("排程總覽（{n} 天）")),
                    )
                    .child(tbl),
            );
        } else {
            // ---- 單一日期頁籤：該日每支程式的執行時間與轉換後參數 ----
            let base_date = dates
                .get(sel_idx)
                .and_then(|d| RocDateUtils::parse_roc_date(&d.roc_date))
                .unwrap_or_else(|| Local::now().date_naive());
            let base_txt = RocDateUtils::to_roc_yyymmdd(&base_date, "/");
            body = body.child(
                h_flex()
                    .w_full()
                    .items_center()
                    .gap_3()
                    .child(
                        div()
                            .text_size(px(16.))
                            .font_weight(FontWeight::BOLD)
                            .child(format!("{}（{}）", date_label(&base_txt), format!("共 {} 支程式", self.exec_entries.len()))),
                    )
                    .child(
                        div()
                            .text_size(px(14.))
                            .text_color(MUTED_FOREGROUND)
                            .child(if n == 0 {
                                "沒有排程日期，參數以今天為基準日轉換".to_string()
                            } else {
                                format!("參數以 {base_txt} 為基準日轉換")
                            }),
                    ),
            );

            for (idx, e) in self.exec_entries.iter().enumerate() {
                let (code, name) = prog_map
                    .get(&e.program_id)
                    .map(|p| (p.job_code.clone(), p.job_name.clone()))
                    .unwrap_or_else(|| ("?".to_string(), "（程式已刪除）".to_string()));
                let time = self.entry_time_text(e, cx);
                let params: &[ExecParamRow] = self
                    .exec_param_rows
                    .get(idx)
                    .map(|v| v.as_slice())
                    .unwrap_or(&[]);

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
                        .child(div().w(px(280.)).flex_shrink_0().child("預設輸入值"))
                        .child(div().flex_1().min_w(px(120.)).child("轉換後參數值（可直接修改）"))
                        .child(div().w(px(60.)).flex_shrink_0()),
                );
                if params.is_empty() {
                    // 無參數：顯示「無參數」，轉換後參數值空白
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
                            .child(
                                div()
                                    .flex_1()
                                    .min_w(px(120.))
                                    .text_color(MUTED_FOREGROUND)
                                    .child("無參數"),
                            )
                            .child(div().w(px(280.)).flex_shrink_0())
                            .child(div().flex_1().min_w(px(120.)))
                            .child(div().w(px(60.)).flex_shrink_0()),
                    );
                }
                for (ri, p) in params.iter().enumerate() {
                    let modified = p.configured && p.input.read(cx).value().to_string() != p.resolved;
                    let reset_input = p.input.clone();
                    let reset_to = p.resolved.clone();
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
                            .child(div().flex_1().min_w(px(120.)).child(p.label.clone()))
                            .child(
                                div()
                                    .w(px(280.))
                                    .flex_shrink_0()
                                    .text_color(if p.configured { FOREGROUND } else { DANGER })
                                    .child(p.default_name.clone()),
                            )
                            .child(if p.configured {
                                div()
                                    .flex_1()
                                    .min_w(px(120.))
                                    .child(Input::new(&p.input))
                                    .into_any_element()
                            } else {
                                div()
                                    .flex_1()
                                    .min_w(px(120.))
                                    .text_color(DANGER)
                                    .child("（尚未設定）")
                                    .into_any_element()
                            })
                            .child(
                                div().w(px(60.)).flex_shrink_0().child(if modified {
                                    Button::new(SharedString::from(format!("ep-reset-{idx}-{ri}")))
                                        .ghost()
                                        .label("還原")
                                        .on_click(cx.listener(move |_, _: &ClickEvent, window, cx| {
                                            reset_input.update(cx, |s, cx| {
                                                s.set_value(reset_to.clone(), window, cx)
                                            });
                                            cx.notify();
                                        }))
                                        .into_any_element()
                                } else {
                                    div().into_any_element()
                                }),
                            ),
                    );
                }

                body = body.child(
                    card()
                        .child(
                            h_flex()
                                .w_full()
                                .flex_wrap()
                                .items_center()
                                .gap_3()
                                .text_size(px(14.))
                                .child(div().text_color(MUTED_FOREGROUND).child(format!("{}", idx + 1)))
                                .child(div().font_weight(FontWeight::BOLD).child(format!("{}　{}", code, name)))
                                .child(Tag::secondary().child(time))
                                .child(div().text_color(MUTED_FOREGROUND).child(format!(
                                    "前置工作序號：{}　｜　發送mail通知：{}",
                                    if e.link_prev { "是" } else { "否" },
                                    if e.req_email { "是" } else { "否" },
                                ))),
                        )
                        .child(table),
                );
            }
        }

        v_flex()
            .size_full()
            .min_h_0()
            .child(header)
            .child(
                div()
                    .id("exec-preview-body")
                    .flex_1()
                    .min_h_0()
                    .w_full()
                    .overflow_y_scroll()
                    .child(body),
            )
            .into_any_element()
    }

    /// 執行主控台：進度、依日期頁籤檢視日誌；執行中鎖住所有頁籤，完成後才能返回
    fn render_exec_console(&self, cx: &mut Context<Self>) -> gpui::AnyElement {
        let rs = &self.runner_state;
        let (curr, total) = rs.progress;
        let ratio = if total > 0 { (curr as f32 / total as f32).clamp(0.0, 1.0) } else { 0.0 };
        let running = rs.is_running;
        let failed = rs.failed;
        let per = rs.steps_per_date.max(1);
        let done_dates = curr / per;
        let n_dates = self.exec_run_dates.len();

        let cancelled = rs.cancelled;
        let (title, title_color) = if running {
            (if self.interrupt_requested { "中斷中" } else { "排程執行中" }, WARNING)
        } else if cancelled {
            ("已中斷", WARNING)
        } else if failed {
            ("執行失敗", DANGER)
        } else {
            ("執行完成", SUCCESS)
        };

        let header = h_flex()
            .w_full()
            .flex_none()
            .flex_wrap()
            .items_center()
            .justify_between()
            .gap_3()
            .pb_3()
            .child(
                v_flex()
                    .gap_1()
                    .child(
                        h_flex()
                            .gap_2()
                            .items_center()
                            .child(
                                div()
                                    .text_xl()
                                    .font_weight(FontWeight::BOLD)
                                    .text_color(title_color)
                                    .child(title),
                            )
                            .child(Tag::secondary().child(rs.current_step.clone())),
                    )
                    .child(
                        div()
                            .text_sm()
                            .text_color(MUTED_FOREGROUND)
                            .child(if running {
                                "執行中，已鎖住所有頁籤，請等待執行完成，或按「中斷」停止（請勿關閉程式）"
                            } else if cancelled {
                                "已依要求中斷，後續排程未執行，可返回排程設定"
                            } else {
                                "執行已結束，可返回排程設定"
                            }),
                    ),
            )
            .child(
                h_flex()
                    .gap_2()
                    .items_center()
                    .child(if running {
                        Button::new("ec-interrupt")
                            .danger()
                            .label(if self.interrupt_requested { "中斷中..." } else { "中斷" })
                            .disabled(self.interrupt_requested)
                            .on_click(cx.listener(|this, _: &ClickEvent, _, cx| {
                                // 先跳出確認視窗，確認後才真的中斷
                                this.interrupt_prompt = true;
                                cx.notify();
                            }))
                            .into_any_element()
                    } else {
                        div().into_any_element()
                    })
                    .child(
                        Button::new("ec-back")
                            .primary()
                            .label("返回排程設定")
                            .disabled(running)
                            .on_click(cx.listener(|this, _: &ClickEvent, _, cx| {
                                if this.runner_state.is_running {
                                    return;
                                }
                                this.exec_console_mode = false;
                                this.active_tab = NavTab::ScheduleExec;
                                cx.notify();
                            })),
                    ),
            );

        let progress = v_flex()
            .w_full()
            .bg(CARD)
            .p_4()
            .rounded_lg()
            .border_1()
            .border_color(BORDER)
            .gap_2()
            .child(
                h_flex()
                    .w_full()
                    .justify_between()
                    .child(div().font_weight(FontWeight::SEMIBOLD).child("執行進度"))
                    .child(div().text_size(px(14.)).child(format!("{curr}/{total} 完成"))),
            )
            .child(
                div().w_full().h(px(10.)).bg(BACKGROUND).rounded_full().child(
                    div()
                        .h_full()
                        .w(gpui::relative(if curr > 0 { ratio.max(0.02) } else { 0.0 }))
                        .bg(if failed { DANGER } else { SUCCESS })
                        .rounded_full(),
                ),
            );

        // 日誌頁籤：全部日誌 + 每個日期（⏳ 執行中 / ✓ 完成 / ✗ 失敗）
        let mut tabs = h_flex().gap_1().items_center();
        tabs = tabs.child(tab_chip(
            SharedString::from("ec-tab-all"),
            "全部日誌".to_string(),
            self.exec_console_tab.is_none(),
            None,
            cx.listener(|this, _: &ClickEvent, _, cx| {
                this.exec_console_tab = None;
                cx.notify();
            }),
        ));
        for (i, label) in self.exec_run_dates.iter().enumerate() {
            let (icon, color) = if i < done_dates {
                ("✓", Some(SUCCESS))
            } else if i == done_dates && cancelled {
                ("■", Some(WARNING))
            } else if i == done_dates && failed {
                ("✗", Some(DANGER))
            } else if i == done_dates && running {
                ("⏳", Some(WARNING))
            } else {
                ("·", None)
            };
            tabs = tabs.child(tab_chip(
                SharedString::from(format!("ec-tab-{i}")),
                format!("{icon} {label}"),
                self.exec_console_tab == Some(i),
                color,
                cx.listener(move |this, _: &ClickEvent, _, cx| {
                    this.exec_console_tab = Some(i);
                    cx.notify();
                }),
            ));
        }

        let mut logs = v_flex().w_full().gap_1();
        let mut shown = 0usize;
        for (line, di) in rs.logs.iter().zip(rs.log_dates.iter()) {
            if let Some(sel) = self.exec_console_tab {
                if (*di).min(n_dates.saturating_sub(1)) != sel {
                    continue;
                }
            }
            shown += 1;
            let color = if line.contains("ERROR") || line.contains("FAILED") {
                DANGER
            } else if line.contains("SUCCESS") || line.contains("DONE") {
                SUCCESS
            } else if line.contains("SCREENSHOT") || line.contains("WARN") {
                WARNING
            } else {
                FOREGROUND
            };
            logs = logs.child(
                div()
                    .text_xs()
                    .font_family(SharedString::from("Consolas, Courier New, monospace"))
                    .text_color(color)
                    .child(line.clone()),
            );
        }
        if shown == 0 {
            logs = logs.child(
                div()
                    .text_size(px(14.))
                    .text_color(MUTED_FOREGROUND)
                    .child("這個日期還沒有日誌"),
            );
        }

        let log_card = v_flex()
            .w_full()
            .bg(BACKGROUND)
            .p_4()
            .rounded_lg()
            .border_1()
            .border_color(BORDER)
            .gap_2()
            .child(
                h_flex()
                    .w_full()
                    .flex_wrap()
                    .justify_between()
                    .gap_2()
                    .child(
                        div()
                            .text_xs()
                            .font_weight(FontWeight::BOLD)
                            .text_color(MUTED_FOREGROUND)
                            .child("執行主控台（即時日誌）"),
                    )
                    .child(
                        div()
                            .text_xs()
                            .text_color(MUTED_FOREGROUND)
                            .child(format!("排程 log 檔：{}", current_log_file_path())),
                    ),
            )
            .child(
                div()
                    .id("ec-log-scroll")
                    .w_full()
                    .h(px(380.))
                    .overflow_y_scroll()
                    .child(logs),
            );

        let mut sn_row = h_flex()
            .w_full()
            .gap_2()
            .flex_wrap()
            .p_3()
            .bg(CARD)
            .rounded_lg()
            .border_1()
            .border_color(BORDER);
        for item in &rs.captured_sns {
            sn_row = sn_row.child(Tag::success().child(format!("{} #{}", item.job_code, item.sn)));
        }

        let mut body = v_flex().w_full().gap_3().child(progress);
        if !rs.captured_sns.is_empty() {
            body = body.child(sn_row);
        }
        body = body
            .child(div().w_full().child(div().id("ec-tabs-scroll").w_full().overflow_x_scroll().child(tabs)))
            .child(log_card);

        v_flex()
            .size_full()
            .min_h_0()
            .child(header)
            .child(
                div()
                    .id("exec-console-body")
                    .flex_1()
                    .min_h_0()
                    .w_full()
                    .overflow_y_scroll()
                    .child(body),
            )
            .into_any_element()
    }
}
