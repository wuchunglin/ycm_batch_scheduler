#[cfg(windows)]
mod win_uia {
    use std::collections::HashSet;
    use std::mem;
    use std::path::Path;
    use std::process::{Child, Command};
    use std::thread;
    use std::time::{Duration, Instant};

    use anyhow::{anyhow, Result};
    use chrono::Local;
    use regex::Regex;
    use tokio::sync::{mpsc::UnboundedSender, oneshot};
    use windows::core::{BOOL, BSTR};
    use windows::Win32::Foundation::{HWND, LPARAM, RECT};
    use windows::Win32::System::Variant::VARIANT;
    use windows::Win32::Graphics::Gdi::{
        BitBlt, CreateCompatibleBitmap, CreateCompatibleDC, DeleteDC, DeleteObject, GetDC,
        GetDIBits, ReleaseDC, SelectObject, BITMAPINFO, BITMAPINFOHEADER, BI_RGB, DIB_RGB_COLORS,
        HGDIOBJ, SRCCOPY,
    };
    use windows::Win32::Storage::Xps::{PrintWindow, PRINT_WINDOW_FLAGS};
    use windows::Win32::System::Com::{
        CoCreateInstance, CoInitializeEx, CoUninitialize, CLSCTX_ALL, COINIT_APARTMENTTHREADED,
    };
    use windows::Win32::UI::Accessibility::{
        CUIAutomation, IUIAutomation, IUIAutomationElement, IUIAutomationInvokePattern,
        IUIAutomationScrollItemPattern, IUIAutomationSelectionItemPattern, IUIAutomationTogglePattern, IUIAutomationValuePattern,
        ToggleState_On, TreeScope_Children, TreeScope_Descendants, UIA_AutomationIdPropertyId,
        UIA_ButtonControlTypeId, UIA_ControlTypePropertyId, UIA_CustomControlTypeId,
        UIA_DataItemControlTypeId, UIA_GroupControlTypeId, UIA_InvokePatternId,
        UIA_IsPasswordPropertyId,
        UIA_ListItemControlTypeId, UIA_ScrollItemPatternId, UIA_SelectionItemPatternId, UIA_TextControlTypeId,
        UIA_TogglePatternId, UIA_ValuePatternId,
    };
    use windows::Win32::UI::Input::KeyboardAndMouse::{
        SendInput, INPUT, INPUT_0, INPUT_KEYBOARD, INPUT_MOUSE, KEYBDINPUT, KEYBD_EVENT_FLAGS,
        KEYEVENTF_KEYUP, KEYEVENTF_UNICODE, MOUSEEVENTF_LEFTDOWN,
        MOUSEEVENTF_LEFTUP, MOUSEINPUT, VIRTUAL_KEY, VK_BACK, VK_CONTROL, VK_ESCAPE, VK_MENU, VK_RETURN,
    };
    use windows::Win32::UI::WindowsAndMessaging::{
        BringWindowToTop, EnumWindows, GetClassNameW, GetForegroundWindow, GetWindowRect, GetWindowTextW,
        GetWindowThreadProcessId, IsWindowVisible, SetCursorPos, SetForegroundWindow, ShowWindow,
        SW_RESTORE,
    };

    use crate::engine::browser_runner::{
        append_file_log, emit_log, emit_status, resolve_step_date, ExecutionRequest,
        ExecutionStepInput, RunnerEvent,
    };
    use crate::engine::param_resolver::{ParamResolver, RocDateUtils};
    use crate::models::GeneratedScheduleItem;

    pub async fn execute_uia(
        req: &ExecutionRequest,
        tx: &UnboundedSender<RunnerEvent>,
        total_ops: usize,
    ) -> Result<()> {
        let req_cloned = req.clone();
        let tx_cloned = tx.clone();
        let (done_tx, done_rx) = oneshot::channel::<Result<usize>>();

        thread::Builder::new()
            .name("ycm-uia-automation".to_string())
            .spawn(move || {
                let res = run_uia_thread(&req_cloned, &tx_cloned, total_ops);
                let _ = done_tx.send(res);
            })
            .map_err(|e| anyhow!("建立 Windows UI Automation 執行緒失敗: {e}"))?;

        let completed = done_rx
            .await
            .map_err(|_| anyhow!("Windows UI Automation 執行緒意外中止"))??;

        emit_status(tx, "已完成", format!("全數執行完成（共 {completed} 筆）"));
        append_file_log("DONE", &format!("全數執行完成（共 {completed} 筆）"));
        let _ = tx.send(RunnerEvent::Completed {
            total_executed: completed,
        });
        Ok(())
    }

    /// 是否在填寫完參數後點擊 #btnF8 送出排程。
    /// 預演模式：照排程步驟逐步開啟批次程式並填寫參數，但不點 F8；
    /// 正式執行：填完參數後點 F8 送出。
    fn submit_f8(req: &ExecutionRequest) -> bool {
        !req.config.dry_run
    }

    struct UiaSession {
        uia: IUIAutomation,
        edge_hwnd: HWND,
        root: IUIAutomationElement,
        child: Option<Child>,
    }

    impl UiaSession {
        fn is_edge_foreground(&self) -> bool {
            unsafe { GetForegroundWindow() == self.edge_hwnd }
        }

        /// Bring Edge to the foreground and report whether it really got there.
        /// Windows refuses SetForegroundWindow when another process owns the
        /// foreground, so retry with an Alt tap (sent to the *current* foreground
        /// window, i.e. our own app) which lifts the restriction.
        fn focus_edge(&self) -> bool {
            for attempt in 0..4 {
                unsafe {
                    let _ = ShowWindow(self.edge_hwnd, SW_RESTORE);
                    let _ = SetForegroundWindow(self.edge_hwnd);
                    let _ = BringWindowToTop(self.edge_hwnd);
                }
                thread::sleep(Duration::from_millis(120));
                if self.is_edge_foreground() {
                    return true;
                }
                if attempt >= 1 {
                    send_vk_tap(VK_MENU);
                }
                thread::sleep(Duration::from_millis(120));
            }
            self.is_edge_foreground()
        }

        fn close(&mut self) {
            if let Some(ref mut ch) = self.child {
                let _ = ch.kill();
                let _ = ch.wait();
            }
        }
    }

    struct ComGuard;
    impl Drop for ComGuard {
        fn drop(&mut self) {
            unsafe {
                CoUninitialize();
            }
        }
    }

    fn snapshot_edge_windows() -> HashSet<usize> {
        let mut set = HashSet::new();
        for (hwnd, _, _) in list_chrome_windows() {
            set.insert(hwnd.0 as usize);
        }
        set
    }

    unsafe extern "system" fn enum_windows_proc(hwnd: HWND, lparam: LPARAM) -> BOOL {
        let out = &mut *(lparam.0 as *mut Vec<(HWND, u32, String)>);
        if !IsWindowVisible(hwnd).as_bool() {
            return BOOL(1);
        }

        let mut cls_buf = [0u16; 256];
        let cls_len = GetClassNameW(hwnd, &mut cls_buf);
        if cls_len <= 0 {
            return BOOL(1);
        }
        let cls_name = String::from_utf16_lossy(&cls_buf[..cls_len as usize]);
        if cls_name != "Chrome_WidgetWin_1" {
            return BOOL(1);
        }

        let mut title_buf = [0u16; 512];
        let title_len = GetWindowTextW(hwnd, &mut title_buf);
        let title = if title_len > 0 {
            String::from_utf16_lossy(&title_buf[..title_len as usize])
        } else {
            String::new()
        };
        if title.trim().is_empty() {
            return BOOL(1);
        }

        let mut pid = 0u32;
        let _ = GetWindowThreadProcessId(hwnd, Some(&mut pid));
        out.push((hwnd, pid, title));
        BOOL(1)
    }

