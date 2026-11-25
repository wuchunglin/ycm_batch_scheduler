use std::collections::HashMap;
use std::fs::OpenOptions;
use std::io::Write;
use std::time::Duration;

use chrono::Local;
use serde::{Deserialize, Serialize};
use tokio::sync::mpsc::UnboundedSender;

use crate::engine::param_resolver::{ParamResolver, RocDateUtils};
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

    if req.config.dry_run {
        execute_dry_run(&req, &tx, total_ops).await;
        return;
    }

    if let Err(err) = crate::engine::uia_runner::execute_uia(&req, &tx, total_ops).await {
        emit_log(&tx, "ERROR", format!("自動化執行中斷：{err}"));
        let _ = tx.send(RunnerEvent::Failed {
            error: err.to_string(),
        });
    }
}

async fn execute_dry_run(
    req: &ExecutionRequest,
    tx: &UnboundedSender<RunnerEvent>,
    total_ops: usize,
) {
    emit_status(
        tx,
        "預演模式 (Dry-Run)",
        format!(
            "群組「{}」({} 模式) 開始參數驗證",
            req.group.group_name, req.group.group_mode
        ),
    );
    emit_log(
        tx,
        "INFO",
        format!(
            "[DRY-RUN] 開始驗證「{}」共 {} 個排程日 × {} 個步驟（不實際操作瀏覽器與 F8 送出）",
            req.group.group_name,
            req.schedule_dates.len(),
            req.steps.len()
        ),
    );

    let mut completed = 0usize;
    let mut sn_seq = 900001usize;

    for sched in &req.schedule_dates {
        let base_date = RocDateUtils::parse_roc_date(&sched.roc_date_raw)
            .unwrap_or_else(|| Local::now().date_naive());
        let resolver = ParamResolver::new(base_date).with_custom_vars(req.custom_vars.clone());
        let mut prev_sn: Option<String> = None;

        for (step_idx, step) in req.steps.iter().enumerate() {
            tokio::time::sleep(Duration::from_millis(120)).await;
            let step_date = resolve_step_date(&resolver, &step.item.expect_exe_date_rule, sched);
            let step_time = if step.item.expect_exe_time.trim().is_empty() {
                sched.exe_time.clone()
            } else {
                step.item.expect_exe_time.clone()
            };

            let resolved_fields: Vec<String> = step
                .params
                .iter()
                .filter(|p| p.is_configured || !p.rule_value.is_empty())
                .map(|p| {
                    let val = resolver.resolve(&p.rule_code, &p.rule_value);
                    format!("{} [{}]={}", p.label, p.html_id, val)
                })
                .collect();

            let pre_sn = if step.item.link_prev_sn {
                prev_sn.clone().unwrap_or_else(|| "(上一步序號)".to_string())
            } else {
                step.item.pre_job_sn1.clone()
            };

            let mail_resolved = if step.item.req_email && !step.item.mail_recipients.is_empty() {
                resolver.resolve_template(&step.item.mail_recipients)
            } else {
                String::new()
            };

            let track_label = if req.group.group_mode == "SINGLE" && step_idx == 0 && completed > 0
            {
                "F6修改再執行➜F8"
            } else {
                "F1新增➜F8送出"
            };

            let mock_sn = format!("{sn_seq}");
            sn_seq += 1;
            prev_sn = Some(mock_sn.clone());
            completed += 1;

            let mut details = vec![format!("預計執行={step_date} {step_time}")];
            if !resolved_fields.is_empty() {
                details.push(format!("輸入參數: {}", resolved_fields.join(", ")));
            }
            if !pre_sn.is_empty() {
                details.push(format!("#preJobSn1={pre_sn}"));
            }
            if !mail_resolved.is_empty() {
                details.push(format!("#mailRecipients={mail_resolved}"));
            }

            emit_log(
                tx,
                "SUCCESS",
                format!(
                    "[{}/{}] {} ({}) [{}] | {} ➜ 模擬序號：{}",
                    completed,
                    total_ops,
                    step.program.job_code,
                    step.program.job_name,
                    track_label,
                    details.join(" | "),
                    mock_sn
                ),
            );
            let _ = tx.send(RunnerEvent::JobSnCaptured {
                job_code: step.program.job_code.clone(),
                sn: mock_sn,
            });
            let _ = tx.send(RunnerEvent::StepProgress {
                current: completed,
                total: total_ops,
            });
        }
    }

    emit_status(tx, "已完成", format!("預演驗證完成（共 {completed} 筆）"));
    let _ = tx.send(RunnerEvent::Completed {
        total_executed: completed,
    });
}
