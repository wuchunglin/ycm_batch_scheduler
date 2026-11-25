//! Execution Runner View: real-time monitoring and log stream

use crate::engine::browser_runner::RunnerEvent;

#[derive(Debug, Clone)]
pub struct CapturedJobSn {
    pub job_code: String,
    pub sn: String,
}

pub struct RunnerViewState {
    pub is_running: bool,
    pub current_step: String,
    pub progress: (usize, usize),
    pub captured_sns: Vec<CapturedJobSn>,
    pub logs: Vec<String>,
}

impl Default for RunnerViewState {
    fn default() -> Self {
        Self {
            is_running: false,
            current_step: "就緒 (Idle)".to_string(),
            progress: (0, 0),
            captured_sns: Vec::new(),
            logs: vec![
                "[系統就緒] YCM 共用批次排程系統已載入本機 SQLite 資料庫".to_string(),
                "[系統資訊] 請先於「批次程式管理」匯入程式，並於「排程群組設定」編排執行步驟".to_string(),
            ],
        }
    }
}

impl RunnerViewState {
    pub fn handle_event(&mut self, event: RunnerEvent) {
        match event {
            RunnerEvent::Log {
                timestamp,
                level,
                message,
            } => {
                self.logs.push(format!("[{timestamp}] [{level}] {message}"));
            }
            RunnerEvent::StatusChanged { step, description } => {
                self.current_step = format!("{step} - {description}");
            }
            RunnerEvent::JobSnCaptured { job_code, sn } => {
                self.captured_sns.push(CapturedJobSn { job_code, sn });
            }
            RunnerEvent::ScreenshotSaved {
                job_code,
                file_path,
            } => {
                self.logs
                    .push(format!("[SCREENSHOT] {job_code} 截圖存檔: {file_path}"));
            }
            RunnerEvent::StepProgress { current, total } => {
                self.progress = (current, total);
            }
            RunnerEvent::Completed { total_executed } => {
                self.is_running = false;
                self.current_step = format!("執行完成 (共 {total_executed} 筆)");
                self.logs.push(format!(
                    "[DONE] 所有排程批次皆已順利執行完成（共 {total_executed} 筆）"
                ));
            }
            RunnerEvent::Failed { error } => {
                self.is_running = false;
                self.current_step = format!("執行失敗: {error}");
                self.logs.push(format!("[FAILED] {error}"));
            }
        }
    }
}