    fn list_chrome_windows() -> Vec<(HWND, u32, String)> {
        let mut out: Vec<(HWND, u32, String)> = Vec::new();
        unsafe {
            let _ = EnumWindows(
                Some(enum_windows_proc),
                LPARAM(&mut out as *mut _ as isize),
            );
        }
        out
    }

    fn launch_edge_uia(
        edge_path: &str,
        target_url: &str,
        tx: &UnboundedSender<RunnerEvent>,
    ) -> Result<(HWND, Option<Child>)> {
        let resolved_edge = [
            edge_path.trim(),
            r"C:\Program Files (x86)\Microsoft\Edge\Application\msedge.exe",
            r"C:\Program Files\Microsoft\Edge\Application\msedge.exe",
        ]
        .into_iter()
        .find(|p| !p.is_empty() && Path::new(p).exists())
        .map(str::to_string)
        .ok_or_else(|| anyhow!("找不到 Edge 執行檔 ({edge_path})，請於系統設定確認路徑"))?;

        let before_hwnds = snapshot_edge_windows();

        emit_log(
            tx,
            "INFO",
            format!(
                "以 Windows UI Automation 模式啟動 Edge（使用一般設定檔）：exe={}",
                resolved_edge
            ),
        );

        // 與參考腳本(login_common.py)一致：使用使用者「一般」Edge 設定檔，
        // 不指定 --user-data-dir，才能沿用其自然人憑證/擴充套件/信任設定。
        // 若 Edge 已在執行，新行程會把網址交給既有 Edge 後立即結束，
        // 因此下方以「新出現的視窗」判斷目標視窗。
        let child = Command::new(&resolved_edge)
            .arg("--force-renderer-accessibility")
            .arg("--new-window")
            .arg(target_url)
            .spawn()
            .map_err(|e| anyhow!("啟動 Edge 失敗 ({resolved_edge}): {e}"))?;

        let pid = child.id();
        let deadline = Instant::now() + Duration::from_secs(20);
        while Instant::now() < deadline {
            let candidates = list_chrome_windows();
            // 1. Exact PID match first
            if let Some((hwnd, _, title)) = candidates.iter().find(|(_, p, _)| *p == pid) {
                emit_log(
                    tx,
                    "INFO",
                    format!("已鎖定 Edge 視窗 (PID={pid}, HWND={:?}, 標題={title})", hwnd.0),
                );
                return Ok((*hwnd, Some(child)));
            }
            // 2. Newly appeared Chrome_WidgetWin_1 window
            if let Some((hwnd, wp, title)) = candidates
                .iter()
                .find(|(h, _, _)| !before_hwnds.contains(&(h.0 as usize)))
            {
                emit_log(
                    tx,
                    "INFO",
                    format!(
                        "已偵測到新開啟 Edge 視窗 (PID={wp}, HWND={:?}, 標題={title})",
                        hwnd.0
                    ),
                );
                return Ok((*hwnd, Some(child)));
            }
            // 3. Fallback by Edge/YCM title if user already had matching tab
            if let Some((hwnd, wp, title)) = candidates
                .iter()
                .find(|(_, _, t)| t.contains("Microsoft Edge") || t.contains("YCM") || t.contains("getSysData"))
            {
                if before_hwnds.is_empty() {
                    emit_log(
                        tx,
                        "INFO",
                        format!("已附掛既有 Edge 視窗 (PID={wp}, HWND={:?}, 標題={title})", hwnd.0),
                    );
                    return Ok((*hwnd, None));
                }
            }
            thread::sleep(Duration::from_millis(250));
        }

        Err(anyhow!("等待 Edge 視窗出現逾時（20 秒）"))
    }

    fn run_uia_thread(
        req: &ExecutionRequest,
        tx: &UnboundedSender<RunnerEvent>,
        total_ops: usize,
    ) -> Result<usize> {
        unsafe {
            let hr = CoInitializeEx(None, COINIT_APARTMENTTHREADED);
            if hr.is_err() {
                return Err(anyhow!("CoInitializeEx 失敗: {hr:?}"));
            }
        }
        let _com_guard = ComGuard;

        emit_status(
            tx,
            "啟動瀏覽器",
            "以 Windows UI Automation AP 方式啟動 Microsoft Edge",
        );

        let (edge_hwnd, child) =
            launch_edge_uia(&req.config.edge_path, &req.config.target_url, tx)?;

        let uia: IUIAutomation = unsafe {
            CoCreateInstance(&CUIAutomation, None, CLSCTX_ALL)
                .map_err(|e| anyhow!("建立 IUIAutomation COM 失敗: {e}"))?
        };

        let root = unsafe {
            uia.ElementFromHandle(edge_hwnd)
                .map_err(|e| anyhow!("IUIAutomation::ElementFromHandle 失敗: {e}"))?
        };

        let mut session = UiaSession {
            uia,
            edge_hwnd,
            root,
            child,
        };
        session.focus_edge();

        let timeout = Duration::from_secs(req.config.action_timeout_sec.max(15));
        let run_res = run_uia_workflow(&mut session, req, tx, total_ops, timeout);

        match run_res {
            Ok(completed) => {
                if submit_f8(req) {
                    session.close();
                } else {
                    emit_log(tx, "INFO", "預演完成：所有步驟已填寫（未點 F8），Edge 視窗保持開啟");
                }
                Ok(completed)
            }
            Err(err) => {
                emit_log(
                    tx,
                    "WARN",
                    format!("執行異常：{err}（保留 Edge 視窗 10 秒供檢視畫面，結束後不強制關閉使用者手動視窗）"),
                );
                emit_status(tx, "執行異常", "保留 Edge 視窗 10 秒供檢視原因...");
                thread::sleep(Duration::from_secs(10));
                session.close();
                Err(err)
            }
        }
    }

