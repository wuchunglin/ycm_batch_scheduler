//! YCM Batch Scheduler Application Main Workspace View (Pure Rust GPUI Generic Version)

use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use chrono::Local;
use gpui::{
    div, px, AppContext, AsyncApp, AsyncWindowContext, ClickEvent, Context, Entity, FontWeight,
    InteractiveElement, IntoElement, ParentElement, PathPromptOptions, Render, SharedString,
    StatefulInteractiveElement, Styled, WeakEntity, Window,
};
use gpui_component::{
    button::{Button, ButtonVariants},
    checkbox::Checkbox,
    h_flex,
    input::{Input, InputState},
    notification::Notification,
    tag::Tag,
    v_flex, Root, WindowExt,
};
use rusqlite::Connection;
use tokio::sync::mpsc;

use crate::theme::{
    hsl, BACKGROUND, BORDER, CARD, DANGER, FOREGROUND, MUTED_FOREGROUND, PRIMARY, SECONDARY,
    SUCCESS, WARNING,
};
use ycm_batch_scheduler::db::repository::Repository;
use ycm_batch_scheduler::engine::browser_runner::{
    append_file_log, current_log_file_path, execute_group, BrowserRunnerConfig, ExecutionRequest,
    ExecutionStepInput, RunnerEvent,
};
use ycm_batch_scheduler::engine::param_resolver::{ParamResolver, RocDateUtils};
use ycm_batch_scheduler::engine::schedule_gen::ScheduleGenerator;
use ycm_batch_scheduler::models::{
    CustomVariable, DateRangeMode, GeneratedScheduleItem, GroupItem, GroupItemParamDetail,
    JobGroup, Program, ProgramField, ScheduleMode, ScheduleRule,
};
use ycm_batch_scheduler::parser::html_parser::parse_ycm_html;
use ycm_batch_scheduler::ui::runner_view::RunnerViewState;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NavTab {
    ProgramManager,
    GroupEditor,
    ScheduleRule,
    ExecutionRunner,
    Settings,
}

const RULE_CODES: &[&str] = &[
    "E1",
    "E2",
    "E3",
    "E4",
    "E5",
    "E6",
    "E7",
    "E8",
    "E9",
    "EA",
    "EB",
    "EC",
    "ED",
    "EE",
    "EF",
    "EG",
    "WEEK_DAY",
    "DAY_OFFSET",
    "MONTH_OFFSET",
    "MONTH_EDGE",
    "CUSTOM_TEMPLATE",
    "STATIC",
];

pub struct BatchSchedulerApp {
    conn: Arc<Mutex<Connection>>,
    tokio_rt: Arc<tokio::runtime::Runtime>,
    active_tab: NavTab,

    // --- Program Manager State ---
    programs: Vec<Program>,
    selected_program: Option<Program>,
    selected_fields: Vec<ProgramField>,
    html_path_input: Entity<InputState>,
    prog_code_input: Entity<InputState>,
    prog_name_input: Entity<InputState>,
    editing_field_id: Option<i64>,
    field_scope: String,
    field_tag_type: String,
    field_required: bool,
    field_id_input: Entity<InputState>,
    field_label_input: Entity<InputState>,
    field_maxlen_input: Entity<InputState>,

    // --- Group Editor State ---
    groups: Vec<JobGroup>,
    selected_group: Option<JobGroup>,
    group_items: Vec<GroupItem>,
    selected_item: Option<GroupItem>,
    selected_item_params: Vec<GroupItemParamDetail>,
    new_group_name_input: Entity<InputState>,
    new_group_desc_input: Entity<InputState>,
    // Common fields inputs for selected_item
    item_exe_time_input: Entity<InputState>,
    item_exe_date_rule_input: Entity<InputState>,
    item_pre_sn1_input: Entity<InputState>,
    item_pre_sn2_input: Entity<InputState>,
    item_pre_sn3_input: Entity<InputState>,
    item_rework_sn_input: Entity<InputState>,
    item_mail_input: Entity<InputState>,
    item_pre_note_input: Entity<InputState>,
    item_rear_note_input: Entity<InputState>,
    item_req_op: bool,
    item_link_prev: bool,
    item_req_email: bool,
    // Input parameter editor (欄位名稱 + 欄位 ID + 動態規則)
    param_editing_field_id: Option<i64>,
    param_field_scope: String,
    param_label_input: Entity<InputState>,
    param_html_id_input: Entity<InputState>,
    param_rule_code: String,
    param_rule_value_input: Entity<InputState>,
    // Live expression tester
    tester_date_input: Entity<InputState>,
    tester_expr_input: Entity<InputState>,

    // --- Schedule Rule State ---
    available_years: Vec<i32>,
    schedule_rule: ScheduleRule,
    preview_items: Vec<GeneratedScheduleItem>,
    sched_add_year_input: Entity<InputState>,
    sched_range_start_input: Entity<InputState>,
    sched_range_end_input: Entity<InputState>,
    sched_monthly_days_input: Entity<InputState>,
    sched_specific_dates_input: Entity<InputState>,
    sched_default_time_input: Entity<InputState>,

    // --- Runner State ---
    runner_state: RunnerViewState,
    runner_dry_run: bool,
    runner_override_date_input: Entity<InputState>,

    // --- Settings & Variables State ---
    cfg_edge_path_input: Entity<InputState>,
    cfg_target_url_input: Entity<InputState>,
    cfg_pin_input: Entity<InputState>,
    cfg_screenshot_dir_input: Entity<InputState>,
    cfg_timeout_input: Entity<InputState>,
    custom_variables: Vec<CustomVariable>,
    var_key_input: Entity<InputState>,
    var_value_input: Entity<InputState>,
    var_desc_input: Entity<InputState>,
}

