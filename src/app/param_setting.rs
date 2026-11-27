//! 參數設定：維護「批次程式新增」欄位預設輸入值下拉選單的選項（新增 / 修改 / 刪除）。
//! 下拉選單的選項全部讀取資料表 default_values，程式內建清單只在第一次建立資料庫時寫入。
//! 日期類選項輸出民國年或西元年，由「批次程式新增」的每個欄位各自設定（這裡不設定年度格式）。

use super::helpers::{drag_handle, drag_over_style, labeled_input, mode_pill, DragList, DragRow};
use super::*;

/// (規則類型, 顯示名稱, 規則值填寫說明)
const PD_RULES: &[(&str, &str, &str)] = &[
    ("STATIC", "固定文字", "固定輸入的文字；留空 = 空白"),
    (
        "CUSTOM_TEMPLATE",
        "樣板",
        "可混用文字與 {E1} {執行日前1天} 等，如 DAVDF222.{E1}",
    ),
    ("DAY_OFFSET", "日期偏移", "與執行日相差的天數，如 +8 或 -3（輸出年月日，年度格式依欄位設定）"),
    ("MONTH_OFFSET", "月份偏移", "與執行日相差的月數，如 -1（輸出年月，年度格式依欄位設定）；-1:MM 只輸出月份"),
    (
        "MONTH_EDGE",
        "月初月底",
        "MONTH_FIRST / MONTH_LAST / PREV_MONTH_FIRST / PREV_MONTH_LAST",
    ),
    (
        "DATE_EDGE",
        "期間邊界",
        "PREV_QUARTER_FIRST / PREV_QUARTER_LAST / PREV_HALF_FIRST / PREV_HALF_LAST / YEAR_FIRST / YEAR_LAST，可加 :-4 平移年度",
    ),
    (
        "LAST_YEAR_MONTH",
        "去年月份",
        "月份 1~12，如 3 → 執行日去年3月（民國年 11503、西元年 202503；年度格式依欄位設定）",
    ),
    (
        "PREV_YEAR",
        "以前年度",
        "往前幾年：1 = 去年度、2 = 前年度…，0 = 本年度（執行日 115 年填 2 → 民國 113、西元 2024；年度格式依欄位設定）",
    ),
    ("WEEK_DAY", "本週星期", "1~7（週一~週日），如 5；5:/ 可指定分隔符"),
    ("CODE", "內建代碼", "E1~E8（執行日往前0~7天）、E9/EA/EB/EC（月份）、ED/EE/EF/EG（年月）；年度格式依欄位設定"),
];

const E_CODES: &[&str] = &[
    "E1", "E2", "E3", "E4", "E5", "E6", "E7", "E8", "E9", "EA", "EB", "EC", "ED", "EE", "EF", "EG",
];

/// 資料表的 rule_code → 設定畫面的規則類型
fn rule_key_of(rule_code: &str) -> &'static str {
    for (k, _, _) in PD_RULES {
        if *k != "CODE" && *k == rule_code {
            return *k;
        }
    }
    "CODE"
}

fn rule_label(key: &str) -> &'static str {
    PD_RULES
        .iter()
        .find(|(k, _, _)| *k == key)
        .map(|(_, l, _)| *l)
        .unwrap_or("其他")
}

fn rule_hint(key: &str) -> &'static str {
    PD_RULES
        .iter()
        .find(|(k, _, _)| *k == key)
        .map(|(_, _, h)| *h)
        .unwrap_or("")
}