    fn run_uia_workflow(
        s: &mut UiaSession,
        req: &ExecutionRequest,
        tx: &UnboundedSender<RunnerEvent>,
        total_ops: usize,
        timeout: Duration,
    ) -> Result<usize> {
        emit_status(tx, "開啟網址", format!("導航至 {}", req.config.target_url));
        emit_log(tx, "INFO", format!("Edge 已開啟網址：{}", req.config.target_url));

        // 登入檢查：已登入錨點 (#headerUserInfo) 或 PIN 欄位二擇一，皆未出現則中止，不再盲目往下執行。
        ensure_logged_in(s, req, tx)?;

        emit_status(
            tx,
            "等待畫面就緒",
            "等待 #SampleGrid 載入（若需憑證或手動 SSO 登入，請於 Edge 視窗完成）",
        );

        let do_f8 = submit_f8(req);
        let mut completed = 0usize;
        let mut is_very_first_navigation = true;

        if req.group.group_mode.eq_ignore_ascii_case("SINGLE") {
            let step = &req.steps[0];
            let mut is_first_batch = true;

            for sched in &req.schedule_dates {
                let base_date = RocDateUtils::parse_roc_date(&sched.roc_date_raw)
                    .unwrap_or_else(|| Local::now().date_naive());
                let resolver = ParamResolver::new(base_date).with_custom_vars(req.custom_vars.clone());

                if is_first_batch || !do_f8 {
                    open_job_form_uia(
                        s,
                        &req.config.target_url,
                        &step.program.job_code,
                        timeout,
                        is_very_first_navigation,
                        tx,
                    )?;
                    is_very_first_navigation = false;
                    is_first_batch = false;
                } else {
                    emit_status(tx, "等待F6啟用", "輪詢 #btnF6 (F6修改再執行) 變為可用狀態");
                    wait_enabled_id(s, "btnF6", timeout)?;
                    click_id(s, "btnF6")?;
                    wait_enabled_id(s, "btnF8", timeout)?;
                }

                emit_status(
                    tx,
                    "填寫參數",
                    format!("填寫 {} ({})", step.program.job_code, sched.roc_date),
                );
                fill_step_fields_uia(s, &resolver, step, sched, None)?;

                if !do_f8 {
                    completed += 1;
                    emit_log(
                        tx,
                        "SUCCESS",
                        format!(
                            "[{}/{}] {} ({}) ➜ 預演：參數已填寫（未點 F8）",
                            completed, total_ops, step.program.job_code, sched.roc_date
                        ),
                    );
                    let _ = tx.send(RunnerEvent::StepProgress {
                        current: completed,
                        total: total_ops,
                    });
                    continue;
                }

                emit_status(tx, "F8送出", format!("送出 {} 排程", step.program.job_code));
                click_id(s, "btnF8")?;
                let sn = wait_op_msg_sn_uia(s, timeout)?;
                let shot_path = save_window_screenshot(
                    s.edge_hwnd,
                    &req.config.screenshot_dir,
                    &step.program.job_code,
                    &sched.roc_date_raw,
                    completed + 1,
                )?;

                completed += 1;
                emit_log(
                    tx,
                    "SUCCESS",
                    format!(
                        "[{}/{}] {} ({}) ➜ F8送出成功 ➜ 序號: {} ➜ 截圖: {}",
                        completed, total_ops, step.program.job_code, sched.roc_date, sn, shot_path
                    ),
                );
                let _ = tx.send(RunnerEvent::JobSnCaptured {
                    job_code: step.program.job_code.clone(),
                    sn,
                });
                let _ = tx.send(RunnerEvent::ScreenshotSaved {
                    job_code: step.program.job_code.clone(),
                    file_path: shot_path,
                });
                let _ = tx.send(RunnerEvent::StepProgress {
                    current: completed,
                    total: total_ops,
                });
            }
        } else {
            for sched in &req.schedule_dates {
                let base_date = RocDateUtils::parse_roc_date(&sched.roc_date_raw)
                    .unwrap_or_else(|| Local::now().date_naive());
                let resolver = ParamResolver::new(base_date).with_custom_vars(req.custom_vars.clone());
                let mut prev_sn: Option<String> = None;

                for step in &req.steps {
                    emit_status(
                        tx,
                        "查詢程式",
                        format!("查詢並開啟 {}", step.program.job_code),
                    );
                    open_job_form_uia(
                        s,
                        &req.config.target_url,
                        &step.program.job_code,
                        timeout,
                        is_very_first_navigation,
                        tx,
                    )?;
                    is_very_first_navigation = false;

                    emit_status(
                        tx,
                        "填寫參數",
                        format!("填寫 {} ({})", step.program.job_code, sched.roc_date),
                    );
                    fill_step_fields_uia(s, &resolver, step, sched, prev_sn.as_deref())?;

                    if !do_f8 {
                        completed += 1;
                        emit_log(
                            tx,
                            "SUCCESS",
                            format!(
                                "[{}/{}] {} ({}) ➜ 預演：參數已填寫（未點 F8）",
                                completed, total_ops, step.program.job_code, sched.roc_date
                            ),
                        );
                        let _ = tx.send(RunnerEvent::StepProgress {
                            current: completed,
                            total: total_ops,
                        });
                        continue;
                    }

                    emit_status(tx, "F8送出", format!("送出 {} 排程", step.program.job_code));
                    click_id(s, "btnF8")?;
                    let sn = wait_op_msg_sn_uia(s, timeout)?;
                    prev_sn = Some(sn.clone());

                    let shot_path = save_window_screenshot(
                        s.edge_hwnd,
                        &req.config.screenshot_dir,
                        &step.program.job_code,
                        &sched.roc_date_raw,
                        completed + 1,
                    )?;

                    completed += 1;
                    emit_log(
                        tx,
                        "SUCCESS",
                        format!(
                            "[{}/{}] {} ({}) ➜ F8送出成功 ➜ 序號: {} ➜ 截圖: {}",
                            completed,
                            total_ops,
                            step.program.job_code,
                            sched.roc_date,
                            sn,
                            shot_path
                        ),
                    );
                    let _ = tx.send(RunnerEvent::JobSnCaptured {
                        job_code: step.program.job_code.clone(),
                        sn,
                    });
                    let _ = tx.send(RunnerEvent::ScreenshotSaved {
                        job_code: step.program.job_code.clone(),
                        file_path: shot_path,
                    });
                    let _ = tx.send(RunnerEvent::StepProgress {
                        current: completed,
                        total: total_ops,
                    });
                }
            }
        }

        Ok(completed)
    }

    fn navigate_via_address_bar(s: &mut UiaSession, url: &str) {
        s.focus_edge();
        send_ctrl_key(0x4C); // Ctrl+L focuses Edge address bar across all locales
        thread::sleep(Duration::from_millis(160));
        send_unicode_text(url);
        thread::sleep(Duration::from_millis(80));
        send_vk_tap(VK_RETURN);
        thread::sleep(Duration::from_millis(900));
        if let Ok(fresh_root) = unsafe { s.uia.ElementFromHandle(s.edge_hwnd) } {
            s.root = fresh_root;
        }
    }

    fn open_job_form_uia(
        s: &mut UiaSession,
        target_url: &str,
        job_code: &str,
        timeout: Duration,
        skip_initial_goto: bool,
        tx: &UnboundedSender<RunnerEvent>,
    ) -> Result<()> {
        if !skip_initial_goto {
            navigate_via_address_bar(s, target_url);
        }

        let grid_timeout = if skip_initial_goto {
            timeout.max(Duration::from_secs(60))
        } else {
            timeout
        };
        let grid_el = wait_sample_grid_ready_uia(s, grid_timeout)?;
        drop(grid_el);

        set_id_value(s, "fuzzyInquiry", job_code)?;
        click_id(s, "btnF4")?;
        thread::sleep(Duration::from_millis(450));

        let grid_after = wait_sample_grid_ready_uia(s, timeout)?;
        let rows = collect_table_rows(s, &grid_after)?;
        if rows.len() < 2 {
            return Err(anyhow!("搜尋 [{job_code}] 後 #SampleGrid 無可選資料列"));
        }

        // Click the first data row (row index 1; index 0 is table header)
        let first_data_row = &rows[1];
        click_table_row(s, first_data_row, tx)?;
        thread::sleep(Duration::from_millis(350));
        click_id(s, "btnF1")?;

        let deadline = Instant::now() + timeout;
        while Instant::now() < deadline {
            if let Ok(el) = find_by_id(s, "jobCodeNumber") {
                if get_element_text(s, &el).trim() == job_code {
                    return Ok(());
                }
            }
            thread::sleep(Duration::from_millis(300));
        }

        Err(anyhow!("驗證 #jobCodeNumber == {job_code} 超時"))
    }

    fn fill_step_fields_uia(
        s: &UiaSession,
        resolver: &ParamResolver,
        step: &ExecutionStepInput,
        sched: &GeneratedScheduleItem,
        prev_sn: Option<&str>,
    ) -> Result<()> {
        for param in &step.params {
            if !param.is_configured && param.rule_value.is_empty() {
                continue;
            }
            let val = resolver.resolve(&param.rule_code, &param.rule_value);
            set_id_value(s, &param.html_id, &val)?;
        }

        set_checkbox_id(s, "reqOPConfirm", step.item.req_op_confirm)?;
        set_checkbox_id(s, "reqEmail", step.item.req_email)?;

        if step.item.req_email && !step.item.mail_recipients.trim().is_empty() {
            let resolved_mail = resolver.resolve_template(&step.item.mail_recipients);
            set_id_value(s, "mailRecipients", &resolved_mail)?;
        }

        let pre1 = if step.item.link_prev_sn {
            prev_sn
                .ok_or_else(|| {
                    anyhow!(
                        "步驟 {} 需要前置序號，但上一步序號為空",
                        step.program.job_code
                    )
                })?
                .to_string()
        } else {
            step.item.pre_job_sn1.clone()
        };
        if !pre1.is_empty() {
            set_id_value(s, "preJobSn1", &pre1)?;
        }
        if !step.item.pre_job_sn2.is_empty() {
            set_id_value(s, "preJobSn2", &step.item.pre_job_sn2)?;
        }
        if !step.item.pre_job_sn3.is_empty() {
            set_id_value(s, "preJobSn3", &step.item.pre_job_sn3)?;
        }
        if !step.item.rework_sn.is_empty() {
            set_id_value(s, "reWorkSn", &step.item.rework_sn)?;
        }
        if !step.item.pre_work_note.is_empty() {
            set_id_value(s, "preWork", &step.item.pre_work_note)?;
        }
        if !step.item.rear_work_note.is_empty() {
            set_id_value(s, "rearWork", &step.item.rear_work_note)?;
        }

        let exe_date = resolve_step_date(resolver, &step.item.expect_exe_date_rule, sched);
        set_id_value(s, "expectExeDate", &exe_date)?;
        dismiss_date_picker(s);

        let exe_time = if step.item.expect_exe_time.trim().is_empty() {
            sched.exe_time.clone()
        } else {
            step.item.expect_exe_time.clone()
        };
        if !exe_time.is_empty() {
            set_id_value(s, "expectExeTime", &exe_time)?;
            dismiss_date_picker(s);
        }

        Ok(())
    }