impl BatchSchedulerApp {
    pub fn new(
        conn: Arc<Mutex<Connection>>,
        tokio_rt: Arc<tokio::runtime::Runtime>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Self {
        let (edge_path, target_url, pin, screenshot_dir, timeout_sec) = {
            let guard = conn.lock().unwrap();
            let repo = Repository::new(&guard);
            (
                repo.get_config("edge_path")
                    .unwrap_or(None)
                    .unwrap_or_else(|| {
                        r"C:\Program Files (x86)\Microsoft\Edge\Application\msedge.exe".to_string()
                    }),
                repo.get_config("target_url")
                    .unwrap_or(None)
                    .unwrap_or_else(|| {
                        "https://www.nat.fia.gov.tw/ycm/jobExecute/unRegular/getSysData".to_string()
                    }),
                repo.get_config("default_pin")
                    .unwrap_or(None)
                    .unwrap_or_default(),
                repo.get_config("screenshot_dir")
                    .unwrap_or(None)
                    .unwrap_or_else(|| "screenshots".to_string()),
                repo.get_config("action_timeout_sec")
                    .unwrap_or(None)
                    .unwrap_or_else(|| "30".to_string()),
            )
        };

        let today_roc = RocDateUtils::to_roc_yyymmdd(&Local::now().date_naive(), "/");
        let default_rule = ScheduleRule::default();
        let preview = ScheduleGenerator::generate(&default_rule);

        let mut app = Self {
            conn,
            tokio_rt,
            active_tab: NavTab::ProgramManager,

            programs: Vec::new(),
            selected_program: None,
            selected_fields: Vec::new(),
            html_path_input: cx.new(|cx| {
                InputState::new(window, cx)
                    .placeholder("點選「選擇檔案並匯入」或輸入 HTML 檔案路徑...")
            }),
            prog_code_input: cx
                .new(|cx| InputState::new(window, cx).placeholder("工作代號 (如 YCM001X)")),
            prog_name_input: cx
                .new(|cx| InputState::new(window, cx).placeholder("工作名稱")),
            editing_field_id: None,
            field_scope: "PROGRAM".to_string(),
            field_tag_type: "input".to_string(),
            field_required: true,
            field_id_input: cx.new(|cx| {
                InputState::new(window, cx).placeholder("欄位 ID (如 programParameter_0_0)")
            }),
            field_label_input: cx.new(|cx| {
                InputState::new(window, cx).placeholder("欄位名稱 (如 轉檔起始日期)")
            }),
            field_maxlen_input: cx.new(|cx| {
                InputState::new(window, cx)
                    .placeholder("最大長度")
                    .default_value("0")
            }),

            groups: Vec::new(),
            selected_group: None,
            group_items: Vec::new(),
            selected_item: None,
            selected_item_params: Vec::new(),
            new_group_name_input: cx
                .new(|cx| InputState::new(window, cx).placeholder("新排程群組名稱")),
            new_group_desc_input: cx
                .new(|cx| InputState::new(window, cx).placeholder("群組說明")),
            item_exe_time_input: cx.new(|cx| {
                InputState::new(window, cx)
                    .placeholder("HH:mm (空白=當下時間)")
                    .default_value("13:00")
            }),
            item_exe_date_rule_input: cx.new(|cx| {
                InputState::new(window, cx)
                    .placeholder("GROUP_DATE 或 WEEK_5")
                    .default_value("GROUP_DATE")
            }),
            item_pre_sn1_input: cx
                .new(|cx| InputState::new(window, cx).placeholder("靜態前置序號 1")),
            item_pre_sn2_input: cx
                .new(|cx| InputState::new(window, cx).placeholder("前置序號 2")),
            item_pre_sn3_input: cx
                .new(|cx| InputState::new(window, cx).placeholder("前置序號 3")),
            item_rework_sn_input: cx
                .new(|cx| InputState::new(window, cx).placeholder("重作工作序號")),
            item_mail_input: cx.new(|cx| {
                InputState::new(window, cx).placeholder("分號分隔，支援 {自訂變數}")
            }),
            item_pre_note_input: cx
                .new(|cx| InputState::new(window, cx).placeholder("前置工作備註")),
            item_rear_note_input: cx
                .new(|cx| InputState::new(window, cx).placeholder("後置工作備註")),
            item_req_op: false,
            item_link_prev: false,
            item_req_email: true,

            param_editing_field_id: None,
            param_field_scope: "PROGRAM".to_string(),
            param_label_input: cx
                .new(|cx| InputState::new(window, cx).placeholder("欄位名稱 (Label)")),
            param_html_id_input: cx.new(|cx| {
                InputState::new(window, cx).placeholder("欄位 ID (HTML ID)")
            }),
            param_rule_code: "E1".to_string(),
            param_rule_value_input: cx.new(|cx| {
                InputState::new(window, cx).placeholder("樣板字串 / 偏移 / 固定值")
            }),

            tester_date_input: cx.new(|cx| {
                InputState::new(window, cx)
                    .placeholder("YYY/MM/DD")
                    .default_value(today_roc.clone())
            }),
            tester_expr_input: cx.new(|cx| {
                InputState::new(window, cx)
                    .placeholder("如 FILE_{E1}.DAT")
                    .default_value("FILE_{E1}.DAT")
            }),

            available_years: vec![115, 116],
            schedule_rule: default_rule,
            preview_items: preview,
            sched_add_year_input: cx
                .new(|cx| InputState::new(window, cx).placeholder("新增民國年 (如 117)")),
            sched_range_start_input: cx.new(|cx| {
                InputState::new(window, cx)
                    .placeholder("起 115/01/01")
                    .default_value("115/01/01")
            }),
            sched_range_end_input: cx.new(|cx| {
                InputState::new(window, cx)
                    .placeholder("迄 115/12/31")
                    .default_value("115/12/31")
            }),
            sched_monthly_days_input: cx.new(|cx| {
                InputState::new(window, cx)
                    .placeholder("指定日 (逗號分隔最多3個，如 1,15)")
                    .default_value("1, 15")
            }),
            sched_specific_dates_input: cx.new(|cx| {
                InputState::new(window, cx)
                    .placeholder("指定日期 (逗號分隔最多5組 yyy/MM/dd)")
                    .default_value(today_roc)
            }),
            sched_default_time_input: cx.new(|cx| {
                InputState::new(window, cx)
                    .placeholder("HH:mm")
                    .default_value("13:00")
            }),

            runner_state: RunnerViewState::default(),
            runner_dry_run: true,
            runner_override_date_input: cx.new(|cx| {
                InputState::new(window, cx)
                    .placeholder("選填：單日測試 (yyy/MM/dd)，空白則依群組排程展開")
            }),

            cfg_edge_path_input: cx.new(|cx| {
                InputState::new(window, cx)
                    .placeholder("msedge.exe 路徑")
                    .default_value(edge_path)
            }),
            cfg_target_url_input: cx.new(|cx| {
                InputState::new(window, cx)
                    .placeholder("YCM 目標網址")
                    .default_value(target_url)
            }),
            cfg_pin_input: cx.new(|cx| {
                InputState::new(window, cx)
                    .placeholder("預設 PIN 碼")
                    .default_value(pin)
            }),
            cfg_screenshot_dir_input: cx.new(|cx| {
                InputState::new(window, cx)
                    .placeholder("截圖存檔資料夾")
                    .default_value(screenshot_dir)
            }),
            cfg_timeout_input: cx.new(|cx| {
                InputState::new(window, cx)
                    .placeholder("逾時秒數")
                    .default_value(timeout_sec)
            }),
            custom_variables: Vec::new(),
            var_key_input: cx.new(|cx| {
                InputState::new(window, cx).placeholder("變數鍵名 (如 MONTH_MAIL_11)")
            }),
            var_value_input: cx
                .new(|cx| InputState::new(window, cx).placeholder("變數值")),
            var_desc_input: cx
                .new(|cx| InputState::new(window, cx).placeholder("說明 (選填)")),
        };

        app.reload_all(window, cx);
        app
    }

    fn reload_all(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.reload_programs(cx);
        self.reload_groups(window, cx);
        self.reload_variables(cx);
    }

    fn reload_programs(&mut self, cx: &mut Context<Self>) {
        let guard = self.conn.lock().unwrap();
        let repo = Repository::new(&guard);
        if let Ok(progs) = repo.list_programs() {
            self.programs = progs;
            let sel_id = self.selected_program.as_ref().and_then(|p| p.id);
            if let Some(id) = sel_id {
                self.selected_program = self.programs.iter().find(|p| p.id == Some(id)).cloned();
            }
            if self.selected_program.is_none() && !self.programs.is_empty() {
                self.selected_program = Some(self.programs[0].clone());
            }
            if let Some(pid) = self.selected_program.as_ref().and_then(|p| p.id) {
                self.selected_fields = repo.list_program_fields(pid).unwrap_or_default();
            } else {
                self.selected_fields.clear();
            }
            cx.notify();
        }
    }

    fn reload_groups(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let (groups, selected_group, items) = {
            let guard = self.conn.lock().unwrap();
            let repo = Repository::new(&guard);
            let groups = repo.list_job_groups().unwrap_or_default();
            let sel_id = self.selected_group.as_ref().and_then(|g| g.id);
            let selected = sel_id
                .and_then(|id| groups.iter().find(|g| g.id == Some(id)).cloned())
                .or_else(|| groups.first().cloned());
            let items = selected
                .as_ref()
                .and_then(|g| g.id)
                .and_then(|gid| repo.list_group_items(gid).ok())
                .unwrap_or_default();
            (groups, selected, items)
        };

        self.groups = groups;
        self.selected_group = selected_group;
        self.group_items = items;

        if let Some(ref g) = self.selected_group {
            if let Ok(rule) = serde_json::from_str::<ScheduleRule>(&g.schedule_rule_json) {
                self.schedule_rule = rule;
                self.sync_schedule_inputs_from_rule(window, cx);
                self.preview_items = ScheduleGenerator::generate(&self.schedule_rule);
            }
        }

        let sel_item_id = self.selected_item.as_ref().and_then(|i| i.id);
        let next_item = sel_item_id
            .and_then(|id| self.group_items.iter().find(|i| i.id == Some(id)).cloned())
            .or_else(|| self.group_items.first().cloned());

        if let Some(item) = next_item {
            self.select_group_item(item, window, cx);
        } else {
            self.selected_item = None;
            self.selected_item_params.clear();
        }
        cx.notify();
    }

    fn reload_variables(&mut self, cx: &mut Context<Self>) {
        let guard = self.conn.lock().unwrap();
        let repo = Repository::new(&guard);
        self.custom_variables = repo.list_variables().unwrap_or_default();
        cx.notify();
    }

    fn select_program(&mut self, prog: Program, cx: &mut Context<Self>) {
        let guard = self.conn.lock().unwrap();
        let repo = Repository::new(&guard);
        self.selected_fields = repo.list_program_fields(prog.id.unwrap_or(0)).unwrap_or_default();
        self.selected_program = Some(prog);
        self.editing_field_id = None;
        cx.notify();
    }

    fn select_group_item(&mut self, item: GroupItem, window: &mut Window, cx: &mut Context<Self>) {
        let params = {
            let guard = self.conn.lock().unwrap();
            let repo = Repository::new(&guard);
            repo.list_item_params_detailed(item.id.unwrap_or(0), item.program_id)
                .unwrap_or_default()
        };

        self.item_exe_time_input
            .update(cx, |s, cx| s.set_value(item.expect_exe_time.clone(), window, cx));
        self.item_exe_date_rule_input
            .update(cx, |s, cx| s.set_value(item.expect_exe_date_rule.clone(), window, cx));
        self.item_pre_sn1_input
            .update(cx, |s, cx| s.set_value(item.pre_job_sn1.clone(), window, cx));
        self.item_pre_sn2_input
            .update(cx, |s, cx| s.set_value(item.pre_job_sn2.clone(), window, cx));
        self.item_pre_sn3_input
            .update(cx, |s, cx| s.set_value(item.pre_job_sn3.clone(), window, cx));
        self.item_rework_sn_input
            .update(cx, |s, cx| s.set_value(item.rework_sn.clone(), window, cx));
        self.item_mail_input
            .update(cx, |s, cx| s.set_value(item.mail_recipients.clone(), window, cx));
        self.item_pre_note_input
            .update(cx, |s, cx| s.set_value(item.pre_work_note.clone(), window, cx));
        self.item_rear_note_input
            .update(cx, |s, cx| s.set_value(item.rear_work_note.clone(), window, cx));

        self.item_req_op = item.req_op_confirm;
        self.item_link_prev = item.link_prev_sn;
        self.item_req_email = item.req_email;
        self.selected_item = Some(item);
        self.selected_item_params = params;
        self.clear_param_editor(window, cx);
        cx.notify();
    }

    fn load_param_into_editor(
        &mut self,
        p: &GroupItemParamDetail,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.param_editing_field_id = Some(p.field_id);
        self.param_field_scope = p.field_scope.clone();
        self.param_rule_code = p.rule_code.clone();
        self.param_label_input
            .update(cx, |s, cx| s.set_value(p.label.clone(), window, cx));
        self.param_html_id_input
            .update(cx, |s, cx| s.set_value(p.html_id.clone(), window, cx));
        self.param_rule_value_input
            .update(cx, |s, cx| s.set_value(p.rule_value.clone(), window, cx));
        cx.notify();
    }

    fn clear_param_editor(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.param_editing_field_id = None;
        self.param_field_scope = "PROGRAM".to_string();
        self.param_rule_code = "E1".to_string();
        self.param_label_input
            .update(cx, |s, cx| s.set_value("", window, cx));
        self.param_html_id_input
            .update(cx, |s, cx| s.set_value("", window, cx));
        self.param_rule_value_input
            .update(cx, |s, cx| s.set_value("", window, cx));
        cx.notify();
    }

    // --- Actions: Program Manager ---
    fn browse_and_import_html(&mut self, window: &mut Window, cx: &mut Context<Self>) {
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

    fn import_html_from_path(&mut self, window: &mut Window, cx: &mut Context<Self>) {
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
                            let ok_msg = format!(
                                "成功匯入批次程式 [{job_code}]，共解析 {field_count} 個欄位！"
                            );
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

    fn save_manual_program(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let code = self
            .prog_code_input
            .read(cx)
            .value()
            .trim()
            .to_uppercase();
        let name = self.prog_name_input.read(cx).value().trim().to_string();
        if code.is_empty() {
            window.push_notification(Notification::error("請輸入工作/程式代號"), cx);
            return;
        }
        let guard = self.conn.lock().unwrap();
        let repo = Repository::new(&guard);
        match repo.create_or_update_program(&code, &name) {
            Ok(id) => {
                self.selected_program = repo.get_program_by_id(id).unwrap_or(None);
                drop(repo);
                drop(guard);
                self.prog_code_input
                    .update(cx, |s, cx| s.set_value("", window, cx));
                self.prog_name_input
                    .update(cx, |s, cx| s.set_value("", window, cx));
                self.reload_programs(cx);
                window.push_notification(
                    Notification::success(format!("已儲存批次程式 [{code}]")),
                    cx,
                );
            }
            Err(e) => {
                window.push_notification(Notification::error(format!("儲存程式失敗：{e}")), cx)
            }
        }
    }

    fn delete_selected_program(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(prog) = self.selected_program.clone() else {
            return;
        };
        let Some(pid) = prog.id else { return };
        {
            let guard = self.conn.lock().unwrap();
            let repo = Repository::new(&guard);
            let _ = repo.delete_program(pid);
        }
        self.selected_program = None;
        self.reload_programs(cx);
        self.reload_groups(window, cx);
        window.push_notification(
            Notification::success(format!("已刪除批次程式 [{}]", prog.job_code)),
            cx,
        );
    }

    fn save_program_field(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(prog_id) = self.selected_program.as_ref().and_then(|p| p.id) else {
            window.push_notification(Notification::error("請先選取批次程式"), cx);
            return;
        };
        let html_id = self.field_id_input.read(cx).value().trim().to_string();
        let label = self.field_label_input.read(cx).value().trim().to_string();
        let max_len = self
            .field_maxlen_input
            .read(cx)
            .value()
            .trim()
            .parse::<usize>()
            .unwrap_or(0);

        if html_id.is_empty() {
            window.push_notification(Notification::error("請輸入欄位 ID (HTML ID)"), cx);
            return;
        }
        let clean_label = if label.is_empty() {
            html_id.clone()
        } else {
            label
        };

        let field = ProgramField {
            id: self.editing_field_id,
            program_id: prog_id,
            field_scope: self.field_scope.clone(),
            html_id: html_id.clone(),
            html_name: html_id.clone(),
            label: clean_label.clone(),
            tag_type: self.field_tag_type.clone(),
            max_length: max_len,
            is_required: self.field_required,
            sort_order: self.selected_fields.len() as i32,
        };

        {
            let guard = self.conn.lock().unwrap();
            let repo = Repository::new(&guard);
            if let Err(e) = repo.upsert_program_field(&field) {
                window.push_notification(Notification::error(format!("儲存欄位失敗：{e}")), cx);
                return;
            }
        }

        self.editing_field_id = None;
        self.field_id_input
            .update(cx, |s, cx| s.set_value("", window, cx));
        self.field_label_input
            .update(cx, |s, cx| s.set_value("", window, cx));
        self.reload_programs(cx);
        self.reload_groups(window, cx);
        window.push_notification(
            Notification::success(format!("已儲存欄位：{clean_label} ({html_id})")),
            cx,
        );
    }

    fn delete_program_field(&mut self, field_id: i64, window: &mut Window, cx: &mut Context<Self>) {
        {
            let guard = self.conn.lock().unwrap();
            let repo = Repository::new(&guard);
            let _ = repo.delete_program_field(field_id);
        }
        self.reload_programs(cx);
        self.reload_groups(window, cx);
        window.push_notification(Notification::success("已刪除欄位定義"), cx);
    }

    // --- Actions: Group Editor ---
    fn create_group(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let name = self
            .new_group_name_input
            .read(cx)
            .value()
            .trim()
            .to_string();
        let desc = self
            .new_group_desc_input
            .read(cx)
            .value()
            .trim()
            .to_string();
        if name.is_empty() {
            window.push_notification(Notification::error("請輸入群組名稱"), cx);
            return;
        }
        let rule_json = serde_json::to_string(&ScheduleRule::default()).unwrap_or_default();
        let gid = {
            let guard = self.conn.lock().unwrap();
            let repo = Repository::new(&guard);
            repo.create_job_group(&JobGroup {
                id: None,
                group_name: name.clone(),
                description: desc,
                group_mode: "MULTI".to_string(),
                schedule_rule_json: rule_json,
                created_at: None,
            })
        };
        match gid {
            Ok(id) => {
                self.new_group_name_input
                    .update(cx, |s, cx| s.set_value("", window, cx));
                self.new_group_desc_input
                    .update(cx, |s, cx| s.set_value("", window, cx));
                let guard = self.conn.lock().unwrap();
                let repo = Repository::new(&guard);
                self.selected_group = repo.get_job_group(id).unwrap_or(None);
                drop(repo);
                drop(guard);
                self.reload_groups(window, cx);
                window.push_notification(
                    Notification::success(format!("已建立排程群組「{name}」")),
                    cx,
                );
            }
            Err(e) => {
                window.push_notification(Notification::error(format!("建立群組失敗：{e}")), cx)
            }
        }
    }

    fn set_group_mode(&mut self, mode: &str, window: &mut Window, cx: &mut Context<Self>) {
        let Some(mut group) = self.selected_group.clone() else {
            return;
        };
        group.group_mode = mode.to_string();
        {
            let guard = self.conn.lock().unwrap();
            let repo = Repository::new(&guard);
            let _ = repo.update_job_group(&group);
        }
        self.reload_groups(window, cx);
        window.push_notification(
            Notification::success(format!("群組執行模式已切換為 {mode}")),
            cx,
        );
    }

    fn delete_selected_group(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(group) = self.selected_group.clone() else {
            return;
        };
        let Some(gid) = group.id else { return };
        {
            let guard = self.conn.lock().unwrap();
            let repo = Repository::new(&guard);
            let _ = repo.delete_job_group(gid);
        }
        self.selected_group = None;
        self.reload_groups(window, cx);
        window.push_notification(
            Notification::success(format!("已刪除群組「{}」", group.group_name)),
            cx,
        );
    }

    fn add_program_to_group(
        &mut self,
        program_id: i64,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(gid) = self.selected_group.as_ref().and_then(|g| g.id) else {
            window.push_notification(Notification::error("請先建立或選取排程群組"), cx);
            return;
        };
        let next_order = (self.group_items.len() + 1) as i32;
        let new_item = GroupItem {
            id: None,
            group_id: gid,
            program_id,
            step_order: next_order,
            expect_exe_date_rule: "GROUP_DATE".to_string(),
            expect_exe_time: "13:00".to_string(),
            req_op_confirm: false,
            link_prev_sn: next_order > 1,
            pre_job_sn1: String::new(),
            pre_job_sn2: String::new(),
            pre_job_sn3: String::new(),
            rework_sn: String::new(),
            req_email: true,
            mail_recipients: String::new(),
            pre_work_note: String::new(),
            rear_work_note: String::new(),
        };
        {
            let guard = self.conn.lock().unwrap();
            let repo = Repository::new(&guard);
            if let Err(e) = repo.add_group_item(&new_item) {
                window.push_notification(
                    Notification::error(format!("加入群組步驟失敗：{e}")),
                    cx,
                );
                return;
            }
        }
        self.reload_groups(window, cx);
        window.push_notification(Notification::success("已加入排程步驟"), cx);
    }

    fn save_selected_item_common(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(mut item) = self.selected_item.clone() else {
            return;
        };
        item.expect_exe_time = self.item_exe_time_input.read(cx).value().trim().to_string();
        item.expect_exe_date_rule = self
            .item_exe_date_rule_input
            .read(cx)
            .value()
            .trim()
            .to_string();
        if item.expect_exe_date_rule.is_empty() {
            item.expect_exe_date_rule = "GROUP_DATE".to_string();
        }
        item.pre_job_sn1 = self.item_pre_sn1_input.read(cx).value().trim().to_string();
        item.pre_job_sn2 = self.item_pre_sn2_input.read(cx).value().trim().to_string();
        item.pre_job_sn3 = self.item_pre_sn3_input.read(cx).value().trim().to_string();
        item.rework_sn = self.item_rework_sn_input.read(cx).value().trim().to_string();
        item.mail_recipients = self.item_mail_input.read(cx).value().trim().to_string();
        item.pre_work_note = self.item_pre_note_input.read(cx).value().trim().to_string();
        item.rear_work_note = self.item_rear_note_input.read(cx).value().trim().to_string();
        item.req_op_confirm = self.item_req_op;
        item.link_prev_sn = self.item_link_prev;
        item.req_email = self.item_req_email;

        {
            let guard = self.conn.lock().unwrap();
            let repo = Repository::new(&guard);
            if let Err(e) = repo.update_group_item(&item) {
                window.push_notification(
                    Notification::error(format!("儲存共同項目失敗：{e}")),
                    cx,
                );
                return;
            }
        }
        self.reload_groups(window, cx);
        window.push_notification(Notification::success("已儲存步驟共同項目設定！"), cx);
    }

    fn save_item_param_with_field(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(item) = self.selected_item.clone() else {
            window.push_notification(Notification::error("請先選取群組步驟"), cx);
            return;
        };
        let Some(item_id) = item.id else { return };
        let html_id = self.param_html_id_input.read(cx).value().trim().to_string();
        let label = self.param_label_input.read(cx).value().trim().to_string();
        let rule_value = self
            .param_rule_value_input
            .read(cx)
            .value()
            .trim()
            .to_string();

        if html_id.is_empty() {
            window.push_notification(
                Notification::error("請輸入欄位 ID (HTML ID) 與欄位名稱"),
                cx,
            );
            return;
        }
        let clean_label = if label.is_empty() {
            html_id.clone()
        } else {
            label
        };

        {
            let guard = self.conn.lock().unwrap();
            let repo = Repository::new(&guard);
            if let Err(e) = repo.set_item_param_with_field(
                item_id,
                item.program_id,
                self.param_editing_field_id,
                &self.param_field_scope,
                &html_id,
                &clean_label,
                &self.param_rule_code,
                &rule_value,
            ) {
                window.push_notification(
                    Notification::error(format!("儲存輸入參數失敗：{e}")),
                    cx,
                );
                return;
            }
        }

        self.reload_programs(cx);
        self.reload_groups(window, cx);
        window.push_notification(
            Notification::success(format!("已儲存輸入參數：{clean_label} [{html_id}]")),
            cx,
        );
    }

    fn delete_item_param(
        &mut self,
        field_id: i64,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(item_id) = self.selected_item.as_ref().and_then(|i| i.id) else {
            return;
        };
        {
            let guard = self.conn.lock().unwrap();
            let repo = Repository::new(&guard);
            let _ = repo.delete_item_param(item_id, field_id);
        }
        self.reload_groups(window, cx);
        window.push_notification(Notification::success("已清除參數規則綁定"), cx);
    }

    // --- Actions: Schedule Generator ---
    fn sync_schedule_inputs_from_rule(&mut self, window: &mut Window, cx: &mut Context<Self>) {
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

    fn apply_schedule_inputs_and_save(&mut self, window: &mut Window, cx: &mut Context<Self>) {
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

    // --- Actions: Settings & Variables ---
    fn save_settings(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let edge_path = self.cfg_edge_path_input.read(cx).value().trim().to_string();
        let target_url = self.cfg_target_url_input.read(cx).value().trim().to_string();
        let pin = self.cfg_pin_input.read(cx).value().trim().to_string();
        let screenshot_dir = self
            .cfg_screenshot_dir_input
            .read(cx)
            .value()
            .trim()
            .to_string();
        let timeout_sec = self.cfg_timeout_input.read(cx).value().trim().to_string();

        let guard = self.conn.lock().unwrap();
        let repo = Repository::new(&guard);
        let pairs = [
            ("edge_path", edge_path),
            ("target_url", target_url),
            ("default_pin", pin),
            ("screenshot_dir", screenshot_dir),
            ("action_timeout_sec", timeout_sec),
        ];
        for (k, v) in pairs {
            if let Err(e) = repo.set_config(k, &v) {
                window.push_notification(
                    Notification::error(format!("儲存設定 {k} 失敗：{e}")),
                    cx,
                );
                return;
            }
        }
        window.push_notification(Notification::success("系統全域設定已成功儲存！"), cx);
    }

    fn save_custom_variable(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let key = self
            .var_key_input
            .read(cx)
            .value()
            .trim()
            .to_uppercase();
        let val = self.var_value_input.read(cx).value().trim().to_string();
        let desc = self.var_desc_input.read(cx).value().trim().to_string();
        if key.is_empty() {
            window.push_notification(Notification::error("請輸入變數鍵名"), cx);
            return;
        }
        {
            let guard = self.conn.lock().unwrap();
            let repo = Repository::new(&guard);
            if let Err(e) = repo.upsert_variable(&CustomVariable {
                var_key: key.clone(),
                var_value: val,
                description: desc,
            }) {
                window.push_notification(Notification::error(format!("儲存變數失敗：{e}")), cx);
                return;
            }
        }
        self.var_key_input
            .update(cx, |s, cx| s.set_value("", window, cx));
        self.var_value_input
            .update(cx, |s, cx| s.set_value("", window, cx));
        self.var_desc_input
            .update(cx, |s, cx| s.set_value("", window, cx));
        self.reload_variables(cx);
        window.push_notification(
            Notification::success(format!("已儲存自訂變數 {{{key}}}")),
            cx,
        );
    }

    fn delete_custom_variable(&mut self, key: &str, window: &mut Window, cx: &mut Context<Self>) {
        {
            let guard = self.conn.lock().unwrap();
            let repo = Repository::new(&guard);
            let _ = repo.delete_variable(key);
        }
        self.reload_variables(cx);
        window.push_notification(Notification::success(format!("已刪除變數 {{{key}}}")), cx);
    }

    // --- Actions: Execution Runner ---
    fn start_execution(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.runner_state.is_running {
            return;
        }
        let Some(group) = self.selected_group.clone() else {
            window.push_notification(Notification::error("請先於「排程群組設定」建立並選取群組"), cx);
            return;
        };

        let (steps, custom_vars) = {
            let guard = self.conn.lock().unwrap();
            let repo = Repository::new(&guard);
            let items = repo.list_group_items(group.id.unwrap_or(0)).unwrap_or_default();
            let mut steps = Vec::new();
            for item in items {
                if let Ok(Some(program)) = repo.get_program_by_id(item.program_id) {
                    let params = repo
                        .list_item_params_detailed(item.id.unwrap_or(0), item.program_id)
                        .unwrap_or_default();
                    steps.push(ExecutionStepInput {
                        item,
                        program,
                        params,
                    });
                }
            }
            let vars = repo.get_variables_map().unwrap_or_default();
            (steps, vars)
        };

        if steps.is_empty() {
            window.push_notification(
                Notification::error("群組內尚無任何程式步驟，請先加入批次程式"),
                cx,
            );
            return;
        }

        let override_date = self
            .runner_override_date_input
            .read(cx)
            .value()
            .trim()
            .to_string();
        let schedule_dates = if !override_date.is_empty() {
            if let Some(d) = RocDateUtils::parse_roc_date(&override_date) {
                vec![GeneratedScheduleItem {
                    roc_date: RocDateUtils::to_roc_yyymmdd(&d, "/"),
                    roc_date_raw: RocDateUtils::to_roc_yyymmdd(&d, ""),
                    exe_time: self.schedule_rule.default_time.clone(),
                }]
            } else {
                window.push_notification(
                    Notification::error("單日測試日期格式錯誤，請使用 YYY/MM/DD（如 115/11/24）"),
                    cx,
                );
                return;
            }
        } else if self.preview_items.is_empty() {
            let today = Local::now().date_naive();
            vec![GeneratedScheduleItem {
                roc_date: RocDateUtils::to_roc_yyymmdd(&today, "/"),
                roc_date_raw: RocDateUtils::to_roc_yyymmdd(&today, ""),
                exe_time: self.schedule_rule.default_time.clone(),
            }]
        } else {
            self.preview_items.clone()
        };

        let config = BrowserRunnerConfig {
            edge_path: self.cfg_edge_path_input.read(cx).value().trim().to_string(),
            target_url: self.cfg_target_url_input.read(cx).value().trim().to_string(),
            pin: self.cfg_pin_input.read(cx).value().trim().to_string(),
            screenshot_dir: self
                .cfg_screenshot_dir_input
                .read(cx)
                .value()
                .trim()
                .to_string(),
            action_timeout_sec: self
                .cfg_timeout_input
                .read(cx)
                .value()
                .trim()
                .parse()
                .unwrap_or(30),
            dry_run: self.runner_dry_run,
        };

        self.runner_state.is_running = true;
        self.runner_state.captured_sns.clear();
        self.runner_state.logs.clear();
        self.runner_state.progress = (0, schedule_dates.len() * steps.len());
        self.runner_state.current_step = "啟動中...".to_string();
        cx.notify();

        let req = ExecutionRequest {
            config,
            group,
            steps,
            schedule_dates,
            custom_vars,
        };

        let (tx, mut rx) = mpsc::unbounded_channel::<RunnerEvent>();
        self.tokio_rt.spawn(async move {
            execute_group(req, tx).await;
        });

        cx.spawn(async move |this: WeakEntity<Self>, cx: &mut AsyncApp| {
            loop {
                cx.background_executor()
                    .timer(Duration::from_millis(50))
                    .await;

                let mut drained = Vec::new();
                let mut disconnected = false;
                loop {
                    match rx.try_recv() {
                        Ok(ev) => drained.push(ev),
                        Err(mpsc::error::TryRecvError::Empty) => break,
                        Err(mpsc::error::TryRecvError::Disconnected) => {
                            disconnected = true;
                            break;
                        }
                    }
                }

                if !drained.is_empty() {
                    let mut still_running = true;
                    let updated = this.update(cx, |this, cx| {
                        for ev in drained {
                            this.runner_state.handle_event(ev);
                        }
                        still_running = this.runner_state.is_running;
                        cx.notify();
                    });
                    if updated.is_err() || !still_running || disconnected {
                        break;
                    }
                } else if disconnected {
                    let _ = this.update(cx, |this, cx| {
                        this.runner_state.is_running = false;
                        cx.notify();
                    });
                    break;
                }
            }
        })
        .detach();
    }

    fn build_resolver(&self, cx: &Context<Self>) -> ParamResolver {
        let date_str = self.tester_date_input.read(cx).value().to_string();
        let base_date = RocDateUtils::parse_roc_date(&date_str)
            .unwrap_or_else(|| Local::now().date_naive());
        let vars: HashMap<String, String> = self
            .custom_variables
            .iter()
            .map(|v| (v.var_key.clone(), v.var_value.clone()))
            .collect();
        ParamResolver::new(base_date).with_custom_vars(vars)
    }
}

impl Render for BatchSchedulerApp {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let active = self.active_tab;

        v_flex()
            .size_full()
            .bg(BACKGROUND)
            .text_color(FOREGROUND)
            // Top Header Bar
            .child(
                h_flex()
                    .h(px(54.))
                    .px_6()
                    .bg(CARD)
                    .border_b_1()
                    .border_color(BORDER)
                    .items_center()
                    .justify_between()
                    .child(
                        h_flex()
                            .gap_3()
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
                            .gap_4()
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
                                        "程式: {} 支 | 群組: {} 組",
                                        self.programs.len(),
                                        self.groups.len()
                                    )),
                            ),
                    ),
            )
            // Main Body
            .child(
                h_flex()
                    .flex_1()
                    .min_h_0()
                    .child(
                        v_flex()
                            .w(px(240.))
                            .h_full()
                            .bg(CARD)
                            .border_r_1()
                            .border_color(BORDER)
                            .p_3()
                            .gap_2()
                            .child(nav_button(
                                "nav-prog",
                                "批次程式管理",
                                "程式主檔與欄位定義",
                                active == NavTab::ProgramManager,
                                cx.listener(|this, _: &ClickEvent, _, cx| {
                                    this.active_tab = NavTab::ProgramManager;
                                    cx.notify();
                                }),
                            ))
                            .child(nav_button(
                                "nav-group",
                                "排程群組設定",
                                "共同項目與欄位參數",
                                active == NavTab::GroupEditor,
                                cx.listener(|this, _: &ClickEvent, _, cx| {
                                    this.active_tab = NavTab::GroupEditor;
                                    cx.notify();
                                }),
                            ))
                            .child(nav_button(
                                "nav-sched",
                                "排程時間生成器",
                                "圖 2 時間規則與展開",
                                active == NavTab::ScheduleRule,
                                cx.listener(|this, _: &ClickEvent, _, cx| {
                                    this.active_tab = NavTab::ScheduleRule;
                                    cx.notify();
                                }),
                            ))
                            .child(nav_button(
                                "nav-runner",
                                "自動化執行監控台",
                                "Edge CDP 雙軌執行",
                                active == NavTab::ExecutionRunner,
                                cx.listener(|this, _: &ClickEvent, _, cx| {
                                    this.active_tab = NavTab::ExecutionRunner;
                                    cx.notify();
                                }),
                            ))
                            .child(nav_button(
                                "nav-settings",
                                "系統全域設定",
                                "Edge、PIN 與自訂變數",
                                active == NavTab::Settings,
                                cx.listener(|this, _: &ClickEvent, _, cx| {
                                    this.active_tab = NavTab::Settings;
                                    cx.notify();
                                }),
                            )),
                    )
                    .child(
                        div()
                            .id("workspace-scroll-area")
                            .flex_1()
                            .min_h_0()
                            .overflow_y_scroll()
                            .p_6()
                            .child(match self.active_tab {
                                NavTab::ProgramManager => {
                                    self.render_program_view(cx).into_any_element()
                                }
                                NavTab::GroupEditor => self.render_group_view(cx).into_any_element(),
                                NavTab::ScheduleRule => {
                                    self.render_schedule_view(cx).into_any_element()
                                }
                                NavTab::ExecutionRunner => {
                                    self.render_runner_view(cx).into_any_element()
                                }
                                NavTab::Settings => self.render_settings_view(cx).into_any_element(),
                            }),
                    ),
            )
            .children(Root::render_notification_layer(window, cx))
    }
}

// ---------------- View Implementations ----------------

impl BatchSchedulerApp {
    fn render_program_view(&self, cx: &mut Context<Self>) -> impl IntoElement {
        v_flex()
            .gap_5()
            .child(
                h_flex()
                    .justify_between()
                    .items_center()
                    .gap_4()
                    .child(
                        v_flex()
                            .gap_1()
                            .child(
                                div()
                                    .text_xl()
                                    .font_weight(FontWeight::BOLD)
                                    .child("批次程式與輸入欄位定義管理"),
                            )
                            .child(
                                div()
                                    .text_sm()
                                    .text_color(MUTED_FOREGROUND)
                                    .child("支援匯入任意 YCM321-3W2 HTML 頁面或人工維護程式與欄位（欄位名稱 ＋ 欄位 ID）"),
                            ),
                    )
                    .child(
                        h_flex()
                            .gap_2()
                            .items_center()
                            .child(div().w(px(260.)).child(Input::new(&self.html_path_input)))
                            .child(
                                Button::new("btn-browse-import-html")
                                    .primary()
                                    .label("選擇檔案並匯入")
                                    .on_click(cx.listener(|this, _: &ClickEvent, window, cx| {
                                        this.browse_and_import_html(window, cx);
                                    })),
                            )
                            .child(
                                Button::new("btn-import-html")
                                    .ghost()
                                    .label("依路徑匯入")
                                    .on_click(cx.listener(|this, _: &ClickEvent, window, cx| {
                                        this.import_html_from_path(window, cx);
                                    })),
                            ),
                    ),
            )
            // Manual Program Add Bar
            .child(
                h_flex()
                    .bg(CARD)
                    .p_3()
                    .rounded_lg()
                    .border_1()
                    .border_color(BORDER)
                    .gap_3()
                    .items_center()
                    .child(div().text_xs().font_weight(FontWeight::SEMIBOLD).child("新增/更新程式："))
                    .child(div().w(px(180.)).child(Input::new(&self.prog_code_input)))
                    .child(div().flex_1().child(Input::new(&self.prog_name_input)))
                    .child(
                        Button::new("btn-add-prog")
                            .primary()
                            .label("儲存程式")
                            .on_click(cx.listener(|this, _: &ClickEvent, window, cx| {
                                this.save_manual_program(window, cx);
                            })),
                    ),
            )
            // Two columns: Program List + Fields Manager
            .child(
                h_flex()
                    .gap_5()
                    .items_start()
                    .child(
                        v_flex()
                            .w(px(320.))
                            .bg(CARD)
                            .p_4()
                            .rounded_lg()
                            .border_1()
                            .border_color(BORDER)
                            .gap_3()
                            .child(
                                h_flex()
                                    .justify_between()
                                    .items_center()
                                    .child(div().font_weight(FontWeight::SEMIBOLD).child("已登錄批次程式"))
                                    .child(if self.selected_program.is_some() {
                                        Button::new("btn-del-prog")
                                            .danger()
                                            .label("刪除程式")
                                            .on_click(cx.listener(|this, _: &ClickEvent, window, cx| {
                                                this.delete_selected_program(window, cx);
                                            }))
                                            .into_any_element()
                                    } else {
                                        div().into_any_element()
                                    }),
                            )
                            .child(if self.programs.is_empty() {
                                div()
                                    .text_sm()
                                    .text_color(MUTED_FOREGROUND)
                                    .p_4()
                                    .child("尚無登錄程式，請匯入 YCM HTML 或於上方新增")
                            } else {
                                let mut list = v_flex().gap_2();
                                for p in &self.programs {
                                    let is_sel = self
                                        .selected_program
                                        .as_ref()
                                        .map(|sp| sp.id == p.id)
                                        .unwrap_or(false);
                                    let prog_clone = p.clone();
                                    list = list.child(
                                        div()
                                            .id(SharedString::from(format!("prog-{}", p.job_code)))
                                            .p_3()
                                            .rounded_md()
                                            .bg(if is_sel { SECONDARY } else { BACKGROUND })
                                            .border_1()
                                            .border_color(if is_sel { PRIMARY } else { BORDER })
                                            .cursor_pointer()
                                            .on_click(cx.listener(move |this, _: &ClickEvent, _, cx| {
                                                this.select_program(prog_clone.clone(), cx);
                                            }))
                                            .child(
                                                v_flex()
                                                    .gap_1()
                                                    .child(
                                                        div()
                                                            .font_weight(FontWeight::BOLD)
                                                            .child(p.job_code.clone()),
                                                    )
                                                    .child(
                                                        div()
                                                            .text_xs()
                                                            .text_color(MUTED_FOREGROUND)
                                                            .child(p.job_name.clone()),
                                                    ),
                                            ),
                                    );
                                }
                                list
                            }),
                    )
                    // Right: Fields Editor + Table
                    .child(
                        v_flex()
                            .flex_1()
                            .bg(CARD)
                            .p_4()
                            .rounded_lg()
                            .border_1()
                            .border_color(BORDER)
                            .gap_3()
                            .child(
                                h_flex()
                                    .justify_between()
                                    .items_center()
                                    .child(div().font_weight(FontWeight::SEMIBOLD).child(format!(
                                        "輸入欄位定義 ({})",
                                        self.selected_program
                                            .as_ref()
                                            .map(|p| p.job_code.as_str())
                                            .unwrap_or("未選取")
                                    )))
                                    .child(
                                        Tag::primary()
                                            .child(format!("{} 個欄位", self.selected_fields.len())),
                                    ),
                            )
                            // Add / Edit Field Form
                            .child(
                                v_flex()
                                    .p_3()
                                    .bg(BACKGROUND)
                                    .rounded_md()
                                    .border_1()
                                    .border_color(BORDER)
                                    .gap_2()
                                    .child(
                                        h_flex()
                                            .gap_2()
                                            .items_center()
                                            .child(div().text_xs().font_weight(FontWeight::SEMIBOLD).child(
                                                if self.editing_field_id.is_some() {
                                                    "編輯欄位 (欄位名稱 ＋ 欄位 ID)："
                                                } else {
                                                    "新增欄位 (欄位名稱 ＋ 欄位 ID)："
                                                },
                                            ))
                                            .child(scope_pill("PROGRAM", &self.field_scope, cx.listener(|this, _, _, cx| {
                                                this.field_scope = "PROGRAM".to_string();
                                                cx.notify();
                                            })))
                                            .child(scope_pill("JOB", &self.field_scope, cx.listener(|this, _, _, cx| {
                                                this.field_scope = "JOB".to_string();
                                                cx.notify();
                                            })))
                                            .child(Checkbox::new("chk-fld-req").label("必填").checked(self.field_required).on_click(
                                                cx.listener(|this, checked, _, cx| {
                                                    this.field_required = *checked;
                                                    cx.notify();
                                                }),
                                            )),
                                    )
                                    .child(
                                        h_flex()
                                            .gap_2()
                                            .items_center()
                                            .child(div().flex_1().child(Input::new(&self.field_label_input)))
                                            .child(div().w(px(210.)).child(Input::new(&self.field_id_input)))
                                            .child(div().w(px(70.)).child(Input::new(&self.field_maxlen_input)))
                                            .child(
                                                Button::new("btn-save-field")
                                                    .primary()
                                                    .label("儲存欄位")
                                                    .on_click(cx.listener(|this, _: &ClickEvent, window, cx| {
                                                        this.save_program_field(window, cx);
                                                    })),
                                            ),
                                    ),
                            )
                            // Fields Table
                            .child(if self.selected_fields.is_empty() {
                                div()
                                    .text_sm()
                                    .text_color(MUTED_FOREGROUND)
                                    .p_4()
                                    .child("目前無欄位定義，請匯入 HTML 或於上方手動新增欄位名稱與 ID")
                            } else {
                                let mut table = v_flex().gap_2();
                                table = table.child(
                                    h_flex()
                                        .p_2()
                                        .bg(BACKGROUND)
                                        .rounded_md()
                                        .text_xs()
                                        .font_weight(FontWeight::BOLD)
                                        .text_color(MUTED_FOREGROUND)
                                        .child(div().flex_1().child("欄位名稱 (Label)"))
                                        .child(div().w(px(170.)).child("欄位 ID (HTML ID)"))
                                        .child(div().w(px(80.)).child("所屬區塊"))
                                        .child(div().w(px(60.)).child("必填"))
                                        .child(div().w(px(120.)).child("操作")),
                                );
                                for f in &self.selected_fields {
                                    let f_edit = f.clone();
                                    let fid = f.id.unwrap_or(0);
                                    table = table.child(
                                        h_flex()
                                            .p_2()
                                            .bg(BACKGROUND)
                                            .rounded_md()
                                            .border_1()
                                            .border_color(BORDER)
                                            .text_sm()
                                            .items_center()
                                            .gap_2()
                                            .child(div().flex_1().child(f.label.clone()))
                                            .child(
                                                div()
                                                    .w(px(170.))
                                                    .font_weight(FontWeight::MEDIUM)
                                                    .child(f.html_id.clone()),
                                            )
                                            .child(
                                                div()
                                                    .w(px(80.))
                                                    .child(Tag::secondary().child(f.field_scope.clone())),
                                            )
                                            .child(div().w(px(60.)).child(if f.is_required {
                                                Tag::danger().child("必填")
                                            } else {
                                                Tag::secondary().child("選填")
                                            }))
                                            .child(
                                                h_flex()
                                                    .w(px(120.))
                                                    .gap_1()
                                                    .child(
                                                        Button::new(SharedString::from(format!("edit-fld-{fid}")))
                                                            .ghost()
                                                            .label("編輯")
                                                            .on_click(cx.listener(move |this, _: &ClickEvent, window, cx| {
                                                                this.editing_field_id = f_edit.id;
                                                                this.field_scope = f_edit.field_scope.clone();
                                                                this.field_tag_type = f_edit.tag_type.clone();
                                                                this.field_required = f_edit.is_required;
                                                                this.field_label_input.update(cx, |s, cx| {
                                                                    s.set_value(f_edit.label.clone(), window, cx)
                                                                });
                                                                this.field_id_input.update(cx, |s, cx| {
                                                                    s.set_value(f_edit.html_id.clone(), window, cx)
                                                                });
                                                                this.field_maxlen_input.update(cx, |s, cx| {
                                                                    s.set_value(f_edit.max_length.to_string(), window, cx)
                                                                });
                                                                cx.notify();
                                                            })),
                                                    )
                                                    .child(
                                                        Button::new(SharedString::from(format!("del-fld-{fid}")))
                                                            .danger()
                                                            .label("刪")
                                                            .on_click(cx.listener(move |this, _: &ClickEvent, window, cx| {
                                                                this.delete_program_field(fid, window, cx);
                                                            })),
                                                    ),
                                            ),
                                    );
                                }
                                table
                            }),
                    ),
            )
    }

