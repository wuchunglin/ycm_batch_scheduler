//! YCM Batch Scheduler Application Main Workspace View (Pure Rust GPUI Generic Version)

mod group;
mod group_setup_view;
mod date_picker;
mod group_view;
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
    append_file_log, current_log_file_path, execute_group, BrowserRunnerConfig, ExecutionRequest,
    ExecutionStepInput, RunnerEvent,
};
use ycm_batch_scheduler::engine::param_resolver::{ParamResolver, RocDateUtils};
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
}

/// 「執行清單」中的一筆批次程式（可調整順序、預計執行時間、前置工作序號、發送mail通知）
struct ExecEntry {
    program_id: i64,
    /// Some(群組 id)：由勾選群組帶入；None：直接勾選批次程式
    source_group: Option<i64>,
    time_input: Entity<InputState>,
    link_prev: bool,
    req_email: bool,
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
    /// 剛被「點選外部」關閉的下拉選單（列索引, 滑鼠位置），避免同一次點擊又被欄位重新打開
    picker_closed_by: Option<(usize, gpui::Point<gpui::Pixels>)>,
    /// 下拉選單的焦點（展開時取得焦點，才能接收 Esc）
    picker_focus: FocusHandle,
    default_values: Vec<DefaultValue>,
    field_defaults: HashMap<i64, i64>,
    prog_stats: HashMap<i64, (usize, usize)>,

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
    gs_time_input: Entity<InputState>,
    gs_link_prev: bool,
    gs_req_email: bool,

    // --- 執行清單 (排程設定及執行頁) ---
    exec_entries: Vec<ExecEntry>,
    exec_sel_groups: HashSet<i64>,
    exec_sel_programs: HashSet<i64>,
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
}