    /// 填入日期／時間後送出 Esc，關閉 My97 日期選擇器浮層，避免擋住後續欄位與 F8 按鈕。
    fn dismiss_date_picker(s: &UiaSession) {
        s.focus_edge();
        thread::sleep(Duration::from_millis(120));
        send_vk_tap(VK_ESCAPE);
        thread::sleep(Duration::from_millis(150));
    }

    fn clean_id(id: &str) -> &str {
        id.trim().trim_start_matches('#')
    }

    fn find_by_id(s: &UiaSession, id: &str) -> Result<IUIAutomationElement> {
        let pure_id = clean_id(id);
        let cond = unsafe {
            s.uia.CreatePropertyCondition(
                UIA_AutomationIdPropertyId,
                &VARIANT::from(BSTR::from(pure_id)),
            )
        }
        .map_err(|e| anyhow!("CreatePropertyCondition 失敗 ({pure_id}): {e}"))?;

        let el = unsafe { s.root.FindFirst(TreeScope_Descendants, &cond) }
            .map_err(|_| anyhow!("找不到 AutomationId={pure_id} 的網頁控制項"))?;

        if unsafe { el.CurrentControlType() }.is_err() {
            return Err(anyhow!("找不到 AutomationId={pure_id} 的網頁控制項"));
        }
        Ok(el)
    }

    fn refresh_root(s: &mut UiaSession) {
        if let Ok(fresh) = unsafe { s.uia.ElementFromHandle(s.edge_hwnd) } {
            s.root = fresh;
        }
    }

    /// 已登入錨點：登入後的頁面檔頭一定有 #headerUserInfo（姓名＋員工代號），
    /// 且 getSysData 頁面會有 #SampleGrid / #fuzzyInquiry。PIN 登入頁沒有這些節點，也沒有 #pin 以外的上述 id。
    /// 回傳 Some(使用者資訊文字)（可能為空字串）表示已登入。
    fn detect_logged_in(s: &UiaSession) -> Option<String> {
        let header = find_by_id(s, "headerUserInfo").ok()?;
        let text = get_element_text(s, &header).trim().to_string();
        if !text.is_empty() {
            return Some(text);
        }
        if find_by_id(s, "SampleGrid").is_ok() || find_by_id(s, "fuzzyInquiry").is_ok() {
            return Some(String::new());
        }
        None
    }

    fn find_pin_once(s: &UiaSession, tx: &UnboundedSender<RunnerEvent>) -> Option<IUIAutomationElement> {
        if let Ok(el) = find_by_id(s, "pin") {
            emit_log(tx, "INFO", "以 AutomationId=pin 找到 PIN 欄位");
            return Some(el);
        }
        if let Some(el) = find_password_edit(s) {
            emit_log(tx, "INFO", "以「密碼欄位」屬性找到 PIN 欄位");
            return Some(el);
        }
        if let Some(el) = find_pin_by_enumeration(s) {
            emit_log(tx, "INFO", "以逐一列舉節點找到 PIN 欄位");
            return Some(el);
        }
        None
    }

    fn log_logged_in(tx: &UnboundedSender<RunnerEvent>, who: &str) {
        if who.is_empty() {
            emit_log(tx, "INFO", "已偵測到登入錨點（#SampleGrid / #fuzzyInquiry），視為已登入");
        } else {
            emit_log(tx, "INFO", format!("已偵測到登入錨點 #headerUserInfo：{who}，視為已登入"));
        }
    }

    /// 等待登入錨點出現（PIN 送出後，或使用者手動登入時使用）。逾時回傳錯誤。
    fn wait_logged_in(
        s: &mut UiaSession,
        timeout: Duration,
        tx: &UnboundedSender<RunnerEvent>,
    ) -> Result<()> {
        let deadline = Instant::now() + timeout;
        while Instant::now() < deadline {
            refresh_root(s);
            if let Some(who) = detect_logged_in(s) {
                log_logged_in(tx, &who);
                return Ok(());
            }
            // PIN 登入頁的錯誤訊息區 #certErrorDisplay 一旦出現文字（PIN 錯誤、元件未啟動等）即提早中止
            if let Ok(err_el) = find_by_id(s, "certErrorDisplay") {
                let msg = get_element_text(s, &err_el).trim().to_string();
                if !msg.is_empty() {
                    return Err(anyhow!("登入頁回報錯誤：{msg}"));
                }
            }
            thread::sleep(Duration::from_millis(500));
        }
        emit_log(tx, "WARN", format!("最終診斷：{}", uia_diag_summary(s)));
        Err(anyhow!(
            "等待 {} 秒仍未偵測到登入錨點（#headerUserInfo），請確認 PIN 是否正確或已完成登入",
            timeout.as_secs()
        ))
    }

    /// 登入流程：
    /// 1) 已有登入錨點 → 直接略過 PIN；
    /// 2) 找到 #pin → 輸入 PIN 並等待登入錨點；
    /// 3) 25 秒內兩者皆沒有 → 中止（不再往下執行排程）。
    fn ensure_logged_in(
        s: &mut UiaSession,
        req: &ExecutionRequest,
        tx: &UnboundedSender<RunnerEvent>,
    ) -> Result<()> {
        emit_status(tx, "登入檢查", "偵測登入狀態（#headerUserInfo 或 #pin 欄位）");

        let start = Instant::now();
        let detect_timeout = Duration::from_secs(25);
        let mut attempt = 0u32;
        let mut pin_el: Option<IUIAutomationElement> = None;

        while start.elapsed() < detect_timeout {
            attempt += 1;
            refresh_root(s);

            if let Some(who) = detect_logged_in(s) {
                log_logged_in(tx, &who);
                return Ok(());
            }
            if let Some(el) = find_pin_once(s, tx) {
                pin_el = Some(el);
                break;
            }
            if attempt % 10 == 1 {
                emit_log(
                    tx,
                    "DEBUG",
                    format!(
                        "尚未偵測到登入錨點或 PIN 欄位（{}s）：{}",
                        start.elapsed().as_secs(),
                        uia_diag_summary(s)
                    ),
                );
            }
            thread::sleep(Duration::from_millis(500));
        }

        let Some(pin_el) = pin_el else {
            emit_log(tx, "WARN", format!("最終診斷：{}", uia_diag_summary(s)));
            return Err(anyhow!(
                "{} 秒內未偵測到 #pin 欄位，也未偵測到已登入錨點（#headerUserInfo），無法確認登入狀態，已中止排程",
                detect_timeout.as_secs()
            ));
        };

        if req.config.pin.trim().is_empty() {
            emit_log(
                tx,
                "WARN",
                "偵測到 PIN 登入頁，但系統設定的預設 PIN 碼為空，請於 Edge 視窗手動輸入並登入（最多等待 120 秒）",
            );
            return wait_logged_in(s, Duration::from_secs(120), tx);
        }

        emit_status(tx, "登入中", "輸入 PIN 碼並送出登入");
        type_pin_and_submit(s, &pin_el, req.config.pin.trim(), tx)?;
        emit_log(tx, "INFO", "PIN 碼已輸入並送出登入，等待登入錨點出現...");
        wait_logged_in(s, Duration::from_secs(60), tx)
    }