/// 檢查規則值並轉成資料表的 (rule_code, rule_value)
fn validate_rule(key: &str, value: &str) -> Result<(String, String), String> {
    let v = value.trim();
    match key {
        "STATIC" => Ok(("STATIC".to_string(), v.to_string())),
        "CUSTOM_TEMPLATE" => {
            if v.is_empty() {
                Err("樣板不可空白".to_string())
            } else {
                Ok(("CUSTOM_TEMPLATE".to_string(), v.to_string()))
            }
        }
        "DAY_OFFSET" => {
            if v.trim_start_matches('+').parse::<i64>().is_ok() {
                Ok(("DAY_OFFSET".to_string(), v.to_string()))
            } else {
                Err("日期偏移請填整數天數，如 +8 或 -3".to_string())
            }
        }
        "MONTH_OFFSET" => {
            let first = v.split(':').next().unwrap_or("").trim_start_matches('+');
            if first.parse::<i32>().is_ok() {
                Ok(("MONTH_OFFSET".to_string(), v.to_string()))
            } else {
                Err("月份偏移請填整數月數，如 -1 或 -1:MM".to_string())
            }
        }
        "MONTH_EDGE" => {
            let u = v.to_uppercase();
            if ["MONTH_FIRST", "MONTH_LAST", "PREV_MONTH_FIRST", "PREV_MONTH_LAST"]
                .contains(&u.as_str())
            {
                Ok(("MONTH_EDGE".to_string(), u))
            } else {
                Err("月初月底請填 MONTH_FIRST / MONTH_LAST / PREV_MONTH_FIRST / PREV_MONTH_LAST".to_string())
            }
        }
        "DATE_EDGE" => {
            let u = v.to_uppercase();
            let mut parts = u.splitn(2, ':');
            let k = parts.next().unwrap_or("");
            let off_ok = parts
                .next()
                .map(|s| s.trim().trim_start_matches('+').parse::<i32>().is_ok())
                .unwrap_or(true);
            if [
                "PREV_QUARTER_FIRST",
                "PREV_QUARTER_LAST",
                "PREV_HALF_FIRST",
                "PREV_HALF_LAST",
                "YEAR_FIRST",
                "YEAR_LAST",
            ]
            .contains(&k)
                && off_ok
            {
                Ok(("DATE_EDGE".to_string(), u))
            } else {
                Err("期間邊界請填 PREV_QUARTER_FIRST 等代碼，可加 :-4 平移年度".to_string())
            }
        }
        "LAST_YEAR_MONTH" => match v.parse::<u32>() {
            Ok(m) if (1..=12).contains(&m) => Ok(("LAST_YEAR_MONTH".to_string(), m.to_string())),
            _ => Err("去年月份請填 1~12".to_string()),
        },
        "PREV_YEAR" => match v.parse::<i32>() {
            Ok(n) if (0..=99).contains(&n) => Ok(("PREV_YEAR".to_string(), n.to_string())),
            _ => Err("以前年度請填 0 以上的整數（1 = 去年度、2 = 前年度）".to_string()),
        },
        "WEEK_DAY" => {
            let n = v.split(':').next().unwrap_or("").trim();
            if n.parse::<u32>().map(|d| (1..=7).contains(&d)).unwrap_or(false) {
                Ok(("WEEK_DAY".to_string(), v.to_string()))
            } else {
                Err("本週星期請填 1~7".to_string())
            }
        }
        _ => {
            let u = v.to_uppercase();
            if E_CODES.contains(&u.as_str()) {
                Ok((u, String::new()))
            } else {
                Err("內建代碼請填 E1~E8、E9、EA~EG".to_string())
            }
        }
    }
}

impl BatchSchedulerApp {
    pub(super) fn open_param_setting(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.open_picker = None;
        self.open_year_picker = None;
        self.reload_default_values(cx);
        self.pd_reset_form(window, cx);
        self.pd_open = true;
        cx.notify();
    }

    /// 只重新讀取下拉選單選項；已被刪除的選項，從尚未儲存的欄位列中清掉
    pub(super) fn reload_default_values(&mut self, cx: &mut Context<Self>) {
        {
            let guard = self.conn.lock().unwrap();
            let repo = Repository::new(&guard);
            self.default_values = repo.list_default_values().unwrap_or_default();
            if let Some(pid) = self.selected_program.as_ref().and_then(|p| p.id) {
                self.field_defaults = repo.field_defaults_for_program(pid).unwrap_or_default();
            }
            self.prog_stats = repo.program_stats().unwrap_or_default();
        }
        let ids: HashSet<i64> = self.default_values.iter().filter_map(|d| d.id).collect();
        for r in self.field_rows.iter_mut() {
            if let Some(id) = r.default_id {
                if !ids.contains(&id) {
                    r.default_id = None;
                }
            }
        }
        self.exec_param_dirty = true;
        cx.notify();
    }

