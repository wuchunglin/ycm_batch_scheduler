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

        CREATE TABLE IF NOT EXISTS default_values (
            id INTEGER PRIMARY KEY AUTOINCREMENT,
            name TEXT UNIQUE NOT NULL,
            rule_code TEXT NOT NULL,
            rule_value TEXT NOT NULL DEFAULT ''
        );

        CREATE TABLE IF NOT EXISTS field_defaults (
            field_id INTEGER PRIMARY KEY REFERENCES program_fields(id) ON DELETE CASCADE,
            default_id INTEGER NOT NULL REFERENCES default_values(id) ON DELETE CASCADE
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

    // 預設輸入值清單的初始項目（可於「新增預設值」功能擴充）
    let default_values = [
        ("空白", "STATIC", ""),
        ("執行日當天(例:1150101)", "E1", ""),
        ("執行日前月(例:11501)", "EE", ""),
        ("執行日前月第1日(例:1150101)", "MONTH_EDGE", "PREV_MONTH_FIRST"),
        ("執行日前月最後1日(例:1150131)", "MONTH_EDGE", "PREV_MONTH_LAST"),
        ("執行日前1季第1日(例:1150101)", "DATE_EDGE", "PREV_QUARTER_FIRST"),
        ("執行日前1季最後1日(例:1150331)", "DATE_EDGE", "PREV_QUARTER_LAST"),
        ("執行日前半年第1日(例:1150101)", "DATE_EDGE", "PREV_HALF_FIRST"),
        ("執行日前半年最後1日(例:1150630)", "DATE_EDGE", "PREV_HALF_LAST"),
        ("執行日本年第1日(例:1150101)", "DATE_EDGE", "YEAR_FIRST"),
        ("執行日本年最後1日(例:1151231)", "DATE_EDGE", "YEAR_LAST"),
        ("執行日4年前前1季第1日(例:1110101)", "DATE_EDGE", "PREV_QUARTER_FIRST:-4"),
        ("執行日4年前前1季最後1日(例:1110331)", "DATE_EDGE", "PREV_QUARTER_LAST:-4"),
        ("執行日後1天", "DAY_OFFSET", "+1"),
        ("執行日後2天", "DAY_OFFSET", "+2"),
        ("執行日後3天", "DAY_OFFSET", "+3"),
        ("執行日後4天", "DAY_OFFSET", "+4"),
        ("執行日後5天", "DAY_OFFSET", "+5"),
        ("執行日後6天", "DAY_OFFSET", "+6"),
        ("執行日後7天", "DAY_OFFSET", "+7"),
        ("執行日前1天", "DAY_OFFSET", "-1"),
        ("執行日前2天", "DAY_OFFSET", "-2"),
        ("執行日前3天", "DAY_OFFSET", "-3"),
        ("執行日前4天", "DAY_OFFSET", "-4"),
        ("執行日前5天", "DAY_OFFSET", "-5"),
        ("執行日前6天", "DAY_OFFSET", "-6"),
        ("執行日前7天", "DAY_OFFSET", "-7"),
    ];
    // 移除舊版預設值，並將「固定值：空白」更名為「空白」
    for old_name in ["今日民國日期", "前一日民國日期", "當月民國年月", "當月月份"] {
        conn.execute(
            "DELETE FROM field_defaults WHERE default_id IN (SELECT id FROM default_values WHERE name = ?1)",
            params![old_name],
        )?;
        conn.execute("DELETE FROM default_values WHERE name = ?1", params![old_name])?;
    }
    let blank_exists: i64 = conn.query_row(
        "SELECT COUNT(*) FROM default_values WHERE name = '空白'",
        [],
        |r| r.get(0),
    )?;
    if blank_exists == 0 {
        conn.execute(
            "UPDATE default_values SET name = '空白' WHERE name = '固定值：空白'",
            [],
        )?;
    }

    for (name, code, value) in default_values {
        conn.execute(
            "INSERT OR IGNORE INTO default_values (name, rule_code, rule_value) VALUES (?1, ?2, ?3)",
            params![name, code, value],
        )?;
    }

    Ok(())
}