    /// Enumerate descendants (capped) looking for id "pin" or a password edit.
    fn find_pin_by_enumeration(s: &UiaSession) -> Option<IUIAutomationElement> {
        let all = unsafe { s.uia.CreateTrueCondition() }.ok()?;
        let arr = unsafe { s.root.FindAll(TreeScope_Descendants, &all) }.ok()?;
        let len = unsafe { arr.Length().unwrap_or(0) }.min(5000);
        for i in 0..len {
            if let Ok(el) = unsafe { arr.GetElement(i) } {
                let id = unsafe { el.CurrentAutomationId() }
                    .map(|b| b.to_string())
                    .unwrap_or_default();
                if id == "pin" {
                    return Some(el);
                }
                let is_pw = unsafe { el.CurrentIsPassword() }
                    .map(|b| b.as_bool())
                    .unwrap_or(false);
                if is_pw {
                    return Some(el);
                }
            }
        }
        None
    }

    /// One-line description of what UIA currently sees in the Edge window.
    fn uia_diag_summary(s: &UiaSession) -> String {
        let mut title_buf = [0u16; 256];
        let n = unsafe { GetWindowTextW(s.edge_hwnd, &mut title_buf) };
        let title = if n > 0 {
            String::from_utf16_lossy(&title_buf[..n as usize])
        } else {
            String::new()
        };
        let mut docs = 0usize;
        let mut edits: Vec<String> = Vec::new();
        let mut total = 0i32;
        if let Ok(all) = unsafe { s.uia.CreateTrueCondition() } {
            if let Ok(arr) = unsafe { s.root.FindAll(TreeScope_Descendants, &all) } {
                total = unsafe { arr.Length().unwrap_or(0) };
                for i in 0..total.min(5000) {
                    if let Ok(el) = unsafe { arr.GetElement(i) } {
                        let ct = unsafe { el.CurrentControlType() }.unwrap_or(UIA_CustomControlTypeId);
                        if ct.0 == 50030 {
                            docs += 1;
                        } else if ct.0 == 50004 && edits.len() < 6 {
                            let id = unsafe { el.CurrentAutomationId() }
                                .map(|b| b.to_string())
                                .unwrap_or_default();
                            let nm = unsafe { el.CurrentName() }
                                .map(|b| b.to_string())
                                .unwrap_or_default();
                            edits.push(format!("Edit(id='{id}', name='{nm}')"));
                        }
                    }
                }
            }
        }
        format!(
            "視窗標題='{title}', UIA 節點數={total}, Document 數={docs}, Edit 欄位=[{}]",
            edits.join("; ")
        )
    }

    /// First *visible* password edit. The page also contains a second password
    /// input (#pincode, 臨時憑證 panel) that must never be picked while hidden,
    /// so off-screen / zero-size candidates are skipped.
    fn find_password_edit(s: &UiaSession) -> Option<IUIAutomationElement> {
        let pw = unsafe {
            s.uia
                .CreatePropertyCondition(UIA_IsPasswordPropertyId, &VARIANT::from(true))
        }
        .ok()?;
        let arr = unsafe { s.root.FindAll(TreeScope_Descendants, &pw) }.ok()?;
        let len = unsafe { arr.Length().unwrap_or(0) };
        for i in 0..len {
            if let Ok(el) = unsafe { arr.GetElement(i) } {
                let offscreen = unsafe { el.CurrentIsOffscreen() }
                    .map(|b| b.as_bool())
                    .unwrap_or(false);
                let ok_rect = unsafe { el.CurrentBoundingRectangle() }
                    .map(|r| r.right > r.left && r.bottom > r.top)
                    .unwrap_or(false);
                if !offscreen && ok_rect {
                    return Some(el);
                }
            }
        }
        None
    }

    /// Invoke the 登入 button on the SAME ROW as the PIN input, i.e. the
    /// `<div class="input-container">` holding `#pin` + `<button class="login-btn">登入</button>`.
    /// Chromium's UIA tree drops plain <div>s, so instead of walking parents we use
    /// geometry: among visible buttons named exactly 「登入」 pick the one vertically
    /// aligned with the PIN field and closest to its right edge. The tab buttons
    /// (自然人憑證登入 / 臨時憑證登入) have different names and are never candidates.
    fn invoke_login_near(
        s: &UiaSession,
        pin_el: &IUIAutomationElement,
        tx: &UnboundedSender<RunnerEvent>,
    ) -> bool {
        let Ok(pin_rc) = (unsafe { pin_el.CurrentBoundingRectangle() }) else {
            return false;
        };
        let pin_cy = (pin_rc.top + pin_rc.bottom) / 2;
        let pin_h = (pin_rc.bottom - pin_rc.top).max(1);
        let Ok(btn_cond) = (unsafe {
            s.uia.CreatePropertyCondition(
                UIA_ControlTypePropertyId,
                &VARIANT::from(UIA_ButtonControlTypeId.0),
            )
        }) else {
            return false;
        };
        let Ok(arr) = (unsafe { s.root.FindAll(TreeScope_Descendants, &btn_cond) }) else {
            return false;
        };
        let len = unsafe { arr.Length().unwrap_or(0) };

        let mut best: Option<(i64, IUIAutomationElement)> = None;
        let mut seen: Vec<String> = Vec::new();
        for i in 0..len {
            let Ok(btn) = (unsafe { arr.GetElement(i) }) else { continue };
            let name = unsafe { btn.CurrentName() }
                .map(|b| b.to_string())
                .unwrap_or_default();
            let Ok(rc) = (unsafe { btn.CurrentBoundingRectangle() }) else { continue };
            let visible = rc.right > rc.left && rc.bottom > rc.top;
            let offscreen = unsafe { btn.CurrentIsOffscreen() }
                .map(|b| b.as_bool())
                .unwrap_or(false);
            if visible && !offscreen {
                seen.push(format!(
                    "'{}'@({},{},{},{})",
                    name.trim(), rc.left, rc.top, rc.right, rc.bottom
                ));
            }
            if name.trim() != "登入" || !visible || offscreen {
                continue;
            }
            let cy = (rc.top + rc.bottom) / 2;
            let same_row = (cy - pin_cy).abs() <= pin_h;
            let right_of_pin = rc.left >= pin_rc.right - 5;
            if !(same_row && right_of_pin) {
                continue;
            }
            let dist = (rc.left - pin_rc.right).abs() as i64 + (cy - pin_cy).abs() as i64 * 4;
            if best.as_ref().map(|(d, _)| dist < *d).unwrap_or(true) {
                best = Some((dist, btn));
            }
        }
        emit_log(
            tx,
            "DEBUG",
            format!(
                "PIN 欄位@({},{},{},{})；畫面上可見按鈕：[{}]",
                pin_rc.left, pin_rc.top, pin_rc.right, pin_rc.bottom,
                seen.join(" | ")
            ),
        );
        if let Some((_, btn)) = best {
            if let Ok(inv) = unsafe {
                btn.GetCurrentPatternAs::<IUIAutomationInvokePattern>(UIA_InvokePatternId)
            } {
                if unsafe { inv.Invoke() }.is_ok() {
                    emit_log(tx, "INFO", "已 Invoke PIN 欄位同一列的「登入」按鈕 (makeSignature)");
                    return true;
                }
            }
            emit_log(tx, "WARN", "找到同列「登入」按鈕但 Invoke 失敗");
        } else {
            emit_log(tx, "WARN", "找不到與 PIN 欄位同一列、名稱為「登入」的按鈕");
        }
        false
    }

