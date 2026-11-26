//! 狀態初始化、資料重新載入與選取邏輯

use super::*;
use chrono::Datelike;

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
            field_rows: Vec::new(),
            field_rows_dirty: true,
            open_picker: None,
            picker_closed_by: None,
            picker_focus: cx.focus_handle(),
            default_values: Vec::new(),
            field_defaults: HashMap::new(),
            prog_stats: HashMap::new(),

            groups: Vec::new(),
            selected_group: None,
            group_items: Vec::new(),
            selected_item: None,
            selected_item_params: Vec::new(),
            new_group_name_input: cx
                .new(|cx| InputState::new(window, cx).placeholder("新排程群組名稱")),
            new_group_desc_input: cx
                .new(|cx| InputState::new(window, cx).placeholder("群組說明")),
            exec_entries: Vec::new(),
            exec_sel_groups: HashSet::new(),
            exec_sel_programs: HashSet::new(),
            exec_year: Some(115),
            exec_range_start_input: cx
                .new(|cx| InputState::new(window, cx).placeholder("YYY/MM/DD")),
            exec_range_end_input: cx
                .new(|cx| InputState::new(window, cx).placeholder("YYY/MM/DD")),
            exec_date_mode: 3,
            exec_monthly_input: cx
                .new(|cx| InputState::new(window, cx).placeholder("每月日期")),
            exec_weekdays: [false; 7],
            exec_preview_date_idx: 0,
            exec_preview_overview: false,
            exec_reviewed: HashSet::new(),
            exec_console_mode: false,
            exec_run_dates: Vec::new(),
            exec_console_tab: None,
            exec_cal_open: None,
            exec_cal_ym: (Local::now().date_naive().year(), Local::now().date_naive().month()),
            exec_cal_closed_by: None,
            exec_specific_inputs: (0..5)
                .map(|_| {
                    cx.new(|cx| InputState::new(window, cx).placeholder("YYY/MM/DD"))
                })
                .collect(),
            gs_selected_program: None,
            gs_time_input: cx.new(|cx| {
                InputState::new(window, cx)
                    .placeholder("HH:MM")
                    .default_value("13:00")
            }),
            gs_link_prev: false,
            gs_req_email: true,
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
                    .masked(true)
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
        };

        app.reload_all(window, cx);
        app.fill_program_inputs(window, cx);
        app
    }

    pub(super) fn reload_all(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.reload_programs(cx);
        self.reload_groups(window, cx);
        self.reload_variables(cx);
    }

    pub(super) fn reload_programs(&mut self, cx: &mut Context<Self>) {
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
                self.field_defaults = repo.field_defaults_for_program(pid).unwrap_or_default();
            } else {
                self.selected_fields.clear();
                self.field_defaults.clear();
            }
            self.default_values = repo.list_default_values().unwrap_or_default();
            self.prog_stats = repo.program_stats().unwrap_or_default();
            self.field_rows_dirty = true;
            cx.notify();
        }
    }

    pub(super) fn reload_groups(&mut self, window: &mut Window, cx: &mut Context<Self>) {
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

    pub(super) fn reload_variables(&mut self, cx: &mut Context<Self>) {
        let guard = self.conn.lock().unwrap();
        let repo = Repository::new(&guard);
        self.custom_variables = repo.list_variables().unwrap_or_default();
        cx.notify();
    }

    pub(super) fn select_program(&mut self, prog: Program, window: &mut Window, cx: &mut Context<Self>) {
        {
            let guard = self.conn.lock().unwrap();
            let repo = Repository::new(&guard);
            let pid = prog.id.unwrap_or(0);
            self.selected_fields = repo.list_program_fields(pid).unwrap_or_default();
            self.field_defaults = repo.field_defaults_for_program(pid).unwrap_or_default();
        }
        self.selected_program = Some(prog);
        self.field_rows_dirty = true;
        self.fill_program_inputs(window, cx);
        cx.notify();
    }

    /// 把目前選取的程式帶入上方「工作代號 / 工作名稱」輸入框
    pub(super) fn fill_program_inputs(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let (code, name) = self
            .selected_program
            .as_ref()
            .map(|p| (p.job_code.clone(), p.job_name.clone()))
            .unwrap_or_default();
        self.prog_code_input
            .update(cx, |s, cx| s.set_value(code, window, cx));
        self.prog_name_input
            .update(cx, |s, cx| s.set_value(name, window, cx));
    }

    pub(super) fn default_name(&self, id: Option<i64>) -> String {
        id.and_then(|i| self.default_values.iter().find(|d| d.id == Some(i)))
            .map(|d| d.name.clone())
            .unwrap_or_else(|| "(未設定)".to_string())
    }

    /// 程式有欄位、但尚未全部選好預設輸入值 → 「待設定」，不可排程
    pub(super) fn program_pending(&self, pid: i64) -> bool {
        let (fields, done) = self.prog_stats.get(&pid).copied().unwrap_or((0, 0));
        fields > 0 && done < fields
    }

    pub(super) fn refresh_prog_stats(&mut self) {
        let guard = self.conn.lock().unwrap();
        let repo = Repository::new(&guard);
        self.prog_stats = repo.program_stats().unwrap_or_default();
    }

    /// 依 selected_fields 重建欄位明細表（每列有可直接編輯的輸入框）
    pub(super) fn rebuild_field_rows(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let mut rows = Vec::new();
        for f in self.selected_fields.clone() {
            let label = f.label.clone();
            let html_id = f.html_id.clone();
            let label_input = cx.new(|cx| {
                InputState::new(window, cx)
                    .placeholder("欄位名稱")
                    .default_value(label)
            });
            let id_input = cx.new(|cx| {
                InputState::new(window, cx)
                    .placeholder("欄位 ID")
                    .default_value(html_id)
            });
            let default_id = f.id.and_then(|id| self.field_defaults.get(&id).copied());
            rows.push(FieldRow {
                base: Some(f),
                label_input,
                id_input,
                default_id,
            });
        }
        self.field_rows = rows;
        self.open_picker = None;
    }

    pub(super) fn select_group_item(&mut self, item: GroupItem, window: &mut Window, cx: &mut Context<Self>) {
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

    pub(super) fn load_param_into_editor(
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

    pub(super) fn clear_param_editor(&mut self, window: &mut Window, cx: &mut Context<Self>) {
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
}
