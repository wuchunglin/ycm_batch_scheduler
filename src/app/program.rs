//! 批次程式管理：動作與畫面

use super::*;
use super::helpers::{labeled_input, flex_labeled_input, dropdown_item};

impl BatchSchedulerApp {
    // --- Actions: Program Manager ---
    pub(super) fn browse_and_import_html(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let rx = cx.prompt_for_paths(PathPromptOptions {
            files: true,
            directories: false,
            multiple: false,
            prompt: Some("選擇 YCM 批次作業 HTML 檔案".into()),
        });

        cx.spawn_in(
            window,
            async move |this: WeakEntity<Self>, cx: &mut AsyncWindowContext| {
                if let Ok(Ok(Some(paths))) = rx.await {
                    if let Some(path) = paths.into_iter().next() {
                        let path_str = path.to_string_lossy().to_string();
                        let _ = this.update_in(cx, |this, window, cx| {
                            this.html_path_input
                                .update(cx, |s, cx| s.set_value(path_str, window, cx));
                            this.import_html_from_path(window, cx);
                        });
                    }
                }
            },
        )
        .detach();
    }

    pub(super) fn import_html_from_path(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let path = self.html_path_input.read(cx).value().trim().to_string();
        if path.is_empty() {
            window.push_notification(
                Notification::error("請先選擇或輸入 HTML 檔案路徑"),
                cx,
            );
            return;
        }
        match std::fs::read(&path) {
            Ok(bytes) => {
                let content = String::from_utf8_lossy(&bytes);
                match parse_ycm_html(&content) {
                Ok(parsed) => {
                    if parsed.program.job_code.is_empty() {
                        window.push_notification(
                            Notification::error("HTML 解析失敗：找不到 #jobCodeNumber 工作代號"),
                            cx,
                        );
                        return;
                    }
                    let job_code = parsed.program.job_code.clone();
                    let field_count = parsed.fields.len();
                    let guard = self.conn.lock().unwrap();
                    let repo = Repository::new(&guard);
                    match repo.save_program_with_fields(&parsed.program, &parsed.fields) {
                        Ok(id) => {
                            self.selected_program = repo.get_program_by_id(id).unwrap_or(None);
                            drop(repo);
                            drop(guard);
                            self.reload_programs(cx);
                            self.fill_program_inputs(window, cx);
                            let ok_msg = if field_count > 0 {
                                format!(
                                    "成功匯入批次程式 [{job_code}]，共解析 {field_count} 個欄位！請於下方為每個欄位選擇預設輸入值，未選完前無法排程"
                                )
                            } else {
                                format!("成功匯入批次程式 [{job_code}]（無參數）")
                            };
                            append_file_log("INFO", &ok_msg);
                            window.push_notification(Notification::success(ok_msg), cx);
                        }
                        Err(e) => {
                            let err_msg = format!("寫入資料庫失敗：{e}");
                            append_file_log("ERROR", &err_msg);
                            window.push_notification(Notification::error(err_msg), cx);
                        }
                    }
                }
                Err(e) => {
                    let err_msg = format!("HTML 解析失敗：{e}");
                    append_file_log("ERROR", &err_msg);
                    window.push_notification(Notification::error(err_msg), cx);
                }
                }
            }
            Err(e) => {
                let err_msg = format!("找不到檔案 {path}：{e}");
                append_file_log("ERROR", &err_msg);
                window.push_notification(Notification::error(err_msg), cx);
            }
        }
    }

    pub(super) fn add_program(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let code = self.prog_code_input.read(cx).value().trim().to_uppercase();
        let name = self.prog_name_input.read(cx).value().trim().to_string();
        if code.is_empty() {
            window.push_notification(Notification::error("請輸入工作代號"), cx);
            return;
        }
        let exists = {
            let guard = self.conn.lock().unwrap();
            let repo = Repository::new(&guard);
            let found = repo.get_program_by_code(&code).unwrap_or(None).is_some();
            found
        };
        if exists {
            window.push_notification(
                Notification::error(format!("工作代號 [{code}] 已存在，請改用「修改」")),
                cx,
            );
            return;
        }
        let res = {
            let guard = self.conn.lock().unwrap();
            let repo = Repository::new(&guard);
            let r = repo
                .create_or_update_program(&code, &name)
                .map(|id| repo.get_program_by_id(id).unwrap_or(None));
            r
        };
        match res {
            Ok(prog) => {
                self.selected_program = prog;
                self.reload_programs(cx);
                self.fill_program_inputs(window, cx);
                window.push_notification(
                    Notification::success(format!("已新增批次程式 [{code}]，請匯入或新增輸入欄位")),
                    cx,
                );
            }
            Err(e) => {
                window.push_notification(Notification::error(format!("新增程式失敗：{e}")), cx)
            }
        }
    }