    fn pd_reset_form(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.pd_editing = None;
        self.pd_rule = "STATIC".to_string();
        self.pd_confirm_delete = None;
        self.pd_name_input
            .update(cx, |s, cx| s.set_value("", window, cx));
        self.pd_value_input
            .update(cx, |s, cx| s.set_value("", window, cx));
    }

    fn pd_start_edit(&mut self, d: DefaultValue, window: &mut Window, cx: &mut Context<Self>) {
        let key = rule_key_of(&d.rule_code);
        let value = if key == "CODE" {
            d.rule_code.clone()
        } else {
            d.rule_value.clone()
        };
        self.pd_editing = d.id;
        self.pd_rule = key.to_string();
        self.pd_confirm_delete = None;
        let name = d.name.clone();
        self.pd_name_input
            .update(cx, |s, cx| s.set_value(name, window, cx));
        self.pd_value_input
            .update(cx, |s, cx| s.set_value(value, window, cx));
        cx.notify();
    }

    fn pd_save(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let name = self.pd_name_input.read(cx).value().trim().to_string();
        let value = self.pd_value_input.read(cx).value().trim().to_string();
        if name.is_empty() {
            window.push_notification(Notification::error("請輸入選項名稱"), cx);
            return;
        }
        if name.starts_with("自訂：") {
            window.push_notification(
                Notification::error("選項名稱不可以「自訂：」開頭（保留給欄位的「自行輸入」使用）"),
                cx,
            );
            return;
        }
        if self
            .default_values
            .iter()
            .any(|d| d.name == name && d.id != self.pd_editing)
        {
            window.push_notification(
                Notification::error(format!("已有同名的選項「{name}」，名稱不可重複")),
                cx,
            );
            return;
        }
        let (code, val) = match validate_rule(&self.pd_rule, &value) {
            Ok(v) => v,
            Err(msg) => {
                window.push_notification(Notification::error(msg), cx);
                return;
            }
        };
        let editing = self.pd_editing;
        let res = {
            let guard = self.conn.lock().unwrap();
            let repo = Repository::new(&guard);
            match editing {
                Some(id) => repo.update_default_value(id, &name, &code, &val),
                None => repo.add_default_value(&name, &code, &val).map(|_| ()),
            }
        };
        match res {
            Ok(()) => {
                window.push_notification(
                    Notification::success(if editing.is_some() {
                        format!("已修改選項「{name}」")
                    } else {
                        format!("已新增選項「{name}」")
                    }),
                    cx,
                );
                self.pd_reset_form(window, cx);
                self.reload_default_values(cx);
            }
            Err(e) => {
                window.push_notification(Notification::error(format!("儲存失敗：{e}")), cx);
            }
        }
    }

    /// 拖曳排序：把選項 id 移到 target_id 的位置
    fn pd_move_to(&mut self, id: i64, target_id: i64, cx: &mut Context<Self>) {
        let mut ids: Vec<i64> = self
            .default_values
            .iter()
            .filter(|d| !is_custom_default(d))
            .filter_map(|d| d.id)
            .collect();
        let (Some(from), Some(to)) = (
            ids.iter().position(|i| *i == id),
            ids.iter().position(|i| *i == target_id),
        ) else {
            return;
        };
        if from == to {
            return;
        }
        super::helpers::move_vec_item(&mut ids, from, to);
        {
            let guard = self.conn.lock().unwrap();
            let repo = Repository::new(&guard);
            let _ = repo.reorder_default_values(&ids);
        }
        self.reload_default_values(cx);
    }

    fn pd_request_delete(&mut self, id: i64, cx: &mut Context<Self>) {
        let used = {
            let guard = self.conn.lock().unwrap();
            let repo = Repository::new(&guard);
            repo.default_value_usage(id).unwrap_or(0)
        };
        self.pd_confirm_delete = Some((id, used));
        cx.notify();
    }

    fn pd_delete(&mut self, id: i64, window: &mut Window, cx: &mut Context<Self>) {
        let res = {
            let guard = self.conn.lock().unwrap();
            let repo = Repository::new(&guard);
            repo.delete_default_value(id)
        };
        match res {
            Ok(()) => {
                window.push_notification(Notification::success("已刪除選項"), cx);
                if self.pd_editing == Some(id) {
                    self.pd_reset_form(window, cx);
                }
                self.pd_confirm_delete = None;
                self.reload_default_values(cx);
            }
            Err(e) => {
                window.push_notification(Notification::error(format!("刪除失敗：{e}")), cx);
            }
        }
    }

