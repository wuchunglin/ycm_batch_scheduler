//! 群組編輯：動作（群組、步驟、參數、執行清單）

use super::*;
use super::helpers::{parse_hhmm};

impl BatchSchedulerApp {
    // --- Actions: Group Editor ---
    pub(super) fn create_group(&mut self, window: &mut Window, cx: &mut Context<Self>) {
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

    pub(super) fn set_group_mode(&mut self, mode: &str, window: &mut Window, cx: &mut Context<Self>) {
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

    pub(super) fn delete_selected_group(&mut self, window: &mut Window, cx: &mut Context<Self>) {
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

    pub(super) fn add_program_to_group(
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

    pub(super) fn add_program_with_options(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(gid) = self.selected_group.as_ref().and_then(|g| g.id) else {
            window.push_notification(Notification::error("請先建立或選取排程群組"), cx);
            return;
        };
        let Some(program_id) = self.gs_selected_program else {
            window.push_notification(Notification::error("請先選擇批次程式"), cx);
            return;
        };
        let raw = self.gs_time_input.read(cx).value().trim().to_string();
        let Some(time) = parse_hhmm(&raw) else {
            window.push_notification(
                Notification::error("預計執行時間格式須為 HH:MM（00:00～23:59）"),
                cx,
            );
            return;
        };
        let next_order = (self.group_items.len() + 1) as i32;
        let new_item = GroupItem {
            id: None,
            group_id: gid,
            program_id,
            step_order: next_order,
            expect_exe_date_rule: "GROUP_DATE".to_string(),
            expect_exe_time: time,
            req_op_confirm: false,
            link_prev_sn: self.gs_link_prev,
            pre_job_sn1: String::new(),
            pre_job_sn2: String::new(),
            pre_job_sn3: String::new(),
            rework_sn: String::new(),
            req_email: self.gs_req_email,
            mail_recipients: String::new(),
            pre_work_note: String::new(),
            rear_work_note: String::new(),
        };
        {
            let guard = self.conn.lock().unwrap();
            let repo = Repository::new(&guard);
            if let Err(e) = repo.add_group_item(&new_item) {
                window.push_notification(
                    Notification::error(format!("加入群組失敗：{e}")),
                    cx,
                );
                return;
            }
        }
        self.reload_groups(window, cx);
        window.push_notification(Notification::success("已加入群組"), cx);
    }

    pub(super) fn move_group_item(
        &mut self,
        item_id: i64,
        up: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(gid) = self.selected_group.as_ref().and_then(|g| g.id) else {
            return;
        };
        {
            let guard = self.conn.lock().unwrap();
            let repo = Repository::new(&guard);
            if let Err(e) = repo.swap_group_item_order(gid, item_id, up) {
                window.push_notification(
                    Notification::error(format!("調整順序失敗：{e}")),
                    cx,
                );
                return;
            }
        }
        self.reload_groups(window, cx);
    }

    pub(super) fn new_exec_time_input(
        time: &str,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Entity<InputState> {
        let time = time.to_string();
        cx.new(|cx| {
            InputState::new(window, cx)
                .placeholder("HH:MM")
                .default_value(time)
        })
    }

    pub(super) fn toggle_exec_group(
        &mut self,
        gid: i64,
        checked: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if checked {
            if !self.exec_sel_groups.insert(gid) {
                return;
            }
            let items = {
                let guard = self.conn.lock().unwrap();
                let repo = Repository::new(&guard);
                repo.list_group_items(gid).unwrap_or_default()
            };
            let mut skipped: Vec<String> = Vec::new();
            for it in items {
                // 預設輸入值尚未設定完成的程式（待設定）不可排程
                if self.program_pending(it.program_id) {
                    let code = self
                        .programs
                        .iter()
                        .find(|p| p.id == Some(it.program_id))
                        .map(|p| p.job_code.clone())
                        .unwrap_or_else(|| "?".to_string());
                    skipped.push(code);
                    continue;
                }
                let time_input = Self::new_exec_time_input(&it.expect_exe_time, window, cx);
                self.exec_entries.push(ExecEntry {
                    program_id: it.program_id,
                    source_group: Some(gid),
                    time_input,
                    link_prev: it.link_prev_sn,
                    req_email: it.req_email,
                });
            }
            if !skipped.is_empty() {
                window.push_notification(
                    Notification::warning(format!(
                        "下列程式的欄位尚未全部選好預設輸入值，未加入執行清單：{}",
                        skipped.join("、")
                    )),
                    cx,
                );
            }
        } else {
            self.exec_sel_groups.remove(&gid);
            self.exec_entries.retain(|e| e.source_group != Some(gid));
        }
        cx.notify();
    }

    pub(super) fn toggle_exec_program(
        &mut self,
        pid: i64,
        checked: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if checked {
            if self.program_pending(pid) {
                window.push_notification(
                    Notification::warning("此批次程式的欄位尚未全部選好預設輸入值，請先到「批次程式新增」設定"),
                    cx,
                );
                cx.notify();
                return;
            }
            if !self.exec_sel_programs.insert(pid) {
                return;
            }
            let time_input = Self::new_exec_time_input("13:00", window, cx);
            self.exec_entries.push(ExecEntry {
                program_id: pid,
                source_group: None,
                time_input,
                link_prev: false,
                req_email: true,
            });
        } else {
            self.exec_sel_programs.remove(&pid);
            self.exec_entries
                .retain(|e| !(e.source_group.is_none() && e.program_id == pid));
        }
        cx.notify();
    }

    pub(super) fn move_exec_entry(&mut self, idx: usize, up: bool, cx: &mut Context<Self>) {
        let target = if up {
            idx.checked_sub(1)
        } else if idx + 1 < self.exec_entries.len() {
            Some(idx + 1)
        } else {
            None
        };
        if let Some(t) = target {
            self.exec_entries.swap(idx, t);
            cx.notify();
        }
    }

    pub(super) fn remove_exec_entry(&mut self, idx: usize, cx: &mut Context<Self>) {
        if idx < self.exec_entries.len() {
            let e = self.exec_entries.remove(idx);
            if e.source_group.is_none() {
                self.exec_sel_programs.remove(&e.program_id);
            }
            cx.notify();
        }
    }

    pub(super) fn remove_group_item(&mut self, item_id: i64, window: &mut Window, cx: &mut Context<Self>) {
        let Some(gid) = self.selected_group.as_ref().and_then(|g| g.id) else {
            return;
        };
        {
            let guard = self.conn.lock().unwrap();
            let repo = Repository::new(&guard);
            if let Err(e) = repo.delete_group_item(gid, item_id) {
                window.push_notification(
                    Notification::error(format!("移除群組程式失敗：{e}")),
                    cx,
                );
                return;
            }
        }
        self.reload_groups(window, cx);
        window.push_notification(Notification::success("已從群組移除"), cx);
    }

    pub(super) fn save_selected_item_common(&mut self, window: &mut Window, cx: &mut Context<Self>) {
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

    pub(super) fn save_item_param_with_field(&mut self, window: &mut Window, cx: &mut Context<Self>) {
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

    pub(super) fn delete_item_param(
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
}