    /// Enter the PIN and submit. Strategy (never types unless Edge is foreground,
    /// so the PIN can't leak into another window such as this app):
    ///   1. UIA ValuePattern.SetValue on the password field (no focus needed)
    ///   2. else real keystrokes, only if Edge truly holds the foreground
    ///   3. submit by UIA Invoke on the 登入 button; else Enter if foreground
    fn type_pin_and_submit(
        s: &mut UiaSession,
        pin_el: &IUIAutomationElement,
        pin: &str,
        tx: &UnboundedSender<RunnerEvent>,
    ) -> Result<()> {
        let fg = s.focus_edge();
        emit_log(tx, "DEBUG", format!("Edge 取得前景焦點：{fg}"));

        let mut filled = false;
        if let Ok(vp) =
            unsafe { pin_el.GetCurrentPatternAs::<IUIAutomationValuePattern>(UIA_ValuePatternId) }
        {
            match unsafe { vp.SetValue(&BSTR::from(pin)) } {
                Ok(_) => {
                    filled = true;
                    emit_log(tx, "INFO", "已以 UIA ValuePattern 寫入 PIN 欄位");
                }
                Err(e) => emit_log(tx, "WARN", format!("ValuePattern.SetValue 失敗：{e}")),
            }
        } else {
            emit_log(tx, "WARN", "PIN 欄位不支援 ValuePattern，改用鍵盤輸入");
        }

        if !filled {
            if !s.focus_edge() {
                return Err(anyhow!(
                    "無法將 Edge 切到前景，為避免 PIN 被輸入到其他視窗，已中止輸入；請手動輸入 PIN"
                ));
            }
            let _ = unsafe { pin_el.SetFocus() };
            let _ = click_uia_element(s, pin_el);
            thread::sleep(Duration::from_millis(200));
            if !s.is_edge_foreground() {
                return Err(anyhow!("輸入前 Edge 失去前景焦點，已中止輸入 PIN"));
            }
            send_ctrl_key(0x41); // Ctrl+A
            thread::sleep(Duration::from_millis(50));
            send_vk_tap(VK_BACK);
            thread::sleep(Duration::from_millis(50));
            send_unicode_text(pin);
            thread::sleep(Duration::from_millis(250));
        }

        thread::sleep(Duration::from_millis(300));
        // 1) 優先 Invoke「PIN 欄位同一容器內」的登入按鈕（不依賴焦點，也不會碰到憑證分頁按鈕）
        // 2) 否則在 Edge 前景下以滑鼠點 PIN 欄位取得 DOM 焦點，再按 Enter（觸發 makeSignature()）
        let mut submitted = invoke_login_near(s, pin_el, tx);
        if !submitted && s.focus_edge() {
            let _ = click_uia_element(s, pin_el);
            thread::sleep(Duration::from_millis(250));
            if s.is_edge_foreground() {
                send_vk_tap(VK_RETURN);
                emit_log(tx, "INFO", "已點擊 PIN 欄位並按下 Enter 送出登入");
                submitted = true;
            }
        }
        if !submitted {
            return Err(anyhow!("無法送出登入（找不到欄位旁的登入按鈕，且 Edge 不在前景），請手動按登入"));
        }

        thread::sleep(Duration::from_millis(2500));
        if let Ok(fresh) = unsafe { s.uia.ElementFromHandle(s.edge_hwnd) } {
            s.root = fresh;
        }
        if find_by_id(s, "pin").is_ok() || find_password_edit(s).is_some() {
            emit_log(
                tx,
                "WARN",
                "送出後登入頁仍在（可能 PIN 驗證中、PIN 錯誤，或需於 Edge 視窗操作）",
            );
        }
        Ok(())
    }

    fn wait_enabled_id(s: &UiaSession, id: &str, timeout: Duration) -> Result<()> {
        let deadline = Instant::now() + timeout;
        while Instant::now() < deadline {
            if let Ok(el) = find_by_id(s, id) {
                if unsafe { el.CurrentIsEnabled() }.map(|b| b.as_bool()).unwrap_or(false) {
                    return Ok(());
                }
            }
            thread::sleep(Duration::from_millis(300));
        }
        Err(anyhow!("等待 UIA 控制項 #{id} 變為可用狀態超時"))
    }

    fn collect_table_rows(
        s: &UiaSession,
        grid: &IUIAutomationElement,
    ) -> Result<Vec<IUIAutomationElement>> {
        let true_cond = unsafe { s.uia.CreateTrueCondition() }
            .map_err(|e| anyhow!("CreateTrueCondition 失敗: {e}"))?;
        let arr = unsafe { grid.FindAll(TreeScope_Children, &true_cond) }
            .map_err(|e| anyhow!("讀取 #SampleGrid 子節點失敗: {e}"))?;
        let len = unsafe { arr.Length().unwrap_or(0) };

        let mut direct_rows = Vec::new();
        let mut group_containers = Vec::new();

        for i in 0..len {
            if let Ok(child) = unsafe { arr.GetElement(i) } {
                let ct = unsafe { child.CurrentControlType() }.unwrap_or(UIA_CustomControlTypeId);
                if ct == UIA_DataItemControlTypeId || ct == UIA_ListItemControlTypeId {
                    direct_rows.push(child);
                } else if ct == UIA_GroupControlTypeId || ct == UIA_CustomControlTypeId {
                    group_containers.push(child);
                }
            }
        }

        // If <table> wraps rows inside <thead>/<tbody> group nodes, expand one level
        if direct_rows.is_empty() && !group_containers.is_empty() {
            let mut expanded_rows = Vec::new();
            for grp in &group_containers {
                if let Ok(sub_arr) = unsafe { grp.FindAll(TreeScope_Children, &true_cond) } {
                    let sub_len = unsafe { sub_arr.Length().unwrap_or(0) };
                    for j in 0..sub_len {
                        if let Ok(row) = unsafe { sub_arr.GetElement(j) } {
                            if let Ok(rect) = unsafe { row.CurrentBoundingRectangle() } {
                                if rect.right > rect.left && rect.bottom > rect.top {
                                    expanded_rows.push(row);
                                }
                            }
                        }
                    }
                }
            }
            if !expanded_rows.is_empty() {
                return Ok(expanded_rows);
            }
            // Otherwise treat visible custom containers directly as rows
            return Ok(group_containers
                .into_iter()
                .filter(|r| {
                    unsafe { r.CurrentBoundingRectangle() }
                        .map(|rc| rc.right > rc.left && rc.bottom > rc.top)
                        .unwrap_or(false)
                })
                .collect());
        }

        Ok(direct_rows)
    }

    /// Select a table row with a REAL mouse click. UIA Select()/Invoke() often report
    /// success on a plain <tr> without firing the page's click handler, so they are
    /// not used here. Scrolls the row into view and requires Edge to be foreground,
    /// otherwise the click would land on another window.
    fn click_table_row(
        s: &UiaSession,
        row: &IUIAutomationElement,
        tx: &UnboundedSender<RunnerEvent>,
    ) -> Result<()> {
        if let Ok(sp) = unsafe {
            row.GetCurrentPatternAs::<IUIAutomationScrollItemPattern>(UIA_ScrollItemPatternId)
        } {
            let _ = unsafe { sp.ScrollIntoView() };
            thread::sleep(Duration::from_millis(200));
        }
        if !s.focus_edge() {
            return Err(anyhow!("無法將 Edge 切到前景，已中止點選資料列（避免點到其他視窗）"));
        }
        let rect = unsafe { row.CurrentBoundingRectangle() }
            .map_err(|e| anyhow!("取得資料列座標失敗: {e}"))?;
        if rect.right <= rect.left || rect.bottom <= rect.top {
            return Err(anyhow!("資料列沒有有效的螢幕座標（可能被捲出可視範圍）"));
        }
        let name = unsafe { row.CurrentName() }
            .map(|b| b.to_string())
            .unwrap_or_default();
        let cx = (rect.left + rect.right) / 2;
        let cy = (rect.top + rect.bottom) / 2;
        emit_log(
            tx,
            "DEBUG",
            format!(
                "點選資料列：名稱='{}'，範圍=({},{},{},{})，點擊座標=({},{})",
                name.trim(), rect.left, rect.top, rect.right, rect.bottom, cx, cy
            ),
        );
        send_mouse_click(cx, cy);
        thread::sleep(Duration::from_millis(250));
        if !s.is_edge_foreground() {
            emit_log(tx, "WARN", "點選資料列後 Edge 不在前景，點擊可能未生效");
        }
        Ok(())
    }