    pub(super) fn render_param_setting(&self, cx: &mut Context<Self>) -> Option<gpui::AnyElement> {
        if !self.pd_open {
            return None;
        }

        // --- 選項清單 ---
        let mut list = v_flex()
            .id("pd-list")
            .w_full()
            .max_h(px(300.))
            .overflow_y_scroll()
            .gap_1();
        for (idx, d) in self
            .default_values
            .iter()
            .filter(|d| !is_custom_default(d))
            .enumerate()
        {
            let did = d.id.unwrap_or(0);
            let key = rule_key_of(&d.rule_code);
            let summary = if key == "CODE" {
                format!("{}：{}", rule_label(key), d.rule_code)
            } else if d.rule_value.is_empty() {
                format!("{}：（空白）", rule_label(key))
            } else {
                format!("{}：{}", rule_label(key), d.rule_value)
            };
            let confirming = match self.pd_confirm_delete {
                Some((i, used)) if i == did => Some(used),
                _ => None,
            };
            let is_editing = self.pd_editing == d.id;
            let d_edit = d.clone();

            let actions = if let Some(used) = confirming {
                h_flex()
                    .items_center()
                    .gap_2()
                    .child(
                        div()
                            .text_xs()
                            .text_color(DANGER)
                            .child(if used > 0 {
                                format!("{used} 個欄位使用中，刪除後變「待設定」")
                            } else {
                                "確定刪除？".to_string()
                            }),
                    )
                    .child(
                        Button::new(SharedString::from(format!("pd-del-yes-{did}")))
                            .small()
                            .danger()
                            .label("確定刪除")
                            .on_click(cx.listener(move |this, _: &ClickEvent, window, cx| {
                                this.pd_delete(did, window, cx);
                            })),
                    )
                    .child(
                        Button::new(SharedString::from(format!("pd-del-no-{did}")))
                            .small()
                            .ghost()
                            .label("取消")
                            .on_click(cx.listener(|this, _: &ClickEvent, _, cx| {
                                this.pd_confirm_delete = None;
                                cx.notify();
                            })),
                    )
            } else {
                h_flex()
                    .items_center()
                    .gap_2()
                    .child(
                        Button::new(SharedString::from(format!("pd-edit-{did}")))
                            .small()
                            .label("修改")
                            .on_click(cx.listener(move |this, _: &ClickEvent, window, cx| {
                                this.pd_start_edit(d_edit.clone(), window, cx);
                            })),
                    )
                    .child(
                        Button::new(SharedString::from(format!("pd-del-{did}")))
                            .small()
                            .danger()
                            .label("刪除")
                            .on_click(cx.listener(move |this, _: &ClickEvent, _, cx| {
                                this.pd_request_delete(did, cx);
                            })),
                    )
            };

            list = list.child(
                h_flex()
                    .w_full()
                    .items_center()
                    .gap_2()
                    .px_2()
                    .py_1()
                    .rounded_md()
                    .bg(BACKGROUND)
                    .border_1()
                    .border_color(if is_editing { PRIMARY } else { BORDER })
                    .drag_over::<DragRow>(|style, d, _, _| drag_over_style(style, d, DragList::DefaultValue))
                    .on_drop(cx.listener(move |this, d: &DragRow, _, cx| {
                        if d.list == DragList::DefaultValue {
                            this.pd_move_to(d.id, did, cx);
                        }
                    }))
                    .child(drag_handle(
                        SharedString::from(format!("pd-drag-{did}")),
                        DragList::DefaultValue,
                        did,
                        d.name.clone(),
                    ))
                    .child(
                        div()
                            .flex_1()
                            .min_w_0()
                            .text_sm()
                            .child(format!("{}. {}", idx + 1, d.name)),
                    )
                    .child(
                        div()
                            .w(px(230.))
                            .flex_shrink_0()
                            .text_xs()
                            .text_color(MUTED_FOREGROUND)
                            .child(summary),
                    )
                    .child(actions),
            );
        }

        // --- 新增 / 修改表單 ---
        let editing = self.pd_editing.is_some();
        let mut pills = h_flex().flex_wrap().gap_1();
        for (code, label, _) in PD_RULES.iter() {
            let code: &'static str = *code;
            pills = pills.child(mode_pill(
                SharedString::from(format!("pd-rule-{code}")),
                *label,
                self.pd_rule == code,
                cx.listener(move |this, _: &ClickEvent, _, cx| {
                    this.pd_rule = code.to_string();
                    cx.notify();
                }),
            ));
        }
        let mut btns = h_flex().gap_2().justify_end().child(
            Button::new("pd-save")
                .primary()
                .label(if editing { "儲存修改" } else { "＋ 新增選項" })
                .on_click(cx.listener(|this, _: &ClickEvent, window, cx| {
                    this.pd_save(window, cx);
                })),
        );
        if editing {
            btns = btns.child(
                Button::new("pd-cancel-edit")
                    .ghost()
                    .label("取消修改")
                    .on_click(cx.listener(|this, _: &ClickEvent, window, cx| {
                        this.pd_reset_form(window, cx);
                        cx.notify();
                    })),
            );
        }
        let form = v_flex()
            .w_full()
            .p_3()
            .gap_2()
            .bg(BACKGROUND)
            .rounded_md()
            .border_1()
            .border_color(BORDER)
            .child(
                div()
                    .text_sm()
                    .font_weight(FontWeight::SEMIBOLD)
                    .child(if editing { "修改選項" } else { "新增選項" }),
            )
            .child(labeled_input("選項名稱（下拉選單顯示的文字）", &self.pd_name_input))
            .child(div().text_xs().text_color(MUTED_FOREGROUND).child("規則類型"))
            .child(pills)
            .child(labeled_input("規則值", &self.pd_value_input))
            .child(
                div()
                    .text_xs()
                    .text_color(MUTED_FOREGROUND)
                    .child(format!("說明：{}", rule_hint(&self.pd_rule))),
            )
            .child(btns);

        Some(
            div()
                .id("param-setting-overlay")
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
                        .w(px(800.))
                        .max_h(px(680.))
                        .p_5()
                        .gap_3()
                        .bg(CARD)
                        .rounded_lg()
                        .border_1()
                        .border_color(BORDER)
                        .child(
                            h_flex()
                                .w_full()
                                .items_center()
                                .justify_between()
                                .child(
                                    div()
                                        .text_lg()
                                        .font_weight(FontWeight::BOLD)
                                        .child("參數設定 — 預設輸入值下拉選單選項"),
                                )
                                .child(
                                    Button::new("pd-close")
                                        .label("關閉")
                                        .on_click(cx.listener(|this, _: &ClickEvent, _, cx| {
                                            this.pd_open = false;
                                            this.pd_confirm_delete = None;
                                            cx.notify();
                                        })),
                                ),
                        )
                        .child(
                            div()
                                .text_xs()
                                .text_color(MUTED_FOREGROUND)
                                .child("欄位的「預設輸入值」下拉選單讀取此清單，並依此處的順序顯示（可按住左側 ≡ 拖曳調整；新增的選項排在最後）。修改選項會立即套用到已使用它的欄位；刪除選項會清除這些欄位的設定。"),
                        )
                        .child(list)
                        .child(form),
                )
                .into_any_element(),
        )
    }

    /// 「參數設定說明」彈窗：詳細說明參數設定的規則與條件
    pub(super) fn render_param_help(&self, cx: &mut Context<Self>) -> Option<gpui::AnyElement> {
        if !self.param_help_open {
            return None;
        }
        let section = |title: &str| {
            div()
                .mt_2()
                .font_weight(FontWeight::BOLD)
                .text_color(FOREGROUND)
                .child(title.to_string())
        };
        let line = |text: &str| {
            div()
                .text_sm()
                .text_color(MUTED_FOREGROUND)
                .child(text.to_string())
        };

        let mut body = v_flex().w_full().gap_1();
        body = body
            .child(section("一、基本規則"))
            .child(line("• 選一支批次程式後，逐欄位設定「預設輸入值」；有參數的欄位一定要設定好才能儲存。"))
            .child(line("• 每個欄位的值有兩種來源：從「預設輸入值」下拉選單選擇，或在「自行輸入」欄填寫；兩者都有時優先採用自行輸入。"))
            .child(line("• 下拉選單的選項由「參數設定」維護（新增 / 修改 / 刪除 / 拖曳排序）。刪除選項後，使用它的欄位會變回「待設定」。"))
            .child(line("• 日期類的值以「執行日」為基準計算；輸出民國年或西元年，由每個欄位各自設定（預設民國年）。"))
            .child(section("二、下拉選單選項的規則類型"));
        for (_, label, hint) in PD_RULES.iter() {
            body = body.child(line(&format!("• {label}：{hint}")));
        }
        body = body
            .child(section("三、自行輸入（樣板）可用的變數"))
            .child(line("樣板可混用一般文字與 {變數}，執行時才換成實際值；無法辨識的 {變數} 會原樣保留。"))
            .child(line("• 日期：{E1} 執行日、{E2}~{E8} 執行日前 1~7 天（輸出年月日）"))
            .child(line("• 月份：{E9} 當月月份、{EA}{EB}{EC} 前 1/2/3 個月的月份（輸出 MM）"))
            .child(line("• 年月：{ED} 當月、{EE}{EF}{EG} 前 1/2/3 個月（輸出年月）"))
            .child(line("• 月初月底：{MONTH_FIRST} {MONTH_LAST} 當月月初 / 月底；{PREV_MONTH_FIRST} {PREV_MONTH_LAST} 前月月初 / 月底"))
            .child(line("• 偏移天數：{DAY_OFFSET:+1}、{DAY_OFFSET:-3}，或直接寫 {+8}、{-3}"))
            .child(line("• 偏移月份：{MONTH_OFFSET:-1} 輸出年月；{MONTH_OFFSET:-1:MM} 只輸出月份"))
            .child(line("• 本週星期：{WEEK_1}~{WEEK_7}（週一~週日），或 {WEEK_MON} {WEEK_TUE} {WEEK_WED} {WEEK_THU} {WEEK_FRI} {WEEK_SAT} {WEEK_SUN}"))
            .child(line("• 中文別名（一定要以「執行日」開頭）：{執行日當天}、{執行日前1天}、{執行日後3天}、{執行日當月}、{執行日前月}"))
            .child(line("　　{執行日前月第1日} {執行日前月最後1日} {執行日當月第1日} {執行日當月最後1日}"))
            .child(line("　　{執行日前1季第1日} {執行日前1季最後1日} {執行日前半年第1日} {執行日前半年最後1日} {執行日本年第1日} {執行日本年最後1日}"))
            .child(line("• 自訂變數：{自訂變數名稱} 會取「系統設定」中自訂變數的內容；{MONTH_MAIL} 依執行日月份取 MONTH_MAIL_<MM>"))
            .child(section("四、年度格式"))
            .child(line("• 民國年（預設）：{E1} → 1151127、{ED} → 11511"))
            .child(line("• 西元年：{E1} → 20261127、{ED} → 202611"))
            .child(line("• 年度格式依每個欄位各自設定，所有日期、年月、年度類規則都會跟著變。"))
            .child(section("五、範例"))
            .child(line("• DAVDF222.{E1} → DAVDF222.1151127（執行日 2026/11/27，民國年）"))
            .child(line("• RPT_{EE}_{執行日前月最後1日} → 前月年月加前月最後一天"));

        Some(
            div()
                .id("param-help-overlay")
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
                        .w(px(860.))
                        .max_h(px(680.))
                        .p_5()
                        .gap_3()
                        .bg(CARD)
                        .rounded_lg()
                        .border_1()
                        .border_color(BORDER)
                        .child(
                            h_flex()
                                .w_full()
                                .items_center()
                                .justify_between()
                                .child(
                                    div()
                                        .text_lg()
                                        .font_weight(FontWeight::BOLD)
                                        .child("參數設定說明"),
                                )
                                .child(
                                    Button::new("param-help-close")
                                        .label("關閉")
                                        .on_click(cx.listener(|this, _: &ClickEvent, _, cx| {
                                            this.param_help_open = false;
                                            cx.notify();
                                        })),
                                ),
                        )
                        .child(
                            div()
                                .id("param-help-scroll")
                                .w_full()
                                .flex_1()
                                .min_h_0()
                                .overflow_y_scroll()
                                .child(body),
                        ),
                )
                .into_any_element(),
        )
    }
}
