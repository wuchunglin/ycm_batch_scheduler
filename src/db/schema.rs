use rusqlite::{params, Connection, Result};

fn column_exists(conn: &Connection, table: &str, column: &str) -> Result<bool> {
    let mut stmt = conn.prepare(&format!("PRAGMA table_info({table})"))?;
    let cols = stmt.query_map([], |r| r.get::<_, String>(1))?;
    for c in cols {
        if c? == column {
            return Ok(true);
        }
    }
    Ok(false)
}

/// 年度格式改為「每個欄位各自設定」（program_fields.year_ad，0 = 民國年、1 = 西元年）：
/// 1. 舊資料庫補上 year_ad 欄位（既有欄位一律民國年）。
/// 2. 舊版「參數設定」把年度格式存成規則值後綴 `<數字>:AD`（去年月份 / 以前年度）。
///    只執行一次：使用到 `:AD` 選項的欄位改設為西元年（輸出結果不變），並拿掉選項上的 `:AD` 後綴。
fn migrate_year_format(conn: &Connection) -> Result<()> {
    if !column_exists(conn, "program_fields", "year_ad")? {
        conn.execute(
            "ALTER TABLE program_fields ADD COLUMN year_ad INTEGER NOT NULL DEFAULT 0",
            [],
        )?;
    }

    let migrated: Option<String> = conn
        .query_row(
            "SELECT value FROM sys_config WHERE key = 'year_format_migrated'",
            [],
            |r| r.get(0),
        )
        .ok();
    if migrated.is_some() {
        return Ok(());
    }

    const YEAR_RULES: &str = "('LAST_YEAR_MONTH', 'PREV_YEAR')";
    // 下拉選項帶 :AD → 使用它的欄位改為西元年
    conn.execute(
        &format!(
            "UPDATE program_fields SET year_ad = 1 WHERE id IN (\
                SELECT fd.field_id FROM field_defaults fd \
                JOIN default_values dv ON dv.id = fd.default_id \
                WHERE dv.rule_code IN {YEAR_RULES} AND UPPER(dv.rule_value) LIKE '%:AD')"
        ),
        [],
    )?;
    // 群組步驟舊有參數規則帶 :AD → 該欄位改為西元年
    conn.execute(
        &format!(
            "UPDATE program_fields SET year_ad = 1 WHERE id IN (\
                SELECT field_id FROM group_item_params \
                WHERE rule_code IN {YEAR_RULES} AND UPPER(rule_value) LIKE '%:AD')"
        ),
        [],
    )?;
    // 拿掉規則值的 :AD 後綴
    for table in ["default_values", "group_item_params"] {
        conn.execute(
            &format!(
                "UPDATE {table} SET rule_value = TRIM(SUBSTR(rule_value, 1, INSTR(rule_value, ':') - 1)) \
                 WHERE rule_code IN {YEAR_RULES} AND INSTR(rule_value, ':') > 0 \
                 AND UPPER(TRIM(SUBSTR(rule_value, INSTR(rule_value, ':') + 1))) = 'AD'"
            ),
            [],
        )?;
    }
    conn.execute(
        "INSERT OR REPLACE INTO sys_config (key, value) VALUES ('year_format_migrated', '1')",
        [],
    )?;
    Ok(())
}

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
            sort_order INTEGER DEFAULT 0,
            year_ad INTEGER NOT NULL DEFAULT 0
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

    // 下拉選單選項的顯示順序：舊資料庫補上 sort_order 欄位（原順序 = 原 id 順序）
    let has_sort = column_exists(conn, "default_values", "sort_order")?;
    if !has_sort {
        conn.execute(
            "ALTER TABLE default_values ADD COLUMN sort_order INTEGER NOT NULL DEFAULT 0",
            [],
        )?;
        conn.execute("UPDATE default_values SET sort_order = id", [])?;
    }

    migrate_year_format(conn)?;

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

    // 預設輸入值清單（下拉選單選項）只在「第一次」從程式內建清單寫入資料表，
    // 之後一律以資料表為準，可由「參數設定」新增 / 修改 / 刪除（刪除後不會再被補回來）。
    let seeded: Option<String> = conn
        .query_row(
            "SELECT value FROM sys_config WHERE key = 'default_values_seeded'",
            [],
            |r| r.get(0),
        )
        .ok();
    if seeded.is_none() {
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
        conn.execute(
            "INSERT OR REPLACE INTO sys_config (key, value) VALUES ('default_values_seeded', '1')",
            [],
        )?;
    }

    // 尚未排序（sort_order = 0）的選項，順序先比照 id
    conn.execute("UPDATE default_values SET sort_order = id WHERE sort_order = 0", [])?;

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 舊版資料庫（program_fields 沒有 year_ad、選項帶 `:AD` 後綴）升級後：
    /// 補上 year_ad 欄位、`:AD` 選項的使用欄位改設為西元年、後綴被拿掉，且只轉換一次。
    #[test]
    fn migrates_legacy_ad_suffix_to_field_year_ad() {
        let conn = Connection::open_in_memory().unwrap();
        conn.execute_batch(
            r#"
            CREATE TABLE sys_config (key TEXT PRIMARY KEY, value TEXT NOT NULL);
            CREATE TABLE programs (id INTEGER PRIMARY KEY AUTOINCREMENT, job_code TEXT UNIQUE NOT NULL,
                job_name TEXT NOT NULL, created_at DATETIME DEFAULT CURRENT_TIMESTAMP);
            CREATE TABLE program_fields (
                id INTEGER PRIMARY KEY AUTOINCREMENT, program_id INTEGER NOT NULL, field_scope TEXT NOT NULL,
                html_id TEXT NOT NULL, html_name TEXT NOT NULL, label TEXT NOT NULL, tag_type TEXT NOT NULL,
                max_length INTEGER DEFAULT 0, is_required BOOLEAN DEFAULT 0, sort_order INTEGER DEFAULT 0);
            CREATE TABLE default_values (id INTEGER PRIMARY KEY AUTOINCREMENT, name TEXT UNIQUE NOT NULL,
                rule_code TEXT NOT NULL, rule_value TEXT NOT NULL DEFAULT '', sort_order INTEGER NOT NULL DEFAULT 0);
            CREATE TABLE field_defaults (field_id INTEGER PRIMARY KEY, default_id INTEGER NOT NULL);
            CREATE TABLE group_item_params (id INTEGER PRIMARY KEY AUTOINCREMENT, group_item_id INTEGER NOT NULL,
                field_id INTEGER NOT NULL, rule_code TEXT NOT NULL, rule_value TEXT NOT NULL);
            INSERT INTO programs (id, job_code, job_name) VALUES (1, 'J1', 'job');
            INSERT INTO program_fields (id, program_id, field_scope, html_id, html_name, label, tag_type) VALUES
                (1, 1, 'PROGRAM', 'a', 'a', 'A', 'input'),
                (2, 1, 'PROGRAM', 'b', 'b', 'B', 'input'),
                (3, 1, 'PROGRAM', 'c', 'c', 'C', 'input'),
                (4, 1, 'PROGRAM', 'd', 'd', 'D', 'input');
            INSERT INTO default_values (id, name, rule_code, rule_value) VALUES
                (1, '前年度(西元)', 'PREV_YEAR', '2:AD'),
                (2, '前年度(民國)', 'PREV_YEAR', '2'),
                (3, '去年3月(西元)', 'LAST_YEAR_MONTH', '3:ad'),
                (4, '偏移', 'DAY_OFFSET', '-1');
            INSERT INTO field_defaults (field_id, default_id) VALUES (1, 1), (2, 2), (3, 3), (4, 4);
            INSERT INTO group_item_params (group_item_id, field_id, rule_code, rule_value)
                VALUES (1, 2, 'PREV_YEAR', '1:AD');
            "#,
        )
        .unwrap();

        init_db(&conn).unwrap();

        let ad = |id: i64| -> i64 {
            conn.query_row("SELECT year_ad FROM program_fields WHERE id = ?1", [id], |r| r.get(0))
                .unwrap()
        };
        assert_eq!(ad(1), 1, "選項帶 :AD → 欄位改為西元年");
        assert_eq!(ad(2), 1, "群組舊參數帶 :AD → 欄位改為西元年");
        assert_eq!(ad(3), 1, "小寫 :ad 也要轉換");
        assert_eq!(ad(4), 0, "非年度規則的欄位維持民國年");

        let val = |id: i64| -> String {
            conn.query_row("SELECT rule_value FROM default_values WHERE id = ?1", [id], |r| r.get(0))
                .unwrap()
        };
        assert_eq!(val(1), "2");
        assert_eq!(val(3), "3");
        assert_eq!(val(4), "-1");
        let gp: String = conn
            .query_row("SELECT rule_value FROM group_item_params", [], |r| r.get(0))
            .unwrap();
        assert_eq!(gp, "1");

        // 只轉換一次：之後手動把欄位改回民國年，再次啟動不會被改回西元年
        conn.execute("UPDATE program_fields SET year_ad = 0 WHERE id = 1", []).unwrap();
        init_db(&conn).unwrap();
        assert_eq!(ad(1), 0);
    }

    #[test]
    fn fresh_db_has_year_ad_default_roc() {
        let conn = Connection::open_in_memory().unwrap();
        init_db(&conn).unwrap();
        conn.execute(
            "INSERT INTO programs (id, job_code, job_name) VALUES (1, 'J', 'n')",
            [],
        )
        .unwrap();
        conn.execute(
            "INSERT INTO program_fields (program_id, field_scope, html_id, html_name, label, tag_type) \
             VALUES (1, 'PROGRAM', 'x', 'x', 'X', 'input')",
            [],
        )
        .unwrap();
        let v: i64 = conn
            .query_row("SELECT year_ad FROM program_fields", [], |r| r.get(0))
            .unwrap();
        assert_eq!(v, 0);
    }
}
