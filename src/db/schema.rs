use rusqlite::{params, Connection, Result};

pub fn init_db(conn: &Connection) -> Result<()> {
    conn.execute_batch(
        r#"
        PRAGMA foreign_keys = ON;

        CREATE TABLE IF NOT EXISTS sys_config (
            key TEXT PRIMARY KEY,
            value TEXT NOT NULL
        );

        CREATE TABLE IF NOT EXISTS custom_variables (
            var_key TEXT PRIMARY KEY,
            var_value TEXT NOT NULL,
            description TEXT DEFAULT ''
        );

        CREATE TABLE IF NOT EXISTS programs (
            id INTEGER PRIMARY KEY AUTOINCREMENT,
            job_code TEXT UNIQUE NOT NULL,
            job_name TEXT NOT NULL,
            created_at DATETIME DEFAULT CURRENT_TIMESTAMP
        );

        CREATE TABLE IF NOT EXISTS program_fields (
            id INTEGER PRIMARY KEY AUTOINCREMENT,
            program_id INTEGER NOT NULL REFERENCES programs(id) ON DELETE CASCADE,
            field_scope TEXT NOT NULL,
            html_id TEXT NOT NULL,
            html_name TEXT NOT NULL,
            label TEXT NOT NULL,
            tag_type TEXT NOT NULL,
            max_length INTEGER DEFAULT 0,
            is_required BOOLEAN DEFAULT 0,
            sort_order INTEGER DEFAULT 0
        );

        CREATE TABLE IF NOT EXISTS job_groups (
            id INTEGER PRIMARY KEY AUTOINCREMENT,
            group_name TEXT UNIQUE NOT NULL,
            description TEXT DEFAULT '',
            group_mode TEXT NOT NULL,
            schedule_rule_json TEXT,
            created_at DATETIME DEFAULT CURRENT_TIMESTAMP
        );

        CREATE TABLE IF NOT EXISTS group_items (
            id INTEGER PRIMARY KEY AUTOINCREMENT,
            group_id INTEGER NOT NULL REFERENCES job_groups(id) ON DELETE CASCADE,
            program_id INTEGER NOT NULL REFERENCES programs(id) ON DELETE CASCADE,
            step_order INTEGER NOT NULL,
            expect_exe_date_rule TEXT DEFAULT 'GROUP_DATE',
            expect_exe_time TEXT DEFAULT '',
            req_op_confirm BOOLEAN DEFAULT 0,
            link_prev_sn BOOLEAN DEFAULT 0,
            pre_job_sn1 TEXT DEFAULT '',
            pre_job_sn2 TEXT DEFAULT '',
            pre_job_sn3 TEXT DEFAULT '',
            rework_sn TEXT DEFAULT '',
            req_email BOOLEAN DEFAULT 1,
            mail_recipients TEXT DEFAULT '',
            pre_work_note TEXT DEFAULT '',
            rear_work_note TEXT DEFAULT ''
        );

        CREATE TABLE IF NOT EXISTS group_item_params (
            id INTEGER PRIMARY KEY AUTOINCREMENT,
            group_item_id INTEGER NOT NULL REFERENCES group_items(id) ON DELETE CASCADE,
            field_id INTEGER NOT NULL REFERENCES program_fields(id) ON DELETE CASCADE,
            rule_code TEXT NOT NULL,
            rule_value TEXT NOT NULL,
            UNIQUE(group_item_id, field_id)
        );
        "#,
    )?;

    // Generic default configurations only (no hardcoded business-specific jobs)
    let default_configs = [
        ("edge_path", r"C:\Program Files (x86)\Microsoft\Edge\Application\msedge.exe"),
        ("target_url", "https://www.nat.fia.gov.tw/ycm/jobExecute/unRegular/getSysData"),
        ("default_pin", ""),
        ("screenshot_dir", "screenshots"),
        ("action_timeout_sec", "30"),
    ];

    for (k, v) in default_configs {
        conn.execute(
            "INSERT OR IGNORE INTO sys_config (key, value) VALUES (?1, ?2)",
            params![k, v],
        )?;
    }

    Ok(())
}
