use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct SysConfig {
    pub key: String,
    pub value: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct CustomVariable {
    pub var_key: String,
    pub var_value: String,
    pub description: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct Program {
    pub id: Option<i64>,
    pub job_code: String,
    pub job_name: String,
    pub created_at: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ProgramField {
    pub id: Option<i64>,
    pub program_id: i64,
    pub field_scope: String, // "JOB" or "PROGRAM"
    pub html_id: String,
    pub html_name: String,
    pub label: String,
    pub tag_type: String, // "input", "select", "textarea"
    pub max_length: usize,
    pub is_required: bool,
    pub sort_order: i32,
    /// 年度格式：false = 民國年（預設，{E1} → 1151127）、true = 西元年（{E1} → 20261127）
    #[serde(default)]
    pub year_ad: bool,
}

/// 預設輸入值清單項目（欄位的預設輸入值由此清單選擇）
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct DefaultValue {
    pub id: Option<i64>,
    pub name: String,
    pub rule_code: String, // "E1".."EG", "STATIC", ...
    pub rule_value: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct JobGroup {
    pub id: Option<i64>,
    pub group_name: String,
    pub description: String,
    pub group_mode: String, // "SINGLE" or "MULTI"
    pub schedule_rule_json: String,
    pub created_at: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct GroupItem {
    pub id: Option<i64>,
    pub group_id: i64,
    pub program_id: i64,
    pub step_order: i32,
    pub expect_exe_date_rule: String, // "GROUP_DATE", "WEEK_1".."WEEK_7", etc.
    pub expect_exe_time: String,      // "13:00", or "" for current system time
    pub req_op_confirm: bool,
    pub link_prev_sn: bool,
    pub pre_job_sn1: String,
    pub pre_job_sn2: String,
    pub pre_job_sn3: String,
    pub rework_sn: String,
    pub req_email: bool,
    pub mail_recipients: String,
    pub pre_work_note: String,
    pub rear_work_note: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct GroupItemParam {
    pub id: Option<i64>,
    pub group_item_id: i64,
    pub field_id: i64,
    pub rule_code: String, // "E1".."EG", "DAY_OFFSET", "MONTH_OFFSET", "WEEK_DAY", "MONTH_EDGE", "CUSTOM_TEMPLATE", "STATIC"
    pub rule_value: String,
}

/// Joined view model pairing a GroupItemParam with its ProgramField metadata (欄位名稱 + 欄位 ID).
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct GroupItemParamDetail {
    pub param_id: Option<i64>,
    pub group_item_id: i64,
    pub field_id: i64,
    pub field_scope: String,
    pub html_id: String,
    pub html_name: String,
    pub label: String,
    pub tag_type: String,
    pub is_required: bool,
    pub rule_code: String,
    pub rule_value: String,
    pub is_configured: bool,
    /// 欄位的年度格式：false = 民國年（預設）、true = 西元年
    #[serde(default)]
    pub year_ad: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(tag = "type", content = "value")]
pub enum DateRangeMode {
    AllYear,
    CustomRange {
        start: String, // YYY/MM/DD or YYYMMDD
        end: String,   // YYY/MM/DD or YYYMMDD
    },
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(tag = "mode")]
pub enum ScheduleMode {
    Monthly {
        days: Vec<u32>, // 1..=31
        last_day: bool,
    },
    Weekly {
        weekdays: Vec<u32>, // 1=Mon .. 7=Sun
    },
    SpecificDates {
        dates: Vec<String>, // "115/01/15" or "1150115"
    },
    SingleRun {
        date: Option<String>,
    },
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ScheduleRule {
    pub years: Vec<i32>, // ROC years, e.g. [115, 116]
    pub range_mode: DateRangeMode,
    pub schedule_mode: ScheduleMode,
    pub default_time: String, // "HH:mm", e.g. "13:00"
}

impl Default for ScheduleRule {
    fn default() -> Self {
        Self {
            years: vec![115],
            range_mode: DateRangeMode::AllYear,
            schedule_mode: ScheduleMode::Weekly { weekdays: vec![5] },
            default_time: "13:00".to_string(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct GeneratedScheduleItem {
    pub roc_date: String,     // "115/11/24"
    pub roc_date_raw: String, // "1151124"
    pub exe_time: String,     // "13:00"
}
