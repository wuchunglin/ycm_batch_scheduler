//! YCM Batch Scheduler Application Main Workspace View (Pure Rust GPUI Generic Version)

mod group;
mod group_setup_view;
mod date_picker;
mod group_view;
mod param_setting;
mod helpers;
mod program;
mod render;
mod runner;
mod schedule;
mod exec_preview_view;
mod schedule_exec_view;
mod settings;
mod state;

use std::collections::{HashMap, HashSet};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use chrono::Local;
use gpui::{
    div, px, AppContext, AsyncApp, AsyncWindowContext, ClickEvent, Context, Entity,
    FocusHandle, FontWeight, InteractiveElement, IntoElement, KeyDownEvent, MouseButton, MouseDownEvent,
    ParentElement, PathPromptOptions, Render, SharedString, StatefulInteractiveElement, Styled,
    WeakEntity, Window,
};
use gpui_component::{
    button::{Button, ButtonVariants},
    Sizable,
    checkbox::Checkbox,
    h_flex,
    input::{Input, InputState},
    notification::Notification,
    tag::Tag,
    v_flex, Disableable, Root, WindowExt,
};
use rusqlite::Connection;
use tokio::sync::mpsc;

use crate::theme::{
    hsl, BACKGROUND, BORDER, CARD, DANGER, FOREGROUND, MUTED_FOREGROUND, PRIMARY, SECONDARY,
    SUCCESS, WARNING,
};
use ycm_batch_scheduler::db::repository::Repository;
use ycm_batch_scheduler::engine::browser_runner::{
    append_file_log, current_log_file_path, execute_group, request_cancel, reset_cancel, BrowserRunnerConfig, ExecutionRequest,
    ExecutionStepInput, RunnerEvent,
};
use ycm_batch_scheduler::engine::param_resolver::{ParamResolver, RocDateUtils};
use ycm_batch_scheduler::engine::year_format::YearFormat;
use ycm_batch_scheduler::engine::schedule_gen::ScheduleGenerator;
use ycm_batch_scheduler::models::{
    CustomVariable, DateRangeMode, DefaultValue, GeneratedScheduleItem, GroupItem, GroupItemParamDetail,
    JobGroup, Program, ProgramField, ScheduleMode, ScheduleRule,
};
use ycm_batch_scheduler::parser::html_parser::parse_ycm_html;
use ycm_batch_scheduler::ui::runner_view::RunnerViewState;

#[allow(dead_code)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NavTab {
    ProgramManager,
    ScheduleExec,
    ExecPreview,
    GroupSetup,
    GroupEditor,
    ScheduleRule,
    ExecutionRunner,
    Settings,
}

/// 日曆選擇器對應的日期欄位
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CalTarget {
    RangeStart,
    RangeEnd,
    Specific(usize),
}

/// 欄位明細表中的一列（可直接在表格內編輯欄位名稱、欄位 ID 與預設輸入值）
struct FieldRow {
    base: Option<ProgramField>,
    label_input: Entity<InputState>,
    id_input: Entity<InputState>,
    default_id: Option<i64>,
    /// 自行輸入的預設值（可用 {E1} 等樣板；有填寫時優先於下拉選單）
    custom_input: Entity<InputState>,
    /// 年度格式（每個欄位各自設定）：false = 民國年（預設）、true = 西元年
    year_ad: bool,
}

/// 預覽頁一列可修改的參數
struct ExecParamRow {
    field_id: i64,
    label: String,
    default_name: String,
    configured: bool,
    /// 依預設輸入值轉換出的原值（用來判斷是否被修改）
    resolved: String,
    input: Entity<InputState>,
}

/// 群組設定頁中，群組內一筆批次程式的編輯狀態
struct GsItemEdit {
    item_id: i64,
    time_input: Entity<InputState>,
    link_prev: bool,
    req_email: bool,
}

/// 「執行清單」中的一筆批次程式（可調整順序、預計執行時間、前置工作序號、發送mail通知）
struct ExecEntry {
    program_id: i64,
    /// Some(群組 id)：由群組帶入（群組被刪除時用來一併移除）；None：單獨加入的批次程式
    source_group: Option<i64>,
    time_input: Entity<InputState>,
    link_prev: bool,
    req_email: bool,
}

