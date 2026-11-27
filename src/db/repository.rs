use std::collections::HashMap;
use anyhow::Result;
use rusqlite::{params, Connection};

use crate::models::{
    CustomVariable, DefaultValue, GroupItem, GroupItemParam, GroupItemParamDetail, JobGroup, Program,
    ProgramField, SysConfig,
};

pub struct Repository<'a> {
    conn: &'a Connection,
}

impl<'a> Repository<'a> {
    pub fn new(conn: &'a Connection) -> Self {
        Self { conn }
    }

    // --- Sys Config ---
    pub fn get_config(&self, key: &str) -> Result<Option<String>> {
        let mut stmt = self.conn.prepare("SELECT value FROM sys_config WHERE key = ?1")?;
        let mut rows = stmt.query(params![key])?;
        if let Some(row) = rows.next()? {
            Ok(Some(row.get(0)?))
        } else {
            Ok(None)
        }
    }

    pub fn set_config(&self, key: &str, value: &str) -> Result<()> {
        self.conn.execute(
            "INSERT INTO sys_config (key, value) VALUES (?1, ?2) ON CONFLICT(key) DO UPDATE SET value = excluded.value",
            params![key, value],
        )?;
        Ok(())
    }

    pub fn list_configs(&self) -> Result<Vec<SysConfig>> {
        let mut stmt = self.conn.prepare("SELECT key, value FROM sys_config ORDER BY key")?;
        let rows = stmt.query_map([], |row| {
            Ok(SysConfig {
                key: row.get(0)?,
                value: row.get(1)?,
            })
        })?;
        let mut list = Vec::new();
        for r in rows {
            list.push(r?);
        }
        Ok(list)
    }

    // --- Custom Variables (replaces hardcoded mail maps) ---
    pub fn list_variables(&self) -> Result<Vec<CustomVariable>> {
        let mut stmt = self.conn.prepare(
            "SELECT var_key, var_value, description FROM custom_variables ORDER BY var_key",
        )?;
        let rows = stmt.query_map([], |row| {
            Ok(CustomVariable {
                var_key: row.get(0)?,
                var_value: row.get(1)?,
                description: row.get(2)?,
            })
        })?;
        let mut list = Vec::new();
        for r in rows {
            list.push(r?);
        }
        Ok(list)
    }

    pub fn get_variables_map(&self) -> Result<HashMap<String, String>> {
        let mut map = HashMap::new();
        for v in self.list_variables()? {
            map.insert(v.var_key, v.var_value);
        }
        Ok(map)
    }

    pub fn upsert_variable(&self, var: &CustomVariable) -> Result<()> {
        self.conn.execute(
            "INSERT INTO custom_variables (var_key, var_value, description) VALUES (?1, ?2, ?3)
             ON CONFLICT(var_key) DO UPDATE SET var_value = excluded.var_value, description = excluded.description",
            params![var.var_key.trim().to_uppercase(), var.var_value.trim(), var.description.trim()],
        )?;
        Ok(())
    }

    pub fn delete_variable(&self, var_key: &str) -> Result<()> {
        self.conn.execute("DELETE FROM custom_variables WHERE var_key = ?1", params![var_key])?;
        Ok(())
    }