    pub(super) fn modify_program(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(id) = self.selected_program.as_ref().and_then(|p| p.id) else {
            window.push_notification(Notification::error("請先點選要修改的程式"), cx);
            return;
        };
        let code = self.prog_code_input.read(cx).value().trim().to_uppercase();
        let name = self.prog_name_input.read(cx).value().trim().to_string();
        if code.is_empty() {
            window.push_notification(Notification::error("請輸入工作代號"), cx);
            return;
        }
        let res = {
            let guard = self.conn.lock().unwrap();
            let repo = Repository::new(&guard);
            let r = repo.update_program(id, &code, &name);
            r
        };
        match res {
            Ok(()) => {
                self.reload_programs(cx);
                self.fill_program_inputs(window, cx);
                window.push_notification(
                    Notification::success(format!("已修改批次程式 [{code}]")),
                    cx,
                );
            }
            Err(e) => {
                window.push_notification(Notification::error(format!("修改程式失敗：{e}")), cx)
            }
        }
    }

    pub(super) fn delete_program(&mut self, prog: Program, window: &mut Window, cx: &mut Context<Self>) {
        let Some(pid) = prog.id else { return };
        {
            let guard = self.conn.lock().unwrap();
            let repo = Repository::new(&guard);
            let _ = repo.delete_program(pid);
        }
        if self.selected_program.as_ref().and_then(|p| p.id) == Some(pid) {
            self.selected_program = None;
        }
        self.reload_programs(cx);
        self.reload_groups(window, cx);
        self.fill_program_inputs(window, cx);
        window.push_notification(
            Notification::success(format!("已刪除批次程式 [{}]", prog.job_code)),
            cx,
        );
    }

    pub(super) fn add_field_row(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.selected_program.is_none() {
            window.push_notification(Notification::error("請先選取批次程式"), cx);
            return;
        }
        // 有參數一定要選預設輸入值：前面的列沒選完，不能再新增下一列
        if let Some(pos) = self
            .field_rows
            .iter()
            .position(|r| r.default_id.is_none() && r.custom_input.read(cx).value().trim().is_empty())
        {
            let label = self.field_rows[pos].label_input.read(cx).value().trim().to_string();
            let shown = if label.is_empty() { String::new() } else { format!("「{label}」") };
            window.push_notification(
                Notification::warning(format!(
                    "請先為第 {} 列欄位{shown}選擇預設輸入值，再新增下一個欄位",
                    pos + 1
                )),
                cx,
            );
            return;
        }
        let label_input = cx.new(|cx| InputState::new(window, cx).placeholder("欄位名稱"));
        let id_input = cx.new(|cx| InputState::new(window, cx).placeholder("欄位 ID"));
        let custom_input = cx.new(|cx| {
            InputState::new(window, cx).placeholder("自行輸入，如 DAVDF222.{E1}")
        });
        self.field_rows.push(FieldRow {
            base: None,
            label_input,
            id_input,
            default_id: None,
            custom_input,
            // 新欄位預設民國年
            year_ad: false,
        });
        cx.notify();
    }

    pub(super) fn delete_field_row(&mut self, idx: usize, window: &mut Window, cx: &mut Context<Self>) {
        if idx >= self.field_rows.len() {
            return;
        }
        let row = self.field_rows.remove(idx);
        if let Some(fid) = row.base.as_ref().and_then(|b| b.id) {
            {
                let guard = self.conn.lock().unwrap();
                let repo = Repository::new(&guard);
                let _ = repo.delete_program_field(fid);
            }
            self.selected_fields.retain(|f| f.id != Some(fid));
            self.reload_groups(window, cx);
        }
        self.open_picker = None;
        self.open_year_picker = None;
        self.refresh_prog_stats();
        cx.notify();
        window.push_notification(Notification::success("已刪除欄位"), cx);
    }