    fn render_group_view(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let resolver = self.build_resolver(cx);
        let test_expr = self.tester_expr_input.read(cx).value().to_string();
        let test_result = resolver.resolve_template(&test_expr);

        let prog_map: HashMap<i64, &Program> = self
            .programs
            .iter()
            .filter_map(|p| p.id.map(|id| (id, p)))
            .collect();

        v_flex()
            .gap_5()
            .child(
                v_flex()
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
                    .bg(CARD)
                    .p_4()
                    .rounded_lg()
                    .border_1()
                    .border_color(BORDER)
                    .gap_3()
                    .child(
                        h_flex()
                            .gap_2()
                            .items_center()
                            .child(div().w(px(220.)).child(Input::new(&self.new_group_name_input)))
                            .child(div().flex_1().child(Input::new(&self.new_group_desc_input)))
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
                        let mut grp_tabs = h_flex().gap_2().flex_wrap().items_center();
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
                h_flex()
                    .gap_5()
                    .items_start()
                    // Left: Steps + Editor
                    .child(
                        v_flex()
                            .flex_1()
                            .gap_4()
                            // Mode + Add Program Step
                            .child(
                                v_flex()
                                    .bg(CARD)
                                    .p_4()
                                    .rounded_lg()
                                    .border_1()
                                    .border_color(BORDER)
                                    .gap_3()
                                    .child(
                                        h_flex()
                                            .justify_between()
                                            .items_center()
                                            .child(
                                                div()
                                                    .font_weight(FontWeight::SEMIBOLD)
                                                    .child(format!("群組步驟編排：{}", group.group_name)),
                                            )
                                            .child(
                                                h_flex()
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
                            }),
                    )
                    // Right: Figure 1 Quick Reference & Live Tester
                    .child(
                        v_flex()
                            .w(px(330.))
                            .gap_4()
                            .child(
                                v_flex()
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
                            ),
                    )
                    .into_any_element()
            } else {
                div().into_any_element()
            })
    }

    fn render_step_detail_editor(
        &self,
        step_code: &str,
        resolver: &ParamResolver,
        cx: &mut Context<Self>,
    ) -> impl IntoElement {
        let param_val_str = self.param_rule_value_input.read(cx).value().to_string();
        let preview_param_val = resolver.resolve(&self.param_rule_code, &param_val_str);

        v_flex()
            .bg(CARD)
            .p_4()
            .rounded_lg()
            .border_1()
            .border_color(BORDER)
            .gap_4()
            .child(
                h_flex()
                    .justify_between()
                    .items_center()
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
                    .gap_3()
                    .child(labeled_input("預計執行時間 (HH:mm)", &self.item_exe_time_input))
                    .child(labeled_input("日期規則 (GROUP_DATE/WEEK_5)", &self.item_exe_date_rule_input))
                    .child(labeled_input("Mail 收件者 (#mailRecipients)", &self.item_mail_input)),
            )
            .child(
                h_flex()
                    .gap_3()
                    .child(labeled_input("前置序號 1 (#preJobSn1)", &self.item_pre_sn1_input))
                    .child(labeled_input("前置序號 2 (#preJobSn2)", &self.item_pre_sn2_input))
                    .child(labeled_input("前置序號 3 (#preJobSn3)", &self.item_pre_sn3_input))
                    .child(labeled_input("重作序號 (#reWorkSn)", &self.item_rework_sn_input)),
            )
            .child(
                h_flex()
                    .gap_3()
                    .child(labeled_input("前置備註 (#preWork)", &self.item_pre_note_input))
                    .child(labeled_input("後置備註 (#rearWork)", &self.item_rear_note_input)),
            )
            .child(
                h_flex()
                    .gap_5()
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
                    .pt_3()
                    .border_t_1()
                    .border_color(BORDER)
                    .gap_3()
                    .child(
                        h_flex()
                            .justify_between()
                            .items_center()
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
                            .p_3()
                            .bg(BACKGROUND)
                            .rounded_md()
                            .border_1()
                            .border_color(BORDER)
                            .gap_2()
                            .child(
                                h_flex()
                                    .gap_2()
                                    .items_end()
                                    .child(labeled_input("欄位名稱 (Label)", &self.param_label_input))
                                    .child(labeled_input("欄位 ID (HTML ID)", &self.param_html_id_input))
                                    .child(labeled_input("規則值 / 樣板", &self.param_rule_value_input)),
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

    fn render_schedule_view(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let weekday_names = ["週一", "週二", "週三", "週四", "週五", "週六", "週日"];

        v_flex()
            .gap_5()
            .child(
                h_flex()
                    .justify_between()
                    .items_center()
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
                                    .child("設定選定群組之年度別、時間區間與每月/每週/指定日期規則，右側即時展開排程清單"),
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
            .child(
                h_flex()
                    .gap_5()
                    .items_start()
                    // Left Controls
                    .child(
                        v_flex()
                            .w(px(400.))
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
                            )))
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
                            .child(labeled_input("4. 預計執行時間 (HH:mm)", &self.sched_default_time_input)),
                    )
                    // Right Preview Table
                    .child(
                        v_flex()
                            .flex_1()
                            .bg(CARD)
                            .p_4()
                            .rounded_lg()
                            .border_1()
                            .border_color(BORDER)
                            .gap_3()
                            .child(
                                h_flex()
                                    .justify_between()
                                    .items_center()
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
                                    .max_h(px(480.))
                                    .overflow_y_scroll()
                                    .child({
                                        let mut preview_rows = v_flex().gap_1();
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
                            ),
                    ),
            )
    }

    fn render_runner_view(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let (curr, total) = self.runner_state.progress;
        let ratio = if total > 0 {
            (curr as f32 / total as f32).clamp(0.0, 1.0)
        } else {
            0.0
        };

        v_flex()
            .gap_5()
            .child(
                h_flex()
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
                                    .child("自動化執行監控台"),
                            )
                            .child(div().text_sm().text_color(MUTED_FOREGROUND).child(format!(
                                "目前群組：{} | 雙軌狀態機 (Windows UI Automation 操作桌面 Edge，免 WebView2 / 免遠端除錯)",
                                self.selected_group
                                    .as_ref()
                                    .map(|g| g.group_name.as_str())
                                    .unwrap_or("未選取")
                            ))),
                    )
                    .child(
                        h_flex()
                            .gap_3()
                            .items_center()
                            .child(
                                Checkbox::new("chk-dry-run")
                                    .label("預演模式 (Dry-Run，不啟動瀏覽器)")
                                    .checked(self.runner_dry_run)
                                    .on_click(cx.listener(|this, chk, _, cx| {
                                        this.runner_dry_run = *chk;
                                        cx.notify();
                                    })),
                            )
                            .child(div().w(px(230.)).child(Input::new(&self.runner_override_date_input)))
                            .child(
                                Button::new("btn-start-run")
                                    .primary()
                                    .label(if self.runner_state.is_running {
                                        "執行中..."
                                    } else {
                                        "開始執行排程作業"
                                    })
                                    .on_click(cx.listener(|this, _: &ClickEvent, window, cx| {
                                        this.start_execution(window, cx);
                                    })),
                            ),
                    ),
            )
            // Progress Bar
            .child(
                v_flex()
                    .bg(CARD)
                    .p_4()
                    .rounded_lg()
                    .border_1()
                    .border_color(BORDER)
                    .gap_3()
                    .child(
                        h_flex()
                            .justify_between()
                            .items_center()
                            .child(
                                h_flex()
                                    .gap_2()
                                    .items_center()
                                    .child(div().font_weight(FontWeight::SEMIBOLD).child("執行進度"))
                                    .child(Tag::secondary().child(self.runner_state.current_step.clone())),
                            )
                            .child(div().text_xs().child(format!("{curr}/{total} 完成"))),
                    )
                    .child(
                        div()
                            .w_full()
                            .h(px(10.))
                            .bg(BACKGROUND)
                            .rounded_full()
                            .child(
                                div()
                                    .h_full()
                                    .w(px((ratio * 800.0).max(if curr > 0 { 12.0 } else { 0.0 })))
                                    .bg(SUCCESS)
                                    .rounded_full(),
                            ),
                    ),
            )
            // Captured SN Badges
            .child(if self.runner_state.captured_sns.is_empty() {
                div().into_any_element()
            } else {
                let mut row = h_flex().gap_2().flex_wrap().p_3().bg(CARD).rounded_lg().border_1().border_color(BORDER);
                for item in &self.runner_state.captured_sns {
                    row = row.child(Tag::success().child(format!("{} #{}", item.job_code, item.sn)));
                }
                row.into_any_element()
            })
            // Log Stream
            .child(
                v_flex()
                    .flex_1()
                    .bg(BACKGROUND)
                    .p_4()
                    .rounded_lg()
                    .border_1()
                    .border_color(BORDER)
                    .gap_2()
                    .child(
                        h_flex()
                            .justify_between()
                            .items_center()
                            .child(
                                div()
                                    .text_xs()
                                    .font_weight(FontWeight::BOLD)
                                    .text_color(MUTED_FOREGROUND)
                                    .child("即時執行日誌串流 (Real-time Log Stream)"),
                            )
                            .child(
                                div()
                                    .text_xs()
                                    .text_color(MUTED_FOREGROUND)
                                    .child(format!("實體日誌檔：{}", current_log_file_path())),
                            ),
                    )
                    .child({
                        let mut logs = v_flex().gap_1();
                        for log in &self.runner_state.logs {
                            let color = if log.contains("ERROR") || log.contains("FAILED") {
                                DANGER
                            } else if log.contains("SUCCESS") || log.contains("DONE") {
                                SUCCESS
                            } else if log.contains("SCREENSHOT") || log.contains("WARN") {
                                WARNING
                            } else {
                                FOREGROUND
                            };
                            logs = logs.child(
                                div()
                                    .text_xs()
                                    .font_family(SharedString::from(
                                        "Consolas, Courier New, monospace",
                                    ))
                                    .text_color(color)
                                    .child(log.clone()),
                            );
                        }
                        logs
                    }),
            )
    }

    fn render_settings_view(&self, cx: &mut Context<Self>) -> impl IntoElement {
        h_flex()
            .gap_5()
            .items_start()
            // Left: System Global Config
            .child(
                v_flex()
                    .w(px(520.))
                    .bg(CARD)
                    .p_6()
                    .rounded_lg()
                    .border_1()
                    .border_color(BORDER)
                    .gap_4()
                    .child(div().text_xl().font_weight(FontWeight::BOLD).child("系統全域設定"))
                    .child(labeled_input("Edge 瀏覽器執行檔路徑 (edge_path，UIA 模式使用)", &self.cfg_edge_path_input))
                    .child(labeled_input("YCM 目標網址 (target_url)", &self.cfg_target_url_input))
                    .child(labeled_input("預設 PIN 碼 (default_pin)", &self.cfg_pin_input))
                    .child(labeled_input("截圖存檔目錄 (screenshot_dir)", &self.cfg_screenshot_dir_input))
                    .child(labeled_input("元素等待逾時秒數 (action_timeout_sec)", &self.cfg_timeout_input))
                    .child(
                        Button::new("btn-save-settings")
                            .primary()
                            .label("儲存全域設定")
                            .on_click(cx.listener(|this, _: &ClickEvent, window, cx| {
                                this.save_settings(window, cx);
                            })),
                    ),
            )
            // Right: Custom Variable Dictionary
            .child(
                v_flex()
                    .flex_1()
                    .bg(CARD)
                    .p_6()
                    .rounded_lg()
                    .border_1()
                    .border_color(BORDER)
                    .gap_4()
                    .child(div().text_xl().font_weight(FontWeight::BOLD).child("自訂動態變數字典"))
                    .child(
                        div()
                            .text_xs()
                            .text_color(MUTED_FOREGROUND)
                            .child("用於樣板中的 {KEY} 替換（例如設定 MONTH_MAIL_01 ~ 12 後，即可使用 {MONTH_MAIL} 自動對應當月值）"),
                    )
                    .child(
                        h_flex()
                            .gap_2()
                            .items_end()
                            .child(labeled_input("變數鍵名", &self.var_key_input))
                            .child(labeled_input("變數值", &self.var_value_input))
                            .child(labeled_input("說明", &self.var_desc_input))
                            .child(
                                Button::new("btn-save-var")
                                    .primary()
                                    .label("儲存變數")
                                    .on_click(cx.listener(|this, _: &ClickEvent, window, cx| {
                                        this.save_custom_variable(window, cx);
                                    })),
                            ),
                    )
                    .child(if self.custom_variables.is_empty() {
                        div()
                            .text_xs()
                            .text_color(MUTED_FOREGROUND)
                            .p_3()
                            .bg(BACKGROUND)
                            .rounded_md()
                            .child("目前無自訂變數")
                    } else {
                        let mut list = v_flex().gap_2();
                        for v in &self.custom_variables {
                            let k = v.var_key.clone();
                            list = list.child(
                                h_flex()
                                    .p_2()
                                    .bg(BACKGROUND)
                                    .rounded_md()
                                    .border_1()
                                    .border_color(BORDER)
                                    .justify_between()
                                    .items_center()
                                    .text_xs()
                                    .child(
                                        h_flex()
                                            .gap_3()
                                            .child(Tag::primary().child(format!("{{{}}}", v.var_key)))
                                            .child(div().child(v.var_value.clone()))
                                            .child(div().text_color(MUTED_FOREGROUND).child(v.description.clone())),
                                    )
                                    .child(
                                        Button::new(SharedString::from(format!("del-var-{}", v.var_key)))
                                            .danger()
                                            .label("刪除")
                                            .on_click(cx.listener(move |this, _: &ClickEvent, window, cx| {
                                                this.delete_custom_variable(&k, window, cx);
                                            })),
                                    ),
                            );
                        }
                        list
                    }),
            )
    }
}

// ---------------- Helper Components ----------------

fn nav_button(
    id: &'static str,
    title: &'static str,
    subtitle: &'static str,
    is_active: bool,
    on_click: impl Fn(&ClickEvent, &mut Window, &mut gpui::App) + 'static,
) -> impl IntoElement {
    div()
        .id(SharedString::from(id))
        .p_3()
        .rounded_md()
        .bg(if is_active { PRIMARY } else { CARD })
        .text_color(if is_active {
            hsl(0.0, 0.0, 100.0)
        } else {
            FOREGROUND
        })
        .cursor_pointer()
        .on_click(on_click)
        .child(
            v_flex()
                .gap_1()
                .child(div().font_weight(FontWeight::SEMIBOLD).text_sm().child(title))
                .child(
                    div()
                        .text_xs()
                        .text_color(if is_active {
                            hsl(0.0, 0.0, 90.0)
                        } else {
                            MUTED_FOREGROUND
                        })
                        .child(subtitle),
                ),
        )
}

fn labeled_input(label: &'static str, state: &Entity<InputState>) -> impl IntoElement {
    v_flex()
        .flex_1()
        .gap_1()
        .child(
            div()
                .text_xs()
                .text_color(MUTED_FOREGROUND)
                .child(label),
        )
        .child(Input::new(state))
}

fn scope_pill(
    label: &'static str,
    current: &str,
    on_click: impl Fn(&ClickEvent, &mut Window, &mut gpui::App) + 'static,
) -> impl IntoElement {
    let active = current == label;
    div()
        .id(SharedString::from(format!("scope-{label}")))
        .px_2()
        .py_1()
        .rounded_md()
        .bg(if active { PRIMARY } else { SECONDARY })
        .border_1()
        .border_color(if active { PRIMARY } else { BORDER })
        .text_xs()
        .cursor_pointer()
        .on_click(on_click)
        .child(label)
}

fn mode_pill(
    id: impl Into<SharedString>,
    label: &str,
    active: bool,
    on_click: impl Fn(&ClickEvent, &mut Window, &mut gpui::App) + 'static,
) -> impl IntoElement {
    div()
        .id(id.into())
        .px_2()
        .py_1()
        .rounded_md()
        .bg(if active { PRIMARY } else { SECONDARY })
        .border_1()
        .border_color(if active { PRIMARY } else { BORDER })
        .text_xs()
        .cursor_pointer()
        .on_click(on_click)
        .child(label.to_string())
}

fn rule_badge(code: &'static str, desc: &'static str) -> impl IntoElement {
    h_flex()
        .p_2()
        .bg(BACKGROUND)
        .rounded_md()
        .items_center()
        .gap_2()
        .text_xs()
        .child(Tag::primary().child(code))
        .child(div().child(desc))
}