    // --- Programs ---
    pub fn save_program_with_fields(&self, program: &Program, fields: &[ProgramField]) -> Result<i64> {
        let prog_id = match self.get_program_by_code(&program.job_code)? {
            Some(existing) => {
                let id = existing.id.unwrap();
                self.conn.execute(
                    "UPDATE programs SET job_name = ?1 WHERE id = ?2",
                    params![program.job_name, id],
                )?;
                id
            }
            None => {
                self.conn.execute(
                    "INSERT INTO programs (job_code, job_name) VALUES (?1, ?2)",
                    params![program.job_code, program.job_name],
                )?;
                self.conn.last_insert_rowid()
            }
        };

        // Preserve existing field IDs when html_id matches so group_item_params bindings remain valid
        let existing_fields = self.list_program_fields(prog_id)?;
        let existing_by_html: HashMap<String, i64> = existing_fields
            .into_iter()
            .filter_map(|f| f.id.map(|id| (f.html_id, id)))
            .collect();

        let mut kept_ids = Vec::new();
        for field in fields {
            if let Some(&fid) = existing_by_html.get(&field.html_id) {
                self.conn.execute(
                    "UPDATE program_fields SET field_scope = ?1, html_name = ?2, label = ?3, tag_type = ?4, max_length = ?5, is_required = ?6, sort_order = ?7 WHERE id = ?8",
                    params![
                        field.field_scope,
                        field.html_name,
                        field.label,
                        field.tag_type,
                        field.max_length as i64,
                        field.is_required,
                        field.sort_order,
                        fid,
                    ],
                )?;
                kept_ids.push(fid);
            } else {
                self.conn.execute(
                    r#"
                    INSERT INTO program_fields
                    (program_id, field_scope, html_id, html_name, label, tag_type, max_length, is_required, sort_order)
                    VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)
                    "#,
                    params![
                        prog_id,
                        field.field_scope,
                        field.html_id,
                        field.html_name,
                        field.label,
                        field.tag_type,
                        field.max_length as i64,
                        field.is_required,
                        field.sort_order,
                    ],
                )?;
                kept_ids.push(self.conn.last_insert_rowid());
            }
        }

        // Remove fields no longer present in newly parsed HTML
        for old_id in existing_by_html.values() {
            if !kept_ids.contains(old_id) {
                self.conn.execute("DELETE FROM program_fields WHERE id = ?1", params![old_id])?;
            }
        }