/// 由「自行輸入」自動建立的預設值（名稱以「自訂：」開頭、規則為 CUSTOM_TEMPLATE）
pub(crate) fn is_custom_default(d: &DefaultValue) -> bool {
    d.rule_code == "CUSTOM_TEMPLATE" && d.name.starts_with("自訂：")
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
    field_rows: Vec<FieldRow>,
    field_rows_dirty: bool,
    open_picker: Option<usize>,
    /// 展開中的「年度格式」下拉選單（欄位列索引）
    open_year_picker: Option<usize>,
    year_picker_closed_by: Option<(usize, gpui::Point<gpui::Pixels>)>,
    /// 剛被「點選外部」關閉的下拉選單（列索引, 滑鼠位置），避免同一次點擊又被欄位重新打開
    picker_closed_by: Option<(usize, gpui::Point<gpui::Pixels>)>,
    /// 下拉選單的焦點（展開時取得焦點，才能接收 Esc）
    picker_focus: FocusHandle,
    default_values: Vec<DefaultValue>,
    field_defaults: HashMap<i64, i64>,
    prog_stats: HashMap<i64, (usize, usize)>,

    // --- 參數設定（維護下拉選單選項）State ---
    pd_open: bool,
    /// 「參數設定說明」彈窗是否開啟
    param_help_open: bool,
    /// 正在修改的選項 id；None = 新增
    pd_editing: Option<i64>,
    pd_name_input: Entity<InputState>,
    pd_value_input: Entity<InputState>,
    /// 規則類型（見 param_setting.rs 的 PD_RULES）
    pd_rule: String,
    /// 等待確認刪除的 (選項 id, 使用中的欄位數)
    pd_confirm_delete: Option<(i64, i64)>,

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

    // --- Group Setup (新增批次到群組) State ---
    gs_selected_program: Option<i64>,
    /// 群組內每個批次程式可編輯的欄位（預計執行時間 / 前置工作序號 / 發送mail通知），按「儲存變更」才寫入資料庫
    gs_edits: Vec<GsItemEdit>,
    /// gs_edits 目前對應的群組 id（切換群組時重建）
    gs_edits_group: Option<i64>,
    /// 離開群組設定頁時發現有未儲存的變更：暫存使用者要前往的頁籤，並顯示確認視窗
    leave_prompt: Option<NavTab>,

    // --- 執行清單 (排程設定及執行頁) ---
    exec_entries: Vec<ExecEntry>,
    /// 群組清單 / 批次程式清單目前點選（反白）的列，供標題列「加入執行清單」使用
    exec_pick_group: Option<i64>,
    exec_pick_program: Option<i64>,
    /// 年度別：Some(115/116)；None = 時間區間
    exec_year: Option<i32>,
    exec_range_start_input: Entity<InputState>,
    exec_range_end_input: Entity<InputState>,
    /// 排程日期模式：0 每月、1 最後一天、2 每周、3 指定日期
    exec_date_mode: u8,
    exec_monthly_input: Entity<InputState>,
    /// 每周勾選（一 ~ 日）
    exec_weekdays: [bool; 7],
    exec_specific_inputs: Vec<Entity<InputState>>,
    /// 預覽頁目前選取的基準日期（索引，對應產生出的排程日期）
    exec_preview_date_idx: usize,
    /// 預覽頁是否停在「總覽」頁籤
    exec_preview_overview: bool,
    /// 已檢視過的排程日期（索引）；全部檢視後才能「開始執行」
    exec_reviewed: HashSet<usize>,
    /// 執行主控台模式（按「開始執行」後顯示，直到使用者返回）
    exec_console_mode: bool,
    /// 開始執行時的日期標籤（供主控台頁籤使用）
    exec_run_dates: Vec<String>,
    /// 主控台目前頁籤：None = 全部日誌，Some(i) = 第 i 個日期
    exec_console_tab: Option<usize>,
    /// 日曆選擇器目前開啟的欄位、顯示的（西元年, 月）
    exec_cal_open: Option<CalTarget>,
    exec_cal_ym: (i32, u32),
    exec_cal_closed_by: Option<(CalTarget, gpui::Point<gpui::Pixels>)>,
    /// 預覽頁每個執行項目的可修改參數列（與 exec_entries 同順序）
    exec_param_rows: Vec<Vec<ExecParamRow>>,
    /// exec_param_rows 目前對應的排程日期（roc_date_raw）
    exec_param_date: String,
    exec_param_dirty: bool,
    /// 手動修改過的參數：(roc_date_raw, program_id, field_id) -> 值
    exec_param_overrides: HashMap<(String, i64, i64), String>,

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
    /// 按「中斷」後的確認視窗是否顯示
    interrupt_prompt: bool,
    /// 已確認中斷、等待執行引擎停止
    interrupt_requested: bool,
    runner_override_date_input: Entity<InputState>,

    // --- Settings & Variables State ---
    cfg_edge_path_input: Entity<InputState>,
    cfg_target_url_input: Entity<InputState>,
    cfg_pin_input: Entity<InputState>,
    cfg_screenshot_dir_input: Entity<InputState>,
    cfg_timeout_input: Entity<InputState>,
    custom_variables: Vec<CustomVariable>,
}