    fn wait_sample_grid_ready_uia(
        s: &UiaSession,
        timeout: Duration,
    ) -> Result<IUIAutomationElement> {
        let deadline = Instant::now() + timeout;
        while Instant::now() < deadline {
            if let Ok(grid) = find_by_id(s, "SampleGrid") {
                if let Ok(rows) = collect_table_rows(s, &grid) {
                    if rows.len() >= 2 {
                        return Ok(grid);
                    }
                }
            }
            thread::sleep(Duration::from_millis(350));
        }
        Err(anyhow!("等待 #SampleGrid 載入資料列超時（請確認已登入且 Edge 頁面已開啟）"))
    }

    fn set_id_value(s: &UiaSession, id: &str, value: &str) -> Result<()> {
        let el = find_by_id(s, id)?;
        set_element_value(s, &el, value)
    }

    fn set_element_value(
        s: &UiaSession,
        el: &IUIAutomationElement,
        value: &str,
    ) -> Result<()> {
        s.focus_edge();
        let _ = unsafe { el.SetFocus() };

        if let Ok(vp) = unsafe { el.GetCurrentPatternAs::<IUIAutomationValuePattern>(UIA_ValuePatternId) } {
            if unsafe { vp.SetValue(&BSTR::from(value)) }.is_ok() {
                if let Ok(curr) = unsafe { vp.CurrentValue() } {
                    if curr.to_string() == value {
                        return Ok(());
                    }
                }
            }
        }

        // Fallback: click inside control, select all, backspace, and type Unicode text via SendInput
        click_uia_element(s, el)?;
        thread::sleep(Duration::from_millis(80));
        send_ctrl_key(0x41); // Ctrl+A
        thread::sleep(Duration::from_millis(50));
        send_vk_tap(VK_BACK);
        thread::sleep(Duration::from_millis(50));
        if !value.is_empty() {
            send_unicode_text(value);
        }
        Ok(())
    }

    fn set_checkbox_id(s: &UiaSession, id: &str, target_checked: bool) -> Result<()> {
        let el = find_by_id(s, id)?;
        if let Ok(tp) =
            unsafe { el.GetCurrentPatternAs::<IUIAutomationTogglePattern>(UIA_TogglePatternId) }
        {
            let curr = unsafe { tp.CurrentToggleState() }?;
            let is_on = curr == ToggleState_On;
            if is_on != target_checked {
                if unsafe { tp.Toggle() }.is_err() {
                    click_uia_element(s, &el)?;
                }
            }
            return Ok(());
        }
        click_uia_element(s, &el)?;
        Ok(())
    }

    fn click_id(s: &UiaSession, id: &str) -> Result<()> {
        let el = find_by_id(s, id)?;
        let enabled = unsafe { el.CurrentIsEnabled() }
            .map(|b| b.as_bool())
            .unwrap_or(true);
        if !enabled {
            return Err(anyhow!("無法點擊控制項 #{id} (目前為 Disabled 狀態)"));
        }
        if let Ok(inv) =
            unsafe { el.GetCurrentPatternAs::<IUIAutomationInvokePattern>(UIA_InvokePatternId) }
        {
            if unsafe { inv.Invoke() }.is_ok() {
                return Ok(());
            }
        }
        click_uia_element(s, &el)
    }

    fn click_uia_element(s: &UiaSession, el: &IUIAutomationElement) -> Result<()> {
        s.focus_edge();
        if let Ok(sel) = unsafe {
            el.GetCurrentPatternAs::<IUIAutomationSelectionItemPattern>(UIA_SelectionItemPatternId)
        } {
            let _ = unsafe { sel.Select() };
        }
        if let Ok(inv) =
            unsafe { el.GetCurrentPatternAs::<IUIAutomationInvokePattern>(UIA_InvokePatternId) }
        {
            if unsafe { inv.Invoke() }.is_ok() {
                return Ok(());
            }
        }
        let rect = unsafe { el.CurrentBoundingRectangle() }
            .map_err(|e| anyhow!("取得元素螢幕座標失敗: {e}"))?;
        if rect.right <= rect.left || rect.bottom <= rect.top {
            return Err(anyhow!("目標 UI 元素沒有有效的螢幕矩形座標"));
        }
        let cx = (rect.left + rect.right) / 2;
        let cy = (rect.top + rect.bottom) / 2;
        send_mouse_click(cx, cy);
        Ok(())
    }

    fn get_element_text(s: &UiaSession, el: &IUIAutomationElement) -> String {
        if let Ok(vp) = unsafe { el.GetCurrentPatternAs::<IUIAutomationValuePattern>(UIA_ValuePatternId) } {
            if let Ok(v) = unsafe { vp.CurrentValue() } {
                let st = v.to_string();
                if !st.trim().is_empty() {
                    return st;
                }
            }
        }
        if let Ok(name) = unsafe { el.CurrentName() } {
            let st = name.to_string();
            if !st.trim().is_empty() {
                return st;
            }
        }
        if let Ok(text_cond) = unsafe {
            s.uia.CreatePropertyCondition(
                UIA_ControlTypePropertyId,
                &VARIANT::from(UIA_TextControlTypeId.0),
            )
        } {
            if let Ok(arr) = unsafe { el.FindAll(TreeScope_Descendants, &text_cond) } {
                let len = unsafe { arr.Length().unwrap_or(0) };
                let mut combined = String::new();
                for i in 0..len {
                    if let Ok(node) = unsafe { arr.GetElement(i) } {
                        if let Ok(n) = unsafe { node.CurrentName() } {
                            combined.push_str(&n.to_string());
                        }
                    }
                }
                if !combined.trim().is_empty() {
                    return combined;
                }
            }
        }
        String::new()
    }

    fn wait_op_msg_sn_uia(s: &UiaSession, timeout: Duration) -> Result<String> {
        let re = Regex::new(r"\d+")?;
        let deadline = Instant::now() + timeout;
        while Instant::now() < deadline {
            if let Ok(el) = find_by_id(s, "OpMsg") {
                let text = get_element_text(s, &el);
                if text.contains("新增成功") || text.contains("成功") {
                    if let Some(m) = re.find(&text) {
                        return Ok(m.as_str().to_string());
                    }
                }
            }
            thread::sleep(Duration::from_millis(500));
        }
        Err(anyhow!("等待 #OpMsg 取得工作序號超時"))
    }

    fn send_ctrl_key(vk_code: u16) {
        let inputs = [
            INPUT {
                r#type: INPUT_KEYBOARD,
                Anonymous: INPUT_0 {
                    ki: KEYBDINPUT {
                        wVk: VK_CONTROL,
                        wScan: 0,
                        dwFlags: KEYBD_EVENT_FLAGS(0),
                        time: 0,
                        dwExtraInfo: 0,
                    },
                },
            },
            INPUT {
                r#type: INPUT_KEYBOARD,
                Anonymous: INPUT_0 {
                    ki: KEYBDINPUT {
                        wVk: VIRTUAL_KEY(vk_code),
                        wScan: 0,
                        dwFlags: KEYBD_EVENT_FLAGS(0),
                        time: 0,
                        dwExtraInfo: 0,
                    },
                },
            },
            INPUT {
                r#type: INPUT_KEYBOARD,
                Anonymous: INPUT_0 {
                    ki: KEYBDINPUT {
                        wVk: VIRTUAL_KEY(vk_code),
                        wScan: 0,
                        dwFlags: KEYEVENTF_KEYUP,
                        time: 0,
                        dwExtraInfo: 0,
                    },
                },
            },
            INPUT {
                r#type: INPUT_KEYBOARD,
                Anonymous: INPUT_0 {
                    ki: KEYBDINPUT {
                        wVk: VK_CONTROL,
                        wScan: 0,
                        dwFlags: KEYEVENTF_KEYUP,
                        time: 0,
                        dwExtraInfo: 0,
                    },
                },
            },
        ];
        unsafe {
            let _ = SendInput(&inputs, mem::size_of::<INPUT>() as i32);
        }
    }