    /// 欄位名稱、欄位 ID 皆不可重複；通過檢查後一次儲存所有列（含預設輸入值）
    pub(super) fn save_field_rows(&mut self, only: Option<usize>, window: &mut Window, cx: &mut Context<Self>) {
        let Some(prog_id) = self.selected_program.as_ref().and_then(|p| p.id) else {
            window.push_notification(Notification::error("請先選取批次程式"), cx);
            return;
        };
        // (原欄位, 欄位名稱, 欄位 ID, 預設輸入值 id, 自行輸入, 年度格式 true=西元年)
        let mut entries: Vec<(Option<ProgramField>, String, String, Option<i64>, String, bool)> = Vec::new();
        for row in &self.field_rows {
            let label = row.label_input.read(cx).value().trim().to_string();
            let html_id = row.id_input.read(cx).value().trim().to_string();
            let custom = row.custom_input.read(cx).value().trim().to_string();
            entries.push((row.base.clone(), label, html_id, row.default_id, custom, row.year_ad));
        }

        let mut seen_labels: HashSet<String> = HashSet::new();
        let mut seen_ids: HashSet<String> = HashSet::new();
        for (i, (_, label, html_id, _, _, _)) in entries.iter().enumerate() {
            let n = i + 1;
            if label.is_empty() || html_id.is_empty() {
                window.push_notification(
                    Notification::error(format!("第 {n} 列的欄位名稱與欄位 ID 都必須填寫")),
                    cx,
                );
                return;
            }
            if !seen_labels.insert(label.clone()) {
                window.push_notification(
                    Notification::error(format!("欄位名稱重複：{label}（第 {n} 列）")),
                    cx,
                );
                return;
            }
            if !seen_ids.insert(html_id.clone()) {
                window.push_notification(
                    Notification::error(format!("欄位 ID 重複：{html_id}（第 {n} 列）")),
                    cx,
                );
                return;
            }
        }

        // 有參數一定要選預設輸入值，否則不能儲存（單列修改只檢查該列）
        for (i, (_, label, _, default_id, custom, _)) in entries.iter().enumerate() {
            if let Some(o) = only {
                if o != i {
                    continue;
                }
            }
            if default_id.is_none() && custom.is_empty() {
                window.push_notification(
                    Notification::error(format!(
                        "第 {} 列欄位「{label}」尚未選擇預設輸入值，請選擇後再儲存",
                        i + 1
                    )),
                    cx,
                );
                return;
            }
        }

        let mut err: Option<String> = None;
        let mut saved: Vec<(usize, ProgramField)> = Vec::new();
        {
            let guard = self.conn.lock().unwrap();
            let repo = Repository::new(&guard);
            for (i, (base, label, html_id, default_id, custom, year_ad)) in entries.iter().enumerate() {
                if let Some(o) = only {
                    if o != i {
                        continue;
                    }
                }
                let field = match base {
                    Some(b) => ProgramField {
                        id: b.id,
                        program_id: prog_id,
                        field_scope: b.field_scope.clone(),
                        html_id: html_id.clone(),
                        html_name: if b.html_id == *html_id {
                            b.html_name.clone()
                        } else {
                            html_id.clone()
                        },
                        label: label.clone(),
                        tag_type: b.tag_type.clone(),
                        max_length: b.max_length,
                        is_required: b.is_required,
                        sort_order: i as i32,
                        year_ad: *year_ad,
                    },
                    None => ProgramField {
                        id: None,
                        program_id: prog_id,
                        field_scope: "PROGRAM".to_string(),
                        html_id: html_id.clone(),
                        html_name: html_id.clone(),
                        label: label.clone(),
                        tag_type: "input".to_string(),
                        max_length: 0,
                        is_required: true,
                        sort_order: i as i32,
                        year_ad: *year_ad,
                    },
                };
                match repo.upsert_program_field(&field) {
                    Ok(fid) => {
                        let mut saved_field = field.clone();
                        saved_field.id = Some(fid);
                        saved.push((i, saved_field));
                        // 有自行輸入內容時優先採用（自動建立 CUSTOM_TEMPLATE 預設值）
                        let eff_default = if custom.is_empty() {
                            Ok(*default_id)
                        } else {
                            repo.ensure_custom_default(custom).map(Some)
                        };
                        let eff_default = match eff_default {
                            Ok(d) => d,
                            Err(e) => {
                                err = Some(format!("儲存自行輸入值失敗：{e}"));
                                break;
                            }
                        };
                        if let Err(e) = repo.set_field_default(fid, eff_default) {
                            err = Some(format!("儲存預設輸入值失敗：{e}"));
                            break;
                        }
                    }
                    Err(e) => {
                        err = Some(format!("儲存欄位失敗：{e}"));
                        break;
                    }
                }
            }
        }
        if let Some(e) = err {
            window.push_notification(Notification::error(e), cx);
            return;
        }
        // 年度格式 / 預設輸入值有變動 → 執行預覽的參數要重新轉換
        self.exec_param_dirty = true;
        if only.is_some() {
            for (i, f) in saved {
                if let Some(row) = self.field_rows.get_mut(i) {
                    row.base = Some(f);
                }
            }
            self.refresh_prog_stats();
            cx.notify();
            window.push_notification(Notification::success("已修改欄位"), cx);
            return;
        }
        self.reload_programs(cx);
        self.reload_groups(window, cx);
        window.push_notification(
            Notification::success(format!("已儲存 {} 個欄位", entries.len())),
            cx,
        );
    }

