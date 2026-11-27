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
    /// 每行日誌所屬的排程日期索引（與 logs 一一對應）
    pub log_dates: Vec<usize>,
    /// 每個排程日期要跑幾支程式（用來由進度推算目前日期）
    pub steps_per_date: usize,
    pub failed: bool,
    /// 使用者中斷執行
    pub cancelled: bool,
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
            log_dates: vec![0, 0],
            steps_per_date: 0,
            failed: false,
            cancelled: false,
        }
    }
}

impl RunnerViewState {
    /// 新增一行日誌，並記錄它屬於第幾個排程日期（由目前完成筆數推算）
    pub fn push_log(&mut self, line: String) {
        let idx = if self.steps_per_date > 0 {
            self.progress.0 / self.steps_per_date
        } else {
            0
        };
        self.logs.push(line);
        self.log_dates.push(idx);
    }

    /// 開始新一輪執行前清空日誌
    pub fn reset_logs(&mut self) {
        self.logs.clear();
        self.log_dates.clear();
    }

    pub fn handle_event(&mut self, event: RunnerEvent) {
        match event {
            RunnerEvent::Log {
                timestamp,
                level,
                message,
            } => {
                self.push_log(format!("[{timestamp}] [{level}] {message}"));
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
                self.push_log(format!("[SCREENSHOT] {job_code} 截圖存檔: {file_path}"));
            }
            RunnerEvent::StepProgress { current, total } => {
                self.progress = (current, total);
            }
            RunnerEvent::Completed { total_executed } => {
                self.is_running = false;
                self.current_step = format!("執行完成 (共 {total_executed} 筆)");
                self.push_log(format!(
                    "[DONE] 所有排程批次皆已順利執行完成（共 {total_executed} 筆）"
                ));
            }
            RunnerEvent::Cancelled => {
                self.is_running = false;
                self.cancelled = true;
                self.current_step = "已中斷".to_string();
                self.push_log("[WARN] 使用者已中斷執行，後續排程不再執行".to_string());
            }
            RunnerEvent::Failed { error } => {
                self.is_running = false;
                self.failed = true;
                self.current_step = format!("執行失敗: {error}");
                self.push_log(format!("[FAILED] {error}"));
            }
        }
    }
}
