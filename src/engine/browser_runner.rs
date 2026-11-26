use std::collections::HashMap;
use std::fs::OpenOptions;
use std::io::Write;

use chrono::Local;
use serde::{Deserialize, Serialize};
use tokio::sync::mpsc::UnboundedSender;

use crate::engine::param_resolver::ParamResolver;
use crate::models::{
    GeneratedScheduleItem, GroupItem, GroupItemParamDetail, JobGroup, Program,
};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum RunnerEvent {
    Log {
        timestamp: String,
        level: String,
        message: String,
    },
    StatusChanged {
        step: String,
        description: String,
    },
    JobSnCaptured {
        job_code: String,
        sn: String,
    },
    ScreenshotSaved {
        job_code: String,
        file_path: String,
    },
    StepProgress {
        current: usize,
        total: usize,
    },
    Completed {
        total_executed: usize,
    },
    Failed {
        error: String,
    },
}

#[derive(Debug, Clone)]
pub struct BrowserRunnerConfig {
    pub edge_path: String,
    pub target_url: String,
    pub pin: String,
    pub screenshot_dir: String,
    pub action_timeout_sec: u64,
    pub dry_run: bool,
}

#[derive(Debug, Clone)]
pub struct ExecutionStepInput {
    pub item: GroupItem,
    pub program: Program,
    pub params: Vec<GroupItemParamDetail>,
}

#[derive(Debug, Clone)]
pub struct ExecutionRequest {
    pub config: BrowserRunnerConfig,
    pub group: JobGroup,
    pub steps: Vec<ExecutionStepInput>,
    pub schedule_dates: Vec<GeneratedScheduleItem>,
    pub custom_vars: HashMap<String, String>,
}

pub fn current_log_file_path() -> String {
    let date_tag = Local::now().format("%Y%m%d").to_string();
    format!("logs/ycm_run_{date_tag}.log")
}

pub fn append_file_log(level: &str, msg: &str) {
    let _ = std::fs::create_dir_all("logs");
    let path = current_log_file_path();
    if let Ok(mut file) = OpenOptions::new().create(true).append(true).open(&path) {
        let ts = Local::now().format("%Y-%m-%d %H:%M:%S").to_string();
        let _ = writeln!(file, "[{ts}] [{level}] {msg}");
        let _ = file.flush();
    }
}

fn now_hms() -> String {
    Local::now().format("%H:%M:%S").to_string()
}

pub(crate) fn emit_log(tx: &UnboundedSender<RunnerEvent>, level: &str, msg: impl Into<String>) {
    let message = msg.into();
    append_file_log(level, &message);
    let _ = tx.send(RunnerEvent::Log {
        timestamp: now_hms(),
        level: level.to_string(),
        message,
    });
}

pub(crate) fn emit_status(
    tx: &UnboundedSender<RunnerEvent>,
    step: &str,
    desc: impl Into<String>,
) {
    let description = desc.into();
    append_file_log("STATUS", &format!("{step} - {description}"));
    let _ = tx.send(RunnerEvent::StatusChanged {
        step: step.to_string(),
        description,
    });
}

pub(crate) fn resolve_step_date(
    resolver: &ParamResolver,
    rule: &str,
    sched: &GeneratedScheduleItem,
) -> String {
    let trimmed = rule.trim();
    if trimmed.is_empty() || trimmed.eq_ignore_ascii_case("GROUP_DATE") {
        sched.roc_date.clone()
    } else if trimmed.to_uppercase().starts_with("WEEK_") {
        let raw = resolver.resolve_week_day(trimmed);
        if raw.len() == 7 {
            format!("{}/{}/{}", &raw[..3], &raw[3..5], &raw[5..7])
        } else {
            sched.roc_date.clone()
        }
    } else {
        sched.roc_date.clone()
    }
}

/// Generic Dual-Track execution engine (SINGLE F8->F6 loop / MULTI step-by-step chaining)
pub async fn execute_group(req: ExecutionRequest, tx: UnboundedSender<RunnerEvent>) {
    emit_log(
        &tx,
        "INFO",
        format!(
            "實體日誌檔路徑：{} | 群組「{}」({} 模式)",
            current_log_file_path(),
            req.group.group_name,
            req.group.group_mode
        ),
    );

    if req.steps.is_empty() {
        let msg = "群組內尚未加入任何批次程式步驟".to_string();
        append_file_log("FAILED", &msg);
        let _ = tx.send(RunnerEvent::Failed { error: msg });
        return;
    }

    if req.schedule_dates.is_empty() {
        let msg = "排程執行日期清單為空".to_string();
        append_file_log("FAILED", &msg);
        let _ = tx.send(RunnerEvent::Failed { error: msg });
        return;
    }

    let total_ops = req.schedule_dates.len() * req.steps.len();
    let _ = tx.send(RunnerEvent::StepProgress {
        current: 0,
        total: total_ops,
    });

    if let Err(err) = crate::engine::uia_runner::execute_uia(&req, &tx, total_ops).await {
        emit_log(&tx, "ERROR", format!("自動化執行中斷：{err}"));
        let _ = tx.send(RunnerEvent::Failed {
            error: err.to_string(),
        });
    }
}