        Ok(prog_id)
    }

    pub fn create_or_update_program(&self, job_code: &str, job_name: &str) -> Result<i64> {
        if let Some(existing) = self.get_program_by_code(job_code)? {
            let id = existing.id.unwrap();
            self.conn.execute(
                "UPDATE programs SET job_name = ?1 WHERE id = ?2",
                params![job_name, id],
            )?;
            Ok(id)
        } else {
            self.conn.execute(
                "INSERT INTO programs (job_code, job_name) VALUES (?1, ?2)",
                params![job_code, job_name],
            )?;
            Ok(self.conn.last_insert_rowid())
        }
    }

    pub fn delete_program(&self, program_id: i64) -> Result<()> {
        self.conn.execute("DELETE FROM programs WHERE id = ?1", params![program_id])?;
        Ok(())
    }

    pub fn get_program_by_id(&self, program_id: i64) -> Result<Option<Program>> {
        let mut stmt = self
            .conn
            .prepare("SELECT id, job_code, job_name, created_at FROM programs WHERE id = ?1")?;
        let mut rows = stmt.query(params![program_id])?;
        if let Some(row) = rows.next()? {
            Ok(Some(Program {
                id: Some(row.get(0)?),
                job_code: row.get(1)?,
                job_name: row.get(2)?,
                created_at: Some(row.get(3)?),
            }))
        } else {
            Ok(None)
        }
    }

    pub fn get_program_by_code(&self, job_code: &str) -> Result<Option<Program>> {
        let mut stmt = self
            .conn
            .prepare("SELECT id, job_code, job_name, created_at FROM programs WHERE job_code = ?1")?;
        let mut rows = stmt.query(params![job_code])?;
        if let Some(row) = rows.next()? {
            Ok(Some(Program {
                id: Some(row.get(0)?),
                job_code: row.get(1)?,
                job_name: row.get(2)?,
                created_at: Some(row.get(3)?),
            }))
        } else {
            Ok(None)
        }
    }

    pub fn list_programs(&self) -> Result<Vec<Program>> {
        let mut stmt = self
            .conn
            .prepare("SELECT id, job_code, job_name, created_at FROM programs ORDER BY job_code")?;
        let rows = stmt.query_map([], |row| {
            Ok(Program {
                id: Some(row.get(0)?),
                job_code: row.get(1)?,
                job_name: row.get(2)?,
                created_at: Some(row.get(3)?),
            })
        })?;
        let mut list = Vec::new();
        for r in rows {
            list.push(r?);
        }
        Ok(list)
    }

    pub fn list_program_fields(&self, program_id: i64) -> Result<Vec<ProgramField>> {
        let mut stmt = self.conn.prepare(
            r#"
            SELECT id, program_id, field_scope, html_id, html_name, label, tag_type, max_length, is_required, sort_order
            FROM program_fields WHERE program_id = ?1 ORDER BY sort_order ASC, id ASC
            "#,
        )?;
        let rows = stmt.query_map(params![program_id], |row| {
            let max_len: i64 = row.get(7)?;
            Ok(ProgramField {
                id: Some(row.get(0)?),
                program_id: row.get(1)?,
                field_scope: row.get(2)?,
                html_id: row.get(3)?,
                html_name: row.get(4)?,
                label: row.get(5)?,
                tag_type: row.get(6)?,
                max_length: max_len.max(0) as usize,
                is_required: row.get(8)?,
                sort_order: row.get(9)?,
            })
        })?;
        let mut list = Vec::new();
        for r in rows {
            list.push(r?);
        }
        Ok(list)
    }

    pub fn upsert_program_field(&self, field: &ProgramField) -> Result<i64> {
        if let Some(fid) = field.id {
            self.conn.execute(
                r#"
                UPDATE program_fields
                SET field_scope = ?1, html_id = ?2, html_name = ?3, label = ?4, tag_type = ?5,
                    max_length = ?6, is_required = ?7, sort_order = ?8
                WHERE id = ?9
                "#,
                params![
                    field.field_scope,
                    field.html_id,
                    field.html_name,
                    field.label,
                    field.tag_type,
                    field.max_length as i64,
                    field.is_required,
                    field.sort_order,
                    fid,
                ],
            )?;
            Ok(fid)
        } else {
            self.conn.execute(
                r#"
                INSERT INTO program_fields
                (program_id, field_scope, html_id, html_name, label, tag_type, max_length, is_required, sort_order)
                VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)
                "#,
                params![
                    field.program_id,
                    field.field_scope,
                    field.html_id,
                    field.html_name,
                    field.label,
                    field.tag_type,
                    field.max_length as i64,
                    field.is_required,
                    field.sort_order,
                ],
            )?;
            Ok(self.conn.last_insert_rowid())
        }
    }

    pub fn delete_program_field(&self, field_id: i64) -> Result<()> {
        self.conn
            .execute("DELETE FROM program_fields WHERE id = ?1", params![field_id])?;
        Ok(())
    }

    // --- Default values (預設輸入值) ---
    pub fn list_default_values(&self) -> Result<Vec<DefaultValue>> {
        let mut stmt = self
            .conn
            .prepare("SELECT id, name, rule_code, rule_value FROM default_values ORDER BY id ASC")?;
        let rows = stmt.query_map([], |row| {
            Ok(DefaultValue {
                id: Some(row.get(0)?),
                name: row.get(1)?,
                rule_code: row.get(2)?,
                rule_value: row.get(3)?,
            })
        })?;
        let mut list = Vec::new();
        for r in rows {
            list.push(r?);
        }
        Ok(list)
    }

    /// 「自行輸入」的預設值：以 "自訂：<內容>" 為名建立（已存在則重用），規則為 CUSTOM_TEMPLATE
    pub fn ensure_custom_default(&self, text: &str) -> Result<i64> {
        let name = format!("自訂：{text}");
        self.conn.execute(
            "INSERT OR IGNORE INTO default_values (name, rule_code, rule_value) VALUES (?1, 'CUSTOM_TEMPLATE', ?2)",
            params![name, text],
        )?;
        let id: i64 = self.conn.query_row(
            "SELECT id FROM default_values WHERE name = ?1",
            params![name],
            |r| r.get(0),
        )?;
        Ok(id)
    }

    /// field_id -> default_id
    pub fn field_defaults_for_program(&self, program_id: i64) -> Result<HashMap<i64, i64>> {
        let mut stmt = self.conn.prepare(
            "SELECT d.field_id, d.default_id FROM field_defaults d \
             JOIN program_fields f ON f.id = d.field_id WHERE f.program_id = ?1",
        )?;
        let rows = stmt.query_map(params![program_id], |row| {
            Ok((row.get::<_, i64>(0)?, row.get::<_, i64>(1)?))
        })?;
        let mut map = HashMap::new();
        for r in rows {
            let (field_id, default_id) = r?;
            map.insert(field_id, default_id);
        }
        Ok(map)
    }

    pub fn set_field_default(&self, field_id: i64, default_id: Option<i64>) -> Result<()> {
        match default_id {
            Some(d) => {
                self.conn.execute(
                    "INSERT INTO field_defaults (field_id, default_id) VALUES (?1, ?2) \
                     ON CONFLICT(field_id) DO UPDATE SET default_id = excluded.default_id",
                    params![field_id, d],
                )?;
            }
            None => {
                self.conn
                    .execute("DELETE FROM field_defaults WHERE field_id = ?1", params![field_id])?;
            }
        }
        Ok(())
    }

    /// program_id -> (欄位數, 已設定預設值的欄位數)
    pub fn program_stats(&self) -> Result<HashMap<i64, (usize, usize)>> {
        let mut stmt = self.conn.prepare(
            "SELECT f.program_id, COUNT(f.id), COUNT(d.default_id) FROM program_fields f \
             LEFT JOIN field_defaults d ON d.field_id = f.id GROUP BY f.program_id",
        )?;
        let rows = stmt.query_map([], |row| {
            Ok((
                row.get::<_, i64>(0)?,
                row.get::<_, i64>(1)?,
                row.get::<_, i64>(2)?,
            ))
        })?;
        let mut map = HashMap::new();
        for r in rows {
            let (pid, total, done) = r?;
            map.insert(pid, (total.max(0) as usize, done.max(0) as usize));
        }
        Ok(map)
    }

    /// 找出其他程式中「同欄位 ID」已設定的預設值
    pub fn find_default_by_html_id(&self, html_id: &str, exclude_program: i64) -> Result<Option<i64>> {
        let mut stmt = self.conn.prepare(
            "SELECT d.default_id FROM program_fields f \
             JOIN field_defaults d ON d.field_id = f.id \
             WHERE f.html_id = ?1 AND f.program_id <> ?2 ORDER BY d.field_id DESC LIMIT 1",
        )?;
        let mut rows = stmt.query(params![html_id, exclude_program])?;
        if let Some(row) = rows.next()? {
            Ok(Some(row.get(0)?))
        } else {
            Ok(None)
        }
    }

    pub fn update_program(&self, id: i64, job_code: &str, job_name: &str) -> Result<()> {
        self.conn.execute(
            "UPDATE programs SET job_code = ?1, job_name = ?2 WHERE id = ?3",
            params![job_code, job_name, id],
        )?;
        Ok(())
    }

    // --- Job Groups ---
    pub fn create_job_group(&self, group: &JobGroup) -> Result<i64> {
        self.conn.execute(
            "INSERT INTO job_groups (group_name, description, group_mode, schedule_rule_json) VALUES (?1, ?2, ?3, ?4)",
            params![
                group.group_name,
                group.description,
                group.group_mode,
                group.schedule_rule_json,
            ],
        )?;
        Ok(self.conn.last_insert_rowid())
    }

    pub fn update_job_group(&self, group: &JobGroup) -> Result<()> {
        let id = group.id.ok_or_else(|| anyhow::anyhow!("Missing group id"))?;
        self.conn.execute(
            "UPDATE job_groups SET group_name = ?1, description = ?2, group_mode = ?3, schedule_rule_json = ?4 WHERE id = ?5",
            params![
                group.group_name,
                group.description,
                group.group_mode,
                group.schedule_rule_json,
                id
            ],
        )?;
        Ok(())
    }

    pub fn update_group_schedule_rule(&self, group_id: i64, schedule_rule_json: &str) -> Result<()> {
        self.conn.execute(
            "UPDATE job_groups SET schedule_rule_json = ?1 WHERE id = ?2",
            params![schedule_rule_json, group_id],
        )?;
        Ok(())
    }

    pub fn delete_job_group(&self, group_id: i64) -> Result<()> {
        self.conn
            .execute("DELETE FROM job_groups WHERE id = ?1", params![group_id])?;
        Ok(())
    }

    pub fn get_job_group(&self, group_id: i64) -> Result<Option<JobGroup>> {
        let mut stmt = self.conn.prepare(
            "SELECT id, group_name, description, group_mode, schedule_rule_json, created_at FROM job_groups WHERE id = ?1",
        )?;
        let mut rows = stmt.query(params![group_id])?;
        if let Some(row) = rows.next()? {
            Ok(Some(JobGroup {
                id: Some(row.get(0)?),
                group_name: row.get(1)?,
                description: row.get::<_, Option<String>>(2)?.unwrap_or_default(),
                group_mode: row.get(3)?,
                schedule_rule_json: row.get::<_, Option<String>>(4)?.unwrap_or_default(),
                created_at: Some(row.get(5)?),
            }))
        } else {
            Ok(None)
        }
    }

    pub fn list_job_groups(&self) -> Result<Vec<JobGroup>> {
        let mut stmt = self.conn.prepare(
            "SELECT id, group_name, description, group_mode, schedule_rule_json, created_at FROM job_groups ORDER BY id",
        )?;
        let rows = stmt.query_map([], |row| {
            Ok(JobGroup {
                id: Some(row.get(0)?),
                group_name: row.get(1)?,
                description: row.get::<_, Option<String>>(2)?.unwrap_or_default(),
                group_mode: row.get(3)?,
                schedule_rule_json: row.get::<_, Option<String>>(4)?.unwrap_or_default(),
                created_at: Some(row.get(5)?),
            })
        })?;
        let mut list = Vec::new();
        for r in rows {
            list.push(r?);
        }
        Ok(list)
    }

    // --- Group Items ---
    pub fn add_group_item(&self, item: &GroupItem) -> Result<i64> {
        self.conn.execute(
            r#"
            INSERT INTO group_items (
                group_id, program_id, step_order, expect_exe_date_rule, expect_exe_time,
                req_op_confirm, link_prev_sn, pre_job_sn1, pre_job_sn2, pre_job_sn3,
                rework_sn, req_email, mail_recipients, pre_work_note, rear_work_note
            ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15)
            "#,
            params![
                item.group_id,
                item.program_id,
                item.step_order,
                item.expect_exe_date_rule,
                item.expect_exe_time,
                item.req_op_confirm,
                item.link_prev_sn,
                item.pre_job_sn1,
                item.pre_job_sn2,
                item.pre_job_sn3,
                item.rework_sn,
                item.req_email,
                item.mail_recipients,
                item.pre_work_note,
                item.rear_work_note,
            ],
        )?;
        Ok(self.conn.last_insert_rowid())
    }

    pub fn update_group_item(&self, item: &GroupItem) -> Result<()> {
        let id = item.id.ok_or_else(|| anyhow::anyhow!("Missing group_item id"))?;
        self.conn.execute(
            r#"
            UPDATE group_items SET
                step_order = ?1, expect_exe_date_rule = ?2, expect_exe_time = ?3,
                req_op_confirm = ?4, link_prev_sn = ?5, pre_job_sn1 = ?6, pre_job_sn2 = ?7,
                pre_job_sn3 = ?8, rework_sn = ?9, req_email = ?10, mail_recipients = ?11,
                pre_work_note = ?12, rear_work_note = ?13
            WHERE id = ?14
            "#,
            params![
                item.step_order,
                item.expect_exe_date_rule,
                item.expect_exe_time,
                item.req_op_confirm,
                item.link_prev_sn,
                item.pre_job_sn1,
                item.pre_job_sn2,
                item.pre_job_sn3,
                item.rework_sn,
                item.req_email,
                item.mail_recipients,
                item.pre_work_note,
                item.rear_work_note,
                id,
            ],
        )?;
        Ok(())
    }

    pub fn delete_group_item(&self, group_id: i64, item_id: i64) -> Result<()> {
        self.conn
            .execute("DELETE FROM group_items WHERE id = ?1", params![item_id])?;
        // Renumber remaining steps cleanly
        let items = self.list_group_items(group_id)?;
        for (idx, item) in items.iter().enumerate() {
            if let Some(id) = item.id {
                self.conn.execute(
                    "UPDATE group_items SET step_order = ?1 WHERE id = ?2",
                    params![(idx + 1) as i32, id],
                )?;
            }
        }
        Ok(())
    }

    pub fn swap_group_item_order(&self, group_id: i64, item_id: i64, move_up: bool) -> Result<()> {
        let items = self.list_group_items(group_id)?;
        if let Some(pos) = items.iter().position(|i| i.id == Some(item_id)) {
            let target_pos = if move_up && pos > 0 {
                Some(pos - 1)
            } else if !move_up && pos + 1 < items.len() {
                Some(pos + 1)
            } else {
                None
            };
            if let Some(t_pos) = target_pos {
                let a = &items[pos];
                let b = &items[t_pos];
                if let (Some(a_id), Some(b_id)) = (a.id, b.id) {
                    self.conn.execute(
                        "UPDATE group_items SET step_order = ?1 WHERE id = ?2",
                        params![b.step_order, a_id],
                    )?;
                    self.conn.execute(
                        "UPDATE group_items SET step_order = ?1 WHERE id = ?2",
                        params![a.step_order, b_id],
                    )?;
                }
            }
        }
        Ok(())
    }

    pub fn list_group_items(&self, group_id: i64) -> Result<Vec<GroupItem>> {
        let mut stmt = self.conn.prepare(
            r#"
            SELECT id, group_id, program_id, step_order, expect_exe_date_rule, expect_exe_time,
                   req_op_confirm, link_prev_sn, pre_job_sn1, pre_job_sn2, pre_job_sn3,
                   rework_sn, req_email, mail_recipients, pre_work_note, rear_work_note
            FROM group_items WHERE group_id = ?1 ORDER BY step_order ASC, id ASC
            "#,
        )?;
        let rows = stmt.query_map(params![group_id], |row| {
            Ok(GroupItem {
                id: Some(row.get(0)?),
                group_id: row.get(1)?,
                program_id: row.get(2)?,
                step_order: row.get(3)?,
                expect_exe_date_rule: row.get(4)?,
                expect_exe_time: row.get(5)?,
                req_op_confirm: row.get(6)?,
                link_prev_sn: row.get(7)?,
                pre_job_sn1: row.get(8)?,
                pre_job_sn2: row.get(9)?,
                pre_job_sn3: row.get(10)?,
                rework_sn: row.get(11)?,
                req_email: row.get(12)?,
                mail_recipients: row.get(13)?,
                pre_work_note: row.get(14)?,
                rear_work_note: row.get(15)?,
            })
        })?;
        let mut list = Vec::new();
        for r in rows {
            list.push(r?);
        }
        Ok(list)
    }

    // --- Group Item Params ---
    pub fn set_item_param(&self, param: &GroupItemParam) -> Result<i64> {
        self.conn.execute(
            r#"
            INSERT INTO group_item_params (group_item_id, field_id, rule_code, rule_value)
            VALUES (?1, ?2, ?3, ?4)
            ON CONFLICT(group_item_id, field_id)
            DO UPDATE SET rule_code = excluded.rule_code, rule_value = excluded.rule_value
            "#,
            params![param.group_item_id, param.field_id, param.rule_code, param.rule_value],
        )?;
        Ok(self.conn.last_insert_rowid())
    }

    /// Saves or creates a ProgramField (欄位名稱 label + 欄位 ID html_id) and binds its rule in one operation.
    pub fn set_item_param_with_field(
        &self,
        group_item_id: i64,
        program_id: i64,
        field_id: Option<i64>,
        field_scope: &str,
        html_id: &str,
        label: &str,
        rule_code: &str,
        rule_value: &str,
    ) -> Result<i64> {
        let clean_id = html_id.trim();
        let clean_label = if label.trim().is_empty() {
            clean_id
        } else {
            label.trim()
        };

        let target_field_id = if let Some(fid) = field_id {
            self.conn.execute(
                "UPDATE program_fields SET field_scope = ?1, html_id = ?2, html_name = ?2, label = ?3 WHERE id = ?4",
                params![field_scope, clean_id, clean_label, fid],
            )?;
            fid
        } else {
            let existing = self
                .list_program_fields(program_id)?
                .into_iter()
                .find(|f| f.html_id == clean_id);
            if let Some(ex) = existing {
                let fid = ex.id.unwrap();
                self.conn.execute(
                    "UPDATE program_fields SET field_scope = ?1, label = ?2 WHERE id = ?3",
                    params![field_scope, clean_label, fid],
                )?;
                fid
            } else {
                let next_order = self.list_program_fields(program_id)?.len() as i32;
                self.upsert_program_field(&ProgramField {
                    id: None,
                    program_id,
                    field_scope: field_scope.to_string(),
                    html_id: clean_id.to_string(),
                    html_name: clean_id.to_string(),
                    label: clean_label.to_string(),
                    tag_type: "input".to_string(),
                    max_length: 0,
                    is_required: true,
                    sort_order: next_order,
                })?
            }
        };

        self.set_item_param(&GroupItemParam {
            id: None,
            group_item_id,
            field_id: target_field_id,
            rule_code: rule_code.to_string(),
            rule_value: rule_value.to_string(),
        })
    }

    pub fn delete_item_param(&self, group_item_id: i64, field_id: i64) -> Result<()> {
        self.conn.execute(
            "DELETE FROM group_item_params WHERE group_item_id = ?1 AND field_id = ?2",
            params![group_item_id, field_id],
        )?;
        Ok(())
    }

    pub fn list_item_params(&self, group_item_id: i64) -> Result<Vec<GroupItemParam>> {
        let mut stmt = self.conn.prepare(
            "SELECT id, group_item_id, field_id, rule_code, rule_value FROM group_item_params WHERE group_item_id = ?1",
        )?;
        let rows = stmt.query_map(params![group_item_id], |row| {
            Ok(GroupItemParam {
                id: Some(row.get(0)?),
                group_item_id: row.get(1)?,
                field_id: row.get(2)?,
                rule_code: row.get(3)?,
                rule_value: row.get(4)?,
            })
        })?;
        let mut list = Vec::new();
        for r in rows {
            list.push(r?);
        }
        Ok(list)
    }

    /// Lists all ProgramFields for `program_id`, merged with any configured GroupItemParam for `group_item_id`,
    /// guaranteeing every field shows both 欄位名稱 (label) and 欄位 ID (html_id).
    pub fn list_item_params_detailed(
        &self,
        group_item_id: i64,
        program_id: i64,
    ) -> Result<Vec<GroupItemParamDetail>> {
        let fields = self.list_program_fields(program_id)?;
        let saved_params = self.list_item_params(group_item_id)?;
        let saved_map: HashMap<i64, GroupItemParam> =
            saved_params.into_iter().map(|p| (p.field_id, p)).collect();

        let mut details = Vec::new();
        for f in fields {
            let fid = f.id.unwrap_or(0);
            if let Some(p) = saved_map.get(&fid) {
                details.push(GroupItemParamDetail {
                    param_id: p.id,
                    group_item_id,
                    field_id: fid,
                    field_scope: f.field_scope,
                    html_id: f.html_id,
                    html_name: f.html_name,
                    label: f.label,
                    tag_type: f.tag_type,
                    is_required: f.is_required,
                    rule_code: p.rule_code.clone(),
                    rule_value: p.rule_value.clone(),
                    is_configured: true,
                });
            } else {
                details.push(GroupItemParamDetail {
                    param_id: None,
                    group_item_id,
                    field_id: fid,
                    field_scope: f.field_scope,
                    html_id: f.html_id,
                    html_name: f.html_name,
                    label: f.label,
                    tag_type: f.tag_type,
                    is_required: f.is_required,
                    rule_code: "STATIC".to_string(),
                    rule_value: String::new(),
                    is_configured: false,
                });
            }
        }
        Ok(details)
    }
}