    /// 以其他程式中「相同欄位 ID」已設定的預設值，帶入尚未設定的列
    #[allow(dead_code)]
    pub(super) fn apply_same_name_defaults(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(prog_id) = self.selected_program.as_ref().and_then(|p| p.id) else {
            return;
        };
        let mut applied = 0;
        for row in self.field_rows.iter_mut() {
            if row.default_id.is_some() {
                continue;
            }
            let html_id = row.id_input.read(cx).value().trim().to_string();
            if html_id.is_empty() {
                continue;
            }
            let found = {
                let guard = self.conn.lock().unwrap();
                let repo = Repository::new(&guard);
                let r = repo.find_default_by_html_id(&html_id, prog_id).unwrap_or(None);
                r
            };
            if let Some(d) = found {
                row.default_id = Some(d);
                applied += 1;
            }
        }
        cx.notify();
        let msg = if applied > 0 {
            format!("已依同欄位 ID 帶入 {applied} 個預設值，請按「儲存欄位變更」")
        } else {
            "沒有可套用的同欄位 ID 預設值".to_string()
        };
        window.push_notification(Notification::success(msg), cx);
    }

    pub(super) fn render_program_view(&self, viewport_w: gpui::Pixels, cx: &mut Context<Self>) -> impl IntoElement {
        let is_narrow = viewport_w < px(860.);
        let is_compact = viewport_w < px(1060.);
        let sel_id = self.selected_program.as_ref().and_then(|p| p.id);

        // ---- 上層：程式清單 ----
        let mut prog_rows = v_flex().w_full().gap_1();
        if !is_narrow {
            prog_rows = prog_rows.child(
                h_flex()
                    .w_full()
                    .px_2()
                    .py_1()
                    .gap_2()
                    .text_size(px(14.))
                    .font_weight(FontWeight::BOLD)
                    .text_color(MUTED_FOREGROUND)
                    .child(
                        h_flex()
                            .flex_1()
                            .min_w_0()
                            .gap_2()
                            .child(div().w(px(140.)).flex_shrink_0().child("工作代號"))
                            .child(div().flex_1().min_w_0().child("工作名稱"))
                            .child(div().w(px(64.)).flex_shrink_0().child("欄位數"))
                            .child(div().w(px(120.)).flex_shrink_0().child("預設值設定")),
                    )
                    .child(div().w(px(70.)).flex_shrink_0().child("操作")),
            );
        }
        if self.programs.is_empty() {
            prog_rows = prog_rows.child(
                div()
                    .w_full()
                    .p_3()
                    .text_sm()
                    .text_color(MUTED_FOREGROUND)
                    .child("尚無批次程式，請於上方新增或匯入 HTML"),
            );
        }
        for p in &self.programs {
            let pid = p.id.unwrap_or(0);
            let (field_count, default_count) =
                self.prog_stats.get(&pid).copied().unwrap_or((0, 0));
            let is_sel = sel_id == p.id;
            let complete = field_count > 0 && default_count == field_count;
            let p_select = p.clone();
            let p_delete = p.clone();
            let status_tag = if field_count == 0 {
                Tag::secondary().child("無參數")
            } else if complete {
                Tag::primary().child(format!("{default_count} / {field_count} 完成"))
            } else {
                Tag::warning().child(format!("待設定 {default_count} / {field_count}"))
            };

            if is_narrow {
                prog_rows = prog_rows.child(
                    v_flex()
                        .w_full()
                        .gap_2()
                        .px_3()
                        .py_2()
                        .text_size(px(14.))
                        .rounded_md()
                        .bg(if is_sel { SECONDARY } else { BACKGROUND })
                        .border_1()
                        .border_color(if is_sel { PRIMARY } else { BORDER })
                        .child(
                            h_flex()
                                .w_full()
                                .items_center()
                                .justify_between()
                                .gap_2()
                                .child(
                                    h_flex()
                                        .id(SharedString::from(format!("prog-row-{pid}")))
                                        .flex_1()
                                        .min_w_0()
                                        .flex_wrap()
                                        .items_center()
                                        .gap_2()
                                        .cursor_pointer()
                                        .on_click(cx.listener(move |this, _: &ClickEvent, window, cx| {
                                            this.select_program(p_select.clone(), window, cx);
                                        }))
                                        .child(
                                            div()
                                                .font_weight(FontWeight::BOLD)
                                                .child(p.job_code.clone()),
                                        )
                                        .child(
                                            div()
                                                .text_size(px(14.))
                                                .text_color(MUTED_FOREGROUND)
                                                .child(p.job_name.clone()),
                                        ),
                                )
                                .child(
                                    Button::new(SharedString::from(format!("del-prog-{pid}")))
                                        .small()
                                        .danger()
                                        .label("刪除")
                                        .on_click(cx.listener(move |this, _: &ClickEvent, window, cx| {
                                            this.delete_program(p_delete.clone(), window, cx);
                                        })),
                                ),
                        )
                        .child(
                            h_flex()
                                .w_full()
                                .flex_wrap()
                                .items_center()
                                .gap_3()
                                .text_size(px(14.))
                                .text_color(MUTED_FOREGROUND)
                                .child(format!("欄位數：{field_count}"))
                                .child(status_tag),
                        ),
                );
            } else {
                prog_rows = prog_rows.child(
                    h_flex()
                        .w_full()
                        .items_center()
                        .gap_2()
                        .px_2()
                        .py_0p5()
                        .text_size(px(14.))
                        .rounded_md()
                        .bg(if is_sel { SECONDARY } else { BACKGROUND })
                        .border_1()
                        .border_color(if is_sel { PRIMARY } else { BORDER })
                        .child(
                            h_flex()
                                .id(SharedString::from(format!("prog-row-{pid}")))
                                .flex_1()
                                .min_w_0()
                                .items_center()
                                .gap_2()
                                .py_0p5()
                                .cursor_pointer()
                                .on_click(cx.listener(move |this, _: &ClickEvent, window, cx| {
                                    this.select_program(p_select.clone(), window, cx);
                                }))
                                .child(
                                    div()
                                        .w(px(140.))
                                        .flex_shrink_0()
                                        .font_weight(FontWeight::BOLD)
                                        .child(p.job_code.clone()),
                                )
                                .child(div().flex_1().min_w_0().child(p.job_name.clone()))
                                .child(div().w(px(64.)).flex_shrink_0().child(format!("{field_count}")))
                                .child(div().w(px(120.)).flex_shrink_0().child(status_tag)),
                        )
                        .child(
                            div().w(px(70.)).flex_shrink_0().child(
                                Button::new(SharedString::from(format!("del-prog-{pid}")))
                                    .small()
                                    .danger()
                                    .label("刪除")
                                    .on_click(cx.listener(move |this, _: &ClickEvent, window, cx| {
                                        this.delete_program(p_delete.clone(), window, cx);
                                    })),
                            ),
                        ),
                );
            }
        }

        // ---- 下層：欄位明細（可直接編輯 / 刪除）----
        let prog_title = self
            .selected_program
            .as_ref()
            .map(|p| p.job_code.clone())
            .unwrap_or_else(|| "未選取".to_string());

        let mut field_table = v_flex().w_full().gap_2();
        if self.selected_program.is_some() && !is_compact {
            field_table = field_table.child(
                h_flex()
                    .w_full()
                    .px_2()
                    .py_1()
                    .gap_2()
                    .text_xs()
                    .font_weight(FontWeight::BOLD)
                    .text_color(MUTED_FOREGROUND)
                    .child(div().w(px(32.)).flex_shrink_0().child("#"))
                    .child(div().flex_1().min_w(px(120.)).child("欄位名稱"))
                    .child(div().flex_1().min_w(px(120.)).child("欄位 ID"))
                    .child(div().w(px(240.)).flex_shrink_0().child("預設輸入值（下拉）"))
                    .child(div().flex_1().min_w(px(200.)).child("自行輸入（優先，可用 {E1} 等變數）"))
                    .child(div().w(px(150.)).flex_shrink_0().child("年度格式"))
                    .child(div().w(px(80.)).flex_shrink_0().child("操作")),
            );
        }
        if self.selected_program.is_some() && self.field_rows.is_empty() {
            field_table = field_table.child(
                div()
                    .w_full()
                    .p_3()
                    .text_sm()
                    .text_color(MUTED_FOREGROUND)
                    .child("此程式無參數（若有輸入欄位，可按「＋ 新增欄位」或匯入 HTML 帶入）"),
            );
        }
        for (i, row) in self.field_rows.iter().enumerate() {
            let has_custom = !row.custom_input.read(cx).value().trim().is_empty();
            let def_label = if has_custom {
                "（以右側自行輸入為準）".to_string()
            } else {
                self.default_name(row.default_id)
            };
            let picker_open = self.open_picker == Some(i);
            let pick_btn = {
                let trigger = h_flex()
                    .id(SharedString::from(format!("pick-{i}")))
                    .w_full()
                    .h(px(28.))
                    .px_3()
                    .items_center()
                    .justify_between()
                    .bg(BACKGROUND)
                    .border_1()
                    .border_color(if picker_open {
                        PRIMARY
                    } else if row.default_id.is_none() && !has_custom {
                        DANGER
                    } else {
                        BORDER
                    })
                    .rounded_md()
                    .cursor_pointer()
                    .on_mouse_down(
                        MouseButton::Left,
                        cx.listener(move |this, ev: &MouseDownEvent, window, cx| {
                            // 此次點擊已先被選單的「點外部關閉」處理過 → 視為關閉，不再重開
                            if this.picker_closed_by == Some((i, ev.position)) {
                                this.picker_closed_by = None;
                                cx.notify();
                                return;
                            }
                            this.picker_closed_by = None;
                            this.open_year_picker = None;
                            this.open_picker = if this.open_picker == Some(i) {
                                None
                            } else {
                                Some(i)
                            };
                            if this.open_picker.is_some() {
                                window.focus(&this.picker_focus);
                            }
                            cx.notify();
                        }),
                    )
                    .child(
                        div()
                            .text_sm()
                            .text_color(if row.default_id.is_none() && !has_custom { DANGER } else { FOREGROUND })
                            .child(if row.default_id.is_none() && !has_custom {
                                "請選擇預設輸入值".to_string()
                            } else {
                                def_label
                            }),
                    )
                    .child(div().text_xs().text_color(MUTED_FOREGROUND).child("▼"));

                let mut cell = div().relative().w_full().child(trigger);
                if picker_open {
                    let mut menu = v_flex()
                        .id(SharedString::from(format!("pk-menu-{i}")))
                        .w(px(260.))
                        .max_h(px(240.))
                        .overflow_y_scroll()
                        .p_1()
                        .gap_0p5()
                        .bg(CARD)
                        .border_1()
                        .border_color(BORDER)
                        .rounded_md()
                        .shadow_lg()
                        .occlude()
                        .track_focus(&self.picker_focus)
                        .on_key_down(cx.listener(|this, ev: &KeyDownEvent, _, cx| {
                            if ev.keystroke.key == "escape" {
                                this.open_picker = None;
                                cx.notify();
                            }
                        }))
                        .on_mouse_down_out(cx.listener(move |this, ev: &MouseDownEvent, _, cx| {
                            if this.open_picker == Some(i) {
                                this.open_picker = None;
                                this.picker_closed_by = Some((i, ev.position));
                                cx.notify();
                            }
                        }));
                    for d in self.default_values.iter().filter(|d| !is_custom_default(d)) {
                        let did = d.id;
                        menu = menu.child(dropdown_item(
                            SharedString::from(format!("pk-{i}-{}", did.unwrap_or(0))),
                            &d.name,
                            row.default_id == did && !has_custom,
                            cx.listener(move |this, _: &ClickEvent, window, cx| {
                                if let Some(r) = this.field_rows.get_mut(i) {
                                    r.default_id = did;
                                    // 改選下拉項目 → 清掉自行輸入，避免兩者衝突
                                    r.custom_input
                                        .update(cx, |s, cx| s.set_value("", window, cx));
                                }
                                this.open_picker = None;
                                cx.notify();
                            }),
                        ));
                    }
                    // 用 anchored 讓選單自動貼齊視窗邊界：下方空間不夠時會往上移，不會被視窗截掉
                    let anchored_menu = gpui::anchored()
                        .anchor(gpui::Corner::TopLeft)
                        .offset(gpui::point(px(0.), px(36.)))
                        .snap_to_window_with_margin(px(8.))
                        .child(menu);
                    cell = cell.child(gpui::deferred(anchored_menu).with_priority(10));
                }
                cell
            };
            // 年度格式：民國年（預設）/ 西元年，每個欄位各自設定
            // 年度格式：下拉選單（民國年為預設 / 西元年），每個欄位各自設定
            let year_open = self.open_year_picker == Some(i);
            let year_trigger = h_flex()
                .id(SharedString::from(format!("year-pick-{i}")))
                .w_full()
                .h(px(28.))
                .px_3()
                .items_center()
                .justify_between()
                .bg(BACKGROUND)
                .border_1()
                .border_color(if year_open { PRIMARY } else { BORDER })
                .rounded_md()
                .cursor_pointer()
                .on_mouse_down(
                    MouseButton::Left,
                    cx.listener(move |this, ev: &MouseDownEvent, _, cx| {
                        // 此次點擊已先被選單的「點外部關閉」處理過 → 視為關閉，不再重開
                        if this.year_picker_closed_by == Some((i, ev.position)) {
                            this.year_picker_closed_by = None;
                            cx.notify();
                            return;
                        }
                        this.year_picker_closed_by = None;
                        this.open_picker = None;
                        this.open_year_picker = if this.open_year_picker == Some(i) {
                            None
                        } else {
                            Some(i)
                        };
                        cx.notify();
                    }),
                )
                .child(div().text_sm().child(if row.year_ad { "西元年" } else { "民國年" }))
                .child(div().text_xs().text_color(MUTED_FOREGROUND).child("▼"));
            let mut year_cell = div().relative().w_full().child(year_trigger);
            if year_open {
                let mut menu = v_flex()
                    .id(SharedString::from(format!("year-menu-{i}")))
                    .w(px(130.))
                    .p_1()
                    .gap_0p5()
                    .bg(CARD)
                    .border_1()
                    .border_color(BORDER)
                    .rounded_md()
                    .shadow_lg()
                    .occlude()
                    .on_mouse_down_out(cx.listener(move |this, ev: &MouseDownEvent, _, cx| {
                        if this.open_year_picker == Some(i) {
                            this.open_year_picker = None;
                            this.year_picker_closed_by = Some((i, ev.position));
                            cx.notify();
                        }
                    }));
                for (label, is_ad) in [("民國年", false), ("西元年", true)] {
                    menu = menu.child(dropdown_item(
                        SharedString::from(format!("year-opt-{i}-{}", is_ad as u8)),
                        label,
                        row.year_ad == is_ad,
                        cx.listener(move |this, _: &ClickEvent, _, cx| {
                            if let Some(r) = this.field_rows.get_mut(i) {
                                r.year_ad = is_ad;
                            }
                            this.open_year_picker = None;
                            cx.notify();
                        }),
                    ));
                }
                let anchored_menu = gpui::anchored()
                    .anchor(gpui::Corner::TopLeft)
                    .offset(gpui::point(px(0.), px(36.)))
                    .snap_to_window_with_margin(px(8.))
                    .child(menu);
                year_cell = year_cell.child(gpui::deferred(anchored_menu).with_priority(10));
            }
            let year_radios = year_cell;
            let del_btn = Button::new(SharedString::from(format!("del-fld-{i}")))
                .small()
                .danger()
                .label("刪除")
                .on_click(cx.listener(move |this, _: &ClickEvent, window, cx| {
                    this.delete_field_row(i, window, cx);
                }));

            if is_compact {
                field_table = field_table.child(
                    v_flex()
                        .w_full()
                        .p_3()
                        .gap_2()
                        .bg(BACKGROUND)
                        .rounded_md()
                        .border_1()
                        .border_color(BORDER)
                        .child(
                            h_flex()
                                .w_full()
                                .flex_wrap()
                                .items_center()
                                .justify_between()
                                .gap_2()
                                .child(
                                    h_flex()
                                        .items_center()
                                        .gap_2()
                                        .child(
                                            div()
                                                .text_xs()
                                                .font_weight(FontWeight::BOLD)
                                                .text_color(MUTED_FOREGROUND)
                                                .child(format!("#{}", i + 1)),
                                        )
                                        .child(div().w(px(260.)).child(pick_btn)),
                                )
                                .child(h_flex().gap_1().child(del_btn)),
                        )
                        .child(
                            v_flex()
                                .w_full()
                                .gap_2()
                                .child(labeled_input("欄位名稱", &row.label_input))
                                .child(labeled_input("欄位 ID", &row.id_input))
                                .child(labeled_input("自行輸入（優先，可用 {E1} 等變數）", &row.custom_input))
                                .child(
                                    h_flex()
                                        .items_center()
                                        .gap_3()
                                        .child(div().text_xs().text_color(MUTED_FOREGROUND).child("年度格式"))
                                        .child(div().w(px(130.)).child(year_radios)),
                                ),
                        ),
                );
            } else {
                field_table = field_table.child(
                    h_flex()
                        .w_full()
                        .px_2()
                        .py_0p5()
                        .gap_2()
                        .items_center()
                        .bg(BACKGROUND)
                        .rounded_md()
                        .border_1()
                        .border_color(BORDER)
                        .child(
                            div()
                                .w(px(32.))
                                .flex_shrink_0()
                                .text_color(MUTED_FOREGROUND)
                                .child(format!("{}", i + 1)),
                        )
                        .child(div().flex_1().min_w(px(120.)).child(Input::new(&row.label_input).small()))
                        .child(div().flex_1().min_w(px(120.)).child(Input::new(&row.id_input).small()))
                        .child(div().w(px(240.)).flex_shrink_0().child(pick_btn))
                        .child(div().flex_1().min_w(px(200.)).child(Input::new(&row.custom_input).small()))
                        .child(div().w(px(150.)).flex_shrink_0().child(year_radios))
                        .child(
                            h_flex()
                                .w(px(80.))
                                .flex_shrink_0()
                                .gap_1()
                                .child(del_btn),
                        ),
                );
            }
        }

        let prog_form_buttons = h_flex()
            .flex_wrap()
            .gap_2()
            .child(
                Button::new("btn-add-prog")
                    .primary()
                    .label("新增")
                    .on_click(cx.listener(|this, _: &ClickEvent, window, cx| {
                        this.add_program(window, cx);
                    })),
            )
            .child(
                Button::new("btn-modify-prog")
                    .label("修改")
                    .on_click(cx.listener(|this, _: &ClickEvent, window, cx| {
                        this.modify_program(window, cx);
                    })),
            )
            .child(
                Button::new("btn-import-html")
                    .label("匯入")
                    .on_click(cx.listener(|this, _: &ClickEvent, window, cx| {
                        this.browse_and_import_html(window, cx);
                    })),
            );

        let prog_form_card = if is_narrow {
            v_flex()
                .w_full()
                .bg(CARD)
                .p_3()
                .rounded_lg()
                .border_1()
                .border_color(BORDER)
                .gap_3()
                .items_start()
                .child(labeled_input("工作代號", &self.prog_code_input))
                .child(labeled_input("工作名稱", &self.prog_name_input))
                .child(prog_form_buttons)
                .into_any_element()
        } else {
            h_flex()
                .w_full()
                .flex_wrap()
                .bg(CARD)
                .p_3()
                .rounded_lg()
                .border_1()
                .border_color(BORDER)
                .gap_3()
                .items_end()
                .child(
                    div()
                        .w(px(200.))
                        .min_w(px(150.))
                        .child(labeled_input("工作代號", &self.prog_code_input)),
                )
                .child(flex_labeled_input("工作名稱", &self.prog_name_input, px(200.)))
                .child(prog_form_buttons)
                .into_any_element()
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
                    .child(div().text_xl().font_weight(FontWeight::BOLD).child("批次程式新增"))
                    .child(
                        v_flex()
                            .w_full()
                            .gap_0p5()
                            .text_sm()
                            .text_color(MUTED_FOREGROUND)
                            .child("1. 上方維護程式；選一支程式後，下方逐欄位設定預設輸入值。")
                            .child("2. 有參數的欄位一定要設定好才能儲存：從下拉選單選擇，或在「自行輸入」欄填寫（填寫時優先採用）。")
                            .child(
                                h_flex()
                                    .items_center()
                                    .gap_1()
                                    .child("3. 自行輸入可混用文字與日期變數，例如：DAVDF222.{E1} → DAVDF222.1151127，")
                                    .child(
                                        Button::new("param-help-open")
                                            .small()
                                            .label("詳參數設定說明")
                                            .on_click(cx.listener(|this, _: &ClickEvent, _, cx| {
                                                this.param_help_open = true;
                                                cx.notify();
                                            })),
                                    ),
                            )
                            .child("4. 年度格式依欄位設定：民國年（預設）{E1} → 1151127、{ED} → 11511；西元年 {E1} → 20261127、{ED} → 202611"),
                    ),
            )
            // 工作代號 / 工作名稱 + 新增 修改 匯入
            .child(prog_form_card)
            // 程式清單
            .child(
                div()
                    .id("prog-table")
                    .w_full()
                    .max_h(px(240.))
                    .overflow_y_scroll()
                    .bg(CARD)
                    .p_2()
                    .rounded_lg()
                    .border_1()
                    .border_color(BORDER)
                    .child(prog_rows),
            )
            // 欄位明細
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
                                    .child(format!("輸入欄位與預設輸入值 — {prog_title}")),
                            )
                            .child(
                                h_flex()
                                    .flex_wrap()
                                    .gap_2()
                                    .child(
                                        Button::new("btn-param-setting")
                                            .label("參數設定")
                                            .on_click(cx.listener(|this, _: &ClickEvent, window, cx| {
                                                this.open_param_setting(window, cx);
                                            })),
                                    )
                                    .child(
                                        Button::new("btn-add-field")
                                            .label("＋ 新增欄位")
                                            .on_click(cx.listener(|this, _: &ClickEvent, window, cx| {
                                                this.add_field_row(window, cx);
                                            })),
                                    )
                                    .child(
                                        Button::new("btn-save-fields")
                                            .primary()
                                            .label("儲存欄位變更")
                                            .on_click(cx.listener(|this, _: &ClickEvent, window, cx| {
                                                this.save_field_rows(None, window, cx);
                                            })),
                                    ),
                            ),
                    )
                    .child(field_table),
            )
            .into_any_element()
    }
}