    fn send_vk_tap(vk: VIRTUAL_KEY) {
        let inputs = [
            INPUT {
                r#type: INPUT_KEYBOARD,
                Anonymous: INPUT_0 {
                    ki: KEYBDINPUT {
                        wVk: vk,
                        wScan: 0,
                        dwFlags: KEYBD_EVENT_FLAGS(0),
                        time: 0,
                        dwExtraInfo: 0,
                    },
                },
            },
            INPUT {
                r#type: INPUT_KEYBOARD,
                Anonymous: INPUT_0 {
                    ki: KEYBDINPUT {
                        wVk: vk,
                        wScan: 0,
                        dwFlags: KEYEVENTF_KEYUP,
                        time: 0,
                        dwExtraInfo: 0,
                    },
                },
            },
        ];
        unsafe {
            let _ = SendInput(&inputs, mem::size_of::<INPUT>() as i32);
        }
    }

    fn send_unicode_text(text: &str) {
        let mut inputs = Vec::with_capacity(text.len() * 2);
        for code_unit in text.encode_utf16() {
            inputs.push(INPUT {
                r#type: INPUT_KEYBOARD,
                Anonymous: INPUT_0 {
                    ki: KEYBDINPUT {
                        wVk: VIRTUAL_KEY(0),
                        wScan: code_unit,
                        dwFlags: KEYEVENTF_UNICODE,
                        time: 0,
                        dwExtraInfo: 0,
                    },
                },
            });
            inputs.push(INPUT {
                r#type: INPUT_KEYBOARD,
                Anonymous: INPUT_0 {
                    ki: KEYBDINPUT {
                        wVk: VIRTUAL_KEY(0),
                        wScan: code_unit,
                        dwFlags: KEYEVENTF_UNICODE | KEYEVENTF_KEYUP,
                        time: 0,
                        dwExtraInfo: 0,
                    },
                },
            });
        }
        if !inputs.is_empty() {
            unsafe {
                let _ = SendInput(&inputs, mem::size_of::<INPUT>() as i32);
            }
        }
    }

    /// Move the real cursor to (x, y) in physical screen pixels, then left-click.
    /// NOTE: MOUSEEVENTF_ABSOLUTE without MOUSEEVENTF_MOVE does NOT move the cursor,
    /// the click would land wherever the cursor already is, so SetCursorPos is used.
    fn send_mouse_click(x: i32, y: i32) {
        unsafe {
            let _ = SetCursorPos(x, y);
            thread::sleep(Duration::from_millis(80));

            let mk = |flags| INPUT {
                r#type: INPUT_MOUSE,
                Anonymous: INPUT_0 {
                    mi: MOUSEINPUT {
                        dx: 0,
                        dy: 0,
                        mouseData: 0,
                        dwFlags: flags,
                        time: 0,
                        dwExtraInfo: 0,
                    },
                },
            };
            let inputs = [mk(MOUSEEVENTF_LEFTDOWN), mk(MOUSEEVENTF_LEFTUP)];
            let _ = SendInput(&inputs, mem::size_of::<INPUT>() as i32);
        }
    }

    fn save_window_screenshot(
        hwnd: HWND,
        dir: &str,
        job_code: &str,
        roc_date_raw: &str,
        seq: usize,
    ) -> Result<String> {
        std::fs::create_dir_all(dir)?;
        let filename = format!("{}_{}_{:03}.png", job_code, roc_date_raw, seq);
        let path = Path::new(dir).join(filename);

        unsafe {
            let mut rect = RECT::default();
            GetWindowRect(hwnd, &mut rect)
                .map_err(|e| anyhow!("GetWindowRect 失敗: {e}"))?;
            let width = rect.right - rect.left;
            let height = rect.bottom - rect.top;
            if width <= 0 || height <= 0 {
                return Err(anyhow!("Edge 視窗大小無效 ({width}x{height})，無法截圖"));
            }

            let screen_dc = GetDC(None);
            let mem_dc = CreateCompatibleDC(Some(screen_dc));
            if mem_dc.is_invalid() {
                let _ = ReleaseDC(None, screen_dc);
                return Err(anyhow!("CreateCompatibleDC 失敗"));
            }

            let h_bitmap = CreateCompatibleBitmap(screen_dc, width, height);
            if h_bitmap.is_invalid() {
                let _ = DeleteDC(mem_dc);
                let _ = ReleaseDC(None, screen_dc);
                return Err(anyhow!("CreateCompatibleBitmap 失敗"));
            }

            let old_obj: HGDIOBJ = SelectObject(mem_dc, HGDIOBJ(h_bitmap.0));

            // PW_RENDERFULLCONTENT = 2 captures Edge DirectComposition layers on Windows 10
            let printed = PrintWindow(hwnd, mem_dc, PRINT_WINDOW_FLAGS(2)).as_bool();
            if !printed {
                let _ = BitBlt(
                    mem_dc,
                    0,
                    0,
                    width,
                    height,
                    Some(screen_dc),
                    rect.left,
                    rect.top,
                    SRCCOPY,
                );
            }

            let mut bi = BITMAPINFO {
                bmiHeader: BITMAPINFOHEADER {
                    biSize: mem::size_of::<BITMAPINFOHEADER>() as u32,
                    biWidth: width,
                    biHeight: -height,
                    biPlanes: 1,
                    biBitCount: 32,
                    biCompression: BI_RGB.0,
                    ..Default::default()
                },
                ..Default::default()
            };

            let row_stride = (width as usize) * 4;
            let mut pixels = vec![0u8; row_stride * (height as usize)];
            let lines = GetDIBits(
                mem_dc,
                h_bitmap,
                0,
                height as u32,
                Some(pixels.as_mut_ptr().cast()),
                &mut bi,
                DIB_RGB_COLORS,
            );

            SelectObject(mem_dc, old_obj);
            let _ = DeleteObject(HGDIOBJ(h_bitmap.0));
            let _ = DeleteDC(mem_dc);
            let _ = ReleaseDC(None, screen_dc);

            if lines == 0 {
                return Err(anyhow!("GetDIBits 讀取視窗像素失敗"));
            }

            for px in pixels.chunks_exact_mut(4) {
                px.swap(0, 2);
                px[3] = 255;
            }

            image::save_buffer(
                &path,
                &pixels,
                width as u32,
                height as u32,
                image::ExtendedColorType::Rgba8,
            )
            .map_err(|e| anyhow!("儲存 PNG 截圖失敗: {e}"))?;
        }

        Ok(path.to_string_lossy().to_string())
    }
}

#[cfg(windows)]
pub use win_uia::execute_uia;

#[cfg(not(windows))]
pub async fn execute_uia(
    _req: &crate::engine::browser_runner::ExecutionRequest,
    _tx: &tokio::sync::mpsc::UnboundedSender<crate::engine::browser_runner::RunnerEvent>,
    _total_ops: usize,
) -> anyhow::Result<()> {
    Err(anyhow::anyhow!(
        "Windows UI Automation (IUIAutomation) 僅支援 Windows 桌面環境；此功能無法在目前系統執行"
    ))
}
